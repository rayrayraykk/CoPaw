# -*- coding: utf-8 -*-
"""Unit tests for profiles and gates routers."""
# pylint: disable=redefined-outer-name,unused-argument,protected-access
from __future__ import annotations

from unittest.mock import MagicMock, patch

import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient

from qwenpaw.app.routers.profiles import (
    router as profiles_router,
    _sync_legacy_loop_config,
)
from qwenpaw.app.routers.gates import (
    router as gates_router,
)
from qwenpaw.config.config import (
    AgentsRunningConfig,
)
from qwenpaw.loop.gate_catalog import GateCatalog


@pytest.fixture(autouse=True)
def _reset_catalog():
    GateCatalog._reset()
    yield
    GateCatalog._reset()


def _make_workspace():
    ws = MagicMock()
    ws.agent_id = "test-agent"
    return ws


def _make_agent_config(
    profiles=None,
):
    from qwenpaw.config.config import (
        AgentProfileConfig,
    )

    rc = AgentsRunningConfig()
    if profiles:
        rc.loop.profiles = profiles
    cfg = MagicMock(spec=AgentProfileConfig)
    cfg.running = rc
    return cfg


def _build_app(mock_workspace):
    """Create test FastAPI app with mocked deps."""
    app = FastAPI()
    app.include_router(
        profiles_router,
        prefix="/api",
    )
    app.include_router(
        gates_router,
        prefix="/api",
    )
    return app


@pytest.fixture
def mock_workspace():
    return _make_workspace()


@pytest.fixture
def mock_config():
    return _make_agent_config()


_MOD = "qwenpaw.app.routers.profiles"


@pytest.fixture
def client(mock_workspace, mock_config):
    app = _build_app(mock_workspace)

    async def fake_get_agent(request):
        return mock_workspace

    with (
        patch(
            f"{_MOD}.get_agent_for_request",
            side_effect=fake_get_agent,
        ),
        patch(
            f"{_MOD}.load_agent_config",
            return_value=mock_config,
        ),
        patch(
            f"{_MOD}.save_agent_config",
        ),
        patch(
            f"{_MOD}.schedule_agent_reload",
        ),
    ):
        yield TestClient(app)


class TestGatesCatalog:
    """GET /api/gates/catalog."""

    def test_returns_gates(self, client):
        resp = client.get("/api/gates/catalog")
        assert resp.status_code == 200
        data = resp.json()
        assert "gates" in data
        gates = data["gates"]
        types = {g["type"] for g in gates}
        assert "iteration" in types
        assert "doom_loop" in types

    def test_no_factory_in_response(self, client):
        resp = client.get("/api/gates/catalog")
        for gate in resp.json()["gates"]:
            assert "factory" not in gate


class TestListProfiles:
    """GET /api/loops/profiles."""

    def test_returns_builtin_profiles(self, client):
        resp = client.get("/api/loops/profiles")
        assert resp.status_code == 200
        data = resp.json()
        names = {p["name"] for p in data}
        assert "default" in names
        assert "goal" in names
        assert "mission" in names

    def test_builtin_marked_correctly(self, client):
        resp = client.get("/api/loops/profiles")
        for p in resp.json():
            if p["name"] in (
                "default",
                "goal",
                "mission",
            ):
                assert p["is_builtin"] is True


class TestCreateProfile:
    """POST /api/loops/profiles."""

    def test_create_custom_profile(self, client):
        resp = client.post(
            "/api/loops/profiles",
            json={
                "name": "my_custom",
                "description": "A test profile",
                "gates": [],
            },
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["status"] == "ok"
        assert data["profile"] == "my_custom"

    def test_reject_builtin_name(self, client):
        resp = client.post(
            "/api/loops/profiles",
            json={
                "name": "default",
                "description": "Trying builtin",
                "gates": [],
            },
        )
        assert resp.status_code == 400
        assert "reserved" in resp.json()["detail"]

    def test_reject_unknown_gate_type(self, client):
        resp = client.post(
            "/api/loops/profiles",
            json={
                "name": "bad_gates",
                "description": "",
                "gates": [
                    {
                        "type": "nonexistent_gate",
                        "enabled": True,
                        "priority": 10,
                        "params": {},
                    },
                ],
            },
        )
        assert resp.status_code == 400
        assert "Unknown" in resp.json()["detail"]


class TestUpdateProfile:
    """PUT /api/loops/profiles/{name}."""

    def test_update_builtin_valid(self, client):
        resp = client.put(
            "/api/loops/profiles/default",
            json={
                "gates": [
                    {
                        "type": "iteration",
                        "enabled": True,
                        "priority": 10,
                        "params": {
                            "max_iterations": 50,
                        },
                    },
                ],
            },
        )
        assert resp.status_code == 200

    def test_update_builtin_reject_invalid_gate(
        self,
        client,
    ):
        resp = client.put(
            "/api/loops/profiles/default",
            json={
                "gates": [
                    {
                        "type": "nonexistent",
                        "enabled": True,
                        "priority": 10,
                        "params": {},
                    },
                ],
            },
        )
        assert resp.status_code == 400


class TestDeleteProfile:
    """DELETE /api/loops/profiles/{name}."""

    def test_reject_delete_builtin(self, client):
        resp = client.delete(
            "/api/loops/profiles/default",
        )
        assert resp.status_code == 400
        assert "builtin" in resp.json()["detail"].lower()

    def test_delete_custom(self, client):
        resp = client.delete(
            "/api/loops/profiles/my_custom",
        )
        assert resp.status_code == 200
        assert resp.json()["deleted"] == "my_custom"


class TestSyncLegacyLoopConfig:
    """Test _sync_legacy_loop_config correctness."""

    def test_sync_iteration(self):
        rc = AgentsRunningConfig()
        overrides = {
            "iteration": {
                "enabled": False,
                "params": {"max_iterations": 42},
            },
        }
        _sync_legacy_loop_config(rc, overrides)
        assert rc.loop.iteration.enabled is False
        assert rc.loop.iteration.max_iterations == 42

    def test_sync_doom_loop(self):
        rc = AgentsRunningConfig()
        overrides = {
            "doom_loop": {
                "enabled": True,
                "params": {
                    "window_size": 5,
                    "similarity_threshold": 0.8,
                },
            },
        }
        _sync_legacy_loop_config(rc, overrides)
        assert rc.loop.doom_loop.enabled is True
        assert rc.loop.doom_loop.window_size == 5
        assert rc.loop.doom_loop.similarity_threshold == 0.8

    def test_sync_rubric(self):
        rc = AgentsRunningConfig()
        overrides = {
            "rubric": {
                "enabled": True,
                "params": {
                    "prompt": "Check this",
                    "max_interventions": 3,
                },
            },
        }
        _sync_legacy_loop_config(rc, overrides)
        assert rc.loop.rubric.enabled is True
        assert rc.loop.rubric.prompt == "Check this"
        assert rc.loop.rubric.max_interventions == 3

    def test_sync_partial_params(self):
        rc = AgentsRunningConfig()
        original_max = rc.loop.iteration.max_iterations
        overrides = {
            "iteration": {
                "enabled": True,
                "params": {},
            },
        }
        _sync_legacy_loop_config(rc, overrides)
        assert rc.loop.iteration.max_iterations == original_max

    def test_sync_empty_overrides(self):
        rc = AgentsRunningConfig()
        _sync_legacy_loop_config(rc, {})
        assert rc.loop.iteration.enabled is True
