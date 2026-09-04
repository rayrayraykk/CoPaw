from __future__ import annotations

import json
import os
import queue
import shutil
import subprocess
import threading
from collections.abc import Callable, Iterator, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Any, TextIO

from .errors import (
    ProtocolVersionError,
    RequestTimeoutError,
    RpcRequestError,
    TransportClosedError,
)
from .models import JsonObject, Notification, TurnResult
from .protocol import PROTOCOL_VERSION

NotificationHandler = Callable[[Notification], None]
CloseHandler = Callable[[Exception], None]
ApprovalHandler = Callable[[Notification], str]


@dataclass(frozen=True, slots=True)
class QwenPawConfig:
    """Configuration for launching the local App Server runtime."""

    core_bin: str | Path | None = None
    launch_args_override: tuple[str, ...] | None = None
    cwd: str | Path | None = None
    env: dict[str, str] | None = None
    client_name: str = f"qwenpaw_python_sdk"
    client_title: str = f"QwenPaw Python SDK"
    client_version: str = f"0.2.0"
    request_timeout: float = 15.0
    turn_timeout: float = 3600.0


@dataclass(slots=True)
class _PendingRequest:
    event: threading.Event
    result: Any = None
    error: Exception | None = None


class AppServerClient:
    """Synchronous JSON-RPC client for App Server over stdio."""

    def __init__(self, config: QwenPawConfig | None = None) -> None:
        self.config = config or QwenPawConfig()
        self._process: subprocess.Popen[str] | None = None
        self._reader: threading.Thread | None = None
        self._next_id = 1
        self._pending: dict[int, _PendingRequest] = {}
        self._handlers: set[NotificationHandler] = set()
        self._close_handlers: set[CloseHandler] = set()
        self._state_lock = threading.Lock()
        self._write_lock = threading.Lock()
        self._closed_error: Exception | None = None

    def start(self) -> JsonObject:
        """Start and initialize the configured App Server."""

        if self._process is not None:
            raise RuntimeError(f"QwenPaw App Server is already started")
        args = self._launch_args()
        env = os.environ.copy()
        if self.config.env is not None:
            env.update(self.config.env)
        self._process = subprocess.Popen(
            args,
            cwd=self.config.cwd,
            env=env,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=None,
            text=True,
            bufsize=1,
        )
        self._reader = threading.Thread(
            target=self._read_loop,
            name=f"qwenpaw-app-server-reader",
            daemon=True,
        )
        self._reader.start()
        try:
            response = self.request(
                f"initialize",
                {
                    f"clientInfo": {
                        f"name": self.config.client_name,
                        f"title": self.config.client_title,
                        f"version": self.config.client_version,
                    }
                },
            )
        except Exception:
            self.close()
            raise
        actual = response.get(f"protocolVersion")
        if actual != PROTOCOL_VERSION:
            self.close()
            raise ProtocolVersionError(
                f"QwenPaw protocol version {actual} does not match "
                f"SDK version {PROTOCOL_VERSION}"
            )
        self.notify(f"initialized", {})
        return response

    def request(
        self,
        method: str,
        params: JsonObject,
        timeout: float | None = None,
    ) -> JsonObject:
        """Send one App Protocol request and await its response."""

        process = self._require_process()
        stdin = process.stdin
        if stdin is None:
            raise TransportClosedError(f"App Server stdin is unavailable")
        with self._state_lock:
            if self._closed_error is not None:
                raise self._closed_error
            request_id = self._next_id
            self._next_id += 1
            pending = _PendingRequest(threading.Event())
            self._pending[request_id] = pending
        message = {
            f"id": request_id,
            f"method": method,
            f"params": params,
        }
        try:
            self._write(stdin, message)
        except Exception:
            with self._state_lock:
                self._pending.pop(request_id, None)
            raise
        wait_timeout = (
            self.config.request_timeout if timeout is None else timeout
        )
        if not pending.event.wait(wait_timeout):
            with self._state_lock:
                self._pending.pop(request_id, None)
            raise RequestTimeoutError(
                f"QwenPaw Core request timed out: {method}"
            )
        if pending.error is not None:
            raise pending.error
        if not isinstance(pending.result, dict):
            raise TransportClosedError(
                f"QwenPaw Core returned a non-object result"
            )
        return pending.result

    def notify(self, method: str, params: JsonObject) -> None:
        """Send one client notification."""

        process = self._require_process()
        stdin = process.stdin
        if stdin is None:
            raise TransportClosedError(f"App Server stdin is unavailable")
        self._write(
            stdin,
            {f"method": method, f"params": params},
        )

    def on_notification(
        self,
        handler: NotificationHandler,
    ) -> Callable[[], None]:
        """Subscribe to notifications and return an unsubscribe callback."""

        with self._state_lock:
            self._handlers.add(handler)

        def unsubscribe() -> None:
            with self._state_lock:
                self._handlers.discard(handler)

        return unsubscribe

    def on_close(self, handler: CloseHandler) -> Callable[[], None]:
        """Subscribe to closure and return an unsubscribe callback."""

        with self._state_lock:
            error = self._closed_error
            if error is None:
                self._close_handlers.add(handler)
        if error is not None:
            handler(error)
            return lambda: None

        def unsubscribe() -> None:
            with self._state_lock:
                self._close_handlers.discard(handler)

        return unsubscribe

    def close(self) -> None:
        """Close the transport and terminate the owned child process."""

        process = self._process
        if process is None:
            return
        self._close_with_error(
            TransportClosedError(f"QwenPaw App Server was closed")
        )
        if process.stdin is not None:
            process.stdin.close()
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
        reader = self._reader
        if reader is not None and reader is not threading.current_thread():
            reader.join(timeout=5)
        if process.stdout is not None:
            process.stdout.close()
        self._reader = None
        self._process = None

    def _launch_args(self) -> list[str]:
        override = self.config.launch_args_override
        if override is not None:
            if not override:
                raise ValueError(f"launch_args_override cannot be empty")
            return list(override)
        configured = self.config.core_bin
        core_bin = str(configured) if configured is not None else None
        resolved = core_bin or shutil.which(f"qwenpaw-core")
        if resolved is None:
            raise FileNotFoundError(
                f"qwenpaw-core was not found; set QwenPawConfig.core_bin"
            )
        return [resolved, f"app-server", f"--stdio"]

    def _require_process(self) -> subprocess.Popen[str]:
        process = self._process
        if process is None:
            raise TransportClosedError(f"QwenPaw App Server is not started")
        return process

    def _write(self, stream: TextIO, message: JsonObject) -> None:
        encoded = json.dumps(message, separators=(f",", f":"))
        with self._write_lock:
            stream.write(f"{encoded}\n")
            stream.flush()

    def _read_loop(self) -> None:
        process = self._process
        if process is None or process.stdout is None:
            return
        try:
            for line in process.stdout:
                self._handle_line(line)
        except Exception as error:
            self._close_with_error(error)
        finally:
            self._close_with_error(
                TransportClosedError(f"QwenPaw App Server closed stdout")
            )

    def _handle_line(self, line: str) -> None:
        try:
            message = json.loads(line)
        except json.JSONDecodeError as error:
            self._close_with_error(error)
            return
        request_id = message.get(f"id")
        if isinstance(request_id, int):
            with self._state_lock:
                pending = self._pending.pop(request_id, None)
            if pending is None:
                return
            rpc_error = message.get(f"error")
            if isinstance(rpc_error, dict):
                pending.error = RpcRequestError(
                    int(rpc_error.get(f"code", -32000)),
                    str(rpc_error.get(f"message", f"unknown error")),
                )
            else:
                pending.result = message.get(f"result")
            pending.event.set()
            return
        method = message.get(f"method")
        params = message.get(f"params")
        if not isinstance(method, str) or not isinstance(params, dict):
            return
        notification = Notification(method=method, params=params)
        with self._state_lock:
            handlers = tuple(self._handlers)
        for handler in handlers:
            try:
                handler(notification)
            except Exception:
                continue

    def _close_with_error(self, error: Exception) -> None:
        with self._state_lock:
            if self._closed_error is not None:
                return
            self._closed_error = error
            pending = tuple(self._pending.values())
            self._pending.clear()
            close_handlers = tuple(self._close_handlers)
            self._close_handlers.clear()
            self._handlers.clear()
        for request in pending:
            request.error = error
            request.event.set()
        for handler in close_handlers:
            try:
                handler(error)
            except Exception:
                continue


