# -*- coding: utf-8 -*-
"""Built-in loop profiles — code-level fixed structure.

Users can change params and enabled state but cannot
add, remove, or reorder gates in built-in profiles.
Only custom profiles allow full pipeline editing.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Dict, List


@dataclass
class GateSpec:
    """One gate slot in a built-in profile."""

    type: str
    priority: int
    default_params: Dict[str, Any] = field(
        default_factory=dict,
    )
    default_enabled: bool = True


@dataclass
class BuiltinProfileSpec:
    """Definition of a built-in profile."""

    name: str
    scope: str
    description: str
    gates: List[GateSpec]


_WARN_REPETITIVE = (
    "[WARNING] Repetitive pattern detected."
    " Try a completely different approach."
)
_WARN_REPETITIVE_SHORT = (
    "[WARNING] Repetitive pattern."
    " Try a different approach."  # pylint: disable=implicit-str-concat
)
_WARN_MISSION = "[WARNING] Repetitive pattern in mission."
_STOP_DOOM = "Doom loop: stuck after {n} repetitions"
_STOP_GOAL = "Doom loop during goal execution"
_STOP_MISSION = "Doom loop during mission execution"
_RUBRIC_PROMPT = (
    "You did not call any tool."
    " If the task is complete, confirm."
    " Otherwise, continue with tool calls."
)

BUILTIN_PROFILES: dict[str, BuiltinProfileSpec] = {
    "default": BuiltinProfileSpec(
        name="default",
        scope="default",
        description=(
            "Standard ReAct loop with iteration"
            " limit, repetition protection, and"
            " completion check"
        ),
        gates=[
            GateSpec(
                type="doom_loop",
                priority=5,
                default_params={
                    "window_size": 3,
                    "similarity_threshold": 1.0,
                    "stages": [
                        {
                            "after": 3,
                            "action": "modify_prompt",
                            "prompt": _WARN_REPETITIVE,
                        },
                        {
                            "after": 6,
                            "action": "stop",
                            "prompt": _STOP_DOOM.format(
                                n=6,
                            ),
                        },
                    ],
                },
            ),
            GateSpec(
                type="iteration",
                priority=10,
                default_params={
                    "max_iterations": 100,
                },
            ),
            GateSpec(
                type="rubric",
                priority=90,
                default_params={
                    "prompt": _RUBRIC_PROMPT,
                    "max_interventions": 1,
                },
                default_enabled=False,
            ),
        ],
    ),
    "goal": BuiltinProfileSpec(
        name="goal",
        scope="goal",
        description=(
            "Goal mode: agent loops until the"
            " goal is achieved or limits reached"
        ),
        gates=[
            GateSpec(
                type="iteration",
                priority=10,
                default_params={
                    "max_iterations": 20,
                },
            ),
            GateSpec(
                type="budget",
                priority=20,
                default_params={
                    "max_tokens": 300_000,
                },
            ),
            GateSpec(
                type="doom_loop",
                priority=5,
                default_params={
                    "window_size": 3,
                    "similarity_threshold": 1.0,
                    "stages": [
                        {
                            "after": 3,
                            "action": "modify_prompt",
                            "prompt": _WARN_REPETITIVE_SHORT,
                        },
                        {
                            "after": 6,
                            "action": "stop",
                            "prompt": _STOP_GOAL,
                        },
                    ],
                },
                default_enabled=False,
            ),
        ],
    ),
    "mission": BuiltinProfileSpec(
        name="mission",
        scope="mission",
        description=(
            "Mission mode: decomposes complex"
            " tasks into sub-tasks with PRD"
            " tracking"
        ),
        gates=[
            GateSpec(
                type="iteration",
                priority=10,
                default_params={
                    "max_iterations": 30,
                },
            ),
            GateSpec(
                type="doom_loop",
                priority=5,
                default_params={
                    "window_size": 3,
                    "similarity_threshold": 1.0,
                    "stages": [
                        {
                            "after": 4,
                            "action": "modify_prompt",
                            "prompt": _WARN_MISSION,
                        },
                        {
                            "after": 8,
                            "action": "stop",
                            "prompt": _STOP_MISSION,
                        },
                    ],
                },
                default_enabled=False,
            ),
        ],
    ),
}


def get_builtin_profile_names() -> list[str]:
    """Return names of all built-in profiles."""
    return list(BUILTIN_PROFILES.keys())


def is_builtin_profile(name: str) -> bool:
    """Check if a profile name is built-in."""
    return name in BUILTIN_PROFILES


__all__ = [
    "BUILTIN_PROFILES",
    "BuiltinProfileSpec",
    "GateSpec",
    "get_builtin_profile_names",
    "is_builtin_profile",
]
