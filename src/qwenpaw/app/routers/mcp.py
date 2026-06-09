# -*- coding: utf-8 -*-
"""API routes for MCP (Model Context Protocol) clients management."""

from __future__ import annotations

import asyncio
import logging
from typing import Any, Dict, List, Optional, Literal

from fastapi import APIRouter, Body, HTTPException, Path, Request
from pydantic import BaseModel, Field

from ...drivers.adapters.mcp_console import (
    build_mcp_client_info_payload,
    build_mcp_credential_record,
    build_mcp_driver_card,
    mcp_credential_ref,
    mcp_oauth_credential_ref,
)
from ...drivers.contracts import (
    DriverPolicy,
    PolicyPrincipal,
    PolicyRule,
    PolicyTarget,
    iter_credential_refs,
)
from ...drivers.credentials.store import CredentialStore
from ...drivers.credentials.types import CredentialRecord
from ...drivers.errors import CredentialNotFoundError
from ...drivers.storage import (
    card_path,
    delete_card_paths_for_name,
    dump_card,
    list_card_paths,
    load_card,
)

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/mcp", tags=["mcp"])


class MCPClientOAuthStatus(BaseModel):
    """Summarised OAuth status returned in client info."""

    authorized: bool = False
    expires_at: float = 0.0
    scope: str = ""
    client_id: str = ""


class MCPAccessSummary(BaseModel):
    """Small access policy summary for MCP client cards."""

    default_effect: Literal["allow", "ask", "deny"] = "deny"
    overrides_count: int = 0


class MCPClientInfo(BaseModel):
    """MCP client information for API responses."""

    key: str = Field(..., description="Unique client key identifier")
    name: str = Field(..., description="Client display name")
    description: str = Field(default="", description="Client description")
    enabled: bool = Field(..., description="Whether the client is enabled")
    transport: Literal["stdio", "streamable_http", "sse"] = Field(
        ...,
        description="MCP transport type",
    )
    url: str = Field(
        default="",
        description="Remote MCP endpoint URL (for HTTP/SSE transports)",
    )
    headers: Dict[str, str] = Field(
        default_factory=dict,
        description="HTTP headers for remote transport",
    )
    command: str = Field(
        default="",
        description="Command to launch the MCP server",
    )
    args: List[str] = Field(
        default_factory=list,
        description="Command-line arguments",
    )
    env: Dict[str, str] = Field(
        default_factory=dict,
        description="Environment variables",
    )
    cwd: str = Field(
        default="",
        description="Working directory for stdio MCP command",
    )
    oauth_status: Optional[MCPClientOAuthStatus] = Field(
        default=None,
        description="OAuth token status (None if OAuth not configured)",
    )
    access_summary: MCPAccessSummary = Field(
        default_factory=MCPAccessSummary,
        description="Summarised MCP access policy",
    )


class MCPClientCreateRequest(BaseModel):
    """Request body for creating/updating an MCP client."""

    name: str = Field(..., description="Client display name")
    description: str = Field(default="", description="Client description")
    enabled: bool = Field(
        default=True,
        description="Whether to enable the client",
    )
    transport: Literal["stdio", "streamable_http", "sse"] = Field(
        default="stdio",
        description="MCP transport type",
    )
    url: str = Field(
        default="",
        description="Remote MCP endpoint URL (for HTTP/SSE transports)",
    )
    headers: Dict[str, str] = Field(
        default_factory=dict,
        description="HTTP headers for remote transport",
    )
    command: str = Field(
        default="",
        description="Command to launch the MCP server",
    )
    args: List[str] = Field(
        default_factory=list,
        description="Command-line arguments",
    )
    env: Dict[str, str] = Field(
        default_factory=dict,
        description="Environment variables",
    )
    cwd: str = Field(
        default="",
        description="Working directory for stdio MCP command",
    )


