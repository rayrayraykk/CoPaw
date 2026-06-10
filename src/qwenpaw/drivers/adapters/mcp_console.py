# -*- coding: utf-8 -*-
"""Console MCP DTO adapters for DriverCard and CredentialRecord."""

from __future__ import annotations

import re
import time
from dataclasses import replace
from typing import Any

from qwenpaw.drivers.contracts import CredentialRef, DriverCard, DriverPolicy
from qwenpaw.drivers.credentials.types import CredentialRecord

_SAFE_KEY_PATTERN = re.compile(r"[^a-z0-9_]+")
PUBLIC_HEADER_KEYS = {
    "accept",
    "content-type",
    "user-agent",
    "x-client-name",
}
SECRET_HEADER_KEYS = {
    "authorization",
    "cookie",
    "set-cookie",
    "x-api-key",
    "api-key",
    "x-auth-token",
}
PUBLIC_ENV_KEYS = {
    "NODE_ENV",
    "LOG_LEVEL",
    "DEBUG",
    "MCP_MODE",
}
SECRET_ENV_KEY_PARTS = (
    "KEY",
    "TOKEN",
    "SECRET",
    "PASSWORD",
    "PASSWD",
    "CREDENTIAL",
    "AUTH",
)
STATIC_CREDENTIAL_ALIAS = "static"
OAUTH_CREDENTIAL_ALIAS = "oauth"


def normalize_secret_key(name: str, existing: set[str] | None = None) -> str:
    """Return a lowercase credential secret key for an env/header name."""
    base = _SAFE_KEY_PATTERN.sub("_", name.strip().lower()).strip("_")
    if not base:
        base = "secret"
    if existing is None or base not in existing:
        return base
    index = 2
    while f"{base}_{index}" in existing:
        index += 1
    return f"{base}_{index}"


def build_mcp_credential_record(
    client_key: str,
    client: Any,
    *,
    existing: CredentialRecord | None = None,
) -> CredentialRecord:
    """Build the static MCP credential record from console request data."""
    ref = mcp_credential_ref(client_key)
    incoming_env = dict(_read_field(client, "env", {}) or {})
    incoming_headers = dict(_read_field(client, "headers", {}) or {})
    existing_secrets = dict(existing.secrets) if existing else {}
    secrets: dict[str, str] = {}

    for key, value in incoming_env.items():
        if (
            classify_mcp_binding(section="env", key=str(key), value=str(value))
            == "public"
        ):
            continue
        secret_key = str(key)
        secrets[secret_key] = _restore_masked_value(
            str(value),
            existing_secrets.get(secret_key, ""),
        )

    used = set(secrets)
    for header, value in incoming_headers.items():
        if (
            classify_mcp_binding(
                section="headers",
                key=str(header),
                value=str(value),
            )
            == "public"
        ):
            continue
        secret_key = normalize_secret_key(str(header), used)
        used.add(secret_key)
        old_value = existing_secrets.get(secret_key, "")
        secrets[secret_key] = _restore_masked_value(str(value), old_value)

    now = time.time()
    meta = dict(existing.meta) if existing else {"created_at": now}
    meta["updated_at"] = now
    return CredentialRecord(
        ref=ref,
        kind="static",
        public={},
        secrets=secrets,
        meta=meta,
    )


