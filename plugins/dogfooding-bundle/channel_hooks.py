# -*- coding: utf-8 -*-
# pylint: disable=relative-beyond-top-level,protected-access
"""Channel hooks for dogfooding feedback (DingTalk action cards)."""

from __future__ import annotations

import logging
from typing import Any, Dict

from .feedback_service import (
    BAD_FEEDBACK_REASONS,
    build_dingtalk_feedback_action_card,
    build_dingtalk_reason_action_card,
    handle_dingtalk_inbound_text,
    is_dogfooding_model,
    start_dingtalk_feedback,
)

logger = logging.getLogger(__name__)


def _extract_last_user_text(msgs) -> str:
    if not msgs:
        return ""
    last = msgs[-1]
    content = getattr(last, "content", None)
    if isinstance(content, str):
        return content.strip()
    if isinstance(content, list):
        parts = []
        for item in content:
            if isinstance(item, dict) and item.get("type") == "text":
                parts.append(str(item.get("text", "")))
        return "\n".join(parts).strip()
    return ""


def _extract_last_assistant_text(agent) -> str:
    memory = getattr(agent, "memory", None)
    content = getattr(memory, "content", None) if memory else None
    if not content:
        return ""
    for msg, _marks in reversed(content):
        if getattr(msg, "role", None) != "assistant":
            continue
        body = getattr(msg, "content", None)
        if isinstance(body, str):
            return body.strip()
        if isinstance(body, list):
            parts = []
            for item in body:
                if isinstance(item, dict) and item.get("type") == "text":
                    parts.append(str(item.get("text", "")))
            return "\n".join(parts).strip()
    return ""


def _get_active_model_name(agent_id: str) -> str:
    try:
        from qwenpaw.config.config import load_agent_config

        cfg = load_agent_config(agent_id)
        llm = getattr(cfg, "active_llm", None)
        if llm is None:
            return ""
        return str(getattr(llm, "model", "") or "")
    except Exception:
        return ""


def _should_attach_feedback(
    channel: str,
    agent_id: str,
) -> bool:
    if channel != "dingtalk":
        return False
    return is_dogfooding_model(_get_active_model_name(agent_id))


def patch_dingtalk_channel() -> None:
    """Patch DingTalk channel to send feedback cards after AI replies."""
    try:
        from qwenpaw.app.channels.dingtalk.channel import DingTalkChannel
    except ImportError:
        logger.warning(
            "DingTalk channel unavailable; feedback card hook skipped",
        )
        return

    if getattr(DingTalkChannel, "_dogfooding_feedback_patched", False):
        return

    original_completed = DingTalkChannel._on_process_completed

    async def patched_completed(
        self,
        request: Any,
        to_handle: str,
        send_meta: Dict[str, Any],
    ) -> None:
        await original_completed(self, request, to_handle, send_meta)

        agent_id = getattr(self, "_agent_id", "") or getattr(
            getattr(self, "_workspace", None),
            "agent_id",
            "",
        )
        channel = getattr(request, "channel", "") or "dingtalk"
        if not _should_attach_feedback(channel, agent_id):
            return

        trace_ctx = getattr(request, "_dogfooding_trace", None) or {}
        trace_id = str(trace_ctx.get("trace_id") or "").strip()
        session_id = str(getattr(request, "session_id", "") or "").strip()
        conversation_id = str((send_meta or {}).get("conversation_id") or "")
        if not trace_id or not session_id:
            return

        session_webhook = (send_meta or {}).get("session_webhook")
        if not session_webhook:
            return

        user_id = str(trace_ctx.get("user_id") or "")
        user_name = str((send_meta or {}).get("user_name") or "")

        start_dingtalk_feedback(
            trace_id=trace_id,
            conversation_id=conversation_id or session_id,
            session_id=session_id,
            user_id=user_id,
            user_name=user_name,
        )

        payload = build_dingtalk_feedback_action_card(trace_id)
        try:
            await self._send_payload_via_session_webhook(
                session_webhook,
                payload,
            )
        except Exception:
            logger.debug(
                "Failed to send DingTalk feedback card",
                exc_info=True,
            )

    DingTalkChannel._on_process_completed = patched_completed
    DingTalkChannel._dogfooding_feedback_patched = True
    logger.info("Patched DingTalkChannel for dogfooding feedback cards")


def patch_dingtalk_handler() -> None:
    """Intercept DingTalk inbound messages for feedback continuation."""
    try:
        from qwenpaw.app.channels.dingtalk.handler import DingTalkHandler
    except ImportError:
        logger.warning("DingTalk handler unavailable; feedback hook skipped")
        return

    if getattr(DingTalkHandler, "_dogfooding_feedback_patched", False):
        return

    original_process = DingTalkHandler.process

    async def patched_process(self, callback) -> tuple[int, str]:
        try:
            raw_data = getattr(callback, "data", None) or {}
            text = str(raw_data.get("text") or "").strip()
            if not text and isinstance(raw_data.get("text"), dict):
                text = str(raw_data["text"].get("content") or "").strip()
            conversation_id = str(
                raw_data.get("conversationId")
                or raw_data.get("conversation_id")
                or "",
            ).strip()
            if text and conversation_id:
                result = handle_dingtalk_inbound_text(
                    conversation_id=conversation_id,
                    text=text,
                    user_id=str(raw_data.get("senderStaffId") or ""),
                    user_name=str(raw_data.get("senderNick") or ""),
                )
                if result and result.get("action") == "ask_reason":
                    channel = getattr(self, "_channel", None)
                    session_webhook = raw_data.get("sessionWebhook")
                    if channel and session_webhook:
                        payload = build_dingtalk_reason_action_card()
                        await channel._send_payload_via_session_webhook(
                            session_webhook,
                            payload,
                        )
                    lines = "\n".join(
                        f"{idx}. {reason}"
                        for idx, reason in enumerate(BAD_FEEDBACK_REASONS, 1)
                    )
                    import dingtalk_stream

                    return (
                        dingtalk_stream.AckMessage.STATUS_OK,
                        "很抱歉没有达到您的期望，请回复问题编号：\n" + lines,
                    )
                if result and result.get("action") == "submitted":
                    import dingtalk_stream

                    return (
                        dingtalk_stream.AckMessage.STATUS_OK,
                        "✅ 反馈已提交，感谢你的宝贵意见！",
                    )
        except Exception:
            logger.debug("DingTalk feedback pre-process failed", exc_info=True)

        return await original_process(self, callback)

    DingTalkHandler.process = patched_process
    DingTalkHandler._dogfooding_feedback_patched = True
    logger.info("Patched DingTalkHandler for dogfooding feedback flow")


def register_channel_hooks() -> None:
    """Register all channel-level dogfooding hooks."""
    patch_dingtalk_channel()
    patch_dingtalk_handler()
