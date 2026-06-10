# -*- coding: utf-8 -*-
import asyncio
from pathlib import Path
from typing import Any

import pytest

from qwenpaw.drivers.capabilities import (
    CapabilityExposure,
    DriverCapability,
    DriverInvocation,
    format_capability_id,
)
from qwenpaw.drivers.credentials.store import CredentialStore
from qwenpaw.drivers.credentials.types import ResolvedCredential
from qwenpaw.drivers.errors import UnsupportedProtocolError
from qwenpaw.drivers.handler import DriverHandler
from qwenpaw.drivers.contracts import CredentialRef, DriverCard
from qwenpaw.drivers.manager import DriverManager
from qwenpaw.drivers.policy import PolicyContext
from qwenpaw.drivers.storage import card_path, dump_card

SETUP_EVENTS: list[str] = []
SHUTDOWN_EVENTS: list[str] = []


class FakeHandler(DriverHandler):
    async def _setup(self) -> None:
        SETUP_EVENTS.append(_event(self))
        if self.card.config.get("cancel_setup"):
            raise asyncio.CancelledError
        if self.card.config.get("fail_setup"):
            raise RuntimeError("setup failed")

    async def _teardown(self) -> None:
        SHUTDOWN_EVENTS.append(_event(self))

    async def list_capabilities(
        self,
        request_context: dict[str, str] | None = None,
    ) -> list[DriverCapability]:
        del request_context
        if self.card.config.get("fail_list"):
            raise RuntimeError(f"list failed for {self.name}")
        namespace = str(self.card.config.get("display_name") or self.name)
        return [
            DriverCapability(
                capability_id=format_capability_id(
                    self.card.protocol,
                    self.name,
                    "tool",
                    "invoke",
                    "noop",
                ),
                driver_name=self.name,
                protocol=self.card.protocol,
                kind="tool",
                action="invoke",
                name="noop",
                exposure=CapabilityExposure(
                    as_tool=True,
                    namespace=namespace,
                    tool_name=f"{namespace}__noop",
                ),
            ),
        ]

    async def _execute(
        self,
        credential: ResolvedCredential,
        context: PolicyContext,
        **kwargs: Any,
    ) -> Any:
        del credential
        del context
        return kwargs


class AlternateHandler(FakeHandler):
    pass


def _event(handler: DriverHandler) -> str:
    failed = bool(handler.card.config.get("fail_setup"))
    return f"{handler.name}:{type(handler).__name__}:{failed}"


def _card(
    name: str,
    protocol: str = "fake",
    enabled: bool = True,
    fail_setup: bool = False,
    cancel_setup: bool = False,
    fail_list: bool = False,
) -> DriverCard:
    config = {}
    if fail_setup:
        config["fail_setup"] = True
    if cancel_setup:
        config["cancel_setup"] = True
    if fail_list:
        config["fail_list"] = True
    return DriverCard(
        name=name,
        protocol=protocol,
        endpoint={},
        credential=CredentialRef(kind="none"),
        enabled=enabled,
        config=config,
    )


def _manager(tmp_path: Path) -> DriverManager:
    manager = DriverManager(
        tmp_path / "drivers",
        CredentialStore(tmp_path / "credentials.yaml"),
    )
    manager.register_handler_type("fake", FakeHandler)
    manager.register_handler_type("vendor/tool", AlternateHandler)
    manager.register_handler_type("vendor/special/tool", FakeHandler)
    return manager


async def _active_names(manager: DriverManager) -> list[str]:
    infos = await manager.list_drivers()
    return [info.name for info in infos if info.status == "active"]


@pytest.fixture(autouse=True)
def clear_events() -> None:
    SETUP_EVENTS.clear()
    SHUTDOWN_EVENTS.clear()


@pytest.mark.asyncio
async def test_protocol_routing_uses_exact_match(tmp_path: Path) -> None:
    manager = _manager(tmp_path)

    await manager.register_driver(_card("a", "fake"))
    await manager.register_driver(_card("b", "vendor/tool"))
    await manager.register_driver(_card("c", "vendor/special/tool"))

    assert SETUP_EVENTS == [
        "a:FakeHandler:False",
        "b:AlternateHandler:False",
        "c:FakeHandler:False",
    ]


@pytest.mark.asyncio
async def test_register_driver_writes_protocol_directory(
    tmp_path: Path,
) -> None:
    manager = _manager(tmp_path)

    await manager.register_driver(_card("demo"))

    nested = tmp_path / "drivers" / "fake" / "demo.yaml"
    assert nested.is_file()
    assert not (tmp_path / "drivers" / "demo.yaml").exists()


@pytest.mark.asyncio
async def test_protocol_routing_rejects_prefix_only_match(
    tmp_path: Path,
) -> None:
    manager = _manager(tmp_path)

    with pytest.raises(UnsupportedProtocolError):
        await manager.register_driver(_card("a", "vendor/tool/extra"))


@pytest.mark.asyncio
async def test_unsupported_protocol_raises(tmp_path: Path) -> None:
    with pytest.raises(UnsupportedProtocolError):
        await _manager(tmp_path).register_driver(_card("a", "missing"))


