# -*- coding: utf-8 -*-
import asyncio
from types import SimpleNamespace
from typing import Any

import pytest

# This module verifies the stateful client's internal lifecycle signals.
# pylint: disable=protected-access

from qwenpaw.drivers.handlers.mcp_stateful_client import (
    HttpStatefulClient,
    StdIOStatefulClient,
    _is_401_error,
    _is_transport_error,
)


class FakeTool:
    def __init__(self, name: str) -> None:
        self.name = name
        self.description = f"{name} description"
        self.inputSchema = {"type": "object", "properties": {}}
        self.annotations = None


class FakeSession:
    def __init__(
        self,
        tools: list[FakeTool] | None = None,
        result: Any | None = None,
        error: BaseException | None = None,
    ) -> None:
        self.tools = tools or [FakeTool("echo")]
        self.result = result or {"ok": True}
        self.error = error
        self.calls: list[tuple[str, dict[str, Any]]] = []

    async def list_tools(self) -> SimpleNamespace:
        if self.error:
            raise self.error
        return SimpleNamespace(tools=self.tools)

    async def call_tool(self, name: str, arguments: dict[str, Any]) -> Any:
        self.calls.append((name, arguments))
        if self.error:
            raise self.error
        return self.result


class FastLifecycleClient(StdIOStatefulClient):
    def __init__(self) -> None:
        super().__init__(name="fast", command="server")
        self.connection_count = 0

    async def _run_lifecycle(self) -> None:
        while not self._stop_event.is_set():
            self.connection_count += 1
            self.session = FakeSession()
            self.is_connected = True
            self._ready_event.set()

            while (
                not self._reload_event.is_set()
                and not self._stop_event.is_set()
            ):
                await asyncio.sleep(0.001)

            self.session = None
            self.is_connected = False

            if self._reload_event.is_set():
                self._reload_event.clear()
                self._ready_event.clear()
            else:
                self._cached_tools = None


class TimeoutLifecycleClient(StdIOStatefulClient):
    async def _run_lifecycle(self) -> None:
        while not self._stop_event.is_set():
            await asyncio.sleep(0.001)


class OAuthLifecycleClient(StdIOStatefulClient):
    async def _run_lifecycle(self) -> None:
        self._oauth_required = True
        self._stop_event.set()
        self._ready_event.set()


def test_stdio_constructor_validates_inputs() -> None:
    with pytest.raises(TypeError, match="name must be str"):
        StdIOStatefulClient(name=123, command="server")

    with pytest.raises(TypeError, match="command must be str"):
        StdIOStatefulClient(name="demo", command=object())


def test_http_constructor_validates_inputs() -> None:
    with pytest.raises(TypeError, match="name must be str"):
        HttpStatefulClient(name=123, transport="streamable_http", url="url")

    with pytest.raises(TypeError, match="transport must be str"):
        HttpStatefulClient(name="demo", transport=123, url="url")

    with pytest.raises(ValueError, match="streamable_http"):
        HttpStatefulClient(name="demo", transport="websocket", url="url")

    with pytest.raises(TypeError, match="url must be str"):
        HttpStatefulClient(
            name="demo",
            transport="streamable_http",
            url=object(),
        )


@pytest.mark.asyncio
async def test_connect_reload_and_close_use_single_lifecycle_task() -> None:
    client = FastLifecycleClient()

    await client.connect(timeout=1)
    assert client.is_connected is True
    assert client.connection_count == 1

    await client.reload(timeout=1)
    assert client.is_connected is True
    assert client.connection_count == 2

    await client.close()
    assert client.is_connected is False
    assert client.session is None
    assert client._lifecycle_task is None


@pytest.mark.asyncio
async def test_connect_rejects_duplicate_connection() -> None:
    client = FastLifecycleClient()
    await client.connect(timeout=1)

    with pytest.raises(RuntimeError, match="already connected"):
        await client.connect(timeout=1)

    await client.close()


