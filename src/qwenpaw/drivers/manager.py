# -*- coding: utf-8 -*-
"""Driver manager and lifecycle owner."""

from __future__ import annotations

import asyncio
import logging
from collections.abc import Callable
from pathlib import Path

from qwenpaw.drivers.approval import ApprovalGate
from qwenpaw.drivers.capabilities import (
    DriverCapability,
    DriverInvocation,
    DriverInvocationResult,
    DriverRuntimeInfo,
    parse_capability_id,
)
from qwenpaw.drivers.constants import (
    CREDENTIAL_ALIAS_DEFAULT,
    CREDENTIAL_KIND_NONE,
)
from qwenpaw.drivers.credentials.providers import build_provider
from qwenpaw.drivers.credentials.store import CredentialStoreProtocol
from qwenpaw.drivers.credentials.types import CredentialRecord
from qwenpaw.drivers.errors import (
    DriverNotFoundError,
    UnsupportedProtocolError,
)
from qwenpaw.drivers.handler import DriverHandler
from qwenpaw.drivers.contracts import (
    CredentialRef,
    DriverCard,
    coerce_card,
    iter_credential_refs,
    validate_card,
)
from qwenpaw.drivers.storage import (
    card_path,
    card_paths_for_name,
    delete_card_paths_for_name,
    dump_card,
    list_card_paths,
    load_card,
)

logger = logging.getLogger(__name__)
_SHUTDOWN_TIMEOUT_SECONDS = 10.0
EndpointValidator = Callable[[DriverCard], None]


