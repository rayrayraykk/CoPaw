# -*- coding: utf-8 -*-
"""Workspace backed by an external ACP agent process.

Instead of running a local QwenPawAgent through Runtime,
this workspace spawns an external agent via ACP and bridges
its event stream into the standard Envelope SSE format so
upstream consumers (ChannelManager, DynamicMultiAgentRunner,
ACP Server) see no difference.
"""
from __future__ import annotations

import asyncio
import logging
import os
import uuid
from contextlib import AsyncExitStack
from pathlib import Path
from types import SimpleNamespace
from typing import Any, AsyncGenerator

import psutil

from acp import PROTOCOL_VERSION, spawn_agent_process, text_block
from acp.schema import ClientCapabilities, Implementation

from ...config.config import (
    ACPAgentConfig,
    load_agent_config,
)
from ...schemas import AgentRequest
from ..task_tracker import TaskTracker
from .service_manager import ServiceManager
from .workspace_plugins import WorkspacePlugins

logger = logging.getLogger(__name__)


def _kill_process_tree(pid: int) -> None:
    """Kill a process and all descendants."""
    try:
        parent = psutil.Process(pid)
    except psutil.NoSuchProcess:
        return
    for child in parent.children(recursive=True):
        try:
            child.kill()
        except psutil.NoSuchProcess:
            pass
    try:
        parent.kill()
    except psutil.NoSuchProcess:
        pass


def _synthetic_event(
    event_type: str,
    **kwargs: Any,
) -> SimpleNamespace:
    """Build a synthetic agentscope-style event.

    Envelope.translate_event() checks ``event.type`` against
    ``EventType.<NAME>.value`` (str comparison), then reads
    named attributes (block_id, delta, tool_call_id, etc.).

    A SimpleNamespace lets us provide exactly those attributes
    without importing or instantiating real agentscope events.
    """
    return SimpleNamespace(type=event_type, **kwargs)


