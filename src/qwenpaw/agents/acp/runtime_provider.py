# -*- coding: utf-8 -*-
"""Ephemeral model providers for headless ACP runtimes."""

from __future__ import annotations

import json
import os
from dataclasses import dataclass
from typing import Any, Mapping
from urllib.parse import urlparse

from ...config.config import ModelSlotConfig
from ...providers.openai_provider import OpenAIProvider
from ...providers.provider import ModelInfo

RUNTIME_OPENAI_PROVIDER_ID = "runtime-openai"
HARBOR_MODEL_INFO_ENV = "HARBOR_MODEL_INFO_JSON"


def _optional_positive_int(
    model_info: Mapping[str, Any],
    name: str,
) -> int | None:
    """Parse one optional positive integer from Harbor model metadata."""
    value = model_info.get(name)
    if value is None:
        return None
    if isinstance(value, bool):
        raise ValueError(
            f"{HARBOR_MODEL_INFO_ENV}.{name} must be a positive integer",
        )
    try:
        parsed = int(value)
    except (TypeError, ValueError) as exc:
        raise ValueError(
            f"{HARBOR_MODEL_INFO_ENV}.{name} must be a positive integer",
        ) from exc
    if parsed <= 0 or str(value).strip() not in {str(parsed), f"{parsed}.0"}:
        raise ValueError(
            f"{HARBOR_MODEL_INFO_ENV}.{name} must be a positive integer",
        )
    return parsed


def _load_harbor_model_info(source: Mapping[str, str]) -> dict[str, Any]:
    """Load optional Harbor model metadata without accepting other shapes."""
    raw = str(source.get(HARBOR_MODEL_INFO_ENV, "")).strip()
    if not raw:
        return {}
    try:
        model_info = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise ValueError(
            f"{HARBOR_MODEL_INFO_ENV} must be a JSON object",
        ) from exc
    if not isinstance(model_info, dict):
        raise ValueError(f"{HARBOR_MODEL_INFO_ENV} must be a JSON object")
    return model_info


@dataclass(frozen=True)
class OpenAIRuntimeProviderConfig:
    """One process-scoped OpenAI-compatible model connection."""

    base_url: str
    api_key: str
    model: str
    max_input_tokens: int | None = None
    max_output_tokens: int | None = None

    @classmethod
    def from_env(
        cls,
        environ: Mapping[str, str] | None = None,
    ) -> "OpenAIRuntimeProviderConfig":
        """Load and validate the runtime provider environment."""
        source = os.environ if environ is None else environ
        names = (
            "OPENAI_BASE_URL",
            "OPENAI_API_KEY",
            "OPENAI_MODEL",
        )
        values = {name: str(source.get(name, "")).strip() for name in names}
        missing = [name for name, value in values.items() if not value]
        if missing:
            missing_text = ", ".join(missing)
            raise ValueError(
                f"Missing runtime provider environment: {missing_text}",
            )

        base_url = values["OPENAI_BASE_URL"].rstrip("/")
        parsed = urlparse(base_url)
        if parsed.scheme not in {"http", "https"} or not parsed.netloc:
            raise ValueError(
                "OPENAI_BASE_URL must be an absolute HTTP(S) URL",
            )

        model_info = _load_harbor_model_info(source)

        return cls(
            base_url=base_url,
            api_key=values["OPENAI_API_KEY"],
            model=values["OPENAI_MODEL"],
            max_input_tokens=_optional_positive_int(
                model_info,
                "max_input_tokens",
            ),
            max_output_tokens=_optional_positive_int(
                model_info,
                "max_output_tokens",
            ),
        )

    @property
    def model_slot(self) -> ModelSlotConfig:
        """Return the per-request model selection."""
        return ModelSlotConfig(
            provider_id=RUNTIME_OPENAI_PROVIDER_ID,
            model=self.model,
        )

    def build_provider(self) -> OpenAIProvider:
        """Create the in-memory provider without writing credentials."""
        model_kwargs: dict[str, Any] = {}
        if self.max_input_tokens is not None:
            model_kwargs.update(
                {
                    "max_input_length": self.max_input_tokens,
                    "max_input_length_configured": True,
                },
            )
        if self.max_output_tokens is not None:
            model_kwargs["max_tokens"] = self.max_output_tokens
        return OpenAIProvider(
            id=RUNTIME_OPENAI_PROVIDER_ID,
            name="ACP Runtime OpenAI",
            base_url=self.base_url,
            api_key=self.api_key,
            models=[
                ModelInfo(
                    id=self.model,
                    name=self.model,
                    **model_kwargs,
                ),
            ],
            require_api_key=True,
            support_connection_check=False,
            support_model_discovery=False,
            is_custom=True,
        )
