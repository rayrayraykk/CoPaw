# -*- coding: utf-8 -*-
from __future__ import annotations

from types import SimpleNamespace

from fastapi import HTTPException
import pytest

from qwenpaw.app.routers.mcp import (
    MCPAccessRule,
    MCPAccessPolicy,
    MCPToolDefaultPolicy,
    MCPToolAccessOverride,
    _ensure_mcp_driver_active,
    _ensure_mcp_display_name_unique,
    get_mcp_policy,
    update_mcp_policy,
)
from qwenpaw.drivers.capabilities import DriverRuntimeInfo
from qwenpaw.drivers.contracts import (
    DriverCard,
    DriverPolicy,
    PolicyPrincipal,
    PolicyRule,
    PolicyTarget,
)
from qwenpaw.drivers.storage import card_path, dump_card, load_card


class FakeDriverManager:
    def __init__(self, infos: list[DriverRuntimeInfo]) -> None:
        self.infos = infos

    async def list_drivers(
        self,
        protocol: str | None = None,
    ) -> list[DriverRuntimeInfo]:
        assert protocol == "mcp"
        return self.infos


@pytest.mark.asyncio
async def test_ensure_mcp_driver_active_allows_active_driver() -> None:
    manager = FakeDriverManager(
        [
            DriverRuntimeInfo(
                name="demo",
                protocol="mcp",
                enabled=True,
                status="active",
            ),
        ],
    )

    await _ensure_mcp_driver_active(manager, "demo")


@pytest.mark.asyncio
async def test_ensure_mcp_driver_active_rejects_inactive_driver() -> None:
    manager = FakeDriverManager(
        [
            DriverRuntimeInfo(
                name="demo",
                protocol="mcp",
                enabled=True,
                status="inactive",
            ),
        ],
    )

    with pytest.raises(HTTPException) as exc_info:
        await _ensure_mcp_driver_active(manager, "demo")

    assert exc_info.value.status_code == 503
    assert "status=inactive" in str(exc_info.value.detail)


async def _fake_agent_context(agent):
    return agent


def test_mcp_display_name_must_be_unique(tmp_path) -> None:
    agent = SimpleNamespace(workspace_dir=tmp_path)
    dump_card(
        DriverCard(
            name="aone-code-mcp",
            protocol="mcp",
            endpoint={"transport": "stdio", "command": "demo"},
            config={"display_name": "aone-code-platform"},
        ),
        card_path(tmp_path / "drivers", "aone-code-mcp", protocol="mcp"),
    )

    with pytest.raises(HTTPException) as duplicate_display:
        _ensure_mcp_display_name_unique(
            agent,
            "AONE-CODE-PLATFORM",
            client_key="other-mcp",
        )
    assert duplicate_display.value.status_code == 400
    assert "already exists" in str(duplicate_display.value.detail)

    with pytest.raises(HTTPException) as duplicate_key:
        _ensure_mcp_display_name_unique(
            agent,
            "aone-code-mcp",
            client_key="other-mcp",
        )
    assert duplicate_key.value.status_code == 400
    assert "conflicts with existing MCP client key" in str(
        duplicate_key.value.detail,
    )

    _ensure_mcp_display_name_unique(
        agent,
        "aone-code-platform",
        client_key="aone-code-mcp",
    )


