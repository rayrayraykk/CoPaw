# -*- coding: utf-8 -*-
"""Dogfooding tracking & data backflow per 集团AI产品埋点规范."""

from __future__ import annotations

import json
import logging
import time
from pathlib import Path
from typing import Any, Dict, Optional

logger = logging.getLogger(__name__)

# ── Product constants ─────────────────────────────────────────────────────
PRODUCT_CODE = "qwenpaw"
PRODUCT_VERSION = "1.1.12.post2"
MODEL_ID = "Qwen3.7-Max-DogFooding"
TRACKING_PID = "idealab_talk"
TRACKING_SCENE = "chat"

# User-behavior event keys (gmkey is always CLK for interaction events)
GMKEY_CLK = "CLK"
LOGKEY_FEEDBACK = "fu.track.interaction.feedback"

SCORE_LABEL_TO_VALUE = {
    "bad": 1,
    "fine": 2,
    "good": 3,
}

SCORE_LABEL_ZH = {
    "bad": "糟糕",
    "fine": "一般",
    "good": "优秀",
}

# Latest chat span context per conversation, used to nest feedback under the
# model reply instead of creating independent trace nodes.
_LATEST_SPAN_CTX_BY_CONV: Dict[str, Dict[str, str]] = {}
_LATEST_RPC_BY_TRACE: Dict[str, str] = {}
_EAGLEEYE_RPC_ID_ATTR = "eagleeye.rpc_id"
_CONV_ID_ATTR = "gen_ai.conversation.id"


def remember_trace_id(session_id: str, trace_id: str) -> None:
    if not session_id or not trace_id:
        return
    entry = _LATEST_SPAN_CTX_BY_CONV.setdefault(session_id, {})
    entry["trace_id"] = trace_id


def remember_chat_span_link(
    span,
    *,
    session_id: str = "",
) -> None:
    """Capture chat span rpc_id so feedback can nest under the model reply."""
    if span is None:
        return
    attrs = getattr(span, "attributes", None) or {}
    rpc_id = attrs.get(_EAGLEEYE_RPC_ID_ATTR)
    if not rpc_id:
        return
    name = (getattr(span, "name", None) or "").lower()
    if "chat" not in name and "llm" not in name:
        return
    ctx = span.get_span_context()
    if not ctx or not ctx.trace_id:
        return
    trace_id = format(ctx.trace_id, "032x")
    rpc_id_str = str(rpc_id)
    _LATEST_RPC_BY_TRACE[trace_id] = rpc_id_str
    conv_id = str(attrs.get(_CONV_ID_ATTR) or session_id or "")
    if conv_id:
        entry = _LATEST_SPAN_CTX_BY_CONV.setdefault(conv_id, {})
        entry["trace_id"] = trace_id
        entry["parent_rpc_id"] = rpc_id_str


def lookup_parent_rpc_id(
    *,
    conversation_id: str = "",
    trace_id: str = "",
) -> str:
    if conversation_id:
        rpc_id = _LATEST_SPAN_CTX_BY_CONV.get(conversation_id, {}).get(
            "parent_rpc_id",
            "",
        )
        if rpc_id:
            return rpc_id
    if trace_id:
        return _LATEST_RPC_BY_TRACE.get(trace_id, "")
    return ""


def lookup_trace_id(conversation_id: str) -> str:
    return _LATEST_SPAN_CTX_BY_CONV.get(conversation_id, {}).get(
        "trace_id",
        "",
    )


def build_sam(
    conversation_id: str,
    trace_id: str,
    *,
    pid: str = TRACKING_PID,
    scene: str = TRACKING_SCENE,
) -> str:
    """Build composite sam id from pid, scene, conversation, trace."""
    return f"{pid}.{scene}.{conversation_id}.{trace_id}"


def _backflow_dir() -> Path:
    from qwenpaw.constant import WORKING_DIR

    return WORKING_DIR / "dogfooding" / "backflow"


def _append_backflow_record(record: Dict[str, Any]) -> None:
    """Append one JSON line to local backflow log (best-effort)."""
    try:
        target = _backflow_dir() / "records.jsonl"
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open("a", encoding="utf-8") as fh:
            fh.write(json.dumps(record, ensure_ascii=False) + "\n")
    except OSError as exc:
        logger.warning("Failed to write dogfooding backflow record: %s", exc)


def _normalize_otel_value(value: Any) -> bool | str | int | float:
    """Coerce values to OTel-compatible attribute types."""
    if isinstance(value, bool):
        return value
    if isinstance(value, int):
        return value
    if isinstance(value, float):
        return value
    if isinstance(value, str):
        return value
    return json.dumps(value, ensure_ascii=False)


def _context_from_trace_id(trace_id: str, parent_rpc_id: str = ""):
    """Rebuild EagleEye/OTel parent context for nesting under the chat span."""
    if not trace_id:
        return None
    try:
        from opentelemetry.propagators.eagleeye import (
            EAGLEEYE_RPCID_HEADER_KEY,
            EAGLEEYE_TRACEID_HEADER_KEY,
            EagleeyePropagator,
        )

        propagator = EagleeyePropagator()
        carrier = {EAGLEEYE_TRACEID_HEADER_KEY: trace_id}
        if parent_rpc_id:
            carrier[EAGLEEYE_RPCID_HEADER_KEY] = parent_rpc_id
        return propagator.extract(carrier)
    except Exception:
        logger.debug("Failed to rebuild trace context", exc_info=True)
        return None


