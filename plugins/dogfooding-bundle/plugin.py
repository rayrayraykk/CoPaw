# -*- coding: utf-8 -*-
"""Dogfooding Bundle Plugin.

Internal org bundle that registers three capabilities in one shot:

1. AgentScope Dogfooding provider
   - OpenAI-compatible LLM via https://proxy.agentscope.design/v1
2. AgentTrack startup hook
   - Initialises AgentTrack SDK (app_name="qwenpaw") on startup
   - Registers a SpanProcessor that stamps emp_id on every span,
     and overrides agentscope run_id for correct conversation.id
3. /feedback command
   - Query-rewrite hook that turns /feedback into an agent prompt
4. Dogfooding account API
   - Saves dogfooding user_account.json under the QwenPaw working directory
"""

import json
import logging
from contextvars import ContextVar
from pathlib import Path

from fastapi import APIRouter, HTTPException
from pydantic import BaseModel, Field

from qwenpaw.constant import WORKING_DIR
from qwenpaw.plugins.api import PluginApi
from qwenpaw.providers.openai_provider import OpenAIProvider
from qwenpaw.providers.provider import ModelInfo

logger = logging.getLogger(__name__)

# Per-request user identity for EvaPlus "用户ID" field
_trace_user_id: ContextVar[str] = ContextVar(
    "bundle_trace_user_id",
    default="",
)

# EvaPlus span attribute names
_EMP_ID_ATTR = "alibaba.base.emp_id"
_CONV_ID_ATTR = "gen_ai.conversation.id"

# ── AgentScope provider constants ────────────────────────────────────────
_BASE_URL = "https://proxy.agentscope.design/v1"

_DEFAULT_MODELS = [
    ModelInfo(
        id="qwen3.6-plus-dogfooding",
        name="Qwen 3.6 Plus",
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


class DogfoodingAccountResponse(BaseModel):
    """Response body after saving the dogfooding user account."""

    ok: bool
    path: str


def _dogfooding_dir() -> Path:
    return WORKING_DIR / "dogfooding"


def _user_account_path() -> Path:
    return _dogfooding_dir() / "user_account.json"


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

        return DogfoodingAccountResponse(ok=True, path=str(target))

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
        self._register_account_router(api)
        logger.info("Dogfooding Bundle fully registered")

    # ── provider ──────────────────────────────────────────────────────────

    def _register_provider(self, api: PluginApi):
        """Register the AgentScope Dogfooding LLM provider.

        Args:
            api: PluginApi instance.
        """
        api.register_provider(
            provider_id="agentscope-dogfooding",
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
        api.register_startup_hook(
            hook_name="feedback_query_rewriter",
            callback=self._patch_query_handler,
            priority=50,
        )
        logger.info("/feedback query-rewrite hook registered")

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
            "POST /api/dogfooding-account/",
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
            user_id = getattr(request, "user_id", "") or ""

            # Store user_id in ContextVar for the SpanProcessor.
            _trace_user_id.set(user_id)

            logger.info(
                f"Trace context: " f"session={session_id!r} user={user_id!r}",
            )

            # Stamp the root span (created before this handler ran) with
            # gen_ai.conversation.id and alibaba.base.emp_id.
            _stamp_current_span(session_id, user_id)

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
        logger.info(
            "Patched AgentRunner.query_handler for /feedback command",
        )


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
        """No-op."""

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


def _stamp_current_span(session_id: str, user_id: str) -> None:
    """Stamp gen_ai.conversation.id and alibaba.base.emp_id on the active span.

    The root HTTP / FastAPI span is created before patched_query_handler
    runs, so the SpanProcessor misses it.  This function patches that
    span directly after the request context is known.

    Args:
        session_id: Session identifier from the current request.
        user_id: User emp_id from the current request.
    """
    try:
        from opentelemetry import trace as _otel_trace

        span = _otel_trace.get_current_span()
        if not span.is_recording():
            return
        if session_id:
            span.set_attribute(_CONV_ID_ATTR, session_id)
        if user_id:
            span.set_attribute(_EMP_ID_ATTR, user_id)
    except Exception as exc:
        logger.debug(f"Could not stamp current span: {exc}")


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
