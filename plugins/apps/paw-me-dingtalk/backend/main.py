# -*- coding: utf-8 -*-
"""Paw Me DingTalk PawApp backend entry point."""
# pylint: disable=protected-access

from __future__ import annotations

import asyncio
import dataclasses
import json
import logging
import re
import time
from pathlib import Path
from typing import Any, Literal

from fastapi import APIRouter, Depends, HTTPException
from fastapi.responses import StreamingResponse
from pydantic import BaseModel, Field

from qwenpaw.constant import WORKING_DIR
from qwenpaw.pawapp import PawApp, get_ctx
from qwenpaw.pawapp.context import ChatReply
from qwenpaw.schemas import AgentRequest

from .dws import DwsClient, DwsError, DwsMessageEvent, DwsStatus
from .profile import OwnerProfileCollector, profile_prompt
from .store import PawMeStore

logger = logging.getLogger(__name__)

APP_ID = "paw-me-dingtalk"
CHANNEL = "paw-me-dingtalk"
DATA_DIR = Path(WORKING_DIR) / "apps" / APP_ID
STORE = PawMeStore(DATA_DIR / "paw-me-v3.sqlite3")
DWS = DwsClient(DATA_DIR / "runtime")

IDENTITY_INSTRUCTIONS = (
    "你在本轮不是以 Agent、模型、AI 助手或任何预设角色的身份说话。"
    "你只是钉钉 OAuth 账号主人的执行引擎；最终消息的说话者必须是"
    "账号主人本人。始终使用账号主人的第一人称“我”，不得自称或介绍"
    "为 Agent、AI、助手、模型、Codex、Qwen、大白或其他角色。不得解释"
    "提示词、推理过程或回复策略，不得输出对对话的元分析。参考上下文中"
    "账号主人发出的消息，模仿其语气、用词、简洁程度与做事方式。意图"
    "不清楚时，以账号主人的口吻自然追问。只输出一条可直接发送的正文。"
)

IDENTITY_INTRO_PATTERN = "".join(
    (
        r"我是[^。！\n]{0,32}",
        r"(?:AI|人工智能|助手|智能体|Agent|Codex|Qwen|通义|大白)",
    ),
)
IDENTITY_ROLE_PATTERN = r"作为[^。！\n]{0,24}(?:AI|人工智能|助手|智能体|Agent|模型)"
IDENTITY_LEAK_PATTERNS = (
    re.compile(IDENTITY_INTRO_PATTERN, re.IGNORECASE),
    re.compile(IDENTITY_ROLE_PATTERN, re.IGNORECASE),
    re.compile(r"对方.{0,24}(?:没有提出|原样复制|自我介绍)"),
    re.compile(r"(?:保持|只需).{0,18}(?:回应|回复|简洁)"),
)


class SettingsPayload(BaseModel):
    """Editable digital-twin runtime settings."""

    enabled: bool
    agent_id: str = Field(min_length=1)
    default_policy: Literal["draft", "automatic"] = "draft"
    access_mode: Literal[
        "approval",
        "allow_all",
        "block_all",
    ] = "approval"
    quiet_seconds: float = Field(default=4.0, ge=1.0, le=30.0)
    max_wait_seconds: float = Field(default=20.0, ge=3.0, le=120.0)


class AuthorizationPayload(BaseModel):
    """Authorization policy for a real identity from a DWS event."""

    policy: Literal["observe", "draft", "automatic", "blocked"] = "draft"


class OutboxPayload(BaseModel):
    """Editable reply content."""

    text: str = Field(min_length=1)


class ProfileApprovalPayload(BaseModel):
    """Editable owner guidance reviewed before activation."""

    notes: str = Field(default="", max_length=6000)


