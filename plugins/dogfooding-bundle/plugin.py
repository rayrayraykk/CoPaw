# -*- coding: utf-8 -*-
"""Dogfooding Bundle Plugin.

Internal org bundle that registers capabilities in one shot:

1. AgentScope Dogfooding provider (Qwen3.7-Max-DogFooding)
2. AgentTrack startup hook + Q&A / feedback data backflow
3. Console feedback UI + DingTalk feedback action cards
4. /feedback command query-rewrite hook
5. Dogfooding account API
"""

import json
import logging
import uuid
from contextvars import ContextVar
from pathlib import Path
from typing import Literal

from fastapi import APIRouter, HTTPException
from pydantic import BaseModel, Field

from qwenpaw.constant import WORKING_DIR
from qwenpaw.plugins.api import PluginApi
from qwenpaw.providers.openai_provider import OpenAIProvider
from qwenpaw.providers.provider import ModelInfo

# pylint: disable=relative-beyond-top-level,protected-access
from .feedback_service import (
    DOGFOODING_META_KEY,
    build_dogfooding_meta,
    is_dogfooding_model,
    record_qa_turn,
    submit_feedback,
)
from .tracking import (
    MODEL_ID,
    lookup_parent_rpc_id,
    lookup_trace_id,
    remember_chat_span_link,
    remember_trace_id,
)

logger = logging.getLogger(__name__)

# Per-request trace context for backflow + frontend feedback binding
_trace_user_id: ContextVar[str] = ContextVar(
    "bundle_trace_user_id",
    default="",
)
_trace_id: ContextVar[str] = ContextVar("bundle_trace_id", default="")
_trace_session_id: ContextVar[str] = ContextVar(
    "bundle_trace_session_id",
    default="",
)
_trace_channel: ContextVar[str] = ContextVar(
    "bundle_trace_channel",
    default="web",
)

# Latest dogfooding trace context is tracked in tracking.py
# for feedback nesting.

# EvaPlus span attribute names
_EMP_ID_ATTR = "alibaba.base.emp_id"
_CONV_ID_ATTR = "gen_ai.conversation.id"

# ── AgentScope provider constants ────────────────────────────────────────
_BASE_URL = "http://proxy.agentscope.design/v1"
# Legacy endpoints to auto-migrate away from. The raw-IP:8081 form is
# blocked by the AliLang network agent for x86_64/Rosetta processes
# (connect() returns EBADF), so route through the domain on port 80.
_LEGACY_BASE_URLS = {
    "https://proxy.agentscope.design/v1",
    "http://121.43.136.192:8081/v1",
    "https://121.43.136.192:8081/v1",
}
_PROVIDER_ID = "agentscope-dogfooding"

_DEFAULT_MODELS = [
    ModelInfo(
        id="qwen3.7-max-dogfooding",
        name="Qwen3.7-Max-DogFooding",
        supports_multimodal=True,
        supports_image=True,
        supports_video=False,
    ),
]


class AgentScopeDogfoodingProvider(OpenAIProvider):
    """OpenAI-compatible provider via the AgentScope proxy."""

    @staticmethod
    def get_default_models():
        """Return the pre-defined model list for this provider.

        Returns:
            List of ModelInfo objects.
        """
        return _DEFAULT_MODELS


# ── Dogfooding account API ────────────────────────────────────────────────


class DogfoodingAccountPayload(BaseModel):
    """Request body for saving the dogfooding user account."""

    user_account: str = Field(..., min_length=1)
    proxy_api_key: str = ""


class DogfoodingProviderConfigPayload(BaseModel):
    """Request body for saving the dogfooding provider API key."""

    proxy_api_key: str = Field(..., min_length=1)


class DogfoodingAccountResponse(BaseModel):
    """Response body after saving the dogfooding user account."""

    ok: bool
    path: str
    provider_configured: bool = False


class DogfoodingProviderConfigResponse(BaseModel):
    """Response body after saving the provider API key."""

    ok: bool
    provider_id: str


def _dogfooding_dir() -> Path:
    return WORKING_DIR / "dogfooding"


def _user_account_path() -> Path:
    return _dogfooding_dir() / "user_account.json"


