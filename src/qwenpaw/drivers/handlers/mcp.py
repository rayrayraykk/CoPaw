# -*- coding: utf-8 -*-
"""MCP Driver handler."""

from __future__ import annotations

import asyncio
import base64
import logging
import re
from typing import Any

from qwenpaw.drivers.capabilities import (
    CapabilityExposure,
    DriverCapability,
    DriverInvocation,
    DriverInvocationResult,
    format_capability_id,
    parse_capability_id,
)
from qwenpaw.drivers.contracts import PolicyTarget
from qwenpaw.drivers.handlers.mcp_stateful_client import (
    HttpStatefulClient,
    StdIOStatefulClient,
)
from qwenpaw.drivers.credentials.types import ResolvedCredential
from qwenpaw.drivers.errors import (
    ApprovalRequiredError,
    DriverPermissionDeniedError,
)
from qwenpaw.drivers.handler import DriverHandler
from qwenpaw.drivers.policy import PolicyContext

logger = logging.getLogger(__name__)


class MCPDriverHandler(DriverHandler):
    def __init__(self, *args, **kwargs) -> None:
        super().__init__(*args, **kwargs)
        self._client: Any | None = None

    async def _setup(self) -> None:
        """Create and connect StdIOStatefulClient or HttpStatefulClient."""
        endpoint = self._card.endpoint
        transport = str(endpoint.get("transport") or "stdio")
        credentials = await self._resolve_credentials()

        if transport == "stdio":
            self._client = StdIOStatefulClient(
                name=self._card.name,
                command=str(endpoint.get("command") or ""),
                args=list(endpoint.get("args") or []),
                env=self._resolve_binding(
                    endpoint.get("env") or {},
                    credentials,
                ),
                cwd=endpoint.get("cwd") or None,
            )
        else:
            headers = self._resolve_binding(
                endpoint.get("headers") or {},
                credentials,
            )
            headers.update(self._implicit_auth_headers(credentials, headers))
            self._client = HttpStatefulClient(
                name=self._card.name,
                transport=transport,
                url=str(endpoint.get("url") or ""),
                headers=headers or None,
            )

        try:
            await self._client.connect()
        except asyncio.CancelledError:
            await self._client.close(ignore_errors=True)
            self._client = None
            raise
        except Exception:
            await self._client.close(ignore_errors=True)
            self._client = None
            raise

    async def _teardown(self) -> None:
        """Close connected MCP client if present."""
        if self._client is not None:
            await self._client.close()
            self._client = None

    async def _execute(
        self,
        credential: ResolvedCredential,
        context: PolicyContext,
        **kwargs: Any,
    ) -> Any:
        """Call MCP tool on underlying client."""
        del credential
        del context
        if self._client is None:
            raise RuntimeError(f"MCP driver '{self.name}' is not connected")
        return await self._client.call_tool(
            str(kwargs["tool_name"]),
            dict(kwargs.get("arguments") or {}),
        )

    async def list_tools(self) -> Any:
        """Delegate to underlying MCP client list_tools."""
        if self._client is None:
            raise RuntimeError(f"MCP driver '{self.name}' is not connected")
        return await self._client.list_tools()

    async def list_capabilities(
        self,
        request_context: dict[str, str] | None = None,
    ) -> list[DriverCapability]:
        """Expose MCP tools as protocol-neutral Driver capabilities."""
        del request_context
        tools = await self.list_tools()
        return [
            _mcp_tool_to_capability(
                self.name,
                tool,
                display_name=str(self._card.config.get("display_name") or ""),
            )
            for tool in tools
        ]

    async def invoke_capability(
        self,
        invocation: DriverInvocation,
    ) -> DriverInvocationResult:
        """Invoke one MCP tool capability through Driver policy."""
        try:
            (
                protocol,
                driver_name,
                kind,
                action,
                tool_name,
            ) = parse_capability_id(
                invocation.capability_id,
            )
        except ValueError as exc:
            return DriverInvocationResult(
                ok=False,
                error_type="invalid_capability_id",
                message=str(exc),
            )
        if (
            protocol != "mcp"
            or driver_name != self.name
            or kind != "tool"
            or action != "invoke"
        ):
            return DriverInvocationResult(
                ok=False,
                error_type="unsupported_capability",
                message=(
                    f"Unsupported MCP capability: {invocation.capability_id}"
                ),
            )
        subjects = _subjects_from_context(invocation.request_context)
        subject = subjects[0]
        try:
            value = await self._guarded_execute(
                subject,
                operation="invoke",
                target=PolicyTarget(kind="tool", name=tool_name),
                request_context=invocation.request_context,
                subjects=subjects,
                tool_name=tool_name,
                arguments=dict(invocation.payload or {}),
            )
        except DriverPermissionDeniedError as exc:
            return DriverInvocationResult(
                ok=False,
                error_type="driver_policy_denied",
                message=exc.to_user_message(),
                metadata=exc.to_result(),
            )
        except ApprovalRequiredError as exc:
            return DriverInvocationResult(
                ok=False,
                error_type="driver_policy_approval_required",
                message=str(exc),
            )
        except Exception as exc:
            logger.warning(
                "MCP capability invocation failed for Driver '%s' tool '%s': %s",
                self.name,
                tool_name,
                exc,
                exc_info=True,
            )
            return DriverInvocationResult(
                ok=False,
                error_type="execution_error",
                message=str(exc),
                metadata={"driver_name": self.name, "tool_name": tool_name},
            )
        return DriverInvocationResult(ok=True, value=value)

    @staticmethod
    def _resolve_binding(
        binding: dict[str, Any],
        credentials: dict[str, ResolvedCredential],
    ) -> dict[str, str]:
        # A binding maps runtime names, such as env vars or HTTP headers, to
        # public literals or keys inside resolved credential secrets.
        if not isinstance(binding, dict):
            return {}
        if "public" not in binding and "secret_refs" not in binding:
            result: dict[str, str] = {}
            for output_name, spec in binding.items():
                value = _resolve_value_source(spec, credentials)
                if value is not None:
                    result[str(output_name)] = value
            return result

        result = {
            str(key): str(value)
            for key, value in dict(binding.get("public") or {}).items()
        }
        for output_name, secret_key in dict(
            binding.get("secret_refs") or {},
        ).items():
            value = _lookup_credential_value(credentials, str(secret_key))
            if value is not None:
                result[str(output_name)] = str(value)
        return result

    @staticmethod
    def _implicit_auth_headers(
        credentials: dict[str, ResolvedCredential],
        existing_headers: dict[str, str],
    ) -> dict[str, str]:
        if any(key.lower() == "authorization" for key in existing_headers):
            return {}

        credential = credentials.get("oauth")
        if credential is None:
            credential = credentials.get("default") or next(
                iter(credentials.values()),
                ResolvedCredential.EMPTY,
            )
        values = credential.values
        if not values:
            return {}

        headers = values.get("headers")
        if isinstance(headers, dict):
            return {str(key): str(value) for key, value in headers.items()}

        access_token = values.get("access_token") or values.get("token")
        if access_token:
            return {"Authorization": f"Bearer {access_token}"}

        username = values.get("username")
        password = values.get("password")
        if username is not None and password is not None:
            raw = f"{username}:{password}".encode("utf-8")
            encoded = base64.b64encode(raw).decode("ascii")
            return {"Authorization": f"Basic {encoded}"}

        return {}

    async def _resolve_credentials(self) -> dict[str, ResolvedCredential]:
        credentials: dict[str, ResolvedCredential] = {}
        for alias, provider in self._credential_providers.items():
            credentials[alias] = await provider.resolve()
        if "default" not in credentials and len(credentials) == 1:
            credentials["default"] = next(iter(credentials.values()))
        return credentials