class ExternalAgentWorkspace:
    """Workspace that delegates to an external ACP agent.

    Implements the same public interface as ``Workspace``
    (duck typing) so it can be used interchangeably by
    WorkspaceRegistry, MultiAgentManager, and channels.

    Lifecycle:
      start()  -> spawn ACP subprocess, initialize,
                  new_session
      stream_query() -> translate AgentRequest to ACP prompt,
                        yield Envelope-compatible SSE events
      stop()   -> terminate subprocess
    """

    def __init__(
        self,
        agent_id: str,
        workspace_dir: str,
    ) -> None:
        self.agent_id = agent_id
        self.workspace_dir = Path(workspace_dir).expanduser()
        self.workspace_dir.mkdir(parents=True, exist_ok=True)

        self.plugins = WorkspacePlugins()
        self._service_manager = ServiceManager(self)
        self._config = None
        self._started = False
        self._manager = None
        self._app_services: Any = None
        self._task_tracker = TaskTracker()

        # ACP state
        self._client: Any = None
        self._conn: Any = None
        self._process: Any = None
        self._exit_stack: AsyncExitStack | None = None
        self._session_id: str | None = None
        self._acp_config: ACPAgentConfig | None = None
        self._turn_lock = asyncio.Lock()
        self._needs_auth = False
        self._start_error: str | None = None

    # ── Public interface (duck-type with Workspace) ──

    @property
    def config(self):
        """Get agent configuration."""
        self._config = load_agent_config(self.agent_id)
        return self._config

    @property
    def task_tracker(self) -> TaskTracker:
        """Get task tracker for background tasks."""
        return self._task_tracker

    @property
    def session(self):
        """Session service (not used by external agent)."""
        return self._service_manager.services.get(
            "session",
        )

    @property
    def chat_manager(self):
        """Chat manager service."""
        return self._service_manager.services.get(
            "chat_manager",
        )

    @property
    def channel_manager(self):
        """Channel manager service."""
        return self._service_manager.services.get(
            "channel_manager",
        )

    @property
    def memory_manager(self):
        """Not applicable for external agents."""
        return None

    @property
    def driver_manager(self):
        """Not applicable for external agents."""
        return None

    @property
    def cron_manager(self):
        """Not applicable for external agents."""
        return None

    @property
    def local_workspace(self):
        """Not applicable for external agents."""
        return None

    def set_manager(self, manager: Any) -> None:
        """Set reference to MultiAgentManager."""
        self._manager = manager

    def set_app_services(
        self,
        app_services: Any,
    ) -> None:
        """Inject cross-workspace AppServiceManager."""
        self._app_services = app_services

    def bootstrap_plugins(
        self,
        **kwargs: Any,
    ) -> None:
        """No-op for external agent workspace."""

    async def set_reusable_components(
        self,
        components: dict,
    ) -> None:
        """No-op: external agents need no reusable services."""

    # ── Lifecycle ──

    async def start(self) -> None:
        """Spawn external ACP agent and initialize."""
        if self._started:
            logger.debug(
                "ExternalAgentWorkspace started: %s",
                self.agent_id,
            )
            return

        logger.info(
            "Starting ExternalAgentWorkspace: %s",
            self.agent_id,
        )

        self._config = load_agent_config(self.agent_id)
        self._acp_config = self._resolve_acp_config()

        try:
            await self._spawn_acp_process()
        except FileNotFoundError as exc:
            self._start_error = (
                f"Command not found: {self._acp_config.command}"
            )
            logger.error(
                "ACP command not found for %s: %s",
                self.agent_id,
                exc,
            )
        self._started = True
        logger.info(
            "ExternalAgentWorkspace started: %s (cmd=%s)",
            self.agent_id,
            self._acp_config.command,
        )

    async def stop(self, final: bool = True) -> None:
        """Terminate ACP subprocess and clean up."""
        _ = final
        if not self._started:
            return
        self._started = False
        await self._close_acp_process()
        logger.info(
            "ExternalAgentWorkspace stopped: %s",
            self.agent_id,
        )

    # ── Core: stream_query ──

    async def stream_query(
        self,
        request: Any,
    ) -> AsyncGenerator[Any, None]:
        """Bridge AgentRequest to ACP, yield Envelope events.

        Makes ExternalAgentWorkspace behave identically to a
        native Workspace from the caller's perspective.
        """
        from ...runtime.envelope import Envelope

        if self._start_error:
            envelope = Envelope(session_id="")
            async for obj in envelope.error_envelope(
                self._start_error,
            ):
                yield obj
            return

        if self._needs_auth:
            envelope = Envelope(session_id="")
            async for obj in envelope.error_envelope(
                f"Agent '{self.agent_id}' requires "
                f"authentication. Please set the required "
                f"API key environment variable and restart.",
            ):
                yield obj
            return

        if not self._started or self._conn is None:
            await self._spawn_acp_process()
            self._started = True

        req = self._normalize_request(request)
        prompt_text = self._extract_prompt_text(req)

        async with self._turn_lock:
            async for event in self._run_acp_turn(
                prompt_text,
                session_id=req.session_id or "",
            ):
                yield event

    # ── ACP Communication ──

    async def _spawn_acp_process(self) -> None:
        """Spawn ACP subprocess, authenticate, new_session."""
        from ...agents.acp.client import ACPHostedClient

        cfg = self._acp_config
        if cfg is None:
            raise ValueError(
                f"No ACP config for agent {self.agent_id}",
            )

        self._exit_stack = AsyncExitStack()

        self._client = ACPHostedClient(
            agent_name=self.agent_id,
            agent_config=cfg,
            cwd=str(self.workspace_dir),
        )

        env = {**os.environ, **(cfg.env or {})}

        conn, process = await self._exit_stack.enter_async_context(
            spawn_agent_process(
                self._client,
                cfg.command,
                *cfg.args,
                cwd=str(self.workspace_dir),
                env=env,
                transport_kwargs={
                    "limit": (cfg.stdio_buffer_limit_bytes),
                },
            ),
        )
        self._conn = conn
        self._process = process

        initialized = await self._conn.initialize(
            protocol_version=PROTOCOL_VERSION,
            capabilities=ClientCapabilities(),
            client_info=Implementation(
                name="qwenpaw-external-workspace",
                version="0.1.0",
            ),
        )
        if initialized.protocol_version != PROTOCOL_VERSION:
            logger.warning(
                "ACP protocol mismatch: %s != %s",
                initialized.protocol_version,
                PROTOCOL_VERSION,
            )

        await self._auto_authenticate(initialized)

        try:
            resp = await self._conn.new_session(
                cwd=str(self.workspace_dir),
            )
            self._session_id = resp.session_id
        except Exception as exc:
            err_str = str(exc).lower()
            if "auth" in err_str:
                self._needs_auth = True
                logger.warning(
                    "ACP agent %s needs authentication: %s",
                    self.agent_id,
                    exc,
                )
            else:
                raise

    async def _auto_authenticate(
        self,
        initialized: Any,
    ) -> None:
        """Attempt automatic ACP authentication.

        Scans auth methods returned by ``initialize()`` and
        tries the first env_var method whose required variables
        are already present in the process environment.
        """
        methods = getattr(initialized, "auth_methods", None)
        if not methods:
            return

        for method in methods:
            method_type = getattr(method, "type", None)
            if method_type != "env_var":
                continue
            env_vars = getattr(method, "vars", [])
            all_set = all(
                os.environ.get(v.name)
                for v in env_vars
                if not getattr(v, "optional", False)
            )
            if not all_set:
                continue
            try:
                await self._conn.authenticate(
                    method_id=method.id,
                )
                logger.info(
                    "ACP auth succeeded for %s via %s",
                    self.agent_id,
                    method.id,
                )
                return
            except Exception as exc:
                logger.debug(
                    "ACP auth %s failed for %s: %s",
                    method.id,
                    self.agent_id,
                    exc,
                )

        method_names = [
            f"{m.name} ({getattr(m, 'type', 'agent')})" for m in methods
        ]
        logger.warning(
            "No auto-auth method succeeded for %s. " + "Available: %s",
            self.agent_id,
            ", ".join(method_names),
        )

    async def _close_acp_process(self) -> None:
        """Terminate ACP subprocess."""
        if self._conn is not None and self._session_id:
            try:
                await asyncio.wait_for(
                    self._conn.close_session(
                        session_id=self._session_id,
                    ),
                    timeout=5.0,
                )
            except Exception:
                pass

        if self._process is not None:
            _kill_process_tree(self._process.pid)

        if self._exit_stack is not None:
            await self._exit_stack.aclose()

        self._conn = None
        self._process = None
        self._exit_stack = None
        self._session_id = None

    async def _run_acp_turn(  # noqa: C901
        self,
        prompt_text: str,
        session_id: str = "",
    ) -> AsyncGenerator[Any, None]:
        """Send prompt to ACP, translate to Envelope events.

        Uses Envelope.translate_event() with synthetic
        agentscope events to produce correctly formatted
        SSE output without accessing Envelope internals.
        """
        from ...runtime.envelope import Envelope

        envelope = Envelope(session_id=session_id)

        async for obj in envelope.emit_response_created():
            yield obj

        collected: list[dict[str, Any]] = []

        async def _on_message(
            payload: dict[str, Any],
            _is_last: bool,
        ) -> None:
            collected.append(payload)

        self._client.start_prompt(_on_message)

        try:
            await self._conn.prompt(
                session_id=self._session_id,
                prompt=[text_block(prompt_text)],
            )
        except Exception as exc:
            logger.error(
                "ACP prompt failed for %s: %s",
                self.agent_id,
                exc,
            )
            async for obj in envelope.error_envelope(
                str(exc),
            ):
                yield obj
            return

        await self._client.finish_prompt()

        for payload in collected:
            events = self._to_synthetic_events(payload)
            for evt in events:
                async for obj in envelope.translate_event(
                    evt,
                ):
                    yield obj

        async for obj in envelope.finalize():
            yield obj

    def _to_synthetic_events(
        self,
        payload: dict[str, Any],
    ) -> list[SimpleNamespace]:
        """Convert ACPHostedClient event to synthetic events.

        Envelope.translate_event() understands agentscope
        EventType values; we build SimpleNamespace objects
        that carry the same attributes.
        """
        evt_type = payload.get("type", "")

        if evt_type == "text":
            text = payload.get("text", "")
            if not text:
                return []
            bid = f"acp_{uuid.uuid4().hex[:8]}"
            return [
                _synthetic_event(
                    "TEXT_BLOCK_START",
                    block_id=bid,
                ),
                _synthetic_event(
                    "TEXT_BLOCK_DELTA",
                    block_id=bid,
                    delta=text,
                ),
                _synthetic_event(
                    "TEXT_BLOCK_END",
                    block_id=bid,
                ),
            ]

        if evt_type in (
            "tool_start",
            "tool_update",
            "tool_end",
        ):
            name = payload.get("name", "tool")
            detail = payload.get("detail", "")
            display = detail or name
            bid = f"acp_tool_{uuid.uuid4().hex[:8]}"
            return [
                _synthetic_event(
                    "TEXT_BLOCK_START",
                    block_id=bid,
                ),
                _synthetic_event(
                    "TEXT_BLOCK_DELTA",
                    block_id=bid,
                    delta=f"[{name}] {display}",
                ),
                _synthetic_event(
                    "TEXT_BLOCK_END",
                    block_id=bid,
                ),
            ]

        if evt_type == "status":
            logger.debug(
                "ACP status for %s: %s - %s",
                self.agent_id,
                payload.get("status", ""),
                payload.get("summary", ""),
            )

        return []

    # ── Helpers ──

    def _resolve_acp_config(self) -> ACPAgentConfig:
        """Load ACP runner config from agent.json or root."""
        if self._config and self._config.acp:
            agents = self._config.acp.agents
            if self.agent_id in agents:
                cfg = agents[self.agent_id]
                if cfg.enabled:
                    return cfg

        from ...config.utils import load_config

        root = load_config()
        if root.acp and root.acp.agents:
            if self.agent_id in root.acp.agents:
                cfg = root.acp.agents[self.agent_id]
                if cfg.enabled:
                    return cfg

        raise ValueError(
            f"Agent '{self.agent_id}' has no enabled "
            f"ACP runner configuration",
        )

    @staticmethod
    def _normalize_request(
        request: Any,
    ) -> AgentRequest:
        """Normalize input to AgentRequest."""
        if isinstance(request, AgentRequest):
            return request
        if isinstance(request, dict):
            return AgentRequest(**request)
        return AgentRequest(input=request)

    @staticmethod
    def _extract_prompt_text(
        req: AgentRequest,
    ) -> str:
        """Extract text from AgentRequest messages."""
        parts: list[str] = []
        for msg in req.input or []:
            if isinstance(msg, dict):
                content = msg.get("content", [])
            else:
                content = getattr(msg, "content", [])
            blocks = content if isinstance(content, list) else [content]
            for block in blocks:
                if isinstance(block, str):
                    parts.append(block)
                elif isinstance(block, dict):
                    parts.append(
                        block.get("text", ""),
                    )
                elif hasattr(block, "text"):
                    parts.append(block.text or "")
        return "\n".join(parts)

    def __repr__(self) -> str:
        status = "started" if self._started else "stopped"
        return (
            f"ExternalAgentWorkspace("
            f"id={self.agent_id}, "
            f"status={status})"
        )