# mtime-based cache: (cached_mtime, cached_user_id)
_user_id_cache: tuple[float, str] = (0.0, "")


def _read_user_id_cached() -> str:
    """Read user_id from user_account.json with mtime-based cache.

    Returns empty string when the file is absent or malformed.
    """
    global _user_id_cache
    path = _user_account_path()
    try:
        mtime = path.stat().st_mtime
    except OSError:
        return ""
    cached_mtime, cached_value = _user_id_cache
    if mtime == cached_mtime:
        return cached_value
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
        value = str(data.get("user_account", ""))
    except Exception as exc:
        logger.warning(f"Failed to read user_account.json: {exc}")
        value = ""
    _user_id_cache = (mtime, value)
    return value


def _save_provider_api_key(api_key: str) -> None:
    """Persist proxy API key into the AgentScope Dogfooding provider config."""
    from qwenpaw.providers.provider_manager import ProviderManager

    trimmed = api_key.strip()
    if not trimmed:
        raise HTTPException(status_code=400, detail="proxy_api_key is empty")

    manager = ProviderManager.get_instance()
    ok = manager.update_provider(
        _PROVIDER_ID,
        {"api_key": trimmed, "base_url": _BASE_URL},
    )
    if not ok:
        raise HTTPException(
            status_code=404,
            detail=f"Provider '{_PROVIDER_ID}' not found",
        )
    logger.info("Dogfooding provider API key saved via SSO login")


def _build_dogfooding_account_router() -> APIRouter:
    """Build routes mounted under /api/dogfooding-account."""
    router = APIRouter()

    @router.post("/", response_model=DogfoodingAccountResponse)
    def save_user_account(
        payload: DogfoodingAccountPayload,
    ) -> DogfoodingAccountResponse:
        """Save user_account.json under the QwenPaw working directory."""
        target = _user_account_path()
        try:
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(
                json.dumps(
                    {"user_account": payload.user_account},
                    ensure_ascii=False,
                    indent=2,
                )
                + "\n",
                encoding="utf-8",
            )
        except OSError as exc:
            logger.exception("Failed to save dogfooding user account")
            raise HTTPException(
                status_code=500,
                detail="Failed to save user account",
            ) from exc

        provider_configured = False
        proxy_api_key = payload.proxy_api_key.strip()
        if proxy_api_key:
            _save_provider_api_key(proxy_api_key)
            provider_configured = True

        return DogfoodingAccountResponse(
            ok=True,
            path=str(target),
            provider_configured=provider_configured,
        )

    @router.post(
        "/configure-provider",
        response_model=DogfoodingProviderConfigResponse,
    )
    def configure_provider_api_key(
        payload: DogfoodingProviderConfigPayload,
    ) -> DogfoodingProviderConfigResponse:
        """Save SSO proxy API key into the dogfooding provider config."""
        _save_provider_api_key(payload.proxy_api_key)
        return DogfoodingProviderConfigResponse(
            ok=True,
            provider_id=_PROVIDER_ID,
        )

    return router


class DogfoodingFeedbackPayload(BaseModel):
    """Request body for per-message feedback."""

    trace_id: str = ""
    conversation_id: str = Field(..., min_length=1)
    score_label: Literal["bad", "fine", "good"]
    channel_type: str = "web"
    feedback_reason: str = ""
    feedback_comment: str = ""
    response_id: str = ""


class DogfoodingFeedbackResponse(BaseModel):
    """Response after feedback submission."""

    ok: bool
    record: dict


def _build_dogfooding_feedback_router() -> APIRouter:
    """Build routes mounted under /api/dogfooding-feedback."""
    router = APIRouter()

    @router.post("/", response_model=DogfoodingFeedbackResponse)
    def post_feedback(
        payload: DogfoodingFeedbackPayload,
    ) -> DogfoodingFeedbackResponse:
        user_id = _read_user_id_cached()
        # A just-streamed reply may not carry its trace_id yet; backfill it
        # from the conversation's most recent dogfooding turn so feedback
        # stays correlated with the Q&A backflow record.
        trace_id = (payload.trace_id or "").strip()
        if not trace_id:
            trace_id = lookup_trace_id(payload.conversation_id)
        if not trace_id:
            trace_id = (payload.response_id or "").strip() or (
                f"resp_{uuid.uuid4().hex[:12]}"
            )
        parent_rpc_id = lookup_parent_rpc_id(
            conversation_id=payload.conversation_id,
            trace_id=trace_id,
        )
        try:
            record = submit_feedback(
                trace_id=trace_id,
                conversation_id=payload.conversation_id,
                score_label=payload.score_label,
                channel_type=payload.channel_type or "web",
                feedback_reason=payload.feedback_reason,
                feedback_comment=payload.feedback_comment,
                user_id=user_id,
                parent_rpc_id=parent_rpc_id,
            )
        except ValueError as exc:
            raise HTTPException(status_code=400, detail=str(exc)) from exc
        return DogfoodingFeedbackResponse(ok=True, record=record)

    return router


