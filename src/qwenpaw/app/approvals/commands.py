# -*- coding: utf-8 -*-
"""Unified approval command handling module.

Provides centralized logic for approval commands:
- /approval list - List all pending approvals
- /approval approve <request_id> - Approve specific request
- /approval deny <request_id> - Deny specific request
- /approval show <request_id> - Show request details

This module is called by the Control Command handler and supports backward
compatibility with legacy /approve and /deny commands.
"""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Optional

if TYPE_CHECKING:
    from .service import ApprovalService, PendingApproval

logger = logging.getLogger(__name__)


async def handle_approval_command(
    session_id: str,
    subcommand: str,
    args: str,
) -> str:
    """Handle approval command and return response text.

    Args:
        session_id: Current session ID
        subcommand: Subcommand (list/approve/deny/show)
        args: Command arguments (e.g. request_id)

    Returns:
        Response text for user

    Example:
        handle_approval_command("session-1", "list", "")
        handle_approval_command("session-1", "approve", "abc123de")
        handle_approval_command("session-1", "deny", "abc123de")
    """
    from .service import get_approval_service

    service = get_approval_service()

    # Get root session ID for cross-session lookup
    root_session_id = await service.get_root_session(session_id)

    if subcommand == "list":
        return await _handle_list(service, root_session_id)
    elif subcommand == "approve":
        return await _handle_approve(service, root_session_id, args)
    elif subcommand == "deny":
        return await _handle_deny(service, root_session_id, args)
    elif subcommand == "show":
        return await _handle_show(service, root_session_id, args)
    else:
        return (
            f"**Unknown Subcommand**\n\n"
            f"Unknown subcommand: `{subcommand}`\n\n"
            f"Available commands:\n"
            f"- `/approval list` - List all pending approvals\n"
            f"- `/approval approve <id>` - Approve request\n"
            f"- `/approval deny <id>` - Deny request\n"
            f"- `/approval show <id>` - Show request details"
        )


async def _handle_list(
    service: "ApprovalService",
    root_session_id: str,
) -> str:
    """Handle /approval list command.

    Args:
        service: ApprovalService instance
        root_session_id: Root session ID

    Returns:
        Formatted list of pending approvals
    """
    all_pending = await service.list_pending_by_session(root_session_id)

    if not all_pending:
        return (
            "**No Pending Approvals**\n\n"
            "There are no pending approval requests."
        )

    lines = ["**Pending Approvals**\n"]

    for idx, pending in enumerate(all_pending, 1):
        short_id = pending.request_id[:8]
        short_session = pending.session_id[:12]

        # Determine risk level
        risk_level = "UNKNOWN"
        if pending.findings_count > 0:
            if pending.findings_count >= 3:
                risk_level = "HIGH 🔴"
            elif pending.findings_count >= 1:
                risk_level = "MEDIUM 🟡"
            else:
                risk_level = "LOW 🟢"

        # Show if from sub-session
        session_marker = ""
        if pending.session_id != root_session_id:
            session_marker = f" (sub-session: `{short_session}...`)"

        lines.append(
            f"{idx}. **{pending.tool_name}** - ID: `{short_id}` - "
            f"Risk: {risk_level}{session_marker}",
        )

    lines.append("\n**Commands:**")
    lines.append("- `/approval approve <id>` - Approve request")
    lines.append("- `/approval deny <id>` - Deny request")
    lines.append("- `/approval show <id>` - Show details")

    return "\n".join(lines)


async def _handle_approve(
    service: "ApprovalService",
    root_session_id: str,
    args: str,
) -> str:
    """Handle /approval approve <request_id> command.

    Args:
        service: ApprovalService instance
        root_session_id: Root session ID
        args: Request ID (full or short prefix)

    Returns:
        Success or error message
    """
    request_id = args.strip()

    if not request_id:
        # FIFO: approve oldest pending
        all_pending = await service.list_pending_by_session(root_session_id)
        if not all_pending:
            return (
                "**No Pending Approvals**\n\n"
                "There are no pending approval requests."
            )
        pending = all_pending[0]
    else:
        # Find by ID (exact or prefix match)
        pending = await _find_pending_by_id(
            service,
            root_session_id,
            request_id,
        )
        if pending is None:
            return (
                f"**Request Not Found**\n\n"
                f"No pending approval found with ID: `{request_id}`"
            )

    # Approve the request
    try:
        service.approve(pending.request_id)
        short_id = pending.request_id[:8]
        return (
            f"**Approval Granted**\n\n"
            f"Request `{short_id}` for tool `{pending.tool_name}` approved."
        )
    except Exception as exc:
        logger.exception("Failed to approve request")
        return f"**Approval Failed**\n\n{str(exc)}"