class Thread:
    """A language-friendly handle to one persistent Core thread."""

    def __init__(
        self,
        client: AppServerClient,
        thread: JsonObject,
        approval_handler: ApprovalHandler | None,
    ) -> None:
        self._client = client
        self._approval_handler = approval_handler
        self.thread = thread

    @property
    def id(self) -> str:
        """Return the persistent thread identifier."""

        return str(self.thread[f"id"])

    def run_streamed(
        self,
        prompt: str | Sequence[JsonObject],
    ) -> Iterator[Notification]:
        """Start one turn and yield its notifications in wire order."""

        events: queue.Queue[Notification | Exception] = queue.Queue()

        def receive(notification: Notification) -> None:
            params = notification.params
            thread_id = params.get(f"threadId")
            turn = params.get(f"turn")
            if isinstance(turn, dict):
                thread_id = turn.get(f"threadId")
            if thread_id == self.id:
                events.put(notification)

        unsubscribe = self._client.on_notification(receive)
        unsubscribe_close = self._client.on_close(events.put)
        input_items = (
            [{f"type": f"text", f"text": prompt}]
            if isinstance(prompt, str)
            else list(prompt)
        )
        try:
            response = self._client.request(
                f"turn/start",
                {f"threadId": self.id, f"input": input_items},
            )
            turn = response[f"turn"]
            turn_id = str(turn[f"id"])
            while True:
                try:
                    notification = events.get(
                        timeout=self._client.config.turn_timeout
                    )
                except queue.Empty as error:
                    raise RequestTimeoutError(
                        f"QwenPaw turn timed out: {turn_id}"
                    ) from error
                if isinstance(notification, Exception):
                    raise notification
                if notification.method == f"tool/approval/requested":
                    self._handle_approval(notification)
                yield notification
                if notification.method != f"turn/completed":
                    continue
                completed = notification.params.get(f"turn")
                if isinstance(completed, dict):
                    if str(completed.get(f"id")) == turn_id:
                        break
        finally:
            unsubscribe()
            unsubscribe_close()

    def run(
        self,
        prompt: str | Sequence[JsonObject],
    ) -> TurnResult:
        """Run one turn and collect its final response and items."""

        deltas: list[str] = []
        completed: JsonObject | None = None
        for notification in self.run_streamed(prompt):
            if notification.method == f"item/agentMessage/delta":
                deltas.append(str(notification.params.get(f"delta", f"")))
            elif notification.method == f"turn/completed":
                value = notification.params.get(f"turn")
                if isinstance(value, dict):
                    completed = value
        if completed is None:
            raise TransportClosedError(
                f"QwenPaw turn ended without a completion"
            )
        status = completed.get(f"status")
        if status != f"completed":
            error = completed.get(f"error")
            raise RpcRequestError(
                -32000,
                f"turn ended with status {status}: {error}",
            )
        raw_items = completed.get(f"items", [])
        items = tuple(item for item in raw_items if isinstance(item, dict))
        return TurnResult(
            final_response=f"".join(deltas),
            turn=completed,
            items=items,
        )

    def interrupt(self, turn_id: str) -> bool:
        """Interrupt one active turn."""

        response = self._client.request(
            f"turn/interrupt",
            {f"threadId": self.id, f"turnId": turn_id},
        )
        return bool(response.get(f"accepted"))

    def _handle_approval(self, notification: Notification) -> None:
        decision = (
            f"denied"
            if self._approval_handler is None
            else self._approval_handler(notification)
        )
        if decision not in {f"approved", f"denied"}:
            raise ValueError(f"approval handler returned {decision}")
        self._client.request(
            f"tool/approval/respond",
            {
                f"approvalId": notification.params[f"approvalId"],
                f"decision": decision,
            },
        )


