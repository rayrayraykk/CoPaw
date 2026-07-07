# -*- coding: utf-8 -*-
"""Loop management API.

Endpoints:
    GET  /api/loops           — list available loops
    GET  /api/loops/profiles  — list all profiles
    PUT  /api/loops/profiles/{name} — update profile
    GET  /api/gates/catalog   — list gate types
"""
from __future__ import annotations

import logging
from typing import Any, Dict, List

from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel

from ..agent_context import get_agent_for_request
from ..utils import schedule_agent_reload
from ...config.config import (
    AgentsRunningConfig,
    GateInstanceConfig,
    LoopProfileConfig,
    load_agent_config,
    save_agent_config,
)
from ...loop.builtin_profiles import (
    BUILTIN_PROFILES,
    is_builtin_profile,
)
from ...loop.gate_catalog import GateCatalog

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/loops", tags=["loops"])

gates_router = APIRouter(
    prefix="/gates",
    tags=["gates"],
)

BUILTIN_LOOP = {
    "name": "goal",
    "slash_command": "goal",
    "description": ("Set a goal — agent works until done."),
    "source": "builtin",
}


@router.get("")
async def list_loops() -> list[dict[str, Any]]:
    """List all available loops."""
    result: list[dict[str, Any]] = [BUILTIN_LOOP]
    plugin_loops = _list_plugin_loops()
    result.extend(plugin_loops)

    seen: set[str] = set()
    deduped: list[dict[str, Any]] = []
    for loop in result:
        key = loop.get(
            "slash_command",
            loop["name"],
        )
        if key not in seen:
            seen.add(key)
            deduped.append(loop)
    return deduped


class GateInstancePayload(BaseModel):
    """Gate instance in API request."""

    id: str = ""
    type: str
    enabled: bool = True
    priority: int = 100
    params: Dict[str, Any] = {}


class ProfileUpdatePayload(BaseModel):
    """Payload for updating a profile."""

    gates: List[GateInstancePayload]


class ProfileCreatePayload(BaseModel):
    """Payload for creating a custom profile."""

    name: str
    description: str = ""
    gates: List[GateInstancePayload] = []


@router.get("/profiles")
async def list_profiles(
    request: Request,
) -> list[dict[str, Any]]:
    """List all profiles (builtin + custom)."""
    catalog = GateCatalog()
    catalog.ensure_builtins()
    workspace = await get_agent_for_request(request)
    saved = _load_saved_profiles(workspace.agent_id)
    all_saved = _load_all_custom_profiles(
        workspace.agent_id,
    )
    result = []

    for name, spec in BUILTIN_PROFILES.items():
        overrides = saved.get(name, {})
        gates = []
        for gs in sorted(
            spec.gates,
            key=lambda g: g.priority,
        ):
            gate_id = f"{name}-{gs.type}"
            ov = overrides.get(gs.type, {})
            merged_params = {
                **gs.default_params,
                **ov.get("params", {}),
            }
            enabled = ov.get(
                "enabled",
                gs.default_enabled,
            )
            entry = catalog.get(gs.type)
            gates.append(
                {
                    "id": gate_id,
                    "type": gs.type,
                    "name": (entry.name if entry else gs.type),
                    "description": (entry.description if entry else ""),
                    "category": (entry.category if entry else "unknown"),
                    "enabled": enabled,
                    "priority": gs.priority,
                    "params": merged_params,
                    "params_schema": (entry.params_schema if entry else {}),
                },
            )
        result.append(
            {
                "name": name,
                "scope": spec.scope,
                "is_builtin": True,
                "description": spec.description,
                "gates": gates,
            },
        )

    for cp in all_saved:
        if is_builtin_profile(cp.name):
            continue
        gates = []
        for g in cp.gates:
            entry = catalog.get(g.type)
            gates.append(
                {
                    "id": g.id or f"{cp.name}-{g.type}",
                    "type": g.type,
                    "name": (entry.name if entry else g.type),
                    "description": (entry.description if entry else ""),
                    "category": (entry.category if entry else "unknown"),
                    "enabled": g.enabled,
                    "priority": g.priority,
                    "params": g.params,
                    "params_schema": (entry.params_schema if entry else {}),
                },
            )
        result.append(
            {
                "name": cp.name,
                "scope": cp.scope,
                "is_builtin": False,
                "description": cp.description,
                "gates": gates,
            },
        )

    return result