# ── Bundle plugin ────────────────────────────────────────────────────────


class DogfoodingBundlePlugin:
    """Bundle plugin entry point.

    Registers all three internal-org capabilities with a single
    install / uninstall operation.
    """

    def register(self, api: PluginApi):
        """Register all capabilities bundled in this plugin.

        Args:
            api: PluginApi instance provided by the plugin loader.
        """
        self._register_provider(api)
        self._register_agenttrack_hook(api)
        self._register_feedback_hook(api)
        self._register_feedback_router(api)
        self._register_account_router(api)
        logger.info("Dogfooding Bundle fully registered")

    # ── provider ──────────────────────────────────────────────────────────

    def _register_provider(self, api: PluginApi):
        """Register the AgentScope Dogfooding LLM provider.

        Args:
            api: PluginApi instance.
        """
        _migrate_saved_provider_config()
        api.register_provider(
            provider_id=_PROVIDER_ID,
            provider_class=AgentScopeDogfoodingProvider,
            label="AgentScope Dogfooding",
            base_url=_BASE_URL,
            chat_model="OpenAIChatModel",
            require_api_key=True,
        )
        logger.info("AgentScope Dogfooding provider registered")

    # ── AgentTrack hook ───────────────────────────────────────────────────

    def _register_agenttrack_hook(self, api: PluginApi):
        """Register the AgentTrack startup hook.

        Args:
            api: PluginApi instance.
        """

        def startup_hook():
            """Initialise AgentTrack and register span processor."""
            try:
                logger.info("=== AgentTrack Initialization ===")
                from agenttrack.sdk import AgentTrack
                from traceloop.sdk.instruments import Instruments

                AgentTrack.init(
                    app_name="qwenpaw",
                    block_instruments={Instruments.TERMINAL_BENCH},
                )
                logger.info("AgentTrack initialized (app_name=qwenpaw)")
            except ImportError as exc:
                logger.error(
                    f"Failed to import AgentTrack SDK: {exc}. "
                    "Please ensure agenttrack-sdk is installed.",
                    exc_info=True,
                )
                return
            except Exception as exc:
                logger.error(
                    f"Failed to initialize AgentTrack: {exc}",
                    exc_info=True,
                )
                return

            _register_span_processor()

        api.register_startup_hook(
            hook_name="agenttrack_init",
            callback=startup_hook,
            priority=0,
        )
        logger.info("AgentTrack startup hook registered")

    # ── /feedback command hook ────────────────────────────────────────────

    def _register_feedback_hook(self, api: PluginApi):
        """Register the /feedback query-rewrite startup hook.

        Args:
            api: PluginApi instance.
        """

        def startup():
            self._patch_query_handler()
            self._patch_turn_metadata()
            self._patch_finalize_turn()
            from .channel_hooks import register_channel_hooks

            register_channel_hooks()

        api.register_startup_hook(
            hook_name="dogfooding_feedback_runtime",
            callback=startup,
            priority=50,
        )
        logger.info("Dogfooding feedback runtime hook registered")

    def _register_feedback_router(self, api: PluginApi):
        """Register feedback submission API."""
        api.register_http_router(
            _build_dogfooding_feedback_router(),
            prefix="/dogfooding-feedback",
            tags=["dogfooding-feedback"],
        )
        logger.info(
            "Dogfooding feedback API registered at "
            "POST /api/dogfooding-feedback/",
        )

    # ── dogfooding account API ────────────────────────────────────────────

    def _register_account_router(self, api: PluginApi):
        """Register the dogfooding account API router."""
        api.register_http_router(
            _build_dogfooding_account_router(),
            prefix="/dogfooding-account",
            tags=["dogfooding-account"],
        )
        logger.info(
            "Dogfooding account API registered at "
            "POST /api/dogfooding-account/ and "
            "POST /api/dogfooding-account/configure-provider",
        )

    def _patch_query_handler(self):
        """Monkey-patch AgentRunner: inject trace attrs, rewrite /feedback."""
        from qwenpaw.app.runner.runner import AgentRunner

        # pylint: disable-next=relative-beyond-top-level
        from .query_rewriter import FeedbackQueryRewriter

        original_query_handler = AgentRunner.query_handler

        async def patched_query_handler(self, msgs, request=None, **kwargs):
            """Query handler: stamps trace context, rewrites /feedback."""
            session_id = getattr(request, "session_id", "") or ""
            channel = getattr(request, "channel", "") or "web"
            user_id = _read_user_id_cached() or (
                getattr(request, "user_id", "") or ""
            )
            trace_id = _current_trace_id()
            _trace_user_id.set(user_id)
            _trace_id.set(trace_id)
            _trace_session_id.set(session_id)
            _trace_channel.set(channel)

            if request is not None:
                setattr(
                    request,
                    "_dogfooding_trace",
                    {
                        "trace_id": trace_id,
                        "session_id": session_id,
                        "channel": channel,
                        "user_id": user_id,
                    },
                )

            logger.info(
                "Trace context: session=%r user=%r trace=%r channel=%r",
                session_id,
                user_id,
                trace_id,
                channel,
            )

            _stamp_current_span(session_id, user_id, trace_id)

            # Override agentscope run_id so every LLM span's
            # gen_ai.conversation.id equals the QwenPaw session_id.
            # _config.run_id is a ContextVar → asyncio-safe per request.
            if session_id:
                try:
                    from agentscope import _config as _as_config

                    _as_config.run_id = session_id
                except Exception as exc:
                    logger.debug(
                        f"Could not override agentscope run_id: {exc}",
                    )

            if msgs:
                last_msg = msgs[-1]
                if hasattr(last_msg, "content"):
                    content_list = (
                        last_msg.content
                        if isinstance(last_msg.content, list)
                        else [last_msg.content]
                    )
                    for content_item in content_list:
                        if (
                            isinstance(content_item, dict)
                            and content_item.get("type") == "text"
                        ):
                            text = content_item.get("text", "")
                            if FeedbackQueryRewriter.should_rewrite(text):
                                rewritten = FeedbackQueryRewriter.rewrite(
                                    text,
                                )
                                logger.info(
                                    f"Rewriting /feedback: "
                                    f"{text[:50]} -> {rewritten[:50]}",
                                )
                                content_item["text"] = rewritten
                                break

            async for result in original_query_handler(
                self,
                msgs,
                request,
                **kwargs,
            ):
                yield result

        AgentRunner.query_handler = patched_query_handler
        logger.info("Patched AgentRunner.query_handler")

    def _patch_turn_metadata(self):
        """Attach dogfooding trace metadata to closing assistant messages."""
        from qwenpaw.token_usage import turn_usage as tu

        if getattr(tu, "_dogfooding_meta_patched", False):
            return

        original = tu.attach_turn_usage_metadata

        def patched(memory, turn, ctx):
            result = original(memory, turn, ctx)
            trace_id = _trace_id.get()
            session_id = _trace_session_id.get()
            if not trace_id or not session_id:
                return result
            msg = tu.find_turn_closing_assistant(memory)
            if msg is None:
                return result
            meta = getattr(msg, "metadata", None)
            if not isinstance(meta, dict):
                meta = {}
            model_name = ""
            if turn and isinstance(turn, dict):
                model_name = str(turn.get("model_name") or "")
            if not is_dogfooding_model(model_name):
                return result
            meta[DOGFOODING_META_KEY] = build_dogfooding_meta(
                trace_id=trace_id,
                session_id=session_id,
                model_name=model_name or MODEL_ID,
            )
            msg.metadata = meta
            remember_trace_id(session_id, trace_id)
            return result

        tu.attach_turn_usage_metadata = patched
        tu._dogfooding_meta_patched = True
        logger.info("Patched attach_turn_usage_metadata for dogfooding meta")

    def _patch_finalize_turn(self):
        """Backflow Q&A records after each console/channel turn."""
        from qwenpaw.token_usage import turn_usage as tu

        if getattr(tu, "_dogfooding_finalize_patched", False):
            return

        original = tu.finalize_console_turn_usage

        async def patched(*, session, session_id, user_id, channel, agent_id):
            turn, ctx = await original(
                session=session,
                session_id=session_id,
                user_id=user_id,
                channel=channel,
                agent_id=agent_id,
            )
            trace_id = _trace_id.get() or _current_trace_id()
            model_name = ""
            if turn and isinstance(turn, dict):
                model_name = str(turn.get("model_name") or "")
            if not is_dogfooding_model(model_name):
                return turn, ctx
            if session_id and trace_id:
                remember_trace_id(session_id, trace_id)
            parent_rpc_id = lookup_parent_rpc_id(
                conversation_id=session_id,
                trace_id=trace_id,
            )
            try:
                from qwenpaw.agents.context.agent_context import AgentContext
                from qwenpaw.agents.utils.estimate_token_counter import (
                    EstimatedTokenCounter,
                )

                state = await session.get_session_state_dict(
                    session_id=session_id,
                    user_id=user_id,
                    channel=channel,
                    allow_not_exist=True,
                )
                memory_state = (state or {}).get("agent", {}).get("memory", {})
                prompt_text = ""
                response_text = ""
                if memory_state:
                    memory = AgentContext(EstimatedTokenCounter())
                    memory.load_state_dict(memory_state, strict=False)
                    content = getattr(memory, "content", None) or []
                    user_parts = []
                    assistant_parts = []
                    for msg, _marks in content:
                        role = getattr(msg, "role", None)
                        body = getattr(msg, "content", None)
                        text = _message_text(body)
                        if role == "user" and text:
                            user_parts.append(text)
                        elif role == "assistant" and text:
                            assistant_parts.append(text)
                    if user_parts:
                        prompt_text = user_parts[-1]
                    if assistant_parts:
                        response_text = assistant_parts[-1]
                record_qa_turn(
                    trace_id=trace_id,
                    conversation_id=session_id,
                    prompt_message=prompt_text,
                    response_message=response_text,
                    channel_type=channel or "web",
                    user_id=user_id or _read_user_id_cached(),
                    model_name=model_name or MODEL_ID,
                    parent_rpc_id=parent_rpc_id,
                )
            except Exception:
                logger.debug("Dogfooding QA backflow skipped", exc_info=True)
            return turn, ctx

        tu.finalize_console_turn_usage = patched
        _replace_token_usage_export("finalize_console_turn_usage", patched)
        tu._dogfooding_finalize_patched = True
        logger.info("Patched finalize_console_turn_usage for QA backflow")