class MCPClientUpdateRequest(BaseModel):
    """Request body for updating an MCP client (all fields optional)."""

    name: Optional[str] = Field(None, description="Client display name")
    description: Optional[str] = Field(None, description="Client description")
    enabled: Optional[bool] = Field(
        None,
        description="Whether to enable the client",
    )
    transport: Optional[Literal["stdio", "streamable_http", "sse"]] = Field(
        None,
        description="MCP transport type",
    )
    url: Optional[str] = Field(
        None,
        description="Remote MCP endpoint URL (for HTTP/SSE transports)",
    )
    headers: Optional[Dict[str, str]] = Field(
        None,
        description="HTTP headers for remote transport",
    )
    command: Optional[str] = Field(
        None,
        description="Command to launch the MCP server",
    )
    args: Optional[List[str]] = Field(
        None,
        description="Command-line arguments",
    )
    env: Optional[Dict[str, str]] = Field(
        None,
        description="Environment variables",
    )
    cwd: Optional[str] = Field(
        None,
        description="Working directory for stdio MCP command",
    )


class MCPAccessRule(BaseModel):
    """Console-managed access rule for one MCP source/object tuple."""

    source_type: Literal["channel", "app"] = Field(
        default="channel",
        description="Where the tool call comes from",
    )
    source_value: str = Field(
        default="console",
        description="Concrete source, e.g. console, dingtalk, Creator",
    )
    subject_type: Literal["all", "user"] = Field(
        default="all",
        description="Object scope within the source",
    )
    subject_value: str = Field(
        default="",
        description="Concrete object value when subject_type is user",
    )
    effect: Literal["allow", "ask", "deny"] = Field(
        ...,
        description="Access effect for this source/object tuple",
    )


class MCPToolDefaultPolicy(BaseModel):
    """Console-managed default policy for one MCP tool."""

    tool_name: str = Field(..., description="MCP tool name")
    effect: Literal["allow", "ask", "deny"] = Field(
        ...,
        description="Default effect for this tool",
    )


class MCPToolAccessOverride(MCPAccessRule):
    """Console-managed access override for one MCP source/object/tool tuple."""

    tool_name: str = Field(..., description="MCP tool name")


class MCPAccessPolicy(BaseModel):
    """Console-friendly MCP access policy payload."""

    default_effect: Literal["allow", "ask", "deny"] = Field(
        default="deny",
        description="Default effect when no MCP rule matches",
    )
    client_overrides: List[MCPAccessRule] = Field(
        default_factory=list,
        description="Console-managed MCP-wide source/object overrides",
    )
    tool_defaults: List[MCPToolDefaultPolicy] = Field(
        default_factory=list,
        description="Console-managed default effects for individual tools",
    )
    tool_overrides: List[MCPToolAccessOverride] = Field(
        default_factory=list,
        description="Console-managed per-source/per-object/per-tool overrides",
    )
    unmanaged_rules_count: int = Field(
        default=0,
        description="Rules preserved but not editable by the console",
    )


def _restore_original_values(
    incoming: Dict[str, str],
    existing: Dict[str, str],
) -> Dict[str, str]:
    """Preserve original values when incoming matches their masked form."""
    restored: Dict[str, str] = {}
    for k, v in incoming.items():
        if k in existing and v == _mask_env_value(existing[k]):
            restored[k] = existing[k]
        else:
            restored[k] = v
    return restored


def _mask_env_value(value: str) -> str:
    """
    Mask environment variable value showing first 2-3 chars and last 4 chars.

    Examples:
        sk-proj-1234567890abcdefghij1234 -> sk-****************************1234
        abc123456789xyz -> ab***********xyz (if no dash)
        my-api-key-value -> my-************lue
        short123 -> ******** (8 chars or less, fully masked)
    """
    if not value:
        return value

    length = len(value)
    if length <= 8:
        # For short values, just mask everything
        return "*" * length

    # Show first 2-3 characters (3 if there's a dash at position 2)
    prefix_len = 3 if length > 2 and value[2] == "-" else 2
    prefix = value[:prefix_len]

    # Show last 4 characters
    suffix = value[-4:]

    # Calculate masked section length (at least 4 asterisks)
    masked_len = max(length - prefix_len - 4, 4)

    return f"{prefix}{'*' * masked_len}{suffix}"


_RESERVED_KEY_PREFIXES = ("tools/", "toggle/", "oauth/", "policy/")


def _validate_client_key(client_key: str) -> None:
    """Raise 400 if the key collides with reserved route prefixes."""
    lower = client_key.lower()
    for prefix in _RESERVED_KEY_PREFIXES:
        if lower == prefix.rstrip("/") or lower.startswith(prefix):
            raise HTTPException(
                400,
                detail=f"MCP client key must not start with reserved "
                f"prefix '{prefix}'. Please choose a different key.",
            )


