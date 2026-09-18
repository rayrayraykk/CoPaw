"""Assert original projection and identity on isolated fixture data."""

import hashlib
import json
import subprocess
import sys
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / f"codex_projection_reference.py"
LOCAL = f"mcp_servers.local"
REMOTE = f"mcp_servers.remote"
DEFAULT_APPROVAL = f'default_tools_approval_mode="prompt"'


def digest(payload):
    """Encode an independently constructed expected fingerprint payload."""
    encoded = json.dumps(
        payload, ensure_ascii=True, sort_keys=True, separators=(f",", f":")
    ).encode(f"utf-8")
    return hashlib.sha256(encoded).hexdigest()


def reference(capabilities, refresh=False):
    """Run the reference program without credentials or an external CLI."""
    process = subprocess.run(
        [
            sys.executable,
            str(SCRIPT),
            json.dumps(
                {
                    f"capabilities": capabilities,
                    f"refresh_runtime_revisions": refresh,
                }
            ),
        ],
        capture_output=True,
        text=True,
        check=True,
        timeout=10,
    )
    return json.loads(process.stdout)


def server_payload(revision=f""):
    """Spell out all original identity fields and defaults."""
    return {
        f"name": f"local",
        f"transport": f"stdio",
        f"command": f"tool",
        f"args": [],
        f"cwd": f"",
        f"url": f"",
        f"env_keys": [f"TOKEN"],
        f"header_keys": [],
        f"tools": None,
        f"tool_policies": {},
        f"default_policy": f"ask",
        f"credential_revision": f"",
        f"runtime_revision": revision,
    }


class CodexProjectionReferenceTests(unittest.TestCase):
    """Keep secret-value revisions and emitted configuration explicit."""

    def test_empty_projection_and_fingerprint(self):
        self.assertEqual(
            reference({}),
            {
                f"fingerprint": digest({f"skills": [], f"mcp_servers": []}),
                f"runtime_revisions": [],
                f"projection": {
                    f"config_overrides": [],
                    f"environment": {},
                    f"skill_roots": [],
                },
            },
        )

    def test_stdio_defaults_and_secret_revision_refresh(self):
        for secret in (f"fake-first", f"fake-second"):
            for refresh in (False, True):
                revision = (
                    digest({f"env": {f"TOKEN": secret}, f"headers": {}})
                    if refresh
                    else f""
                )
                self.assertEqual(
                    reference(
                        {
                            f"mcp_servers": [
                                {
                                    f"name": f"local",
                                    f"display_name": f"Local",
                                    f"transport": f"stdio",
                                    f"command": f"tool",
                                    f"env": {f"TOKEN": secret},
                                }
                            ]
                        },
                        refresh,
                    ),
                    {
                        f"fingerprint": digest(
                            {
                                f"skills": [],
                                f"mcp_servers": [server_payload(revision)],
                            }
                        ),
                        f"runtime_revisions": [revision],
                        f"projection": {
                            f"config_overrides": [
                                f'mcp_servers.local.command="tool"',
                                f"mcp_servers.local.args=[]",
                                f'mcp_servers.local.env_vars=["TOKEN"]',
                                f"{LOCAL}.{DEFAULT_APPROVAL}",
                            ],
                            f"environment": {f"TOKEN": secret},
                            f"skill_roots": [],
                        },
                    },
                )

    def test_first_conflict_preserves_fixture_environment_order(self):
        self.assertEqual(
            reference(
                {
                    f"mcp_servers": [
                        {
                            f"name": name,
                            f"display_name": name,
                            f"transport": f"stdio",
                            f"env_entries": [[f"Z", name], [f"A", name]],
                        }
                        for name in (f"first", f"second")
                    ]
                }
            ),
            {
                f"error": f"Codex MCP servers require conflicting values "
                f"for environment variable Z."
            },
        )

    def test_http_header_values_stay_in_environment(self):
        suffix = hashlib.sha256(f"remote:Authorization".encode(f"utf-8"))
        env_name = f"QWENPAW_MCP_REMOTE_AUTHORIZATION_"
        env_name += suffix.hexdigest()[:12].upper()
        payload = server_payload() | {
            f"name": f"remote",
            f"transport": f"streamable_http",
            f"command": f"",
            f"url": f"https://example.invalid/mcp",
            f"env_keys": [],
            f"header_keys": [f"Authorization"],
        }
        self.assertEqual(
            reference(
                {
                    f"mcp_servers": [
                        {
                            f"name": f"remote",
                            f"display_name": f"Remote",
                            f"transport": f"streamable_http",
                            f"url": f"https://example.invalid/mcp",
                            f"headers": {f"Authorization": f"fake-bearer"},
                        }
                    ]
                }
            ),
            {
                f"fingerprint": digest(
                    {f"skills": [], f"mcp_servers": [payload]}
                ),
                f"runtime_revisions": [f""],
                f"projection": {
                    f"config_overrides": [
                        f'{REMOTE}.url="https://example.invalid/mcp"',
                        f"mcp_servers.remote.env_http_headers={{ "
                        f'"Authorization" = "{env_name}" }}',
                        f"{REMOTE}.{DEFAULT_APPROVAL}",
                    ],
                    f"environment": {env_name: f"fake-bearer"},
                    f"skill_roots": [],
                },
            },
        )


if __name__ == f"__main__":
    unittest.main()
