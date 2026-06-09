# -*- coding: utf-8 -*-
"""API endpoints for Plan mode configuration."""

from __future__ import annotations

from fastapi import APIRouter, Body, Request

from qwenpaw.app.utils import schedule_agent_reload
from qwenpaw.config.config import PlanConfig, save_agent_config

router = APIRouter(prefix="/plan", tags=["plan"])


@router.get(
    "/config",
    response_model=PlanConfig,
    summary="Get plan config",
)
async def get_plan_config(request: Request) -> PlanConfig:
    """Return Plan mode config for the current agent."""
    from qwenpaw.app.agent_context import get_agent_for_request

    agent = await get_agent_for_request(request)
    return agent.config.plan or PlanConfig()


@router.put(
    "/config",
    response_model=PlanConfig,
    summary="Update plan config",
)
async def put_plan_config(
    request: Request,
    plan_config: PlanConfig = Body(...),
) -> PlanConfig:
    """Update Plan mode config for the current agent."""
    from qwenpaw.app.agent_context import get_agent_for_request

    agent = await get_agent_for_request(request)
    agent.config.plan = plan_config
    save_agent_config(agent.agent_id, agent.config)
    schedule_agent_reload(request, agent.agent_id)
    return agent.config.plan


@router.get(
    "/current",
    summary="Get current plan",
)
async def get_current_plan() -> dict | None:
    """Return the current runtime plan if one is available."""
    return None
