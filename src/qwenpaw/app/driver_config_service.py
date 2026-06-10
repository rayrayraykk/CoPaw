# -*- coding: utf-8 -*-
"""Application service for persisted Driver configuration."""

from __future__ import annotations

import asyncio
import logging
from pathlib import Path
from typing import Any

from fastapi import HTTPException

from qwenpaw.drivers.capabilities import DriverRuntimeInfo
from qwenpaw.drivers.contracts import DriverCard
from qwenpaw.drivers.credentials.store import (
    CredentialStore,
    CredentialStoreProtocol,
)
from qwenpaw.drivers.credentials.types import CredentialRecord
from qwenpaw.drivers.errors import CredentialNotFoundError, DriverCardError
from qwenpaw.drivers.storage import (
    card_path,
    delete_card_paths_for_name,
    dump_card,
    list_card_paths,
    load_card,
)

logger = logging.getLogger(__name__)
_MANAGER_NOT_READY_DETAIL = (
    "Driver manager is not ready yet, please try again later"
)


class DriverConfigService:
    """Own app-layer DriverCard and credential persistence concerns."""

    def __init__(self, workspace: Any) -> None:
        self._workspace = workspace

    @property
    def cards_dir(self) -> Path:
        return self._workspace.workspace_dir / "drivers"

    @property
    def credential_store(self) -> CredentialStoreProtocol:
        manager = getattr(self._workspace, "driver_manager", None)
        if manager is not None:
            return manager.credential_store
        return CredentialStore(
            self._workspace.workspace_dir / "credentials.yaml",
        )

    def card_path(self, name: str, *, protocol: str) -> Path:
        try:
            return card_path(self.cards_dir, name, protocol=protocol)
        except DriverCardError as exc:
            raise HTTPException(400, detail=str(exc)) from exc

    def load_card(self, name: str, *, protocol: str) -> DriverCard:
        path = self.card_path(name, protocol=protocol)
        if not path.is_file():
            raise HTTPException(
                404,
                detail=f"{protocol.upper()} client '{name}' not found",
            )
        card = load_card(path)
        if card.protocol != protocol:
            raise HTTPException(
                404,
                detail=f"{protocol.upper()} client '{name}' not found",
            )
        return card

    def list_cards(self, *, protocol: str | None = None) -> list[DriverCard]:
        cards: dict[str, DriverCard] = {}
        for path in list_card_paths(self.cards_dir):
            try:
                card = load_card(path)
            except Exception as exc:
                logger.warning("Failed to load DriverCard %s: %s", path, exc)
                continue
            if protocol is not None and card.protocol != protocol:
                continue
            cards[card.name] = card
        return sorted(cards.values(), key=lambda item: item.name)

    def load_optional_credential(
        self,
        ref: str,
    ) -> CredentialRecord | None:
        if not ref:
            return None
        try:
            return self.credential_store.get(ref)
        except CredentialNotFoundError:
            return None

    async def save_card(
        self,
        card: DriverCard,
        *,
        reload_driver: bool = True,
    ) -> Path:
        path = self.card_path(card.name, protocol=card.protocol)
        dump_card(card, path)
        delete_card_paths_for_name(self.cards_dir, card.name, keep=path)
        if reload_driver:
            await self.reload_driver_best_effort(card.name)
        return path

    async def reload_driver_best_effort(self, name: str) -> None:
        manager = getattr(self._workspace, "driver_manager", None)
        if manager is None:
            return

        async def reload_background() -> None:
            try:
                await manager.reload_driver(name)
                logger.info("Driver '%s' reloaded and active", name)
            except Exception as exc:
                logger.info(
                    "Driver '%s' saved but not active yet: %s",
                    name,
                    exc,
                )

        asyncio.create_task(
            reload_background(),
            name=f"driver-reload:{name}",
        )

    async def delete_driver_best_effort(self, name: str) -> None:
        manager = getattr(self._workspace, "driver_manager", None)
        if manager is not None:
            try:
                await manager.delete_driver(name)
                return
            except Exception as exc:
                logger.info(
                    "Failed to delete active Driver '%s': %s",
                    name,
                    exc,
                )
        delete_card_paths_for_name(self.cards_dir, name)

    async def ensure_driver_active(self, name: str, *, protocol: str) -> None:
        manager = getattr(self._workspace, "driver_manager", None)
        if manager is None:
            raise HTTPException(
                503,
                detail=_MANAGER_NOT_READY_DETAIL,
            )
        drivers = await manager.list_drivers(protocol=protocol)
        current = next(
            (item for item in drivers if item.name == name),
            None,
        )
        if current is None or current.status != "active":
            status = current.status if current is not None else "missing"
            raise HTTPException(
                503,
                detail=(
                    f"{protocol.upper()} client '{name}' is saved but not "
                    f"active yet (status={status})"
                ),
            )

    async def list_driver_capabilities(
        self,
        name: str,
        *,
        protocol: str,
        kind: str,
        request_context: dict[str, str] | None = None,
    ) -> list[Any]:
        manager = getattr(self._workspace, "driver_manager", None)
        if manager is None:
            raise HTTPException(
                503,
                detail=_MANAGER_NOT_READY_DETAIL,
            )
        await self.ensure_driver_active(name, protocol=protocol)
        return await manager.list_driver_capabilities(
            name,
            kind=kind,
            request_context=request_context or {},
        )


async def ensure_driver_active(
    manager: Any,
    name: str,
    *,
    protocol: str,
) -> DriverRuntimeInfo:
    """Compatibility helper for tests and small call sites."""
    drivers = await manager.list_drivers(protocol=protocol)
    current = next((item for item in drivers if item.name == name), None)
    if current is None or current.status != "active":
        status = current.status if current is not None else "missing"
        raise HTTPException(
            503,
            detail=(
                f"{protocol.upper()} client '{name}' is saved but not "
                f"active yet (status={status})"
            ),
        )
    return current