def build_mcp_driver_card(
    client_key: str,
    client: Any,
    credential_ref: str,
    *,
    credential_record: CredentialRecord | None = None,
    existing: DriverCard | None = None,
) -> DriverCard:
    """Build a DriverCard from console MCP create/update request data."""
    current = _card_to_client_data(existing) if existing else {}
    updates = _model_dump(client)
    data = {**current, **{k: v for k, v in updates.items() if v is not None}}
    transport = str(data.get("transport") or "stdio")
    secrets = dict(credential_record.secrets) if credential_record else {}

    if transport == "stdio":
        env = dict(data.get("env") or {})
        public_env, secret_env = split_mcp_binding("env", env)
        endpoint: dict[str, Any] = {
            "transport": "stdio",
            "command": str(data.get("command") or ""),
            "args": list(data.get("args") or []),
            "env": _source_binding_from_split(
                public_env,
                {str(key): str(key) for key in secret_env},
                STATIC_CREDENTIAL_ALIAS,
            ),
        }
        cwd = str(data.get("cwd") or "")
        if cwd:
            endpoint["cwd"] = cwd
    else:
        headers = dict(data.get("headers") or {})
        public_headers, secret_headers = split_mcp_binding("headers", headers)
        used: set[str] = set()
        secret_refs: dict[str, str] = {}
        for header in secret_headers:
            secret_key = normalize_secret_key(str(header), used)
            used.add(secret_key)
            secret_refs[str(header)] = secret_key
        endpoint = {
            "transport": transport,
            "url": str(data.get("url") or ""),
            "headers": _source_binding_from_split(
                public_headers,
                secret_refs,
                STATIC_CREDENTIAL_ALIAS,
            ),
        }
        _preserve_oauth_authorization_binding(existing, endpoint)

    credentials = _credential_refs_from_existing(existing)
    if secrets:
        credentials[STATIC_CREDENTIAL_ALIAS] = CredentialRef(
            kind="static",
            ref=credential_ref,
        )
    else:
        credentials.pop(STATIC_CREDENTIAL_ALIAS, None)
    credential = _legacy_primary_credential(credentials)
    return DriverCard(
        name=client_key,
        protocol="mcp",
        endpoint=endpoint,
        credential=credential,
        credentials=credentials,
        config={
            "display_name": str(data.get("name") or "").strip() or client_key,
            "description": str(data.get("description") or ""),
        },
        enabled=bool(data.get("enabled", True)),
        policy=(
            existing.policy
            if existing
            else DriverPolicy(default_effect="ask", rules=[])
        ),
    )


def build_mcp_client_info_payload(
    card: DriverCard,
    credential: CredentialRecord | None,
    oauth_credential: CredentialRecord | None = None,
) -> dict[str, Any]:
    """Return the MCPClientInfo-compatible API response payload."""
    endpoint = card.endpoint
    transport = str(endpoint.get("transport") or "stdio")
    env = _binding_to_response(endpoint.get("env") or {}, credential)
    headers = _binding_to_response(endpoint.get("headers") or {}, credential)
    return {
        "key": card.name,
        "name": str(card.config.get("display_name") or card.name),
        "description": str(card.config.get("description") or ""),
        "enabled": card.enabled,
        "transport": transport,
        "url": str(endpoint.get("url") or ""),
        "headers": headers,
        "command": str(endpoint.get("command") or ""),
        "args": list(endpoint.get("args") or []),
        "env": env,
        "cwd": str(endpoint.get("cwd") or ""),
        "oauth_status": _oauth_status(oauth_credential),
        "access_summary": {
            "default_effect": card.policy.default_effect,
            "overrides_count": sum(
                1
                for rule in card.policy.rules
                if _is_tool_access_override(rule)
            ),
        },
    }


def _is_tool_access_override(rule: Any) -> bool:
    if (
        rule.condition is not None
        or rule.target.kind != "tool"
        or not rule.target.name
        or rule.effect not in {"allow", "ask", "deny"}
    ):
        return False
    principal = rule.principal
    if (
        principal.source_type.strip().lower() in {"channel", "app"}
        and principal.source_value.strip()
        and principal.subject_type.strip().lower() in {"all", "user"}
    ):
        return True
    subject = rule.subject.strip()
    return (
        subject == "*"
        or subject.startswith("channel:")
        or subject.startswith("app:")
        or subject.startswith("user:")
    )


def mcp_credential_ref(client_key: str) -> str:
    return f"mcp/{client_key}"


def mcp_oauth_credential_ref(client_key: str) -> str:
    return f"mcp/{client_key}/oauth"


def attach_mcp_oauth_credential(card: DriverCard, ref: str) -> DriverCard:
    """Return a card with OAuth source and bearer header binding."""
    credentials = _credential_refs_from_existing(card)
    credentials[OAUTH_CREDENTIAL_ALIAS] = CredentialRef(
        "oauth2_auth_code",
        ref,
    )
    endpoint = dict(card.endpoint)
    if str(endpoint.get("transport") or "stdio") != "stdio":
        headers = dict(endpoint.get("headers") or {})
        headers["Authorization"] = {
            "source": "credential",
            "credential": OAUTH_CREDENTIAL_ALIAS,
            "field": "access_token",
            "format": "Bearer {value}",
        }
        endpoint["headers"] = headers
    return replace(
        card,
        endpoint=endpoint,
        credential=_legacy_primary_credential(credentials),
        credentials=credentials,
    )


