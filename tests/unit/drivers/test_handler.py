# -*- coding: utf-8 -*-
import asyncio
from typing import Any

import pytest

from qwenpaw.drivers.credentials.types import ResolvedCredential
from qwenpaw.drivers.capabilities import DriverInvocation
from qwenpaw.app.approvals.service import ApprovalService
from qwenpaw.app.approvals.driver_gate import QwenPawDriverApprovalGate
from qwenpaw.drivers.approval import ApprovalGate
from qwenpaw.drivers.errors import (
    ApprovalRequiredError,
    DriverPermissionDeniedError,
    PermissionDeniedError,
)
from qwenpaw.drivers.handler import DriverHandler
from qwenpaw.drivers.contracts import (
    DriverCard,
    DriverPolicy,
    PolicyRule,
    PolicyTarget,
)
from qwenpaw.drivers.policy import PolicyContext

# pylint: disable=protected-access


class RecordingProvider:
    def __init__(self, events: list[str]) -> None:
        self.events = events
        self.closed = False

    async def resolve(self) -> ResolvedCredential:
        self.events.append("credential")
        return ResolvedCredential(kind="static", secrets={"token": "abc"})

    async def close(self) -> None:
        self.closed = True
        self.events.append("close")


class RecordingHandler(DriverHandler):
    def __init__(
        self,
        card: DriverCard,
        provider: RecordingProvider,
        events: list[str],
        teardown_raises: bool = False,
        approval_gate: ApprovalGate | None = None,
    ) -> None:
        super().__init__(card, provider, approval_gate=approval_gate)
        self.events = events
        self.teardown_raises = teardown_raises

    async def _setup(self) -> None:
        self.events.append("setup")

    async def _teardown(self) -> None:
        self.events.append("teardown")
        if self.teardown_raises:
            raise RuntimeError("teardown failed")

    async def _execute(
        self,
        credential: ResolvedCredential,
        context: PolicyContext,
        **kwargs: Any,
    ) -> Any:
        del context
        self.events.append("execute")
        return {"credential": credential.values, "kwargs": kwargs}


def _card() -> DriverCard:
    return DriverCard(
        name="demo",
        protocol="fake",
        endpoint={},
    )


def test_driver_permission_denial_message_is_point_in_time() -> None:
    error = DriverPermissionDeniedError(
        driver_name="mcp:notes",
        subject="user:default",
        operation="invoke",
        reason="Rule denied add_note.",
    )

    message = error.to_user_message()

    assert "current tool call" in message
    assert "policy observed at execution time" in message
    assert "automatically retry" not in message
    assert "If the user later asks again" not in message


def test_sync_runtime_metadata_updates_display_and_policy_only() -> None:
    events: list[str] = []
    card = _card()
    card.endpoint["url"] = "old"
    handler = RecordingHandler(card, RecordingProvider(events), events)
    updated = _card()
    updated.endpoint["url"] = "new"
    updated.config["display_name"] = "new-platform"
    updated.enabled = False
    updated.policy = DriverPolicy(
        default_effect="allow",
        rules=[PolicyRule(subject="user:alice", effect="deny")],
    )

    handler.sync_runtime_metadata(updated)

    assert handler.card.endpoint["url"] == "old"
    assert handler.card.config["display_name"] == "new-platform"
    assert handler.card.enabled is False
    assert handler.card.policy.default_effect == "allow"
    assert handler.card.policy.rules[0].subject == "user:alice"


async def _next_pending_request(
    service: ApprovalService,
    task: asyncio.Task | None = None,
):
    # pylint: disable=protected-access
    while not service._pending:
        if task is not None and task.done():
            await task
        await asyncio.sleep(0)
    return next(iter(service._pending.values()))


