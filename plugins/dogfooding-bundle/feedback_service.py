# -*- coding: utf-8 -*-
# pylint: disable=relative-beyond-top-level
"""Dogfooding feedback business logic."""

from __future__ import annotations

import logging
import re
import time
import uuid
from dataclasses import dataclass, field
from typing import Dict, List, Optional

from .tracking import (
    MODEL_ID,
    SCORE_LABEL_ZH,
    emit_feedback_tracking,
    emit_qa_record,
    lookup_parent_rpc_id,
)

logger = logging.getLogger(__name__)

DOGFOODING_META_KEY = "qwenpaw_dogfooding"

BAD_FEEDBACK_REASONS: List[str] = [
    "没理解我的意图",
    "任务没有完成",
    "步骤太繁琐",
    "结果有误",
    "回复风格有问题",
    "存在安全风险",
    "响应太慢",
    "其他",
]

SCORE_LABEL_FROM_ZH = {v: k for k, v in SCORE_LABEL_ZH.items()}


@dataclass
class PendingDingTalkFeedback:
    """Multi-step DingTalk feedback state for one conversation."""

    trace_id: str
    conversation_id: str
    session_id: str
    channel_type: str = "dingtalk"
    user_id: str = ""
    user_name: str = ""
    step: str = "score"  # score | reason
    score_label: str = ""
    created_at: float = field(default_factory=time.time)


_pending_dingtalk: Dict[str, PendingDingTalkFeedback] = {}


def is_dogfooding_model(model_name: str) -> bool:
    """Return True when the active model is a dogfooding model."""
    name = (model_name or "").strip().lower()
    return "dogfooding" in name


def build_dogfooding_meta(
    *,
    trace_id: str,
    session_id: str,
    model_name: str = MODEL_ID,
    response_id: str = "",
) -> dict:
    """Metadata blob attached to assistant messages for frontend/channel."""
    return {
        "trace_id": trace_id,
        "session_id": session_id,
        "model_id": model_name,
        "response_id": response_id or f"resp_{uuid.uuid4().hex[:12]}",
    }


def submit_feedback(
    *,
    trace_id: str,
    conversation_id: str,
    score_label: str,
    channel_type: str = "web",
    feedback_reason: str = "",
    feedback_comment: str = "",
    user_id: str = "",
    user_name: str = "",
    model_name: str = MODEL_ID,
    parent_rpc_id: str = "",
) -> dict:
    """Validate and emit feedback tracking."""
    label = score_label.strip().lower()
    if label not in SCORE_LABEL_ZH:
        raise ValueError(f"Unsupported score_label: {score_label!r}")
    if label == "bad" and not (feedback_reason or "").strip():
        raise ValueError("feedback_reason is required when score_label is bad")

    return emit_feedback_tracking(
        trace_id=trace_id,
        conversation_id=conversation_id,
        score_label=label,
        channel_type=channel_type,
        feedback_reason=feedback_reason.strip(),
        feedback_comment=feedback_comment.strip(),
        user_id=user_id,
        user_name=user_name,
        model_name=model_name,
        parent_rpc_id=parent_rpc_id,
    )


def record_qa_turn(
    *,
    trace_id: str,
    conversation_id: str,
    prompt_message: str,
    response_message: str,
    channel_type: str = "web",
    user_id: str = "",
    user_name: str = "",
    model_name: str = MODEL_ID,
    parent_rpc_id: str = "",
) -> dict:
    """Backflow one completed Q&A turn."""
    if not trace_id or not conversation_id:
        return {}
    if not (prompt_message or response_message):
        return {}
    return emit_qa_record(
        trace_id=trace_id,
        conversation_id=conversation_id,
        user_id=user_id,
        user_name=user_name,
        channel_type=channel_type,
        prompt_message=prompt_message,
        response_message=response_message,
        model_name=model_name,
        parent_rpc_id=parent_rpc_id,
    )


def _pending_key(conversation_id: str) -> str:
    return conversation_id.strip()


def start_dingtalk_feedback(
    *,
    trace_id: str,
    conversation_id: str,
    session_id: str,
    user_id: str = "",
    user_name: str = "",
) -> None:
    """Begin DingTalk multi-step feedback for a conversation."""
    key = _pending_key(conversation_id)
    if not key:
        return
    _pending_dingtalk[key] = PendingDingTalkFeedback(
        trace_id=trace_id,
        conversation_id=conversation_id,
        session_id=session_id,
        user_id=user_id,
        user_name=user_name,
    )


def clear_dingtalk_feedback(conversation_id: str) -> None:
    _pending_dingtalk.pop(_pending_key(conversation_id), None)