def _force_flush_spans() -> None:
    """Best-effort flush so feedback spans reach the OTLP exporter quickly."""
    try:
        from opentelemetry import trace as otel_trace
        from opentelemetry.sdk.trace import TracerProvider as SDKTracerProvider

        provider = otel_trace.get_tracer_provider()
        if isinstance(provider, SDKTracerProvider):
            provider.force_flush(5000)
    except Exception:
        logger.debug("Span force_flush skipped", exc_info=True)


def _emit_platform_span(
    span_name: str,
    attributes: Dict[str, Any],
    *,
    trace_id: str = "",
    conversation_id: str = "",
    user_id: str = "",
    parent_rpc_id: str = "",
) -> None:
    """Export one tracking record as an OTel span via AgentTrack OTLP pipeline.

    Feedback is submitted outside the original LLM request, so there is usually
    no active recording span. Pass the chat span's ``eagleeye.rpc_id`` as
    ``parent_rpc_id`` so the platform nests feedback under the model reply
    instead of creating an independent root node.
    """
    try:
        from opentelemetry import context as context_api
        from opentelemetry import trace as otel_trace

        parent_ctx = _context_from_trace_id(trace_id, parent_rpc_id)
        token = None
        if parent_ctx is not None:
            token = context_api.attach(parent_ctx)

        try:
            tracer = otel_trace.get_tracer("qwenpaw.dogfooding")
            with tracer.start_as_current_span(span_name) as span:
                if not span.is_recording():
                    return
                if conversation_id:
                    span.set_attribute(
                        "gen_ai.conversation.id",
                        conversation_id,
                    )
                if user_id:
                    span.set_attribute("alibaba.base.emp_id", user_id)
                event_attrs: Dict[str, bool | str | int | float] = {}
                for key, value in attributes.items():
                    if value is None or value == "":
                        continue
                    normalized = _normalize_otel_value(value)
                    span.set_attribute(key, normalized)
                    event_attrs[key] = normalized
                if event_attrs:
                    span.add_event(span_name, attributes=event_attrs)
        finally:
            if token is not None:
                context_api.detach(token)

        _force_flush_spans()
    except Exception:
        logger.debug("Platform span export skipped", exc_info=True)


def emit_qa_record(
    *,
    trace_id: str,
    conversation_id: str,
    user_id: str = "",
    user_name: str = "",
    channel_type: str = "web",
    prompt_message: str = "",
    response_message: str = "",
    parent_trace_id: str = "",
    root_trace_id: str = "",
    model_name: str = MODEL_ID,
    parent_rpc_id: str = "",
    extra: Optional[Dict[str, Any]] = None,
) -> Dict[str, Any]:
    """Backflow one Q&A record (问答详情)."""
    record: Dict[str, Any] = {
        "record_type": "qa_detail",
        "event_timestamp": int(time.time() * 1000),
        "trace_id": trace_id,
        "parent_trace_id": parent_trace_id or trace_id,
        "root_trace_id": root_trace_id or trace_id,
        "sam": build_sam(conversation_id, trace_id),
        "user_id": user_id,
        "user_name": user_name,
        "product_code": PRODUCT_CODE,
        "product_version": PRODUCT_VERSION,
        "channel_type": channel_type,
        "model_name": model_name,
        "modelId": model_name,
        "prompt_message": prompt_message,
        "response_message": response_message,
    }
    if extra:
        record.update(extra)

    _append_backflow_record(record)
    _emit_platform_span(
        "dogfooding.qa_detail",
        record,
        trace_id=trace_id,
        conversation_id=conversation_id,
        user_id=user_id,
        parent_rpc_id=parent_rpc_id,
    )
    logger.info(
        "Dogfooding QA backflow: trace=%s session=%s channel=%s",
        trace_id,
        conversation_id,
        channel_type,
    )
    return record


def emit_feedback_tracking(
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
) -> Dict[str, Any]:
    """Emit user feedback per 用户行为埋点规范 (fu.track.interaction.feedback)."""
    label = score_label.strip().lower()
    if label not in SCORE_LABEL_TO_VALUE:
        raise ValueError(f"Invalid score_label: {score_label!r}")

    score = SCORE_LABEL_TO_VALUE[label]
    record: Dict[str, Any] = {
        "record_type": "interaction_feedback",
        "event_timestamp": int(time.time() * 1000),
        "gmkey": GMKEY_CLK,
        "logkey": LOGKEY_FEEDBACK,
        "sam": build_sam(conversation_id, trace_id),
        "trace_id": trace_id,
        "modelId": model_name,
        "model_name": model_name,
        "score": score,
        "score_label": label,
        "feedback_score": SCORE_LABEL_ZH.get(label, label),
        "feedback_reason": feedback_reason,
        "feedback_comment": feedback_comment,
        "product_code": PRODUCT_CODE,
        "product_version": PRODUCT_VERSION,
        "channel_type": channel_type,
        "user_id": user_id,
        "user_name": user_name,
    }

    _append_backflow_record(record)
    _emit_platform_span(
        LOGKEY_FEEDBACK,
        record,
        trace_id=trace_id,
        conversation_id=conversation_id,
        user_id=user_id,
        parent_rpc_id=parent_rpc_id,
    )
    logger.info(
        "Dogfooding feedback: trace=%s label=%s score=%s channel=%s",
        trace_id,
        label,
        score,
        channel_type,
    )
    return record
