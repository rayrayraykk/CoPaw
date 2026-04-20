# -*- coding: utf-8 -*-
"""Handler for /approval command.

The /approval command manages pending tool approval requests with
hierarchical subcommands:
- /approval list - List all pending approvals
- /approval approve <id> - Approve specific request
- /approval deny <id> - Deny specific request
- /approval show <id> - Show request details
"""

from __future__ import annotations

import logging

from .base import BaseControlCommandHandler, ControlContext

logger = logging.getLogger(__name__)


class ApprovalCommandHandler(BaseControlCommandHandler):
    """Handler for /approval command.

    Features:
    - Immediate response (Control Command priority)
    - Hierarchical subcommands (list/approve/deny/show)
    - Cross-session approval support (root session lookup)
    - Short ID prefix matching for convenience

    Usage:
        /approval list                 # List all pending approvals
        /approval approve              # Approve oldest (FIFO)
        /approval approve abc123       # Approve by ID (full or prefix)
        /approval deny abc123          # Deny by ID
        /approval show abc123          # Show request details

    Architecture:
        This handler delegates to the unified commands module at
        `qwenpaw.app.approvals.commands` for business logic.
        This follows the separation of concerns:
        - Handler: Command parsing and Control Command integration
        - Commands module: Business logic and ApprovalService interaction
    """

    command_name = "/approval"

    async def handle(self, context: ControlContext) -> str:
        """Handle /approval command.

        Args:
            context: Control command context

        Returns:
            Response text (success or error message)
        """
        from ...approvals.commands import handle_approval_command

        session_id = context.session_id
        raw_args = context.args.get("_raw_args", "")

        # Parse subcommand and arguments
        parts = raw_args.split(maxsplit=1)

        if not parts:
            # No subcommand: show help
            return self._show_help()

        subcommand = parts[0].lower()
        args = parts[1] if len(parts) > 1 else ""

        logger.info(
            f"/approval command: session={session_id[:30]} "
            f"subcommand={subcommand} args={args[:50]}",
        )

        # Delegate to unified command handler
        return await handle_approval_command(session_id, subcommand, args)

    def _show_help(self) -> str:
        """Return help text for /approval command.

        Returns:
            Formatted help text
        """
        return (
            "**Approval Command**\n\n"
            "Manage pending tool approval requests.\n\n"
            "**Usage:**\n"
            "- `/approval list` - List all pending approvals\n"
            "- `/approval approve [<id>]` - Approve request "
            "(oldest if no ID)\n"
            "- `/approval deny <id>` - Deny request\n"
            "- `/approval show <id>` - Show request details\n\n"
            "**ID Format:**\n"
            "You can use full ID or short prefix (e.g. `abc123`).\n\n"
            "**Examples:**\n"
            "```\n"
            "/approval list\n"
            "/approval approve abc123de\n"
            "/approval deny abc123\n"
            "/approval show abc123\n"
            "```"
        )