@pytest.mark.asyncio
async def test_authorize_invocation_order(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    events: list[str] = []

    def fake_evaluate_policy(*args, **kwargs):
        del args
        del kwargs
        events.append("policy")
        return "allow"

    monkeypatch.setattr(
        "qwenpaw.drivers.handler.evaluate_policy",
        fake_evaluate_policy,
    )
    provider = RecordingProvider(events)
    handler = RecordingHandler(_card(), provider, events)

    result = await handler._authorize_invocation(
        "user:alice",
        extras={"value": 1},
    )

    assert events == ["policy"]
    assert result.extras == {"value": 1}


@pytest.mark.asyncio
async def test_base_invoke_capability_returns_unsupported_result() -> None:
    events: list[str] = []
    handler = RecordingHandler(
        _card(),
        RecordingProvider(events),
        events,
        approval_gate=QwenPawDriverApprovalGate(),
    )

    result = await handler.invoke_capability(
        DriverInvocation(
            capability_id="driver://fake/demo/tools/noop#invoke",
            payload={},
        ),
    )

    assert result.ok is False
    assert result.error_type == "unsupported_capability"
    assert "driver://fake/demo/tools/noop#invoke" in result.message


@pytest.mark.asyncio
async def test_deny_does_not_resolve_credential(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    events: list[str] = []
    monkeypatch.setattr(
        "qwenpaw.drivers.handler.evaluate_policy",
        lambda *_, **__: "deny",
    )
    handler = RecordingHandler(
        _card(),
        RecordingProvider(events),
        events,
        approval_gate=QwenPawDriverApprovalGate(),
    )

    with pytest.raises(PermissionDeniedError):
        await handler._authorize_invocation("user:alice")

    assert not events


@pytest.mark.asyncio
async def test_ask_raises_default_approval_required(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        "qwenpaw.drivers.handler.evaluate_policy",
        lambda *_, **__: "ask",
    )
    events: list[str] = []
    handler = RecordingHandler(_card(), RecordingProvider(events), events)

    with pytest.raises(ApprovalRequiredError):
        await handler._authorize_invocation("user:alice")

    assert not events


@pytest.mark.asyncio
async def test_ask_approval_approved_resumes_execution(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        "qwenpaw.drivers.handler.evaluate_policy",
        lambda *_, **__: "ask",
    )
    service = ApprovalService()
    monkeypatch.setattr(
        "qwenpaw.app.approvals.get_approval_service",
        lambda: service,
    )
    from qwenpaw.security.tool_guard.approval import ApprovalDecision

    events: list[str] = []
    handler = RecordingHandler(
        _card(),
        RecordingProvider(events),
        events,
        approval_gate=QwenPawDriverApprovalGate(),
    )
    task = asyncio.create_task(
        handler._authorize_invocation(
            "user:alice",
            request_context={
                "session_id": "session-1",
                "root_session_id": "session-1",
                "agent_id": "agent-1",
                "user_id": "alice",
                "channel": "console",
            },
            extras={"value": 1},
        ),
    )

    pending = await _next_pending_request(service, task)
    assert pending.tool_name == "driver:fake:demo"
    assert pending.result_summary == (
        "Driver 'fake:demo' requires approval for invoke."
    )
    assert pending.extra["source_type"] == "driver_policy"
    assert pending.extra["driver"]["name"] == "demo"
    assert pending.extra["driver"]["protocol"] == "fake"
    assert pending.extra["tool_call"]["name"] == "driver:fake:demo"
    await service.resolve_request(
        pending.request_id,
        ApprovalDecision.APPROVED,
    )

    context = await task

    assert context.extras == {"value": 1}
    assert not events


@pytest.mark.asyncio
async def test_ask_approval_adds_tool_display_source_metadata(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        "qwenpaw.drivers.handler.evaluate_policy",
        lambda *_, **__: "ask",
    )
    service = ApprovalService()
    monkeypatch.setattr(
        "qwenpaw.app.approvals.get_approval_service",
        lambda: service,
    )
    from qwenpaw.security.tool_guard.approval import ApprovalDecision

    events: list[str] = []
    card = DriverCard(
        name="aone-code-mcp",
        protocol="mcp",
        endpoint={},
    )
    handler = RecordingHandler(
        card,
        RecordingProvider(events),
        events,
        approval_gate=QwenPawDriverApprovalGate(),
    )
    task = asyncio.create_task(
        handler._authorize_invocation(
            "user:alice",
            target=PolicyTarget(
                kind="tool",
                name="search_authorized_repositories",
            ),
            request_context={
                "session_id": "session-1",
                "root_session_id": "session-1",
                "agent_id": "agent-1",
            },
            extras={"arguments": {"query": "qwenpaw"}},
        ),
    )

    pending = await _next_pending_request(service, task)
    assert pending.tool_name == "driver:mcp:aone-code-mcp"
    assert pending.extra["display"] == {
        "tool_name": "search_authorized_repositories",
        "tool_source": "mcp:aone-code-mcp",
    }
    assert pending.result_summary == (
        "Tool 'search_authorized_repositories' from "
        "'mcp:aone-code-mcp' requires approval for invoke."
    )

    await service.resolve_request(
        pending.request_id,
        ApprovalDecision.APPROVED,
    )
    context = await task

    assert context.extras == {"arguments": {"query": "qwenpaw"}}
    assert not events


@pytest.mark.asyncio
async def test_ask_approval_denied_blocks_execution(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        "qwenpaw.drivers.handler.evaluate_policy",
        lambda *_, **__: "ask",
    )
    service = ApprovalService()
    monkeypatch.setattr(
        "qwenpaw.app.approvals.get_approval_service",
        lambda: service,
    )
    from qwenpaw.security.tool_guard.approval import ApprovalDecision

    events: list[str] = []
    handler = RecordingHandler(
        _card(),
        RecordingProvider(events),
        events,
        approval_gate=QwenPawDriverApprovalGate(),
    )
    task = asyncio.create_task(
        handler._authorize_invocation(
            "user:alice",
            request_context={"session_id": "session-1", "agent_id": "agent-1"},
        ),
    )

    pending = await _next_pending_request(service, task)
    await service.resolve_request(pending.request_id, ApprovalDecision.DENIED)

    with pytest.raises(DriverPermissionDeniedError):
        await task

    assert not events


@pytest.mark.asyncio
async def test_shutdown_closes_provider_even_if_teardown_raises() -> None:
    events: list[str] = []
    provider = RecordingProvider(events)
    handler = RecordingHandler(
        _card(),
        provider,
        events,
        teardown_raises=True,
    )

    with pytest.raises(RuntimeError, match="teardown failed"):
        await handler.shutdown()

    assert provider.closed
    assert events == ["teardown", "close"]
