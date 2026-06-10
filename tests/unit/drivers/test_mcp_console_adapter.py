# -*- coding: utf-8 -*-
from dataclasses import dataclass, field
from typing import Literal

from qwenpaw.drivers.adapters.mcp_console import (
    attach_mcp_oauth_credential,
    build_mcp_client_info_payload,
    build_mcp_credential_record,
    build_mcp_driver_card,
    detach_mcp_oauth_credential,
    mcp_credential_ref,
    normalize_secret_key,
)
from qwenpaw.drivers.credentials.types import CredentialRecord


@dataclass
class ClientDTO:
    name: str = "demo"
    description: str = ""
    enabled: bool = True
    transport: Literal["stdio", "streamable_http", "sse"] = "stdio"
    url: str = ""
    headers: dict[str, str] = field(default_factory=dict)
    command: str = "python"
    args: list[str] = field(default_factory=list)
    env: dict[str, str] = field(default_factory=dict)
    cwd: str = ""

    def model_dump(self, exclude_unset: bool = False):
        del exclude_unset
        return dict(vars(self))


def test_build_stdio_card_and_credential_split_secret_refs() -> None:
    client = ClientDTO(env={"TAVILY_API_KEY": "plain-token-value"})

    record = build_mcp_credential_record("tavily", client)
    card = build_mcp_driver_card(
        "tavily",
        client,
        mcp_credential_ref("tavily"),
        credential_record=record,
    )

    assert record.ref == "mcp/tavily"
    assert record.kind == "static"
    assert record.secrets == {"TAVILY_API_KEY": "plain-token-value"}
    assert card.credential.ref == "mcp/tavily"
    assert card.credentials["static"].ref == "mcp/tavily"
    assert card.endpoint["env"] == {
        "TAVILY_API_KEY": {
            "source": "credential",
            "credential": "static",
            "field": "TAVILY_API_KEY",
        },
    }
    assert card.policy.default_effect == "ask"
    assert "plain-token-value" not in str(card.endpoint)


def test_build_http_card_normalizes_header_secret_keys() -> None:
    client = ClientDTO(
        transport="streamable_http",
        url="https://mcp.example.test",
        headers={"Authorization": "Bearer token", "X-API-Key": "key"},
    )

    record = build_mcp_credential_record("remote", client)
    card = build_mcp_driver_card(
        "remote",
        client,
        mcp_credential_ref("remote"),
        credential_record=record,
    )

    assert record.secrets == {
        "authorization": "Bearer token",
        "x_api_key": "key",
    }
    assert card.endpoint["headers"] == {
        "Authorization": {
            "source": "credential",
            "credential": "static",
            "field": "authorization",
        },
        "X-API-Key": {
            "source": "credential",
            "credential": "static",
            "field": "x_api_key",
        },
    }


def test_build_client_info_masks_secret_values() -> None:
    client = ClientDTO(env={"TOKEN": "sk-secret-value"})
    record = build_mcp_credential_record("demo", client)
    card = build_mcp_driver_card(
        "demo",
        client,
        mcp_credential_ref("demo"),
        credential_record=record,
    )

    payload = build_mcp_client_info_payload(card, record)

    assert payload["env"]["TOKEN"] == "sk-********alue"
    assert payload["access_summary"] == {
        "default_effect": "ask",
        "overrides_count": 0,
    }
    assert "sk-secret-value" not in str(payload)


def test_oauth_attach_and_detach_preserve_static_headers() -> None:
    client = ClientDTO(
        transport="streamable_http",
        url="https://mcp.example.test",
        headers={"X-API-Key": "secret-key"},
    )
    record = build_mcp_credential_record("remote", client)
    card = build_mcp_driver_card(
        "remote",
        client,
        mcp_credential_ref("remote"),
        credential_record=record,
    )

    authorized = attach_mcp_oauth_credential(card, "mcp/remote/oauth")

    assert set(authorized.credentials) == {"static", "oauth"}
    assert authorized.endpoint["headers"]["X-API-Key"] == {
        "source": "credential",
        "credential": "static",
        "field": "x_api_key",
    }
    assert authorized.endpoint["headers"]["Authorization"] == {
        "source": "credential",
        "credential": "oauth",
        "field": "access_token",
        "format": "Bearer {value}",
    }

    revoked = detach_mcp_oauth_credential(authorized)

    assert set(revoked.credentials) == {"static"}
    assert "Authorization" not in revoked.endpoint["headers"]
    assert revoked.endpoint["headers"]["X-API-Key"]["credential"] == "static"


def test_masked_update_restores_existing_secret() -> None:
    existing = CredentialRecord(
        ref="mcp/demo",
        kind="static",
        secrets={"TOKEN": "sk-secret-value"},
    )
    client = ClientDTO(env={"TOKEN": "sk-********alue"})

    record = build_mcp_credential_record("demo", client, existing=existing)

    assert record.secrets["TOKEN"] == "sk-secret-value"


def test_normalize_secret_key_handles_header_names() -> None:
    assert normalize_secret_key("X-API-Key") == "x_api_key"
    assert normalize_secret_key("Authorization", {"authorization"}) == (
        "authorization_2"
    )