def _normalize_mcp_display_name(name: str, *, fallback: str) -> str:
    value = str(name or "").strip()
    return value or fallback


def _display_name_key(value: str) -> str:
    return str(value or "").strip().casefold()


def _card_display_name(card: Any) -> str:
    return _normalize_mcp_display_name(
        str(card.config.get("display_name") or ""),
        fallback=card.name,
    )


def _ensure_mcp_display_name_unique(
    agent: Any,
    display_name: str,
    *,
    client_key: str,
) -> None:
    """Ensure display names are unambiguous user-facing MCP identifiers."""
    desired = _display_name_key(display_name)
    for card in _list_mcp_cards(agent):
        if card.name == client_key:
            continue
        if desired == _display_name_key(card.name):
            raise HTTPException(
                400,
                detail=(
                    f"MCP client name '{display_name}' conflicts with "
                    f"existing MCP client key '{card.name}'."
                ),
            )
        existing_display = _card_display_name(card)
        if desired == _display_name_key(existing_display):
            raise HTTPException(
                400,
                detail=(
                    f"MCP client name '{display_name}' already exists "
                    f"for MCP client '{card.name}'."
                ),
            )


class MCPToolInfo(BaseModel):
    """MCP tool information returned from a connected server."""

    name: str = Field(..., description="Tool name")
    description: str = Field(default="", description="Tool description")
    input_schema: Dict[str, Any] = Field(
        default_factory=dict,
        description="JSON Schema for the tool's input parameters",
    )


def _workspace_cards_dir(agent: Any):
    return agent.workspace_dir / "drivers"


def _mcp_card_path(agent: Any, client_key: str):
    return card_path(_workspace_cards_dir(agent), client_key, protocol="mcp")


def _workspace_credential_store(agent: Any) -> CredentialStore:
    manager = getattr(agent, "driver_manager", None)
    if manager is not None:
        return manager.credential_store
    return CredentialStore(agent.workspace_dir / "credentials.yaml")


def _load_mcp_card(agent: Any, client_key: str):
    path = _mcp_card_path(agent, client_key)
    if not path.is_file():
        raise HTTPException(404, detail=f"MCP client '{client_key}' not found")
    card = load_card(path)
    if card.protocol != "mcp":
        raise HTTPException(404, detail=f"MCP client '{client_key}' not found")
    return card


def _load_optional_credential(
    store: CredentialStore,
    ref: str,
) -> CredentialRecord | None:
    if not ref:
        return None
    try:
        return store.get(ref)
    except CredentialNotFoundError:
        return None


def _build_info_from_card(agent: Any, card: Any) -> MCPClientInfo:
    store = _workspace_credential_store(agent)
    credentials = iter_credential_refs(card)
    static_ref = credentials.get("static")
    if static_ref is None and card.credential.kind == "static":
        static_ref = card.credential
    credential = (
        _load_optional_credential(store, static_ref.ref)
        if static_ref is not None
        else None
    )
    oauth_ref = credentials.get("oauth")
    if oauth_ref is None and card.credential.kind == "oauth2_auth_code":
        oauth_ref = card.credential
    oauth_credential = _load_optional_credential(
        store,
        oauth_ref.ref if oauth_ref is not None else mcp_oauth_credential_ref(card.name),
    )
    return MCPClientInfo.model_validate(
        build_mcp_client_info_payload(card, credential, oauth_credential),
    )


def _list_mcp_cards(agent: Any) -> list[Any]:
    cards = {}
    for path in list_card_paths(_workspace_cards_dir(agent)):
        try:
            card = load_card(path)
        except Exception as exc:
            logger.warning("Failed to load DriverCard %s: %s", path, exc)
            continue
        if card.protocol == "mcp":
            cards[card.name] = card
    return sorted(cards.values(), key=lambda item: item.name)


async def _reload_driver_best_effort(agent: Any, client_key: str) -> None:
    manager = getattr(agent, "driver_manager", None)
    if manager is None:
        return

    async def reload_background() -> None:
        try:
            await manager.reload_driver(client_key)
            logger.info("MCP driver '%s' reloaded and active", client_key)
        except asyncio.CancelledError:
            raise
        except Exception as exc:
            logger.info(
                "MCP driver '%s' saved but not active yet: %s",
                client_key,
                exc,
            )

    # The /mcp endpoints are configuration APIs.  Persisting a client should
    # not block on a real MCP handshake because many user/test configs point
    # at commands or URLs that are not running yet.
    asyncio.create_task(
        reload_background(),
        name=f"mcp-driver-reload:{client_key}",
    )


