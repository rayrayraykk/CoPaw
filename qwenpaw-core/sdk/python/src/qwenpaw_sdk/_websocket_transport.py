"""Bounded synchronous access to one asynchronous WebSocket connection."""

from __future__ import annotations

import asyncio
import concurrent.futures
import logging
import ssl
import threading
import time
from collections.abc import Coroutine
from typing import Any

from websockets.asyncio.client import ClientConnection, connect
from websockets.exceptions import InvalidStatus

from .errors import TransportClosedError

CONNECT_TIMEOUT = 15.0
DISCONNECT_TIMEOUT = 5.0
CLEANUP_TIMEOUT = 1.0
MAX_MESSAGE_BYTES = 1_048_576


class _NoRedirectConnect(connect):
    def process_redirect(self, exc: Exception) -> Exception:
        return exc


class WebSocketTransport:
    """Own only the network loop; never start or stop a Core process."""

    def __init__(self) -> None:
        self.loop = asyncio.new_event_loop()
        self._state_lock = threading.Lock()
        self._stopping = False
        self.connection: ClientConnection | None = None
        self._send_lock: asyncio.Lock | None = None
        self.thread = threading.Thread(
            target=self._run,
            name=f"qwenpaw-websocket-io",
            daemon=True,
        )
        self.thread.start()

    def _run(self) -> None:
        asyncio.set_event_loop(self.loop)
        try:
            self.loop.run_forever()
        finally:
            with self._state_lock:
                self._stopping = True
            if self.connection is not None:
                self.connection.transport.abort()
            tasks = asyncio.all_tasks(self.loop)
            for task in tasks:
                task.cancel()
            if tasks:
                self.loop.run_until_complete(
                    asyncio.gather(*tasks, return_exceptions=True),
                )
            self.loop.run_until_complete(self.loop.shutdown_asyncgens())
            self.loop.close()

    def open(
        self,
        endpoint: str,
        token: str | None,
        context: ssl.SSLContext | None,
        cancel: threading.Event | None,
    ) -> None:
        try:
            self._submit(
                self._open(endpoint, token, context),
                CONNECT_TIMEOUT,
                cancel,
            )
        except BaseException as error:
            self.abort()
            if isinstance(error, InvalidStatus):
                raise TransportClosedError(
                    f"Core WebSocket handshake returned HTTP "
                    f"{error.response.status_code}",
                ) from None
            if isinstance(error, TransportClosedError):
                raise error from None
            if isinstance(error, Exception):
                raise TransportClosedError(
                    f"Core WebSocket handshake failed",
                ) from None
            raise

    async def _open(
        self,
        endpoint: str,
        token: str | None,
        context: ssl.SSLContext | None,
    ) -> None:
        # A private disabled logger prevents global DEBUG from logging headers.
        logger = logging.Logger(f"qwenpaw-websocket-private")
        logger.disabled = True
        self.connection = await _NoRedirectConnect(
            endpoint,
            ssl=context,
            additional_headers=(
                {f"Authorization": f"Bearer {token}"} if token else None
            ),
            proxy=None,
            compression=None,
            ping_interval=None,
            open_timeout=CONNECT_TIMEOUT,
            close_timeout=DISCONNECT_TIMEOUT,
            max_size=MAX_MESSAGE_BYTES,
            max_queue=16,
            logger=logger,
        )
        self._send_lock = asyncio.Lock()

    def _submit(
        self,
        coroutine: Coroutine[Any, Any, Any],
        timeout: float | None,
        cancel: threading.Event | None = None,
    ) -> Any:
        try:
            with self._state_lock:
                if self._stopping:
                    raise RuntimeError(f"Core WebSocket is stopping")
                future = asyncio.run_coroutine_threadsafe(coroutine, self.loop)
        except RuntimeError:
            coroutine.close()
            raise TransportClosedError(f"Core WebSocket is closed") from None
        deadline = None if timeout is None else time.monotonic() + timeout
        try:
            while True:
                if cancel is not None and cancel.is_set():
                    raise TransportClosedError(f"Core connection was aborted")
                remaining = (
                    None if deadline is None else deadline - time.monotonic()
                )
                if remaining is not None and remaining <= 0:
                    raise TransportClosedError(f"Core WebSocket timed out")
                wait = remaining
                if cancel is not None:
                    wait = 0.05 if wait is None else min(wait, 0.05)
                try:
                    return future.result(timeout=wait)
                except concurrent.futures.CancelledError:
                    raise TransportClosedError(
                        f"Core WebSocket connection is closed",
                    ) from None
                except concurrent.futures.TimeoutError:
                    if future.done():
                        raise
        except BaseException:
            future.cancel()
            raise

    def send(self, message: str, timeout: float) -> None:
        self._submit(self._send(message), timeout)

    async def _send(self, message: str) -> None:
        assert self.connection is not None and self._send_lock is not None
        async with self._send_lock:
            buffered = self.connection.transport.get_write_buffer_size()
            size = len(message.encode(f"utf-8"))
            if buffered + size > MAX_MESSAGE_BYTES * 2:
                raise TransportClosedError(f"Core WebSocket buffer exceeded")
            await self.connection.send(message)

    def receive(self) -> str | bytes:
        assert self.connection is not None
        return self._submit(self.connection.recv(), None)

    def close(self) -> None:
        try:
            self._submit(self._close(), DISCONNECT_TIMEOUT)
        finally:
            self.abort()

    async def _close(self) -> None:
        assert self.connection is not None
        await self.connection.close()
        if self.connection.close_code not in {1000, 1001, 1005}:
            raise TransportClosedError(f"Core WebSocket closed abnormally")

    def abort(self) -> None:
        with self._state_lock:
            if not self._stopping:
                self._stopping = True
                try:
                    self.loop.call_soon_threadsafe(self.loop.stop)
                except RuntimeError:
                    pass
        if threading.current_thread() is not self.thread:
            self.thread.join(timeout=CLEANUP_TIMEOUT)
            if self.thread.is_alive():
                raise TransportClosedError(
                    f"Core WebSocket I/O cleanup could not be confirmed",
                )