class QwenPaw:
    """High-level SDK facade backed by one App Server process."""

    def __init__(
        self,
        config: QwenPawConfig | None = None,
        approval_handler: ApprovalHandler | None = None,
    ) -> None:
        self.config = config or QwenPawConfig()
        self._approval_handler = approval_handler
        self._client = AppServerClient(self.config)

    def __enter__(self) -> QwenPaw:
        self.start()
        return self

    def __exit__(self, _type: object, _value: object, _trace: object) -> None:
        self.close()

    def start(self) -> JsonObject:
        """Start and initialize the local App Server."""

        return self._client.start()

    def close(self) -> None:
        """Close the owned App Server process."""

        self._client.close()

    def thread_start(
        self,
        model: str | None = None,
        workspace_root: str | Path | None = None,
    ) -> Thread:
        """Create and return one persistent Core thread."""

        response = self._client.request(
            f"thread/start",
            {
                f"model": model,
                f"workspaceRoot": (
                    None if workspace_root is None else str(workspace_root)
                ),
            },
        )
        return Thread(
            self._client,
            response[f"thread"],
            self._approval_handler,
        )

    def thread_resume(self, thread_id: str) -> Thread:
        """Resume and return one persistent Core thread."""

        response = self._client.request(
            f"thread/resume",
            {f"threadId": thread_id},
        )
        return Thread(
            self._client,
            response[f"thread"],
            self._approval_handler,
        )
