# -*- coding: utf-8 -*-
"""Unit tests for environment context and dynamic time injection.

Covers:

- ``build_env_context()`` no longer includes a static ``Current date``
  so that prompt caching benefits from stable env context.
- ``Runtime._inject_current_time()`` inserts a HintBlock into the last
  user message's content list.
"""
# pylint: disable=protected-access,redefined-outer-name
# pylint: disable=unused-argument,wrong-import-position
from __future__ import annotations

from typing import Any

from agentscope.message import HintBlock, Msg, TextBlock

from qwenpaw.app.chats.utils import build_env_context
from qwenpaw.runtime.runtime import Runtime


# ---------------------------------------------------------------------------
# build_env_context — must NOT contain a static current date
# ---------------------------------------------------------------------------


class TestBuildEnvContext:
    """The env context string should be stable across requests."""

    def test_no_current_date_field(self) -> None:
        """``build_env_context()`` must not contain a ``Current date``
        line."""
        ctx = build_env_context(
            session_id="s1",
            user_id="u1",
            add_hint=True,
        )
        assert "Current date" not in ctx

    def test_stable_across_calls(self) -> None:
        """Without time injection the env context should be identical."""
        ctx1 = build_env_context(
            session_id="s1",
            user_id="u1",
            working_dir="/tmp",
            add_hint=True,
        )
        ctx2 = build_env_context(
            session_id="s1",
            user_id="u1",
            working_dir="/tmp",
            add_hint=True,
        )
        assert ctx1 == ctx2


# ---------------------------------------------------------------------------
# _inject_current_time — HintBlock insertion
# ---------------------------------------------------------------------------


def _make_user_msg(text: str = "hello") -> Msg:
    """Create a real agentscope user Msg."""
    return Msg(
        name="user",
        role="user",
        content=[TextBlock(type="text", text=text)],
    )


class TestInjectCurrentTime:
    """``Runtime._inject_current_time()`` inserts a HintBlock."""

    def test_inserts_hint_block(self) -> None:
        """A HintBlock is inserted at position 0 of user content."""
        msgs: list[Any] = [_make_user_msg("hello")]
        Runtime._inject_current_time(msgs)
        content = msgs[0].content
        assert len(content) == 2
        assert isinstance(content[0], HintBlock)
        assert str(content[0].hint).startswith("Current time:")
        assert content[1].text == "hello"

    def test_user_text_unchanged(self) -> None:
        """``get_text_content()`` still returns raw user text."""
        msgs: list[Any] = [_make_user_msg("hello")]
        Runtime._inject_current_time(msgs)
        assert msgs[0].get_text_content() == "hello"

    def test_no_double_injection(self) -> None:
        """Calling twice must not insert the hint twice."""
        msgs: list[Any] = [_make_user_msg("hello")]
        Runtime._inject_current_time(msgs)
        Runtime._inject_current_time(msgs)
        hints = [b for b in msgs[0].content if isinstance(b, HintBlock)]
        assert len(hints) == 1

    def test_empty_message_list(self) -> None:
        """An empty message list should not raise."""
        msgs: list[Any] = []
        Runtime._inject_current_time(msgs)
        assert not msgs

    def test_no_user_message(self) -> None:
        """If there is no user message, nothing changes."""
        msg = Msg(
            name="assistant",
            role="assistant",
            content=[TextBlock(type="text", text="ok")],
        )
        msgs: list[Any] = [msg]
        Runtime._inject_current_time(msgs)
        assert len(msgs) == 1
        assert not any(isinstance(b, HintBlock) for b in msgs[0].content)
