# -*- coding: utf-8 -*-
# pylint: disable=protected-access
"""Tests for safe observation and reply modes."""

from __future__ import annotations

from types import SimpleNamespace

import pytest

from dingtalk_desktop.channel import DingTalkDesktopChannel
from dingtalk_desktop.models import DialogueMessage
from dingtalk_desktop.state import DraftStore


class FakeDriver:
    """Minimal semantic driver used by channel tests."""

    def __init__(self, conversation="allowed", message=None):
        self.conversation = conversation
        self.message = message
        self.sent = []

    def current_conversation(self):
        """Return the currently visible title."""
        return self.conversation

    def read_latest(self, conversation):
        """Return a preconfigured semantic message."""
        assert conversation == self.conversation
        return self.message

    def read_context(self, conversation, limit):
        """Return recent semantically directed history."""
        assert conversation == self.conversation
        assert limit >= 4
        return [self.message] if self.message else []

    def send(self, conversation, text):
        """Record a verified send."""
        self.sent.append((conversation, text))


def bare_channel(tmp_path, driver):
    """Create the minimal channel state needed by focused unit tests."""
    channel = object.__new__(DingTalkDesktopChannel)
    channel.channel = "dingtalk_desktop"
    channel.enabled = True
    channel.reply_mode = "draft"
    channel.allowed_conversations = frozenset({"allowed"})
    channel.driver = driver
    channel.context_messages = 16
    channel.drafts = DraftStore(tmp_path / "drafts.json")
    channel._last_message_fingerprint = ""
    channel._enqueue = None
    return channel


def test_observe_emits_only_new_allowed_incoming_message(tmp_path):
    """One verified incoming message is enqueued once."""
    driver = FakeDriver(
        message=DialogueMessage("question", True),
    )
    channel = bare_channel(tmp_path, driver)
    emitted = []
    channel._enqueue = emitted.append
    channel.build_agent_request_from_native = lambda payload: payload

    channel._observe_once(emit=True)
    channel._observe_once(emit=True)

    assert len(emitted) == 1
    assert emitted[0]["sender_id"] == "allowed"


def test_observe_ignores_non_allowlisted_conversation(tmp_path):
    """A visible title outside the exact allowlist is never read."""
    driver = FakeDriver(
        conversation="other",
        message=DialogueMessage("question", True),
    )
    channel = bare_channel(tmp_path, driver)
    channel._enqueue = pytest.fail

    channel._observe_once(emit=True)


@pytest.mark.asyncio
async def test_send_defaults_to_draft(tmp_path):
    """Draft mode persists text and does not call the desktop sender."""
    driver = FakeDriver()
    channel = bare_channel(tmp_path, driver)

    await channel.send("allowed", "reply")

    assert [item.text for item in channel.drafts.list()] == ["reply"]
    assert not driver.sent


@pytest.mark.asyncio
async def test_automatic_send_requires_explicit_mode(tmp_path):
    """Automatic mode delegates one exact semantic send."""
    driver = FakeDriver()
    channel = bare_channel(tmp_path, driver)
    channel.reply_mode = "automatic"

    await channel.send("allowed", "reply")

    assert driver.sent == [("allowed", "reply")]


@pytest.mark.asyncio
async def test_automatic_send_delivers_observable_steps_in_order(tmp_path):
    """Progress blocks become separate ordered desktop messages."""
    driver = FakeDriver()
    channel = bare_channel(tmp_path, driver)
    channel.reply_mode = "automatic"

    await channel.send(
        "allowed",
        (
            "<dingtalk_message>我先处理下</dingtalk_message>"
            "<dingtalk_message>已经弄好了</dingtalk_message>"
        ),
    )

    assert driver.sent == [
        ("allowed", "我先处理下"),
        ("allowed", "已经弄好了"),
    ]


def test_from_config_accepts_structured_allowlist(monkeypatch, tmp_path):
    """One-click setup preserves titles containing punctuation."""
    monkeypatch.setattr(
        "qwenpaw.app.channels.base.load_config",
        lambda: SimpleNamespace(
            tools=SimpleNamespace(builtin_tools={}),
        ),
    )
    config = SimpleNamespace(
        enabled=True,
        reply_mode="draft",
        allowed_conversations=["A, B"],
        poll_sec=1.0,
        bundle_id="bundle",
        context_messages=16,
    )

    channel = DingTalkDesktopChannel.from_config(
        process=lambda request: request,
        config=config,
        workspace_dir=tmp_path,
    )

    assert channel.allowed_conversations == {"A, B"}


def test_persona_prompt_uses_outgoing_style_and_requires_progress_blocks():
    """The Codex request receives grounded style and progress rules."""
    history = [
        DialogueMessage("能今天给我吗", True),
        DialogueMessage("可以，我晚点看下", False),
    ]

    prompt = DingTalkDesktopChannel._build_persona_prompt(
        history,
        "现在怎么样了",
    )

    assert "[对方] 能今天给我吗" in prompt
    assert "[我] 可以，我晚点看下" in prompt
    assert "只从标记为[我]的历史消息学习" in prompt
    assert "信息不足时不要猜" in prompt
    assert "可观察" in prompt
    assert "<dingtalk_message>" in prompt
    assert "隐藏推理链" in prompt


def test_reply_parts_split_observable_steps():
    """One model response becomes ordered DingTalk progress messages."""
    response = (
        "<dingtalk_message>我先看下</dingtalk_message>"
        "<dingtalk_message>已经处理好了</dingtalk_message>"
    )

    assert DingTalkDesktopChannel._reply_parts(response) == [
        "我先看下",
        "已经处理好了",
    ]
