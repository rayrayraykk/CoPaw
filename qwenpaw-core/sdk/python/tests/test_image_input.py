from __future__ import annotations

import base64
import json
import os
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

from qwenpaw_sdk import QwenPaw, QwenPawConfig

PNG = (
    f"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4"
    f"nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC"
)


class ImageInputTest(unittest.TestCase):
    @unittest.skipUnless(
        os.environ.get(f"QWENPAW_CORE_BIN"),
        f"QWENPAW_CORE_BIN is not set",
    )
    def test_core_snapshots_image_and_reuses_it_on_next_turn(self) -> None:
        requests = []

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_args: object) -> None:
                pass

            def do_POST(self) -> None:
                size = int(self.headers[f"Content-Length"])
                requests.append(json.loads(self.rfile.read(size)))
                payload = (
                    f'data: {{"choices":[{{"delta":{{"content":"red"}}}}]}}'
                    f"\n\ndata: [DONE]\n\n"
                ).encode()
                self.send_response(200)
                self.send_header(f"Content-Type", f"text/event-stream")
                self.send_header(f"Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)

        server = ThreadingHTTPServer((f"127.0.0.1", 0), Handler)
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        try:
            with tempfile.TemporaryDirectory() as directory:
                image = Path(directory) / f"red.png"
                image.write_bytes(base64.b64decode(PNG))
                env = os.environ.copy()
                env[f"QWENPAW_HOME"] = directory
                env[f"QWENPAW_API_KEY"] = f"image-fixture-key"
                env[f"QWENPAW_BASE_URL"] = (
                    f"http://127.0.0.1:{server.server_port}"
                )
                config = QwenPawConfig(
                    core_bin=os.environ[f"QWENPAW_CORE_BIN"],
                    env=env,
                    turn_timeout=10.0,
                )
                with QwenPaw(config) as app:
                    thread = app.thread_start(workspace_root=directory)
                    result = thread.run(
                        [{f"type": f"image", f"path": f"red.png"}],
                    )
                    self.assertEqual(result.final_response, f"red")
                    self.assertEqual(
                        result.items[0],
                        {
                            f"id": result.items[0][f"id"],
                            f"type": f"userMessage",
                            f"text": f"",
                            f"input": [
                                {f"type": f"image", f"path": f"red.png"},
                            ],
                        },
                    )
                    image.write_text(f"changed", encoding=f"utf-8")
                    self.assertEqual(
                        thread.run(f"Recall image").final_response, f"red",
                    )
                expected = {
                    f"role": f"user",
                    f"content": [{
                        f"type": f"image_url",
                        f"image_url": {f"url": f"data:image/png;base64,{PNG}"},
                    }],
                }
                self.assertEqual(
                    [r[f"messages"][1] for r in requests],
                    [expected, expected],
                )
        finally:
            server.shutdown()
            server.server_close()
            worker.join(timeout=5)
