# -*- coding: utf-8 -*-
from __future__ import annotations

import json
import queue
import sys
import tempfile
import threading
import time
import unittest
from collections.abc import Callable
from pathlib import Path
from unittest.mock import patch

import qwenpaw_sdk.client as client_module
from qwenpaw_sdk import (
    AppServerClient,
    ProtocolVersionError,
    QwenPaw,
    QwenPawConfig,
    ShutdownError,
    TransportClosedError,
)


def configuration(directory: Path, mode: str = f"0") -> QwenPawConfig:
    return QwenPawConfig(
        launch_args_override=(
            sys.executable,
            f"-u",
            str(Path(__file__).parent / f"support" / f"close_server.py"),
            str(directory / f"finished.json"),
            mode,
        ),
    )


def start(directory: Path, mode: str = f"0") -> AppServerClient:
    client = AppServerClient(configuration(directory, mode))
    client.start()
    return client


def launch(
    call: Callable[[], object],
) -> tuple[threading.Thread, queue.Queue[Exception | None]]:
    result: queue.Queue[Exception | None] = queue.Queue()

    def invoke() -> None:
        try:
            call()
            result.put(None)
        except Exception as error:
            result.put(error)

    worker = threading.Thread(target=invoke, daemon=True)
    worker.start()
    return worker, result


