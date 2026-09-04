from __future__ import annotations

import sys
import threading
import unittest

from qwenpaw_sdk import (
    PROTOCOL_VERSION,
    QwenPaw,
    QwenPawConfig,
    TransportClosedError,
)

FAKE_SERVER = f"""
import json
import sys

for line in sys.stdin:
    message = json.loads(line)
    method = message.get(\"method\")
    request_id = message.get(\"id\")
    if method == \"initialize\":
        result = {{
            \"protocolVersion\": {PROTOCOL_VERSION},
            \"serverInfo\": {{
                \"name\": \"qwenpaw-core\",
                \"version\": \"0.2.0\",
            }},
        }}
    elif method == \"thread/start\":
        result = {{
            \"thread\": {{
                \"id\": \"thread-1\",
                \"model\": \"qwen\",
                \"workspaceRoot\": None,
                \"status\": \"idle\",
                \"archived\": False,
                \"createdAt\": 1,
                \"updatedAt\": 1,
            }}
        }}
    else:
        continue
    sys.stdout.write(json.dumps({{\"id\": request_id, \"result\": result}}))
    sys.stdout.write(\"\\n\")
    sys.stdout.flush()
"""


class ClientTest(unittest.TestCase):
    def test_starts_app_server_and_creates_a_thread(self) -> None:
        config = QwenPawConfig(
            launch_args_override=(
                sys.executable,
                f"-u",
                f"-c",
                FAKE_SERVER,
            )
        )
        with QwenPaw(config) as qwenpaw:
            thread = qwenpaw.thread_start()
            self.assertEqual(
                thread.thread,
                {
                    f"id": f"thread-1",
                    f"model": f"qwen",
                    f"workspaceRoot": None,
                    f"status": f"idle",
                    f"archived": False,
                    f"createdAt": 1,
                    f"updatedAt": 1,
                },
            )

    def test_stream_reports_app_server_close(self) -> None:
        server = f"""
import json
import sys

for line in sys.stdin:
    message = json.loads(line)
    method = message.get(\"method\")
    request_id = message.get(\"id\")
    if method == \"initialize\":
        result = {{
            \"protocolVersion\": {PROTOCOL_VERSION},
            \"serverInfo\": {{\"name\": \"core\", \"version\": \"0\"}},
        }}
    elif method == \"thread/start\":
        result = {{\"thread\": {{\"id\": \"thread-1\"}}}}
    elif method == \"turn/start\":
        result = {{\"turn\": {{\"id\": \"turn-1\"}}}}
        response = {{\"id\": request_id, \"result\": result}}
        sys.stdout.write(json.dumps(response))
        sys.stdout.write(\"\\n\")
        sys.stdout.flush()
        break
    else:
        continue
    response = {{\"id\": request_id, \"result\": result}}
    sys.stdout.write(json.dumps(response))
    sys.stdout.write(\"\\n\")
    sys.stdout.flush()
"""
        config = QwenPawConfig(
            launch_args_override=(sys.executable, f"-u", f"-c", server),
            turn_timeout=5.0,
        )
        with QwenPaw(config) as qwenpaw:
            thread = qwenpaw.thread_start()
            with self.assertRaises(TransportClosedError):
                next(thread.run_streamed(f"hello"))
        self.assertEqual(
            [
                item.name
                for item in threading.enumerate()
                if item.name == f"qwenpaw-app-server-reader"
            ],
            [],
        )


if __name__ == f"__main__":
    unittest.main()