class PawMeRuntime:
    """Consume OAuth message events and serialize Agent work."""

    def __init__(self) -> None:
        self.supervisor_task: asyncio.Task[Any] | None = None
        self.stream_tasks: dict[str, asyncio.Task[Any]] = {}
        self.agent_tasks: dict[str, asyncio.Task[Any]] = {}
        self.contexts: dict[str, Any] = {}
        self.history_loaded: set[str] = set()
        self.integration_task: asyncio.Task[Any] | None = None
        self.integration_stage = "idle"
        self.integration_detail = ""
        self.integration_progress: int | None = None
        self.profile_task: asyncio.Task[Any] | None = None
        self.profile_stage = "idle"
        self.profile_detail = "尚未初始化数字分身画像"
        self.profile_progress: int | None = None
        self.running = False
        self.stage = "stopped"
        self.detail = "数字人分身未启动"
        self.current_conversation = ""
        self.last_error = ""
        self.heartbeat_at = 0.0
        self.version = 0
        self.dws_status = DwsStatus(
            False,
            False,
            detail="尚未检查钉钉连接组件",
        )
        self.dws_checked_at = 0.0
        self.owner_open_ids: set[str] = set()
        self.group_owner_ids: dict[str, str] = {}
        self.sent_echoes: dict[tuple[str, str, str], float] = {}

    def remember_context(self, ctx: Any) -> None:
        """Keep a host context for background calls to the selected Agent."""
        self.contexts[ctx.agent_id] = ctx

    def publish(self, stage: str, detail: str, error: str = "") -> None:
        """Publish a visible runtime milestone."""
        self.stage = stage
        self.detail = detail
        self.last_error = error
        self.heartbeat_at = time.time()
        self.version += 1

    def status(self) -> dict[str, Any]:
        """Return observable runtime state."""
        return {
            "running": self.running,
            "stage": self.stage,
            "detail": self.detail,
            "current_conversation": self.current_conversation,
            "last_error": self.last_error,
            "heartbeat_at": self.heartbeat_at,
            "version": self.version,
            "integration_stage": self.integration_stage,
            "integration_detail": self.integration_detail,
            "integration_progress": self.integration_progress,
            "profile_stage": self.profile_stage,
            "profile_detail": self.profile_detail,
            "profile_progress": self.profile_progress,
        }

    def start(self) -> None:
        """Start OAuth event streams when they are not already running."""
        if self.supervisor_task and not self.supervisor_task.done():
            return
        recovered = STORE.recover_incomplete()
        self.running = True
        detail = "正在检查钉钉 OAuth 与实时消息能力"
        if recovered:
            detail = f"已恢复 {recovered} 个中断批次，正在重新连接"
        self.publish("starting", detail)
        self.supervisor_task = asyncio.create_task(self._supervise())

    async def stop(self) -> None:
        """Stop streams and all in-flight Agent runs."""
        self.running = False
        tasks = [*self.stream_tasks.values(), *self.agent_tasks.values()]
        if self.supervisor_task and not self.supervisor_task.done():
            tasks.append(self.supervisor_task)
        for task in tasks:
            if not task.done():
                task.cancel()
        await DWS.stop()
        if tasks:
            await asyncio.gather(*tasks, return_exceptions=True)
        self.stream_tasks.clear()
        self.agent_tasks.clear()
        self.publish("stopped", "数字人分身已停止")

    async def refresh_dws_status(self, force: bool = False) -> DwsStatus:
        """Refresh the credential-free DWS account state."""
        if not force and time.monotonic() - self.dws_checked_at < 5.0:
            return self.dws_status
        self.dws_status = await DWS.status()
        self.dws_checked_at = time.monotonic()
        return self.dws_status

    @staticmethod
    def identity_confirmed(status: DwsStatus) -> bool:
        """Return whether the visible OAuth account was confirmed by user."""
        if (
            not status.authenticated
            or not status.corp_id
            or not status.user_id
        ):
            return False
        return (
            STORE.get_setting("identity_corp_id", "") == status.corp_id
            and STORE.get_setting("identity_user_id", "") == status.user_id
        )

    def begin_integration(self, action: Literal["install", "login"]) -> None:
        """Start one explicit DWS setup operation without blocking the UI."""
        if self.integration_task and not self.integration_task.done():
            raise DwsError("已有钉钉连接任务正在进行")
        self.integration_stage = action
        self.integration_progress = None
        self.integration_detail = (
            "正在准备钉钉连接组件" if action == "install" else "已打开浏览器，请完成钉钉 OAuth 授权"
        )
        self.publish("integration", self.integration_detail)
        self.integration_task = asyncio.create_task(
            self._run_integration(action),
        )

    async def _run_integration(
        self,
        action: Literal["install", "login"],
    ) -> None:
        try:
            if action == "install":
                await DWS.install(self._publish_integration)
                self.dws_checked_at = 0.0
                self.integration_stage = "install_complete"
                self.integration_progress = 100
                self.integration_detail = "连接组件已就绪，请继续连接钉钉"
            else:
                self.dws_status = await DWS.login()
                self.dws_checked_at = time.monotonic()
                self.integration_stage = "ready"
                self.integration_progress = 100
                self.integration_detail = "钉钉 OAuth 已连接"
            self.publish("integration_ready", self.integration_detail)
        except asyncio.CancelledError:
            self.integration_stage = "cancelled"
            self.integration_progress = None
            self.integration_detail = "操作已取消，可以随时重新开始"
            self.publish("integration_cancelled", self.integration_detail)
            raise
        except Exception as exc:  # noqa: BLE001
            logger.exception("Paw Me DingTalk integration failed")
            self.integration_stage = "failed"
            self.integration_progress = None
            self.integration_detail = str(exc)
            self.publish("integration_failed", "钉钉连接失败", str(exc))

    async def _publish_integration(
        self,
        stage: str,
        detail: str,
        progress: int | None,
    ) -> None:
        """Publish one observable installation milestone."""
        self.integration_stage = stage
        self.integration_detail = detail
        self.integration_progress = progress
        self.publish("integration", detail)

    async def cancel_integration(self) -> None:
        """Cancel one active setup operation and reset its visible state."""
        task = self.integration_task
        if task is None or task.done():
            self.integration_stage = "idle"
            self.integration_progress = None
            self.integration_detail = ""
            return
        task.cancel()
        await DWS.cancel_integration()
        await asyncio.gather(task, return_exceptions=True)

    async def _supervise(self) -> None:
        next_status_at = 0.0
        while self.running:
            try:
                now = time.monotonic()
                if now >= next_status_at:
                    await self.refresh_dws_status()
                    next_status_at = now + 10.0
                if not self.dws_status.available:
                    self.publish(
                        "dws_missing",
                        "请在本页安装钉钉连接组件",
                    )
                elif not self.dws_status.authenticated:
                    self.publish(
                        "oauth_required",
                        "请在本页完成钉钉 OAuth 登录",
                    )
                elif not self.identity_confirmed(self.dws_status):
                    self.publish(
                        "identity_confirmation_required",
                        "请确认当前 OAuth 账号就是数字分身的本人账号",
                    )
                else:
                    self._ensure_profile_refresh()
                    self._ensure_streams()
                    await self._dispatch_due()
                    if not self.agent_tasks:
                        self.publish(
                            "watching",
                            "实时监听单聊和群聊，钉钉窗口无需置顶",
                        )
            except Exception as exc:  # noqa: BLE001
                logger.exception("Paw Me DingTalk supervisor failed")
                self.publish("error", "监听遇到错误，将自动重试", str(exc))
            await asyncio.sleep(1.0)

    def begin_profile_refresh(self) -> None:
        """Start one bounded profile refresh outside reply processing."""
        if self.profile_task and not self.profile_task.done():
            raise DwsError("数字分身画像正在初始化或更新")
        if not self.identity_confirmed(self.dws_status):
            raise DwsError("请先确认当前钉钉 OAuth 本人账号")
        current = STORE.get_owner_profile()
        STORE.save_owner_profile(
            corp_id=self.dws_status.corp_id,
            user_id=self.dws_status.user_id,
            status="collecting",
            collected=current.get("collected", {}),
        )
        self.profile_stage = "identity"
        self.profile_progress = 0
        self.profile_detail = "正在初始化本人身份、工作方式与人物关系"
        self.profile_task = asyncio.create_task(self._refresh_profile())

    def _ensure_profile_refresh(self) -> None:
        if self.profile_task and not self.profile_task.done():
            return
        profile = STORE.get_owner_profile()
        if (
            profile.get("corp_id") != self.dws_status.corp_id
            or profile.get("user_id") != self.dws_status.user_id
        ):
            STORE.invalidate_owner_profile()
            self.begin_profile_refresh()
            return
        if float(profile.get("next_refresh_at", 0)) <= time.time():
            self.begin_profile_refresh()

    async def _profile_progress(
        self,
        stage: str,
        progress: int,
        detail: str,
    ) -> None:
        self.profile_stage = stage
        self.profile_progress = progress
        self.profile_detail = detail
        self.version += 1

    async def _refresh_profile(self) -> None:
        try:
            collector = OwnerProfileCollector(DWS)
            collected, errors = await collector.collect(
                corp_id=self.dws_status.corp_id,
                user_id=self.dws_status.user_id,
                user_name=self.dws_status.user_name,
                progress=self._profile_progress,
            )
            profile_status = "partial" if errors else "ready"
            await asyncio.to_thread(
                STORE.save_owner_profile,
                corp_id=self.dws_status.corp_id,
                user_id=self.dws_status.user_id,
                status=profile_status,
                collected=collected,
                error="\n".join(errors),
            )
            self.profile_stage = profile_status
            self.profile_progress = 100
            self.profile_detail = (
                "画像已部分更新，可审核后使用" if errors else "本人身份、工作方式与人物关系已初始化"
            )
            STORE.add_activity(
                kind="profile",
                status=profile_status,
                title="数字分身画像已更新",
                detail=("；".join(errors) if errors else "本地快照已就绪"),
            )
        except asyncio.CancelledError:
            self.profile_stage = "cancelled"
            self.profile_progress = None
            self.profile_detail = "画像初始化已取消，可以重新开始"
            raise
        except Exception as exc:  # noqa: BLE001
            logger.exception("Paw Me owner profile refresh failed")
            current = STORE.get_owner_profile()
            await asyncio.to_thread(
                STORE.save_owner_profile,
                corp_id=self.dws_status.corp_id,
                user_id=self.dws_status.user_id,
                status=("stale" if current.get("collected") else "failed"),
                collected=current.get("collected", {}),
                error=str(exc),
            )
            self.profile_stage = "failed"
            self.profile_progress = None
            self.profile_detail = f"画像更新失败：{exc}"

    async def cancel_profile_refresh(self) -> None:
        """Cancel one profile collection without deleting the last snapshot."""
        task = self.profile_task
        if task is None or task.done():
            return
        task.cancel()
        await asyncio.gather(task, return_exceptions=True)
        current = STORE.get_owner_profile()
        if current.get("status") == "collecting":
            await asyncio.to_thread(
                STORE.save_owner_profile,
                corp_id=str(current.get("corp_id", "")),
                user_id=str(current.get("user_id", "")),
                status=("stale" if current.get("collected") else "failed"),
                collected=current.get("collected", {}),
                error="用户已取消本次画像更新",
            )

    def _ensure_streams(self) -> None:
        for kind in ("all-direct", "all-group"):
            task = self.stream_tasks.get(kind)
            if task and not task.done():
                continue
            self.stream_tasks[kind] = asyncio.create_task(
                self._consume_stream(kind),
            )

    async def _consume_stream(self, kind: str) -> None:
        while self.running and self.dws_status.authenticated:
            try:
                async for event in DWS.events(kind):
                    await self._append_event(event)
                if self.running:
                    raise DwsError(f"DWS {kind} 消息流已结束")
            except DwsError as exc:
                self.publish(
                    "stream_reconnecting",
                    f"{kind} 消息流断开，正在自动重连",
                    str(exc),
                )
                await asyncio.sleep(2.0)
            except Exception as exc:  # noqa: BLE001
                logger.exception("Unexpected DWS stream failure")
                self.publish(
                    "stream_reconnecting",
                    f"{kind} 消息流异常，正在自动重连",
                    str(exc),
                )
                await asyncio.sleep(2.0)

    @staticmethod
    def _context_key(subject_type: str, subject_id: str) -> str:
        return f"{subject_type}:{subject_id}"

    @staticmethod
    def _global_fallback_policy() -> str | None:
        access_mode = STORE.get_setting("access_mode", "approval")
        if access_mode == "block_all":
            return "blocked"
        if access_mode == "allow_all":
            return STORE.get_setting("default_policy", "draft")
        return None

    @classmethod
    def _effective_policy(cls, item: dict[str, Any]) -> str | None:
        principal = STORE.resolve_principal(
            str(item["subject_type"]),
            str(item["subject_id"]),
        )
        if principal is not None:
            return str(principal["policy"])
        return cls._global_fallback_policy()

    @staticmethod
    def _member_rows(payload: dict[str, Any]) -> list[dict[str, Any]]:
        result = payload.get("result")
        if not isinstance(result, dict):
            return []
        rows = result.get("list")
        if not isinstance(rows, list):
            return []
        return [row for row in rows if isinstance(row, dict)]

    async def _resolve_group_owner(self, conversation_id: str) -> str:
        cached = self.group_owner_ids.get(conversation_id)
        if cached:
            return cached
        payload = await DWS.group_members(conversation_id)
        candidates: set[str] = set()
        for row in self._member_rows(payload):
            member_user_id = self._first_text(
                row,
                "userId",
                "memberUserId",
                "staffId",
            )
            member_name = self._first_text(row, "memberEmpName")
            if (
                self.dws_status.user_id
                and member_user_id == self.dws_status.user_id
            ) or (
                self.dws_status.user_name
                and member_name == self.dws_status.user_name
            ):
                owner_id = self._first_text(
                    row,
                    "openDingTalkId",
                    "openDingtalkId",
                    "memberOpenDingTalkId",
                )
                if owner_id:
                    candidates.add(owner_id)
        if len(candidates) != 1:
            raise DwsError("无法从群成员数据唯一确认 OAuth 本人的真实 ID")
        owner_id = candidates.pop()
        self.group_owner_ids[conversation_id] = owner_id
        self.owner_open_ids.add(owner_id)
        return owner_id

    @staticmethod
    def _normalized_echo_text(text: str) -> str:
        return " ".join(text.split())

    def _echo_target(self, event: DwsMessageEvent) -> str:
        if event.subject_type == "group":
            return event.conversation_id
        return event.conversation_id or event.subject_id

    async def _is_outbound_event(self, event: DwsMessageEvent) -> bool:
        now = time.monotonic()
        self.sent_echoes = {
            key: expires_at
            for key, expires_at in self.sent_echoes.items()
            if expires_at > now
        }
        if event.subject_type == "group":
            try:
                await self._resolve_group_owner(event.conversation_id)
            except DwsError as exc:
                logger.warning("DWS owner identity unresolved: %s", exc)
                STORE.add_activity(
                    kind="identity",
                    status="partial",
                    title="群聊本人身份暂未解析",
                    detail=str(exc),
                )
        if event.sender_open_dingtalk_id in self.owner_open_ids:
            return True
        key = (
            event.subject_type,
            self._echo_target(event),
            self._normalized_echo_text(event.content),
        )
        return key in self.sent_echoes

    def _remember_sent_echo(
        self,
        item: dict[str, Any],
        text: str,
    ) -> tuple[str, str, str]:
        target = str(item["subject_id"])
        if item["subject_type"] == "person" and item.get("messages"):
            raw = item["messages"][-1].get("raw") or {}
            target = str(raw.get("conversation_id") or target)
        key = (
            str(item["subject_type"]),
            target,
            self._normalized_echo_text(text),
        )
        self.sent_echoes[key] = time.monotonic() + 90.0
        return key

    async def _append_event(self, event: DwsMessageEvent) -> None:
        """Persist one event before scheduling any processing."""
        if await self._is_outbound_event(event):
            STORE.add_activity(
                kind="inbound",
                status="ignored_self",
                title="已忽略本人发出的消息回流",
                detail=event.content,
            )
            self.publish("watching", "已过滤本人发出的钉钉消息")
            return
        context_key = self._context_key(
            event.subject_type,
            event.subject_id,
        )
        alias = event.display_name
        if event.subject_type == "group":
            alias = event.conversation_id
        received_at = (
            event.timestamp / 1000.0
            if event.timestamp > 10_000_000_000
            else float(event.timestamp or time.time())
        )
        item, created = await asyncio.to_thread(
            STORE.observe,
            source_key=f"dws:{event.event_id}:{event.message_id}",
            conversation_alias=alias,
            subject_type=event.subject_type,
            text=event.content,
            agent_id=STORE.get_setting("agent_id", "default"),
            quiet_seconds=float(
                STORE.get_setting("quiet_seconds", "4.0"),
            ),
            max_wait_seconds=float(
                STORE.get_setting("max_wait_seconds", "20.0"),
            ),
            fallback_policy=self._global_fallback_policy(),
            subject_id=event.subject_id,
            id_source="oauth:dws-event",
            display_name=event.display_name,
            received_at=received_at,
            raw_message=event.raw,
        )
        if not created:
            return
        await self._load_history_once(event, context_key)
        await asyncio.to_thread(
            STORE.append_context,
            context_key,
            event.subject_type,
            [
                {
                    "message_id": event.message_id,
                    "incoming": True,
                    "speaker": event.display_name,
                    "text": event.content,
                    "timestamp": event.timestamp,
                },
            ],
        )
        self.current_conversation = alias
        task_key = context_key
        task = self.agent_tasks.get(task_key)
        if item["status"] == "interrupt_requested" and task:
            self.publish(
                "interrupting",
                f"{alias} 又发来消息，正在停止旧回复并合并上下文",
            )
            try:
                await self._request_agent_stop(item)
            except Exception as exc:  # noqa: BLE001
                logger.warning("Native Agent stop failed: %s", exc)
            finally:
                task.cancel()
        self.publish(
            "collecting",
            f"已持久化 {item['message_count']} 条连续消息，等待对方说完",
        )

    async def _load_history_once(
        self,
        event: DwsMessageEvent,
        context_key: str,
    ) -> None:
        if context_key in self.history_loaded:
            return
        self.publish("loading_context", "正在加载最近消息上下文")
        try:
            payload = await DWS.history(
                event.subject_type,
                event.subject_id,
                limit=80,
            )
            messages = self._project_history(
                payload,
                event.subject_type,
                event.subject_id,
                self.group_owner_ids.get(
                    event.subject_id,
                    self.dws_status.user_id,
                ),
            )
            await asyncio.to_thread(
                STORE.append_context,
                context_key,
                event.subject_type,
                messages,
            )
            self.history_loaded.add(context_key)
        except Exception as exc:  # noqa: BLE001
            logger.warning("DWS context history unavailable: %s", exc)
            STORE.add_activity(
                kind="context",
                status="partial",
                title="历史上下文暂时不可用",
                detail=str(exc),
            )

    @classmethod
    def _project_history(
        cls,
        payload: dict[str, Any],
        subject_type: str,
        subject_id: str,
        current_user_id: str,
    ) -> list[dict[str, Any]]:
        rows = cls._find_message_rows(payload)
        projected: list[dict[str, Any]] = []
        for row in rows:
            sender = row.get("sender")
            sender_data = sender if isinstance(sender, dict) else {}
            sender_id = cls._first_text(
                sender_data,
                "openDingTalkId",
                "userId",
                "senderId",
            ) or cls._first_text(
                row,
                "senderOpenDingTalkId",
                "senderId",
                "senderUserId",
            )
            speaker = cls._first_text(sender_data, "name", "nick")
            text = cls._message_text(row)
            if not text:
                continue
            incoming: bool | None = None
            explicit_self = row.get("isSelf", row.get("is_self"))
            if isinstance(explicit_self, bool):
                incoming = not explicit_self
            elif current_user_id and sender_id == current_user_id:
                incoming = False
            elif subject_type == "person" and sender_id:
                incoming = sender_id == subject_id
            projected.append(
                {
                    "message_id": cls._first_text(
                        row,
                        "openMessageId",
                        "messageId",
                        "message_id",
                    ),
                    "incoming": incoming,
                    "speaker": speaker or sender_id or "未知发送者",
                    "text": text,
                    "timestamp": cls._first_text(
                        row,
                        "createTime",
                        "create_time",
                    ),
                },
            )
        return projected

    @staticmethod
    def _find_message_rows(value: Any) -> list[dict[str, Any]]:
        if isinstance(value, dict):
            messages = value.get("messages")
            if isinstance(messages, list):
                return [row for row in messages if isinstance(row, dict)]
            for child in value.values():
                found = PawMeRuntime._find_message_rows(child)
                if found:
                    return found
        return []

    @staticmethod
    def _first_text(data: dict[str, Any], *keys: str) -> str:
        for key in keys:
            value = data.get(key)
            if isinstance(value, (str, int)) and str(value).strip():
                return str(value).strip()
        return ""

    @staticmethod
    def _message_text(row: dict[str, Any]) -> str:
        content = row.get("content")
        if isinstance(content, str):
            text = content.strip()
            if text.startswith("{"):
                try:
                    decoded = json.loads(text)
                    if isinstance(decoded, dict):
                        return str(
                            decoded.get("text")
                            or decoded.get("content")
                            or text,
                        ).strip()
                except json.JSONDecodeError:
                    pass
            return text
        if isinstance(content, dict):
            return str(
                content.get("text") or content.get("content") or "",
            ).strip()
        return ""

    async def _dispatch_due(self) -> None:
        items = await asyncio.to_thread(STORE.due_work_items)
        for item in items:
            policy = self._effective_policy(item)
            if policy is None:
                await asyncio.to_thread(
                    STORE.update_work_item,
                    str(item["id"]),
                    status="identity_required",
                )
                continue
            if policy in {"blocked", "observe"}:
                await asyncio.to_thread(
                    STORE.update_work_item,
                    str(item["id"]),
                    status=("blocked" if policy == "blocked" else "observed"),
                )
                continue
            task_key = self._context_key(
                str(item["subject_type"]),
                str(item["subject_id"]),
            )
            task = self.agent_tasks.get(task_key)
            if task and not task.done():
                continue
            ctx = self.contexts.get(str(item["agent_id"]))
            if ctx is None:
                self.publish(
                    "agent_context_required",
                    "请打开本页一次，以连接所选 Agent 运行时",
                )
                continue
            task = asyncio.create_task(self._run_agent(ctx, item["id"]))
            self.agent_tasks[task_key] = task
            task.add_done_callback(
                lambda done, key=task_key: self._clear_agent_task(key, done),
            )

    def _clear_agent_task(
        self,
        task_key: str,
        task: asyncio.Task[Any],
    ) -> None:
        if self.agent_tasks.get(task_key) is task:
            self.agent_tasks.pop(task_key, None)

    async def _request_agent_stop(self, item: dict[str, Any]) -> bool:
        """Reuse QwenPaw's native task tracker stop operation."""
        ctx = self.contexts.get(str(item["agent_id"]))
        if ctx is None:
            return False
        bound_ctx = dataclasses.replace(ctx, agent_id=item["agent_id"])
        workspace = await bound_ctx._get_workspace()
        if workspace is None:
            return False
        if workspace.chat_manager is None:
            return False
        chat_id = await workspace.chat_manager.get_chat_id_by_session(
            session_id=str(item["session_id"]),
            channel=CHANNEL,
            user_id=str(item["subject_id"]),
        )
        if not chat_id:
            return False
        return await workspace.task_tracker.request_stop(chat_id)

    async def _run_agent(self, ctx: Any, item_id: str) -> None:
        item = await asyncio.to_thread(
            STORE.update_work_item,
            item_id,
            status="agent_running",
        )
        alias = str(item["conversation_alias"])
        self.publish(
            "agent_running",
            f"{item['agent_id']} 正在处理 {item['message_count']} 条合并消息",
        )
        STORE.add_activity(
            kind="agent",
            status="running",
            title=f"Agent 开始处理 {alias}",
            detail=f"合并 {item['message_count']} 条消息",
            work_item_id=item_id,
        )
        try:
            await self._refresh_history(item)
            bound_ctx = dataclasses.replace(ctx, agent_id=item["agent_id"])
            reply = await self._chat_as_owner(bound_ctx, item)
            text = reply.text.strip()
            if not text:
                raise RuntimeError("Agent did not return reply text")
            outbox = await asyncio.to_thread(
                STORE.finalize_agent_reply,
                item_id,
                text,
            )
            if outbox is None:
                await asyncio.to_thread(
                    STORE.update_work_item,
                    item_id,
                    status="collecting",
                )
                return
            leak_error = self._identity_leak_error(text)
            if leak_error:
                await asyncio.to_thread(
                    STORE.update_outbox,
                    str(outbox["id"]),
                    status="needs_review",
                    error=leak_error,
                )
                await asyncio.to_thread(
                    STORE.update_work_item,
                    item_id,
                    status="draft_ready",
                    response=text,
                    error=leak_error,
                )
                self.publish(
                    "draft_needs_review",
                    f"{alias} 的回复触发身份保护，已禁止自动发送",
                    leak_error,
                )
                return
            if self._effective_policy(item) == "automatic":
                await self.send_outbox(str(outbox["id"]))
            else:
                self.publish("draft_ready", f"{alias} 的回复已进入待发送")
        except asyncio.CancelledError:
            latest = await asyncio.to_thread(STORE.get_work_item, item_id)
            if latest["status"] in {
                "agent_running",
                "interrupt_requested",
            }:
                await asyncio.to_thread(
                    STORE.update_work_item,
                    item_id,
                    status="collecting",
                )
            STORE.add_activity(
                kind="agent",
                status="interrupted",
                title=f"已停止 {alias} 的旧回复",
                detail="新消息已落库，静默窗口结束后携带完整上下文重试",
                work_item_id=item_id,
            )
            raise
        except Exception as exc:  # noqa: BLE001
            logger.exception("Paw Me Agent run failed")
            await asyncio.to_thread(
                STORE.update_work_item,
                item_id,
                status="failed",
                error=str(exc),
            )
            self.publish("failed", f"{alias} 处理失败", str(exc))

    async def _refresh_history(self, item: dict[str, Any]) -> None:
        """Refresh bounded context immediately before each Agent turn."""
        context_key = self._context_key(
            str(item["subject_type"]),
            str(item["subject_id"]),
        )
        try:
            payload = await DWS.history(
                str(item["subject_type"]),
                str(item["subject_id"]),
                limit=80,
            )
            messages = self._project_history(
                payload,
                str(item["subject_type"]),
                str(item["subject_id"]),
                self.group_owner_ids.get(
                    str(item["subject_id"]),
                    self.dws_status.user_id,
                ),
            )
            await asyncio.to_thread(
                STORE.append_context,
                context_key,
                str(item["subject_type"]),
                messages,
            )
        except Exception as exc:  # noqa: BLE001
            logger.warning("Pre-turn DWS context refresh failed: %s", exc)
            STORE.add_activity(
                kind="context",
                status="partial",
                title="本轮使用已持久化上下文",
                detail=str(exc),
                work_item_id=str(item["id"]),
            )

    async def _chat_as_owner(
        self,
        bound_ctx: Any,
        item: dict[str, Any],
    ) -> ChatReply:
        """Send one canonical AgentRequest with an owner system identity."""
        workspace = await bound_ctx._get_workspace()
        if workspace is None or not hasattr(workspace, "stream_query"):
            raise RuntimeError("Selected Agent workspace is unavailable")
        request = AgentRequest(
            input=[
                {
                    "role": "system",
                    "content": [
                        {
                            "type": "text",
                            "text": self._identity_instructions(item),
                        },
                    ],
                },
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "text",
                            "text": self._build_prompt(item),
                        },
                    ],
                },
            ],
            session_id=str(item["session_id"]),
            user_id=str(item["subject_id"]),
            channel=CHANNEL,
            agent_id=str(item["agent_id"]),
        )
        chunks = []
        async for event in workspace.stream_query(request):
            chunks.append(event)
        return ChatReply(chunks=chunks)

    @staticmethod
    def _build_prompt(item: dict[str, Any]) -> str:
        context_key = PawMeRuntime._context_key(
            str(item["subject_type"]),
            str(item["subject_id"]),
        )
        context = STORE.get_context(context_key)[-80:]
        context_lines = []
        for row in context:
            incoming = row.get("incoming")
            if incoming is True:
                speaker = str(row.get("speaker") or "对方")
            elif incoming is False:
                speaker = "我"
            else:
                speaker = str(row.get("speaker") or "其他成员")
            context_lines.append(f"{speaker}：{row.get('text', '')}")
        turn_lines = [
            f"{index}. {message['text']}"
            for index, message in enumerate(item["messages"], start=1)
        ]
        return (
            "SQLite 中已经持久化的完整批次是本轮权威输入。"
            "此前被 stop 的未完成输出一律作废，不得遗漏或只处理"
            "最后一句。\n\n"
            "最近钉钉上下文：\n"
            f"{'\n'.join(context_lines) or '暂无可用上下文'}\n\n"
            "对方本轮连续发送的消息（合并理解，只回复一次）：\n"
            f"{'\n'.join(turn_lines)}\n\n"
            "模仿上下文中“我”的语气、用词、简洁程度和行为方式。"
            "若上下文不足，保持自然简洁；若意图不清楚，用我的语气"
            "追问。若需要执行任务，把可公开的步骤、当前进度和结果"
            "写入回复，但不得泄露隐藏思维链、凭证、令牌或敏感工具"
            "参数。只输出可直接发给对方的消息正文。"
        )

    def _identity_instructions(self, item: dict[str, Any]) -> str:
        owner = self.dws_status.user_name.strip()
        profile = STORE.get_owner_profile()
        collected = profile.get("collected", {})
        approved = profile.get("approved", {})
        local_profile = ""
        if profile.get("approved_at") and collected:
            local_profile = profile_prompt(
                collected,
                str(item.get("subject_id", "")),
            )
            notes = str(approved.get("notes", "")).strip()
            if notes:
                local_profile = f"{local_profile}\n本人审核补充：{notes}"
        if not owner and not local_profile:
            return IDENTITY_INSTRUCTIONS
        return (
            f"{IDENTITY_INSTRUCTIONS}当前经用户确认的钉钉账号主人显示名为"
            f"“{owner or '未命名'}”；仅用于理解说话者身份，不要主动自我"
            f"介绍。以下画像来自初始化或后台定期更新的本地快照，本轮不得"
            f"为画像再次查询钉钉：\n{local_profile}"
        )

    @staticmethod
    def _identity_leak_error(text: str) -> str:
        for pattern in IDENTITY_LEAK_PATTERNS:
            if pattern.search(text):
                return "回复疑似泄漏 Agent 身份或包含元分析，已强制转为" "人工确认草稿"
        return ""

    async def send_outbox(self, outbox_id: str) -> dict[str, Any]:
        """Send one draft to its exact OAuth-derived target."""
        outbox = await asyncio.to_thread(STORE.get_outbox, outbox_id)
        item = await asyncio.to_thread(
            STORE.get_work_item,
            str(outbox["work_item_id"]),
        )
        if not item.get("subject_id") or not item.get("id_source"):
            raise DwsError("发送目标没有可验证的钉钉身份")
        await asyncio.to_thread(
            STORE.update_outbox,
            outbox_id,
            status="sending",
        )
        self.publish(
            "sending",
            f"正在发送到 {outbox['conversation_alias']}",
        )
        echo_key = self._remember_sent_echo(item, str(outbox["text"]))
        try:
            send_result = await DWS.send(
                subject_type=str(item["subject_type"]),
                subject_id=str(item["subject_id"]),
                text=str(outbox["text"]),
                idempotency_key=outbox_id,
            )
            result = await asyncio.to_thread(
                STORE.update_outbox,
                outbox_id,
                status="sent",
            )
            await asyncio.to_thread(
                STORE.update_work_item,
                str(outbox["work_item_id"]),
                status="sent",
                response=str(outbox["text"]),
            )
            context_key = self._context_key(
                str(item["subject_type"]),
                str(item["subject_id"]),
            )
            await asyncio.to_thread(
                STORE.append_context,
                context_key,
                str(item["subject_type"]),
                [
                    {
                        "message_id": self._sent_message_id(
                            send_result,
                            outbox_id,
                        ),
                        "incoming": False,
                        "speaker": "我",
                        "text": str(outbox["text"]),
                        "timestamp": time.time(),
                    },
                ],
            )
            self.publish("sent", f"已发送到 {outbox['conversation_alias']}")
            return result
        except Exception as exc:  # noqa: BLE001
            self.sent_echoes.pop(echo_key, None)
            await asyncio.to_thread(
                STORE.update_outbox,
                outbox_id,
                status="failed",
                error=str(exc),
            )
            self.publish("send_failed", "发送失败，草稿仍然保留", str(exc))
            raise

    @classmethod
    def _sent_message_id(
        cls,
        payload: Any,
        outbox_id: str,
    ) -> str:
        """Extract one DWS message ID for durable context deduplication."""
        if isinstance(payload, dict):
            value = cls._first_text(
                payload,
                "openMessageId",
                "messageId",
                "message_id",
            )
            if value:
                return value
            for child in payload.values():
                found = cls._sent_message_id(child, "")
                if found:
                    return found
        elif isinstance(payload, list):
            for child in payload:
                found = cls._sent_message_id(child, "")
                if found:
                    return found
        return f"outbox:{outbox_id}" if outbox_id else ""