async def _delete_driver_best_effort(agent: Any, client_key: str) -> None:
    manager = getattr(agent, "driver_manager", None)
    if manager is not None:
        try:
            await manager.delete_driver(client_key)
            return
        except Exception as exc:
            logger.info("Failed to delete active MCP driver '%s': %s", client_key, exc)
    delete_card_paths_for_name(_workspace_cards_dir(agent), client_key)


async def _ensure_mcp_driver_active(manager: Any, client_key: str) -> None:
    drivers = await manager.list_drivers(protocol="mcp")
    current = next(
        (item for item in drivers if item.name == client_key),
        None,
    )
    if current is None or current.status != "active":
        status = current.status if current is not None else "missing"
        raise HTTPException(
            503,
            detail=(
                f"MCP client '{client_key}' is saved but not active yet "
                f"(status={status})"
            ),
        )


def _merge_update_with_existing(
    existing_info: MCPClientInfo,
    updates: MCPClientUpdateRequest,
) -> MCPClientCreateRequest:
    data = existing_info.model_dump(mode="json")
    data.pop("key", None)
    data.pop("oauth_status", None)
    data.pop("access_summary", None)
    update_data = updates.model_dump(exclude_unset=True)
    data.update({key: value for key, value in update_data.items() if value is not None})
    return MCPClientCreateRequest.model_validate(data)


def _mcp_access_rule_from_rule(
    rule: PolicyRule,
) -> MCPAccessRule | None:
    if (
        rule.condition is not None
        or rule.target.kind != "tool"
        or not rule.target.name
        or rule.effect not in {"allow", "ask", "deny"}
    ):
        return None

    principal = rule.principal
    source_type = principal.source_type.strip().lower()
    source_value = principal.source_value.strip()
    subject_type = principal.subject_type.strip().lower()
    subject_value = principal.subject_value.strip()
    if (
        source_type in {"channel", "app"}
        and source_value
        and subject_type in {"all", "user"}
    ):
        return MCPAccessRule(
            source_type=source_type,  # type: ignore[arg-type]
            source_value=source_value,
            subject_type=subject_type,  # type: ignore[arg-type]
            subject_value="" if subject_type == "all" else subject_value,
            effect=rule.effect,  # type: ignore[arg-type]
        )

    return _legacy_subject_access_rule(rule)


def _legacy_subject_access_rule(
    rule: PolicyRule,
) -> MCPAccessRule | None:
    subject = rule.subject.strip()
    if not subject:
        return None
    if subject == "*":
        return MCPAccessRule(
            source_type="channel",
            source_value="console",
            subject_type="all",
            subject_value="",
            effect=rule.effect,  # type: ignore[arg-type]
        )
    if subject.startswith("channel:"):
        return MCPAccessRule(
            source_type="channel",
            source_value=subject.removeprefix("channel:") or "*",
            subject_type="all",
            subject_value="",
            effect=rule.effect,  # type: ignore[arg-type]
        )
    if subject.startswith("app:"):
        return MCPAccessRule(
            source_type="app",
            source_value=subject.removeprefix("app:") or "*",
            subject_type="all",
            subject_value="",
            effect=rule.effect,  # type: ignore[arg-type]
        )
    if subject.startswith("user:"):
        user = subject.removeprefix("user:")
        return MCPAccessRule(
            source_type="channel",
            source_value="console",
            subject_type="all" if user == "*" else "user",
            subject_value="" if user == "*" else user,
            effect=rule.effect,  # type: ignore[arg-type]
        )
    return None


def _is_mcp_tool_default_rule(rule: PolicyRule) -> bool:
    if (
        rule.condition is not None
        or rule.target.kind != "tool"
        or rule.target.name in {"", "*"}
        or rule.effect not in {"allow", "ask", "deny"}
        or rule.subject != "*"
    ):
        return False
    principal = rule.principal
    return (
        principal.source_type in {"", "*"}
        and principal.source_value in {"", "*"}
        and principal.subject_type in {"", "*"}
        and principal.subject_value in {"", "*"}
    )


