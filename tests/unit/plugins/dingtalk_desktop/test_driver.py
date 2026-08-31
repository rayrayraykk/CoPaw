# -*- coding: utf-8 -*-
# pylint: disable=protected-access
"""Tests for the credential-free DingTalk desktop driver."""

from __future__ import annotations

import json
import subprocess
from pathlib import Path

import pytest

from dingtalk_desktop.driver import (
    DingTalkDesktopDriver,
    DingTalkDesktopError,
)


def test_run_passes_json_to_jxa(monkeypatch, tmp_path):
    """The driver invokes the bundled bridge with a structured request."""
    captured = {}

    def fake_run(command, **kwargs):
        captured["command"] = command
        captured["request"] = json.loads(kwargs["input"])
        return subprocess.CompletedProcess(
            command,
            0,
            stdout=json.dumps({"ok": True, "result": {"value": 1}}),
            stderr="",
        )

    monkeypatch.setattr(subprocess, "run", fake_run)
    bridge = tmp_path / "bridge.js"
    driver = DingTalkDesktopDriver(bridge_path=bridge)

    assert driver._run("probe", {"safe": True}) == {"value": 1}
    assert captured["command"] == [
        "osascript",
        "-l",
        "JavaScript",
        str(bridge),
    ]
    assert captured["request"]["safe"] is True


def test_run_rejects_bridge_failure(monkeypatch):
    """A bridge error is converted to the plugin's stable exception."""
    monkeypatch.setattr(
        subprocess,
        "run",
        lambda *args, **kwargs: subprocess.CompletedProcess(
            args[0],
            0,
            stdout=json.dumps({"ok": False, "error": "denied"}),
            stderr="",
        ),
    )

    with pytest.raises(DingTalkDesktopError, match="denied"):
        DingTalkDesktopDriver()._run("status")


def test_bridge_has_no_coordinate_or_click_fallback():
    """Desktop operations must remain semantic and fail closed."""
    bridge = Path(__file__).resolve().parents[4] / (
        "plugins/channel/dingtalk_desktop/dingtalk_desktop/bridge.js"
    )
    source = bridge.read_text(encoding="utf-8")
    history_source = bridge.with_name("ax_history.swift").read_text(
        encoding="utf-8",
    )

    assert ".position(" not in source
    assert "click(" not in source
    assert "kAXPositionAttribute" not in history_source
    assert "session msg receiving" in source
    assert "session msg receiving" in history_source
    assert "currentConversation(process) !== title" in source
    assert "kAXRowsAttribute" in history_source