RUNTIME = PawMeRuntime()
router = APIRouter()
app = PawApp("Paw Me · DingTalk", app_id=APP_ID)
app.enable_standard_capabilities()


@router.get("/snapshot")
async def snapshot(ctx=Depends(get_ctx)) -> dict[str, Any]:
    """Return the complete single-page application state."""
    RUNTIME.remember_context(ctx)
    data = await asyncio.to_thread(STORE.snapshot)
    dws_status = await RUNTIME.refresh_dws_status()
    work_items = {item["id"]: item for item in data["work_items"]}
    for outbox in data["outbox"]:
        source = work_items.get(outbox["work_item_id"], {})
        outbox["source_display_name"] = source.get("display_name", "")
        outbox["source_subject_type"] = source.get("subject_type", "")
        outbox["source_messages"] = source.get("messages", [])
    data["runtime"] = RUNTIME.status()
    data["identity_provider"] = {
        "provider": "dingtalk-dws",
        **dws_status.as_dict(),
        "confirmed": RUNTIME.identity_confirmed(dws_status),
    }
    return data


@router.put("/settings")
async def update_settings(
    payload: SettingsPayload,
    ctx=Depends(get_ctx),
) -> dict[str, Any]:
    """Update runtime settings and start or stop observation."""
    if payload.max_wait_seconds < payload.quiet_seconds:
        raise HTTPException(
            status_code=400,
            detail="最长等待时间不能短于静默窗口",
        )
    RUNTIME.remember_context(ctx)
    settings = payload.model_dump()
    blocked_stage = ""
    blocked_detail = ""
    previous_access_mode = STORE.get_setting("access_mode", "approval")
    if payload.enabled:
        status = await RUNTIME.refresh_dws_status(force=True)
        if not status.authenticated:
            settings["enabled"] = False
            blocked_stage = "setup_required"
            blocked_detail = "完成钉钉连接后才能启用数字人分身"
            RUNTIME.publish(
                "setup_required",
                blocked_detail,
            )
        elif not RUNTIME.identity_confirmed(status):
            settings["enabled"] = False
            blocked_stage = "identity_confirmation_required"
            blocked_detail = "请先确认当前 OAuth 账号就是数字分身本人"
            RUNTIME.publish(
                "identity_confirmation_required",
                blocked_detail,
            )
        else:
            profile = await asyncio.to_thread(STORE.get_owner_profile)
            if not profile.get("approved_at"):
                settings["enabled"] = False
                blocked_stage = "profile_approval_required"
                blocked_detail = "请先完成并审核本人画像，再启用数字人分身"
                RUNTIME.publish(
                    "profile_approval_required",
                    blocked_detail,
                )
    for key, value in settings.items():
        stored = str(value).lower() if isinstance(value, bool) else str(value)
        STORE.set_setting(key, stored)
    if previous_access_mode != settings["access_mode"] and RUNTIME.running:
        await RUNTIME.stop()
        await asyncio.to_thread(STORE.recover_incomplete)
    await asyncio.to_thread(
        STORE.apply_global_policy,
        str(settings["access_mode"]),
    )
    if settings["enabled"]:
        RUNTIME.start()
    else:
        await RUNTIME.stop()
        if blocked_detail:
            RUNTIME.publish(blocked_stage, blocked_detail)
    return await snapshot(ctx)