@pytest.mark.asyncio
async def test_connect_times_out_and_stops_lifecycle() -> None:
    client = TimeoutLifecycleClient(name="slow", command="server")

    with pytest.raises(asyncio.TimeoutError):
        await client.connect(timeout=0.01)

    assert client._stop_event.is_set()


@pytest.mark.asyncio
async def test_connect_reports_oauth_required() -> None:
    client = OAuthLifecycleClient(name="oauth", command="server")

    with pytest.raises(RuntimeError, match="requires OAuth authorization"):
        await client.connect(timeout=1)


@pytest.mark.asyncio
async def test_reload_requires_connected_client() -> None:
    client = StdIOStatefulClient(name="demo", command="server")

    with pytest.raises(RuntimeError, match="not connected"):
        await client.reload()


@pytest.mark.asyncio
async def test_close_can_raise_when_client_was_never_connected() -> None:
    client = StdIOStatefulClient(name="demo", command="server")

    await client.close()
    with pytest.raises(RuntimeError, match="not connected"):
        await client.close(ignore_errors=False)


@pytest.mark.asyncio
async def test_list_tools_fetches_fresh_schema_and_caches_it() -> None:
    client = StdIOStatefulClient(name="demo", command="server")
    client.is_connected = True
    client.session = FakeSession([FakeTool("search")])

    tools = await client.list_tools()

    assert tools[0].name == "mcp__demo__search"
    assert client._cached_tools[0].name == "search"


@pytest.mark.asyncio
async def test_list_tools_serves_cached_schema_when_disconnected() -> None:
    client = StdIOStatefulClient(name="demo", command="server")
    client._cached_tools = [FakeTool("cached")]

    tools = await client.list_tools()

    assert tools[0].name == "mcp__demo__cached"


@pytest.mark.asyncio
async def test_list_tools_marks_transport_error_for_reconnect() -> None:
    client = StdIOStatefulClient(name="demo", command="server")
    client.is_connected = True
    client._ready_event.set()
    client.session = FakeSession(error=BrokenPipeError("closed"))

    with pytest.raises(BrokenPipeError):
        await client.list_tools()

    assert client.is_connected is False
    assert client._reload_event.is_set()
    assert not client._ready_event.is_set()


@pytest.mark.asyncio
async def test_call_tool_delegates_to_session() -> None:
    session = FakeSession(result={"answer": 42})
    client = StdIOStatefulClient(name="demo", command="server")
    client.is_connected = True
    client.session = session

    result = await client.call_tool("search", {"q": "driver"})

    assert result == {"answer": 42}
    assert session.calls == [("search", {"q": "driver"})]


@pytest.mark.asyncio
async def test_call_tool_marks_transport_error_for_reconnect() -> None:
    client = StdIOStatefulClient(name="demo", command="server")
    client.is_connected = True
    client._ready_event.set()
    client.session = FakeSession(error=BrokenPipeError("closed"))

    with pytest.raises(BrokenPipeError):
        await client.call_tool("search", {})

    assert client.is_connected is False
    assert client._reload_event.is_set()


def test_validate_connection_requires_connected_session() -> None:
    client = StdIOStatefulClient(name="demo", command="server")

    with pytest.raises(RuntimeError, match="not connected"):
        client._validate_connection()

    client.is_connected = True
    with pytest.raises(RuntimeError, match="session is not initialized"):
        client._validate_connection()


def test_transport_error_detection_is_specific() -> None:
    assert _is_transport_error(BrokenPipeError("closed"))
    assert not _is_transport_error(ValueError("bad request"))


def test_401_detection_walks_exception_groups() -> None:
    import httpx

    response = httpx.Response(401, request=httpx.Request("GET", "https://x"))
    error = httpx.HTTPStatusError(
        "unauthorized",
        request=response.request,
        response=response,
    )

    assert _is_401_error(ExceptionGroup("group", [error]))
    assert not _is_401_error(RuntimeError("other"))
