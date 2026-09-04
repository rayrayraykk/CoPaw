from __future__ import annotations

import os
import tempfile
import unittest

from qwenpaw_sdk import QwenPaw, QwenPawConfig


class RealCoreTest(unittest.TestCase):
    @unittest.skipUnless(
        os.environ.get(f"QWENPAW_CORE_BIN"),
        f"QWENPAW_CORE_BIN is not set",
    )
    def test_starts_real_app_server_and_creates_thread(self) -> None:
        core_bin = os.environ[f"QWENPAW_CORE_BIN"]
        with tempfile.TemporaryDirectory() as home:
            env = os.environ.copy()
            env[f"QWENPAW_HOME"] = home
            config = QwenPawConfig(core_bin=core_bin, env=env)
            with QwenPaw(config) as qwenpaw:
                thread = qwenpaw.thread_start()
                self.assertEqual(thread.thread[f"status"], f"idle")
                self.assertEqual(thread.thread[f"archived"], False)