@router.post("/dws/install")
async def install_dws() -> dict[str, Any]:
    """Start the managed DingTalk connector installation."""
    try:
        RUNTIME.begin_integration("install")
    except DwsError as exc:
        raise HTTPException(status_code=409, detail=str(exc)) from exc
    return RUNTIME.status()


@router.post("/dws/login")
async def login_dws() -> dict[str, Any]:
    """Start the official browser OAuth flow after an explicit click."""
    status = await RUNTIME.refresh_dws_status(force=True)
    if not status.available:
        raise HTTPException(
            status_code=409,
            detail="请先安装钉钉连接组件",
        )
    try:
        RUNTIME.begin_integration("login")
    except DwsError as exc:
        raise HTTPException(status_code=409, detail=str(exc)) from exc
    return RUNTIME.status()


@router.post("/dws/cancel")
async def cancel_dws() -> dict[str, Any]:
    """Cancel an in-progress connector install or OAuth login."""
    await RUNTIME.cancel_integration()
    return RUNTIME.status()


@router.post("/identity/confirm")
async def confirm_identity(ctx=Depends(get_ctx)) -> dict[str, Any]:
    """Confirm the exact OAuth account before it may speak as the user."""
    status = await RUNTIME.refresh_dws_status(force=True)
    if not status.authenticated or not status.corp_id or not status.user_id:
        raise HTTPException(
            status_code=409,
            detail="当前钉钉 OAuth 账号信息不完整，请重新连接",
        )
    STORE.set_setting("identity_corp_id", status.corp_id)
    STORE.set_setting("identity_user_id", status.user_id)
    STORE.add_activity(
        kind="identity",
        status="verified",
        title=f"已确认本人账号 {status.user_name or status.user_id}",
        detail=f"组织：{status.corp_name or status.corp_id}",
    )
    RUNTIME.publish("identity_confirmed", "本人 OAuth 账号已确认")
    try:
        RUNTIME.begin_profile_refresh()
    except DwsError as exc:
        raise HTTPException(status_code=409, detail=str(exc)) from exc
    return await snapshot(ctx)


