"""Verify original session method extraction and full result structures."""

import json
import subprocess
import sys
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / f"codex_session_reference.py"


def reference(fixture):
    """Execute only the isolated original-method reference."""
    result = subprocess.run(
        [sys.executable, str(SCRIPT), json.dumps(fixture)],
        check=True,
        capture_output=True,
        text=True,
        timeout=10,
    )
    return json.loads(result.stdout)


def start():
    """Return the original default thread-start request."""
    return {
        f"method": f"thread/start",
        f"params": {
            f"cwd": f"workspace with spaces",
            f"sandbox": f"workspace-write",
            f"approvalPolicy": f"on-request",
        },
    }


class SessionReferenceTests(unittest.TestCase):
    """Check start/cache/reset and both resume outcomes."""

    def test_start_cache_reset(self):
        result = reference(
            {
                f"cwd": f"workspace with spaces",
                f"operations": [
                    {f"session": f"a"},
                    {f"session": f"a"},
                    {f"session": f"a", f"reset": True},
                    {f"session": f"a"},
                ],
            }
        )
        self.assertEqual(
            result,
            {
                f"results": [
                    f"fixture-thread-1",
                    f"fixture-thread-1",
                    None,
                    f"fixture-thread-2",
                ],
                f"requests": [start(), start()],
                f"threads": {f"a": f"fixture-thread-2"},
            },
        )

    def test_resume_and_rejection(self):
        for reject in (False, True):
            with self.subTest(reject=reject):
                result = reference(
                    {
                        f"cwd": f"workspace with spaces",
                        f"threads": {f"a": f"old"},
                        f"reject_resume": reject,
                        f"operations": [{f"session": f"a"}],
                    }
                )
                requests = [
                    {
                        f"method": f"thread/resume",
                        f"params": {f"threadId": f"old"},
                    }
                ]
                if reject:
                    requests.append(start())
                thread = f"fixture-thread-1" if reject else f"old"
                self.assertEqual(
                    result,
                    {
                        f"results": [thread],
                        f"requests": requests,
                        f"threads": {f"a": thread},
                    },
                )


if __name__ == f"__main__":
    unittest.main()
