# -*- coding: utf-8 -*-
"""Agent-scoped setup and draft approval routes."""

from __future__ import annotations

import asyncio
from typing import Literal

from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel

from qwenpaw.app.agent_context import get_agent_for_request
from qwenpaw.app.utils import schedule_agent_reload
from qwenpaw.config.config import ChannelConfig
from qwenpaw.config.config import load_agent_config, save_agent_config

from .driver import DingTalkDesktopDriver, DingTalkDesktopError
from .state import DraftStore, draft_store_path


class SetupRequest(BaseModel):
    """Safe one-click setup options."""

    reply_mode: Literal["draft", "automatic"] = "draft"


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


async def _codex_status(workspace: object) -> dict:
    config = workspace.config
    settings = (
        dict(config.backend_settings) if config.backend == "codex" else {}
    )
    adapter = await workspace.harness_runtime.adapter("codex", settings)
    status = await adapter.status()
    return status.model_dump()


def build_router() -> APIRouter:
    """Build routes mounted under the plugin-owned API prefix."""
    router = APIRouter()

    @router.get("/status")
    async def status(request: Request) -> dict:
        workspace = await get_agent_for_request(request)
        config = load_agent_config(workspace.agent_id)
        driver = _driver_for_config(config)
        desktop, codex = await asyncio.gather(
            asyncio.to_thread(driver.status),
            _codex_status(workspace),
        )
        plugin_config = getattr(
            config.channels,
            "dingtalk_desktop",
            None,
        )
        enabled = bool(getattr(plugin_config, "enabled", False))
        if isinstance(plugin_config, dict):
            enabled = bool(plugin_config.get("enabled", False))
        drafts = DraftStore(draft_store_path(workspace.workspace_dir))
        return {
            "agent_id": workspace.agent_id,
            "backend": config.backend,
            "configured": enabled,
            "desktop": desktop.as_dict(),
            "codex": codex,
            "draft_count": len(drafts.list()),
        }

    @router.post("/setup")
    async def setup(body: SetupRequest, request: Request) -> dict:
        workspace = await get_agent_for_request(request)
        config = load_agent_config(workspace.agent_id)
        if config.backend != "codex":
            raise HTTPException(
                status_code=409,
                detail="Select a Codex-backed agent before setup.",
            )
        codex = await _codex_status(workspace)
        if not codex.get("installed"):
            raise HTTPException(
                status_code=409,
                detail="The Codex runtime is not installed.",
            )
        if not codex.get("authenticated"):
            raise HTTPException(
                status_code=401,
                detail="Complete Codex ChatGPT OAuth before setup.",
            )
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
        conversation = await asyncio.to_thread(
            driver.current_conversation,
        )
        if not conversation:
            raise HTTPException(
                status_code=409,
                detail="Open the DingTalk conversation to bind.",
            )
        if config.channels is None:
            config.channels = ChannelConfig()
        config.channels.dingtalk_desktop = {
            "enabled": True,
            "reply_mode": body.reply_mode,
            "allowed_conversations": [conversation],
            "poll_sec": 1.0,
            "bundle_id": driver.bundle_id,
            "context_messages": 16,
        }
        save_agent_config(workspace.agent_id, config)
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