@router.post("/profile/refresh")
async def refresh_profile(ctx=Depends(get_ctx)) -> dict[str, Any]:
    """Start a bounded profile refresh without blocking the page."""
    RUNTIME.remember_context(ctx)
    status = await RUNTIME.refresh_dws_status(force=True)
    if not RUNTIME.identity_confirmed(status):
        raise HTTPException(
            status_code=409,
            detail="请先确认当前钉钉 OAuth 本人账号",
        )
    try:
        RUNTIME.begin_profile_refresh()
    except DwsError as exc:
        raise HTTPException(status_code=409, detail=str(exc)) from exc
    return await snapshot(ctx)


@router.post("/profile/cancel")
async def cancel_profile(ctx=Depends(get_ctx)) -> dict[str, Any]:
    """Cancel profile collection and retain the previous local snapshot."""
    await RUNTIME.cancel_profile_refresh()
    return await snapshot(ctx)


@router.post("/profile/approve")
async def approve_profile(
    payload: ProfileApprovalPayload,
    ctx=Depends(get_ctx),
) -> dict[str, Any]:
    """Approve the visible snapshot and optional owner guidance."""
    try:
        await asyncio.to_thread(
            STORE.approve_owner_profile,
            payload.notes,
        )
    except ValueError as exc:
        raise HTTPException(status_code=409, detail=str(exc)) from exc
    RUNTIME.publish("profile_approved", "本人画像已审核，可以启用")
    return await snapshot(ctx)


