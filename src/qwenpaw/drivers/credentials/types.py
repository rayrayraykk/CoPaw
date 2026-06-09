"""Runtime credential value types."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, ClassVar


@dataclass(frozen=True)
class CredentialRecord:
    ref: str
    kind: str
    public: dict[str, Any] = field(default_factory=dict)
    secrets: dict[str, Any] = field(default_factory=dict)
    meta: dict[str, Any] = field(default_factory=dict)

    @property
    def values(self) -> dict[str, Any]:
        return {**self.public, **self.secrets}


@dataclass(frozen=True)
class ResolvedCredential:
    kind: str = "none"
    public: dict[str, Any] = field(default_factory=dict)
    secrets: dict[str, Any] = field(default_factory=dict)
    meta: dict[str, Any] = field(default_factory=dict)

    EMPTY: ClassVar["ResolvedCredential"]

    @property
    def values(self) -> dict[str, Any]:
        return {**self.public, **self.secrets}


ResolvedCredential.EMPTY = ResolvedCredential()
