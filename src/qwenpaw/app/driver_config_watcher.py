# -*- coding: utf-8 -*-
"""Watch DriverCard files and hot-reload active Drivers.

This is the protocol-neutral successor to the old MCP-only config watcher.
Console/API saves already trigger reloads directly; this watcher preserves the
manual-edit path for ``drivers/<protocol>/<name>.yaml`` files.
"""

from __future__ import annotations

import asyncio
import logging
from pathlib import Path
from typing import Optional, TYPE_CHECKING

from ..drivers.storage import list_card_paths

if TYPE_CHECKING:
    from ..drivers.manager import DriverManager

logger = logging.getLogger(__name__)

DEFAULT_POLL_INTERVAL = 2.0


class DriverConfigWatcher:
    """Poll DriverCard storage and reload changed external capabilities."""

    def __init__(
        self,
        driver_manager: "DriverManager",
        cards_dir: Path,
        poll_interval: float = DEFAULT_POLL_INTERVAL,
    ) -> None:
        self._driver_manager = driver_manager
        self._cards_dir = cards_dir
        self._poll_interval = poll_interval
        self._task: Optional[asyncio.Task] = None
        self._last_snapshot: dict[str, tuple[str, float]] = {}

    async def start(self) -> None:
        """Take an initial snapshot and start the polling task."""
        self._last_snapshot = self._snapshot()
        self._task = asyncio.create_task(
            self._poll_loop(),
            name="driver_config_watcher",
        )
        logger.info(
            "DriverConfigWatcher started (poll=%.1fs, path=%s)",
            self._poll_interval,
            self._cards_dir,
        )

    async def stop(self) -> None:
        """Stop the polling task."""
        if self._task is None:
            return
        self._task.cancel()
        try:
            await self._task
        except asyncio.CancelledError:
            pass
        self._task = None
        logger.info("DriverConfigWatcher stopped")

    def _snapshot(self) -> dict[str, tuple[str, float]]:
        snapshot: dict[str, tuple[str, float]] = {}
        for path in list_card_paths(self._cards_dir):
            try:
                mtime = path.stat().st_mtime
            except FileNotFoundError:
                continue
            snapshot[path.stem] = (
                path.relative_to(self._cards_dir).as_posix(),
                mtime,
            )
        return snapshot

    async def _poll_loop(self) -> None:
        while True:
            try:
                await asyncio.sleep(self._poll_interval)
                await self._check_once()
            except Exception:
                logger.warning(
                    "DriverConfigWatcher poll iteration failed",
                    exc_info=True,
                )

    async def _check_once(self) -> None:
        current = self._snapshot()
        if current == self._last_snapshot:
            return

        old = self._last_snapshot

        removed = sorted(set(old) - set(current))
        changed = sorted(
            name for name, state in current.items() if old.get(name) != state
        )

        for name in removed:
            try:
                await self._driver_manager.delete_driver(name)
                logger.info("Driver '%s' removed after card deletion", name)
            except Exception:
                logger.warning(
                    "Failed to remove Driver '%s' after card deletion",
                    name,
                    exc_info=True,
                )

        for name in changed:
            try:
                await self._driver_manager.reload_driver(name)
                logger.info("Driver '%s' reloaded after card change", name)
            except Exception:
                logger.warning(
                    "Failed to reload Driver '%s' after card change",
                    name,
                    exc_info=True,
                )

        # reload_driver() normalizes and rewrites cards, which updates mtimes.
        # Refresh after handling changes so the watcher does not reload the
        # same Driver again on every poll.
        self._last_snapshot = self._snapshot()