def _resolve_value_source(
    spec: Any,
    credentials: dict[str, ResolvedCredential],
) -> str | None:
    if not isinstance(spec, dict) or "source" not in spec:
        return str(spec)

    source = str(spec.get("source") or "")
    if source == "literal":
        return str(spec.get("value") or "")
    if source != "credential":
        return None

    alias = str(spec.get("credential") or "default")
    field = str(spec.get("field") or "")
    value = (
        _lookup_credential_value(credentials, f"{alias}.{field}")
        if field
        else None
    )
    if value is None:
        return None

    text = str(value)
    fmt = spec.get("format")
    if isinstance(fmt, str) and fmt:
        return fmt.replace("{value}", text)
    return text


def _lookup_credential_value(
    credentials: dict[str, ResolvedCredential],
    reference: str,
) -> Any:
    alias = ""
    field = reference
    if "." in reference:
        alias, field = reference.split(".", 1)

    candidates: list[ResolvedCredential] = []
    if alias:
        credential = credentials.get(alias)
        if credential is not None:
            candidates.append(credential)
    else:
        for preferred in ("static", "default"):
            credential = credentials.get(preferred)
            if credential is not None:
                candidates.append(credential)
        candidates.extend(
            credential
            for key, credential in credentials.items()
            if key not in {"static", "default"}
        )

    for credential in candidates:
        if field in credential.secrets:
            return credential.secrets[field]
        if field in credential.values:
            return credential.values[field]
    return None