def parse_dingtalk_feedback_message(text: str) -> Optional[str]:
    """Parse explicit feedback prefix from button callback text."""
    raw = (text or "").strip()
    m = re.match(
        (
            r"^__dogfooding_feedback:"
            r"(?P<label>bad|fine|good)"
            r"(?::(?P<tid>[a-f0-9]+))?$"
        ),
        raw,
        re.I,
    )
    if m:
        return m.group("label").lower()
    if raw in SCORE_LABEL_ZH:
        return SCORE_LABEL_FROM_ZH[raw]
    if raw in ("1", "2", "3"):
        return ("bad", "fine", "good")[int(raw) - 1]
    return None


def parse_dingtalk_reason_message(text: str) -> Optional[str]:
    """Parse reason selection from numbered reply."""
    raw = (text or "").strip()
    if raw.isdigit():
        idx = int(raw)
        if 1 <= idx <= len(BAD_FEEDBACK_REASONS):
            return BAD_FEEDBACK_REASONS[idx - 1]
    for reason in BAD_FEEDBACK_REASONS:
        if raw == reason:
            return reason
    return None


def handle_dingtalk_inbound_text(  # pylint: disable=too-many-return-statements
    *,
    conversation_id: str,
    text: str,
    user_id: str = "",
    user_name: str = "",
) -> Optional[dict]:
    """Handle inbound DingTalk text as feedback continuation.

    Returns feedback record dict when completed, else None.
    """
    key = _pending_key(conversation_id)
    pending = _pending_dingtalk.get(key)
    if pending is None:
        return None

    if pending.step == "score":
        label = parse_dingtalk_feedback_message(text)
        if not label:
            return None
        if label == "bad":
            pending.score_label = label
            pending.step = "reason"
            return {"action": "ask_reason", "pending": pending}
        record = submit_feedback(
            trace_id=pending.trace_id,
            conversation_id=pending.conversation_id or pending.session_id,
            score_label=label,
            channel_type=pending.channel_type,
            user_id=user_id or pending.user_id,
            user_name=user_name or pending.user_name,
            parent_rpc_id=lookup_parent_rpc_id(
                conversation_id=pending.conversation_id or pending.session_id,
                trace_id=pending.trace_id,
            ),
        )
        clear_dingtalk_feedback(conversation_id)
        return {"action": "submitted", "record": record}

    if pending.step == "reason":
        reason = parse_dingtalk_reason_message(text) or text.strip()
        if not reason:
            return None
        record = submit_feedback(
            trace_id=pending.trace_id,
            conversation_id=pending.conversation_id or pending.session_id,
            score_label="bad",
            channel_type=pending.channel_type,
            feedback_reason=reason,
            user_id=user_id or pending.user_id,
            user_name=user_name or pending.user_name,
            parent_rpc_id=lookup_parent_rpc_id(
                conversation_id=pending.conversation_id or pending.session_id,
                trace_id=pending.trace_id,
            ),
        )
        clear_dingtalk_feedback(conversation_id)
        return {"action": "submitted", "record": record}

    return None


def build_dingtalk_feedback_action_card(trace_id: str) -> dict:
    """Build DingTalk actionCard payload for score selection."""
    btns = []
    for label, zh in SCORE_LABEL_ZH.items():
        btns.append(
            {
                "title": zh,
                "actionURL": (
                    f"dtmd://dingtalkclient/sendMessage?"
                    f"content=__dogfooding_feedback:{label}:{trace_id}"
                ),
            },
        )
    return {
        "msgtype": "actionCard",
        "actionCard": {
            "title": "这个回答对你有帮助吗？",
            "text": "请为本次 AI 回复评分",
            "btnOrientation": "0",
            "btns": btns,
        },
    }


def build_dingtalk_reason_action_card() -> dict:
    """Build DingTalk actionCard for bad-feedback reason selection."""
    lines = ["很抱歉没有达到您的期望，请选择主要问题："]
    for idx, reason in enumerate(BAD_FEEDBACK_REASONS, start=1):
        lines.append(f"{idx}. {reason}")
    return {
        "msgtype": "actionCard",
        "actionCard": {
            "title": "请告诉我们哪里不好",
            "text": "\n".join(lines),
            "btnOrientation": "1",
            "btns": [
                {
                    "title": reason[:20],
                    "actionURL": (
                        "dtmd://dingtalkclient/sendMessage?" f"content={idx}"
                    ),
                }
                for idx, reason in enumerate(BAD_FEEDBACK_REASONS[:4], start=1)
            ],
        },
    }
