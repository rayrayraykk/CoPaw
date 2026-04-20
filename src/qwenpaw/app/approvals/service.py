# -*- coding: utf-8 -*-
"""Approval service for sensitive tool execution.

The ``ApprovalService`` is the single central store for pending /
completed approval records.  Approval is granted exclusively via
the ``/daemon approve`` command in the chat interface.
"""
from __future__ import annotations

import asyncio
import logging
import time
import uuid
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any

from ...security.tool_guard.approval import ApprovalDecision

if TYPE_CHECKING:
    from ...security.tool_guard.models import ToolGuardResult

logger = logging.getLogger(__name__)

_GC_MAX_AGE_SECONDS = 3600.0
_GC_MAX_COMPLETED = 500
_GC_PENDING_MAX_AGE_SECONDS = 1800.0
_GC_MAX_PENDING = 200
_GC_SESSION_TREE_MAX_AGE_SECONDS = 7200.0  # 2 hours


# ------------------------------------------------------------------
# Data model
# ------------------------------------------------------------------


@dataclass
class SessionNode:
    """Node in the session relationship tree.

    Tracks parent-child relationships between sessions to support
    cross-session approval lookup.

    Attributes:
        session_id: Current session ID
        parent_session_id: Parent session ID (None if root)
        root_session_id: Root session ID (self if root)
        created_at: Node creation timestamp
        last_accessed: Last access timestamp (for GC)
    """

    session_id: str
    parent_session_id: str | None
    root_session_id: str
    created_at: float
    last_accessed: float


@dataclass
class PendingApproval:
    """In-memory record for one pending approval."""

    request_id: str
    session_id: str
    user_id: str
    channel: str
    tool_name: str
    created_at: float
    future: asyncio.Future[ApprovalDecision]
    status: str = "pending"
    resolved_at: float | None = None
    result_summary: str = ""
    findings_count: int = 0
    extra: dict[str, Any] = field(default_factory=dict)
    root_session_id: str = ""  # New: root session for cross-session lookup


# ------------------------------------------------------------------
# Service
# ------------------------------------------------------------------


