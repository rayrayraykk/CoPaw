# -*- coding: utf-8 -*-
"""Adapters from ACP config to DriverCard."""

from __future__ import annotations

from typing import TYPE_CHECKING

from qwenpaw.drivers.contracts import CredentialRef, DriverCard

if TYPE_CHECKING:
    from qwenpaw.config.config import ACPAgentConfig, ACPConfig


def acp_agent_config_to_card(
    name: str,
    config: "ACPAgentConfig",
) -> DriverCard:
    """Map one ACPAgentConfig into an ACP DriverCard skeleton."""
    return DriverCard(
        name=f"acp-{name}",
        protocol="acp",
        endpoint={
            "transport": "stdio",
            "command": config.command,
            "args": list(config.args),
            "env": dict(config.env),
        },
        credential=CredentialRef(kind="none"),
        config={
            "trusted": config.trusted,
            "tool_parse_mode": config.tool_parse_mode,
            "stdio_buffer_limit_bytes": config.stdio_buffer_limit_bytes,
        },
        enabled=config.enabled,
    )


def acp_config_to_cards(config: "ACPConfig") -> list[DriverCard]:
    """Map all ACP agents into DriverCards without persistence."""
    return [
        acp_agent_config_to_card(name, agent_config)
        for name, agent_config in config.agents.items()
    ]
