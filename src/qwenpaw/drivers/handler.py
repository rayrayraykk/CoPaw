"""DriverHandler template method base class."""

from __future__ import annotations

import logging
from abc import ABC, abstractmethod
from typing import Any

from qwenpaw.drivers.capabilities import (
    DriverCapability,
    DriverInvocation,
    DriverInvocationResult,
)
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
    ) -> None:
        self._card = card
        self._credential_provider = credential_provider
        self._credential_providers = credential_providers or {
            "default": credential_provider,
        }

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
    async def _setup(self) -> None: ...

    @abstractmethod
    async def _teardown(self) -> None: ...

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

    async def _guarded_execute(
        self,
        subject: str,
        operation: str = "invoke",
        request_context: dict[str, str] | None = None,
        target: PolicyTarget | None = None,
        subjects: list[str] | tuple[str, ...] | None = None,
        **kwargs: Any,
    ) -> Any:
        context = DriverInvocationContext(
            subject=subject,
            driver_name=self._card.name,
            protocol=self._card.protocol,
            operation=operation,
            target=target or PolicyTarget(),
            subjects=tuple(subjects or ()),
            request_context=dict(request_context or {}),
            extras=dict(kwargs),
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
        credential = await self._credential_provider.resolve()
        return await self._execute(credential, context, **kwargs)

    async def _request_approval(self, context: DriverInvocationContext) -> None:
        # Reuse QwenPaw's approval Future flow: this coroutine pauses until the
        # console or command approval endpoint resolves the pending request.
        ctx = context.request_context
        session_id = str(ctx.get("session_id") or "")
        driver_label = f"driver:{context.protocol}:{context.driver_name}"
        driver_ref = f"{context.protocol}:{context.driver_name}"
        target_name = str(context.target.name or "")
        has_tool_target = context.target.kind == "tool" and bool(target_name)
        display_tool_name = target_name if has_tool_target else driver_label
        display_tool_source = driver_ref
        if has_tool_target:
            result_summary = (
                f"Tool '{display_tool_name}' from '{display_tool_source}' "
                f"requires approval for {context.operation}."
            )
        else:
            result_summary = (
                f"Driver '{driver_ref}' requires approval for {context.operation}."
            )
        if not session_id:
            raise ApprovalRequiredError(
                "Driver approval required but request_context.session_id "
                f"is missing: {context.subject} -> {context.driver_name}",
            )

        from qwenpaw.app.approvals import get_approval_service
        from qwenpaw.app.approvals.service import ApprovalRequestSummary
        from qwenpaw.constant import TOOL_GUARD_APPROVAL_TIMEOUT_SECONDS
        from qwenpaw.security.tool_guard.approval import ApprovalDecision

        svc = get_approval_service()
        tool_call_id = str(ctx.get("tool_call_id") or "")
        if tool_call_id:
            await svc.cancel_stale_pending_for_tool_call(
                session_id,
                tool_call_id,
            )

        pending = await svc.create_pending_summary(
            session_id=session_id,
            root_session_id=str(ctx.get("root_session_id") or session_id),
            owner_agent_id=str(ctx.get("root_agent_id") or ctx.get("agent_id") or ""),
            user_id=str(ctx.get("user_id") or ""),
            channel=str(ctx.get("channel") or ""),
            agent_id=str(ctx.get("agent_id") or "unknown"),
            summary=ApprovalRequestSummary(
                source_type="driver_policy",
                name=driver_label,
                severity="medium",
                findings_count=1,
                result_summary=result_summary,
            ),
            timeout_seconds=TOOL_GUARD_APPROVAL_TIMEOUT_SECONDS,
            extra={
                "display": {
                    "tool_name": display_tool_name,
                    "tool_source": display_tool_source,
                },
                "driver": {
                    "name": context.driver_name,
                    "protocol": context.protocol,
                    "operation": context.operation,
                    "subject": context.subject,
                    "extras": context.extras,
                },
                "tool_call": {
                    "id": tool_call_id,
                    "name": driver_label,
                    "input": context.extras,
                },
            },
        )
        decision = await svc.wait_for_approval(
            pending.request_id,
            TOOL_GUARD_APPROVAL_TIMEOUT_SECONDS,
        )
        if decision == ApprovalDecision.APPROVED:
            return
        raise DriverPermissionDeniedError(
            context.driver_name,
            context.subject,
            context.operation,
            reason=f"User approval decision was {decision.value}.",
        )

    @abstractmethod
    async def _execute(
        self,
        credential: ResolvedCredential,
        context: DriverInvocationContext,
        **kwargs: Any,
    ) -> Any: ...

    def set_policy(self, policy: DriverPolicy | list[PolicyRule]) -> None:
        self._card.policy = coerce_driver_policy(policy)

    def sync_runtime_metadata(self, card: DriverCard) -> None:
        """Apply DriverCard fields that are safe to refresh without reconnecting."""
        if card.name != self._card.name or card.protocol != self._card.protocol:
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
