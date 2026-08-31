# -*- coding: utf-8 -*-
"""Tests for lossless Paw Me identity and message storage."""

from __future__ import annotations

from backend.store import PawMeStore


def observe(store, source_key, text="同一句"):
    """Append one test message with deterministic timing."""
    return store.observe(
        source_key=source_key,
        conversation_alias="用户 09",
        subject_type="person",
        subject_id="real-user-id",
        id_source="oauth:dws-event",
        display_name="用户 09",
        text=text,
        agent_id="selected-agent",
        quiet_seconds=4,
        max_wait_seconds=20,
        received_at=100,
    )


def verified_principal(store, policy="draft"):
    """Add one trusted DingTalk identity."""
    return store.add_principal(
        subject_type="person",
        subject_id="real-user-id",
        id_source="oauth:dws-event",
        display_name="用户 09",
        conversation_alias="用户 09",
        policy=policy,
    )


def test_unknown_identity_fails_closed_and_can_be_bound(tmp_path):
    """A real DWS identity cannot invoke an Agent until it is authorized."""
    store = PawMeStore(tmp_path / "paw-me.sqlite3")
    item, created = observe(store, "event-1")

    assert created is True
    assert item["status"] == "identity_required"
    assert item["subject_id"] == "real-user-id"

    principal = verified_principal(store)
    assert store.bind_pending(principal) == 1
    bound = store.get_work_item(item["id"])
    assert bound["status"] == "collecting"
    assert bound["subject_id"] == "real-user-id"
    assert bound["session_id"].endswith(":person:real-user-id")


def test_repeated_text_is_preserved_and_batched_once(tmp_path):
    """Identical consecutive messages remain two ordered raw events."""
    store = PawMeStore(tmp_path / "paw-me.sqlite3")
    verified_principal(store)

    first, _ = observe(store, "event-1")
    second, _ = observe(store, "event-2")

    assert first["id"] == second["id"]
    assert second["message_count"] == 2
    assert [row["text"] for row in second["messages"]] == [
        "同一句",
        "同一句",
    ]


def test_new_message_requests_stop_for_running_agent(tmp_path):
    """A new message interrupts rather than racing an active reply."""
    store = PawMeStore(tmp_path / "paw-me.sqlite3")
    verified_principal(store)
    item, _ = observe(store, "event-1")
    store.update_work_item(item["id"], status="agent_running")

    updated, _ = observe(store, "event-2", "补充一句")

    assert updated["status"] == "interrupt_requested"
    assert updated["message_count"] == 2
    assert updated["messages"][-1]["text"] == "补充一句"


def test_interrupted_reply_cannot_enter_outbox(tmp_path):
    """A reply racing with a newer event is discarded atomically."""
    store = PawMeStore(tmp_path / "paw-me.sqlite3")
    verified_principal(store)
    item, _ = observe(store, "event-1")
    store.update_work_item(item["id"], status="agent_running")
    observe(store, "event-2", "补充一句")

    assert store.finalize_agent_reply(item["id"], "旧回复") is None
    assert store.list_outbox() == []


def test_restart_recovers_incomplete_agent_without_losing_messages(tmp_path):
    """A process restart returns interrupted input to the dispatch queue."""
    store = PawMeStore(tmp_path / "paw-me.sqlite3")
    verified_principal(store)
    item, _ = observe(store, "event-1")
    store.update_work_item(item["id"], status="agent_running")

    assert store.recover_incomplete() == 1
    recovered = store.get_work_item(item["id"])
    assert recovered["status"] == "collecting"
    assert [row["text"] for row in recovered["messages"]] == ["同一句"]


def test_context_snapshot_is_persisted_in_order(tmp_path):
    """Recent incoming and outgoing context survives process restarts."""
    path = tmp_path / "paw-me.sqlite3"
    store = PawMeStore(path)
    messages = [
        {"incoming": False, "text": "我之前的语气"},
        {"incoming": True, "text": "对方的问题"},
    ]
    store.save_context("用户 09", "person", messages)

    assert PawMeStore(path).get_context("用户 09") == messages


def test_context_append_deduplicates_ids_but_preserves_repeated_text(
    tmp_path,
):
    """Context is lossless for repeated text and idempotent per message ID."""
    store = PawMeStore(tmp_path / "paw-me.sqlite3")
    store.append_context(
        "person:real-user-id",
        "person",
        [
            {"message_id": "m1", "text": "同一句"},
            {"message_id": "m2", "text": "同一句"},
        ],
    )
    store.append_context(
        "person:real-user-id",
        "person",
        [{"message_id": "m2", "text": "同一句"}],
    )

    context = store.get_context("person:real-user-id")
    assert [item["message_id"] for item in context] == ["m1", "m2"]


def test_raw_dws_event_is_persisted_before_processing(tmp_path):
    """The authoritative inbound row retains the original DWS payload."""
    store = PawMeStore(tmp_path / "paw-me.sqlite3")
    item, _ = store.observe(
        source_key="event-raw",
        conversation_alias="用户 09",
        subject_type="person",
        subject_id="real-user-id",
        id_source="oauth:dws-event",
        display_name="用户 09",
        text="原始消息",
        agent_id="agent-a",
        quiet_seconds=4,
        max_wait_seconds=20,
        received_at=100,
        raw_message={"event_id": "event-raw", "message_id": "m-raw"},
    )

    assert item["messages"][0]["raw"]["message_id"] == "m-raw"
