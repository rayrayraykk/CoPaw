# -*- coding: utf-8 -*-
import asyncio
from typing import Any

import pytest

from qwenpaw.drivers.credentials.types import ResolvedCredential
from qwenpaw.drivers.capabilities import DriverInvocation, parse_capability_id
from qwenpaw.drivers.handlers.mcp import (
    MCPDriverHandler,
    _subjects_from_context,
    validate_mcp_endpoint,
)
from qwenpaw.drivers.contracts import CredentialRef, DriverCard, PolicyRule
from qwenpaw.drivers.errors import DriverCardError


class StaticProvider:
    def __init__(self, values: dict[str, Any] | None = None) -> None:
        self.values = values or {}
        self.closed = False
        self.resolve_calls = 0

    async def resolve(self) -> ResolvedCredential:
        self.resolve_calls += 1
        return ResolvedCredential(kind="static", secrets=dict(self.values))

    async def close(self) -> None:
        self.closed = True


class FakeStdIOClient:
    instances: list["FakeStdIOClient"] = []
    connect_error: BaseException | None = None
    call_error: BaseException | None = None

    def __init__(self, **kwargs: Any) -> None:
        self.kwargs = kwargs
        self.connected = False
        self.closed = False
        self.calls: list[tuple[str, dict[str, Any]]] = []
        FakeStdIOClient.instances.append(self)

    async def connect(self) -> None:
        if self.connect_error is not None:
            raise self.connect_error
        self.connected = True

    async def close(self, ignore_errors: bool = True) -> None:
        del ignore_errors
        self.closed = True

    async def call_tool(self, name: str, arguments: dict[str, Any]) -> str:
        if self.call_error is not None:
            raise self.call_error
        self.calls.append((name, arguments))
        return "called"

    async def list_tools(self) -> list[str]:
        return ["tool"]


class FakeHttpClient(FakeStdIOClient):
    instances: list["FakeHttpClient"] = []

    def __init__(self, **kwargs: Any) -> None:
        self.kwargs = kwargs
        self.connected = False
        self.closed = False
        self.calls: list[tuple[str, dict[str, Any]]] = []
        FakeHttpClient.instances.append(self)


@pytest.fixture(autouse=True)
def fake_clients(monkeypatch: pytest.MonkeyPatch) -> None:
    FakeStdIOClient.instances.clear()
    FakeStdIOClient.connect_error = None
    FakeStdIOClient.call_error = None
    FakeHttpClient.instances.clear()
    monkeypatch.setattr(
        "qwenpaw.drivers.handlers.mcp.StdIOStatefulClient",
        FakeStdIOClient,
    )
    monkeypatch.setattr(
        "qwenpaw.drivers.handlers.mcp.HttpStatefulClient",
        FakeHttpClient,
    )


def _card(
    endpoint: dict[str, Any],
    credential_kind: str = "none",
) -> DriverCard:
    return DriverCard(
        name="demo",
        protocol="mcp",
        endpoint=endpoint,
        credential=CredentialRef(kind=credential_kind, ref="demo"),
        policy=[PolicyRule(subject="*", effect="allow")],
    )


def test_subjects_from_context_includes_user_app_channel_and_session() -> None:
    assert _subjects_from_context(
        {
            "user_id": "alice",
            "agent_id": "finance",
            "channel": "console",
            "session_id": "s1",
        },
    ) == (
        "user:alice",
        "session:s1",
        "app:finance",
        "channel:console",
    )


def test_validate_mcp_endpoint_rejects_missing_stdio_command() -> None:
    with pytest.raises(DriverCardError, match="endpoint.command"):
        validate_mcp_endpoint(_card({"transport": "stdio"}))


def test_validate_mcp_endpoint_rejects_missing_http_url() -> None:
    with pytest.raises(DriverCardError, match="endpoint.url"):
        validate_mcp_endpoint(_card({"transport": "streamable_http"}))


def test_validate_mcp_endpoint_rejects_unknown_transport() -> None:
    with pytest.raises(DriverCardError, match="unsupported MCP transport"):
        validate_mcp_endpoint(
            _card({"transport": "websocket", "url": "wss://example.test"}),
        )


@pytest.mark.asyncio
async def test_stdio_endpoint_builds_client() -> None:
    handler = MCPDriverHandler(
        _card(
            {
                "transport": "stdio",
                "command": "python",
                "args": ["server.py"],
                "env": {
                    "public": {"MODE": "test"},
                    "secret_refs": {"TOKEN": "token"},
                },
                "cwd": "/tmp",
            },
        ),
        StaticProvider({"token": "secret"}),
    )

    await handler.init()

    client = FakeStdIOClient.instances[0]
    assert client.kwargs == {
        "name": "demo",
        "command": "python",
        "args": ["server.py"],
        "env": {"MODE": "test", "TOKEN": "secret"},
        "cwd": "/tmp",
    }
    assert client.connected


@pytest.mark.asyncio
async def test_stdio_connect_cancellation_closes_partial_client() -> None:
    FakeStdIOClient.connect_error = asyncio.CancelledError()
    handler = MCPDriverHandler(
        _card({"transport": "stdio", "command": "server"}),
        StaticProvider(),
    )

    with pytest.raises(asyncio.CancelledError):
        await handler.init()

    client = FakeStdIOClient.instances[0]
    assert client.closed
    assert getattr(handler, "_client") is None