class CloseTest(unittest.TestCase):
    def test_eof_drains_output_and_waits_for_the_final_marker(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            client = start(root)
            client.close()
            self.assertEqual(
                json.loads((root / f"finished.json").read_text()),
                {f"methods": [f"initialize", f"initialized"]},
            )
            client.close()
            with self.assertRaises(TransportClosedError):
                client.request(f"thread/list", {})

    def test_nonzero_exit_is_not_success_and_repeated_close_retains_it(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            client = start(root, f"7")
            with self.assertRaisesRegex(ShutdownError, f"exited with code 7"):
                client.close()
            with self.assertRaisesRegex(ShutdownError, f"exited with code 7"):
                client.close()
            self.assertEqual(
                json.loads((root / f"finished.json").read_text()),
                {f"methods": [f"initialize", f"initialized"]},
            )

    def test_concurrent_and_callback_closes_share_completion(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            client = start(root)
            callback_results: list[object] = []

            def on_close(_error: Exception) -> None:
                worker, result = launch(client.close)
                worker.join(timeout=3)
                callback_results.append(worker.is_alive())
                if not worker.is_alive():
                    callback_results.append(result.get_nowait())
                client.close()

            client.on_close(on_close)
            callers = [launch(client.close) for _ in range(4)]
            for worker, result in callers:
                worker.join(timeout=5)
                self.assertEqual(worker.is_alive(), False)
                self.assertEqual(result.get_nowait(), None)
            self.assertEqual(callback_results, [False, None])
            self.assertEqual(
                json.loads((root / f"finished.json").read_text()),
                {f"methods": [f"initialize", f"initialized"]},
            )

    def test_reader_callback_close_transfers_remaining_output(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            client = start(root)
            reader = client._reader
            result: queue.Queue[Exception | None] = queue.Queue()

            def on_notification(_notification: object) -> None:
                try:
                    client.close()
                    result.put(None)
                except Exception as error:
                    result.put(error)

            client.on_notification(on_notification)
            client.notify(f"fixture/notify", {})
            self.assertEqual(result.get(timeout=5), None)
            assert reader is not None
            reader.join(timeout=5)
            self.assertEqual(reader.is_alive(), False)
            client.close()
            self.assertEqual(
                json.loads((root / f"finished.json").read_text()),
                {
                    f"methods": [
                        f"initialize",
                        f"initialized",
                        f"fixture/notify",
                    ],
                },
            )

    def test_reader_reentry_does_not_wait_for_another_closer(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            client = start(Path(directory))
            entered = threading.Event()
            release = threading.Event()
            callback_result: queue.Queue[Exception | None] = queue.Queue()

            def callback(_notification: object) -> None:
                entered.set()
                release.wait(timeout=3)
                try:
                    client.close()
                    callback_result.put(None)
                except Exception as error:
                    callback_result.put(error)

            client.on_notification(callback)
            client.notify(f"fixture/notify", {})
            self.assertEqual(entered.wait(timeout=2), True)
            worker, result = launch(client.close)
            try:
                deadline = time.monotonic() + 2
                while client._closed_error is None:
                    self.assertLess(time.monotonic(), deadline)
                    time.sleep(0.01)
            finally:
                release.set()
            worker.join(timeout=5)
            self.assertEqual(worker.is_alive(), False)
            self.assertEqual(result.get_nowait(), None)
            self.assertEqual(callback_result.get(timeout=1), None)

    def test_pending_request_is_woken_and_notifications_are_rejected(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            client = start(Path(directory))
            received = threading.Event()
            client.on_notification(lambda _: received.set())
            worker, result = launch(
                lambda: client.request(f"fixture/wait", {}),
            )
            self.assertEqual(received.wait(timeout=2), True)
            client.close()
            worker.join(timeout=2)
            self.assertEqual(worker.is_alive(), False)
            self.assertIsInstance(result.get_nowait(), TransportClosedError)
            with self.assertRaises(TransportClosedError):
                client.notify(f"initialized", {})
            with self.assertRaisesRegex(RuntimeError, f"new client"):
                client.start()

    def test_startup_and_context_errors_are_not_replaced_by_cleanup(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaises(ProtocolVersionError):
                start(root, f"badinit")
            self.assertEqual(
                json.loads((root / f"finished.json").read_text()),
                {f"methods": [f"initialize"]},
            )
            with self.assertRaisesRegex(ValueError, f"fixture body"):
                with QwenPaw(configuration(root, f"7")):
                    raise ValueError(f"fixture body")
            self.assertEqual(
                json.loads((root / f"finished.json").read_text()),
                {f"methods": [f"initialize", f"initialized"]},
            )

    def test_timeout_kills_owned_process_and_retains_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            client = start(Path(directory), f"hold")
            process = client._process
            reader = client._reader
            started = time.monotonic()
            with self.assertRaisesRegex(ShutdownError, f"timed out"):
                client.close()
            self.assertGreaterEqual(time.monotonic() - started, 29.5)
            with self.assertRaisesRegex(ShutdownError, f"timed out"):
                client.close()
            assert process is not None and reader is not None
            self.assertIsNotNone(process.poll())
            self.assertNotEqual(process.returncode, 0)
            self.assertEqual(reader.is_alive(), False)
            self.assertEqual(
                (Path(directory) / f"finished.json").exists(),
                False,
            )

    def test_stalled_callback_reports_incomplete_reader_cleanup(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            client = start(Path(directory))
            process = client._process
            reader = client._reader
            entered = threading.Event()
            release = threading.Event()

            def callback(_notification: object) -> None:
                entered.set()
                release.wait(timeout=5)

            client.on_notification(callback)
            client.notify(f"fixture/notify", {})
            self.assertEqual(entered.wait(timeout=2), True)
            try:
                with (
                    patch.object(client_module, f"_SHUTDOWN_TIMEOUT", 0.1),
                    patch.object(client_module, f"_CLEANUP_TIMEOUT", 0.2),
                ):
                    with self.assertRaisesRegex(
                        ShutdownError,
                        f"cleanup could not be confirmed",
                    ):
                        client.close()
                assert process is not None and reader is not None
                self.assertIsNotNone(process.poll())
                self.assertEqual(reader.is_alive(), True)
            finally:
                release.set()
                assert reader is not None
                reader.join(timeout=5)
            self.assertEqual(reader.is_alive(), False)
            with self.assertRaises(ShutdownError):
                client.close()

    def test_stalled_write_lock_retains_handles_and_reports_failure(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            client = start(Path(directory))
            process = client._process
            reader = client._reader
            try:
                with client._write_lock:
                    with (
                        patch.object(client_module, f"_SHUTDOWN_TIMEOUT", 0.1),
                        patch.object(client_module, f"_CLEANUP_TIMEOUT", 0.2),
                    ):
                        with self.assertRaisesRegex(
                            ShutdownError,
                            f"cleanup could not be confirmed",
                        ):
                            client.close()
                    self.assertIs(client._process, process)
            finally:
                assert process is not None and reader is not None
                process.wait(timeout=5)
                assert process.stdin is not None
                process.stdin.close()
                reader.join(timeout=5)
            self.assertEqual(reader.is_alive(), False)
            with self.assertRaises(ShutdownError):
                client.close()