class DriverManager:
    """Own external capability storage, lifecycle, and dispatch.

    DriverManager is protocol-neutral: MCP, ACP, and A2A handlers expose their
    native operations as Driver capabilities with a shared policy, credential,
    and invocation lifecycle.
    """

    def __init__(
        self,
        cards_dir: Path,
        credential_store: CredentialStoreProtocol,
        approval_gate: ApprovalGate | None = None,
    ) -> None:
        self._cards_dir = cards_dir
        self._credential_store = credential_store
        self._approval_gate = approval_gate
        self._handler_types: dict[str, type[DriverHandler]] = {}
        self._endpoint_validators: dict[str, EndpointValidator] = {}
        self._handlers: dict[str, DriverHandler] = {}
        self._lock = asyncio.Lock()

    def register_handler_type(
        self,
        protocol: str,
        cls: type[DriverHandler],
        endpoint_validator: EndpointValidator | None = None,
    ) -> None:
        """Register the handler for an exact Driver protocol."""
        if not protocol:
            raise UnsupportedProtocolError(protocol)
        self._handler_types[protocol] = cls
        if endpoint_validator is not None:
            self._endpoint_validators[protocol] = endpoint_validator

    async def start(self) -> None:
        """Build enabled drivers from persisted DriverCards."""
        await self.build_drivers()

    async def build_drivers(self) -> None:
        """Scan cards_dir and build enabled handlers."""
        built: dict[str, DriverHandler] = {}
        for path in list_card_paths(self._cards_dir):
            try:
                card = load_card(path)
            except Exception as exc:
                logger.warning(
                    "Failed to build Driver from %s: %s",
                    path,
                    exc,
                    exc_info=True,
                )
                continue
            try:
                if not card.enabled:
                    logger.debug(
                        "Driver '%s' is disabled; skipping",
                        card.name,
                    )
                    continue
                handler = await self._build_and_init_handler(card)
                built[card.name] = handler
            except Exception as exc:
                logger.warning(
                    "Failed to build Driver '%s': %s",
                    card.name,
                    exc,
                    exc_info=True,
                )

        async with self._lock:
            old_handlers = self._handlers
            self._handlers = built

        await self._shutdown_handlers(old_handlers.values())

    async def upsert_driver(
        self,
        card: DriverCard,
        credential: CredentialRecord | None = None,
    ) -> DriverRuntimeInfo:
        """Persist Driver data, build handler, then publish after init."""
        if credential is not None:
            self._credential_store.put(credential)
        await self.register_driver(card)
        return self._runtime_info_from_card(card)

    async def register_driver(self, card: DriverCard) -> None:
        """Persist card, build handler, then publish after init success."""
        card = self._validate_card_for_registered_protocol(card)
        path = card_path(self._cards_dir, card.name, card.protocol)
        dump_card(card, path)
        delete_card_paths_for_name(self._cards_dir, card.name, keep=path)
        handler = None
        if card.enabled:
            handler = await self._build_and_init_handler(card)

        async with self._lock:
            old = self._handlers.pop(card.name, None)
            if handler is not None:
                self._handlers[card.name] = handler

        if old is not None:
            await self._shutdown_handler(old)

    async def reload_driver(self, name: str) -> DriverRuntimeInfo | None:
        """Build-before-swap reload. Failure keeps old handler."""
        path = self._stored_card_path(name)
        if path is None:
            raise DriverNotFoundError(name)
        card = load_card(path)
        card = self._validate_card_for_registered_protocol(card)
        handler = None
        if card.enabled:
            handler = await self._build_and_init_handler(card)
        canonical = card_path(self._cards_dir, card.name, card.protocol)
        dump_card(card, canonical)
        delete_card_paths_for_name(self._cards_dir, card.name, keep=canonical)

        async with self._lock:
            old = self._handlers.get(name)
            if handler is None:
                old = self._handlers.pop(name, None)
            else:
                self._handlers[name] = handler

        if old is not None:
            await self._shutdown_handler(old)
        return self._runtime_info_from_card(card)

    async def delete_driver(self, name: str) -> None:
        """Delete persisted card and shutdown a published handler."""
        delete_card_paths_for_name(self._cards_dir, name)
        async with self._lock:
            old = self._handlers.pop(name, None)
        if old is not None:
            await self._shutdown_handler(old)

    async def shutdown_all(self) -> None:
        """Shutdown all handlers and clear manager state."""
        async with self._lock:
            handlers = list(self._handlers.values())
            self._handlers.clear()
        await self._shutdown_handlers(handlers)

    async def list_drivers(
        self,
        protocol: str | None = None,
    ) -> list[DriverRuntimeInfo]:
        """Return configured Drivers with their current lifecycle status."""
        cards: dict[str, DriverRuntimeInfo] = {}
        for path in list_card_paths(self._cards_dir):
            try:
                card = load_card(path)
            except Exception as exc:
                cards[path.stem] = DriverRuntimeInfo(
                    name=path.stem,
                    protocol="",
                    enabled=False,
                    status="error",
                    error=str(exc),
                )
                continue
            if protocol is not None and card.protocol != protocol:
                continue
            cards[card.name] = self._runtime_info_from_card(card)
        return sorted(cards.values(), key=lambda item: item.name)

    async def list_capabilities(
        self,
        *,
        protocol: str | None = None,
        kind: str | None = None,
        request_context: dict[str, str] | None = None,
    ) -> list[DriverCapability]:
        """Return capabilities exposed by active handlers."""
        handlers = self._iter_handlers(protocol)
        capabilities: list[DriverCapability] = []
        for handler in handlers:
            for capability in await handler.list_capabilities(
                request_context=request_context,
            ):
                if kind is None or capability.kind == kind:
                    capabilities.append(capability)
        return sorted(capabilities, key=lambda item: item.capability_id)

    async def list_driver_capabilities(
        self,
        name: str,
        *,
        kind: str | None = None,
        request_context: dict[str, str] | None = None,
    ) -> list[DriverCapability]:
        """Return capabilities from one active Driver only."""
        handler = self._get_handler(name)
        capabilities = await handler.list_capabilities(
            request_context=request_context,
        )
        return sorted(
            [
                capability
                for capability in capabilities
                if kind is None or capability.kind == kind
            ],
            key=lambda item: item.capability_id,
        )

    async def invoke_capability(
        self,
        invocation: DriverInvocation,
    ) -> DriverInvocationResult:
        """Dispatch one capability invocation to its owning handler."""
        try:
            _, driver_name, _, _, _ = parse_capability_id(
                invocation.capability_id,
            )
        except ValueError as exc:
            return DriverInvocationResult(
                ok=False,
                error_type="invalid_capability_id",
                message=str(exc),
            )
        try:
            handler = self._get_handler(driver_name)
        except DriverNotFoundError as exc:
            return DriverInvocationResult(
                ok=False,
                error_type="driver_not_found",
                message=str(exc),
                metadata={"driver_name": exc.name},
            )
        return await handler.invoke_capability(invocation)

    def _get_handler(self, name: str) -> DriverHandler:
        handler = self._handlers.get(name)
        if handler is None:
            raise DriverNotFoundError(name)
        return handler

    def _iter_handlers(
        self,
        protocol: str | None = None,
    ) -> list[DriverHandler]:
        handlers = list(self._handlers.values())
        if protocol is not None:
            handlers = [
                handler
                for handler in handlers
                if handler.card.protocol == protocol
            ]
        return sorted(handlers, key=lambda handler: handler.name)

    async def _build_and_init_handler(self, card: DriverCard) -> DriverHandler:
        handler = self._build_handler(card)
        try:
            await handler.init()
        except asyncio.CancelledError:
            # CancelledError is not caught by ``Exception`` on Python 3.11+.
            await self._shutdown_handler(handler)
            raise
        except Exception:
            await self._shutdown_handler(handler)
            raise
        return handler

    def _build_handler(self, card: DriverCard) -> DriverHandler:
        card = self._validate_card_for_registered_protocol(card)
        handler_type = self._resolve_handler_type(card.protocol)
        refs = iter_credential_refs(card)
        if refs:
            providers = {
                alias: build_provider(ref, self._credential_store)
                for alias, ref in refs.items()
            }
            primary = providers.get(CREDENTIAL_ALIAS_DEFAULT) or next(
                iter(providers.values()),
            )
        else:
            primary = build_provider(
                CredentialRef(kind=CREDENTIAL_KIND_NONE),
                self._credential_store,
            )
            providers = {CREDENTIAL_ALIAS_DEFAULT: primary}
        return handler_type(
            card,
            primary,
            providers,
            approval_gate=self._approval_gate,
        )

    def _resolve_handler_type(self, protocol: str) -> type[DriverHandler]:
        if protocol in self._handler_types:
            return self._handler_types[protocol]
        raise UnsupportedProtocolError(protocol)

    def _validate_card_for_registered_protocol(
        self,
        card: DriverCard,
    ) -> DriverCard:
        card = coerce_card(card)
        validate_card(card)
        self._resolve_handler_type(card.protocol)
        validator = self._endpoint_validators.get(card.protocol)
        if validator is not None:
            validator(card)
        return card

    def _stored_card_path(self, name: str) -> Path | None:
        paths = card_paths_for_name(self._cards_dir, name)
        if paths:
            return paths[0]
        return None

    def _runtime_info_from_card(self, card: DriverCard) -> DriverRuntimeInfo:
        active = card.name in self._handlers
        if active:
            status = "active"
        elif card.enabled:
            status = "inactive"
        else:
            status = "disabled"
        return DriverRuntimeInfo(
            name=card.name,
            protocol=card.protocol,
            enabled=card.enabled,
            status=status,
            display_name=str(card.config.get("display_name") or card.name),
            description=str(card.config.get("description") or ""),
        )

    @property
    def cards_dir(self) -> Path:
        return self._cards_dir

    @property
    def credential_store(self) -> CredentialStoreProtocol:
        return self._credential_store

    async def _shutdown_handlers(self, handlers) -> None:
        await asyncio.gather(
            *[
                self._shutdown_handler_with_timeout(handler)
                for handler in handlers
            ],
            return_exceptions=True,
        )

    @staticmethod
    async def _shutdown_handler(handler: DriverHandler) -> None:
        try:
            await handler.shutdown()
        except Exception as exc:
            logger.warning(
                "Error shutting down Driver '%s': %s",
                handler.name,
                exc,
                exc_info=True,
            )

    @classmethod
    async def _shutdown_handler_with_timeout(
        cls,
        handler: DriverHandler,
    ) -> None:
        task = asyncio.create_task(
            cls._shutdown_handler(handler),
            name=f"driver-shutdown:{handler.name}",
        )
        try:
            await asyncio.wait_for(task, timeout=_SHUTDOWN_TIMEOUT_SECONDS)
        except asyncio.TimeoutError:
            logger.warning(
                "Timed out shutting down Driver '%s' after %.1fs; "
                "cancellation requested",
                handler.name,
                _SHUTDOWN_TIMEOUT_SECONDS,
            )
            if not task.done():
                task.cancel()
            try:
                await task
            except asyncio.CancelledError:
                logger.debug(
                    "Driver '%s' shutdown task cancelled after timeout",
                    handler.name,
                )
