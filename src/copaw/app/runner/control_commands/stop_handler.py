# -*- coding: utf-8 -*-
"""Handler for /stop command.

The /stop command immediately terminates an ongoing agent task.
"""

from __future__ import annotations

import logging

from .base import BaseControlCommandHandler, ControlContext

logger = logging.getLogger(__name__)


class StopCommandHandler(BaseControlCommandHandler):
    """Handler for /stop command.

    Features:
    - Immediate response (priority level 0)
    - Stops task for target session
    - Default: stops current session
    - Optional: specify target session_id

    Usage:
        /stop                  # Stop current session
        /stop session=console:user1  # Stop specific session
    """

    command_name = "/stop"

    async def handle(self, context: ControlContext) -> str:
        """Handle /stop command.

        Args:
            context: Control command context

        Returns:
            Response text (success or error message)
        """
        # Get target session ID (default: current session)
        target_session_id = context.args.get(
            "session",
            context.session_id,
        )

        logger.info(
            f"/stop command: current_session={context.session_id[:30]} "
            f"target_session={target_session_id[:30]}",
        )

        # Get chat_id from session_id via chat_manager
        # Note: This requires chat_manager.get_chat_id_by_session
        # which will be added in Day 4
        workspace = context.workspace
        chat_manager = workspace.chat_manager

        # Resolve chat_id
        chat_id = await chat_manager.get_chat_id_by_session(
            target_session_id,
            context.channel.channel,
        )

        if chat_id is None:
            logger.warning(
                f"/stop: No active chat found for session={target_session_id[:30]}",  # noqa: E501
            )
            return (
                f"**Stop Failed**\n\n"
                f"No active task found for session "
                f"`{target_session_id[:40]}`"
            )

        # Request stop via task_tracker
        task_tracker = workspace.task_tracker
        stopped = await task_tracker.request_stop(chat_id)

        if stopped:
            logger.info(
                f"/stop: Successfully stopped task for chat_id={chat_id[:30]}",
            )
            return (
                f"**Task Stopped**\n\n"
                f"Task for session `{target_session_id[:40]}` "
                f"has been terminated."
            )
        else:
            logger.warning(
                f"/stop: Task not running for chat_id={chat_id[:30]}",
            )
            return (
                f"**No Active Task**\n\n"
                f"No running task found for session "
                f"`{target_session_id[:40]}`"
            )