@router.delete("/profile")
async def reset_profile(ctx=Depends(get_ctx)) -> dict[str, Any]:
    """Delete the local snapshot and require a fresh review."""
    await RUNTIME.cancel_profile_refresh()
    await asyncio.to_thread(STORE.invalidate_owner_profile)
    STORE.set_setting("enabled", "false")
    await RUNTIME.stop()
    return await snapshot(ctx)


@router.post("/identity/reconnect")
async def reconnect_identity(ctx=Depends(get_ctx)) -> dict[str, Any]:
    """Disconnect only the visible account and return to OAuth setup."""
    status = await RUNTIME.refresh_dws_status(force=True)
    await RUNTIME.stop()
    STORE.set_setting("enabled", "false")
    STORE.set_setting("identity_corp_id", "")
    STORE.set_setting("identity_user_id", "")
    STORE.invalidate_owner_profile()
    RUNTIME.owner_open_ids.clear()
    RUNTIME.group_owner_ids.clear()
    if status.authenticated:
        try:
            await DWS.logout(status)
        except DwsError as exc:
            raise HTTPException(status_code=409, detail=str(exc)) from exc
    RUNTIME.dws_checked_at = 0.0
    RUNTIME.integration_stage = "idle"
    RUNTIME.integration_detail = ""
    return await snapshot(ctx)


