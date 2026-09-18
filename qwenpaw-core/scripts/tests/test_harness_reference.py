"""Test original-handler reference records without external runtimes."""

import json
import subprocess
import sys
import unittest
from pathlib import Path

REFERENCE = Path(__file__).resolve().parents[1] / f"harness_reference.py"
SAVED = {f"binary": f"runtime with spaces", f"model": f"saved-model"}
WORKSPACE = {f"method": f"workspace", f"agent": f"default"}


def adapter(provider=f"codex", settings=None):
    """Return the complete expected adapter call."""
    return {
        f"method": f"adapter",
        f"provider": provider,
        f"settings": SAVED if settings is None else settings,
    }


def request(method, suffix, body=None, agent=f"default"):
    """Build an explicit HTTP request with no real network transport."""
    result = {
        f"method": method,
        f"path": f"/api/harnesses{suffix}",
        f"agent": agent,
    }
    if body is not None:
        result[f"body"] = body
    return result


def run_reference(requests, **values):
    """Run every reference in its own interpreter to isolate module stubs."""
    fixture = {
        f"workspaces": {
            f"default": {
                f"backend": f"codex",
                f"backend_settings": SAVED,
                **values,
            },
            f"writer": {
                f"backend": f"qoder",
                f"backend_settings": {f"binary": f"writer-runtime"},
                **values,
            },
        },
        f"requests": requests,
    }
    result = subprocess.run(
        [sys.executable, str(REFERENCE)],
        input=json.dumps(fixture),
        text=True,
        capture_output=True,
        check=True,
        timeout=20,
    )
    return json.loads(result.stdout)[f"responses"]


