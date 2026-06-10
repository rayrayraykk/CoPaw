# -*- coding: utf-8 -*-
from __future__ import annotations

import pytest
from agentscope.message import ToolResultState
from agentscope.tool import ToolChunk
from mcp.types import CallToolResult, TextContent

from qwenpaw.drivers.adapters.agentscope_tool import DriverCapabilityTool
from qwenpaw.drivers.capabilities import (
    CapabilityExposure,
    DriverCapability,
    DriverInvocation,
    DriverInvocationResult,
    format_capability_id,
)


def _capability() -> DriverCapability:
    return DriverCapability(
        capability_id=format_capability_id(
            "mcp",
            "local_stdio_echo",
            "tool",
            "invoke",
            "echo",
        ),
        driver_name="local_stdio_echo",
        protocol="mcp",
        kind="tool",
        action="invoke",
        name="echo",
        description="Echo text.",
        input_schema={"type": "object"},
        exposure=CapabilityExposure(as_tool=True, tool_name="echo"),
    )


@pytest.mark.asyncio
async def test_driver_tool_wraps_mcp_result_as_tool_chunk() -> None:
    seen: list[DriverInvocation] = []

    async def invoker(invocation: DriverInvocation) -> DriverInvocationResult:
        seen.append(invocation)
        return DriverInvocationResult(
            ok=True,
            value=CallToolResult(
                content=[TextContent(type="text", text="hello-debug")],
                isError=False,
            ),
        )

    tool = DriverCapabilityTool(
        _capability(),
        invoker,
        request_context={"session_id": "session-1"},
    )

    assert isinstance(tool, DriverCapabilityTool)
    chunk = await getattr(tool, "__call__")(text="hello-debug")

    assert isinstance(chunk, ToolChunk)
    assert chunk.state == ToolResultState.SUCCESS
    assert chunk.content[0].text == "hello-debug"
    assert seen[0].payload == {"text": "hello-debug"}
    assert seen[0].request_context == {"session_id": "session-1"}


@pytest.mark.asyncio
async def test_driver_tool_preserves_mcp_error_state() -> None:
    async def invoker(_invocation: DriverInvocation) -> DriverInvocationResult:
        return DriverInvocationResult(
            ok=True,
            value=CallToolResult(
                content=[TextContent(type="text", text="tool failed")],
                isError=True,
            ),
        )

    tool = DriverCapabilityTool(_capability(), invoker)
    chunk = await getattr(tool, "__call__")()

    assert isinstance(chunk, ToolChunk)
    assert chunk.state == ToolResultState.ERROR
    assert chunk.content[0].text == "tool failed"


@pytest.mark.asyncio
async def test_driver_tool_wraps_driver_error_as_error_chunk() -> None:
    async def invoker(_invocation: DriverInvocation) -> DriverInvocationResult:
        return DriverInvocationResult(
            ok=False,
            error_type="driver_policy_denied",
            message="User denied.",
            metadata={"driver": "local_stdio_echo"},
        )

    tool = DriverCapabilityTool(_capability(), invoker)
    chunk = await getattr(tool, "__call__")()

    assert isinstance(chunk, ToolChunk)
    assert chunk.state == ToolResultState.ERROR
    text = chunk.content[0].text
    assert '"type": "driver_policy_denied"' in text
    assert '"message": "User denied."' in text
    assert chunk.metadata == {"driver": "local_stdio_echo"}
