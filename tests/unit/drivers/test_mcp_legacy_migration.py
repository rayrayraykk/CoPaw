# -*- coding: utf-8 -*-
from pathlib import Path
from types import SimpleNamespace

import pytest

from qwenpaw.drivers.adapters.mcp_legacy_config import (
    legacy_mcp_client_to_driver,
    migrate_legacy_mcp_if_needed,
)
from qwenpaw.drivers.credentials.store import CredentialStore
from qwenpaw.drivers.manager import DriverManager
from qwenpaw.drivers.storage import card_path, dump_card, load_card


def _config(**kwargs):
    defaults = {
        "name": "demo",
        "description": "",
        "enabled": True,
        "transport": "stdio",
        "url": "",
        "headers": {},
        "command": "python",
        "args": [],
        "env": {},
        "cwd": "",
        "oauth": None,
    }
    defaults.update(kwargs)
    return SimpleNamespace(**defaults)


def test_legacy_stdio_secret_env_to_driver_card_and_credential() -> None:
    card, credential = legacy_mcp_client_to_driver(
        "tavily_search",
        _config(env={"TAVILY_API_KEY": "tvly-xxx"}),
    )

    assert card.endpoint["env"] == {
        "public": {},
        "secret_refs": {"TAVILY_API_KEY": "TAVILY_API_KEY"},
    }
    assert card.credential.ref == "mcp/tavily_search"
    assert credential is not None
    assert credential.secrets["TAVILY_API_KEY"] == "tvly-xxx"
    assert "tvly-xxx" not in str(card)


def test_legacy_stdio_public_and_secret_env_split() -> None:
    card, credential = legacy_mcp_client_to_driver(
        "echo",
        _config(
            env={
                "NODE_ENV": "production",
                "LOG_LEVEL": "info",
                "ECHO_TOKEN": "secret-token",
            },
        ),
    )

    assert card.endpoint["env"]["public"] == {
        "NODE_ENV": "production",
        "LOG_LEVEL": "info",
    }
    assert card.endpoint["env"]["secret_refs"] == {"ECHO_TOKEN": "ECHO_TOKEN"}
    assert credential is not None
    assert credential.secrets["ECHO_TOKEN"] == "secret-token"


def test_legacy_http_headers_split_public_and_secret() -> None:
    card, credential = legacy_mcp_client_to_driver(
        "remote_docs",
        _config(
            transport="streamable_http",
            url="https://docs.example.com/mcp",
            headers={
                "Content-Type": "application/json",
                "X-Client-Name": "qwenpaw",
                "Authorization": "Bearer secret-token",
            },
        ),
    )

    assert card.endpoint["headers"]["public"] == {
        "Content-Type": "application/json",
        "X-Client-Name": "qwenpaw",
    }
    assert card.endpoint["headers"]["secret_refs"] == {
        "Authorization": "authorization",
    }
    assert credential is not None
    assert credential.secrets["authorization"] == "Bearer secret-token"


def test_legacy_oauth_maps_tokens_to_oauth_credential() -> None:
    card, credential = legacy_mcp_client_to_driver(
        "oauth_docs",
        _config(
            transport="streamable_http",
            url="https://oauth.example.com/mcp",
            oauth=SimpleNamespace(
                client_id="client-id",
                scope="read write",
                access_token="access-token",
                refresh_token="refresh-token",
                expires_at=1780000000,
                token_endpoint="https://oauth.example.com/token",
                auth_endpoint="https://oauth.example.com/authorize",
            ),
        ),
    )

    assert card.credential.kind == "oauth2_auth_code"
    assert card.credential.ref == "mcp/oauth_docs/oauth"
    assert credential is not None
    assert credential.public["client_id"] == "client-id"
    assert credential.public["scope"] == "read write"
    assert credential.secrets["access_token"] == "access-token"
    assert credential.secrets["refresh_token"] == "refresh-token"


@pytest.mark.asyncio
async def test_migration_skips_existing_driver_card(tmp_path: Path) -> None:
    manager = DriverManager(
        tmp_path / "drivers",
        CredentialStore(tmp_path / "credentials.yaml"),
    )
    card, _ = legacy_mcp_client_to_driver("tavily_search", _config())
    dump_card(
        card,
        card_path(tmp_path / "drivers", "tavily_search", protocol="mcp"),
    )
    ws = SimpleNamespace(
        _config=SimpleNamespace(
            mcp=SimpleNamespace(clients={"tavily_search": _config()}),
        ),
    )

    report = await migrate_legacy_mcp_if_needed(ws, manager)

    assert report.skipped[0].client_key == "tavily_search"
    assert report.skipped[0].reason == "driver_card_exists"


@pytest.mark.asyncio
async def test_migration_skips_args_that_may_contain_secret(
    tmp_path: Path,
) -> None:
    manager = DriverManager(
        tmp_path / "drivers",
        CredentialStore(tmp_path / "credentials.yaml"),
    )
    ws = SimpleNamespace(
        _config=SimpleNamespace(
            mcp=SimpleNamespace(
                clients={
                    "arg_secret": _config(
                        command="custom-mcp",
                        args=["--api-key", "secret-value"],
                    ),
                },
            ),
        ),
    )

    report = await migrate_legacy_mcp_if_needed(ws, manager)

    assert not (tmp_path / "drivers" / "mcp" / "arg_secret.yaml").exists()
    assert report.warnings[0].reason == "args_may_contain_secret"
    assert report.skipped[0].reason == "unsafe_secret_in_args"


@pytest.mark.asyncio
async def test_migration_writes_card_and_credential(tmp_path: Path) -> None:
    manager = DriverManager(
        tmp_path / "drivers",
        CredentialStore(tmp_path / "credentials.yaml"),
    )
    ws = SimpleNamespace(
        _config=SimpleNamespace(
            mcp=SimpleNamespace(
                clients={
                    "echo": _config(
                        env={
                            "NODE_ENV": "production",
                            "ECHO_TOKEN": "secret-token",
                        },
                    ),
                },
            ),
        ),
    )

    report = await migrate_legacy_mcp_if_needed(ws, manager)

    card = load_card(tmp_path / "drivers" / "mcp" / "echo.yaml")
    record = manager.credential_store.get("mcp/echo")
    assert report.migrated[0].client_key == "echo"
    assert card.endpoint["env"]["public"]["NODE_ENV"] == "production"
    assert card.endpoint["env"]["secret_refs"]["ECHO_TOKEN"] == "ECHO_TOKEN"
    assert card.policy.rules[0].target.kind == "tool"
    assert card.policy.rules[0].target.name == "*"
    assert record.secrets["ECHO_TOKEN"] == "secret-token"