@router.post("/profiles")
async def create_profile(
    payload: ProfileCreatePayload,
    request: Request,
) -> dict[str, Any]:
    """Create a custom profile."""
    if is_builtin_profile(payload.name):
        raise HTTPException(
            status_code=400,
            detail=(f"'{payload.name}' is a" f" reserved builtin name"),
        )

    catalog = GateCatalog()
    catalog.ensure_builtins()

    gates = []
    for i, g in enumerate(payload.gates):
        entry = catalog.get(g.type)
        if entry is None:
            raise HTTPException(
                status_code=400,
                detail=(f"Unknown gate type:" f" '{g.type}'"),
            )
        gates.append(
            GateInstanceConfig(
                id=g.id or f"{payload.name}-{g.type}-{i}",
                type=g.type,
                enabled=g.enabled,
                priority=g.priority,
                params=g.params,
            ),
        )

    await _save_custom_profile(
        request,
        payload.name,
        payload.description,
        gates,
    )
    return {"status": "ok", "profile": payload.name}


@router.put("/profiles/{name}")
async def update_profile(
    name: str,
    payload: ProfileUpdatePayload,
    request: Request,
) -> dict[str, Any]:
    """Update a loop profile."""
    if is_builtin_profile(name):
        spec = BUILTIN_PROFILES[name]
        valid_types = {g.type for g in spec.gates}
        overrides: dict[str, dict[str, Any]] = {}
        for gate in payload.gates:
            if gate.type not in valid_types:
                raise HTTPException(
                    status_code=400,
                    detail=(
                        f"Gate type '{gate.type}'"
                        f" not in builtin profile"
                        f" '{name}'"
                    ),
                )
            overrides[gate.type] = {
                "enabled": gate.enabled,
                "params": gate.params,
            }
        await _save_profile_overrides(
            request,
            name,
            overrides,
        )
    else:
        catalog = GateCatalog()
        catalog.ensure_builtins()
        gates = []
        for i, g in enumerate(payload.gates):
            gates.append(
                GateInstanceConfig(
                    id=g.id or f"{name}-{g.type}-{i}",
                    type=g.type,
                    enabled=g.enabled,
                    priority=g.priority,
                    params=g.params,
                ),
            )
        await _save_custom_profile(
            request,
            name,
            "",
            gates,
        )

    return {"status": "ok", "profile": name}


@router.delete("/profiles/{name}")
async def delete_profile(
    name: str,
    request: Request,
) -> dict[str, Any]:
    """Delete a custom profile."""
    if is_builtin_profile(name):
        raise HTTPException(
            status_code=400,
            detail="Cannot delete builtin profile",
        )
    workspace = await get_agent_for_request(
        request,
    )
    agent_config = load_agent_config(
        workspace.agent_id,
    )
    rc = agent_config.running or AgentsRunningConfig()
    existing = [p for p in (rc.loop.profiles or []) if p.name != name]
    rc.loop.profiles = existing
    agent_config.running = rc
    save_agent_config(
        workspace.agent_id,
        agent_config,
    )
    return {"status": "ok", "deleted": name}


@gates_router.get("/catalog")
async def get_gate_catalog() -> dict[str, Any]:
    """Return all available gate types."""
    catalog = GateCatalog()
    catalog.ensure_builtins()
    return {"gates": catalog.to_api_response()}


def _load_saved_profiles(
    agent_id: str,
) -> dict[str, dict[str, dict[str, Any]]]:
    """Load user overrides from agent config."""
    result: dict[
        str,
        dict[str, dict[str, Any]],
    ] = {}
    try:
        agent_config = load_agent_config(agent_id)
        rc = agent_config.running or AgentsRunningConfig()
        if rc.loop and rc.loop.profiles:
            for p in rc.loop.profiles:
                gate_map: dict[
                    str,
                    dict[str, Any],
                ] = {}
                for g in p.gates:
                    gate_map[g.type] = {
                        "enabled": g.enabled,
                        "params": g.params,
                    }
                result[p.name] = gate_map
    except Exception:
        logger.debug(
            "No saved profiles found",
            exc_info=True,
        )
    return result


def _load_all_custom_profiles(
    agent_id: str,
) -> list[LoopProfileConfig]:
    """Load all saved profiles from config."""
    try:
        agent_config = load_agent_config(agent_id)
        rc = agent_config.running or AgentsRunningConfig()
        return list(rc.loop.profiles or [])
    except Exception:
        logger.debug(
            "No custom profiles found",
            exc_info=True,
        )
        return []


