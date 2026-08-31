# -*- coding: utf-8 -*-
"""macOS adapter for the locally installed DingTalk desktop client."""

from __future__ import annotations

import json
import hashlib
import os
import platform
import plistlib
import subprocess
import tempfile
from pathlib import Path
from typing import Any

from .models import (
    DialogueMessage,
    DesktopMessage,
    DesktopStatus,
)

DEFAULT_APP_PATH = Path("/Applications/iDingTalk.app")
DEFAULT_BUNDLE_ID = "dd.work.exclusive4aliding"


class DingTalkDesktopError(RuntimeError):
    """Raised when the verified desktop operation cannot be completed."""


class DingTalkDesktopDriver:
    """Invoke a bundled JXA bridge without accessing DingTalk credentials."""

    def __init__(
        self,
        bundle_id: str = DEFAULT_BUNDLE_ID,
        app_path: Path = DEFAULT_APP_PATH,
        bridge_path: Path | None = None,
    ) -> None:
        self.bundle_id = bundle_id
        self.app_path = app_path
        self.bridge_path = bridge_path or Path(__file__).with_name(
            "bridge.js",
        )
        self._history_cache: dict[str, list[DialogueMessage]] = {}
        self.ax_source = Path(__file__).with_name("ax_history.swift")

    def _run(
        self,
        action: str,
        payload: dict[str, Any] | None = None,
        timeout: float = 30.0,
    ) -> dict[str, Any]:
        request = {
            "action": action,
            "bundle_id": self.bundle_id,
            **(payload or {}),
        }
        process = subprocess.run(
            ["osascript", "-l", "JavaScript", str(self.bridge_path)],
            input=json.dumps(request, ensure_ascii=False),
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
        if process.returncode != 0:
            detail = (process.stderr or process.stdout).strip()
            raise DingTalkDesktopError(
                f"DingTalk accessibility bridge failed: {detail}",
            )
        try:
            response = json.loads(process.stdout)
        except json.JSONDecodeError as exc:
            raise DingTalkDesktopError(
                "DingTalk accessibility bridge returned invalid JSON",
            ) from exc
        if not response.get("ok"):
            raise DingTalkDesktopError(
                str(response.get("error") or "DingTalk operation failed"),
            )
        result = response.get("result")
        return result if isinstance(result, dict) else {}

    def status(self) -> DesktopStatus:
        """Return install, process, Accessibility, and login readiness."""
        supported = platform.system() == "Darwin"
        installed = self.app_path.is_dir()
        version = self._installed_version() if installed else ""
        if not supported:
            return DesktopStatus(
                supported=False,
                installed=installed,
                running=False,
                accessibility=False,
                logged_in=False,
                bundle_id=self.bundle_id,
                version=version,
                detail="DingTalk Desktop currently supports macOS only.",
            )
        try:
            result = self._run("status", timeout=8.0)
        except (DingTalkDesktopError, subprocess.TimeoutExpired) as exc:
            return DesktopStatus(
                supported=True,
                installed=installed,
                running=False,
                accessibility=False,
                logged_in=False,
                bundle_id=self.bundle_id,
                version=version,
                detail=str(exc),
            )
        return DesktopStatus(
            supported=True,
            installed=installed,
            running=bool(result.get("running")),
            accessibility=bool(result.get("accessibility")),
            logged_in=bool(result.get("logged_in")),
            bundle_id=self.bundle_id,
            version=version,
        )

    def current_conversation(self) -> str:
        """Return the visible conversation title."""
        result = self._run("current")
        return str(result.get("conversation") or "").strip()

    def read_latest(self, conversation: str) -> DesktopMessage | None:
        """Read the latest message from the exact visible conversation."""
        result = self._run(
            "read",
            {"conversation": conversation},
            timeout=45.0,
        )
        message = result.get("message")
        if not isinstance(message, dict):
            return None
        text = str(message.get("text") or "").strip()
        title = str(message.get("conversation") or "").strip()
        if not text or title != conversation:
            return None
        return DesktopMessage(
            conversation=title,
            text=text,
            incoming=bool(message.get("incoming")),
        )

    def read_context(
        self,
        conversation: str,
        limit: int = 16,
    ) -> list[DialogueMessage]:
        """Read semantically directed messages from one visible chat."""
        safe_limit = max(4, min(limit, 30))
        cached = self._history_cache.get(conversation, [])
        try:
            messages = self._read_native_context(conversation, safe_limit)
        except (DingTalkDesktopError, subprocess.TimeoutExpired):
            messages = list(cached)
        if not messages and cached:
            messages = list(cached)
        messages = messages[-safe_limit:]
        self._history_cache[conversation] = messages
        return list(messages)

    def _read_native_context(
        self,
        conversation: str,
        limit: int,
    ) -> list[DialogueMessage]:
        if self.current_conversation() != conversation:
            return []
        status = self._run("status", timeout=8.0)
        helper = self._native_helper()
        process = subprocess.run(
            [str(helper)],
            input=json.dumps(
                {"pid": int(status.get("pid") or 0), "limit": limit},
            ),
            capture_output=True,
            text=True,
            timeout=15.0,
            check=False,
        )
        if process.returncode != 0:
            raise DingTalkDesktopError(
                f"Native AX history reader failed ({process.returncode})",
            )
        try:
            payload = json.loads(process.stdout)
        except json.JSONDecodeError as exc:
            raise DingTalkDesktopError(
                "Native AX history reader returned invalid JSON",
            ) from exc
        if not payload.get("ok"):
            raise DingTalkDesktopError(str(payload.get("error") or "AX error"))
        if self.current_conversation() != conversation:
            return []
        return [
            DialogueMessage(
                text=str(item.get("text") or "").strip(),
                incoming=bool(item.get("incoming")),
            )
            for item in payload.get("messages") or []
            if str(item.get("text") or "").strip()
        ]

    def _native_helper(self) -> Path:
        digest = hashlib.sha256(self.ax_source.read_bytes()).hexdigest()[:12]
        helper = Path(tempfile.gettempdir()) / (
            f"qwenpaw-dingtalk-ax-{os.getuid()}-{digest}"
        )
        if helper.is_file():
            return helper
        process = subprocess.run(
            ["xcrun", "swiftc", str(self.ax_source), "-o", str(helper)],
            capture_output=True,
            text=True,
            timeout=60.0,
            check=False,
        )
        if process.returncode != 0:
            raise DingTalkDesktopError(
                "The macOS Swift compiler is required for AX history",
            )
        os.chmod(helper, 0o700)
        return helper

    def send(self, conversation: str, text: str) -> None:
        """Send text after the bridge verifies the exact conversation."""
        clean_text = text.strip()
        if not conversation.strip() or not clean_text:
            raise DingTalkDesktopError(
                "Conversation and message text must be non-empty",
            )
        self._run(
            "send",
            {"conversation": conversation, "text": clean_text},
            timeout=30.0,
        )
        history = self._history_cache.setdefault(conversation, [])
        history.append(DialogueMessage(text=clean_text, incoming=False))

    def database_path(self) -> Path | None:
        """Find the active encrypted database for change detection only."""
        root = Path.home() / "Library" / "Application Support" / "iDingTalk"
        candidates = list(root.glob("*_v3/DBFiles/dingtalk.db"))
        existing = [path for path in candidates if path.is_file()]
        if not existing:
            return None
        return max(existing, key=lambda path: path.stat().st_mtime_ns)

    def _installed_version(self) -> str:
        info_path = self.app_path / "Contents" / "Info.plist"
        try:
            with info_path.open("rb") as stream:
                info = plistlib.load(stream)
        except (OSError, plistlib.InvalidFileException):
            return ""
        return str(info.get("CFBundleShortVersionString") or "")


__all__ = [
    "DEFAULT_APP_PATH",
    "DEFAULT_BUNDLE_ID",
    "DingTalkDesktopDriver",
    "DingTalkDesktopError",
]
