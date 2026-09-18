# -*- coding: utf-8 -*-
from __future__ import annotations

import contextlib
import json
import os
import sqlite3
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

from qwenpaw_sdk import AppServerClient, QwenPawConfig, ShutdownError


@unittest.skipUnless(
    os.environ.get(f"QWENPAW_CORE_BIN"),
    f"QWENPAW_CORE_BIN is not set",
)
class CoreCloseTest(unittest.TestCase):
    def test_close_saves_interruption_before_startup_recovery(self) -> None:
        self._run_close(False)

    def test_close_reports_real_core_final_persistence_failure(self) -> None:
        self._run_close(True)

    def _close_client(
        self,
        client: AppServerClient,
        reject_final: bool,
    ) -> None:
        if reject_final:
            with self.assertRaises(ShutdownError) as failure:
                client.close()
            self.assertEqual(
                str(failure.exception),
                f"QwenPaw Core exited with code 1",
            )
        else:
            client.close()

    def _run_close(self, reject_final: bool) -> None:
        received = threading.Event()
        release = threading.Event()

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_args: object) -> None:
                pass

            def do_POST(self) -> None:
                self.rfile.read(int(self.headers[f"Content-Length"]))
                received.set()
                release.wait(timeout=10)

        model = ThreadingHTTPServer((f"127.0.0.1", 0), Handler)
        model_worker = threading.Thread(
            target=model.serve_forever,
            daemon=True,
        )
        model_worker.start()
        try:
            with (
                tempfile.TemporaryDirectory() as directory,
                contextlib.ExitStack() as cleanup,
            ):
                root = Path(directory)
                env = os.environ.copy()
                env.update(
                    {
                        f"QWENPAW_HOME": directory,
                        f"QWENPAW_API_KEY": f"close-fixture-key",
                        f"QWENPAW_BASE_URL": (
                            f"http://127.0.0.1:{model.server_port}/v1"
                        ),
                    },
                )
                config = QwenPawConfig(
                    core_bin=os.environ[f"QWENPAW_CORE_BIN"],
                    cwd=root,
                    env=env,
                )
                client = AppServerClient(config)
                client.start()
                cleanup.callback(self._close_client, client, reject_final)
                thread = client.request(
                    f"thread/start",
                    {
                        f"workspaceRoot": directory,
                        f"model": None,
                    },
                )[f"thread"]
                thread_id = thread[f"id"]
                client.request(
                    f"turn/start",
                    {
                        f"threadId": thread_id,
                        f"input": [{f"type": f"text", f"text": f"hold"}],
                    },
                )
                self.assertEqual(received.wait(timeout=5), True)
                expected = client.request(
                    f"thread/read",
                    {f"threadId": thread_id},
                )
                self.assertEqual(len(expected[f"turns"]), 1)
                self.assertEqual(
                    expected[f"turns"][0][f"status"],
                    f"inProgress",
                )
                if reject_final:
                    with contextlib.closing(
                        sqlite3.connect(root / f"threads.sqlite3"),
                    ) as connection:
                        connection.executescript(
                            f"CREATE TRIGGER reject_final "
                            f"BEFORE INSERT ON threads "
                            f"WHEN json_extract(NEW.snapshot, "
                            f"'$.turns[#-1].status') != 'inProgress' "
                            f"BEGIN SELECT RAISE(FAIL, "
                            f"'fixture final write failure'); END;",
                        )
                self._close_client(client, reject_final)
                database = (root / f"threads.sqlite3").resolve().as_uri()
                with contextlib.closing(
                    sqlite3.connect(f"{database}?mode=ro", uri=True),
                ) as connection:
                    row = connection.execute(
                        f"SELECT snapshot FROM threads WHERE id = ?",
                        (thread_id,),
                    ).fetchone()
                self.assertIsNotNone(row)
                saved = json.loads(row[0])
                if not reject_final:
                    self.assertGreaterEqual(
                        saved[f"thread"][f"updatedAt"],
                        expected[f"thread"][f"updatedAt"],
                    )
                    expected[f"thread"][f"updatedAt"] = saved[f"thread"][
                        f"updatedAt"
                    ]
                    expected[f"thread"][f"status"] = f"idle"
                    expected[f"turns"][0][f"status"] = f"interrupted"
                self.assertEqual(
                    {
                        f"thread": saved[f"thread"],
                        f"turns": saved[f"turns"],
                    },
                    expected,
                )
                if reject_final:
                    with contextlib.closing(
                        sqlite3.connect(root / f"threads.sqlite3"),
                    ) as connection:
                        connection.execute(f"DROP TRIGGER reject_final")
                        connection.commit()
                self._check_reopened(config, thread_id, expected, reject_final)
        finally:
            release.set()
            model.shutdown()
            model.server_close()
            model_worker.join(timeout=5)

    def _check_reopened(
        self,
        config: QwenPawConfig,
        thread_id: str,
        expected: dict[str, Any],
        reject_final: bool,
    ) -> None:
        reopened = AppServerClient(config)
        reopened.start()
        try:
            restored = reopened.request(
                f"thread/read",
                {f"threadId": thread_id},
            )
            if reject_final:
                self.assertGreaterEqual(
                    restored[f"thread"][f"updatedAt"],
                    expected[f"thread"][f"updatedAt"],
                )
                expected[f"thread"].update(
                    {
                        f"status": f"idle",
                        f"updatedAt": restored[f"thread"][f"updatedAt"],
                    },
                )
                expected[f"turns"][0][f"status"] = f"interrupted"
            self.assertEqual(restored, expected)
        finally:
            reopened.close()
