# -*- coding: utf-8 -*-
# pylint: disable=redefined-outer-name,protected-access
"""Tests for ExternalAgentWorkspace."""
from __future__ import annotations

from unittest.mock import MagicMock, patch

import pytest

from qwenpaw.app.workspace.external_agent_workspace import (
    ExternalAgentWorkspace,
    _synthetic_event,
)
from qwenpaw.config.config import ACPAgentConfig
from qwenpaw.schemas import AgentRequest, Message, Role


# ── Fixtures ──


@pytest.fixture()
def tmp_workspace(tmp_path):
    """Return a temporary workspace directory."""
    ws = tmp_path / "test_ws"
    ws.mkdir()
    return ws


@pytest.fixture()
def acp_config():
    """Return a minimal ACPAgentConfig."""
    return ACPAgentConfig(
        enabled=True,
        command="echo",
        args=["hello"],
        trusted=True,
    )


@pytest.fixture()
def workspace(tmp_workspace, acp_config):
    """Create ExternalAgentWorkspace with mocked config."""
    ws = ExternalAgentWorkspace(
        agent_id="test_ext",
        workspace_dir=str(tmp_workspace),
    )
    ws._acp_config = acp_config
    return ws


# ── Unit tests ──


class TestInit:
    """Verify constructor initializes state correctly."""

    def test_agent_id(self, workspace):
        assert workspace.agent_id == "test_ext"

    def test_workspace_dir_exists(self, workspace):
        assert workspace.workspace_dir.exists()

    def test_not_started(self, workspace):
        assert not workspace._started

    def test_duck_type_properties(self, workspace):
        assert workspace.memory_manager is None
        assert workspace.driver_manager is None
        assert workspace.cron_manager is None
        assert workspace.local_workspace is None

    def test_repr(self, workspace):
        r = repr(workspace)
        assert "test_ext" in r
        assert "stopped" in r


class TestBootstrapNoOp:
    """bootstrap_plugins should be a no-op."""

    def test_no_error(self, workspace):
        workspace.bootstrap_plugins(
            builtin_tool_funcs=[],
        )


class TestSetManager:
    """set_manager and set_app_services store refs."""

    def test_set_manager(self, workspace):
        mgr = MagicMock()
        workspace.set_manager(mgr)
        assert workspace._manager is mgr

    def test_set_app_services(self, workspace):
        svc = MagicMock()
        workspace.set_app_services(svc)
        assert workspace._app_services is svc


class TestNormalizeRequest:
    """_normalize_request handles various inputs."""

    def test_dict_input(self):
        req = ExternalAgentWorkspace._normalize_request(
            {"input": []},
        )
        assert isinstance(req, AgentRequest)

    def test_agent_request_passthrough(self):
        original = AgentRequest(input=[])
        result = ExternalAgentWorkspace._normalize_request(
            original,
        )
        assert result is original


class TestExtractPromptText:
    """_extract_prompt_text extracts text from messages."""

    def test_text_content(self):
        msg = Message(
            role=Role.USER,
            content=[
                {"type": "text", "text": "hello"},
            ],
        )
        req = AgentRequest(input=[msg])
        text = ExternalAgentWorkspace._extract_prompt_text(
            req,
        )
        assert "hello" in text

    def test_empty_request(self):
        req = AgentRequest(input=[])
        text = ExternalAgentWorkspace._extract_prompt_text(
            req,
        )
        assert text == ""


class TestSyntheticEvent:
    """_synthetic_event builds correct SimpleNamespace."""

    def test_type_set(self):
        evt = _synthetic_event(
            "TEXT_BLOCK_START",
            block_id="b1",
        )
        assert evt.type == "TEXT_BLOCK_START"
        assert evt.block_id == "b1"

    def test_delta(self):
        evt = _synthetic_event(
            "TEXT_BLOCK_DELTA",
            block_id="b1",
            delta="hi",
        )
        assert evt.delta == "hi"


class TestToSyntheticEvents:
    """_to_synthetic_events translates client events."""

    def test_text_event(self, workspace):
        events = workspace._to_synthetic_events(
            {"type": "text", "text": "hello"},
        )
        assert len(events) == 3
        assert events[0].type == "TEXT_BLOCK_START"
        assert events[1].type == "TEXT_BLOCK_DELTA"
        assert events[1].delta == "hello"
        assert events[2].type == "TEXT_BLOCK_END"

    def test_empty_text(self, workspace):
        events = workspace._to_synthetic_events(
            {"type": "text", "text": ""},
        )
        assert events == []

    def test_tool_event(self, workspace):
        events = workspace._to_synthetic_events(
            {
                "type": "tool_start",
                "name": "read_file",
                "detail": "/tmp/f.txt",
            },
        )
        assert len(events) == 3
        assert "[read_file]" in events[1].delta

    def test_status_event(self, workspace):
        events = workspace._to_synthetic_events(
            {
                "type": "status",
                "status": "ok",
                "summary": "done",
            },
        )
        assert events == []

    def test_unknown_event(self, workspace):
        events = workspace._to_synthetic_events(
            {"type": "unknown_type"},
        )
        assert events == []