async def _handle_deny(
    service: "ApprovalService",
    root_session_id: str,
    args: str,
) -> str:
    """Handle /approval deny <request_id> command.

    Args:
        service: ApprovalService instance
        root_session_id: Root session ID
        args: Request ID (full or short prefix)

    Returns:
        Success or error message
    """
    request_id = args.strip()

    if not request_id:
        return (
            "**Missing Request ID**\n\n"
            "Usage: `/approval deny <request_id>`\n\n"
            "Use `/approval list` to see pending requests."
        )

    # Find by ID (exact or prefix match)
    pending = await _find_pending_by_id(
        service,
        root_session_id,
        request_id,
    )
    if pending is None:
        return (
            f"**Request Not Found**\n\n"
            f"No pending approval found with ID: `{request_id}`"
        )

    # Deny the request
    try:
        service.deny(pending.request_id)
        short_id = pending.request_id[:8]
        return (
            f"**Approval Denied**\n\n"
            f"Request `{short_id}` for tool `{pending.tool_name}` denied."
        )
    except Exception as exc:
        logger.exception("Failed to deny request")
        return f"**Denial Failed**\n\n{str(exc)}"


async def _handle_show(
    service: "ApprovalService",
    root_session_id: str,
    args: str,
) -> str:
    """Handle /approval show <request_id> command.

    Args:
        service: ApprovalService instance
        root_session_id: Root session ID
        args: Request ID (full or short prefix)

    Returns:
        Detailed request information
    """
    request_id = args.strip()

    if not request_id:
        return (
            "**Missing Request ID**\n\n"
            "Usage: `/approval show <request_id>`\n\n"
            "Use `/approval list` to see pending requests."
        )

    # Find by ID (exact or prefix match)
    pending = await _find_pending_by_id(
        service,
        root_session_id,
        request_id,
    )
    if pending is None:
        return (
            f"**Request Not Found**\n\n"
            f"No pending approval found with ID: `{request_id}`"
        )

    # Format detailed information
    short_id = pending.request_id[:8]
    short_session = pending.session_id[:12]

    # Determine risk level
    risk_level = "UNKNOWN"
    if pending.findings_count > 0:
        if pending.findings_count >= 3:
            risk_level = "HIGH 🔴"
        elif pending.findings_count >= 1:
            risk_level = "MEDIUM 🟡"
        else:
            risk_level = "LOW 🟢"

    lines = [
        "**Approval Request Details**\n",
        f"**Request ID:** `{short_id}` (full: `{pending.request_id}`)",
        f"**Tool:** `{pending.tool_name}`",
        f"**Risk Level:** {risk_level}",
        f"**Findings Count:** {pending.findings_count}",
        f"**Session:** `{short_session}...`",
    ]

    if pending.session_id != root_session_id:
        lines.append("**Origin:** Sub-session")
    else:
        lines.append("**Origin:** Main session")

    if pending.result_summary:
        lines.append(
            "\n**Security Findings:**\n" f"{pending.result_summary}",
        )

    lines.append("\n**Commands:**")
    lines.append(f"- `/approval approve {short_id}` - Approve")
    lines.append(f"- `/approval deny {short_id}` - Deny")

    return "\n".join(lines)


async def _find_pending_by_id(
    service: "ApprovalService",
    root_session_id: str,
    request_id: str,
) -> Optional["PendingApproval"]:
    """Find pending approval by exact or prefix match.

    Args:
        service: ApprovalService instance
        root_session_id: Root session ID
        request_id: Full or partial request ID

    Returns:
        PendingApproval if found, None otherwise
    """
    all_pending = await service.list_pending_by_session(root_session_id)

    # Try exact match first
    for pending in all_pending:
        if pending.request_id == request_id:
            return pending

    # Try prefix match
    matches = [p for p in all_pending if p.request_id.startswith(request_id)]

    if len(matches) == 1:
        return matches[0]
    elif len(matches) > 1:
        logger.warning(
            f"Ambiguous request ID prefix: {request_id} "
            f"matches {len(matches)} requests",
        )
        return None

    return None
