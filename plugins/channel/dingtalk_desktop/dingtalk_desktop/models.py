# -*- coding: utf-8 -*-
"""Typed desktop bridge and draft records."""

from __future__ import annotations

from dataclasses import asdict, dataclass
from typing import Any


@dataclass(frozen=True)
class DesktopStatus:
    """Local DingTalk installation and session status."""

    supported: bool
    installed: bool
    running: bool
    accessibility: bool
    logged_in: bool
    bundle_id: str
    version: str = ""
    detail: str = ""

    def as_dict(self) -> dict[str, Any]:
        """Return a JSON-compatible representation."""
        return asdict(self)


@dataclass(frozen=True)
class DesktopMessage:
    """Latest visible message from one conversation."""

    conversation: str
    text: str
    incoming: bool


@dataclass(frozen=True)
class DialogueMessage:
    """One semantically directed message from the visible chat history."""

    text: str
    incoming: bool


@dataclass(frozen=True)
class DraftRecord:
    """Pending agent reply stored for explicit approval."""

    id: str
    conversation: str
    text: str
    created_at: float

    def as_dict(self) -> dict[str, Any]:
        """Return a JSON-compatible representation."""
        return asdict(self)
