from __future__ import annotations

import asyncio
import json
import queue
import socket
import threading
import unittest
from concurrent.futures import ThreadPoolExecutor
from unittest.mock import patch

from websockets.exceptions import ConnectionClosed
from websockets.sync.server import ServerConnection, serve

from qwenpaw_sdk import (
    Notification,
    ProtocolVersionError,
    TransportClosedError,
    WebSocketConnection,
    WebSocketOptions,
)
from qwenpaw_sdk import _websocket_transport as transport_module
from qwenpaw_sdk import websocket as client_module


class WebSocketTest(unittest.TestCase):
    def setUp(self) -> None:
        self.addCleanup(self.assert_no_sdk_threads)

    def assert_no_sdk_threads(self) -> None:
        threads = [
            thread
            for thread in threading.enumerate()
            if thread.name.startswith(f"qwenpaw-websocket-")
        ]
        for thread in threads:
            thread.join(timeout=2)
        self.assertEqual(
            [thread.name for thread in threads if thread.is_alive()],
            [],
        )

    def server(self, handler):
        sockets: list[ServerConnection] = []
        handlers: list[threading.Thread] = []
        errors: queue.Queue[BaseException] = queue.Queue()

        def handle(connection: ServerConnection) -> None:
            handlers.append(threading.current_thread())
            sockets.append(connection)
            try:
                handler(connection)
            except ConnectionClosed:
                pass
            except BaseException as error:
                errors.put(error)

        server = serve(handle, f"127.0.0.1", 0, ping_interval=None)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()

        def cleanup() -> None:
            server.shutdown()
            for connection in sockets:
                connection.close_socket()
                connection.recv_events_thread.join(timeout=2)
            thread.join(timeout=2)
            for handler_thread in handlers:
                handler_thread.join(timeout=2)
                self.assertFalse(handler_thread.is_alive())
            self.assertFalse(thread.is_alive())
            self.assertEqual(list(errors.queue), [])

        self.addCleanup(cleanup)
        return f"ws://127.0.0.1:{server.socket.getsockname()[1]}/app-protocol"

    def initialize(self, connection: ServerConnection, version: int = 3):
        request = json.loads(connection.recv(timeout=2))
        self.assertEqual(request[f"method"], f"initialize")
        connection.send(
            json.dumps(
                {
                    f"id": request[f"id"],
                    f"result": {
                        f"protocolVersion": version,
                        f"serverInfo": {f"name": f"fixture", f"version": f"1"},
                    },
                },
                indent=2,
            )
        )

    def connect(self, endpoint: str, **kwargs) -> WebSocketConnection:
        connection = WebSocketConnection.connect(endpoint, **kwargs)
        self.addCleanup(connection.dispose)
        return connection

    def test_validation_and_repr_do_not_expose_credentials(self) -> None:
        for endpoint in (
            f"ws://localhost/",
            f"ws://192.0.2.1/",
            f"ws://[::]/",
            f"https://localhost/",
            f"wss://user:secret@localhost/",
            f"wss://localhost/?secret",
            f"wss://localhost/#secret",
            f"wss://localhost/?",
            f"wss://localhost/#",
            f"wss://localhost:\r\nsecret/",
            f"ws://127.0.0.1:0/",
        ):
            with self.subTest(endpoint=endpoint):
                with self.assertRaises(ValueError) as error:
                    self.connect(endpoint)
                self.assertNotIn(f"secret", str(error.exception))
        for token in (f"short", f"x" * 4097, f"x" * 32 + f"\r\n"):
            with self.assertRaisesRegex(ValueError, f"Invalid Core bearer"):
                self.connect(
                    f"wss://localhost/",
                    options=WebSocketOptions(bearer_token=token),
                )
        self.assertNotIn(
            f"secret",
            repr(
                WebSocketOptions(
                    bearer_token=f"secret" * 8,
                    ca_pem=f"secret CA",
                )
            ),
        )

    def test_correlation_notifications_ping_and_repeat_close(self) -> None:
        observed: queue.Queue[dict] = queue.Queue()
        token = f"fixture-token-01234567890123456789"

        def handle(connection: ServerConnection) -> None:
            self.assertEqual(
                connection.request.headers[f"Authorization"],
                f"Bearer {token}",
            )
            self.initialize(connection)
            observed.put(json.loads(connection.recv(timeout=2)))
            ping = connection.ping(f"fixture")
            requests = [
                json.loads(connection.recv(timeout=2)) for _ in range(2)
            ]
            connection.send(
                json.dumps(
                    {
                        f"method": f"fixture/event",
                        f"params": {f"n": 1},
                    }
                )
            )
            for request in reversed(requests):
                connection.send(
                    json.dumps(
                        {
                            f"id": request[f"id"],
                            f"result": request[f"params"],
                        }
                    )
                )
            self.assertTrue(ping.wait(timeout=2))
            connection.recv(timeout=2)

        connection = self.connect(
            self.server(handle),
            options=WebSocketOptions(bearer_token=token),
        )
        events: queue.Queue[Notification] = queue.Queue()
        connection.client.on_notification(events.put)
        with ThreadPoolExecutor(max_workers=2) as pool:
            first = pool.submit(connection.client.request, f"echo", {f"n": 1})
            second = pool.submit(connection.client.request, f"echo", {f"n": 2})
            self.assertEqual(
                [first.result(timeout=2), second.result(timeout=2)],
                [{f"n": 1}, {f"n": 2}],
            )
        self.assertEqual(
            events.get(timeout=2),
            Notification(
                method=f"fixture/event",
                params={f"n": 1},
            ),
        )
        self.assertEqual(
            observed.get(timeout=2),
            {
                f"method": f"initialized",
                f"params": {},
            },
        )
        connection.disconnect()
        connection.disconnect()
        with self.assertRaises(TransportClosedError):
            connection.client.request(f"config/read", {})

    def test_bad_version_closes_without_initialized(self) -> None:
        def handle(connection: ServerConnection) -> None:
            self.initialize(connection, version=0)
            connection.recv(timeout=2)
            self.fail(f"Incompatible client sent another message")

        with self.assertRaises(ProtocolVersionError):
            self.connect(self.server(handle))

    def test_invalid_frames_close_pending_without_reflecting_payload(self):
        for payload in (
            bytes([1]),
            f"not JSON secret",
            f"null",
            f"x" * 1048577,
        ):
            with self.subTest(size=len(payload)):

                def handle(connection: ServerConnection) -> None:
                    self.initialize(connection)
                    connection.recv(timeout=2)
                    connection.recv(timeout=2)
                    connection.send(payload)
                    connection.recv(timeout=2)

                connection = self.connect(self.server(handle))
                with self.assertRaises(TransportClosedError) as error:
                    connection.client.request(f"echo", {})
                self.assertNotIn(f"secret", str(error.exception))
                with self.assertRaises(TransportClosedError):
                    connection.disconnect()

    def test_outgoing_limit_does_not_send_payload(self) -> None:
        def handle(connection: ServerConnection) -> None:
            self.initialize(connection)
            for message in connection:
                self.assertEqual(
                    json.loads(message),
                    {
                        f"method": f"initialized",
                        f"params": {},
                    },
                )

        connection = self.connect(self.server(handle))
        with self.assertRaises(TransportClosedError):
            connection.client.request(f"echo", {f"text": f"x" * 1048576})
        with self.assertRaises(TransportClosedError):
            connection.disconnect()

    def test_cancel_initialize_wakes_pending_and_stops_threads(self) -> None:
        received = threading.Event()

        def handle(connection: ServerConnection) -> None:
            connection.recv(timeout=2)
            received.set()
            connection.recv(timeout=2)

        endpoint = self.server(handle)
        cancel = threading.Event()
        with ThreadPoolExecutor(max_workers=1) as pool:
            opened = pool.submit(
                WebSocketConnection.connect, endpoint, cancel=cancel
            )
            self.assertTrue(received.wait(timeout=2))
            cancel.set()
            with self.assertRaisesRegex(TransportClosedError, f"aborted"):
                opened.result(timeout=2)

    def test_callback_disconnect_and_concurrent_close(self) -> None:
        proceed = threading.Event()

        def handle(connection: ServerConnection) -> None:
            self.initialize(connection)
            connection.recv(timeout=2)
            self.assertTrue(proceed.wait(timeout=2))
            connection.send(
                json.dumps(
                    {
                        f"method": f"fixture/close",
                        f"params": {},
                    }
                )
            )
            connection.recv(timeout=2)

        connection = self.connect(self.server(handle))
        callback_done = threading.Event()
        connection.client.on_notification(
            lambda _event: (connection.disconnect(), callback_done.set()),
        )
        proceed.set()
        self.assertTrue(callback_done.wait(timeout=2))
        with ThreadPoolExecutor(max_workers=2) as pool:
            results = list(
                pool.map(lambda _: connection.disconnect(), range(2))
            )
        self.assertEqual(results, [None, None])

    def test_disconnect_timeout_is_sticky_and_stops_io(self) -> None:
        def handle(connection: ServerConnection) -> None:
            self.initialize(connection)
            for _ in connection:
                pass

        connection = self.connect(self.server(handle))

        async def stalled() -> None:
            await asyncio.Event().wait()

        with (
            patch.object(transport_module, f"DISCONNECT_TIMEOUT", 0.1),
            patch.object(client_module, f"DISCONNECT_TIMEOUT", 0.1),
            patch.object(connection.client._transport, f"_close", stalled),
        ):
            for _ in range(2):
                with self.assertRaises(TransportClosedError) as error:
                    connection.disconnect()
                self.assertIn(f"timed out", str(error.exception))
        self.assertFalse(connection.client._transport.thread.is_alive())

    def idle_endpoint(self):
        def handle(connection: ServerConnection) -> None:
            self.initialize(connection)
            for _ in connection:
                pass

        return self.server(handle)

    def test_dispose_wakes_pending_without_spawning_a_process(self):
        with patch(
            f"qwenpaw_sdk.client.subprocess.Popen",
            side_effect=AssertionError(f"Connected mode must not spawn Core"),
        ):
            connection = self.connect(self.idle_endpoint())
        received = threading.Event()

        def request():
            received.set()
            return connection.client.request(f"echo", {})

        with ThreadPoolExecutor(max_workers=1) as pool:
            pending = pool.submit(request)
            self.assertTrue(received.wait(timeout=2))
            connection.dispose()
            with self.assertRaises(TransportClosedError):
                pending.result(timeout=2)
        self.assertIsNone(connection.client._process)

    def test_write_backpressure_and_disconnect_do_not_deadlock(self):
        connection = self.connect(self.idle_endpoint())
        entered = threading.Event()

        async def stalled(_message):
            entered.set()
            await asyncio.Event().wait()

        with patch.object(connection.client._transport, f"_send", stalled):
            with ThreadPoolExecutor(max_workers=1) as pool:
                pending = pool.submit(connection.client.request, f"echo", {})
                self.assertTrue(entered.wait(timeout=2))
                connection.disconnect()
                with self.assertRaises(TransportClosedError) as error:
                    pending.result(timeout=2)
                self.assertEqual(
                    error.exception.args,
                    (f"Core WebSocket connection was closed",),
                )
                self.assertIsNone(connection.client._transport_error)

    def test_buffer_budget_fails_closed_and_private_logger_stays_disabled(
        self,
    ):
        connection = self.connect(self.idle_endpoint())
        websocket = connection.client._transport.connection
        self.assertIsNotNone(websocket)
        self.assertTrue(websocket.logger.logger.disabled)
        with patch.object(
            websocket.transport,
            f"get_write_buffer_size",
            return_value=2097152,
        ):
            with self.assertRaises(TransportClosedError):
                connection.client.request(f"echo", {})
        with self.assertRaises(TransportClosedError):
            connection.disconnect()

    def test_stalled_close_handler_has_bounded_cleanup(self):
        for operation in (f"disconnect", f"dispose"):
            with self.subTest(operation=operation):
                connection = self.connect(self.idle_endpoint())
                entered = threading.Event()
                release = threading.Event()

                def callback(_error):
                    entered.set()
                    release.wait(timeout=2)

                connection.client.on_close(callback)
                with ThreadPoolExecutor(max_workers=1) as pool:
                    try:
                        with (
                            patch.object(
                                client_module, f"DISCONNECT_TIMEOUT", 0.05
                            ),
                            patch.object(
                                client_module, f"CLEANUP_TIMEOUT", 0.05
                            ),
                        ):
                            closing = pool.submit(
                                getattr(connection, operation)
                            )
                            self.assertTrue(entered.wait(timeout=2))
                            with self.assertRaisesRegex(
                                TransportClosedError, f"callback cleanup"
                            ):
                                closing.result(timeout=0.5)
                        self.assertFalse(
                            connection.client._transport.thread.is_alive()
                        )
                    finally:
                        release.set()
                with self.assertRaisesRegex(
                    TransportClosedError, f"callback cleanup"
                ):
                    connection.disconnect()

    def test_close_handler_can_reenter_disconnect(self):
        connection = self.connect(self.idle_endpoint())
        results = []

        def callback(_error):
            connection.disconnect()
            results.append(None)

        connection.client.on_close(callback)
        connection.disconnect()
        self.assertEqual(results, [None])

    def test_stalled_callback_reports_incomplete_reader_cleanup(self):
        entered = threading.Event()
        release = threading.Event()
        proceed = threading.Event()

        def handle(connection: ServerConnection) -> None:
            self.initialize(connection)
            connection.recv(timeout=2)
            self.assertTrue(proceed.wait(timeout=2))
            connection.send(
                json.dumps(
                    {
                        f"method": f"fixture/event",
                        f"params": {},
                    }
                )
            )
            connection.recv(timeout=2)

        connection = self.connect(self.server(handle))
        self.addCleanup(release.set)

        def callback(_event):
            entered.set()
            release.wait(timeout=2)

        connection.client.on_notification(callback)
        proceed.set()
        self.assertTrue(entered.wait(timeout=2))
        with patch.object(client_module, f"DISCONNECT_TIMEOUT", 0.05):
            with self.assertRaisesRegex(
                TransportClosedError, f"reader cleanup"
            ):
                connection.disconnect()
        self.assertFalse(connection.client._transport.thread.is_alive())
        release.set()
        connection.client._reader.join(timeout=2)
        with self.assertRaisesRegex(TransportClosedError, f"reader cleanup"):
            connection.disconnect()

    def test_context_body_error_is_preserved_and_precancelled_open_is_local(
        self,
    ):
        with self.assertRaisesRegex(ValueError, f"body failure"):
            with self.connect(self.idle_endpoint()):
                raise ValueError(f"body failure")
        cancelled = threading.Event()
        cancelled.set()
        with self.assertRaisesRegex(TransportClosedError, f"aborted"):
            self.connect(f"ws://127.0.0.1:1/", cancel=cancelled)

    def raw_server(self, reply: bytes | None = None):
        listener = socket.socket()
        listener.bind((f"127.0.0.1", 0))
        listener.listen()
        listener.settimeout(2)
        received = threading.Event()
        eof = threading.Event()

        def serve_raw() -> None:
            with listener.accept()[0] as peer:
                peer.settimeout(2)
                request = bytes()
                while not request.endswith(bytes([13, 10, 13, 10])):
                    request += peer.recv(1)
                received.set()
                if reply is not None:
                    peer.sendall(reply)
                if peer.recv(1) == bytes():
                    eof.set()

        thread = threading.Thread(target=serve_raw, daemon=True)
        thread.start()

        def cleanup() -> None:
            listener.close()
            thread.join(timeout=3)
            self.assertFalse(thread.is_alive())

        self.addCleanup(cleanup)
        return (
            f"ws://127.0.0.1:{listener.getsockname()[1]}/app-protocol",
            received,
            eof,
        )

    def test_handshake_cancel_closes_socket(self) -> None:
        endpoint, received, eof = self.raw_server()
        cancel = threading.Event()
        with ThreadPoolExecutor(max_workers=1) as pool:
            opened = pool.submit(
                WebSocketConnection.connect, endpoint, cancel=cancel
            )
            self.assertTrue(received.wait(timeout=2))
            cancel.set()
            with self.assertRaisesRegex(TransportClosedError, f"aborted"):
                opened.result(timeout=2)
        self.assertTrue(eof.wait(timeout=2))

    def test_handshake_timeout_and_redirect_do_not_leak(self) -> None:
        endpoint, _received, eof = self.raw_server()
        with patch.object(transport_module, f"CONNECT_TIMEOUT", 0.1):
            with self.assertRaises(TransportClosedError):
                self.connect(endpoint)
        self.assertTrue(eof.wait(timeout=2))
        endpoint, _received, eof = self.raw_server(
            (
                f"HTTP/1.1 302 Found\r\nLocation: /secret\r\n"
                f"X-Token: secret\r\nContent-Length: 6\r\n\r\nsecret"
            ).encode(f"ascii")
        )
        with self.assertRaises(TransportClosedError) as error:
            self.connect(endpoint)
        self.assertEqual(
            str(error.exception), f"Core WebSocket handshake returned HTTP 302"
        )
        self.assertTrue(eof.wait(timeout=2))
