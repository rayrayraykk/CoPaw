# -*- coding: utf-8 -*-
"""Tests for Codex-gated one-click setup."""

from __future__ import annotations

from types import SimpleNamespace
from unittest.mock import AsyncMock, Mock

from fastapi import FastAPI
from fastapi.testclient import TestClient

from dingtalk_desktop import router as router_module
from dingtalk_desktop.models import DesktopStatus


def make_client(monkeypatch, backend="codex", authenticated=True):
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
        workspace_dir=SimpleNamespace(),
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
    return TestClient(app), config, saved


def test_setup_rejects_non_codex_agent(monkeypatch):
    """A QwenPaw-native agent cannot silently change backend."""
    client, _, saved = make_client(monkeypatch, backend="qwenpaw")

    response = client.post("/setup", json={"reply_mode": "draft"})

    assert response.status_code == 409
    saved.assert_not_called()


def test_setup_requires_codex_oauth(monkeypatch):
    """The plugin delegates authentication to the existing Codex adapter."""
    client, _, saved = make_client(monkeypatch, authenticated=False)

    response = client.post("/setup", json={"reply_mode": "draft"})

    assert response.status_code == 401
    saved.assert_not_called()


def test_setup_persists_exact_current_conversation(monkeypatch):
    """One-click setup stores a structured one-title allowlist."""
    client, config, saved = make_client(monkeypatch)

    response = client.post("/setup", json={"reply_mode": "draft"})

    assert response.status_code == 200
    channel = config.channels.dingtalk_desktop
    assert channel["allowed_conversations"] == ["Exact conversation"]
    assert channel["reply_mode"] == "draft"
    saved.assert_called_once_with("agent", config)
