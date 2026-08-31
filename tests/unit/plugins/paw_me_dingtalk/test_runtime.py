# -*- coding: utf-8 -*-
# pylint: disable=protected-access
"""Tests for Paw Me batching and authoritative Agent prompts."""

from __future__ import annotations

import importlib
import sys

from backend.store import PawMeStore


def load_runtime(monkeypatch, tmp_path):
    """Load the PawApp entry without touching the user's working data."""
    from qwenpaw import constant

    monkeypatch.setattr(constant, "WORKING_DIR", tmp_path)
    sys.modules.pop("backend.main", None)
    return importlib.import_module("backend.main")


def test_history_projection_identifies_owner_and_peer(monkeypatch, tmp_path):
    """Direct history keeps owner style and peer context in order."""
    runtime = load_runtime(monkeypatch, tmp_path)
    payload = {
        "result": {
            "messages": [
                {
                    "openMessageId": "m1",
                    "sender": {
                        "name": "我",
                        "openDingTalkId": "owner-id",
                    },
                    "content": "行，我来搞",
                },
                {
                    "openMessageId": "m2",
                    "sender": {
                        "name": "用户 09",
                        "openDingTalkId": "peer-id",
                    },
                    "content": "帮我看下",
                },
            ],
        },
    }

    rows = runtime.PawMeRuntime._project_history(
        payload,
        "person",
        "peer-id",
        "owner-id",
    )

    assert [row["incoming"] for row in rows] == [False, True]
    assert [row["message_id"] for row in rows] == ["m1", "m2"]


def test_prompt_contains_full_batch_and_owner_style_context(
    monkeypatch,
    tmp_path,
):
    """An interrupted retry receives every durable message and style cue."""
    runtime = load_runtime(monkeypatch, tmp_path)
    store = PawMeStore(tmp_path / "prompt.sqlite3")
    runtime.STORE = store
    principal = store.add_principal(
        subject_type="person",
        subject_id="real-user-id",
        id_source="oauth:dws-event",
        display_name="用户 09",
        conversation_alias="用户 09",
        policy="draft",
    )
    assert principal["subject_id"] == "real-user-id"
    store.save_context(
        "person:real-user-id",
        "person",
        [
            {"incoming": False, "text": "行，我来搞"},
            {"incoming": True, "text": "帮我看下"},
        ],
    )
    first, _ = store.observe(
        source_key="event-1",
        conversation_alias="用户 09",
        subject_type="person",
        subject_id="real-user-id",
        id_source="oauth:dws-event",
        display_name="用户 09",
        text="先看报错",
        agent_id="agent-a",
        quiet_seconds=4,
        max_wait_seconds=20,
        received_at=100,
    )
    second, _ = store.observe(
        source_key="event-2",
        conversation_alias="用户 09",
        subject_type="person",
        subject_id="real-user-id",
        id_source="oauth:dws-event",
        display_name="用户 09",
        text="再给解决步骤",
        agent_id="agent-a",
        quiet_seconds=4,
        max_wait_seconds=20,
        received_at=101,
    )
    assert first["id"] == second["id"]

    prompt = runtime.PawMeRuntime._build_prompt(second)

    assert "我：行，我来搞" in prompt
    assert "1. 先看报错" in prompt
    assert "2. 再给解决步骤" in prompt
    assert "只回复一次" in prompt
    assert "完整批次" in prompt
