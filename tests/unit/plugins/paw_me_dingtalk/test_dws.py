# -*- coding: utf-8 -*-
# pylint: disable=protected-access
"""Tests for official DWS OAuth event and send contracts."""

from __future__ import annotations

import os
from unittest.mock import AsyncMock

import pytest

from backend.dws import DwsClient, DwsStatus


def test_managed_runtime_does_not_fall_back_to_global_path(
    monkeypatch,
    tmp_path,
):
    """A PawApp install is isolated from any user-global DWS binary."""
    monkeypatch.setattr("backend.dws.shutil.which", lambda _name: "/bin/dws")
    client = DwsClient(tmp_path / "runtime")

    assert client.executable() == ""

    binary = (
        client.runtime_dir / "bin" / ("dws.exe" if os.name == "nt" else "dws")
    )
    binary.parent.mkdir(parents=True)
    binary.write_text("runtime", encoding="utf-8")
    binary.chmod(0o755)

    assert client.executable() == str(binary)


def test_managed_runtime_uses_an_isolated_config_directory(tmp_path):
    """OAuth metadata belongs to Paw Me instead of a global CLI setup."""
    client = DwsClient(tmp_path / "runtime")

    environment = client._environment()

    assert environment["DWS_CONFIG_DIR"] == str(
        tmp_path / "runtime" / "config",
    )


def test_parse_direct_event_uses_real_open_dingtalk_id():
    """A direct event authorizes and sends by sender openDingTalkId."""
    event = DwsClient.parse_event(
        {
            "type": "user_im_message_receive_o2o_all",
            "event_id": "event-1",
            "message_id": "message-1",
            "conversation_id": "conversation-1",
            "sender": "用户 09",
            "sender_open_dingtalk_id": "open-user-09",
            "content": "第一句",
            "timestamp": 1_700_000_000_000,
        },
    )

    assert event is not None
    assert event.subject_type == "person"
    assert event.subject_id == "open-user-09"


def test_parse_group_event_uses_real_conversation_id():
    """A group event authorizes and sends by openConversationId."""
    event = DwsClient.parse_event(
        {
            "type": "user_im_message_receive_group_all",
            "event_id": "event-2",
            "message_id": "message-2",
            "conversation_id": "open-conversation-2",
            "sender": "群成员",
            "sender_open_dingtalk_id": "open-member-2",
            "content": "群消息",
        },
    )

    assert event is not None
    assert event.subject_type == "group"
    assert event.subject_id == "open-conversation-2"


@pytest.mark.parametrize(
    "missing",
    ["event_id", "message_id", "sender_open_dingtalk_id", "content"],
)
def test_parse_direct_event_rejects_missing_stable_fields(missing):
    """An incomplete event fails closed instead of inventing identity."""
    payload = {
        "type": "user_im_message_receive_o2o_all",
        "event_id": "event-1",
        "message_id": "message-1",
        "conversation_id": "conversation-1",
        "sender_open_dingtalk_id": "open-user-09",
        "content": "消息",
    }
    payload.pop(missing)

    assert DwsClient.parse_event(payload) is None


@pytest.mark.asyncio
async def test_send_uses_exact_target_and_idempotency_key(monkeypatch):
    """Sending never resolves a display name or desktop conversation."""
    client = DwsClient()
    run_json = AsyncMock(return_value={"success": True})
    monkeypatch.setattr(client, "_run_json", run_json)

    await client.send(
        subject_type="person",
        subject_id="open-user-09",
        text="一次回复",
        idempotency_key="outbox-1",
    )

    run_json.assert_awaited_once_with(
        "chat",
        "message",
        "send",
        "--open-dingtalk-id",
        "open-user-09",
        "--content",
        "一次回复",
        "--ai-tag=false",
        "--idempotency-key",
        "outbox-1",
        "--format",
        "json",
        timeout=60.0,
    )


@pytest.mark.asyncio
async def test_login_is_tracked_and_times_out_quickly(monkeypatch):
    """Browser OAuth can be cancelled and never spins for ten minutes."""
    client = DwsClient()
    run_json = AsyncMock(return_value={"authenticated": True})
    status = AsyncMock(
        return_value=DwsStatus(available=True, authenticated=True),
    )
    monkeypatch.setattr(client, "_run_json", run_json)
    monkeypatch.setattr(client, "status", status)

    await client.login()

    run_json.assert_awaited_once_with(
        "auth",
        "login",
        "--format",
        "json",
        timeout=120.0,
        integration=True,
    )


@pytest.mark.asyncio
async def test_logout_targets_only_the_confirmed_oauth_account(monkeypatch):
    """Reconnect never clears another organization or account."""
    client = DwsClient()
    run_json = AsyncMock(return_value={"success": True})
    monkeypatch.setattr(client, "_run_json", run_json)
    status = DwsStatus(
        available=True,
        authenticated=True,
        corp_id="corp-a",
        user_id="user-a",
    )

    await client.logout(status)

    run_json.assert_awaited_once_with(
        "auth",
        "logout",
        "--profile",
        "corp-a:user-a",
        "--yes",
        "--format",
        "json",
    )


@pytest.mark.asyncio
async def test_group_members_uses_real_conversation_id(monkeypatch):
    """Owner resolution is based on DWS data, never a display coordinate."""
    client = DwsClient()
    run_json = AsyncMock(return_value={"result": {"list": []}})
    monkeypatch.setattr(client, "_run_json", run_json)

    await client.group_members("open-conversation-2")

    run_json.assert_awaited_once_with(
        "chat",
        "group",
        "members",
        "--id",
        "open-conversation-2",
        "--format",
        "json",
    )
