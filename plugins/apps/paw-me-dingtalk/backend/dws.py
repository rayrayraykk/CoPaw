# -*- coding: utf-8 -*-
"""DingTalk Workspace CLI OAuth and personal-event integration."""

from __future__ import annotations

import asyncio
import json
import os
import shutil
import signal
import tempfile
import urllib.request
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any, AsyncIterator


INSTALL_URLS = {
    "posix": (
        "https://gitee.com/DingTalk-Real-AI/"
        "dingtalk-workspace-cli/raw/main/scripts/install.sh"
    ),
    "nt": (
        "https://gitee.com/DingTalk-Real-AI/"
        "dingtalk-workspace-cli/raw/main/scripts/install.ps1"
    ),
}


@dataclass(frozen=True)
class DwsStatus:
    """Public, credential-free OAuth status."""

    available: bool
    authenticated: bool
    executable: str = ""
    version: str = ""
    corp_id: str = ""
    corp_name: str = ""
    user_id: str = ""
    user_name: str = ""
    detail: str = ""

    def as_dict(self) -> dict[str, Any]:
        """Return a JSON-compatible status object."""
        return asdict(self)


@dataclass(frozen=True)
class DwsMessageEvent:
    """One stable flattened DingTalk personal message event."""

    event_id: str
    event_type: str
    message_id: str
    conversation_id: str
    sender: str
    sender_open_dingtalk_id: str
    content: str
    create_time: str
    timestamp: int
    raw: dict[str, Any]

    @property
    def subject_type(self) -> str:
        """Return the authorization subject type."""
        if "group" in self.event_type:
            return "group"
        return "person"

    @property
    def subject_id(self) -> str:
        """Return the real authorization and send target identifier."""
        if self.subject_type == "group":
            return self.conversation_id
        return self.sender_open_dingtalk_id

    @property
    def display_name(self) -> str:
        """Return a human label without inventing an identity."""
        return self.sender or self.subject_id


class DwsError(RuntimeError):
    """A safe DWS invocation failure without credential output."""


