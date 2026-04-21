# -*- coding: utf-8 -*-
"""Handler for /approval command.

Manages tool guard approval requests through unified control commands.
"""

from __future__ import annotations

import logging
import time

from .base import BaseControlCommandHandler, ControlContext
from ...approvals import get_approval_service
from ....security.tool_guard.approval import ApprovalDecision

logger = logging.getLogger(__name__)


class ApprovalCommandHandler(BaseControlCommandHandler):
    """Handler for /approval command.

    Features:
    - Approve/deny pending tool executions
    - List all pending approvals
    - Cancel specific approval requests

    Usage:
        /approval approve [request_id]  # Approve specific or queue head
        /approval deny [request_id] [reason]  # Deny with optional reason
        /approval list                  # List all pending approvals
        /approval cancel <request_id>   # Cancel specific request
    """

    command_name = "/approval"

    async def handle(self, context: ControlContext) -> str:
        """Handle /approval command with various actions.

        Args:
            context: Control command context

        Returns:
            Response text (markdown formatted)
        """
        action = context.args.get("action", "approve")

        if action == "approve":
            return await self._handle_approve(context)
        elif action == "deny":
            return await self._handle_deny(context)
        elif action == "list":
            return await self._handle_list(context)
        elif action == "cancel":
            return await self._handle_cancel(context)
        else:
            return self._usage_hint()

    async def _handle_approve(self, context: ControlContext) -> str:
        """Approve a pending tool execution."""
        svc = get_approval_service()
        request_id = context.args.get("request_id")

        # If no request_id provided, get queue head (FIFO)
        if not request_id:
            pending = await svc.get_pending_by_session(context.session_id)
            if pending is None:
                return "❌ **无待审批工具**\n\n" "当前会话没有需要审批的工具调用。"
            request_id = pending.request_id

        # Resolve the Future to unblock waiting agent
        resolved = await svc.resolve_request(
            request_id,
            ApprovalDecision.APPROVED,
        )

        if resolved is None:
            return (
                f"❌ **审批请求不存在**\n\n"
                f"请求 ID: `{request_id[:16]}`\n\n"
                f"可能已被处理或已超时。使用 `/approval list` 查看待审批列表。"
            )

        return (
            f"✅ **工具已批准**\n\n"
            f"- 工具: `{resolved.tool_name}`\n"
            f"- 请求 ID: `{request_id[:16]}`\n"
            f"- 状态: 正在执行..."
        )

    async def _handle_deny(self, context: ControlContext) -> str:
        """Deny a pending tool execution."""
        svc = get_approval_service()
        request_id = context.args.get("request_id")
        reason = context.args.get("reason", "用户拒绝")

        # If no request_id provided, get queue head
        if not request_id:
            pending = await svc.get_pending_by_session(context.session_id)
            if pending is None:
                return "❌ **无待审批工具**\n\n" "当前会话没有需要审批的工具调用。"
            request_id = pending.request_id

        # Resolve the Future to unblock waiting agent
        resolved = await svc.resolve_request(
            request_id,
            ApprovalDecision.DENIED,
        )

        if resolved is None:
            return (
                f"❌ **审批请求不存在**\n\n"
                f"请求 ID: `{request_id[:16]}`\n\n"
                f"可能已被处理或已超时。使用 `/approval list` 查看待审批列表。"
            )

        return (
            f"🚫 **工具已拒绝**\n\n"
            f"- 工具: `{resolved.tool_name}`\n"
            f"- 请求 ID: `{request_id[:16]}`\n"
            f"- 原因: {reason}"
        )

    async def _handle_list(self, context: ControlContext) -> str:
        """List all pending approvals for current session."""
        svc = get_approval_service()
        pending_list = await svc.list_pending_by_session(
            context.session_id,
            include_subagents=True,
        )

        if not pending_list:
            return "✅ **无待审批工具**\n\n" "当前会话所有工具调用已处理完毕。"

        lines = ["📋 **待审批工具列表**\n"]
        for i, pending in enumerate(pending_list, 1):
            elapsed = int(time.time() - pending.created_at)
            severity_emoji = self._severity_emoji(pending.severity)
            lines.append(
                f"{i}. `{pending.tool_name}` "
                f"{severity_emoji} `{pending.severity.upper()}` "
                f"- {elapsed}s 前\n"
                f"   请求 ID: `{pending.request_id[:16]}`\n"
                f"   发现问题: {pending.findings_count} 个\n",
            )

        lines.append(
            "\n💡 **操作提示**\n"
            "- 批准: `/approval approve` 或 `/approval approve <request_id>`\n"
            "- 拒绝: `/approval deny` 或 `/approval deny <request_id>`\n"
            "- 若不指定 request_id，默认操作队首工具（FIFO）",
        )

        return "\n".join(lines)

    async def _handle_cancel(self, context: ControlContext) -> str:
        """Cancel a specific approval request."""
        request_id = context.args.get("request_id")

        if not request_id:
            return (
                "❌ **缺少参数**\n\n"
                "用法: `/approval cancel <request_id>`\n\n"
                "使用 `/approval list` 查看待审批列表及其 ID。"
            )

        svc = get_approval_service()
        resolved = await svc.resolve_request(
            request_id,
            ApprovalDecision.DENIED,
        )

        if resolved is None:
            return f"❌ **审批请求不存在**\n\n" f"请求 ID: `{request_id[:16]}`"

        return (
            f"✅ **审批请求已取消**\n\n"
            f"- 工具: `{resolved.tool_name}`\n"
            f"- 请求 ID: `{request_id[:16]}`"
        )

    @staticmethod
    def _severity_emoji(severity: str) -> str:
        """Get emoji for severity level."""
        severity_lower = severity.lower()
        if severity_lower in ("critical", "high"):
            return "🔴"
        elif severity_lower == "medium":
            return "🟡"
        else:  # low, info
            return "🟢"

    @staticmethod
    def _usage_hint() -> str:
        """Return usage hint for /approval command."""
        return (
            "**使用说明: /approval**\n\n"
            "管理工具审批请求\n\n"
            "**子命令**:\n"
            "- `approve [request_id]` - 批准工具执行\n"
            "- `deny [request_id] [reason]` - 拒绝工具执行\n"
            "- `list` - 列出所有待审批工具\n"
            "- `cancel <request_id>` - 取消指定审批\n\n"
            "**示例**:\n"
            "```\n"
            "/approval approve          # 批准队首工具\n"
            "/approval deny             # 拒绝队首工具\n"
            "/approval list             # 查看待审批列表\n"
            "/approval approve abc123   # 批准指定工具\n"
            "```"
        )
