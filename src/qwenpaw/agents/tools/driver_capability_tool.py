"""AgentScope tool adapter for Driver capabilities."""

from __future__ import annotations

import json
from collections.abc import Awaitable, Callable
from typing import Any

from qwenpaw.drivers.capabilities import (
    DriverCapability,
    DriverInvocation,
    DriverInvocationResult,
)

DriverInvoker = Callable[[DriverInvocation], Awaitable[DriverInvocationResult]]


def _text_block(text: str) -> Any:
    from agentscope.message import TextBlock

    return TextBlock(type="text", text=text)


def _stringify(value: Any) -> str:
    if value is None:
        return ""
    if isinstance(value, str):
        return value
    model_dump_json = getattr(value, "model_dump_json", None)
    if callable(model_dump_json):
        return model_dump_json(indent=2)
    try:
        return json.dumps(value, ensure_ascii=False, default=str, indent=2)
    except TypeError:
        return str(value)


def _blocks_from_mcp_content(content: Any) -> list[Any]:
    from agentscope.message import Base64Source, DataBlock

    blocks: list[Any] = []
    for item in content or []:
        text = getattr(item, "text", None)
        if text is not None:
            blocks.append(_text_block(str(text)))
            continue

        data = getattr(item, "data", None)
        mime_type = getattr(item, "mimeType", None)
        if data is not None and mime_type:
            blocks.append(
                DataBlock(
                    source=Base64Source(
                        type="base64",
                        media_type=str(mime_type),
                        data=str(data),
                    ),
                ),
            )
            continue

        resource = getattr(item, "resource", None)
        if resource is not None:
            resource_text = getattr(resource, "text", None)
            blocks.append(
                _text_block(
                    (
                        str(resource_text)
                        if resource_text is not None
                        else _stringify(resource)
                    ),
                ),
            )
            continue

        blocks.append(_text_block(_stringify(item)))
    return blocks


def _blocks_from_value(value: Any) -> list[Any]:
    content = getattr(value, "content", None)
    is_mcp_call_result = content is not None and hasattr(value, "isError")
    if is_mcp_call_result:
        blocks = _blocks_from_mcp_content(content)
        structured = getattr(value, "structuredContent", None)
        if structured is not None:
            blocks.append(_text_block(_stringify(structured)))
        return blocks or [_text_block("")]
    return [_text_block(_stringify(value))]


def _tool_chunk_from_driver_result(result: DriverInvocationResult) -> Any:
    from agentscope.message import ToolResultState
    from agentscope.tool import ToolChunk

    if result.ok:
        value = result.value
        state = (
            ToolResultState.ERROR
            if bool(getattr(value, "isError", False))
            else ToolResultState.SUCCESS
        )
        return ToolChunk(
            content=_blocks_from_value(value),
            state=state,
            is_last=True,
            metadata=dict(result.metadata or {}),
        )

    error_payload = {
        "ok": False,
        "type": result.error_type,
        "message": result.message,
        "metadata": result.metadata,
    }
    return ToolChunk(
        content=[_text_block(_stringify(error_payload))],
        state=ToolResultState.ERROR,
        is_last=True,
        metadata=dict(result.metadata or {}),
    )


class DriverCapabilityTool:
    """Expose one Driver capability as an AgentScope ToolBase instance."""

    def __new__(
        cls,
        capability: DriverCapability,
        invoker: DriverInvoker,
        request_context: dict[str, str] | None = None,
    ) -> Any:  # type: ignore[misc]
        from agentscope.tool import ToolBase

        class _DriverCapabilityTool(ToolBase):
            name = capability.exposure.tool_name or capability.name
            description = capability.description
            input_schema = dict(capability.input_schema or {})
            is_concurrency_safe = False
            is_read_only = False
            is_external_tool = False
            is_state_injected = False
            is_mcp = False
            mcp_name = None

            def __init__(self) -> None:
                self._capability = capability
                self._invoker = invoker
                self._request_context = dict(request_context or {})

            async def check_permissions(
                self,
                *_args: Any,
                **_kwargs: Any,
            ) -> Any:
                from agentscope.permission import (
                    PermissionBehavior,
                    PermissionDecision,
                )

                return PermissionDecision(
                    behavior=PermissionBehavior.ALLOW,
                    message="Driver capability policy is handled by Driver.",
                )

            async def __call__(self, **kwargs: Any) -> Any:
                result = await self._invoker(
                    DriverInvocation(
                        capability_id=self._capability.capability_id,
                        payload=dict(kwargs or {}),
                        request_context=self._request_context,
                    ),
                )
                return _tool_chunk_from_driver_result(result)

        return _DriverCapabilityTool()