class TestResolveACPConfig:
    """_resolve_acp_config loads from agent or root."""

    def test_from_agent_config(
        self,
        workspace,
        acp_config,
    ):
        from qwenpaw.config.config import ACPConfig

        workspace._config = MagicMock()
        workspace._config.acp = ACPConfig(
            agents={"test_ext": acp_config},
        )
        result = workspace._resolve_acp_config()
        assert result.command == "echo"

    def test_missing_raises(self, workspace):
        workspace._config = MagicMock()
        workspace._config.acp = None

        with patch(
            "qwenpaw.config.utils.load_config",
        ) as mock_load:
            mock_cfg = MagicMock()
            mock_cfg.acp = None
            mock_load.return_value = mock_cfg
            with pytest.raises(
                ValueError,
                match="no enabled",
            ):
                workspace._resolve_acp_config()


class TestAutoAuthenticate:
    """_auto_authenticate handles ACP auth methods."""

    @pytest.mark.asyncio()
    async def test_no_auth_methods(self, workspace):
        init_resp = MagicMock()
        init_resp.auth_methods = None
        workspace._conn = MagicMock()
        await workspace._auto_authenticate(init_resp)
        workspace._conn.authenticate.assert_not_called()

    @pytest.mark.asyncio()
    async def test_empty_auth_methods(self, workspace):
        init_resp = MagicMock()
        init_resp.auth_methods = []
        workspace._conn = MagicMock()
        await workspace._auto_authenticate(init_resp)
        workspace._conn.authenticate.assert_not_called()

    @pytest.mark.asyncio()
    async def test_env_var_auth_succeeds(
        self,
        workspace,
    ):
        env_method = MagicMock()
        env_method.type = "env_var"
        env_method.id = "openai-api-key"
        env_method.name = "Use OPENAI_API_KEY"
        env_var = MagicMock()
        env_var.name = "TEST_ACP_KEY"
        env_var.optional = False
        env_method.vars = [env_var]

        init_resp = MagicMock()
        init_resp.auth_methods = [env_method]

        workspace._conn = MagicMock()
        workspace._conn.authenticate = MagicMock(
            return_value=MagicMock(),
        )

        import os

        os.environ["TEST_ACP_KEY"] = "fake-key"
        try:
            await workspace._auto_authenticate(
                init_resp,
            )
            workspace._conn.authenticate.assert_called_once_with(
                method_id="openai-api-key",
            )
        finally:
            del os.environ["TEST_ACP_KEY"]

    @pytest.mark.asyncio()
    async def test_env_var_missing_skips(
        self,
        workspace,
    ):
        env_method = MagicMock()
        env_method.type = "env_var"
        env_method.id = "test-key"
        env_method.name = "Test Key"
        env_var = MagicMock()
        env_var.name = "NONEXISTENT_ACP_KEY_XYZ"
        env_var.optional = False
        env_method.vars = [env_var]

        init_resp = MagicMock()
        init_resp.auth_methods = [env_method]

        workspace._conn = MagicMock()
        await workspace._auto_authenticate(init_resp)
        workspace._conn.authenticate.assert_not_called()

    @pytest.mark.asyncio()
    async def test_agent_auth_method_skipped(
        self,
        workspace,
    ):
        agent_method = MagicMock()
        agent_method.type = None
        agent_method.id = "chatgpt"
        agent_method.name = "Login with ChatGPT"

        init_resp = MagicMock()
        init_resp.auth_methods = [agent_method]

        workspace._conn = MagicMock()
        await workspace._auto_authenticate(init_resp)
        workspace._conn.authenticate.assert_not_called()


class TestStopIdempotent:
    """stop() should be safe when not started."""

    @pytest.mark.asyncio()
    async def test_stop_not_started(self, workspace):
        await workspace.stop()


class TestSetReusableComponents:
    """set_reusable_components should be a no-op."""

    @pytest.mark.asyncio()
    async def test_no_op(self, workspace):
        await workspace.set_reusable_components(
            {"memory_manager": MagicMock()},
        )
