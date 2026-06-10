# -*- coding: utf-8 -*-
"""Service factory functions for workspace components.

Factory functions are used by Workspace._register_services() to create
and initialize service components. Extracted from local functions to
improve testability and code organization.
"""

from typing import TYPE_CHECKING
import logging

if TYPE_CHECKING:
    from .workspace import Workspace

logger = logging.getLogger(__name__)


async def create_driver_service(ws: "Workspace", _service):
    """Create and initialize the per-workspace DriverManager.

    Args:
        ws: Workspace instance
        _service: Unused service slot from ServiceDescriptor.post_init

    Returns:
        DriverManager instance
    """
    # pylint: disable=protected-access

    from ...drivers.credentials.store import CredentialStore
    from ...drivers.handlers import (
        A2ADriverHandler,
        ACPDriverHandler,
        MCPDriverHandler,
    )
    from ...drivers.handlers.mcp import validate_mcp_endpoint
    from ...drivers.manager import DriverManager
    from ..approvals.driver_gate import QwenPawDriverApprovalGate

    credential_store = CredentialStore(ws.workspace_dir / "credentials.yaml")
    driver_manager = DriverManager(
        ws.workspace_dir / "drivers",
        credential_store,
        approval_gate=QwenPawDriverApprovalGate(),
    )
    driver_manager.register_handler_type(
        "mcp",
        MCPDriverHandler,
        endpoint_validator=validate_mcp_endpoint,
    )
    driver_manager.register_handler_type("acp", ACPDriverHandler)
    driver_manager.register_handler_type("a2a", A2ADriverHandler)
    from ...drivers.adapters.mcp_legacy_config import (
        migrate_legacy_mcp_if_needed,
    )

    await migrate_legacy_mcp_if_needed(ws, driver_manager)
    await driver_manager.start()
    ws._service_manager.services["driver_manager"] = driver_manager
    runner = ws._service_manager.services.get("runner")
    if runner is not None:
        runner.set_driver_manager(driver_manager)
    logger.debug("DriverManager initialized for agent: %s", ws.agent_id)
    return driver_manager


async def create_chat_service(ws: "Workspace", service):
    """Create and attach chat manager, or reuse existing one.

    Args:
        ws: Workspace instance
        service: Existing ChatManager if reused, None if creating new
    """
    # pylint: disable=protected-access
    from ..runner.manager import ChatManager
    from ..runner.repo.json_repo import JsonChatRepository

    if service is not None:
        # Reused ChatManager - just wire to new runner
        cm = service
        logger.info(f"Reusing ChatManager for {ws.agent_id}")
    else:
        # Create new ChatManager
        chats_path = str(ws.workspace_dir / "chats.json")
        chat_repo = JsonChatRepository(chats_path)
        cm = ChatManager(repo=chat_repo)
        ws._service_manager.services["chat_manager"] = cm
        logger.info(f"ChatManager created: {chats_path}")

    # Always wire to new runner
    ws._service_manager.services["runner"].set_chat_manager(cm)
    # pylint: enable=protected-access


async def create_channel_service(ws: "Workspace", _):
    """Create channel manager if configured.

    Args:
        ws: Workspace instance
        _: Unused service parameter

    Returns:
        ChannelManager instance or None if not configured
    """
    # pylint: disable=protected-access
    if not ws._config.channels:
        return None

    from ...config import Config, update_last_dispatch
    from ..channels.manager import ChannelManager
    from ..channels.utils import make_process_from_runner
    from ..channels.access_control import init_access_control_store

    # Initialise the access-control store for this workspace so that
    # each workspace maintains its own access_control.json.
    init_access_control_store(ws.workspace_dir)

    temp_config = Config(channels=ws._config.channels)
    runner = ws._service_manager.services["runner"]

    def on_last_dispatch(channel, user_id, session_id):
        update_last_dispatch(
            channel=channel,
            user_id=user_id,
            session_id=session_id,
            agent_id=ws.agent_id,
        )

    cm = ChannelManager.from_config(
        process=make_process_from_runner(runner),
        config=temp_config,
        on_last_dispatch=on_last_dispatch,
        workspace_dir=ws.workspace_dir,
    )
    ws._service_manager.services["channel_manager"] = cm

    # Inject workspace into ChannelManager and all channels
    cm.set_workspace(ws)

    # Propagate agent language to channels for i18n deny messages
    agent_language = getattr(ws._config, "language", "zh") or "zh"
    for ch in cm.channels:
        ch._language = agent_language

    # Inject workspace into runner for control command handlers
    runner.set_workspace(ws)

    return cm
    # pylint: enable=protected-access


async def create_agent_config_watcher(ws: "Workspace", _):
    """Create agent config watcher if channel/cron exists.

    The watcher only triggers reloads via ``MultiAgentManager`` and
    does not need direct references to channel/cron managers anymore.
    Creation is still gated on having at least one of them, since
    workspaces with neither have no externally-visible state that
    benefits from auto-reload.

    Args:
        ws: Workspace instance
        _: Unused service parameter

    Returns:
        AgentConfigWatcher instance or None if not needed
    """
    # pylint: disable=protected-access
    channel_mgr = ws._service_manager.services.get("channel_manager")
    cron_mgr = ws._service_manager.services.get("cron_manager")

    if not (channel_mgr or cron_mgr):
        return None

    from ..agent_config_watcher import AgentConfigWatcher

    watcher = AgentConfigWatcher(
        agent_id=ws.agent_id,
        workspace_dir=ws.workspace_dir,
        workspace=ws,
    )
    ws._service_manager.services["agent_config_watcher"] = watcher
    return watcher
    # pylint: enable=protected-access