async def _save_custom_profile(
    request: Request,
    name: str,
    description: str,
    gates: list[GateInstanceConfig],
) -> None:
    """Save a custom profile."""
    workspace = await get_agent_for_request(
        request,
    )
    agent_config = load_agent_config(
        workspace.agent_id,
    )
    rc = agent_config.running or AgentsRunningConfig()

    new_profile = LoopProfileConfig(
        name=name,
        scope="custom",
        is_builtin=False,
        description=description,
        gates=gates,
    )

    existing = list(rc.loop.profiles or [])
    found = False
    for i, p in enumerate(existing):
        if p.name == name:
            new_profile.description = description or p.description
            existing[i] = new_profile
            found = True
            break
    if not found:
        existing.append(new_profile)

    rc.loop.profiles = existing
    agent_config.running = rc
    save_agent_config(
        workspace.agent_id,
        agent_config,
    )
    schedule_agent_reload(
        request,
        workspace.agent_id,
    )


async def _save_profile_overrides(
    request: Request,
    name: str,
    overrides: dict[str, dict[str, Any]],
) -> None:
    """Persist profile overrides to config."""
    workspace = await get_agent_for_request(request)
    agent_config = load_agent_config(
        workspace.agent_id,
    )
    rc = agent_config.running or AgentsRunningConfig()
    spec = BUILTIN_PROFILES[name]

    gates = []
    for gs in spec.gates:
        ov = overrides.get(gs.type, {})
        gates.append(
            GateInstanceConfig(
                id=f"{name}-{gs.type}",
                type=gs.type,
                enabled=ov.get(
                    "enabled",
                    gs.default_enabled,
                ),
                priority=gs.priority,
                params={
                    **gs.default_params,
                    **ov.get("params", {}),
                },
            ),
        )

    new_profile = LoopProfileConfig(
        name=name,
        scope=spec.scope,
        is_builtin=True,
        description=spec.description,
        gates=gates,
    )

    existing = list(rc.loop.profiles or [])
    found = False
    for i, p in enumerate(existing):
        if p.name == name:
            existing[i] = new_profile
            found = True
            break
    if not found:
        existing.append(new_profile)

    rc.loop.profiles = existing

    if name == "default":
        _sync_legacy_loop_config(rc, overrides)

    agent_config.running = rc
    save_agent_config(
        workspace.agent_id,
        agent_config,
    )
    schedule_agent_reload(
        request,
        workspace.agent_id,
    )


def _sync_legacy_loop_config(
    rc: AgentsRunningConfig,
    overrides: dict[str, dict[str, Any]],
) -> None:
    """Sync default profile back to legacy."""
    if "iteration" in overrides:
        ov = overrides["iteration"]
        rc.loop.iteration.enabled = ov.get(
            "enabled",
            True,
        )
        p = ov.get("params", {})
        if "max_iterations" in p:
            rc.loop.iteration.max_iterations = p["max_iterations"]

    if "doom_loop" in overrides:
        ov = overrides["doom_loop"]
        rc.loop.doom_loop.enabled = ov.get(
            "enabled",
            True,
        )
        p = ov.get("params", {})
        for k in (
            "window_size",
            "similarity_threshold",
        ):
            if k in p:
                setattr(rc.loop.doom_loop, k, p[k])

    if "rubric" in overrides:
        ov = overrides["rubric"]
        rc.loop.rubric.enabled = ov.get(
            "enabled",
            False,
        )
        p = ov.get("params", {})
        for k in ("prompt", "max_interventions"):
            if k in p:
                setattr(rc.loop.rubric, k, p[k])


def _list_plugin_loops() -> list[dict[str, Any]]:
    """List loops registered by plugins."""
    result: list[dict[str, Any]] = []
    try:
        from ...plugins.registry import (
            PluginRegistry,
        )

        mgr = PluginRegistry().get_workspace_manager()
        if mgr is None:
            return result
        for ws in getattr(
            mgr,
            "workspaces",
            {},
        ).values():
            plugins = getattr(ws, "plugins", None)
            if plugins is None:
                continue
            for h in getattr(
                plugins,
                "stop_handlers",
                [],
            ):
                meta = getattr(h, "metadata", {})
                if meta.get("loop_name"):
                    result.append(
                        {
                            "name": meta["loop_name"],
                            "slash_command": meta.get(
                                "slash_command",
                                meta["loop_name"],
                            ),
                            "description": meta.get(
                                "description",
                                "",
                            ),
                            "source": "plugin",
                        },
                    )
    except Exception as exc:
        logger.warning(
            "Failed to list plugin loops: %s",
            exc,
        )
    return result
