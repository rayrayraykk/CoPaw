# -*- coding: utf-8 -*-
from pathlib import Path

import pytest

from qwenpaw.app.workspace import Workspace
from qwenpaw.drivers.contracts import DriverCard
from qwenpaw.drivers.storage import card_path, dump_card


class _FakeMCPClient:
    def __init__(self, **_kwargs) -> None:
        self.is_connected = False

    async def connect(self) -> None:
        self.is_connected = True

    async def close(self, ignore_errors: bool = True) -> None:
        del ignore_errors
        self.is_connected = False

    async def list_tools(self) -> list:
        return []

    async def call_tool(self, _name: str, _arguments: dict):
        raise AssertionError("call_tool is not used by these tests")


def _patch_mcp_client(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(
        "qwenpaw.drivers.handlers.mcp.StdIOStatefulClient",
        _FakeMCPClient,
    )


async def _start_driver_service(workspace: Workspace):
    # pylint: disable=protected-access
    descriptor = workspace._service_manager.descriptors["driver_manager"]
    await workspace._service_manager._start_service(descriptor)
    return workspace.driver_manager


async def _stop_driver_service(workspace: Workspace) -> None:
    # pylint: disable=protected-access
    descriptor = workspace._service_manager.descriptors["driver_manager"]
    await workspace._service_manager._stop_service(descriptor, final=True)


async def _active_driver_names(manager) -> list[str]:
    infos = await manager.list_drivers()
    return [info.name for info in infos if info.status == "active"]


def _mcp_card(name: str, enabled: bool = True) -> DriverCard:
    return DriverCard(
        name=name,
        protocol="mcp",
        endpoint={"transport": "stdio", "command": "fake-mcp"},
        enabled=enabled,
    )


@pytest.mark.asyncio
async def test_driver_manager_property_none_before_start(
    tmp_path: Path,
) -> None:
    workspace = Workspace("agent", str(tmp_path / "agent"))

    assert workspace.driver_manager is None


@pytest.mark.asyncio
async def test_driver_manager_starts_without_drivers_dir(
    tmp_path: Path,
) -> None:
    workspace = Workspace("agent", str(tmp_path / "agent"))

    manager = await _start_driver_service(workspace)

    assert manager is workspace.driver_manager
    assert await _active_driver_names(manager) == []


@pytest.mark.asyncio
async def test_driver_manager_skips_disabled_card(tmp_path: Path) -> None:
    workspace = Workspace("agent", str(tmp_path / "agent"))
    dump_card(
        _mcp_card("disabled", enabled=False),
        card_path(
            workspace.workspace_dir / "drivers",
            "disabled",
            protocol="mcp",
        ),
    )

    manager = await _start_driver_service(workspace)

    assert await _active_driver_names(manager) == []


@pytest.mark.asyncio
async def test_driver_manager_shutdown_all_on_service_stop(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _patch_mcp_client(monkeypatch)
    workspace = Workspace("agent", str(tmp_path / "agent"))
    dump_card(
        _mcp_card("enabled"),
        card_path(
            workspace.workspace_dir / "drivers",
            "enabled",
            protocol="mcp",
        ),
    )
    manager = await _start_driver_service(workspace)

    assert await _active_driver_names(manager) == ["enabled"]

    await _stop_driver_service(workspace)

    assert await _active_driver_names(manager) == []
