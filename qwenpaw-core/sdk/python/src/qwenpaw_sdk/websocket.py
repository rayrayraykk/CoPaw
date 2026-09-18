"""Explicit connections share Core protocol logic, not process ownership."""

from __future__ import annotations

import ipaddress
import json
import math
import ssl
import threading
import time
from dataclasses import dataclass, field
from pathlib import Path
from urllib.parse import urlsplit

from websockets.exceptions import ConnectionClosedOK

from ._websocket_transport import (
    CLEANUP_TIMEOUT,
    DISCONNECT_TIMEOUT,
    MAX_MESSAGE_BYTES,
    WebSocketTransport,
)
from .client import (
    ApprovalHandler,
    AppServerClient,
    CloseHandler,
    QwenPawConfig,
    Thread,
    _PendingRequest,
)
from .errors import TransportClosedError
from .models import JsonObject


@dataclass(frozen=True, slots=True)
class WebSocketOptions:
    """Connection-only options; no model or subprocess configuration."""

    bearer_token: str | None = field(default=None, repr=False)
    ca_pem: str | None = field(default=None, repr=False)
    client_name: str = f"qwenpaw_python_sdk"
    client_title: str = f"QwenPaw Python SDK"
    client_version: str = f"0.2.0"
    request_timeout: float = 15.0
    turn_timeout: float = 3600.0


def _tls_context(
    endpoint: str,
    options: WebSocketOptions,
) -> ssl.SSLContext | None:
    invalid = f"Invalid Core endpoint; use WSS or literal loopback WS"
    try:
        url = urlsplit(endpoint)
        host = url.hostname
        port = url.port
        try:
            loopback = ipaddress.ip_address(host or f"").is_loopback
        except ValueError:
            loopback = False
        if (
            url.scheme not in {f"ws", f"wss"}
            or not host
            or port == 0
            or (url.scheme == f"ws" and not loopback)
            or url.username is not None
            or url.password is not None
            or f"?" in endpoint
            or f"#" in endpoint
            or any(ord(char) <= 32 or ord(char) == 127 for char in endpoint)
        ):
            raise ValueError(invalid)
    except ValueError:
        raise ValueError(invalid) from None
    token = options.bearer_token
    if token is not None and (
        not 32 <= len(token) <= 4096
        or any(not 33 <= ord(char) <= 126 for char in token)
    ):
        raise ValueError(f"Invalid Core bearer token")
    for timeout in (options.request_timeout, options.turn_timeout):
        if not math.isfinite(timeout) or timeout <= 0:
            raise ValueError(f"Core timeouts must be positive and finite")
    if url.scheme == f"ws":
        return None
    try:
        return ssl.create_default_context(cadata=options.ca_pem)
    except (ValueError, ssl.SSLError):
        raise ValueError(f"Invalid Core CA certificates") from None