@pytest.mark.asyncio
async def test_http_endpoint_injects_token_from_static_credential() -> None:
    handler = MCPDriverHandler(
        _card(
            {
                "transport": "streamable_http",
                "url": "https://mcp.example.test",
                "headers": {
                    "public": {"X-Trace": "1"},
                    "secret_refs": {"Authorization": "authorization"},
                },
            },
            credential_kind="static",
        ),
        StaticProvider({"authorization": "Bearer abc"}),
    )

    await handler.init()

    assert FakeHttpClient.instances[0].kwargs["headers"] == {
        "X-Trace": "1",
        "Authorization": "Bearer abc",
    }


@pytest.mark.asyncio
async def test_http_endpoint_injects_oauth_access_token() -> None:
    handler = MCPDriverHandler(
        _card(
            {
                "transport": "sse",
                "url": "https://mcp.example.test/sse",
                "headers": {},
            },
            credential_kind="oauth2_auth_code",
        ),
        StaticProvider({"access_token": "oauth"}),
    )

    await handler.init()

    assert FakeHttpClient.instances[0].kwargs["headers"] == {
        "Authorization": "Bearer oauth",
    }


@pytest.mark.asyncio
async def test_invoke_capability_calls_underlying_tool() -> None:
    handler = MCPDriverHandler(
        _card({"transport": "stdio", "command": "server"}),
        StaticProvider(),
    )
    await handler.init()

    capability = (await handler.list_capabilities())[0]
    result = await handler.invoke_capability(
        DriverInvocation(
            capability_id=capability.capability_id,
            payload={"q": "driver"},
            request_context={"user_id": "alice"},
        ),
    )

    assert result.ok is True
    assert result.value == "called"
    assert FakeStdIOClient.instances[0].calls == [
        ("tool", {"q": "driver"}),
    ]


@pytest.mark.asyncio
async def test_display_name_is_used_as_tool_namespace() -> None:
    card = _card({"transport": "stdio", "command": "server"})
    card.config["display_name"] = "aone-code-platform"
    handler = MCPDriverHandler(card, StaticProvider())
    await handler.init()

    capability = (await handler.list_capabilities())[0]

    assert capability.driver_name == "demo"
    assert capability.exposure.namespace == "aone-code-platform"
    assert capability.exposure.tool_name == "aone-code-platform__tool"
    assert capability.metadata == {
        "driver_key": "demo",
        "display_name": "aone-code-platform",
    }
    assert parse_capability_id(capability.capability_id) == (
        "mcp",
        "demo",
        "tool",
        "invoke",
        "tool",
    )
    assert "MCP server display name: aone-code-platform" in (
        capability.description
    )


@pytest.mark.asyncio
async def test_list_tools_and_shutdown_delegate_to_client() -> None:
    provider = StaticProvider()
    handler = MCPDriverHandler(
        _card({"transport": "stdio", "command": "server"}),
        provider,
    )
    await handler.init()

    assert await handler.list_tools() == ["tool"]
    capabilities = await handler.list_capabilities()
    assert capabilities[0].name == "tool"
    assert capabilities[0].kind == "tool"
    assert capabilities[0].exposure.as_tool is True

    await handler.shutdown()

    assert FakeStdIOClient.instances[0].closed
    assert provider.closed


@pytest.mark.asyncio
async def test_capability_invoke_returns_policy_denial() -> None:
    card = _card({"transport": "stdio", "command": "server"})
    card.policy = [PolicyRule(subject="*", effect="deny")]
    handler = MCPDriverHandler(card, StaticProvider())
    await handler.init()

    capability = (await handler.list_capabilities())[0]
    result = await handler.invoke_capability(
        DriverInvocation(
            capability_id=capability.capability_id,
            payload={"q": "driver"},
            request_context={"session_id": "session-1"},
        ),
    )

    assert result.ok is False
    assert result.error_type == "driver_policy_denied"


@pytest.mark.asyncio
async def test_capability_invoke_returns_approval_required() -> None:
    card = _card({"transport": "stdio", "command": "server"})
    card.policy = [PolicyRule(subject="*", effect="ask")]
    handler = MCPDriverHandler(card, StaticProvider())
    await handler.init()

    capability = (await handler.list_capabilities())[0]
    result = await handler.invoke_capability(
        DriverInvocation(
            capability_id=capability.capability_id,
            payload={"q": "driver"},
            request_context={},
        ),
    )

    assert result.ok is False
    assert result.error_type == "driver_policy_approval_required"
    assert FakeStdIOClient.instances[0].calls == []


@pytest.mark.asyncio
async def test_capability_invoke_returns_execution_error() -> None:
    FakeStdIOClient.call_error = RuntimeError("transport failed")
    handler = MCPDriverHandler(
        _card({"transport": "stdio", "command": "server"}),
        StaticProvider(),
    )
    await handler.init()

    capability = (await handler.list_capabilities())[0]
    result = await handler.invoke_capability(
        DriverInvocation(
            capability_id=capability.capability_id,
            payload={"q": "driver"},
            request_context={},
        ),
    )

    assert result.ok is False
    assert result.error_type == "execution_error"
    assert result.metadata == {
        "driver_name": "demo",
        "tool_name": "tool",
    }