def _mcp_client_override_from_rule(rule: PolicyRule) -> MCPAccessRule | None:
    if rule.target.kind != "tool" or rule.target.name != "*":
        return None
    return _mcp_access_rule_from_rule(rule)


def _mcp_tool_default_from_rule(
    rule: PolicyRule,
) -> MCPToolDefaultPolicy | None:
    if not _is_mcp_tool_default_rule(rule):
        return None
    return MCPToolDefaultPolicy(
        tool_name=rule.target.name,
        effect=rule.effect,  # type: ignore[arg-type]
    )


def _mcp_tool_override_from_rule(
    rule: PolicyRule,
) -> MCPToolAccessOverride | None:
    if (
        rule.target.kind != "tool"
        or rule.target.name in {"", "*"}
        or _is_mcp_tool_default_rule(rule)
    ):
        return None
    access_rule = _mcp_access_rule_from_rule(rule)
    if access_rule is None:
        return None
    return MCPToolAccessOverride(
        tool_name=rule.target.name,
        **access_rule.model_dump(mode="json"),
    )


def _is_console_managed_mcp_policy_rule(rule: PolicyRule) -> bool:
    return (
        _mcp_client_override_from_rule(rule) is not None
        or _mcp_tool_default_from_rule(rule) is not None
        or _mcp_tool_override_from_rule(rule) is not None
    )


def _mcp_access_policy_from_card(card: Any) -> MCPAccessPolicy:
    client_overrides = [
        override
        for rule in card.policy.rules
        if (override := _mcp_client_override_from_rule(rule)) is not None
    ]
    tool_defaults = [
        default
        for rule in card.policy.rules
        if (default := _mcp_tool_default_from_rule(rule)) is not None
    ]
    tool_overrides = [
        override
        for rule in card.policy.rules
        if (override := _mcp_tool_override_from_rule(rule)) is not None
    ]
    unmanaged_rules_count = sum(
        1 for rule in card.policy.rules if not _is_console_managed_mcp_policy_rule(rule)
    )
    return MCPAccessPolicy(
        default_effect=card.policy.default_effect,
        client_overrides=client_overrides,
        tool_defaults=tool_defaults,
        tool_overrides=tool_overrides,
        unmanaged_rules_count=unmanaged_rules_count,
    )


def _driver_policy_from_mcp_access_update(
    existing: DriverPolicy,
    access: MCPAccessPolicy,
) -> DriverPolicy:
    unmanaged_rules = [
        rule for rule in existing.rules if not _is_console_managed_mcp_policy_rule(rule)
    ]
    seen_rules: set[tuple[str, str, str, str, str]] = set()
    seen_defaults: set[str] = set()
    managed_rules: list[PolicyRule] = []
    for default in access.tool_defaults:
        tool_name = default.tool_name.strip()
        if not tool_name or tool_name == "*":
            raise HTTPException(400, detail="MCP tool default name is empty")
        if tool_name in seen_defaults:
            continue
        seen_defaults.add(tool_name)
        managed_rules.append(
            PolicyRule(
                subject="*",
                effect=default.effect,
                target=PolicyTarget(kind="tool", name=tool_name),
                principal=PolicyPrincipal(),
            )
        )
    for target_name, override in [
        ("*", override) for override in access.client_overrides
    ] + [(override.tool_name.strip(), override) for override in access.tool_overrides]:
        if not target_name:
            raise HTTPException(400, detail="MCP tool override name is empty")
        source_value = override.source_value.strip()
        subject_value = override.subject_value.strip()
        if not source_value:
            raise HTTPException(400, detail="MCP policy source value is empty")
        if override.subject_type == "user" and not subject_value:
            raise HTTPException(400, detail="MCP policy user value is empty")
        if override.subject_type == "all":
            subject_value = ""
        key = (
            target_name,
            override.source_type,
            source_value,
            override.subject_type,
            subject_value,
        )
        if key in seen_rules:
            continue
        seen_rules.add(key)
        managed_rules.append(
            PolicyRule(
                subject="*",
                effect=override.effect,
                target=PolicyTarget(kind="tool", name=target_name),
                principal=PolicyPrincipal(
                    source_type=override.source_type,
                    source_value=source_value,
                    subject_type=override.subject_type,
                    subject_value=subject_value,
                ),
            )
        )
    return DriverPolicy(
        default_effect=access.default_effect,
        rules=[*unmanaged_rules, *managed_rules],
    )