class _WebSocketClient(AppServerClient):
    def __init__(
        self,
        transport: WebSocketTransport,
        options: WebSocketOptions,
        cancel: threading.Event | None,
    ) -> None:
        super().__init__(
            QwenPawConfig(
                client_name=options.client_name,
                client_title=options.client_title,
                client_version=options.client_version,
                request_timeout=options.request_timeout,
                turn_timeout=options.turn_timeout,
            )
        )
        self._transport = transport
        self._opening_cancel = cancel
        self._transport_error: Exception | None = None
        self._close_callback_thread: threading.Thread | None = None

    def start(self) -> JsonObject:
        raise RuntimeError(f"Use WebSocketConnection.connect for this client")

    def initialize(self) -> JsonObject:
        self._reader = threading.Thread(
            target=self._read_websocket,
            name=f"qwenpaw-websocket-reader",
            daemon=True,
        )
        self._reader.start()
        try:
            return self._initialize()
        finally:
            self._opening_cancel = None

    def _wait_response(
        self,
        pending: _PendingRequest,
        timeout: float,
    ) -> bool:
        if self._opening_cancel is None:
            return super()._wait_response(pending, timeout)
        deadline = time.monotonic() + timeout
        while True:
            cancel = self._opening_cancel
            if cancel is not None and cancel.is_set():
                raise TransportClosedError(f"Core connection was aborted")
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                return False
            if pending.event.wait(min(remaining, 0.05)):
                return True

    def _send_message(self, message: JsonObject) -> None:
        with self._state_lock:
            if self._closed_error is not None:
                raise self._closed_error
        encoded = json.dumps(message, separators=(f",", f":"))
        try:
            if len(encoded.encode(f"utf-8")) > MAX_MESSAGE_BYTES:
                raise TransportClosedError(f"Core outgoing message too large")
            self._transport.send(encoded, self.config.request_timeout)
        except Exception:
            with self._state_lock:
                closed_error = self._closed_error
            if closed_error is not None:
                # Cancellation by disconnect isn't a new transport failure.
                raise closed_error from None
            self._transport_error = TransportClosedError(
                f"Core WebSocket send failed or exceeded limits",
            )
            self._transport.abort()
            self._close_with_error(self._transport_error)
            raise self._transport_error from None

    def _read_websocket(self) -> None:
        try:
            while self._closed_error is None:
                message = self._transport.receive()
                if not isinstance(message, str):
                    raise ValueError(f"Expected a text message")
                if not isinstance(json.loads(message), dict):
                    raise ValueError(f"Expected a JSON object")
                if self._closed_error is None:
                    self._handle_line(message)
        except ConnectionClosedOK:
            pass
        except Exception:
            if self._closed_error is None:
                self._transport_error = TransportClosedError(
                    f"Core WebSocket receive failed or invalid message",
                )
        finally:
            if self._close_owner is None:
                try:
                    self._transport.abort()
                except Exception as failure:
                    self._transport_error = failure
            error = self._transport_error or TransportClosedError(
                f"Core WebSocket connection is closed",
            )
            self._close_with_error(error)

    def close(self) -> None:
        """Detach one connection, preserving shared host and accepted work."""

        current = threading.current_thread()
        with self._state_lock:
            first = self._close_owner is None
            if first:
                self._close_owner = current
        if not first:
            if not self._close_done.is_set():
                if current in {
                    self._close_owner,
                    self._reader,
                    self._close_callback_thread,
                }:
                    return
                if not self._close_done.wait(
                    DISCONNECT_TIMEOUT + CLEANUP_TIMEOUT,
                ):
                    raise TransportClosedError(
                        f"Concurrent WebSocket disconnect did not finish",
                    )
            if self._close_result is not None:
                raise self._close_result
            return
        deadline = time.monotonic() + DISCONNECT_TIMEOUT
        error = TransportClosedError(f"Core WebSocket connection was closed")
        handlers = self._close_with_error(error, defer_handlers=True)
        try:
            if self._transport_error is not None:
                self._transport.abort()
                raise self._transport_error
            if self._transport.thread.is_alive():
                self._transport.close()
            if self._transport_error is not None:
                raise self._transport_error
        except Exception as failure:
            self._close_result = failure
        finally:
            reader = self._reader
            if reader is not None and reader is not current:
                reader.join(max(0, deadline - time.monotonic()))
                if reader.is_alive():
                    self._close_result = TransportClosedError(
                        f"WebSocket reader cleanup could not be confirmed",
                    )
            try:
                self._finish_close_handlers(handlers, error, deadline)
            finally:
                self._close_done.set()
        if self._close_result is not None:
            raise self._close_result

    def dispose(self) -> None:
        """Abort local network I/O without waiting for a close handshake."""

        deadline = time.monotonic() + CLEANUP_TIMEOUT
        error = TransportClosedError(f"Core WebSocket connection was disposed")
        handlers = self._close_with_error(error, defer_handlers=True)
        try:
            self._transport.abort()
        finally:
            self._finish_close_handlers(handlers, error, deadline)

    def _finish_close_handlers(
        self,
        handlers: tuple[CloseHandler, ...],
        error: Exception,
        deadline: float,
    ) -> None:
        if handlers:
            self._close_callback_thread = threading.Thread(
                target=self._notify_close_handlers,
                args=(handlers, error),
                name=f"qwenpaw-websocket-close-callbacks",
                daemon=True,
            )
            self._close_callback_thread.start()
        worker = self._close_callback_thread
        if worker is not None and worker is not threading.current_thread():
            worker.join(max(0, deadline - time.monotonic()))
            if worker.is_alive():
                self._close_result = TransportClosedError(
                    f"WebSocket callback cleanup could not be confirmed",
                )
                raise self._close_result


class WebSocketConnection:
    """A synchronous connection owner, independent of the Core lifetime."""

    def __init__(self, client: _WebSocketClient) -> None:
        self.client = client

    @classmethod
    def connect(
        cls,
        endpoint: str,
        options: WebSocketOptions | None = None,
        *,
        cancel: threading.Event | None = None,
    ) -> WebSocketConnection:
        """Connect and initialize; cancel applies only until this returns."""

        options = options or WebSocketOptions()
        context = _tls_context(endpoint, options)
        if cancel is not None and cancel.is_set():
            raise TransportClosedError(f"Core connection was aborted")
        transport = WebSocketTransport()
        transport.open(endpoint, options.bearer_token, context, cancel)
        client = _WebSocketClient(transport, options, cancel)
        try:
            client.initialize()
        except BaseException:
            client.dispose()
            raise
        return cls(client)

    def thread_start(
        self,
        model: str | None = None,
        workspace_root: str | Path | None = None,
        approval_handler: ApprovalHandler | None = None,
    ) -> Thread:
        """Create a Thread using the existing language-friendly facade."""

        response = self.client.request(
            f"thread/start",
            {
                f"model": model,
                f"workspaceRoot": (
                    None if workspace_root is None else str(workspace_root)
                ),
            },
        )
        return Thread(self.client, response[f"thread"], approval_handler)

    def thread_resume(
        self,
        thread_id: str,
        approval_handler: ApprovalHandler | None = None,
    ) -> Thread:
        """Resume a Thread without changing the shared model configuration."""

        response = self.client.request(
            f"thread/resume",
            {f"threadId": thread_id},
        )
        return Thread(self.client, response[f"thread"], approval_handler)

    def disconnect(self) -> None:
        """Detach this client; do not stop Core or acknowledge saving."""

        self.client.close()

    def dispose(self) -> None:
        """Immediately abort local I/O, not the host process."""

        self.client.dispose()

    def __enter__(self) -> WebSocketConnection:
        return self

    def __exit__(self, kind: object, _value: object, _trace: object) -> None:
        if kind is None:
            self.disconnect()
        else:
            try:
                self.dispose()
            except Exception:
                pass
