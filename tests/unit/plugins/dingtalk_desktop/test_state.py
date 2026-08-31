# -*- coding: utf-8 -*-
"""Tests for agent-scoped DingTalk draft persistence."""

from __future__ import annotations

import json

from dingtalk_desktop.state import DraftStore


def test_draft_store_round_trip_and_remove(tmp_path):
    """Drafts survive a new store instance and can be removed."""
    path = tmp_path / "state" / "drafts.json"
    created = DraftStore(path).add("conversation", "reply")

    loaded = DraftStore(path).list()

    assert loaded == [created]
    assert DraftStore(path).get(created.id) == created
    assert DraftStore(path).remove(created.id) is True
    assert DraftStore(path).remove(created.id) is False
    assert DraftStore(path).list() == []


def test_draft_store_ignores_invalid_records(tmp_path):
    """A malformed record cannot prevent valid drafts from loading."""
    path = tmp_path / "drafts.json"
    path.write_text(
        json.dumps(
            {
                "drafts": [
                    {"invalid": True},
                    {
                        "id": "valid",
                        "conversation": "chat",
                        "text": "reply",
                        "created_at": 1.0,
                    },
                ],
            },
        ),
        encoding="utf-8",
    )

    assert [item.id for item in DraftStore(path).list()] == ["valid"]
