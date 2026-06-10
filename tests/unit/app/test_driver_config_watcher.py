# -*- coding: utf-8 -*-
# pylint: disable=protected-access
from __future__ import annotations

from pathlib import Path

import pytest

from qwenpaw.app.driver_config_watcher import DriverConfigWatcher
from qwenpaw.drivers.contracts import CredentialRef, DriverCard
from qwenpaw.drivers.storage import card_path, dump_card


def _card(
    name: str = "demo",
    *,
    command: str = "demo",
) -> DriverCard:
    return DriverCard(
        name=name,
        protocol="mcp",
        endpoint={"transport": "stdio", "command": command},
        credential=CredentialRef(kind="none"),
    )


class FakeDriverManager:
    def __init__(self) -> None:
        self.reloaded: list[str] = []
        self.deleted: list[str] = []
        self.rewrite_on_reload = False
        self.cards_dir: Path | None = None

    async def reload_driver(self, name: str) -> None:
        self.reloaded.append(name)
        if self.rewrite_on_reload:
            assert self.cards_dir is not None
            dump_card(
                _card(name, command=f"rewritten-{len(self.reloaded)}"),
                card_path(self.cards_dir, name, protocol="mcp"),
            )

    async def delete_driver(self, name: str) -> None:
        self.deleted.append(name)


@pytest.mark.asyncio
async def test_driver_config_watcher_reloads_changed_card(
    tmp_path: Path,
) -> None:
    cards_dir = tmp_path / "drivers"
    manager = FakeDriverManager()
    watcher = DriverConfigWatcher(manager, cards_dir)
    path = card_path(cards_dir, "demo", protocol="mcp")
    dump_card(_card(command="before"), path)
    watcher._last_snapshot = watcher._snapshot()

    dump_card(_card(command="after"), path)
    await watcher._check_once()

    assert manager.reloaded == ["demo"]
    assert not manager.deleted


@pytest.mark.asyncio
async def test_driver_config_watcher_deletes_removed_card(
    tmp_path: Path,
) -> None:
    cards_dir = tmp_path / "drivers"
    manager = FakeDriverManager()
    watcher = DriverConfigWatcher(manager, cards_dir)
    path = card_path(cards_dir, "demo", protocol="mcp")
    dump_card(_card(), path)
    watcher._last_snapshot = watcher._snapshot()

    path.unlink()
    await watcher._check_once()

    assert manager.deleted == ["demo"]
    assert not manager.reloaded


@pytest.mark.asyncio
async def test_driver_config_watcher_refreshes_snapshot_after_reload_rewrite(
    tmp_path: Path,
) -> None:
    cards_dir = tmp_path / "drivers"
    manager = FakeDriverManager()
    manager.cards_dir = cards_dir
    manager.rewrite_on_reload = True
    watcher = DriverConfigWatcher(manager, cards_dir)
    path = card_path(cards_dir, "demo", protocol="mcp")
    dump_card(_card(command="before"), path)
    watcher._last_snapshot = watcher._snapshot()

    dump_card(_card(command="after"), path)
    await watcher._check_once()
    await watcher._check_once()

    assert manager.reloaded == ["demo"]
