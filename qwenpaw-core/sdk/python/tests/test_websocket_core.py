from __future__ import annotations

import json
import os
import queue
import re
import subprocess
import tempfile
import threading
import time
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

from qwenpaw_sdk import (
    TransportClosedError,
    WebSocketConnection,
    WebSocketOptions,
)


@unittest.skipUnless(
    os.environ.get(f"QWENPAW_CORE_BIN"), f"Source Core needed"
)
class WebSocketCoreTest(unittest.TestCase):
    @unittest.skipUnless(
        os.environ.get(f"QWENPAW_TS_SDK_ENTRY"),
        f"TypeScript SDK entry needed",
    )
    def test_typescript_and_python_share_one_existing_core(self):
        home, process, endpoint = self.start_core()
        first = self.connect(endpoint)
        thread = first.thread_start(workspace_root=home)
        before = first.client.request(
            f"thread/read",
            {f"threadId": thread.id},
        )
        script = f"""
const {{WebSocketConnection}} = require(process.argv[1]);
(async () => {{
  const c = await WebSocketConnection.connect(process.argv[2], {{
    clientInfo: {{name: 'cross-language', title: 'Fixture', version: '1'}}
  }});
  try {{
    const read = await c.client.request('thread/read', {{
      threadId: process.argv[3]
    }});
    const own = await c.startThread({{workspaceRoot: process.argv[4]}});
    console.log(JSON.stringify({{read, own: own.thread}}));
  }} finally {{ await c.disconnect(); }}
}})().catch(error => {{ console.error(error); process.exitCode = 1; }});
"""
        output = subprocess.run(
            [
                f"node",
                f"-e",
                script,
                os.environ[f"QWENPAW_TS_SDK_ENTRY"],
                endpoint,
                thread.id,
                str(home),
            ],
            check=True,
            capture_output=True,
            text=True,
            timeout=5,
        )
        result = json.loads(output.stdout)
        self.assertEqual(result[f"read"], before)
        self.assertEqual(
            first.thread_resume(result[f"own"][f"id"]).thread,
            result[f"own"],
        )
        first.disconnect()
        self.assertIsNone(process.poll())
        with self.connect(endpoint) as again:
            self.assertEqual(
                again.client.request(
                    f"thread/read",
                    {f"threadId": thread.id},
                ),
                before,
            )

    def start_core(self, args=(), environment=None):
        directory = tempfile.TemporaryDirectory(prefix=f"qwenpaw-python-ws-")
        self.addCleanup(directory.cleanup)
        home = Path(directory.name)
        extra_args = args(home) if callable(args) else args
        process = subprocess.Popen(
            [
                os.environ[f"QWENPAW_CORE_BIN"],
                f"app-server",
                f"--listen",
                f"127.0.0.1:0",
                *extra_args,
            ],
            cwd=home,
            env={
                **os.environ,
                f"QWENPAW_HOME": str(home),
                f"RUST_LOG": f"info",
                **(environment or {}),
            },
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
        )
        ready: queue.Queue[str] = queue.Queue()

        def logs() -> None:
            assert process.stderr is not None
            for line in process.stderr:
                found = re.search(f"address=127[.]0[.]0[.]1:([0-9]+)", line)
                if found:
                    ready.put(found.group(1))

        reader = threading.Thread(target=logs, daemon=True)
        reader.start()

        def cleanup() -> None:
            if process.poll() is None:
                process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=2)
                self.fail(f"Fixture Core did not terminate")
            finally:
                reader.join(timeout=2)
                assert process.stderr is not None
                process.stderr.close()
            self.assertFalse(reader.is_alive())

        self.addCleanup(cleanup)
        port = ready.get(timeout=5)
        return home, process, f"ws://127.0.0.1:{port}/app-protocol"

    def connect(self, endpoint, options=None):
        connection = WebSocketConnection.connect(endpoint, options)
        self.addCleanup(connection.dispose)
        return connection

    def test_shared_core_turn_finishes_after_detach_and_facade_runs(self):
        requested = threading.Event()
        release = threading.Event()
        errors: queue.Queue[BaseException] = queue.Queue()
        owner = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, _format, *args):
                pass

            def do_POST(self):
                try:
                    owner.assertEqual(
                        self.headers.get(f"Authorization"),
                        f"Bearer local-model-fixture-key",
                    )
                    self.rfile.read(int(self.headers[f"Content-Length"]))
                    requested.set()
                    owner.assertTrue(release.wait(timeout=3))
                    self.send_response(200)
                    self.send_header(f"content-type", f"text/event-stream")
                    self.end_headers()
                    event = json.dumps(
                        {
                            f"choices": [
                                {
                                    f"delta": {
                                        f"content": f"reply after detach"
                                    },
                                    f"finish_reason": None,
                                }
                            ],
                        }
                    )
                    self.wfile.write(
                        f"data: {event}\n\ndata: [DONE]\n\n".encode(f"utf-8"),
                    )
                except BaseException as error:
                    errors.put(error)

        model = ThreadingHTTPServer((f"127.0.0.1", 0), Handler)
        serving = threading.Thread(target=model.serve_forever, daemon=True)
        serving.start()

        def stop_model():
            release.set()
            model.shutdown()
            model.server_close()
            serving.join(timeout=2)
            self.assertFalse(serving.is_alive())
            self.assertEqual(list(errors.queue), [])

        self.addCleanup(stop_model)
        home, process, endpoint = self.start_core(
            environment={
                f"QWENPAW_API_KEY": f"local-model-fixture-key",
                f"QWENPAW_BASE_URL": (
                    f"http://127.0.0.1:{model.server_port}/v1"
                ),
            }
        )
        first = self.connect(endpoint)
        second = self.connect(endpoint)
        config = second.client.request(f"config/read", {})
        thread = first.thread_start(workspace_root=home)
        resumed = second.thread_resume(thread.id)
        self.assertEqual(resumed.thread, thread.thread)
        first.client.request(
            f"turn/start",
            {
                f"threadId": thread.id,
                f"input": [{f"type": f"text", f"text": f"hold until detach"}],
            },
        )
        self.assertTrue(requested.wait(timeout=2))
        params = {f"threadId": thread.id}
        before = first.client.request(f"thread/read", params)
        self.assertEqual(before[f"turns"][0][f"status"], f"inProgress")
        first.disconnect()
        self.assertEqual(second.client.request(f"thread/read", params), before)
        release.set()
        deadline = time.monotonic() + 5
        while True:
            after = second.client.request(f"thread/read", params)
            if after[f"turns"][0][f"status"] != f"inProgress":
                break
            self.assertLess(time.monotonic(), deadline)
            time.sleep(0.01)
        self.assertEqual(after[f"turns"][0][f"status"], f"completed")
        self.assertEqual(second.client.request(f"config/read", {}), config)
        second.disconnect()
        self.assertIsNone(process.poll())
        third = self.connect(endpoint)
        self.assertEqual(third.client.request(f"thread/read", params), after)
        result = third.thread_resume(thread.id).run(f"another turn")
        self.assertEqual(result.final_response, f"reply after detach")
        latest = third.client.request(f"thread/read", params)
        self.assertEqual(result.turn, latest[f"turns"][-1])
        third.disconnect()
        self.assertIsNone(process.poll())

    def test_real_wss_requires_ca_hostname_and_token(self):
        token = f"fixture-core-token-01234567890123456789"

        def credentials(home: Path):
            certificate = home / f"cert.pem"
            key = home / f"key.pem"
            token_file = home / f"token"
            subprocess.run(
                [
                    f"openssl",
                    f"req",
                    f"-x509",
                    f"-newkey",
                    f"rsa:2048",
                    f"-nodes",
                    f"-days",
                    f"1",
                    f"-subj",
                    f"/CN=localhost",
                    f"-addext",
                    f"subjectAltName=DNS:localhost",
                    f"-keyout",
                    str(key),
                    f"-out",
                    str(certificate),
                ],
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            key.chmod(0o600)
            descriptor = os.open(
                token_file,
                os.O_CREAT | os.O_EXCL | os.O_WRONLY,
                0o600,
            )
            with os.fdopen(descriptor, f"w") as output:
                output.write(token)
            return [
                f"--remote",
                f"--tls-cert",
                str(certificate),
                f"--tls-key",
                str(key),
                f"--auth-token-file",
                str(token_file),
            ]

        home, process, endpoint = self.start_core(credentials)
        endpoint = endpoint.replace(f"ws:", f"wss:")
        url = endpoint.replace(f"127.0.0.1", f"localhost")
        ca = (home / f"cert.pem").read_text()
        with self.assertRaises(TransportClosedError):
            self.connect(url, WebSocketOptions(bearer_token=token))
        with self.assertRaises(TransportClosedError):
            self.connect(
                endpoint,
                WebSocketOptions(
                    bearer_token=token,
                    ca_pem=ca,
                ),
            )
        for bearer in (None, f"wrong-fixture-token-01234567890123456789"):
            with self.assertRaises(TransportClosedError) as error:
                self.connect(
                    url,
                    WebSocketOptions(
                        bearer_token=bearer,
                        ca_pem=ca,
                    ),
                )
            self.assertEqual(
                str(error.exception),
                f"Core WebSocket handshake returned HTTP 401",
            )
        options = WebSocketOptions(bearer_token=token, ca_pem=ca)
        first = self.connect(url, options)
        second = self.connect(url, options)
        config = first.client.request(f"config/read", {})
        first.disconnect()
        self.assertEqual(second.client.request(f"config/read", {}), config)
        second.disconnect()
        self.assertIsNone(process.poll())
