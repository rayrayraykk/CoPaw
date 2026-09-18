"""Test original provider status using fake accounts."""

import json
import subprocess
import sys
import unittest
from pathlib import Path


class HarnessProviderReferenceTests(unittest.TestCase):
    @staticmethod
    def reference(fixture):
        script = (
            Path(__file__).resolve().parents[1]
            / f"harness_provider_reference.py"
        )
        result = subprocess.run(
            [sys.executable, str(script), json.dumps(fixture)],
            check=True,
            capture_output=True,
            text=True,
            timeout=20,
        )
        return json.loads(result.stdout)

    def expected(self, installed=False, account=None, error=None):
        catalog = self.reference({f"operation": f"catalog"})
        return {
            f"id": f"codex",
            f"name": f"Codex",
            f"available": True,
            f"coming_soon": False,
            f"installed": installed,
            f"authenticated": account is not None,
            f"account": account,
            f"runtime_path": (
                str(Path(f"fixture-codex")) if installed else None
            ),
            f"runtime_source": f"configured" if installed else None,
            f"error": error,
            f"capabilities": catalog[0][f"capabilities"],
        }

    def test_catalog_identity_order_and_provider_mcp_difference(self):
        catalog = self.reference({f"operation": f"catalog"})
        self.assertEqual(
            [
                (item[f"id"], item[f"name"], item[f"coming_soon"])
                for item in catalog
            ],
            [
                (f"codex", f"Codex", False),
                (f"claude", f"Claude Code", True),
                (f"qoder", f"Qoder", False),
            ],
        )
        self.assertEqual(
            [
                item[f"capabilities"][f"provider_mcp_discovery"]
                for item in catalog
            ],
            [True, False, False],
        )

    def test_missing_status_has_full_original_defaults(self):
        message = (
            f"Codex runtime not found. Install qwenpaw[codex] or provide a "
            f"standalone Codex CLI."
        )
        self.assertEqual(
            self.reference({f"operation": f"status", f"installed": False}),
            self.expected(error=message),
        )

    def test_authenticated_status_filters_account_without_losing_metadata(
        self,
    ):
        public = {f"type": f"chatgpt", f"email": None, f"planType": f"pro"}
        self.assertEqual(
            self.reference(
                {
                    f"operation": f"status",
                    f"installed": True,
                    f"runtime_path": f"fixture-codex",
                    f"runtime_source": f"configured",
                    f"response": {
                        f"account": {**public, f"privateToken": f"not-public"}
                    },
                }
            ),
            self.expected(installed=True, account=public),
        )

    def test_transport_error_keeps_installed_without_fabricated_login(self):
        self.assertEqual(
            self.reference(
                {
                    f"operation": f"status",
                    f"installed": True,
                    f"runtime_path": f"fixture-codex",
                    f"runtime_source": f"configured",
                    f"error": f"login required",
                }
            ),
            self.expected(installed=True, error=f"login required"),
        )


if __name__ == f"__main__":
    unittest.main()
