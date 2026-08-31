# -*- coding: utf-8 -*-
"""Tests for bounded owner-profile collection and local prompt use."""

from __future__ import annotations

from unittest.mock import AsyncMock

import pytest

from backend.profile import OwnerProfileCollector, profile_prompt


@pytest.mark.asyncio
async def test_profile_collects_only_owner_message_bodies():
    """Peer text contributes counts but is never copied into the profile."""
    dws = AsyncMock()
    dws.self_profile.return_value = {
        "result": {"userId": "owner", "name": "本人"},
    }
    dws.people_by_dimension.return_value = {"data": {"items": []}}
    dws.conversations.return_value = {
        "conversations": [
            {
                "openConversationId": "conversation-1",
                "conversationName": "项目群",
            },
        ],
    }
    dws.group_members.return_value = {
        "result": {
            "list": [
                {
                    "userId": "owner",
                    "openDingtalkId": "open-owner",
                    "memberEmpName": "本人",
                },
                {
                    "openDingtalkId": "open-peer",
                    "memberEmpName": "同事",
                },
            ],
        },
    }
    dws.conversation_history.return_value = {
        "messages": [
            {
                "senderOpenDingTalkId": "open-owner",
                "content": "我自己的表达样本",
            },
            {
                "senderOpenDingTalkId": "open-peer",
                "sender": "同事",
                "content": "不应保存的对方正文",
            },
        ],
    }
    dws.created_todos.return_value = {"data": {"created": []}}
    dws.agenda.return_value = {"data": {"events": []}}
    progress = AsyncMock()

    profile, errors = await OwnerProfileCollector(dws).collect(
        corp_id="corp",
        user_id="owner",
        user_name="本人",
        progress=progress,
    )

    assert errors == []
    assert profile["work_style"]["voice_examples"] == [
        "我自己的表达样本",
    ]
    assert "不应保存的对方正文" not in str(profile)
    assert profile["relationships"][0]["interaction_count"] == 1


@pytest.mark.asyncio
async def test_member_failure_does_not_drop_conversation_history():
    """A direct chat remains collectable when group membership is invalid."""
    dws = AsyncMock()
    dws.self_profile.return_value = {"userId": "owner", "name": "本人"}
    dws.people_by_dimension.return_value = {"items": []}
    dws.conversations.return_value = {
        "conversations": [
            {"openConversationId": "direct-1", "conversationName": "单聊"},
        ],
    }
    dws.group_members.side_effect = RuntimeError("not a group")
    dws.conversation_history.return_value = {
        "messages": [{"isSelf": True, "content": "仍然保留"}],
    }
    dws.created_todos.return_value = {"created": []}
    dws.agenda.return_value = {"events": []}

    profile, _errors = await OwnerProfileCollector(dws).collect(
        corp_id="corp",
        user_id="owner",
        user_name="本人",
        progress=AsyncMock(),
    )

    assert profile["work_style"]["voice_examples"] == ["仍然保留"]


def test_profile_prompt_selects_only_current_relationship():
    """The hot-path prompt reads one local relationship by real ID."""
    profile = {
        "identity": {"name": "本人", "departments": ["研发"]},
        "work_style": {"voice_examples": ["直接推进"]},
        "relationships": [
            {
                "subject_id": "peer-a",
                "name": "甲",
                "interaction_count": 3,
                "shared_group_count": 1,
            },
            {
                "subject_id": "peer-b",
                "name": "乙",
                "interaction_count": 8,
                "shared_group_count": 2,
            },
        ],
    }

    prompt = profile_prompt(profile, "peer-a")

    assert "甲，互动 3 次" in prompt
    assert "乙" not in prompt