@pytest.mark.asyncio
async def test_get_mcp_policy_reads_saved_policy_without_driver_manager(
    tmp_path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    agent = SimpleNamespace(workspace_dir=tmp_path)
    dump_card(
        DriverCard(
            name="demo",
            protocol="mcp",
            endpoint={"transport": "stdio", "command": "demo"},
            policy=DriverPolicy(
                default_effect="ask",
                rules=[
                    PolicyRule(
                        subject="*",
                        effect="allow",
                        target=PolicyTarget(kind="tool", name="*"),
                        principal=PolicyPrincipal(
                            source_type="channel",
                            source_value="console",
                            subject_type="all",
                            subject_value="",
                        ),
                    ),
                    PolicyRule(
                        subject="*",
                        effect="deny",
                        target=PolicyTarget(kind="tool", name="search"),
                        principal=PolicyPrincipal(),
                    ),
                    PolicyRule(
                        subject="*",
                        effect="allow",
                        target=PolicyTarget(kind="tool", name="echo"),
                        principal=PolicyPrincipal(
                            source_type="channel",
                            source_value="console",
                            subject_type="all",
                            subject_value="",
                        ),
                    ),
                    PolicyRule(
                        subject="*",
                        effect="deny",
                        target=PolicyTarget(kind="tool", name="danger"),
                        principal=PolicyPrincipal(
                            source_type="channel",
                            source_value="dingtalk",
                            subject_type="user",
                            subject_value="alice",
                        ),
                    ),
                ],
            ),
        ),
        card_path(tmp_path / "drivers", "demo", protocol="mcp"),
    )
    monkeypatch.setattr(
        "qwenpaw.app.agent_context.get_agent_for_request",
        _fake_agent_context,
    )

    policy = await get_mcp_policy(agent, "demo")

    assert policy.default_effect == "ask"
    assert policy.client_overrides == [
        MCPAccessRule(
            source_type="channel",
            source_value="console",
            subject_type="all",
            subject_value="",
            effect="allow",
        ),
    ]
    assert policy.tool_defaults == [
        MCPToolDefaultPolicy(tool_name="search", effect="deny"),
    ]
    assert policy.tool_overrides == [
        MCPToolAccessOverride(
            tool_name="echo",
            source_type="channel",
            source_value="console",
            subject_type="all",
            subject_value="",
            effect="allow",
        ),
        MCPToolAccessOverride(
            tool_name="danger",
            source_type="channel",
            source_value="dingtalk",
            subject_type="user",
            subject_value="alice",
            effect="deny",
        ),
    ]
    assert policy.unmanaged_rules_count == 0


@pytest.mark.asyncio
async def test_get_mcp_policy_maps_legacy_wildcard_to_client_override(
    tmp_path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    agent = SimpleNamespace(workspace_dir=tmp_path)
    dump_card(
        DriverCard(
            name="demo",
            protocol="mcp",
            endpoint={"transport": "stdio", "command": "demo"},
            policy=DriverPolicy(
                default_effect="deny",
                rules=[PolicyRule(subject="*", effect="allow")],
            ),
        ),
        card_path(tmp_path / "drivers", "demo", protocol="mcp"),
    )
    monkeypatch.setattr(
        "qwenpaw.app.agent_context.get_agent_for_request",
        _fake_agent_context,
    )

    policy = await get_mcp_policy(agent, "demo")

    assert policy.client_overrides == [
        MCPAccessRule(
            source_type="channel",
            source_value="console",
            subject_type="all",
            subject_value="",
            effect="allow",
        ),
    ]
    assert policy.unmanaged_rules_count == 0


@pytest.mark.asyncio
async def test_update_mcp_policy_replaces_rules_and_preserves_unmanaged(
    tmp_path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    agent = SimpleNamespace(workspace_dir=tmp_path)
    dump_card(
        DriverCard(
            name="demo",
            protocol="mcp",
            endpoint={"transport": "stdio", "command": "demo"},
            policy=DriverPolicy(
                default_effect="ask",
                rules=[
                    PolicyRule(
                        subject="*",
                        effect="allow",
                        target=PolicyTarget(kind="tool", name="old"),
                        principal=PolicyPrincipal(
                            source_type="channel",
                            source_value="console",
                            subject_type="all",
                            subject_value="",
                        ),
                    ),
                    PolicyRule(
                        subject="user:*",
                        effect="deny",
                        target=PolicyTarget(kind="resource", name="danger"),
                    ),
                ],
            ),
        ),
        card_path(tmp_path / "drivers", "demo", protocol="mcp"),
    )
    monkeypatch.setattr(
        "qwenpaw.app.agent_context.get_agent_for_request",
        _fake_agent_context,
    )

    updated = await update_mcp_policy(
        agent,
        "demo",
        MCPAccessPolicy(
            default_effect="allow",
            client_overrides=[
                MCPAccessRule(
                    source_type="channel",
                    source_value="console",
                    subject_type="all",
                    subject_value="",
                    effect="allow",
                ),
            ],
            tool_defaults=[
                MCPToolDefaultPolicy(tool_name="search", effect="deny"),
            ],
            tool_overrides=[
                MCPToolAccessOverride(
                    tool_name="echo",
                    source_type="app",
                    source_value="Creator",
                    subject_type="all",
                    subject_value="",
                    effect="ask",
                ),
            ],
        ),
    )
    saved = load_card(card_path(tmp_path / "drivers", "demo", protocol="mcp"))

    assert updated.default_effect == "allow"
    assert not (tmp_path / "drivers" / "demo.yaml").exists()
    assert updated.client_overrides == [
        MCPAccessRule(
            source_type="channel",
            source_value="console",
            subject_type="all",
            subject_value="",
            effect="allow",
        ),
    ]
    assert updated.tool_defaults == [
        MCPToolDefaultPolicy(tool_name="search", effect="deny"),
    ]
    assert updated.tool_overrides == [
        MCPToolAccessOverride(
            tool_name="echo",
            source_type="app",
            source_value="Creator",
            subject_type="all",
            subject_value="",
            effect="ask",
        ),
    ]
    assert updated.unmanaged_rules_count == 1
    assert [
        (
            rule.subject,
            rule.target.kind,
            rule.target.name,
            rule.principal.source_type,
            rule.principal.source_value,
            rule.principal.subject_type,
            rule.principal.subject_value,
        )
        for rule in saved.policy.rules
    ] == [
        ("user:*", "resource", "danger", "*", "*", "*", "*"),
        ("*", "tool", "search", "*", "*", "*", "*"),
        ("*", "tool", "*", "channel", "console", "all", ""),
        ("*", "tool", "echo", "app", "Creator", "all", ""),
    ]