def _replace_token_usage_export(name: str, value) -> None:
    """Patch turn_usage and the qwenpaw.token_usage package re-export.

    Console channel imports ``finalize_console_turn_usage`` from the package
    (__init__), not from turn_usage directly, so both must stay in sync.
    """
    from qwenpaw.token_usage import turn_usage as tu
    import qwenpaw.token_usage as tu_pkg

    setattr(tu, name, value)
    setattr(tu_pkg, name, value)


def _message_text(body) -> str:
    if isinstance(body, str):
        return body.strip()
    if isinstance(body, list):
        parts = []
        for item in body:
            if isinstance(item, dict) and item.get("type") == "text":
                parts.append(str(item.get("text", "")))
        return "\n".join(parts).strip()
    return ""


def _current_trace_id() -> str:
    try:
        from opentelemetry import trace as otel_trace

        span = otel_trace.get_current_span()
        ctx = span.get_span_context()
        if ctx.trace_id:
            return format(ctx.trace_id, "032x")
    except Exception:
        pass
    return uuid.uuid4().hex


# ── SpanProcessor ─────────────────────────────────────────────────────────
# Stamps alibaba.base.emp_id (EvaPlus 用户ID) on every new OTel span
# created after patched_query_handler sets _trace_user_id.


class _BundleSpanProcessor:
    """SpanProcessor that stamps alibaba.base.emp_id on every span.

    Reads from the module-level _trace_user_id ContextVar which is set
    at the start of each request.  This covers all spans created inside
    the request, including agentscope LLM / agent / tool spans.
    """

    def on_start(
        self,
        span,
        parent_context=None,
    ):  # pylint: disable=unused-argument
        """Inject emp_id when a span starts.

        Args:
            span: The span being started.
            parent_context: Parent OTel context (unused here).
        """
        if not span.is_recording():
            return
        uid = _trace_user_id.get()
        if uid:
            span.set_attribute(_EMP_ID_ATTR, uid)

    def on_end(self, span):
        """Remember the last chat/LLM span rpc_id for feedback nesting."""
        remember_chat_span_link(span, session_id=_trace_session_id.get())

    def _on_ending(self, span):
        """No-op (required by OTel SDK >= 0.62)."""

    def shutdown(self):
        """No-op."""

    def force_flush(
        self,
        timeout_millis=30000,
    ):  # pylint: disable=unused-argument
        """No-op flush; always succeeds.

        Args:
            timeout_millis: Ignored.

        Returns:
            True
        """
        return True


