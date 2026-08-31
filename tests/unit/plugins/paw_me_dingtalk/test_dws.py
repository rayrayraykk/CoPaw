# -*- coding: utf-8 -*-
"""Tests for official DWS OAuth event and send contracts."""

from __future__ import annotations

from unittest.mock import AsyncMock

import pytest

from backend.dws import DwsClient


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