def detach_mcp_oauth_credential(card: DriverCard) -> DriverCard:
    """Return a card with OAuth source and bearer binding removed."""
    credentials = _credential_refs_from_existing(card)
    credentials.pop(OAUTH_CREDENTIAL_ALIAS, None)
    endpoint = dict(card.endpoint)
    headers = endpoint.get("headers")
    if isinstance(headers, dict):
        updated_headers = dict(headers)
        auth_spec = updated_headers.get("Authorization")
        if (
            isinstance(auth_spec, dict)
            and auth_spec.get("source") == "credential"
            and auth_spec.get("credential") == OAUTH_CREDENTIAL_ALIAS
        ):
            updated_headers.pop("Authorization", None)
        endpoint["headers"] = updated_headers
    return replace(
        card,
        endpoint=endpoint,
        credential=_legacy_primary_credential(credentials),
        credentials=credentials,
    )


# pylint: disable-next=too-many-return-statements
def classify_mcp_binding(
    *,
    section: str,
    key: str,
    value: str,
) -> str:
    """Classify one legacy Console MCP env/header value."""
    del value
    if section == "headers":
        lowered = key.strip().lower()
        if lowered in SECRET_HEADER_KEYS:
            return "secret"
        if lowered in PUBLIC_HEADER_KEYS:
            return "public"
        return "secret"

    if section == "env":
        stripped = key.strip()
        upper = stripped.upper()
        if any(part in upper for part in SECRET_ENV_KEY_PARTS):
            return "secret"
        if stripped in PUBLIC_ENV_KEYS or upper in PUBLIC_ENV_KEYS:
            return "public"
        return "secret"

    return "secret"


def split_mcp_binding(
    section: str,
    values: dict[str, str],
) -> tuple[dict[str, str], dict[str, str]]:
    """Split legacy env/header maps into public literals and secret values."""
    public: dict[str, str] = {}
    secrets: dict[str, str] = {}
    for key, value in dict(values or {}).items():
        target = classify_mcp_binding(
            section=section,
            key=str(key),
            value=str(value),
        )
        if target == "public":
            public[str(key)] = str(value)
        else:
            secrets[str(key)] = str(value)
    return public, secrets


def _source_binding_from_split(
    public: dict[str, str],
    secret_refs: dict[str, str],
    credential_alias: str,
) -> dict[str, dict[str, str]]:
    binding: dict[str, dict[str, str]] = {}
    for key, value in public.items():
        binding[str(key)] = {"source": "literal", "value": str(value)}
    for key, secret_key in secret_refs.items():
        binding[str(key)] = {
            "source": "credential",
            "credential": credential_alias,
            "field": str(secret_key),
        }
    return binding


def _binding_to_response(
    binding: Any,
    credential: CredentialRecord | None,
) -> dict[str, str]:
    if not isinstance(binding, dict):
        return {}
    if "public" not in binding and "secret_refs" not in binding:
        result: dict[str, str] = {}
        secrets = credential.secrets if credential else {}
        for key, spec in binding.items():
            if isinstance(spec, dict) and spec.get("source") == "literal":
                result[str(key)] = str(spec.get("value") or "")
            elif (
                isinstance(spec, dict)
                and spec.get("source") == "credential"
                and spec.get("credential") == STATIC_CREDENTIAL_ALIAS
            ):
                value = secrets.get(str(spec.get("field") or ""), "")
                result[str(key)] = _mask_env_value(str(value))
            elif not isinstance(spec, dict):
                result[str(key)] = str(spec)
        return result
    result = {
        str(key): str(value)
        for key, value in dict(binding.get("public") or {}).items()
    }
    secrets = credential.secrets if credential else {}
    for output_name, secret_key in dict(
        binding.get("secret_refs") or {},
    ).items():
        value = secrets.get(str(secret_key), "")
        result[str(output_name)] = _mask_env_value(str(value))
    return result


def _oauth_status(record: CredentialRecord | None) -> dict[str, Any] | None:
    if record is None:
        return None
    access_token = str(record.secrets.get("access_token") or "")
    expires_at = float(record.public.get("expires_at") or 0.0)
    authorized = bool(access_token) and (
        expires_at <= 0 or expires_at > time.time()
    )
    return {
        "authorized": authorized,
        "expires_at": expires_at,
        "scope": str(record.public.get("scope") or ""),
        "client_id": str(record.public.get("client_id") or ""),
    }