@router.get(
    "/tools/{client_key:path}",
    response_model=List[MCPToolInfo],
    summary="List tools from a connected MCP server",
)
async def list_mcp_tools(
    request: Request,
    client_key: str = Path(...),
) -> List[MCPToolInfo]:
    """Query a running MCP server for its available tools.

    Returns 503 if the client is not yet connected, empty list if
    disabled, or 502 if the MCP server query fails.
    """
    from ..agent_context import get_agent_for_request

    agent = await get_agent_for_request(request)
    card = _load_mcp_card(agent, client_key)
    if not card.enabled:
        return []

    manager = getattr(agent, "driver_manager", None)
    if manager is None:
        raise HTTPException(
            503,
            detail="Driver manager is not ready yet, please try again later",
        )

    try:
        await _ensure_mcp_driver_active(manager, client_key)
        capabilities = await manager.list_capabilities(
            protocol="mcp",
            kind="tool",
            request_context={},
        )
    except Exception as e:
        logger.warning(
            f"Failed to list tools for MCP client '{client_key}': {e}",
        )
        raise HTTPException(
            502,
            detail=f"Failed to query tools from MCP server: {e}",
        ) from e

    return [
        MCPToolInfo(
            name=capability.name,
            description=capability.description,
            input_schema=capability.input_schema,
        )
        for capability in capabilities
        if capability.driver_name == client_key
    ]


@router.get(
    "/policy/{client_key:path}",
    response_model=MCPAccessPolicy,
    summary="Get saved MCP access policy",
)
async def get_mcp_policy(
    request: Request,
    client_key: str = Path(...),
) -> MCPAccessPolicy:
    """Return saved MCP access policy without querying the MCP server."""
    from ..agent_context import get_agent_for_request

    agent = await get_agent_for_request(request)
    card = _load_mcp_card(agent, client_key)
    return _mcp_access_policy_from_card(card)


@router.put(
    "/policy/{client_key:path}",
    response_model=MCPAccessPolicy,
    summary="Update saved MCP access policy",
)
async def update_mcp_policy(
    request: Request,
    client_key: str = Path(...),
    access: MCPAccessPolicy = Body(...),
) -> MCPAccessPolicy:
    """Update console-managed MCP policy without querying the MCP server."""
    from ..agent_context import get_agent_for_request

    agent = await get_agent_for_request(request)
    card = _load_mcp_card(agent, client_key)
    card.policy = _driver_policy_from_mcp_access_update(
        card.policy,
        access,
    )
    path = _mcp_card_path(agent, client_key)
    dump_card(card, path)
    delete_card_paths_for_name(_workspace_cards_dir(agent), client_key, keep=path)
    await _reload_driver_best_effort(agent, client_key)
    return _mcp_access_policy_from_card(card)


@router.get(
    "",
    response_model=List[MCPClientInfo],
    summary="List all MCP clients",
)
async def list_mcp_clients(request: Request) -> List[MCPClientInfo]:
    """Get list of all configured MCP clients."""
    from ..agent_context import get_agent_for_request

    agent = await get_agent_for_request(request)
    return [_build_info_from_card(agent, card) for card in _list_mcp_cards(agent)]


@router.post(
    "",
    response_model=MCPClientInfo,
    summary="Create a new MCP client",
    status_code=201,
)
async def create_mcp_client(
    request: Request,
    client_key: str = Body(..., embed=True),
    client: MCPClientCreateRequest = Body(..., embed=True),
) -> MCPClientInfo:
    """Create a new MCP client configuration."""
    from ..agent_context import get_agent_for_request

    _validate_client_key(client_key)

    agent = await get_agent_for_request(request)
    path = _mcp_card_path(agent, client_key)
    if path.is_file():
        raise HTTPException(
            400,
            detail=f"MCP client '{client_key}' already exists. Use PUT to " f"update.",
        )
    _ensure_mcp_display_name_unique(
        agent,
        _normalize_mcp_display_name(client.name, fallback=client_key),
        client_key=client_key,
    )

    store = _workspace_credential_store(agent)
    credential = build_mcp_credential_record(client_key, client)
    card = build_mcp_driver_card(
        client_key,
        client,
        mcp_credential_ref(client_key),
        credential_record=credential,
    )
    if credential.secrets:
        store.put(credential)
    else:
        store.delete(credential.ref)
    dump_card(card, path)
    await _reload_driver_best_effort(agent, client_key)

    return _build_info_from_card(agent, card)


