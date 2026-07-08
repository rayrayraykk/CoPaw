# -*- coding: utf-8 -*-
"""Gate Catalog — registry of available gate types.

Maps gate_type string -> factory + JSON Schema for
front-end driven parameter editing.

Usage:
    catalog = GateCatalog()
    catalog.register(entry)
    gate = catalog.create("iteration", {"max_iterations": 50})
"""
from __future__ import annotations

import logging
from dataclasses import dataclass, field
from typing import Any, Callable, Dict, List

from .gates.base import StopGate

logger = logging.getLogger(__name__)


@dataclass(frozen=True)
class GateCatalogEntry:
    """Metadata + factory for one gate type."""

    type: str
    name: str
    description: str
    category: str
    default_priority: int
    params_schema: Dict[str, Any]
    factory: Callable[..., StopGate]
    scope_hints: List[str] = field(
        default_factory=list,
    )
    builtin_in: List[str] = field(
        default_factory=list,
    )


class GateCatalog:
    """Singleton registry of gate types."""

    _instance: GateCatalog | None = None
    _entries: Dict[str, GateCatalogEntry]
    _initialized: bool

    def __new__(cls) -> GateCatalog:
        if cls._instance is None:
            obj = super().__new__(cls)
            obj._entries = {}
            obj._initialized = False
            cls._instance = obj
        return cls._instance

    def register(self, entry: GateCatalogEntry) -> None:
        """Register a gate type."""
        self._entries[entry.type] = entry
        logger.debug(
            "GateCatalog: registered '%s'",
            entry.type,
        )

    def unregister(self, gate_type: str) -> None:
        """Remove a gate type."""
        self._entries.pop(gate_type, None)

    def get(
        self,
        gate_type: str,
    ) -> GateCatalogEntry | None:
        """Lookup entry by type."""
        return self._entries.get(gate_type)

    def list_entries(self) -> list[GateCatalogEntry]:
        """Return all registered entries."""
        return list(self._entries.values())

    def create(
        self,
        gate_type: str,
        params: dict[str, Any] | None = None,
    ) -> StopGate:
        """Instantiate a gate from catalog.

        Raises:
            KeyError: unknown gate_type.
        """
        entry = self._entries.get(gate_type)
        if entry is None:
            raise KeyError(
                f"Unknown gate type: {gate_type}",
            )
        return entry.factory(params or {})

    def ensure_builtins(self) -> None:
        """Register built-in gates (idempotent)."""
        if self._initialized:
            return
        self._initialized = True
        _register_builtin_gates(self)

    def to_api_response(
        self,
    ) -> list[dict[str, Any]]:
        """Serialize catalog for API response."""
        self.ensure_builtins()
        result = []
        for e in self._entries.values():
            result.append(
                {
                    "type": e.type,
                    "name": e.name,
                    "description": e.description,
                    "category": e.category,
                    "default_priority": e.default_priority,
                    "scope_hints": e.scope_hints,
                    "builtin_in": e.builtin_in,
                    "params_schema": e.params_schema,
                },
            )
        return result

    @classmethod
    def _reset(cls) -> None:
        """Reset singleton — test-only, not for production."""
        cls._instance = None


def _make_iteration_gate(
    params: dict[str, Any],
) -> StopGate:
    """Factory for IterationGate."""
    from .gates.iteration import IterationGate

    gate = IterationGate(
        max_iterations=params.get(
            "max_iterations",
            100,
        ),
    )
    gate.activate()
    return gate


def _make_doom_loop_gate(
    params: dict[str, Any],
) -> StopGate:
    """Factory for DoomLoopGate."""
    from .gates.doom_loop import DoomLoopGate
    from ..config.config import DoomLoopStageConfig

    raw_stages = params.get("stages", [])
    stages = []
    for s in raw_stages:
        if isinstance(s, dict):
            stages.append(DoomLoopStageConfig(**s))
        else:
            stages.append(s)
    gate = DoomLoopGate(
        window_size=params.get("window_size", 3),
        similarity_threshold=params.get(
            "similarity_threshold",
            1.0,
        ),
        stages=stages,
    )
    gate.activate()
    return gate


