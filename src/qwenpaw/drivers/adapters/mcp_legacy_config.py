# -*- coding: utf-8 -*-
"""Legacy agent.json MCP migration helpers."""

from __future__ import annotations

import time
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any

import yaml

from qwenpaw.drivers.adapters.mcp_console import (
    mcp_credential_ref,
    mcp_oauth_credential_ref,
    normalize_secret_key,
    split_mcp_binding,
)
from qwenpaw.drivers.contracts import CredentialRef, DriverCard, PolicyRule
from qwenpaw.drivers.credentials.types import CredentialRecord
from qwenpaw.drivers.manager import DriverManager
from qwenpaw.drivers.storage import card_path, dump_card


@dataclass
class LegacyMCPMigratedClient:
    client_key: str
    card_path: str
    credential_ref: str


@dataclass
class LegacyMCPMigrationSkippedClient:
    client_key: str
    reason: str


@dataclass
class LegacyMCPMigrationWarning:
    client_key: str
    field: str
    reason: str


@dataclass
class LegacyMCPMigrationReport:
    migrated: list[LegacyMCPMigratedClient] = field(default_factory=list)
    skipped: list[LegacyMCPMigrationSkippedClient] = field(
        default_factory=list,
    )
    warnings: list[LegacyMCPMigrationWarning] = field(default_factory=list)


async def migrate_legacy_mcp_if_needed(
    ws: Any,
    driver_manager: DriverManager,
) -> LegacyMCPMigrationReport:
    """Migrate legacy agent.json mcp.clients into Driver storage."""
    report = LegacyMCPMigrationReport()
    legacy_mcp = getattr(getattr(ws, "_config", None), "mcp", None)
    clients = getattr(legacy_mcp, "clients", None)
    if not clients:
        return report

    for client_key, config in dict(clients).items():
        _migrate_one_client(
            str(client_key),
            config,
            driver_manager,
            report,
        )

    _write_report(driver_manager.cards_dir, report)
    return report


def _migrate_one_client(
    client_key: str,
    config: Any,
    driver_manager: DriverManager,
    report: LegacyMCPMigrationReport,
) -> None:
    target = card_path(driver_manager.cards_dir, client_key, protocol="mcp")
    if target.is_file():
        report.skipped.append(
            LegacyMCPMigrationSkippedClient(
                client_key=client_key,
                reason="driver_card_exists",
            ),
        )
        return

    if _args_may_contain_secret(list(getattr(config, "args", []) or [])):
        report.warnings.append(
            LegacyMCPMigrationWarning(
                client_key=client_key,
                field="args",
                reason="args_may_contain_secret",
            ),
        )
        report.skipped.append(
            LegacyMCPMigrationSkippedClient(
                client_key=client_key,
                reason="unsafe_secret_in_args",
            ),
        )
        return

    card, credential = legacy_mcp_client_to_driver(client_key, config)
    if credential is not None:
        try:
            driver_manager.credential_store.get(credential.ref)
        except Exception:
            driver_manager.credential_store.put(credential)
    dump_card(card, target)
    report.migrated.append(
        LegacyMCPMigratedClient(
            client_key=client_key,
            card_path=str(target),
            credential_ref=credential.ref if credential else "",
        ),
    )


