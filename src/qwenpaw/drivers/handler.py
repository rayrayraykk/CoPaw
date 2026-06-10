# -*- coding: utf-8 -*-
"""DriverHandler template method base class."""

from __future__ import annotations

import logging
from abc import ABC, abstractmethod
from typing import Any

from qwenpaw.drivers.approval import ApprovalGate
from qwenpaw.drivers.capabilities import (
    DriverCapability,
    DriverInvocation,
    DriverInvocationResult,
)
from qwenpaw.drivers.credentials.bindings import resolve_credentials
from qwenpaw.drivers.credentials.providers import CredentialProvider
from qwenpaw.drivers.credentials.types import ResolvedCredential
from qwenpaw.drivers.errors import (
    ApprovalRequiredError,
    DriverPermissionDeniedError,
)
from qwenpaw.drivers.contracts import (
    DriverCard,
    DriverPolicy,
    PolicyRule,
    PolicyTarget,
    coerce_driver_policy,
)
from qwenpaw.drivers.policy import DriverInvocationContext, evaluate_policy

logger = logging.getLogger(__name__)


class DriverHandler(ABC):
    def __init__(
        self,
        card: DriverCard,
        credential_provider: CredentialProvider,
        credential_providers: dict[str, CredentialProvider] | None = None,
        approval_gate: ApprovalGate | None = None,
    ) -> None:
        self._card = card
        self._credential_provider = credential_provider
        self._credential_providers = credential_providers or {
            "default": credential_provider,
        }
        self._approval_gate = approval_gate

    async def init(self) -> None:
        await self._setup()

    async def shutdown(self) -> None:
        try:
            await self._teardown()
        finally:
            seen: set[int] = set()
            for provider in self._credential_providers.values():
                provider_id = id(provider)
                if provider_id in seen:
                    continue
                seen.add(provider_id)
                await provider.close()

    @abstractmethod
    async def _setup(self) -> None:
        ...

    @abstractmethod
    async def _teardown(self) -> None:
        ...

    async def list_capabilities(
        self,
        request_context: dict[str, str] | None = None,
    ) -> list[DriverCapability]:
        """Return protocol-neutral capabilities exposed by this Driver."""
        del request_context
        return []

    async def invoke_capability(
        self,
        invocation: DriverInvocation,
    ) -> DriverInvocationResult:
        """Invoke one capability. Protocol handlers override this method."""
        return DriverInvocationResult(
            ok=False,
            error_type="unsupported_capability",
            message=(
                f"Driver '{self.name}' does not support capability "
                f"invocation: {invocation.capability_id}"
            ),
        )

    async def _authorize_invocation(
        self,
        subject: str,
        operation: str = "invoke",
        request_context: dict[str, str] | None = None,
        target: PolicyTarget | None = None,
        subjects: list[str] | tuple[str, ...] | None = None,
        extras: dict[str, Any] | None = None,
    ) -> DriverInvocationContext:
        """Evaluate Driver policy for one protocol-specific operation."""
        context = DriverInvocationContext(
            subject=subject,
            driver_name=self._card.name,
            protocol=self._card.protocol,
            operation=operation,
            target=target or PolicyTarget(),
            subjects=tuple(subjects or ()),
            request_context=dict(request_context or {}),
            extras=dict(extras or {}),
        )
        effect = evaluate_policy(self._card.policy, context)
        if effect == "deny":
            raise DriverPermissionDeniedError(
                self._card.name,
                subject,
                operation,
            )
        if effect == "ask":
            await self._request_approval(context)
        return context

    async def _resolve_credentials(self) -> dict[str, ResolvedCredential]:
        """Resolve all credential aliases for protocol handlers."""
        return await resolve_credentials(self._credential_providers)

    async def _request_approval(
        self,
        context: DriverInvocationContext,
    ) -> None:
        if self._approval_gate is None:
            raise ApprovalRequiredError(
                "Driver approval required but no approval gate is wired: "
                f"{context.subject} -> {context.driver_name}",
            )
        await self._approval_gate.request_approval(context)

    async def _execute(
        self,
        credential: ResolvedCredential,
        context: DriverInvocationContext,
        **kwargs: Any,
    ) -> Any:
        del credential
        del context
        del kwargs
        raise NotImplementedError(
            f"Driver '{self.name}' does not implement _execute(). "
            "Protocol handlers may instead override invoke_capability().",
        )

    def set_policy(self, policy: DriverPolicy | list[PolicyRule]) -> None:
        self._card.policy = coerce_driver_policy(policy)

    def sync_runtime_metadata(self, card: DriverCard) -> None:
        """Refresh DriverCard fields that do not require reconnecting."""
        if (
            card.name != self._card.name
            or card.protocol != self._card.protocol
        ):
            return
        self._card.config = dict(card.config)
        self._card.policy = coerce_driver_policy(card.policy)
        self._card.enabled = bool(card.enabled)

    @property
    def name(self) -> str:
        return self._card.name

    @property
    def card(self) -> DriverCard:
        return self._card
