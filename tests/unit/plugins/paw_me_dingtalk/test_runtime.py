# -*- coding: utf-8 -*-
# pylint: disable=protected-access
"""Tests for Paw Me batching and authoritative Agent prompts."""

from __future__ import annotations

import asyncio
import importlib
import sys
from dataclasses import dataclass, field
from types import SimpleNamespace
from unittest.mock import AsyncMock

import pytest

from backend.dws import DwsMessageEvent, DwsStatus
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


@pytest.mark.asyncio
async def test_cancelled_oauth_resets_visible_integration_state(
    monkeypatch,
    tmp_path,
):
    """Closing setup can be followed by an explicit cancel and retry."""
    module = load_runtime(monkeypatch, tmp_path)
    started = asyncio.Event()

    async def wait_for_login():
        started.set()
        await asyncio.Event().wait()

    monkeypatch.setattr(module.DWS, "login", wait_for_login)
    runtime = module.PawMeRuntime()
    runtime.begin_integration("login")
    await started.wait()

    await runtime.cancel_integration()

    assert runtime.integration_stage == "cancelled"
    assert runtime.integration_detail == "操作已取消，可以随时重新开始"
    assert runtime.integration_task is not None
    assert runtime.integration_task.cancelled()


@pytest.mark.asyncio
async def test_unauthenticated_enable_is_normalized_without_conflict(
    monkeypatch,
    tmp_path,
):
    """A stale enabled toggle returns a setup state instead of HTTP 409."""
    module = load_runtime(monkeypatch, tmp_path)
    status = DwsStatus(
        available=True,
        authenticated=False,
        detail="需要完成钉钉 OAuth 登录",
    )
    monkeypatch.setattr(
        module.RUNTIME,
        "refresh_dws_status",
        AsyncMock(return_value=status),
    )
    ctx = SimpleNamespace(agent_id="default")
    payload = module.SettingsPayload(
        enabled=True,
        agent_id="default",
        default_policy="draft",
        quiet_seconds=4,
        max_wait_seconds=20,
    )

    result = await module.update_settings(payload, ctx)

    assert result["settings"]["enabled"] is False


@pytest.mark.asyncio
async def test_authenticated_account_must_be_confirmed_before_enable(
    monkeypatch,
    tmp_path,
):
    """An OAuth login alone cannot authorize the digital twin to speak."""
    module = load_runtime(monkeypatch, tmp_path)
    status = DwsStatus(
        available=True,
        authenticated=True,
        corp_id="corp-a",
        user_id="user-a",
        user_name="本人",
    )
    monkeypatch.setattr(
        module.RUNTIME,
        "refresh_dws_status",
        AsyncMock(return_value=status),
    )
    ctx = SimpleNamespace(agent_id="default")
    payload = module.SettingsPayload(
        enabled=True,
        agent_id="default",
        default_policy="draft",
        quiet_seconds=4,
        max_wait_seconds=20,
    )

    result = await module.update_settings(payload, ctx)

    assert result["settings"]["enabled"] is False
    assert result["identity_provider"]["confirmed"] is False


@pytest.mark.asyncio
async def test_group_event_from_oauth_owner_is_ignored(monkeypatch, tmp_path):
    """A sent group message cannot re-enter Paw Me as peer input."""
    module = load_runtime(monkeypatch, tmp_path)
    store = PawMeStore(tmp_path / "self-filter.sqlite3")
    module.STORE = store
    runtime = module.PawMeRuntime()
    runtime.dws_status = DwsStatus(
        available=True,
        authenticated=True,
        user_id="owner-user-id",
        user_name="hello",
    )
    monkeypatch.setattr(
        module.DWS,
        "group_members",
        AsyncMock(
            return_value={
                "result": {
                    "list": [
                        {
                            "memberEmpName": "hello",
                            "openDingtalkId": "owner-open-id",
                        },
                    ],
                },
            },
        ),
    )
    event = DwsMessageEvent(
        event_id="event-self",
        event_type="user_im_message_receive_group_all",
        message_id="message-self",
        conversation_id="group-real-id",
        sender="本人",
        sender_open_dingtalk_id="owner-open-id",
        content="刚发出的回复",
        create_time="",
        timestamp=100,
        raw={},
    )

    await runtime._append_event(event)

    assert store.list_work_items() == []
    assert store.list_activity()[0]["status"] == "ignored_self"


@dataclass
class _ReplyWorkspace:
    requests: list = field(default_factory=list)

    async def stream_query(self, request):
        self.requests.append(request)
        yield "我是大白，你的个人 AI 助手，可以帮你处理任务。"


@dataclass
class _ReplyContext:
    agent_id: str
    workspace: _ReplyWorkspace

    async def _get_workspace(self):
        return self.workspace


@pytest.mark.asyncio
async def test_agent_identity_leak_is_never_auto_sent(monkeypatch, tmp_path):
    """Agent persona leakage fails closed even under automatic policy."""
    module = load_runtime(monkeypatch, tmp_path)
    store = PawMeStore(tmp_path / "identity-gate.sqlite3")
    module.STORE = store
    store.add_principal(
        subject_type="person",
        subject_id="peer-real-id",
        id_source="oauth:dws-event",
        display_name="用户 09",
        conversation_alias="用户 09",
        policy="automatic",
    )
    item, _ = store.observe(
        source_key="event-identity",
        conversation_alias="用户 09",
        subject_type="person",
        subject_id="peer-real-id",
        id_source="oauth:dws-event",
        display_name="用户 09",
        text="你是谁",
        agent_id="agent-a",
        quiet_seconds=4,
        max_wait_seconds=20,
        received_at=100,
    )
    runtime = module.PawMeRuntime()
    runtime.dws_status = DwsStatus(
        available=True,
        authenticated=True,
        user_name="账号主人",
    )
    monkeypatch.setattr(runtime, "_refresh_history", AsyncMock())
    send = AsyncMock()
    monkeypatch.setattr(runtime, "send_outbox", send)
    workspace = _ReplyWorkspace()
    context = _ReplyContext(agent_id="agent-a", workspace=workspace)

    await runtime._run_agent(context, item["id"])

    assert len(workspace.requests) == 1
    request = workspace.requests[0]
    assert [message.role.value for message in request.input] == [
        "system",
        "user",
    ]
    instructions = request.input[0].content[0].text
    assert "执行引擎" in instructions
    assert "账号主人" in instructions
    outbox = store.list_outbox()[0]
    assert outbox["status"] == "needs_review"
    assert "身份" in outbox["error"]
    send.assert_not_awaited()
