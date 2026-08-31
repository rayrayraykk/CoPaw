# -*- coding: utf-8 -*-
# pylint: disable=protected-access
"""Tests for safe observation and reply modes."""

from __future__ import annotations

import threading
from types import SimpleNamespace

import pytest

from dingtalk_desktop.channel import DingTalkDesktopChannel
from dingtalk_desktop.models import DesktopStatus, DialogueMessage
from dingtalk_desktop.state import DraftStore
from qwenpaw.app.channels.access_control import AccessControlStore


class FakeDriver:
    """Minimal semantic driver used by channel tests."""

    def __init__(self, conversation="allowed", message=None, ready=True):
        self.conversation = conversation
        self.message = message
        self.sent = []
        self.ready = ready

    def status(self):
        """Return configurable desktop readiness."""
        return DesktopStatus(
            supported=True,
            installed=True,
            running=self.ready,
            accessibility=self.ready,
            logged_in=self.ready,
            bundle_id="bundle",
            detail="ready" if self.ready else "open DingTalk",
        )

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
    channel.driver = driver
    channel.poll_sec = 1.0
    channel.context_messages = 16
    channel.drafts = DraftStore(tmp_path / "drafts.json")
    channel._last_message_fingerprint = ""
    channel._enqueue = None
    channel._stop_event = threading.Event()
    channel._thread = None
    channel._desktop_ready = False
    channel._desktop_detail = ""
    channel.access_control_dm = True
    channel.access_control_group = False
    channel._language = "zh"
    access_store = AccessControlStore(tmp_path / "access_control.json")
    access_store.add_to_whitelist(
        channel="dingtalk_desktop",
        user_id="allowed",
    )
    channel._get_acl_store = lambda: access_store
    return channel


def test_observe_emits_only_new_visible_incoming_message(tmp_path):
    """One verified incoming message is enqueued once."""
    driver = FakeDriver(
        message=DialogueMessage("question", True),
    )
    channel = bare_channel(tmp_path, driver)
    emitted = []
    channel._enqueue = emitted.append

    channel._observe_once(emit=True)
    channel._observe_once(emit=True)

    assert len(emitted) == 1
    assert emitted[0]["sender_id"] == "allowed"


@pytest.mark.asyncio
async def test_unapproved_conversation_enters_shared_pending_queue(tmp_path):
    """A new visible conversation uses the shared channel ACL gate."""
    driver = FakeDriver(
        conversation="other",
        message=DialogueMessage("question", True),
    )
    channel = bare_channel(tmp_path, driver)
    emitted = []
    channel._enqueue = emitted.append

    channel._observe_once(emit=True)
    blocked = await channel._access_control_gate(emitted[0])

    assert blocked is True
    pending = channel._get_acl_store().get_acl(channel.channel)["pending"]
    assert pending[0]["user_id"] == "other"
    assert pending[0]["first_message"] == "question"


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


@pytest.mark.asyncio
async def test_send_drops_conversation_removed_from_shared_acl(tmp_path):
    """Draft creation stops immediately after unified access is revoked."""
    driver = FakeDriver(conversation="other")
    channel = bare_channel(tmp_path, driver)

    await channel.send("other", "reply")

    assert channel.drafts.list() == []
    assert not driver.sent


def test_from_config_enables_shared_dm_access_control(monkeypatch, tmp_path):
    """The desktop channel always delegates authorization to shared ACL."""
    monkeypatch.setattr(
        "qwenpaw.app.channels.base.load_config",
        lambda: SimpleNamespace(
            tools=SimpleNamespace(builtin_tools={}),
        ),
    )
    config = SimpleNamespace(
        enabled=True,
        access_control_dm=True,
        reply_mode="draft",
        poll_sec=1.0,
        bundle_id="bundle",
        context_messages=16,
    )

    channel = DingTalkDesktopChannel.from_config(
        process=lambda request: request,
        config=config,
        workspace_dir=tmp_path,
    )

    assert channel.access_control_dm is True
    assert channel.access_control_group is False


def test_from_config_rejects_legacy_private_allowlist(monkeypatch, tmp_path):
    """An old private allowlist cannot bypass one-click ACL setup."""
    monkeypatch.setattr(
        "qwenpaw.app.channels.base.load_config",
        lambda: SimpleNamespace(
            tools=SimpleNamespace(builtin_tools={}),
        ),
    )
    config = SimpleNamespace(
        enabled=True,
        reply_mode="automatic",
        allowed_conversations="Legacy title",
        poll_sec=1.0,
        bundle_id="bundle",
        context_messages=16,
    )

    channel = DingTalkDesktopChannel.from_config(
        process=lambda request: request,
        config=config,
        workspace_dir=tmp_path,
    )

    assert channel.enabled is False


@pytest.mark.asyncio
async def test_start_retries_when_desktop_is_not_ready(tmp_path):
    """A signed-out desktop starts in recoverable degraded mode."""
    channel = bare_channel(tmp_path, FakeDriver(ready=False))
    channel._watcher_loop = lambda: None

    await channel.start()

    assert channel._desktop_ready is False
    assert channel._desktop_detail == "open DingTalk"
    assert channel._thread is not None
    channel._thread.join()


def test_persona_prompt_uses_outgoing_style_and_requires_progress_blocks():
    """The agent request receives grounded style and progress rules."""
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
