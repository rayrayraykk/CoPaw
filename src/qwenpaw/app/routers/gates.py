# -*- coding: utf-8 -*-
"""Gate catalog API.

Endpoints:
    GET /api/gates/catalog — list gate types
"""
from __future__ import annotations

from typing import Any

from fastapi import APIRouter

from ...loop.gate_catalog import GateCatalog

router = APIRouter(prefix="/gates", tags=["gates"])


@router.get("/catalog")
async def get_gate_catalog() -> dict[str, Any]:
    """Return all available gate types."""
    catalog = GateCatalog()
    catalog.ensure_builtins()
    return {"gates": catalog.to_api_response()}
