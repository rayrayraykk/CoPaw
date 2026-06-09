from pathlib import Path

import pytest

from qwenpaw.app.workspace import Workspace
from qwenpaw.drivers.contracts import CredentialRef, DriverCard
from qwenpaw.drivers.storage import card_path, dump_card


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


def _a2a_card(name: str, enabled: bool = True) -> DriverCard:
    return DriverCard(
        name=name,
        protocol="a2a",
        endpoint={"transport": "stdio"},
        credential=CredentialRef(kind="none"),
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
        _a2a_card("disabled", enabled=False),
        card_path(workspace.workspace_dir / "drivers", "disabled", protocol="a2a"),
    )

    manager = await _start_driver_service(workspace)

    assert await _active_driver_names(manager) == []


@pytest.mark.asyncio
async def test_driver_manager_shutdown_all_on_service_stop(
    tmp_path: Path,
) -> None:
    workspace = Workspace("agent", str(tmp_path / "agent"))
    dump_card(
        _a2a_card("enabled"),
        card_path(workspace.workspace_dir / "drivers", "enabled", protocol="a2a"),
    )
    manager = await _start_driver_service(workspace)

    assert await _active_driver_names(manager) == ["enabled"]

    await _stop_driver_service(workspace)

    assert await _active_driver_names(manager) == []