def _subject_from_context(request_context: dict[str, str]) -> str:
    return _subjects_from_context(request_context)[0]


def _subjects_from_context(request_context: dict[str, str]) -> tuple[str, ...]:
    subjects: list[str] = []

    def add(subject: str) -> None:
        if subject and subject not in subjects:
            subjects.append(subject)

    explicit = str(request_context.get("subject") or "").strip()
    if explicit:
        add(explicit)

    user_id = str(request_context.get("user_id") or "").strip()
    if user_id:
        add(_typed_subject("user", user_id))

    session_id = str(request_context.get("session_id") or "").strip()
    if session_id:
        add(_typed_subject("session", session_id))

    for key in ("app_id", "domain_app_id", "agent_id", "root_agent_id"):
        value = str(request_context.get(key) or "").strip()
        if value:
            add(_typed_subject("app", value))

    channel = str(request_context.get("channel") or "").strip()
    if channel:
        add(_typed_subject("channel", channel))

    return tuple(subjects or ("user:unknown",))


def _typed_subject(kind: str, value: str) -> str:
    if value.startswith(f"{kind}:"):
        return value
    return f"{kind}:{value}"


def _mcp_tool_to_capability(
    driver_name: str,
    tool: Any,
    *,
    display_name: str = "",
) -> DriverCapability:
    raw_tool = getattr(tool, "_tool", tool)
    name = str(getattr(raw_tool, "name", getattr(tool, "name", tool)))
    if name.startswith(f"mcp__{driver_name}__"):
        name = name[len(f"mcp__{driver_name}__") :]
    display_namespace = _tool_namespace_from_display_name(
        display_name,
        fallback=driver_name,
    )
    description = str(
        getattr(raw_tool, "description", getattr(tool, "description", ""))
        or "",
    )
    if display_namespace != driver_name:
        description = (
            f"{description}\n\n"
            f"MCP server display name: {display_name}. "
            f"Stable MCP client key: {driver_name}."
        ).strip()
    input_schema = (
        getattr(raw_tool, "inputSchema", None)
        or getattr(raw_tool, "input_schema", None)
        or getattr(tool, "input_schema", None)
        or {}
    )
    if not isinstance(input_schema, dict):
        input_schema = {}
    input_schema = dict(input_schema)
    input_schema.setdefault("type", "object")
    input_schema.setdefault("properties", {})
    input_schema.setdefault("required", [])
    return DriverCapability(
        capability_id=format_capability_id(
            "mcp",
            driver_name,
            "tool",
            "invoke",
            name,
        ),
        driver_name=driver_name,
        protocol="mcp",
        kind="tool",
        action="invoke",
        name=name,
        description=description,
        input_schema=input_schema,
        exposure=CapabilityExposure(
            as_tool=True,
            namespace=display_namespace,
            tool_name=f"{display_namespace}__{name}",
        ),
        metadata={
            "driver_key": driver_name,
            "display_name": display_name or driver_name,
        },
    )


_TOOL_NAME_SAFE_CHARS = re.compile(r"[^A-Za-z0-9_-]+")


def _tool_namespace_from_display_name(
    display_name: str,
    *,
    fallback: str,
) -> str:
    namespace = _TOOL_NAME_SAFE_CHARS.sub("_", display_name.strip()).strip("_")
    return namespace or fallback
