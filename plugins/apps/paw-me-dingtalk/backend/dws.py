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
from typing import Any, AsyncIterator, Awaitable, Callable


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

    def __init__(self, runtime_dir: Path | None = None) -> None:
        self.runtime_dir = runtime_dir
        self._processes: set[asyncio.subprocess.Process] = set()
        self._integration_process: asyncio.subprocess.Process | None = None

    def executable(self) -> str:
        """Locate the app-managed DWS runtime or a legacy installation."""
        names = ["dws.exe", "dws"] if os.name == "nt" else ["dws"]
        if self.runtime_dir is not None:
            for name in names:
                candidate = self.runtime_dir / "bin" / name
                if candidate.is_file() and os.access(candidate, os.X_OK):
                    return str(candidate)
            return ""
        discovered = shutil.which("dws")
        if discovered:
            return discovered
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
        integration: bool = False,
    ) -> dict[str, Any]:
        executable = self.executable()
        if not executable:
            raise DwsError("未安装钉钉连接组件")
        process = await asyncio.create_subprocess_exec(
            executable,
            *args,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            env=self._environment(),
        )
        if integration:
            self._integration_process = process
        try:
            stdout, stderr = await asyncio.wait_for(
                process.communicate(),
                timeout=timeout,
            )
        except TimeoutError as exc:
            await self._terminate(process)
            raise DwsError("钉钉授权等待超时，请重新连接") from exc
        except asyncio.CancelledError:
            await self._terminate(process)
            raise
        finally:
            if self._integration_process is process:
                self._integration_process = None
        if process.returncode != 0:
            detail = self._safe_error(stderr or stdout)
            raise DwsError(detail or "钉钉连接请求失败")
        try:
            payload = json.loads(stdout.decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError) as exc:
            raise DwsError("钉钉连接组件返回了无效结果") from exc
        if not isinstance(payload, dict):
            raise DwsError("钉钉连接组件返回了无效对象")
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
        return "钉钉认证或请求失败，请重新连接后重试"

    async def status(self) -> DwsStatus:
        """Read the official OAuth status without accessing token storage."""
        executable = self.executable()
        if not executable:
            return DwsStatus(
                available=False,
                authenticated=False,
                detail="未安装钉钉连接组件",
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

    def _environment(self) -> dict[str, str]:
        """Return an isolated environment for the managed runtime."""
        environment = dict(os.environ)
        if self.runtime_dir is not None:
            environment["DWS_CONFIG_DIR"] = str(
                self.runtime_dir / "config",
            )
        return environment

    async def install(
        self,
        progress: Callable[[str, str, int | None], Awaitable[None]],
    ) -> None:
        """Install the official binary into this PawApp's runtime directory."""
        if self.runtime_dir is None:
            raise DwsError("未配置 Paw Me 运行目录")
        url = INSTALL_URLS["nt" if os.name == "nt" else "posix"]
        suffix = ".ps1" if os.name == "nt" else ".sh"
        with tempfile.TemporaryDirectory(prefix="paw-me-dws-") as folder:
            script = Path(folder) / f"install{suffix}"
            await progress("downloading", "正在下载官方连接组件", 0)
            await asyncio.to_thread(
                self._download_installer,
                url,
                script,
                progress,
                asyncio.get_running_loop(),
            )
            await progress("preparing", "正在准备独立运行环境", None)
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
            install_dir = self.runtime_dir / "bin"
            install_dir.mkdir(parents=True, exist_ok=True)
            environment = self._environment()
            environment["DWS_INSTALL_DIR"] = str(install_dir)
            environment["DWS_NO_SKILLS"] = "1"
            environment[
                "DWS_GITEE_REPO"
            ] = "DingTalk-Real-AI/dingtalk-workspace-cli"
            process = await asyncio.create_subprocess_exec(
                *command,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
                env=environment,
            )
            self._integration_process = process
            try:
                stdout_task = asyncio.create_task(
                    self._observe_install_output(process, progress),
                )
                stderr = await process.stderr.read() if process.stderr else b""
                await process.wait()
                stdout = await stdout_task
            except asyncio.CancelledError:
                await self._terminate(process)
                raise
            finally:
                if self._integration_process is process:
                    self._integration_process = None
            if process.returncode != 0:
                detail = self._safe_error(stderr or stdout)
                raise DwsError(detail or "钉钉连接组件安装失败")
        if not self.executable():
            raise DwsError("安装程序已结束，但仍未找到 dws 可执行文件")
        await progress("ready", "钉钉连接组件已就绪", 100)

    @staticmethod
    def _download_installer(
        url: str,
        destination: Path,
        progress: Callable[[str, str, int | None], Awaitable[None]],
        loop: asyncio.AbstractEventLoop,
    ) -> None:
        """Download the official installer and report real byte progress."""
        with urllib.request.urlopen(url, timeout=30) as response:
            total = int(response.headers.get("Content-Length") or 0)
            downloaded = 0
            with destination.open("wb") as target:
                while True:
                    chunk = response.read(64 * 1024)
                    if not chunk:
                        break
                    target.write(chunk)
                    downloaded += len(chunk)
                    percent = int(downloaded * 100 / total) if total else None
                    asyncio.run_coroutine_threadsafe(
                        progress(
                            "downloading",
                            f"已下载 {downloaded} 字节",
                            percent,
                        ),
                        loop,
                    ).result()

    @staticmethod
    async def _observe_install_output(
        process: asyncio.subprocess.Process,
        progress: Callable[[str, str, int | None], Awaitable[None]],
    ) -> bytes:
        """Map installer output to truthful, non-fabricated stages."""
        if process.stdout is None:
            return b""
        captured = bytearray()
        while True:
            line = await process.stdout.readline()
            if not line:
                return bytes(captured)
            captured.extend(line)
            text = line.decode("utf-8", errors="replace")
            if "Downloading" in text:
                await progress("installing", "正在下载连接运行时", None)
            elif "checksum verified" in text.lower():
                await progress("verifying", "官方校验已通过", None)
            elif "Extracting" in text:
                await progress("installing", "正在安装连接运行时", None)
            elif "Binary installed" in text:
                await progress("verifying", "正在验证安装结果", None)

    async def login(self) -> DwsStatus:
        """Open the official OAuth login and wait for its completion."""
        await self._run_json(
            "auth",
            "login",
            "--format",
            "json",
            timeout=120.0,
            integration=True,
        )
        status = await self.status()
        if not status.authenticated:
            raise DwsError(status.detail or "钉钉 OAuth 登录未完成")
        return status

    async def logout(self, status: DwsStatus) -> None:
        """Log out only the exact OAuth account shown for confirmation."""
        if not status.corp_id or not status.user_id:
            raise DwsError("当前钉钉账号缺少可验证的组织或用户 ID")
        await self._run_json(
            "auth",
            "logout",
            "--profile",
            f"{status.corp_id}:{status.user_id}",
            "--yes",
            "--format",
            "json",
        )

    async def cancel_integration(self) -> None:
        """Cancel the currently tracked installer or OAuth process."""
        process = self._integration_process
        if process is not None:
            await self._terminate(process)

    async def events(self, kind: str) -> AsyncIterator[DwsMessageEvent]:
        """Yield personal IM events from one official DWS Stream."""
        if kind not in {"all-direct", "all-group"}:
            raise ValueError(f"Unsupported DWS event kind: {kind}")
        executable = self.executable()
        if not executable:
            raise DwsError("未安装钉钉连接组件")
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
            env=self._environment(),
        )
        self._processes.add(process)
        stderr_task = asyncio.create_task(self._drain_stderr(process))
        try:
            if process.stdout is None:
                raise DwsError("钉钉消息流输出不可用")
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

    async def group_members(self, conversation_id: str) -> dict[str, Any]:
        """Read real group members to identify the OAuth account owner."""
        if not conversation_id.strip():
            raise DwsError("群会话缺少真实 openConversationId")
        return await self._run_json(
            "chat",
            "group",
            "members",
            "--id",
            conversation_id,
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
