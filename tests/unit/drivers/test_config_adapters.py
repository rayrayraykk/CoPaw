# -*- coding: utf-8 -*-
from qwenpaw.config.config import (
    ACPAgentConfig,
    ACPConfig,
)
from qwenpaw.drivers.adapters.acp_config import (
    acp_agent_config_to_card,
    acp_config_to_cards,
)
from qwenpaw.drivers.adapters.channel_credentials import (
    extract_channel_credentials,
)


def test_acp_config_adapter_skeleton() -> None:
    config = ACPAgentConfig(
        enabled=True,
        command="qwen",
        args=["--acp"],
        env={"A": "B"},
        trusted=False,
        tool_parse_mode="call_detail",
    )

    card = acp_agent_config_to_card("qwen_code", config)

    assert card.name == "acp-qwen_code"
    assert card.protocol == "acp"
    assert card.endpoint["command"] == "qwen"
    assert card.config["trusted"] is False


def test_acp_config_to_cards() -> None:
    cards = acp_config_to_cards(
        ACPConfig(
            agents={
                "demo": ACPAgentConfig(
                    enabled=True,
                    command="demo",
                    args=[],
                ),
            },
        ),
    )

    assert any(card.name == "acp-demo" for card in cards)


def test_channel_credential_extraction_is_read_only() -> None:
    extracted = extract_channel_credentials(
        "telegram",
        {"bot_token": "token"},
    )

    assert extracted == [
        ("channel-telegram", "static", {"bot_token": "token"}),
    ]
