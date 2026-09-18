"""Assert complete original Codex control-reference results and requests."""

import json
import subprocess
import sys
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / f"codex_control_reference.py"


def reference(operation, responses, cwd=None, **fixture):
    """Run the isolated reference with fake protocol data only."""
    result = subprocess.run(
        [
            sys.executable,
            str(SCRIPT),
            json.dumps(
                {
                    f"operation": operation,
                    f"responses": responses,
                    f"cwd": cwd,
                    **fixture,
                }
            ),
        ],
        capture_output=True,
        text=True,
        check=True,
        timeout=10,
    )
    return json.loads(result.stdout)


class CodexControlReferenceTests(unittest.TestCase):
    """Keep reference projection and RPC capture explicit."""

    def test_status_filters_private_fields_and_preserves_null(self):
        self.assertEqual(
            reference(
                f"status",
                [
                    {
                        f"account": {
                            f"type": f"chatgpt",
                            f"email": None,
                            f"planType": f"plus",
                            f"secret": f"not-public",
                        }
                    }
                ],
            ),
            {
                f"result": {
                    f"authenticated": True,
                    f"account": {
                        f"type": f"chatgpt",
                        f"email": None,
                        f"planType": f"plus",
                    },
                },
                f"requests": [
                    {
                        f"method": f"account/read",
                        f"params": {f"refreshToken": False},
                    }
                ],
            },
        )

    def test_models_walk_all_pages_and_fill_original_defaults(self):
        self.assertEqual(
            reference(
                f"models",
                [
                    {f"data": [], f"nextCursor": f"next"},
                    {f"data": [{f"id": f"one"}, {}]},
                ],
            ),
            {
                f"result": [
                    {
                        f"id": f"one",
                        f"name": f"one",
                        f"description": f"",
                        f"is_default": False,
                        f"reasoning_efforts": [],
                        f"default_reasoning_effort": None,
                    }
                ],
                f"requests": [
                    {
                        f"method": f"model/list",
                        f"params": {
                            f"cursor": cursor,
                            f"includeHidden": False,
                        },
                    }
                    for cursor in (None, f"next")
                ],
            },
        )

    def test_browser_and_device_parameters_and_login_results(self):
        for operation, params in (
            (
                f"browser",
                {
                    f"type": f"chatgpt",
                    f"useHostedLoginSuccessPage": True,
                    f"appBrand": f"codex",
                },
            ),
            (f"device", {f"type": f"chatgptDeviceCode"}),
        ):
            self.assertEqual(
                reference(operation, [None]),
                {
                    f"result": {},
                    f"requests": [
                        {f"method": f"account/login/start", f"params": params}
                    ],
                },
            )

    def test_logout_waits_for_provider_request(self):
        self.assertEqual(
            reference(f"logout", [{}]),
            {
                f"result": None,
                f"requests": [{f"method": f"account/logout", f"params": {}}],
            },
        )

    def test_skills_keep_first_source_entry_and_only_public_fields(self):
        cwd = f"workspace with spaces/技能"
        self.assertEqual(
            reference(
                f"skills",
                [
                    {
                        f"data": [
                            {
                                f"skills": [
                                    {
                                        f"name": f"one",
                                        f"scope": f"user",
                                        f"enabled": False,
                                        f"path": f"private",
                                    },
                                    {f"name": f"one", f"scope": f"user"},
                                    {f"name": f"one", f"scope": f"repo"},
                                    None,
                                    {},
                                ]
                            }
                        ]
                    }
                ],
                cwd,
            ),
            {
                f"result": [
                    {
                        f"name": f"one",
                        f"description": f"",
                        f"provider_id": f"codex",
                        f"source": source,
                        f"enabled": enabled,
                        f"read_only": True,
                        f"scope": f"provider",
                    }
                    for source, enabled in ((f"user", False), (f"repo", True))
                ],
                f"requests": [
                    {
                        f"method": f"skills/list",
                        f"params": {f"cwds": [cwd], f"forceReload": False},
                    }
                ],
            },
        )


if __name__ == f"__main__":
    unittest.main()