class ApprovalService:
    """Central approval service.

    Tracks pending and completed approval records.  Approval is
    resolved via ``/daemon approve`` (see ``runner.py`` and
    ``daemon_commands.py``).
    """

    def __init__(self) -> None:
        self._lock = asyncio.Lock()
        self._pending: dict[str, PendingApproval] = {}
        self._completed: dict[str, PendingApproval] = {}

        # Session relationship tree (Phase 1)
        self._session_tree: dict[str, SessionNode] = {}
        self._session_tree_lock = asyncio.Lock()

    def _get_base_url(self) -> str | None:
        """Get API base URL from config.

        Returns:
            Base URL (e.g. "http://localhost:8088") or None if not configured
        """
        try:
            from ...config.utils import read_last_api

            api_info = read_last_api()
            if api_info:
                host, port = api_info
                return f"http://{host}:{port}"
            return None
        except Exception as exc:
            logger.warning(
                "Failed to read API config for notifications: %s",
                exc,
            )
            return None

    # ------------------------------------------------------------------
    # Core approval lifecycle
    # ------------------------------------------------------------------

    async def create_pending(
        self,
        *,
        session_id: str,
        user_id: str,
        channel: str,
        tool_name: str,
        result: "ToolGuardResult",
        extra: dict[str, Any] | None = None,
        root_session_id: str = "",  # Phase 2: root for cross-session
    ) -> PendingApproval:
        """Create a pending approval record and return it.

        Args:
            session_id: Current session ID
            user_id: User ID
            channel: Channel name
            tool_name: Tool name requiring approval
            result: Tool guard result
            extra: Extra metadata
            root_session_id: Root session ID (for cross-session approval)

        Returns:
            PendingApproval record
        """
        from ...security.tool_guard.approval import format_findings_summary

        request_id = str(uuid.uuid4())
        loop = asyncio.get_running_loop()

        # Use provided root_session_id or default to session_id
        if not root_session_id:
            root_session_id = session_id

        pending = PendingApproval(
            request_id=request_id,
            session_id=session_id,
            user_id=user_id,
            channel=channel,
            tool_name=tool_name,
            root_session_id=root_session_id,
            created_at=time.time(),
            future=loop.create_future(),
            result_summary=format_findings_summary(result),
            findings_count=result.findings_count,
            extra=dict(extra or {}),
        )

        async with self._lock:
            self._pending[request_id] = pending
            self._gc_pending_locked()
            self._gc_completed_locked()

        # Phase 2: Send notification if this is a sub-session approval
        # (Only notify when session_id != root_session_id)
        base_url = self._get_base_url()
        if base_url and root_session_id and session_id != root_session_id:
            await self._send_approval_notification(pending, base_url)

        return pending

    async def resolve_request(
        self,
        request_id: str,
        decision: ApprovalDecision,
    ) -> PendingApproval | None:
        """Resolve one pending approval request."""
        async with self._lock:
            pending = self._pending.pop(request_id, None)
            if pending is None:
                return self._completed.get(request_id)

            pending.status = decision.value
            pending.resolved_at = time.time()
            self._completed[request_id] = pending
            self._gc_completed_locked()

        if not pending.future.done():
            pending.future.set_result(decision)

        return pending

    async def get_request(self, request_id: str) -> PendingApproval | None:
        """Get a request by id whether pending or already resolved."""
        async with self._lock:
            return self._pending.get(request_id) or self._completed.get(
                request_id,
            )

    async def get_pending_by_session(
        self,
        session_id: str,
    ) -> PendingApproval | None:
        """Return the next pending approval for *session_id* (FIFO).

        Pending approvals are consumed in creation order, so repeated
        ``/approve`` inputs walk the queue from oldest to newest.
        """
        async with self._lock:
            for pending in self._pending.values():
                if (
                    pending.session_id == session_id
                    and pending.status == "pending"
                ):
                    return pending
        return None

    async def get_all_pending_by_session(
        self,
        session_id: str,
    ) -> list[PendingApproval]:
        """Return all pending approvals for *session_id* (FIFO order)."""
        async with self._lock:
            return [
                p
                for p in self._pending.values()
                if p.session_id == session_id and p.status == "pending"
            ]

    async def cancel_stale_pending_for_tool_call(
        self,
        session_id: str,
        tool_call_id: str,
    ) -> int:
        """Cancel pending approvals whose stored tool_call id matches.

        When a tool call is replayed (e.g. after ``/approve`` triggers
        sibling replay), the guard may create a *new* pending for the
        same logical tool call.  This method cancels the old pending
        first so orphaned records don't accumulate.

        Returns the number of records cancelled.
        """
        now = time.time()
        cancelled = 0
        async with self._lock:
            to_cancel = [
                k
                for k, p in self._pending.items()
                if p.session_id == session_id
                and p.status == "pending"
                and isinstance(p.extra.get("tool_call"), dict)
                and p.extra["tool_call"].get("id") == tool_call_id
            ]
            for k in to_cancel:
                pending = self._pending.pop(k)
                if not pending.future.done():
                    pending.future.set_result(ApprovalDecision.TIMEOUT)
                pending.status = "superseded"
                pending.resolved_at = now
                self._completed[k] = pending
                cancelled += 1
        if cancelled:
            logger.info(
                "Tool guard: cancelled %d stale pending approval(s) "
                "for tool_call %s (session %s)",
                cancelled,
                tool_call_id,
                session_id[:8],
            )
        return cancelled

    async def consume_approval(
        self,
        session_id: str,
        tool_name: str,
        tool_params: dict[str, Any] | None = None,
    ) -> bool:
        """Check and consume a one-shot tool approval.

        If *tool_name* was recently approved via ``/daemon approve``
        for *session_id*, remove the completed record and return
        ``True`` so the caller can skip the guard check.

        When *tool_params* is given, the approved call's stored
        parameters are compared.  A mismatch causes the approval
        to be rejected (returns ``False``), preventing an approved
        ``rm foo.txt`` from being used to execute ``rm -rf /``.
        """
        async with self._lock:
            for key, completed in list(self._completed.items()):
                if (
                    completed.session_id == session_id
                    and completed.tool_name == tool_name
                    and completed.status == "approved"
                ):
                    if tool_params is not None:
                        approved_call = completed.extra.get(
                            "tool_call",
                            {},
                        )
                        approved_params = approved_call.get(
                            "input",
                            {},
                        )
                        if approved_params != tool_params:
                            logger.warning(
                                "Tool guard: params mismatch for "
                                "'%s' (session %s), rejecting "
                                "stale approval",
                                tool_name,
                                session_id[:8],
                            )
                            del self._completed[key]
                            return False
                    del self._completed[key]
                    return True
        return False

    # ------------------------------------------------------------------
    # Garbage collection
    # ------------------------------------------------------------------

    def _gc_pending_locked(self) -> None:
        """Evict stale pending records whose futures were never resolved.

        Caller must hold ``_lock``.
        """
        now = time.time()
        expired = [
            k
            for k, v in self._pending.items()
            if now - v.created_at > _GC_PENDING_MAX_AGE_SECONDS
        ]
        for k in expired:
            pending = self._pending.pop(k)
            if not pending.future.done():
                pending.future.set_result(ApprovalDecision.TIMEOUT)
            pending.status = "timeout"
            pending.resolved_at = now
            self._completed[k] = pending

        overflow = len(self._pending) - _GC_MAX_PENDING
        if overflow <= 0:
            return
        ordered = sorted(
            self._pending.items(),
            key=lambda item: item[1].created_at,
        )
        for key, pending in ordered[:overflow]:
            del self._pending[key]
            if not pending.future.done():
                pending.future.set_result(ApprovalDecision.TIMEOUT)
            pending.status = "timeout"
            pending.resolved_at = now
            self._completed[key] = pending

    def _gc_completed_locked(self) -> None:
        """Remove stale/overflow completed records.

        Caller must hold ``_lock``.
        """
        now = time.time()
        expired = [
            k
            for k, v in self._completed.items()
            if v.resolved_at and now - v.resolved_at > _GC_MAX_AGE_SECONDS
        ]
        for k in expired:
            del self._completed[k]

        # Still over cap: evict oldest completed records first.
        overflow = len(self._completed) - _GC_MAX_COMPLETED
        if overflow <= 0:
            return
        ordered = sorted(
            self._completed.items(),
            key=lambda item: item[1].resolved_at or item[1].created_at,
        )
        for key, _pending in ordered[:overflow]:
            del self._completed[key]

    # ------------------------------------------------------------------
    # Session relationship tree (Phase 1)
    # ------------------------------------------------------------------

    async def register_session_relationship(
        self,
        session_id: str,
        parent_session_id: str | None = None,
    ) -> str:
        """Register parent-child session relationship.

        Args:
            session_id: Current session ID
            parent_session_id: Parent session ID (None if root)

        Returns:
            root_session_id: Root session ID for this session tree

        Example:
            # Root session (no parent)
            root_id = await svc.register_session_relationship("session-main")
            # root_id == "session-main"

            # Child session
            root_id = await svc.register_session_relationship(
                "session-worker-1",
                "session-main"
            )
            # root_id == "session-main"
        """
        if not session_id:
            logger.warning("Empty session_id in register_session_relationship")
            return session_id

        now = time.time()

        async with self._session_tree_lock:
            # If already registered, update last_accessed and return root
            if session_id in self._session_tree:
                node = self._session_tree[session_id]
                node.last_accessed = now
                return node.root_session_id

        # Determine root_session_id (outside lock to allow recursion)
        if parent_session_id:
            async with self._session_tree_lock:
                if parent_session_id in self._session_tree:
                    # Inherit root from parent
                    parent_node = self._session_tree[parent_session_id]
                    root_session_id = parent_node.root_session_id
                else:
                    # Need to register parent first - release lock
                    pass  # Will register parent outside lock

            # Register parent if needed (outside lock to avoid deadlock)
            async with self._session_tree_lock:
                if parent_session_id not in self._session_tree:
                    # Release lock and register parent
                    pass

            # Check again after potential parent registration
            if parent_session_id not in self._session_tree:
                # Parent not registered yet, create parent as root first
                parent_root = await self.register_session_relationship(
                    parent_session_id,
                    None,
                )
                root_session_id = parent_root
            else:
                async with self._session_tree_lock:
                    parent_node = self._session_tree[parent_session_id]
                    root_session_id = parent_node.root_session_id
        else:
            # No parent, this is root
            root_session_id = session_id

        # Create node
        async with self._session_tree_lock:
            # Double-check if already registered (race condition)
            if session_id in self._session_tree:
                node = self._session_tree[session_id]
                node.last_accessed = now
                return node.root_session_id

            node = SessionNode(
                session_id=session_id,
                parent_session_id=parent_session_id,
                root_session_id=root_session_id,
                created_at=now,
                last_accessed=now,
            )
            self._session_tree[session_id] = node

            logger.debug(
                "Registered session: %s parent=%s root=%s",
                session_id[:8],
                parent_session_id[:8] if parent_session_id else "None",
                root_session_id[:8],
            )

            # GC old nodes
            self._gc_session_tree_locked()

        return root_session_id

    async def get_root_session(self, session_id: str) -> str:
        """Get root session ID for given session (O(1) lookup).

        Args:
            session_id: Session ID to query

        Returns:
            root_session_id: Root session ID (self if not registered)

        Example:
            root = await svc.get_root_session("session-worker-1")
            # root == "session-main"
        """
        if not session_id:
            return session_id

        async with self._session_tree_lock:
            node = self._session_tree.get(session_id)
            if node:
                node.last_accessed = time.time()
                return node.root_session_id
            # Not registered, return self as root
            return session_id

    def _gc_session_tree_locked(self) -> None:
        """Remove stale session nodes.

        Nodes not accessed for 2 hours are removed.
        Caller must hold _session_tree_lock.
        """
        now = time.time()
        expired = [
            k
            for k, v in self._session_tree.items()
            if now - v.last_accessed > _GC_SESSION_TREE_MAX_AGE_SECONDS
        ]
        for k in expired:
            del self._session_tree[k]
            logger.debug(
                "GC session node: %s (age=%.1fs)",
                k[:8],
                now
                - self._session_tree.get(
                    k,
                    SessionNode("", None, "", 0, 0),
                ).last_accessed
                if k in self._session_tree
                else 0,
            )

    # ------------------------------------------------------------------
    # Cross-session approval lookup (Phase 2)
    # ------------------------------------------------------------------

    async def get_all_pending_by_root_session(
        self,
        root_session_id: str,
    ) -> list[PendingApproval]:
        """Get all pending approvals under a root session.

        Args:
            root_session_id: Root session ID

        Returns:
            List of pending approvals (oldest first)

        Example:
            # Main session queries all pending (including sub-sessions)
            all_pending = await svc.get_all_pending_by_root_session(
                "session-main"
            )
            # Returns pending from session-main, session-worker-1, etc.
        """
        if not root_session_id:
            return []

        async with self._lock:
            # Find all pending with matching root_session_id
            matching = [
                p
                for p in self._pending.values()
                if p.root_session_id == root_session_id
            ]
            # Sort by created_at (oldest first)
            matching.sort(key=lambda p: p.created_at)
            return matching

    async def _send_approval_notification(
        self,
        pending: PendingApproval,
        base_url: str,
    ) -> None:
        """Send approval notification to root session via HTTP API.

        This method is called when a sub-session triggers an approval.
        It uses the existing `qwenpaw channels send` HTTP API.

        Args:
            pending: PendingApproval record
            base_url: API base URL from config

        Note:
            Only called when session_id != root_session_id
        """
        try:
            import httpx

            # Build notification message
            risk_level = "UNKNOWN"
            if pending.findings_count > 0:
                if pending.findings_count >= 3:
                    risk_level = "HIGH 🔴"
                elif pending.findings_count >= 1:
                    risk_level = "MEDIUM 🟡"
                else:
                    risk_level = "LOW 🟢"

            short_session = pending.session_id[:12] + "..."
            short_request = pending.request_id[:8]

            notification_text = (
                f"🔔 **Approval Required** (from sub-session)\n\n"
                f"- Sub-session: `{short_session}`\n"
                f"- Tool: `{pending.tool_name}`\n"
                f"- Risk: {risk_level}\n"
                f"- Request ID: `{short_request}`\n\n"
                f"Commands:\n"
                f"- `/approval approve {short_request}` - Approve this\n"
                f"- `/approval deny {short_request}` - Deny this\n"
                f"- `/approval list` - View all pending\n\n"
                f"Shortcuts: `/approve {short_request}` "
                f"or `/deny {short_request}`\n"
            )

            # Call channels send HTTP API
            async with httpx.AsyncClient(timeout=10.0) as client:
                response = await client.post(
                    f"{base_url}/api/channels/send",
                    json={
                        "channel": pending.channel,
                        "target_user": pending.user_id,
                        "target_session": pending.root_session_id,
                        "text": notification_text,
                    },
                )
                response.raise_for_status()

            logger.info(
                "Approval notification sent: request_id=%s root_session=%s",
                pending.request_id[:8],
                pending.root_session_id[:8],
            )

        except Exception as exc:
            logger.warning(
                "Failed to send approval notification: %s",
                exc,
                exc_info=True,
            )


# ------------------------------------------------------------------
# Singleton
# ------------------------------------------------------------------

_approval_service: ApprovalService | None = None


def get_approval_service() -> ApprovalService:
    """Return the process-wide approval service singleton."""
    global _approval_service
    if _approval_service is None:
        _approval_service = ApprovalService()
    return _approval_service