class DwsClient:
    """Run official DWS commands without reading or storing OAuth tokens."""

    def __init__(self) -> None:
        self._processes: set[asyncio.subprocess.Process] = set()

    @staticmethod
    def executable() -> str:
        """Locate DWS in PATH and common user installation locations."""
        discovered = shutil.which("dws")
        if discovered:
            return discovered
        names = ["dws.exe", "dws"] if os.name == "nt" else ["dws"]
        roots = [
            Path.home() / ".local" / "bin",
            Path.home() / "bin",
            Path("/usr/local/bin"),
            Path("/opt/homebrew/bin"),
        ]
        for root in roots:
            for name in names:
                candidate = root / name
                if candidate.is_file() and os.access(candidate, os.X_OK):
                    return str(candidate)
        return ""

    async def _run_json(
        self,
        *args: str,
        timeout: float = 30.0,
    ) -> dict[str, Any]:
        executable = self.executable()
        if not executable:
            raise DwsError("未安装钉钉 DWS")
        process = await asyncio.create_subprocess_exec(
            executable,
            *args,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        try:
            stdout, stderr = await asyncio.wait_for(
                process.communicate(),
                timeout=timeout,
            )
        except TimeoutError as exc:
            await self._terminate(process)
            raise DwsError("DWS 请求超时") from exc
        if process.returncode != 0:
            detail = self._safe_error(stderr or stdout)
            raise DwsError(detail or "DWS 请求失败")
        try:
            payload = json.loads(stdout.decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError) as exc:
            raise DwsError("DWS 未返回可解析的 JSON") from exc
        if not isinstance(payload, dict):
            raise DwsError("DWS 返回了非对象 JSON")
        return payload

    @staticmethod
    def _safe_error(raw: bytes) -> str:
        """Return one bounded error line without exposing token material."""
        text = raw.decode("utf-8", errors="replace")
        lines = [line.strip() for line in text.splitlines() if line.strip()]
        for line in reversed(lines):
            lowered = line.lower()
            if "token" not in lowered and "secret" not in lowered:
                return line[:500]
        return "DWS 认证或请求失败，请重新登录后重试"

    async def status(self) -> DwsStatus:
        """Read the official OAuth status without accessing token storage."""
        executable = self.executable()
        if not executable:
            return DwsStatus(
                available=False,
                authenticated=False,
                detail="未安装钉钉 DWS",
            )
        version = ""
        try:
            version_data = await self._run_json(
                "version",
                "--format",
                "json",
            )
            version = str(
                version_data.get("version")
                or version_data.get("Version")
                or "",
            )
        except DwsError:
            pass
        try:
            data = await self._run_json(
                "auth",
                "status",
                "--format",
                "json",
            )
        except DwsError as exc:
            return DwsStatus(
                available=True,
                authenticated=False,
                executable=executable,
                version=version,
                detail=str(exc),
            )
        authenticated = bool(data.get("authenticated"))
        detail = (
            "钉钉 OAuth 已连接"
            if authenticated
            else str(data.get("message") or "需要完成钉钉 OAuth 登录")
        )
        return DwsStatus(
            available=True,
            authenticated=authenticated,
            executable=executable,
            version=version,
            corp_id=str(data.get("corp_id") or ""),
            corp_name=str(data.get("corp_name") or ""),
            user_id=str(data.get("user_id") or ""),
            user_name=str(data.get("user_name") or ""),
            detail=detail,
        )

    async def install(self) -> None:
        """Run the official installer after explicit UI authorization."""
        url = INSTALL_URLS["nt" if os.name == "nt" else "posix"]
        suffix = ".ps1" if os.name == "nt" else ".sh"
        with tempfile.TemporaryDirectory(prefix="paw-me-dws-") as folder:
            script = Path(folder) / f"install{suffix}"
            await asyncio.to_thread(urllib.request.urlretrieve, url, script)
            if os.name == "nt":
                command = [
                    "powershell",
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    str(script),
                ]
            else:
                command = ["sh", str(script)]
            process = await asyncio.create_subprocess_exec(
                *command,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
            )
            stdout, stderr = await process.communicate()
            if process.returncode != 0:
                detail = self._safe_error(stderr or stdout)
                raise DwsError(detail or "DWS 安装失败")
        if not self.executable():
            raise DwsError("安装程序已结束，但仍未找到 dws 可执行文件")

    async def login(self) -> DwsStatus:
        """Open the official OAuth login and wait for its completion."""
        await self._run_json(
            "auth",
            "login",
            "--format",
            "json",
            timeout=600.0,
        )
        status = await self.status()
        if not status.authenticated:
            raise DwsError(status.detail or "钉钉 OAuth 登录未完成")
        return status

    async def events(self, kind: str) -> AsyncIterator[DwsMessageEvent]:
        """Yield personal IM events from one official DWS Stream."""
        if kind not in {"all-direct", "all-group"}:
            raise ValueError(f"Unsupported DWS event kind: {kind}")
        executable = self.executable()
        if not executable:
            raise DwsError("未安装钉钉 DWS")
        process = await asyncio.create_subprocess_exec(
            executable,
            "event",
            "+listen-im",
            "--kind",
            kind,
            "--events",
            "message",
            "--format",
            "ndjson",
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        self._processes.add(process)
        stderr_task = asyncio.create_task(self._drain_stderr(process))
        try:
            if process.stdout is None:
                raise DwsError("DWS 事件流 stdout 不可用")
            while True:
                raw = await process.stdout.readline()
                if not raw:
                    break
                try:
                    payload = json.loads(raw.decode("utf-8"))
                except (UnicodeDecodeError, json.JSONDecodeError):
                    continue
                event = self.parse_event(payload)
                if event is not None:
                    yield event
            return_code = await process.wait()
            if return_code != 0:
                detail = await stderr_task
                raise DwsError(detail or f"DWS {kind} 事件流已退出")
        finally:
            self._processes.discard(process)
            if process.returncode is None:
                await self._terminate(process)
            if not stderr_task.done():
                stderr_task.cancel()
            await asyncio.gather(stderr_task, return_exceptions=True)

    @staticmethod
    async def _drain_stderr(
        process: asyncio.subprocess.Process,
    ) -> str:
        if process.stderr is None:
            return ""
        last_safe = ""
        while True:
            raw = await process.stderr.readline()
            if not raw:
                return last_safe
            safe = DwsClient._safe_error(raw)
            if safe and not safe.startswith("[event] ready"):
                last_safe = safe

    @staticmethod
    def parse_event(payload: Any) -> DwsMessageEvent | None:
        """Validate a flattened DWS message event without inventing IDs."""
        if not isinstance(payload, dict):
            return None
        event_type = str(payload.get("type") or "").strip()
        event_id = str(payload.get("event_id") or "").strip()
        message_id = str(payload.get("message_id") or "").strip()
        conversation_id = str(payload.get("conversation_id") or "").strip()
        sender_id = str(payload.get("sender_open_dingtalk_id") or "").strip()
        content = str(payload.get("content") or "").strip()
        is_group = "group" in event_type
        subject_id = conversation_id if is_group else sender_id
        if not all((event_type, event_id, message_id, subject_id, content)):
            return None
        return DwsMessageEvent(
            event_id=event_id,
            event_type=event_type,
            message_id=message_id,
            conversation_id=conversation_id,
            sender=str(payload.get("sender") or "").strip(),
            sender_open_dingtalk_id=sender_id,
            content=content,
            create_time=str(payload.get("create_time") or "").strip(),
            timestamp=DwsClient._safe_int(payload.get("timestamp")),
            raw=payload,
        )

    @staticmethod
    def _safe_int(value: Any) -> int:
        try:
            return int(value or 0)
        except (TypeError, ValueError):
            return 0

    async def history(
        self,
        subject_type: str,
        subject_id: str,
        limit: int = 80,
    ) -> dict[str, Any]:
        """Read bounded recent context for one verified event target."""
        target_flag = (
            "--conversation-id"
            if subject_type == "group"
            else "--open-dingtalk-id"
        )
        return await self._run_json(
            "chat",
            "message",
            "list",
            target_flag,
            subject_id,
            "--limit",
            str(max(1, min(limit, 100))),
            "--format",
            "json",
        )

    async def send(
        self,
        *,
        subject_type: str,
        subject_id: str,
        text: str,
        idempotency_key: str,
    ) -> dict[str, Any]:
        """Send once as the OAuth user with a stable idempotency key."""
        target_flag = (
            "--conversation-id"
            if subject_type == "group"
            else "--open-dingtalk-id"
        )
        return await self._run_json(
            "chat",
            "message",
            "send",
            target_flag,
            subject_id,
            "--content",
            text,
            "--ai-tag=false",
            "--idempotency-key",
            idempotency_key,
            "--format",
            "json",
            timeout=60.0,
        )

    async def stop(self) -> None:
        """Gracefully close every owned event subscription process."""
        processes = list(self._processes)
        for process in processes:
            await self._terminate(process)

    @staticmethod
    async def _terminate(process: asyncio.subprocess.Process) -> None:
        if process.returncode is not None:
            return
        if process.stdin is not None:
            process.stdin.close()
        try:
            await asyncio.wait_for(process.wait(), timeout=3.0)
            return
        except TimeoutError:
            pass
        if os.name == "nt":
            process.terminate()
        else:
            process.send_signal(signal.SIGTERM)
        try:
            await asyncio.wait_for(process.wait(), timeout=3.0)
        except TimeoutError:
            process.kill()
            await process.wait()