def _card_to_client_data(card: DriverCard | None) -> dict[str, Any]:
    if card is None:
        return {}
    endpoint = card.endpoint
    return {
        "name": card.config.get("display_name") or card.name,
        "description": card.config.get("description") or "",
        "enabled": card.enabled,
        "transport": endpoint.get("transport") or "stdio",
        "url": endpoint.get("url") or "",
        "headers": _binding_plain_keys(endpoint.get("headers") or {}),
        "command": endpoint.get("command") or "",
        "args": list(endpoint.get("args") or []),
        "env": _binding_plain_keys(endpoint.get("env") or {}),
        "cwd": endpoint.get("cwd") or "",
    }


def _binding_plain_keys(binding: Any) -> dict[str, str]:
    if not isinstance(binding, dict):
        return {}
    if "public" not in binding and "secret_refs" not in binding:
        result: dict[str, str] = {}
        for key, spec in binding.items():
            if isinstance(spec, dict) and spec.get("source") == "literal":
                result[str(key)] = str(spec.get("value") or "")
            elif (
                isinstance(spec, dict)
                and spec.get("source") == "credential"
                and spec.get("credential") == STATIC_CREDENTIAL_ALIAS
            ):
                result[str(key)] = ""
            elif not isinstance(spec, dict):
                result[str(key)] = str(spec)
        return result
    result = {
        str(key): str(value)
        for key, value in dict(binding.get("public") or {}).items()
    }
    for key in dict(binding.get("secret_refs") or {}):
        result[str(key)] = ""
    return result


def _restore_masked_value(incoming: str, existing: str) -> str:
    if existing and incoming == _mask_env_value(existing):
        return existing
    return incoming


def _mask_env_value(value: str) -> str:
    if not value:
        return value
    length = len(value)
    if length <= 8:
        return "*" * length
    prefix_len = 3 if length > 2 and value[2] == "-" else 2
    prefix = value[:prefix_len]
    suffix = value[-4:]
    masked_len = max(length - prefix_len - 4, 4)
    return f"{prefix}{'*' * masked_len}{suffix}"


def _model_dump(value: Any) -> dict[str, Any]:
    if hasattr(value, "model_dump"):
        return value.model_dump(exclude_unset=True)
    if hasattr(value, "dict"):
        return value.dict(exclude_unset=True)
    if isinstance(value, dict):
        return dict(value)
    return dict(vars(value))


def _read_field(value: Any, field: str, default: Any = None) -> Any:
    if isinstance(value, dict):
        return value.get(field, default)
    return getattr(value, field, default)


def update_oauth_credential_ref(card: DriverCard, ref: str) -> DriverCard:
    """Return a copy of card pointing at an OAuth auth-code credential."""
    return attach_mcp_oauth_credential(card, ref)


def _credential_refs_from_existing(
    existing: DriverCard | None,
) -> dict[str, CredentialRef]:
    if existing is None:
        return {}
    credentials = dict(existing.credentials)
    if existing.credential.kind != "none" and not credentials:
        alias = (
            OAUTH_CREDENTIAL_ALIAS
            if existing.credential.kind == "oauth2_auth_code"
            else (
                STATIC_CREDENTIAL_ALIAS
                if existing.credential.kind == "static"
                else "default"
            )
        )
        credentials[alias] = existing.credential
    return credentials


def _legacy_primary_credential(
    credentials: dict[str, CredentialRef],
) -> CredentialRef:
    for alias in (OAUTH_CREDENTIAL_ALIAS, STATIC_CREDENTIAL_ALIAS, "default"):
        credential = credentials.get(alias)
        if credential is not None:
            return credential
    return next(iter(credentials.values()), CredentialRef("none"))


def _preserve_oauth_authorization_binding(
    existing: DriverCard | None,
    endpoint: dict[str, Any],
) -> None:
    if existing is None:
        return
    if OAUTH_CREDENTIAL_ALIAS not in _credential_refs_from_existing(existing):
        return
    headers = dict(endpoint.get("headers") or {})
    if "Authorization" in headers:
        return
    existing_headers = existing.endpoint.get("headers")
    if not isinstance(existing_headers, dict):
        return
    existing_auth = existing_headers.get("Authorization")
    if (
        isinstance(existing_auth, dict)
        and existing_auth.get("source") == "credential"
        and existing_auth.get("credential") == OAUTH_CREDENTIAL_ALIAS
    ):
        headers["Authorization"] = dict(existing_auth)
        endpoint["headers"] = headers
