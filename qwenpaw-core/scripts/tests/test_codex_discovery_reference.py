"""Test the isolated original discovery reference with fixture files."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


class CodexDiscoveryReferenceTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name).resolve()
        self.home = self.root / f"home with spaces"
        self.home.mkdir()
        self.script = (
            Path(__file__).resolve().parents[1]
            / f"codex_discovery_reference.py"
        )

    def executable(self, relative):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(f"fixture; do not execute", encoding=f"utf-8")
        path.chmod(0o700)
        return path

    def resolve(self, configured=None, bundled=None, environment=None):
        fixture = {
            f"cwd": str(self.root),
            f"home": str(self.home),
            f"environment": environment or {f"PATH": f""},
            f"configured": str(configured) if configured else None,
            f"bundled": str(bundled) if bundled else None,
        }
        result = subprocess.run(
            [sys.executable, str(self.script), json.dumps(fixture)],
            check=True,
            capture_output=True,
            text=True,
            timeout=20,
        )
        return json.loads(result.stdout)

    def test_explicit_and_environment_precede_bundled(self):
        manual = self.executable(f"manual/codex")
        bundled = self.executable(f"sdk/codex")
        self.assertEqual(
            self.resolve(manual, bundled),
            {f"path": str(manual), f"source": f"configured"},
        )
        self.assertEqual(
            self.resolve(
                bundled=bundled,
                environment={f"PATH": f"", f"CODEX_BINARY": str(manual)},
            ),
            {f"path": str(manual), f"source": f"environment"},
        )
        self.assertEqual(
            self.resolve(bundled=bundled),
            {f"path": str(bundled), f"source": f"python-sdk"},
        )

    def test_invalid_manual_and_embedded_do_not_fall_back(self):
        bundled = self.executable(f"sdk/codex")
        embedded = self.executable(f"Tool.app/bin/codex")
        self.assertEqual(self.resolve(f"missing-cli", bundled), None)
        self.assertEqual(self.resolve(embedded, bundled), None)
        self.assertEqual(
            self.resolve(bundled=embedded),
            {f"path": str(embedded), f"source": f"python-sdk"},
        )

    def test_directory_and_non_executable_are_unavailable(self):
        self.assertEqual(self.resolve(self.home), None)
        if os.name != f"nt":
            path = self.executable(f"unavailable/codex")
            path.chmod(0o600)
            self.assertEqual(self.resolve(path), None)

    def test_standalone_and_tilde_resolve_to_full_path(self):
        relative = (
            f"AppData/Local/Programs/OpenAI/Codex/bin/codex.exe"
            if os.name == f"nt"
            else f".local/bin/codex"
        )
        path = self.executable(f"home with spaces/{relative}")
        self.assertEqual(
            self.resolve(),
            {f"path": str(path), f"source": f"standalone"},
        )
        self.assertEqual(
            self.resolve(f"~/{relative}"),
            {f"path": str(path), f"source": f"configured"},
        )


if __name__ == f"__main__":
    unittest.main()
