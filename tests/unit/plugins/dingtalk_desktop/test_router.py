# -*- coding: utf-8 -*-
"""Tests for Codex-gated one-click setup."""

from __future__ import annotations

from types import SimpleNamespace
from unittest.mock import AsyncMock, Mock

from fastapi import FastAPI
from fastapi.testclient import TestClient

from dingtalk_desktop import router as router_module
from dingtalk_desktop.models import DesktopStatus
from dingtalk_desktop.state import DraftStore, draft_store_path


def make_client(
    monkeypatch,
    tmp_path,
    backend="codex",
    authenticated=True,
):
    """Build a setup API with isolated workspace and desktop doubles."""
    config = SimpleNamespace(
        backend=backend,
        backend_settings={},
        channels=None,
    )
    provider_status = SimpleNamespace(
        model_dump=lambda: {
            "installed": True,
            "authenticated": authenticated,
        },
    )
    adapter = SimpleNamespace(status=AsyncMock(return_value=provider_status))
    runtime = SimpleNamespace(adapter=AsyncMock(return_value=adapter))
    workspace = SimpleNamespace(
        agent_id="agent",
        workspace_dir=tmp_path,
        config=config,
        harness_runtime=runtime,
    )
    driver = SimpleNamespace(
        bundle_id="dd.work.exclusive4aliding",
        status=Mock(
            return_value=DesktopStatus(
                supported=True,
                installed=True,
                running=True,
                accessibility=True,
                logged_in=True,
                bundle_id="dd.work.exclusive4aliding",
            ),
        ),
        current_conversation=Mock(return_value="Exact conversation"),
        send=Mock(),
    )

    async def get_workspace(_request):
        return workspace

    saved = Mock()
    monkeypatch.setattr(
        router_module,
        "get_agent_for_request",
        get_workspace,
    )
    monkeypatch.setattr(router_module, "load_agent_config", lambda _: config)
    monkeypatch.setattr(router_module, "save_agent_config", saved)
    monkeypatch.setattr(router_module, "schedule_agent_reload", Mock())
    monkeypatch.setattr(
        router_module,
        "_driver_for_config",
        lambda _: driver,
    )
    app = FastAPI()
    app.include_router(router_module.build_router())
    return TestClient(app), config, saved, driver


def test_setup_rejects_non_codex_agent(monkeypatch, tmp_path):
    """A QwenPaw-native agent cannot silently change backend."""
    client, _, saved, _ = make_client(
        monkeypatch,
        tmp_path,
        backend="qwenpaw",
    )

    response = client.post("/setup", json={"reply_mode": "draft"})

    assert response.status_code == 409
    saved.assert_not_called()


def test_setup_requires_codex_oauth(monkeypatch, tmp_path):
    """The plugin delegates authentication to the existing Codex adapter."""
    client, _, saved, _ = make_client(
        monkeypatch,
        tmp_path,
        authenticated=False,
    )

    response = client.post("/setup", json={"reply_mode": "draft"})

    assert response.status_code == 401
    saved.assert_not_called()


def test_setup_authorizes_current_conversation_in_shared_acl(
    monkeypatch,
    tmp_path,
):
    """One-click setup uses the existing channel access-control store."""
    client, config, saved, _ = make_client(monkeypatch, tmp_path)

    response = client.post("/setup", json={"reply_mode": "draft"})

    assert response.status_code == 200
    channel = config.channels.dingtalk_desktop
    assert "allowed_conversations" not in channel
    assert channel["access_control_dm"] is True
    assert channel["reply_mode"] == "draft"
    access_store = router_module.get_access_control_store(tmp_path)
    assert access_store.is_whitelisted(
        "dingtalk_desktop",
        "Exact conversation",
    )
    saved.assert_called_once_with("agent", config)


def test_draft_send_rechecks_shared_acl(monkeypatch, tmp_path):
    """A revoked or unknown conversation cannot send an old draft."""
    client, _, _, driver = make_client(monkeypatch, tmp_path)
    draft_store = DraftStore(draft_store_path(tmp_path))
    draft = draft_store.add("Unapproved conversation", "reply")

    response = client.post(f"/drafts/{draft.id}/send")

    assert response.status_code == 403
    driver.send.assert_not_called()
    assert draft_store.get(draft.id) is not None