def legacy_mcp_client_to_driver(
    client_key: str,
    config: Any,
) -> tuple[DriverCard, CredentialRecord | None]:
    """Convert one legacy MCP config object into Driver contracts."""
    transport = str(getattr(config, "transport", "stdio") or "stdio")
    oauth = getattr(config, "oauth", None)
    now = time.time()

    env_public, env_secrets = split_mcp_binding(
        "env",
        dict(getattr(config, "env", {}) or {}),
    )
    header_public, header_secrets = split_mcp_binding(
        "headers",
        dict(getattr(config, "headers", {}) or {}),
    )

    endpoint: dict[str, Any]
    if transport == "stdio":
        endpoint = {
            "transport": "stdio",
            "command": str(getattr(config, "command", "") or ""),
            "args": list(getattr(config, "args", []) or []),
            "env": {
                "public": env_public,
                "secret_refs": {key: key for key in env_secrets},
            },
        }
        cwd = str(getattr(config, "cwd", "") or "")
        if cwd:
            endpoint["cwd"] = cwd
    else:
        used: set[str] = set()
        header_secret_refs: dict[str, str] = {}
        for header in header_secrets:
            secret_key = normalize_secret_key(header, used)
            used.add(secret_key)
            header_secret_refs[header] = secret_key
        endpoint = {
            "transport": transport,
            "url": str(getattr(config, "url", "") or ""),
            "headers": {
                "public": header_public,
                "secret_refs": header_secret_refs,
            },
        }

    credential = _build_legacy_credential(
        client_key,
        oauth,
        env_secrets,
        header_secrets,
        endpoint,
        now,
    )
    card = DriverCard(
        name=client_key,
        protocol="mcp",
        endpoint=endpoint,
        credential=(
            CredentialRef(kind=credential.kind, ref=credential.ref)
            if credential is not None
            else CredentialRef(kind="none")
        ),
        config={
            "display_name": str(getattr(config, "name", "") or client_key),
            "description": str(getattr(config, "description", "") or ""),
        },
        enabled=bool(getattr(config, "enabled", True)),
        policy=[PolicyRule(subject="*", effect="allow")],
    )
    return card, credential


def _build_legacy_credential(
    client_key: str,
    oauth: Any,
    env_secrets: dict[str, str],
    header_secrets: dict[str, str],
    endpoint: dict[str, Any],
    now: float,
) -> CredentialRecord | None:
    secrets: dict[str, Any] = {}
    public: dict[str, Any] = {}
    kind = "static"
    ref = mcp_credential_ref(client_key)

    for key, value in env_secrets.items():
        secrets[key] = value

    headers = endpoint.get("headers") if isinstance(endpoint, dict) else None
    secret_refs = {}
    if isinstance(headers, dict):
        secret_refs = dict(headers.get("secret_refs") or {})
    for header, value in header_secrets.items():
        secret_key = str(
            secret_refs.get(header) or normalize_secret_key(header),
        )
        secrets[secret_key] = value

    if oauth is not None:
        kind = "oauth2_auth_code"
        ref = mcp_oauth_credential_ref(client_key)
        public.update(
            {
                "client_id": str(getattr(oauth, "client_id", "") or ""),
                "scope": str(getattr(oauth, "scope", "") or ""),
                "expires_at": float(getattr(oauth, "expires_at", 0.0) or 0.0),
                "token_endpoint": str(
                    getattr(oauth, "token_endpoint", "") or "",
                ),
                "auth_endpoint": str(
                    getattr(oauth, "auth_endpoint", "") or "",
                ),
            },
        )
        for key in ("access_token", "refresh_token", "client_secret"):
            value = getattr(oauth, key, "")
            if value:
                secrets[key] = value

    if not secrets and not public:
        return None
    return CredentialRecord(
        ref=ref,
        kind=kind,
        public=public,
        secrets=secrets,
        meta={
            "created_at": now,
            "updated_at": now,
            "source": "legacy_agent_json_mcp",
        },
    )


def _args_may_contain_secret(args: list[str]) -> bool:
    markers = ("api-key", "apikey", "token", "secret", "password", "auth")
    return any(
        any(marker in str(arg).lower() for marker in markers) for arg in args
    )


def _write_report(cards_dir: Path, report: LegacyMCPMigrationReport) -> None:
    if not (report.migrated or report.skipped or report.warnings):
        return
    cards_dir.mkdir(parents=True, exist_ok=True)
    path = cards_dir / ".legacy_mcp_migration_report.yaml"
    payload = asdict(report)
    path.write_text(
        yaml.safe_dump(payload, allow_unicode=False, sort_keys=False),
        encoding="utf-8",
    )
