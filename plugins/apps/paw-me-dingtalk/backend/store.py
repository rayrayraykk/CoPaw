# -*- coding: utf-8 -*-
"""Transactional application storage for Paw Me DingTalk."""

from __future__ import annotations

import json
import sqlite3
import threading
import time
import uuid
from pathlib import Path
from typing import Any


VALID_POLICIES = frozenset({"observe", "draft", "automatic", "blocked"})
VALID_SUBJECT_TYPES = frozenset({"person", "group"})
ACTIVE_BATCH_STATES = (
    "collecting",
    "agent_running",
    "interrupt_requested",
)


class PawMeStore:  # pylint: disable=too-many-public-methods
    """Own verified identities, raw messages, turns, drafts, and audit."""

    def __init__(self, path: Path) -> None:
        self.path = path
        self._lock = threading.RLock()
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self._initialize()

    def _connect(self) -> sqlite3.Connection:
        connection = sqlite3.connect(self.path, timeout=10)
        connection.row_factory = sqlite3.Row
        connection.execute("PRAGMA foreign_keys = ON")
        connection.execute("PRAGMA journal_mode = WAL")
        return connection

    def _initialize(self) -> None:
        schema = """
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS principals (
            id TEXT PRIMARY KEY,
            subject_type TEXT NOT NULL,
            subject_id TEXT NOT NULL,
            id_source TEXT NOT NULL,
            display_name TEXT NOT NULL,
            conversation_alias TEXT NOT NULL,
            policy TEXT NOT NULL,
            created_at REAL NOT NULL,
            updated_at REAL NOT NULL,
            UNIQUE(subject_type, subject_id)
        );
        CREATE TABLE IF NOT EXISTS work_items (
            id TEXT PRIMARY KEY,
            conversation_alias TEXT NOT NULL,
            subject_type TEXT NOT NULL,
            subject_id TEXT,
            id_source TEXT NOT NULL DEFAULT '',
            display_name TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL,
            agent_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            quiet_deadline REAL NOT NULL,
            hard_deadline REAL NOT NULL,
            message_count INTEGER NOT NULL DEFAULT 0,
            response TEXT NOT NULL DEFAULT '',
            error TEXT NOT NULL DEFAULT '',
            created_at REAL NOT NULL,
            updated_at REAL NOT NULL
        );
        CREATE TABLE IF NOT EXISTS inbound_messages (
            id TEXT PRIMARY KEY,
            source_key TEXT NOT NULL UNIQUE,
            work_item_id TEXT NOT NULL,
            conversation_alias TEXT NOT NULL,
            text TEXT NOT NULL,
            raw_json TEXT NOT NULL DEFAULT '{}',
            received_at REAL NOT NULL,
            ordinal INTEGER NOT NULL,
            FOREIGN KEY(work_item_id) REFERENCES work_items(id)
        );
        CREATE TABLE IF NOT EXISTS outbox (
            id TEXT PRIMARY KEY,
            work_item_id TEXT NOT NULL UNIQUE,
            subject_id TEXT NOT NULL,
            conversation_alias TEXT NOT NULL,
            text TEXT NOT NULL,
            status TEXT NOT NULL,
            error TEXT NOT NULL DEFAULT '',
            created_at REAL NOT NULL,
            updated_at REAL NOT NULL,
            sent_at REAL,
            FOREIGN KEY(work_item_id) REFERENCES work_items(id)
        );
        CREATE TABLE IF NOT EXISTS activity (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            kind TEXT NOT NULL,
            status TEXT NOT NULL,
            title TEXT NOT NULL,
            detail TEXT NOT NULL,
            work_item_id TEXT,
            created_at REAL NOT NULL
        );
        CREATE TABLE IF NOT EXISTS conversation_context (
            conversation_alias TEXT PRIMARY KEY,
            subject_type TEXT NOT NULL,
            messages_json TEXT NOT NULL,
            captured_at REAL NOT NULL
        );
        """
        with self._lock, self._connect() as connection:
            connection.executescript(schema)
            self._ensure_column(
                connection,
                "work_items",
                "id_source",
                "TEXT NOT NULL DEFAULT ''",
            )
            self._ensure_column(
                connection,
                "work_items",
                "display_name",
                "TEXT NOT NULL DEFAULT ''",
            )
            self._ensure_column(
                connection,
                "inbound_messages",
                "raw_json",
                "TEXT NOT NULL DEFAULT '{}'",
            )

    @staticmethod
    def _ensure_column(
        connection: sqlite3.Connection,
        table: str,
        column: str,
        declaration: str,
    ) -> None:
        """Add one backward-compatible SQLite column when it is missing."""
        columns = {
            str(row[1])
            for row in connection.execute(f"PRAGMA table_info({table})")
        }
        if column not in columns:
            connection.execute(
                f"ALTER TABLE {table} ADD COLUMN {column} {declaration}",
            )

    def get_setting(self, key: str, default: str = "") -> str:
        """Return one application setting."""
        with self._lock, self._connect() as connection:
            row = connection.execute(
                "SELECT value FROM settings WHERE key = ?",
                (key,),
            ).fetchone()
        return str(row["value"]) if row else default

    def set_setting(self, key: str, value: str) -> None:
        """Persist one application setting."""
        with self._lock, self._connect() as connection:
            connection.execute(
                """
                INSERT INTO settings(key, value) VALUES(?, ?)
                ON CONFLICT(key) DO UPDATE SET value = excluded.value
                """,
                (key, value),
            )

    def add_principal(
        self,
        *,
        subject_type: str,
        subject_id: str,
        id_source: str,
        display_name: str,
        conversation_alias: str,
        policy: str,
    ) -> dict[str, Any]:
        """Create or update one verified person or group."""
        if subject_type not in VALID_SUBJECT_TYPES:
            raise ValueError("Invalid subject type")
        if policy not in VALID_POLICIES:
            raise ValueError("Invalid policy")
        values = (
            subject_id.strip(),
            id_source.strip(),
            display_name.strip(),
            conversation_alias.strip(),
        )
        if not all(values):
            raise ValueError("Identity fields must not be empty")
        clean_id, clean_source, clean_name, clean_alias = values
        if not clean_source.startswith(("oauth:", "openapi:")):
            raise ValueError("Identity source must be OAuth or OpenAPI")
        now = time.time()
        with self._lock, self._connect() as connection:
            connection.execute(
                """
                INSERT INTO principals(
                    id, subject_type, subject_id, id_source, display_name,
                    conversation_alias, policy, created_at, updated_at
                ) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(subject_type, subject_id) DO UPDATE SET
                    id_source = excluded.id_source,
                    display_name = excluded.display_name,
                    conversation_alias = excluded.conversation_alias,
                    policy = excluded.policy,
                    updated_at = excluded.updated_at
                """,
                (
                    str(uuid.uuid4()),
                    subject_type,
                    clean_id,
                    clean_source,
                    clean_name,
                    clean_alias,
                    policy,
                    now,
                    now,
                ),
            )
            row = connection.execute(
                """
                SELECT * FROM principals
                WHERE subject_type = ? AND subject_id = ?
                """,
                (subject_type, clean_id),
            ).fetchone()
        return dict(row)

    def list_principals(self) -> list[dict[str, Any]]:
        """List configured identities with newest changes first."""
        with self._lock, self._connect() as connection:
            rows = connection.execute(
                "SELECT * FROM principals ORDER BY updated_at DESC",
            ).fetchall()
        return [dict(row) for row in rows]

    def delete_principal(self, principal_id: str) -> bool:
        """Delete one application-owned identity policy."""
        with self._lock, self._connect() as connection:
            cursor = connection.execute(
                "DELETE FROM principals WHERE id = ?",
                (principal_id,),
            )
        return cursor.rowcount > 0

    def update_principal_policy(
        self,
        principal_id: str,
        policy: str,
    ) -> dict[str, Any]:
        """Update policy while preserving the verified identity fields."""
        if policy not in VALID_POLICIES:
            raise ValueError("Invalid policy")
        with self._lock, self._connect() as connection:
            connection.execute(
                """
                UPDATE principals SET policy = ?, updated_at = ?
                WHERE id = ?
                """,
                (policy, time.time(), principal_id),
            )
            row = connection.execute(
                "SELECT * FROM principals WHERE id = ?",
                (principal_id,),
            ).fetchone()
        if row is None:
            raise KeyError(principal_id)
        return dict(row)

    def bind_pending(self, principal: dict[str, Any]) -> int:
        """Bind unresolved turns that exactly match one verified principal."""
        policy = str(principal["policy"])
        status = "collecting"
        if policy == "blocked":
            status = "blocked"
        elif policy == "observe":
            status = "observed"
        subject_type = str(principal["subject_type"])
        subject_id = str(principal["subject_id"])
        session_id = self._session_id(subject_type, subject_id)
        with self._lock, self._connect() as connection:
            cursor = connection.execute(
                """
                UPDATE work_items SET subject_id = ?, session_id = ?,
                    status = ?, updated_at = ?
                WHERE subject_id = ? AND subject_type = ?
                  AND status = 'identity_required'
                """,
                (
                    subject_id,
                    session_id,
                    status,
                    time.time(),
                    subject_id,
                    subject_type,
                ),
            )
        return cursor.rowcount

    def resolve_principal(
        self,
        subject_type: str,
        subject_id: str,
    ) -> dict[str, Any] | None:
        """Resolve only an exact real identity returned by DWS."""
        if not subject_id.strip():
            return None
        with self._lock, self._connect() as connection:
            rows = connection.execute(
                """
                SELECT * FROM principals
                WHERE subject_id = ? AND subject_type = ?
                """,
                (subject_id.strip(), subject_type),
            ).fetchall()
        return dict(rows[0]) if len(rows) == 1 else None

    def observe(
        self,
        *,
        source_key: str,
        conversation_alias: str,
        subject_type: str,
        subject_id: str,
        id_source: str,
        display_name: str,
        text: str,
        agent_id: str,
        quiet_seconds: float,
        max_wait_seconds: float,
        received_at: float | None = None,
        raw_message: dict[str, Any] | None = None,
    ) -> tuple[dict[str, Any], bool]:
        """Durably append one raw message to its active conversation turn."""
        clean_text = text.strip()
        if not source_key.strip() or not clean_text:
            raise ValueError("Message source and text must not be empty")
        principal = self.resolve_principal(
            subject_type,
            subject_id,
        )
        clean_subject_id = subject_id.strip()
        clean_id_source = id_source.strip()
        if not clean_subject_id:
            raise ValueError("Real DingTalk identity is required")
        if clean_id_source != "oauth:dws-event":
            raise ValueError("DWS OAuth event identity is required")
        if not display_name.strip():
            raise ValueError("DWS display name is required")
        raw_json = json.dumps(raw_message or {}, ensure_ascii=False)
        now = received_at or time.time()
        with self._lock, self._connect() as connection:
            duplicate = connection.execute(
                """
                SELECT work_item_id FROM inbound_messages
                WHERE source_key = ?
                """,
                (source_key,),
            ).fetchone()
            if duplicate:
                item = self._get_work_item(
                    connection,
                    duplicate["work_item_id"],
                )
                return self._with_messages(connection, item), False

            active = connection.execute(
                """
                SELECT * FROM work_items
                WHERE subject_type = ?
                  AND (
                    subject_id = ?
                  )
                  AND status IN (?, ?, ?)
                ORDER BY created_at DESC LIMIT 1
                """,
                (
                    subject_type,
                    clean_subject_id,
                    *ACTIVE_BATCH_STATES,
                ),
            ).fetchone()
            if active is None:
                item_id = str(uuid.uuid4())
                status, resolved_id = self._initial_status(
                    principal,
                    clean_subject_id,
                )
                session_id = self._session_id(subject_type, resolved_id)
                connection.execute(
                    """
                    INSERT INTO work_items(
                        id, conversation_alias, subject_type, subject_id,
                        id_source, display_name,
                        status, agent_id, session_id, quiet_deadline,
                        hard_deadline, created_at, updated_at
                    ) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    """,
                    (
                        item_id,
                        conversation_alias,
                        subject_type,
                        resolved_id,
                        clean_id_source,
                        display_name.strip() or conversation_alias,
                        status,
                        agent_id,
                        session_id,
                        now + quiet_seconds,
                        now + max_wait_seconds,
                        now,
                        now,
                    ),
                )
            else:
                item_id = str(active["id"])
                status = str(active["status"])
                if status == "agent_running":
                    status = "interrupt_requested"
                quiet_deadline = min(
                    float(active["hard_deadline"]),
                    now + quiet_seconds,
                )
                connection.execute(
                    """
                    UPDATE work_items SET status = ?, quiet_deadline = ?,
                        updated_at = ? WHERE id = ?
                    """,
                    (status, quiet_deadline, now, item_id),
                )

            row = connection.execute(
                "SELECT message_count FROM work_items WHERE id = ?",
                (item_id,),
            ).fetchone()
            ordinal = int(row["message_count"]) + 1
            connection.execute(
                """
                INSERT INTO inbound_messages(
                    id, source_key, work_item_id, conversation_alias,
                    text, raw_json, received_at, ordinal
                ) VALUES(?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    str(uuid.uuid4()),
                    source_key,
                    item_id,
                    conversation_alias,
                    clean_text,
                    raw_json,
                    now,
                    ordinal,
                ),
            )
            connection.execute(
                """
                UPDATE work_items SET message_count = message_count + 1,
                    updated_at = ? WHERE id = ?
                """,
                (now, item_id),
            )
            item = self._get_work_item(connection, item_id)
            result = self._with_messages(connection, item)
        self.add_activity(
            kind="inbound",
            status=str(result["status"]),
            title=f"收到来自 {conversation_alias} 的消息",
            detail=clean_text,
            work_item_id=item_id,
        )
        return result, True

    @staticmethod
    def _session_id(subject_type: str, subject_id: str | None) -> str:
        stable = subject_id or "identity-required"
        return f"pawapp:paw-me-dingtalk:{subject_type}:{stable}"

    @staticmethod
    def _initial_status(
        principal: dict[str, Any] | None,
        verified_subject_id: str,
    ) -> tuple[str, str | None]:
        if principal is None:
            return "identity_required", verified_subject_id
        subject_id = str(principal["subject_id"])
        if principal["policy"] == "blocked":
            return "blocked", subject_id
        if principal["policy"] == "observe":
            return "observed", subject_id
        return "collecting", subject_id

    @staticmethod
    def _get_work_item(
        connection: sqlite3.Connection,
        item_id: str,
    ) -> sqlite3.Row:
        row = connection.execute(
            "SELECT * FROM work_items WHERE id = ?",
            (item_id,),
        ).fetchone()
        if row is None:
            raise KeyError(item_id)
        return row

    @staticmethod
    def _with_messages(
        connection: sqlite3.Connection,
        row: sqlite3.Row,
    ) -> dict[str, Any]:
        result = dict(row)
        messages = connection.execute(
            """
            SELECT id, source_key, text, raw_json, received_at, ordinal
            FROM inbound_messages WHERE work_item_id = ?
            ORDER BY ordinal ASC
            """,
            (row["id"],),
        ).fetchall()
        result["messages"] = []
        for message in messages:
            item = dict(message)
            try:
                item["raw"] = json.loads(str(item.pop("raw_json")))
            except (TypeError, ValueError, json.JSONDecodeError):
                item["raw"] = {}
            result["messages"].append(item)
        result["text"] = "\n".join(message["text"] for message in messages)
        return result

    def update_work_item(
        self,
        item_id: str,
        *,
        status: str,
        response: str = "",
        error: str = "",
        subject_id: str | None = None,
    ) -> dict[str, Any]:
        """Move one work item to a new state."""
        now = time.time()
        with self._lock, self._connect() as connection:
            connection.execute(
                """
                UPDATE work_items SET status = ?, response = ?, error = ?,
                    subject_id = COALESCE(?, subject_id), updated_at = ?
                WHERE id = ?
                """,
                (status, response, error, subject_id, now, item_id),
            )
            row = self._get_work_item(connection, item_id)
            result = self._with_messages(connection, row)
        return result

    def finalize_agent_reply(
        self,
        item_id: str,
        text: str,
    ) -> dict[str, Any] | None:
        """Atomically keep a reply only if no newer message interrupted it."""
        clean_text = text.strip()
        if not clean_text:
            raise ValueError("Reply text must not be empty")
        now = time.time()
        with self._lock, self._connect() as connection:
            item = self._get_work_item(connection, item_id)
            if item["status"] != "agent_running":
                return None
            outbox_id = str(uuid.uuid4())
            connection.execute(
                """
                INSERT INTO outbox(
                    id, work_item_id, subject_id, conversation_alias,
                    text, status, created_at, updated_at
                ) VALUES(?, ?, ?, ?, ?, 'pending', ?, ?)
                ON CONFLICT(work_item_id) DO UPDATE SET
                    text = excluded.text, status = 'pending', error = '',
                    updated_at = excluded.updated_at
                """,
                (
                    outbox_id,
                    item_id,
                    item["subject_id"],
                    item["conversation_alias"],
                    clean_text,
                    now,
                    now,
                ),
            )
            connection.execute(
                """
                UPDATE work_items SET status = 'draft_ready', response = ?,
                    error = '', updated_at = ? WHERE id = ?
                """,
                (clean_text, now, item_id),
            )
            row = connection.execute(
                "SELECT * FROM outbox WHERE work_item_id = ?",
                (item_id,),
            ).fetchone()
        return dict(row)

    def recover_incomplete(self) -> int:
        """Recover interrupted process state without discarding input."""
        now = time.time()
        with self._lock, self._connect() as connection:
            cursor = connection.execute(
                """
                UPDATE work_items SET status = 'collecting',
                    quiet_deadline = ?, updated_at = ?
                WHERE status IN ('agent_running', 'interrupt_requested')
                """,
                (now, now),
            )
            connection.execute(
                """
                UPDATE outbox SET status = 'failed',
                    error = '发送进程中断，请确认后重试', updated_at = ?
                WHERE status = 'sending'
                """,
                (now,),
            )
        return cursor.rowcount

    def save_context(
        self,
        context_key: str,
        subject_type: str,
        messages: list[dict[str, Any]],
    ) -> None:
        """Persist the latest ordered semantic context for one conversation."""
        payload = json.dumps(messages, ensure_ascii=False)
        with self._lock, self._connect() as connection:
            connection.execute(
                """
                INSERT INTO conversation_context(
                    conversation_alias, subject_type, messages_json,
                    captured_at
                ) VALUES(?, ?, ?, ?)
                ON CONFLICT(conversation_alias) DO UPDATE SET
                    subject_type = excluded.subject_type,
                    messages_json = excluded.messages_json,
                    captured_at = excluded.captured_at
                """,
                (
                    context_key,
                    subject_type,
                    payload,
                    time.time(),
                ),
            )

    def append_context(
        self,
        context_key: str,
        subject_type: str,
        messages: list[dict[str, Any]],
        limit: int = 200,
    ) -> None:
        """Append real messages while retaining order and exact repeats."""
        existing = self.get_context(context_key)
        known_ids = {
            str(item.get("message_id"))
            for item in existing
            if item.get("message_id")
        }
        merged = list(existing)
        for message in messages:
            message_id = str(message.get("message_id") or "")
            if message_id and message_id in known_ids:
                continue
            merged.append(message)
            if message_id:
                known_ids.add(message_id)
        self.save_context(context_key, subject_type, merged[-limit:])

    def get_context(self, context_key: str) -> list[dict[str, Any]]:
        """Return the latest persisted semantic conversation context."""
        with self._lock, self._connect() as connection:
            row = connection.execute(
                """
                SELECT messages_json FROM conversation_context
                WHERE conversation_alias = ?
                """,
                (context_key,),
            ).fetchone()
        if row is None:
            return []
        payload = json.loads(str(row["messages_json"]))
        return payload if isinstance(payload, list) else []

    def get_work_item(self, item_id: str) -> dict[str, Any]:
        """Return one turn and all of its persisted raw messages."""
        with self._lock, self._connect() as connection:
            row = self._get_work_item(connection, item_id)
            return self._with_messages(connection, row)

    def due_work_items(self, now: float | None = None) -> list[dict[str, Any]]:
        """Return collected turns whose quiet or hard deadline has passed."""
        moment = now or time.time()
        with self._lock, self._connect() as connection:
            rows = connection.execute(
                """
                SELECT * FROM work_items
                WHERE status = 'collecting'
                  AND (quiet_deadline <= ? OR hard_deadline <= ?)
                ORDER BY created_at ASC
                """,
                (moment, moment),
            ).fetchall()
            return [self._with_messages(connection, row) for row in rows]

    def list_work_items(self, limit: int = 100) -> list[dict[str, Any]]:
        """List recent turns including every raw message."""
        safe_limit = max(1, min(limit, 500))
        with self._lock, self._connect() as connection:
            rows = connection.execute(
                """
                SELECT * FROM work_items
                ORDER BY updated_at DESC LIMIT ?
                """,
                (safe_limit,),
            ).fetchall()
            return [self._with_messages(connection, row) for row in rows]

    def create_outbox(
        self,
        *,
        work_item_id: str,
        subject_id: str,
        conversation_alias: str,
        text: str,
    ) -> dict[str, Any]:
        """Create or replace one editable pending reply for a turn."""
        now = time.time()
        with self._lock, self._connect() as connection:
            connection.execute(
                """
                INSERT INTO outbox(
                    id, work_item_id, subject_id, conversation_alias,
                    text, status, created_at, updated_at
                ) VALUES(?, ?, ?, ?, ?, 'pending', ?, ?)
                ON CONFLICT(work_item_id) DO UPDATE SET
                    text = excluded.text, status = 'pending', error = '',
                    updated_at = excluded.updated_at
                """,
                (
                    str(uuid.uuid4()),
                    work_item_id,
                    subject_id,
                    conversation_alias,
                    text.strip(),
                    now,
                    now,
                ),
            )
            row = connection.execute(
                "SELECT * FROM outbox WHERE work_item_id = ?",
                (work_item_id,),
            ).fetchone()
        return dict(row)

    def update_outbox(
        self,
        outbox_id: str,
        *,
        status: str,
        text: str | None = None,
        error: str = "",
    ) -> dict[str, Any]:
        """Update text or delivery state for one outbox entry."""
        now = time.time()
        sent_at = now if status == "sent" else None
        with self._lock, self._connect() as connection:
            connection.execute(
                """
                UPDATE outbox SET status = ?, text = COALESCE(?, text),
                    error = ?, updated_at = ?, sent_at = COALESCE(?, sent_at)
                WHERE id = ?
                """,
                (status, text, error, now, sent_at, outbox_id),
            )
            row = connection.execute(
                "SELECT * FROM outbox WHERE id = ?",
                (outbox_id,),
            ).fetchone()
        if row is None:
            raise KeyError(outbox_id)
        return dict(row)

    def get_outbox(self, outbox_id: str) -> dict[str, Any]:
        """Return one generated reply."""
        with self._lock, self._connect() as connection:
            row = connection.execute(
                "SELECT * FROM outbox WHERE id = ?",
                (outbox_id,),
            ).fetchone()
        if row is None:
            raise KeyError(outbox_id)
        return dict(row)

    def delete_outbox(self, outbox_id: str) -> bool:
        """Delete one pending or failed outbox entry."""
        with self._lock, self._connect() as connection:
            cursor = connection.execute(
                "DELETE FROM outbox WHERE id = ? AND status != 'sending'",
                (outbox_id,),
            )
        return cursor.rowcount > 0

    def list_outbox(self, limit: int = 100) -> list[dict[str, Any]]:
        """List recent generated replies."""
        safe_limit = max(1, min(limit, 500))
        with self._lock, self._connect() as connection:
            rows = connection.execute(
                """
                SELECT * FROM outbox
                ORDER BY updated_at DESC LIMIT ?
                """,
                (safe_limit,),
            ).fetchall()
        return [dict(row) for row in rows]

    def add_activity(
        self,
        *,
        kind: str,
        status: str,
        title: str,
        detail: str = "",
        work_item_id: str | None = None,
    ) -> None:
        """Append one audit event."""
        with self._lock, self._connect() as connection:
            connection.execute(
                """
                INSERT INTO activity(
                    kind, status, title, detail, work_item_id, created_at
                ) VALUES(?, ?, ?, ?, ?, ?)
                """,
                (
                    kind,
                    status,
                    title,
                    detail[:2000],
                    work_item_id,
                    time.time(),
                ),
            )

    def list_activity(self, limit: int = 200) -> list[dict[str, Any]]:
        """List the newest audit events."""
        safe_limit = max(1, min(limit, 1000))
        with self._lock, self._connect() as connection:
            rows = connection.execute(
                """
                SELECT * FROM activity
                ORDER BY id DESC LIMIT ?
                """,
                (safe_limit,),
            ).fetchall()
        return [dict(row) for row in rows]

    def snapshot(self) -> dict[str, Any]:
        """Return the data needed by the single-page UI."""
        return {
            "settings": {
                "enabled": self.get_setting("enabled", "false") == "true",
                "agent_id": self.get_setting("agent_id", "default"),
                "default_policy": self.get_setting(
                    "default_policy",
                    "draft",
                ),
                "quiet_seconds": float(
                    self.get_setting("quiet_seconds", "4.0"),
                ),
                "max_wait_seconds": float(
                    self.get_setting("max_wait_seconds", "20.0"),
                ),
            },
            "principals": self.list_principals(),
            "work_items": self.list_work_items(),
            "outbox": self.list_outbox(),
            "activity": self.list_activity(),
        }