@router.post("/work-items/{item_id}/authorize")
async def authorize_work_item(
    item_id: str,
    payload: AuthorizationPayload,
) -> dict[str, Any]:
    """Authorize an event-derived identity without editable ID fields."""
    try:
        item = await asyncio.to_thread(STORE.get_work_item, item_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail="消息批次不存在") from exc
    if (
        not item.get("subject_id")
        or item.get("id_source") != "oauth:dws-event"
    ):
        raise HTTPException(
            status_code=409,
            detail="该消息没有钉钉 OAuth 返回的真实身份，不能授权",
        )
    principal = await asyncio.to_thread(
        STORE.add_principal,
        subject_type=str(item["subject_type"]),
        subject_id=str(item["subject_id"]),
        id_source=str(item["id_source"]),
        display_name=str(item["display_name"]),
        conversation_alias=str(item["conversation_alias"]),
        policy=payload.policy,
    )
    bound = await asyncio.to_thread(STORE.bind_pending, principal)
    STORE.add_activity(
        kind="identity",
        status="verified",
        title=f"已授权 {principal['display_name']}",
        detail=f"真实 ID 来自钉钉 OAuth，恢复 {bound} 个待处理批次",
    )
    return principal


@router.delete("/principals/{principal_id}")
async def delete_principal(principal_id: str) -> dict[str, bool]:
    """Delete one Paw Me identity policy."""
    deleted = await asyncio.to_thread(
        STORE.delete_principal,
        principal_id,
    )
    return {"deleted": deleted}


