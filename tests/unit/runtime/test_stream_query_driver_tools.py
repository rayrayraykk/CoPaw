# -*- coding: utf-8 -*-
from __future__ import annotations

from typing import Any

import pytest

from qwenpaw.drivers.capabilities import (
    CapabilityExposure,
    DriverCapability,
    DriverInvocation,
    format_capability_id,
)
from qwenpaw.runtime.stream_query import Runner
from qwenpaw.schemas import AgentRequest, RunStatus


def _capability(
    name: str,
    *,
    as_tool: bool,
) -> DriverCapability:
    return DriverCapability(
        capability_id=format_capability_id(
            "mcp",
            "demo",
            "tool",
            "invoke",
            name,
        ),
        driver_name="demo",
        protocol="mcp",
        kind="tool",
        action="invoke",
        name=name,
        description=f"{name} tool",
        input_schema={"type": "object"},
        exposure=CapabilityExposure(as_tool=as_tool, tool_name=name),
    )


class FakeDriverManager:
    def __init__(self) -> None:
        self.list_calls: list[dict[str, Any]] = []

    async def list_capabilities(
        self,
        *,
        kind: str,
        request_context: dict[str, str],
    ) -> list[DriverCapability]:
        self.list_calls.append(
            {
                "kind": kind,
                "request_context": dict(request_context),
            },
        )
        return [
            _capability("echo", as_tool=True),
            _capability("hidden_resource", as_tool=False),
        ]

    async def invoke_capability(
        self,
        invocation: DriverInvocation,
    ) -> None:
        raise AssertionError(f"unexpected invocation: {invocation}")


class FakeAgent:
    async def reply_stream(self, *, inputs):
        del inputs
        for event in ():
            yield event


@pytest.mark.asyncio
async def test_stream_query_injects_driver_tools_and_policy_prompt(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    captured: dict[str, Any] = {}

    def fake_build_agent(
        session_id: str,
        **kwargs: Any,
    ) -> FakeAgent:
        captured["session_id"] = session_id
        captured.update(kwargs)
        return FakeAgent()

    monkeypatch.setattr(
        "qwenpaw.runtime.stream_query._request_input_to_msgs",
        lambda _raw: [],
    )
    monkeypatch.setattr(
        "qwenpaw.runtime.stream_query.build_agent",
        fake_build_agent,
    )

    manager = FakeDriverManager()
    runner = Runner()
    setattr(runner, "_driver_manager", manager)
    runner.agent_id = "assistant"
    request = AgentRequest(
        input=[],
        session_id="session-1",
        user_id="alice",
        channel="console",
        channel_meta={"user_name": "Alice"},
    )

    events = [event async for event in runner.stream_query(request)]

    assert events[-1].status == RunStatus.Completed
    assert manager.list_calls == [
        {
            "kind": "tool",
            "request_context": {
                "session_id": "session-1",
                "user_id": "alice",
                "channel": "console",
                "agent_id": "assistant",
                "root_session_id": "session-1",
                "root_agent_id": "assistant",
                "user_name": "Alice",
            },
        },
    ]
    assert captured["session_id"] == "session-1"
    assert (
        captured["request_context"] == manager.list_calls[0]["request_context"]
    )

    tools = captured["external_tools"]
    assert [tool.name for tool in tools] == ["echo"]
    assert (
        getattr(tools[0], "_request_context")
        == manager.list_calls[0]["request_context"]
    )
    assert len(captured["extra_prompts"]) == 1
    assert "Driver and MCP permission" in captured["extra_prompts"][0]


@pytest.mark.asyncio
async def test_stream_query_omits_driver_prompt_when_no_exposed_tools(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    captured: dict[str, Any] = {}

    class Manager:
        async def list_capabilities(
            self,
            *,
            kind: str,
            request_context: dict[str, str],
        ) -> list[DriverCapability]:
            del kind
            del request_context
            return [_capability("internal", as_tool=False)]

        async def invoke_capability(
            self,
            invocation: DriverInvocation,
        ) -> None:
            raise AssertionError(f"unexpected invocation: {invocation}")

    def fake_build_agent(
        _session_id: str,
        **kwargs: Any,
    ) -> FakeAgent:
        captured.update(kwargs)
        return FakeAgent()

    monkeypatch.setattr(
        "qwenpaw.runtime.stream_query._request_input_to_msgs",
        lambda _raw: [],
    )
    monkeypatch.setattr(
        "qwenpaw.runtime.stream_query.build_agent",
        fake_build_agent,
    )

    runner = Runner()
    setattr(runner, "_driver_manager", Manager())

    events = [
        event
        async for event in runner.stream_query(
            AgentRequest(input=[], session_id="session-2"),
        )
    ]

    assert events[-1].status == RunStatus.Completed
    assert captured["external_tools"] is None
    assert captured["extra_prompts"] is None
