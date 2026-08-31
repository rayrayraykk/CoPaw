# -*- coding: utf-8 -*-
"""Agent-scoped setup and draft approval routes."""

from __future__ import annotations

import asyncio
from pathlib import Path
from typing import Literal

from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel

from qwenpaw.app.agent_context import get_agent_for_request
from qwenpaw.app.channels.access_control import (
    AccessControlStore,
    get_access_control_store,
)
from qwenpaw.app.utils import schedule_agent_reload
from qwenpaw.config.config import ChannelConfig
from qwenpaw.config.config import load_agent_config, save_agent_config

from .driver import DingTalkDesktopDriver, DingTalkDesktopError
from .state import DraftStore, draft_store_path


class SetupRequest(BaseModel):
    """Safe one-click setup options."""

    reply_mode: Literal["draft", "automatic"] = "draft"


def _access_store(workspace: object) -> AccessControlStore:
    """Return the existing per-agent channel access-control store."""
    workspace_dir = Path(getattr(workspace, "workspace_dir"))
    return get_access_control_store(workspace_dir)


def _access_summary(workspace: object) -> dict[str, int]:
    """Return counts without exposing conversation titles."""
    access_control = _access_store(workspace).get_acl("dingtalk_desktop")
    return {
        "whitelist_count": len(access_control["whitelist"]),
        "blacklist_count": len(access_control["blacklist"]),
        "pending_count": len(access_control["pending"]),
    }


def _authorize_conversation(workspace: object, conversation: str) -> None:
    """Authorize setup through the shared channel whitelist."""
    _access_store(workspace).add_to_whitelist(
        channel="dingtalk_desktop",
        user_id=conversation,
        username=conversation,
        remark="Authorized during desktop setup",
    )


def _config_value(config: object, name: str, default: object) -> object:
    """Read one field from a typed or plugin-owned channel config."""
    if isinstance(config, dict):
        return config.get(name, default)
    return getattr(config, name, default)


def _driver_for_config(config: object) -> DingTalkDesktopDriver:
    channels = getattr(config, "channels", None)
    plugin_config = getattr(channels, "dingtalk_desktop", None)
    bundle_id = getattr(
        plugin_config,
        "bundle_id",
        "dd.work.exclusive4aliding",
    )
    if isinstance(plugin_config, dict):
        bundle_id = plugin_config.get("bundle_id", bundle_id)
    return DingTalkDesktopDriver(bundle_id=str(bundle_id))


async def _ready_conversation(
    config: object,
) -> tuple[DingTalkDesktopDriver, str]:
    """Require a ready desktop and return its current conversation."""
    driver = _driver_for_config(config)
    desktop = await asyncio.to_thread(driver.status)
    if not desktop.logged_in or not desktop.accessibility:
        raise HTTPException(
            status_code=409,
            detail=(
                "Open and sign in to DingTalk, then grant macOS "
                "Accessibility access."
            ),
        )
    conversation = await asyncio.to_thread(driver.current_conversation)
    if not conversation:
        raise HTTPException(
            status_code=409,
            detail="Open the DingTalk conversation to bind.",
        )
    return driver, conversation


def build_router() -> APIRouter:
    """Build routes mounted under the plugin-owned API prefix."""
    router = APIRouter()

    @router.get("/status")
    async def status(request: Request) -> dict:
        workspace = await get_agent_for_request(request)
        config = load_agent_config(workspace.agent_id)
        driver = _driver_for_config(config)
        desktop = await asyncio.to_thread(driver.status)
        plugin_config = getattr(
            config.channels,
            "dingtalk_desktop",
            None,
        )
        enabled = bool(_config_value(plugin_config, "enabled", False))
        access_control_dm = bool(
            _config_value(plugin_config, "access_control_dm", False),
        )
        drafts = DraftStore(draft_store_path(workspace.workspace_dir))
        access_control = _access_summary(workspace)
        return {
            "agent_id": workspace.agent_id,
            "backend": config.backend,
            "configured": bool(
                enabled
                and access_control_dm
                and access_control["whitelist_count"] > 0,
            ),
            "desktop": desktop.as_dict(),
            "draft_count": len(drafts.list()),
            "access_control": access_control,
        }

    @router.post("/setup")
    async def setup(body: SetupRequest, request: Request) -> dict:
        workspace = await get_agent_for_request(request)
        config = load_agent_config(workspace.agent_id)
        driver, conversation = await _ready_conversation(config)
        if config.channels is None:
            config.channels = ChannelConfig()
        config.channels.dingtalk_desktop = {
            "enabled": True,
            "reply_mode": body.reply_mode,
            "access_control_dm": True,
            "poll_sec": 1.0,
            "bundle_id": driver.bundle_id,
            "context_messages": 16,
        }
        save_agent_config(workspace.agent_id, config)
        _authorize_conversation(workspace, conversation)
        schedule_agent_reload(request, workspace.agent_id)
        return {
            "configured": True,
            "agent_id": workspace.agent_id,
            "conversation": conversation,
            "reply_mode": body.reply_mode,
        }

    @router.get("/drafts")
    async def list_drafts(request: Request) -> dict:
        workspace = await get_agent_for_request(request)
        store = DraftStore(draft_store_path(workspace.workspace_dir))
        return {"drafts": [item.as_dict() for item in store.list()]}

    @router.post("/drafts/{draft_id}/send")
    async def send_draft(draft_id: str, request: Request) -> dict:
        workspace = await get_agent_for_request(request)
        config = load_agent_config(workspace.agent_id)
        store = DraftStore(draft_store_path(workspace.workspace_dir))
        draft = store.get(draft_id)
        if draft is None:
            raise HTTPException(status_code=404, detail="Draft not found.")
        if not _access_store(workspace).is_whitelisted(
            "dingtalk_desktop",
            draft.conversation,
        ):
            raise HTTPException(
                status_code=403,
                detail="The conversation is not approved for this channel.",
            )
        driver = _driver_for_config(config)
        try:
            await asyncio.to_thread(
                driver.send,
                draft.conversation,
                draft.text,
            )
        except DingTalkDesktopError as exc:
            raise HTTPException(status_code=409, detail=str(exc)) from exc
        store.remove(draft_id)
        return {"sent": True}

    @router.delete("/drafts/{draft_id}")
    async def delete_draft(draft_id: str, request: Request) -> dict:
        workspace = await get_agent_for_request(request)
        store = DraftStore(draft_store_path(workspace.workspace_dir))
        if not store.remove(draft_id):
            raise HTTPException(status_code=404, detail="Draft not found.")
        return {"deleted": True}

    return router


__all__ = ["SetupRequest", "build_router"]