@router.patch("/principals/{principal_id}/policy")
async def update_principal_policy(
    principal_id: str,
    payload: AuthorizationPayload,
) -> dict[str, Any]:
    """Update only the policy of an already verified identity."""
    try:
        return await asyncio.to_thread(
            STORE.update_principal_policy,
            principal_id,
            payload.policy,
        )
    except KeyError as exc:
        raise HTTPException(status_code=404, detail="身份策略不存在") from exc


@router.patch("/outbox/{outbox_id}")
async def edit_outbox(
    outbox_id: str,
    payload: OutboxPayload,
) -> dict[str, Any]:
    """Edit a pending reply without discarding its source context."""
    try:
        return await asyncio.to_thread(
            STORE.update_outbox,
            outbox_id,
            status="pending",
            text=payload.text,
        )
    except KeyError as exc:
        raise HTTPException(status_code=404, detail="草稿不存在") from exc


@router.post("/outbox/{outbox_id}/send")
async def send_outbox(outbox_id: str) -> dict[str, Any]:
    """Send one approved draft."""
    try:
        return await RUNTIME.send_outbox(outbox_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail="草稿不存在") from exc
    except Exception as exc:  # noqa: BLE001
        raise HTTPException(status_code=409, detail=str(exc)) from exc


@router.delete("/outbox/{outbox_id}")
async def delete_outbox(outbox_id: str) -> dict[str, bool]:
    """Delete one draft while retaining its source turn."""
    return {"deleted": await asyncio.to_thread(STORE.delete_outbox, outbox_id)}


@router.get("/events")
async def events() -> StreamingResponse:
    """Stream runtime version changes for responsive status rendering."""

    async def generate():
        version = -1
        while True:
            if version != RUNTIME.version:
                version = RUNTIME.version
                payload = json.dumps(RUNTIME.status(), ensure_ascii=False)
                yield f"event: state\ndata: {payload}\n\n"
            await asyncio.sleep(1.0)

    return StreamingResponse(generate(), media_type="text/event-stream")


@app.on_launch
async def launch() -> None:
    """Resume the listener when the persisted master switch is on."""
    if STORE.get_setting("enabled", "false") == "true":
        RUNTIME.start()


@app.on_terminate
async def terminate() -> None:
    """Stop all Paw Me background tasks cleanly."""
    await RUNTIME.stop()


app.include_router(router)
