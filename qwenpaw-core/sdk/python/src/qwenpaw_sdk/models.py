from __future__ import annotations

from dataclasses import dataclass
from typing import Any

JsonObject = dict[str, Any]


@dataclass(frozen=True, slots=True)
class Notification:
    """One App Protocol server notification."""

    method: str
    params: JsonObject


@dataclass(frozen=True, slots=True)
class TurnResult:
    """Final result collected from one streamed turn."""

    final_response: str
    turn: JsonObject
    items: tuple[JsonObject, ...]