def _make_rubric_gate(
    params: dict[str, Any],
) -> StopGate:
    """Factory for StandaloneRubricGate."""
    from .gates.rubric import StandaloneRubricGate

    return StandaloneRubricGate(
        prompt=params.get("prompt", ""),
        max_interventions=params.get(
            "max_interventions",
            1,
        ),
    )


def _make_budget_gate(
    params: dict[str, Any],
) -> StopGate:
    """Factory for BudgetGate."""
    from .gates.budget import BudgetGate

    gate = BudgetGate(
        max_tokens=params.get(
            "max_tokens",
            300_000,
        ),
    )
    gate.activate()
    return gate


def _register_builtin_gates(
    catalog: GateCatalog,
) -> None:
    """Register all built-in gate types."""
    catalog.register(
        GateCatalogEntry(
            type="iteration",
            name="Iteration Limit",
            description=(
                "Stop the agent after a fixed number" " of loop turns"
            ),
            category="budget",
            default_priority=10,
            scope_hints=["default", "goal"],
            builtin_in=["default"],
            params_schema={
                "type": "object",
                "properties": {
                    "max_iterations": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 500,
                        "default": 100,
                        "description": (
                            "Maximum loop turns" " before stopping"
                        ),
                    },
                },
            },
            factory=_make_iteration_gate,
        ),
    )

    catalog.register(
        GateCatalogEntry(
            type="doom_loop",
            name="Repetition Protection",
            description=(
                "Detect and escalate when the agent" " repeats similar actions"
            ),
            category="safety",
            default_priority=5,
            scope_hints=[
                "default",
                "goal",
                "mission",
            ],
            builtin_in=["default"],
            params_schema={
                "type": "object",
                "properties": {
                    "window_size": {
                        "type": "integer",
                        "minimum": 2,
                        "maximum": 20,
                        "default": 3,
                    },
                    "similarity_threshold": {
                        "type": "number",
                        "minimum": 0,
                        "maximum": 1,
                        "default": 1.0,
                    },
                    "stages": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "after": {
                                    "type": "integer",
                                    "minimum": 1,
                                },
                                "action": {
                                    "type": "string",
                                    "enum": [
                                        "modify_prompt",
                                        "stop",
                                    ],
                                },
                                "prompt": {
                                    "type": "string",
                                },
                            },
                        },
                    },
                },
            },
            factory=_make_doom_loop_gate,
        ),
    )

    catalog.register(
        GateCatalogEntry(
            type="rubric",
            name="Completion Check",
            description=(
                "Re-prompt the agent when it produces"
                " a text-only response without tool"
                " calls"
            ),
            category="completion",
            default_priority=90,
            scope_hints=["default"],
            builtin_in=["default"],
            params_schema={
                "type": "object",
                "properties": {
                    "prompt": {
                        "type": "string",
                        "default": (
                            "You did not call any tool."
                            " If the task is complete,"
                            " confirm. Otherwise,"
                            " continue with tool calls."
                        ),
                    },
                    "max_interventions": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 10,
                        "default": 1,
                    },
                },
            },
            factory=_make_rubric_gate,
        ),
    )

    catalog.register(
        GateCatalogEntry(
            type="budget",
            name="Token Budget",
            description=("Stop the agent when token budget" " is exceeded"),
            category="budget",
            default_priority=20,
            scope_hints=["goal"],
            builtin_in=["goal"],
            params_schema={
                "type": "object",
                "properties": {
                    "max_tokens": {
                        "type": "integer",
                        "minimum": 1000,
                        "maximum": 10_000_000,
                        "default": 300_000,
                    },
                },
            },
            factory=_make_budget_gate,
        ),
    )


__all__ = [
    "GateCatalog",
    "GateCatalogEntry",
]
