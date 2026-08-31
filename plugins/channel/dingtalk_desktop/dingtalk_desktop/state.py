# -*- coding: utf-8 -*-
"""Persistent draft storage for the DingTalk Desktop channel."""

from __future__ import annotations

import json
import os
import threading
import uuid
from pathlib import Path
from time import time

from .models import DraftRecord

_STORE_LOCK = threading.RLock()


class DraftStore:
    """Store agent replies until the user explicitly approves them."""

    def __init__(self, path: Path) -> None:
        self.path = path
        self._lock = _STORE_LOCK

    def list(self) -> list[DraftRecord]:
        """Return drafts ordered from oldest to newest."""
        with self._lock:
            records = self._read()
        return sorted(records, key=lambda item: item.created_at)

    def add(self, conversation: str, text: str) -> DraftRecord:
        """Persist one draft and return its generated identifier."""
        record = DraftRecord(
            id=uuid.uuid4().hex,
            conversation=conversation,
            text=text,
            created_at=time(),
        )
        with self._lock:
            records = self._read()
            records.append(record)
            self._write(records)
        return record

    def get(self, draft_id: str) -> DraftRecord | None:
        """Return one draft without changing the store."""
        return next(
            (item for item in self.list() if item.id == draft_id),
            None,
        )

    def remove(self, draft_id: str) -> bool:
        """Remove one draft and report whether it existed."""
        with self._lock:
            records = self._read()
            remaining = [item for item in records if item.id != draft_id]
            if len(remaining) == len(records):
                return False
            self._write(remaining)
            return True

    def _read(self) -> list[DraftRecord]:
        if not self.path.is_file():
            return []
        try:
            payload = json.loads(self.path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return []
        records: list[DraftRecord] = []
        for item in payload.get("drafts", []):
            try:
                records.append(DraftRecord(**item))
            except (TypeError, ValueError):
                continue
        return records

    def _write(self, records: list[DraftRecord]) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        temporary = self.path.with_suffix(f"{self.path.suffix}.tmp")
        payload = {"drafts": [item.as_dict() for item in records]}
        temporary.write_text(
            json.dumps(payload, ensure_ascii=False, indent=2),
            encoding="utf-8",
        )
        os.chmod(temporary, 0o600)
        temporary.replace(self.path)


def draft_store_path(workspace_dir: Path) -> Path:
    """Return the agent-scoped plugin state path."""
    return workspace_dir / ".qwenpaw" / "dingtalk-desktop-drafts.json"


__all__ = ["DraftStore", "draft_store_path"]
