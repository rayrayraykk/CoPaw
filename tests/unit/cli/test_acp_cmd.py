# -*- coding: utf-8 -*-
"""Tests for the ``qwenpaw acp`` CLI command."""
from __future__ import annotations

from click.testing import CliRunner

from qwenpaw.cli.acp_cmd import acp_cmd


def test_acp_cmd_passes_local_diagnostics(monkeypatch, tmp_path):
    captured = {}

    async def fake_run_qwenpaw_agent(**kwargs):
        captured.update(kwargs)

    monkeypatch.setattr(
        "qwenpaw.agents.acp.server.run_qwenpaw_agent",
        fake_run_qwenpaw_agent,
    )

    result = CliRunner().invoke(
        acp_cmd,
        [
            "--agent",
            "writer",
            "--workspace",
            str(tmp_path),
            "--local-diagnostics",
        ],
    )

    assert result.exit_code == 0
    assert captured["agent_id"] == "writer"
    assert captured["workspace_dir"] == tmp_path
    assert captured["local_diagnostics"] is True
    assert captured["runtime_provider"] is None


def test_acp_cmd_loads_openai_runtime_provider(monkeypatch):
    captured = {}

    async def fake_run_qwenpaw_agent(**kwargs):
        captured.update(kwargs)

    monkeypatch.setattr(
        "qwenpaw.agents.acp.server.run_qwenpaw_agent",
        fake_run_qwenpaw_agent,
    )

    result = CliRunner().invoke(
        acp_cmd,
        ["--runtime-provider", "openai-env"],
        env={
            "OPENAI_BASE_URL": "https://policy.example.test/v1",
            "OPENAI_API_KEY": "execution-secret",
            "OPENAI_MODEL": "policy",
        },
    )

    assert result.exit_code == 0
    config = captured["runtime_provider"]
    assert config.base_url == "https://policy.example.test/v1"
    assert config.api_key == "execution-secret"
    assert config.model == "policy"


def test_acp_cmd_rejects_incomplete_runtime_provider_environment():
    result = CliRunner().invoke(
        acp_cmd,
        ["--runtime-provider", "openai-env"],
        env={
            "OPENAI_BASE_URL": "https://policy.example.test/v1",
            "OPENAI_API_KEY": "execution-secret",
            "OPENAI_MODEL": "",
        },
    )

    assert result.exit_code == 2
    assert "OPENAI_MODEL" in result.output
    assert "execution-secret" not in result.output
