"""Verify original MCP subprocess mapping using explicit byte fixtures."""

import json
import unittest

from .test_codex_control_reference import reference


def mcp(stdout=f"[]", stderr=f"", returncode=0, binary=f"fixture-codex"):
    """Run no executable; capture the original command and response only."""
    return reference(
        f"mcp",
        [],
        f"workspace with spaces/技能",
        binary=binary,
        stdout=list(stdout.encode(f"utf-8")),
        stderr=list(
            stderr.encode(f"utf-8") if isinstance(stderr, str) else stderr
        ),
        returncode=returncode,
    )


def calls():
    """Represent the exact original subprocess boundary."""
    return [
        {
            f"program": f"fixture-codex",
            f"args": [f"mcp", f"list", f"--json"],
            f"cwd": f"workspace with spaces/技能",
            f"stdout": -1,
            f"stderr": -1,
        }
    ]


class CodexMcpReferenceTests(unittest.TestCase):
    """Compare complete results without real configuration or credentials."""

    def test_missing_installation_never_creates_a_process(self):
        self.assertEqual(mcp(binary=None), {f"result": [], f"requests": []})

    def test_non_array_json_is_empty_but_still_runs_the_command(self):
        self.assertEqual(mcp(f"{{}}"), {f"result": [], f"requests": calls()})

    def test_full_metadata_filters_configuration_and_preserves_duplicates(
        self,
    ):
        self.assertEqual(
            mcp(
                json.dumps(
                    [
                        {
                            f"name": f"one",
                            f"transport": {
                                f"type": f"stdio",
                                f"command": f"private",
                            },
                            f"enabled": True,
                        },
                        {f"name": f"one"},
                    ]
                )
            ),
            {
                f"result": [
                    {
                        f"name": f"one",
                        f"provider_id": f"codex",
                        f"transport": transport,
                        f"enabled": enabled,
                        f"auth_status": f"",
                        f"read_only": True,
                        f"scope": f"provider",
                    }
                    for transport, enabled in ((f"stdio", True), (f"", False))
                ],
                f"requests": calls(),
            },
        )

    def test_invalid_json_and_nonzero_exit_preserve_original_messages(self):
        self.assertEqual(
            mcp(f"invalid"),
            {
                f"error": f"Codex returned invalid MCP discovery data",
                f"requests": calls(),
            },
        )
        self.assertEqual(
            mcp(
                f"ignored",
                bytes().join(
                    (
                        f" denied ".encode(f"utf-8"),
                        bytes([255]),
                        f" \n".encode(f"utf-8"),
                    )
                ),
                7,
            ),
            {
                f"error": f"Failed to discover Codex MCP servers: denied �",
                f"requests": calls(),
            },
        )


if __name__ == f"__main__":
    unittest.main()