@pytest.mark.asyncio
async def test_invoke_capability_returns_invalid_id_result(
    tmp_path: Path,
) -> None:
    result = await _manager(tmp_path).invoke_capability(
        DriverInvocation(capability_id="invalid", payload={}),
    )

    assert result.ok is False
    assert result.error_type == "invalid_capability_id"


@pytest.mark.asyncio
async def test_invoke_capability_returns_missing_driver_result(
    tmp_path: Path,
) -> None:
    result = await _manager(tmp_path).invoke_capability(
        DriverInvocation(
            capability_id="driver://fake/missing/tools/noop#invoke",
            payload={},
        ),
    )

    assert result.ok is False
    assert result.error_type == "driver_not_found"
    assert result.metadata == {"driver_name": "missing"}


@pytest.mark.asyncio
async def test_build_drivers_skips_disabled_card(tmp_path: Path) -> None:
    manager = _manager(tmp_path)
    cards_dir = tmp_path / "drivers"
    dump_card(
        _card("enabled"),
        card_path(cards_dir, "enabled", protocol="fake"),
    )
    dump_card(
        _card("disabled", enabled=False),
        card_path(cards_dir, "disabled", protocol="fake"),
    )

    await manager.build_drivers()

    assert await _active_names(manager) == ["enabled"]
    assert (cards_dir / "fake" / "enabled.yaml").is_file()
    assert (cards_dir / "fake" / "disabled.yaml").is_file()


@pytest.mark.asyncio
async def test_init_failure_does_not_publish_handler(tmp_path: Path) -> None:
    manager = _manager(tmp_path)
    cards_dir = tmp_path / "drivers"
    dump_card(
        _card("bad", fail_setup=True),
        card_path(cards_dir, "bad", protocol="fake"),
    )

    await manager.build_drivers()

    assert await _active_names(manager) == []


@pytest.mark.asyncio
async def test_reload_success_replaces_and_shutdowns_old(
    tmp_path: Path,
) -> None:
    manager = _manager(tmp_path)
    await manager.register_driver(_card("demo"))
    SETUP_EVENTS.clear()
    SHUTDOWN_EVENTS.clear()
    dump_card(
        _card("demo", protocol="vendor/tool"),
        card_path(tmp_path / "drivers", "demo", protocol="fake"),
    )

    await manager.reload_driver("demo")

    assert await _active_names(manager) == ["demo"]
    assert SETUP_EVENTS == ["demo:AlternateHandler:False"]
    assert SHUTDOWN_EVENTS == ["demo:FakeHandler:False"]


@pytest.mark.asyncio
async def test_reload_failure_keeps_old_handler(tmp_path: Path) -> None:
    manager = _manager(tmp_path)
    await manager.register_driver(_card("demo"))
    SETUP_EVENTS.clear()
    SHUTDOWN_EVENTS.clear()
    dump_card(
        _card("demo", fail_setup=True),
        card_path(tmp_path / "drivers", "demo", protocol="fake"),
    )

    with pytest.raises(RuntimeError, match="setup failed"):
        await manager.reload_driver("demo")

    assert await _active_names(manager) == ["demo"]
    assert SETUP_EVENTS == ["demo:FakeHandler:True"]
    assert SHUTDOWN_EVENTS == ["demo:FakeHandler:True"]


@pytest.mark.asyncio
async def test_list_capabilities_uses_latest_stored_runtime_metadata(
    tmp_path: Path,
) -> None:
    manager = _manager(tmp_path)
    card = _card("demo")
    card.config["display_name"] = "old-platform"
    await manager.register_driver(card)

    updated = _card("demo")
    updated.config["display_name"] = "new-platform"
    dump_card(
        updated,
        card_path(tmp_path / "drivers", "demo", protocol="fake"),
    )

    capabilities = await manager.list_capabilities(kind="tool")

    assert capabilities[0].exposure.namespace == "new-platform"
    assert capabilities[0].exposure.tool_name == "new-platform__noop"


@pytest.mark.asyncio
async def test_list_driver_capabilities_queries_only_target(
    tmp_path: Path,
) -> None:
    manager = _manager(tmp_path)
    await manager.register_driver(_card("good"))
    await manager.register_driver(_card("bad", fail_list=True))

    capabilities = await manager.list_driver_capabilities("good", kind="tool")

    assert [capability.driver_name for capability in capabilities] == ["good"]


@pytest.mark.asyncio
async def test_cancelled_setup_shuts_down_partial_handler(
    tmp_path: Path,
) -> None:
    manager = _manager(tmp_path)

    with pytest.raises(asyncio.CancelledError):
        await manager.register_driver(_card("demo", cancel_setup=True))

    assert await _active_names(manager) == []
    assert SETUP_EVENTS == ["demo:FakeHandler:False"]
    assert SHUTDOWN_EVENTS == ["demo:FakeHandler:False"]


@pytest.mark.asyncio
async def test_shutdown_all_clears_handlers(tmp_path: Path) -> None:
    manager = _manager(tmp_path)
    await manager.register_driver(_card("a"))
    await manager.register_driver(_card("b"))

    await manager.shutdown_all()

    assert await _active_names(manager) == []
    assert SHUTDOWN_EVENTS == [
        "a:FakeHandler:False",
        "b:FakeHandler:False",
    ]