@router.patch(
    "/toggle/{client_key:path}",
    response_model=MCPClientInfo,
    summary="Toggle MCP client enabled status",
)
async def toggle_mcp_client(
    request: Request,
    client_key: str = Path(...),
) -> MCPClientInfo:
    """Toggle the enabled status of an MCP client."""
    from ..agent_context import get_agent_for_request

    agent = await get_agent_for_request(request)
    card = _load_mcp_card(agent, client_key)
    card.enabled = not card.enabled
    path = _mcp_card_path(agent, client_key)
    dump_card(card, path)
    delete_card_paths_for_name(_workspace_cards_dir(agent), client_key, keep=path)
    await _reload_driver_best_effort(agent, client_key)
    return _build_info_from_card(agent, card)


# ---------------------------------------------------------------------------
# Catch-all routes using {client_key:path} — MUST be registered last
# because :path greedily matches any remaining path segments including '/'.
# ---------------------------------------------------------------------------


@router.get(
    "/{client_key:path}",
    response_model=MCPClientInfo,
    summary="Get MCP client details",
)
async def get_mcp_client(
    request: Request,
    client_key: str = Path(...),
) -> MCPClientInfo:
    """Get details of a specific MCP client."""
    from ..agent_context import get_agent_for_request

    agent = await get_agent_for_request(request)
    card = _load_mcp_card(agent, client_key)
    return _build_info_from_card(agent, card)


@router.put(
    "/{client_key:path}",
    response_model=MCPClientInfo,
    summary="Update an MCP client",
)
async def update_mcp_client(
    request: Request,
    client_key: str = Path(...),
    updates: MCPClientUpdateRequest = Body(...),
) -> MCPClientInfo:
    """Update an existing MCP client configuration."""
    from ..agent_context import get_agent_for_request

    agent = await get_agent_for_request(request)
    existing_card = _load_mcp_card(agent, client_key)
    existing_info = _build_info_from_card(agent, existing_card)
    merged_client = _merge_update_with_existing(existing_info, updates)
    _ensure_mcp_display_name_unique(
        agent,
        _normalize_mcp_display_name(merged_client.name, fallback=client_key),
        client_key=client_key,
    )
    store = _workspace_credential_store(agent)
    existing_credential = _load_optional_credential(
        store,
        mcp_credential_ref(client_key),
    )
    credential = build_mcp_credential_record(
        client_key,
        merged_client,
        existing=existing_credential,
    )
    card = build_mcp_driver_card(
        client_key,
        merged_client,
        mcp_credential_ref(client_key),
        credential_record=credential,
        existing=existing_card,
    )
    if credential.secrets:
        store.put(credential)
    else:
        store.delete(credential.ref)
    path = _mcp_card_path(agent, client_key)
    dump_card(card, path)
    delete_card_paths_for_name(_workspace_cards_dir(agent), client_key, keep=path)
    await _reload_driver_best_effort(agent, client_key)
    return _build_info_from_card(agent, card)


@router.delete(
    "/{client_key:path}",
    response_model=Dict[str, str],
    summary="Delete an MCP client",
)
async def delete_mcp_client(
    request: Request,
    client_key: str = Path(...),
) -> Dict[str, str]:
    """Delete an MCP client configuration."""
    from ..agent_context import get_agent_for_request

    agent = await get_agent_for_request(request)
    card = _load_mcp_card(agent, client_key)
    store = _workspace_credential_store(agent)
    deleted_refs = set()
    for credential_ref in iter_credential_refs(card).values():
        if credential_ref.ref and credential_ref.ref not in deleted_refs:
            store.delete(credential_ref.ref)
            deleted_refs.add(credential_ref.ref)
    if card.credential.ref and card.credential.ref not in deleted_refs:
        store.delete(card.credential.ref)
    store.delete(mcp_oauth_credential_ref(client_key))
    await _delete_driver_best_effort(agent, client_key)

    return {"message": f"MCP client '{client_key}' deleted successfully"}