class HarnessReferenceTests(unittest.TestCase):
    """Assert complete handler responses and dependency call order."""

    def test_status_overrides_adapter_capabilities_and_keeps_live_fields(self):
        enabled = (
            f"authentication model_selection reasoning_effort "
            f"reasoning_stream tool_stream session_resume attachments "
            f"qwenpaw_skills_projection "
            f"qwenpaw_mcp_projection provider_skills_discovery "
            f"provider_mcp_discovery mcp_tool_allowlist"
        ).split()
        disabled = (
            f"workspace_ui native_skills_ui native_tools_ui native_mcp_ui "
            f"loop_modes context_usage skills_commands"
        ).split()
        capabilities = {key: True for key in enabled}
        capabilities.update({key: False for key in disabled})
        capabilities[f"commands"] = [
            {
                f"name": name,
                f"description": description,
                f"accepts_arguments": False,
            }
            for name, description in (
                (f"compact", f"Compact the current Codex thread"),
                (f"review", f"Review uncommitted workspace changes"),
                (f"skills", f"List skills available to Codex"),
                (f"status", f"Show Codex account and session status"),
            )
        ]
        capabilities[f"approval_presets"] = [
            {
                f"id": preset,
                f"name": name,
                f"description": description,
                f"settings": {f"sandbox": sandbox, f"approval_policy": policy},
            }
            for preset, name, description, sandbox, policy in (
                (
                    f"ask",
                    f"Ask before changes",
                    f"Allow workspace changes and ask before "
                    f"elevated actions.",
                    f"workspace-write",
                    f"on-request",
                ),
                (
                    f"read-only",
                    f"Read only",
                    f"Inspect files without changing them.",
                    f"read-only",
                    f"on-request",
                ),
                (
                    f"workspace",
                    f"Workspace access",
                    f"Allow workspace changes without confirmation.",
                    f"workspace-write",
                    f"never",
                ),
                (
                    f"full-access",
                    f"Full access",
                    f"Allow unrestricted local execution "
                    f"without confirmation.",
                    f"danger-full-access",
                    f"never",
                ),
            )
        ]
        status = {
            f"id": f"codex",
            f"name": f"Codex",
            f"available": True,
            f"installed": False,
            f"error": f"runtime missing",
            f"capabilities": {f"workspace_ui": True},
        }
        unsaved = {f"binary": f"unsaved runtime"}
        self.assertEqual(
            run_reference(
                [
                    request(f"POST", f"/codex/status", {f"settings": unsaved}),
                    request(f"POST", f"/codex/status", {}),
                ],
                status=status,
            ),
            [
                {
                    f"status": 200,
                    f"body": {
                        **status,
                        f"capabilities": capabilities,
                        f"coming_soon": False,
                        f"authenticated": False,
                        f"account": None,
                        f"runtime_path": None,
                        f"runtime_source": None,
                    },
                    f"calls": [
                        WORKSPACE,
                        adapter(settings=settings),
                        {
                            f"method": f"status",
                        },
                    ],
                }
                for settings in (unsaved, {})
            ],
        )

    def test_unknown_and_planned_providers_precede_workspace_lookup(self):
        requests = []
        expected = []
        for provider, status, message in (
            (f"unknown", 404, f"Unknown third-party agent backend: unknown"),
            (f"claude", 409, f"Claude Code is not available yet"),
        ):
            for suffix in (f"models", f"mcp", f"skills"):
                requests.append(
                    request(
                        f"GET",
                        f"/{provider}/{suffix}",
                        agent=f"missing",
                    )
                )
                expected.append(
                    {
                        f"status": status,
                        f"body": {f"detail": message},
                        f"calls": [],
                    }
                )
            for suffix in (f"status", f"login", f"logout"):
                requests.append(
                    request(
                        f"POST",
                        f"/{provider}/{suffix}",
                        {},
                        f"missing",
                    )
                )
                expected.append(expected[-1].copy())
        self.assertEqual(run_reference(requests), expected)

    def test_models_preserve_defaults_settings_and_agent_selection(self):
        expected_body = {
            f"models": [
                {
                    f"id": f"model-a",
                    f"name": f"Model A",
                    f"description": f"",
                    f"is_default": False,
                    f"reasoning_efforts": [],
                    f"default_reasoning_effort": None,
                }
            ]
        }
        requests = [
            request(f"GET", f"/codex/models"),
            request(f"GET", f"/qoder/models"),
            request(f"GET", f"/qoder/models", agent=f"writer"),
        ]
        self.assertEqual(
            run_reference(
                requests,
                models=[{f"id": f"model-a", f"name": f"Model A"}],
            ),
            [
                {
                    f"status": 200,
                    f"body": expected_body,
                    f"calls": [
                        WORKSPACE,
                        adapter(),
                        {f"method": f"models"},
                    ],
                },
                {
                    f"status": 200,
                    f"body": expected_body,
                    f"calls": [
                        WORKSPACE,
                        adapter(f"qoder", {}),
                        {f"method": f"models"},
                    ],
                },
                {
                    f"status": 200,
                    f"body": expected_body,
                    f"calls": [
                        {f"method": f"workspace", f"agent": f"writer"},
                        adapter(f"qoder", {f"binary": f"writer-runtime"}),
                        {f"method": f"models"},
                    ],
                },
            ],
        )

    def test_unavailable_capabilities_do_not_execute_discovery(self):
        requests = []
        expected = []
        for suffix, collection in (
            (f"models", f"models"),
            (f"mcp", f"servers"),
            (f"skills", f"skills"),
        ):
            requests.append(request(f"GET", f"/codex/{suffix}"))
            expected.append(
                {
                    f"status": 200,
                    f"body": {collection: [], f"message": f"runtime missing"},
                    f"calls": [WORKSPACE, adapter()],
                }
            )
        requests.append(request(f"GET", f"/qoder/mcp", agent=f"missing"))
        expected.append(
            {
                f"status": 200,
                f"body": {f"servers": []},
                f"calls": [],
            }
        )
        self.assertEqual(
            run_reference(requests, unavailable=f"runtime missing"),
            expected,
        )

    def test_discovery_preserves_full_defaults_and_resolved_workspace(self):
        self.assertEqual(
            run_reference(
                [
                    request(f"GET", f"/codex/mcp"),
                    request(f"GET", f"/qoder/skills", agent=f"writer"),
                ],
                servers=[
                    {
                        f"name": f"docs",
                        f"provider_id": f"codex",
                        f"transport": f"stdio",
                        f"enabled": True,
                    }
                ],
                skills=[{f"name": f"review", f"provider_id": f"qoder"}],
            ),
            [
                {
                    f"status": 200,
                    f"body": {
                        f"servers": [
                            {
                                f"name": f"docs",
                                f"provider_id": f"codex",
                                f"transport": f"stdio",
                                f"enabled": True,
                                f"auth_status": f"",
                                f"read_only": True,
                                f"scope": f"provider",
                            }
                        ]
                    },
                    f"calls": [
                        WORKSPACE,
                        adapter(),
                        {
                            f"method": f"discover_mcp",
                            f"cwd": f"$WORKSPACE/default",
                        },
                    ],
                },
                {
                    f"status": 200,
                    f"body": {
                        f"skills": [
                            {
                                f"name": f"review",
                                f"provider_id": f"qoder",
                                f"description": f"",
                                f"source": f"",
                                f"enabled": True,
                                f"read_only": True,
                                f"scope": f"provider",
                            }
                        ]
                    },
                    f"calls": [
                        {f"method": f"workspace", f"agent": f"writer"},
                        adapter(f"qoder", {f"binary": f"writer-runtime"}),
                        {
                            f"method": f"discover_skills",
                            f"cwd": f"$WORKSPACE/writer",
                        },
                    ],
                },
            ],
        )

    def test_login_and_logout_use_unsaved_settings_and_exact_errors(self):
        unsaved = {f"binary": f"different runtime", f"extra": 1}
        login = {
            f"type": f"deviceCode",
            f"loginId": f"test-login",
            f"verificationUrl": f"https://example.invalid/test",
            f"userCode": f"TEST-ONLY",
        }
        requests = [
            request(
                f"POST",
                f"/codex/login",
                {
                    f"device_code": True,
                    f"settings": unsaved,
                },
            ),
            request(f"POST", f"/codex/login", {}),
            request(f"POST", f"/codex/logout", {f"settings": unsaved}),
        ]
        self.assertEqual(
            run_reference(requests, login=login),
            [
                {
                    f"status": 200,
                    f"body": login,
                    f"calls": [
                        WORKSPACE,
                        adapter(settings=unsaved),
                        {f"method": f"start_login", f"device_code": True},
                    ],
                },
                {
                    f"status": 200,
                    f"body": login,
                    f"calls": [
                        WORKSPACE,
                        adapter(settings={}),
                        {f"method": f"start_login", f"device_code": False},
                    ],
                },
                {
                    f"status": 200,
                    f"body": {f"ok": True},
                    f"calls": [
                        WORKSPACE,
                        adapter(settings=unsaved),
                        {f"method": f"logout"},
                    ],
                },
            ],
        )
        message = (
            f"Qoder CLI does not expose a non-interactive logout command."
        )
        self.assertEqual(
            run_reference(
                [
                    request(f"POST", f"/qoder/logout", {}),
                ],
                logout_not_supported=message,
            ),
            [
                {
                    f"status": 409,
                    f"body": {
                        f"detail": {
                            f"code": f"logout_not_supported",
                            f"message": message,
                        }
                    },
                    f"calls": [
                        WORKSPACE,
                        adapter(f"qoder", {}),
                        {
                            f"method": f"logout",
                        },
                    ],
                }
            ],
        )

    def test_list_forwards_only_active_backend_saved_settings(self):
        self.assertEqual(
            run_reference(
                [
                    request(f"GET", f""),
                    request(f"GET", f"", agent=f"writer"),
                ]
            ),
            [
                {
                    f"status": 200,
                    f"body": {f"providers": []},
                    f"calls": [
                        WORKSPACE,
                        {
                            f"method": f"providers",
                            f"settings": {
                                f"codex": SAVED,
                            },
                        },
                    ],
                },
                {
                    f"status": 200,
                    f"body": {f"providers": []},
                    f"calls": [
                        {f"method": f"workspace", f"agent": f"writer"},
                        {
                            f"method": f"providers",
                            f"settings": {
                                f"qoder": {f"binary": f"writer-runtime"},
                            },
                        },
                    ],
                },
            ],
        )


if __name__ == f"__main__":
    unittest.main()