def _migrate_saved_provider_config() -> None:
    """Upgrade persisted provider config when defaults change."""
    try:
        from qwenpaw.constant import SECRET_DIR

        config_path = (
            SECRET_DIR / "providers" / "plugin" / f"{_PROVIDER_ID}.json"
        )
        if not config_path.is_file():
            return

        data = json.loads(config_path.read_text(encoding="utf-8"))
        changed = False

        current_url = str(data.get("base_url") or "").strip().rstrip("/")
        target_url = _BASE_URL.rstrip("/")
        if current_url in _LEGACY_BASE_URLS or (
            current_url
            and current_url != target_url
            and "121.43.136.192:8081" in current_url
        ):
            data["base_url"] = _BASE_URL
            changed = True

        models = data.get("models")
        if isinstance(models, list):
            new_models = []
            for item in models:
                if not isinstance(item, dict):
                    new_models.append(item)
                    continue
                model_id = str(item.get("id") or "")
                if model_id == "qwen3.6-plus-dogfooding":
                    item = {
                        **item,
                        "id": "qwen3.7-max-dogfooding",
                        "name": "Qwen3.7-Max-DogFooding",
                    }
                    changed = True
                new_models.append(item)
            data["models"] = new_models

        if not changed:
            return

        config_path.write_text(
            json.dumps(data, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
        logger.info(
            "Migrated dogfooding provider config: base_url=%s",
            _BASE_URL,
        )
    except Exception as exc:
        logger.warning("Dogfooding provider config migration skipped: %s", exc)


def _stamp_current_span(
    session_id: str,
    user_id: str,
    trace_id: str = "",
) -> None:
    """Stamp conversation / user / model attrs on the active span."""
    try:
        from opentelemetry import trace as _otel_trace

        span = _otel_trace.get_current_span()
        if not span.is_recording():
            return
        if session_id:
            span.set_attribute(_CONV_ID_ATTR, session_id)
        if user_id:
            span.set_attribute(_EMP_ID_ATTR, user_id)
        if trace_id:
            span.set_attribute("dogfooding.trace_id", trace_id)
        span.set_attribute("dogfooding.model_id", MODEL_ID)
    except Exception as exc:
        logger.debug("Could not stamp current span: %s", exc)


def _register_span_processor() -> None:
    """Add _BundleSpanProcessor to the live TracerProvider.

    Must be called after AgentTrack.init() so the SDK TracerProvider
    has been installed as the global provider.
    """
    from opentelemetry import trace as _otel_trace
    from opentelemetry.sdk.trace import (
        TracerProvider as _SDKTracerProvider,
    )

    provider = _otel_trace.get_tracer_provider()
    if isinstance(provider, _SDKTracerProvider):
        provider.add_span_processor(_BundleSpanProcessor())
        logger.info("Bundle SpanProcessor registered")
    else:
        logger.warning(
            f"TracerProvider is {type(provider).__name__!r}; "
            "cannot register SpanProcessor — "
            "emp_id will not be stamped on spans",
        )


plugin = DogfoodingBundlePlugin()
