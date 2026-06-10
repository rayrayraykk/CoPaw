# -*- coding: utf-8 -*-
"""Driver card and policy data contracts."""

from __future__ import annotations

from collections.abc import Iterator
from dataclasses import dataclass, field
from typing import Any

from qwenpaw.drivers.errors import DriverCardError

ALLOWED_POLICY_EFFECTS: frozenset[str] = frozenset(
    {"allow", "deny", "ask"},
)


def validate_card_name(name: str) -> None:
    """Reject Driver names that cannot safely be used as storage keys."""
    if not name or not isinstance(name, str):
        raise DriverCardError("DriverCard.name must be a non-empty string")
    if "\x00" in name or "/" in name or "\\" in name or ".." in name:
        raise DriverCardError(
            "DriverCard.name must not contain path separators, null bytes, "
            "or '..'",
        )
    if name in {".", ".."}:
        raise DriverCardError("DriverCard.name must be a safe file name")


@dataclass(frozen=True)
class CredentialRef:
    """Reference from DriverCard to a credential source."""

    kind: str
    ref: str = ""


def coerce_credential_ref(value: Any) -> CredentialRef:
    """Normalize a loose credential mapping into ``CredentialRef``."""
    if isinstance(value, CredentialRef):
        return value
    if isinstance(value, dict):
        return CredentialRef(
            kind=str(value.get("kind") or ""),
            ref=str(value.get("ref") or ""),
        )
    if value is None:
        return CredentialRef(kind="none")
    return CredentialRef(
        kind=str(getattr(value, "kind", "") or ""),
        ref=str(getattr(value, "ref", "") or ""),
    )


def coerce_credential_refs(value: Any) -> dict[str, CredentialRef]:
    """Normalize a mapping of credential aliases to credential refs."""
    if value is None:
        return {}
    if not isinstance(value, dict):
        return {}
    result: dict[str, CredentialRef] = {}
    for alias, raw_ref in value.items():
        alias_str = str(alias)
        if not alias_str:
            continue
        ref = coerce_credential_ref(raw_ref)
        if ref.kind and ref.kind != "none":
            result[alias_str] = ref
    return result


@dataclass
class TimeRange:
    after: str | None = None
    before: str | None = None
    weekdays: list[int] | None = None


@dataclass
class RateLimit:
    max_calls: int
    window_seconds: int


@dataclass
class PolicyCondition:
    time_range: TimeRange | None = None
    rate_limit: RateLimit | None = None


@dataclass
class PolicyTarget:
    kind: str = "*"
    name: str = "*"


@dataclass
class PolicyPrincipal:
    """Structured caller selector for Driver policy rules."""

    source_type: str = "*"
    source_value: str = "*"
    subject_type: str = "*"
    subject_value: str = "*"


@dataclass
class PolicyRule:
    subject: str = "*"
    effect: str = "ask"
    target: PolicyTarget = field(default_factory=PolicyTarget)
    principal: PolicyPrincipal = field(default_factory=PolicyPrincipal)
    condition: PolicyCondition | None = None


@dataclass
class DriverPolicy:
    default_effect: str = "deny"
    rules: list[PolicyRule] = field(default_factory=list)

    def __iter__(self) -> Iterator[PolicyRule]:
        return iter(self.rules)

    def __getitem__(self, index: int) -> PolicyRule:
        return self.rules[index]

    def __len__(self) -> int:
        return len(self.rules)


@dataclass
class DriverCard:
    name: str
    protocol: str
    endpoint: dict[str, Any]
    credential: CredentialRef = field(
        default_factory=lambda: CredentialRef("none"),
    )
    credentials: dict[str, CredentialRef] = field(default_factory=dict)
    config: dict[str, Any] = field(default_factory=dict)
    enabled: bool = True
    policy: DriverPolicy = field(default_factory=DriverPolicy)

    def __post_init__(self) -> None:
        self.policy = coerce_driver_policy(self.policy)
        self.credential = coerce_credential_ref(self.credential)
        self.credentials = coerce_credential_refs(self.credentials)
        if not self.credentials and self.credential.kind != "none":
            self.credentials = {"default": self.credential}
        if self.credential.kind == "none" and self.credentials:
            self.credential = self.credentials.get("default") or next(
                iter(self.credentials.values()),
            )


def iter_credential_refs(card: DriverCard) -> dict[str, CredentialRef]:
    """Return the effective credential refs declared by a DriverCard."""
    if card.credentials:
        return dict(card.credentials)
    if card.credential.kind != "none":
        return {"default": card.credential}
    return {}


def coerce_driver_policy(value: Any) -> DriverPolicy:
    """Normalize legacy list policies into the DriverPolicy shape."""
    if isinstance(value, DriverPolicy):
        return value
    if value is None:
        return DriverPolicy()
    if isinstance(value, list):
        return DriverPolicy(
            default_effect="deny",
            rules=[_coerce_policy_rule(item) for item in value],
        )
    if isinstance(value, dict):
        return DriverPolicy(
            default_effect=str(value.get("default_effect") or "deny"),
            rules=[
                _coerce_policy_rule(item)
                for item in list(value.get("rules") or [])
            ],
        )
    return DriverPolicy()


def _coerce_policy_rule(value: Any) -> PolicyRule:
    if isinstance(value, PolicyRule):
        value.target = _coerce_policy_target(value.target)
        value.principal = _coerce_policy_principal(
            getattr(value, "principal", None),
        )
        return value
    if isinstance(value, dict):
        return PolicyRule(
            subject=str(value.get("subject") or "*"),
            effect=str(value.get("effect") or "ask"),
            target=_coerce_policy_target(value.get("target")),
            principal=_coerce_policy_principal(value.get("principal")),
            condition=value.get("condition"),
        )
    return PolicyRule()


def _coerce_policy_target(value: Any) -> PolicyTarget:
    if isinstance(value, PolicyTarget):
        return value
    if isinstance(value, dict):
        return PolicyTarget(
            kind=str(value.get("kind") or "*"),
            name=str(value.get("name") or "*"),
        )
    return PolicyTarget()


def _coerce_policy_principal(value: Any) -> PolicyPrincipal:
    if isinstance(value, PolicyPrincipal):
        return value
    if isinstance(value, dict):
        return PolicyPrincipal(
            source_type=str(value.get("source_type") or "*"),
            source_value=str(value.get("source_value") or "*"),
            subject_type=str(value.get("subject_type") or "*"),
            subject_value=str(value.get("subject_value", "*")),
        )
    return PolicyPrincipal()


def validate_card(card: DriverCard) -> None:
    """Validate the public DriverCard contract."""
    _validate_card_identity(card)
    card.policy = coerce_driver_policy(card.policy)
    _normalize_card_credentials(card)
    _validate_card_credentials(card)
    _validate_driver_policy(card)
    _validate_endpoint_bindings(card)


def _validate_card_identity(card: DriverCard) -> None:
    validate_card_name(card.name)
    if not card.protocol or not isinstance(card.protocol, str):
        raise DriverCardError(
            f"DriverCard.protocol must be non-empty for {card.name}",
        )
    if not isinstance(card.endpoint, dict):
        raise DriverCardError(
            f"DriverCard.endpoint must be a mapping for {card.name}",
        )
    if not isinstance(card.config, dict):
        raise DriverCardError(
            f"DriverCard.config must be a mapping for {card.name}",
        )


def _normalize_card_credentials(card: DriverCard) -> None:
    card.credential = coerce_credential_ref(card.credential)
    card.credentials = coerce_credential_refs(card.credentials)
    if not card.credentials and card.credential.kind != "none":
        card.credentials = {"default": card.credential}
    if card.credential.kind == "none" and card.credentials:
        card.credential = card.credentials.get("default") or next(
            iter(card.credentials.values()),
        )


def _validate_card_credentials(card: DriverCard) -> None:
    if not card.credential.kind or not isinstance(card.credential.kind, str):
        raise DriverCardError(
            f"DriverCard {card.name} credential.kind must be non-empty",
        )
    for alias, credential_ref in card.credentials.items():
        if not alias or not isinstance(alias, str):
            raise DriverCardError(
                f"DriverCard {card.name} credentials aliases must be "
                "non-empty strings",
            )
        if not credential_ref.kind or not isinstance(credential_ref.kind, str):
            raise DriverCardError(
                f"DriverCard {card.name} credentials.{alias}.kind must be "
                "non-empty",
            )


def _validate_driver_policy(card: DriverCard) -> None:
    if card.policy.default_effect not in ALLOWED_POLICY_EFFECTS:
        raise DriverCardError(
            f"DriverCard {card.name} has invalid default policy effect: "
            f"{card.policy.default_effect}",
        )

    for rule in card.policy.rules:
        if rule.effect not in ALLOWED_POLICY_EFFECTS:
            raise DriverCardError(
                f"DriverCard {card.name} has invalid policy effect: "
                f"{rule.effect}",
            )
        if not rule.target.kind or not isinstance(rule.target.kind, str):
            raise DriverCardError(
                f"DriverCard {card.name} policy target.kind must be non-empty",
            )
        if not rule.target.name or not isinstance(rule.target.name, str):
            raise DriverCardError(
                f"DriverCard {card.name} policy target.name must be non-empty",
            )
        for field_name in (
            "source_type",
            "source_value",
            "subject_type",
            "subject_value",
        ):
            value = getattr(rule.principal, field_name)
            if not isinstance(value, str):
                raise DriverCardError(
                    f"DriverCard {card.name} policy principal."
                    f"{field_name} must be a string",
                )
        if (
            rule.principal.subject_type.strip().lower() == "user"
            and not rule.principal.subject_value.strip()
        ):
            raise DriverCardError(
                f"DriverCard {card.name} policy principal.subject_value "
                "must be non-empty when subject_type is user",
            )


def _validate_endpoint_bindings(card: DriverCard) -> None:
    # Binding sections keep the DriverCard secret-free: public values are
    # literals, while secret_refs point into CredentialRecord.secrets.
    for section_name in ("env", "headers"):
        section = card.endpoint.get(section_name)
        if section is None:
            continue
        if not isinstance(section, dict):
            raise DriverCardError(
                f"DriverCard {card.name} endpoint.{section_name} "
                "must be a mapping",
            )
        if "public" not in section and "secret_refs" not in section:
            _validate_value_source_bindings(card, section_name, section)
            continue
        _validate_binding_mapping(
            card.name,
            section_name,
            "public",
            section.get("public", {}),
        )
        _validate_binding_mapping(
            card.name,
            section_name,
            "secret_refs",
            section.get("secret_refs", {}),
        )


def _validate_value_source_bindings(
    card: DriverCard,
    section_name: str,
    section: dict[str, Any],
) -> None:
    aliases = set(iter_credential_refs(card))
    for output_name, spec in section.items():
        if isinstance(spec, dict) and "source" in spec:
            source = str(spec.get("source") or "")
            if source == "literal":
                continue
            if source != "credential":
                raise DriverCardError(
                    f"DriverCard {card.name} endpoint.{section_name}."
                    f"{output_name} "
                    f"has invalid source: {source}",
                )
            alias = str(spec.get("credential") or "")
            field_name = str(spec.get("field") or "")
            if not alias:
                raise DriverCardError(
                    f"DriverCard {card.name} endpoint.{section_name}."
                    f"{output_name} "
                    "credential source must name a credential alias",
                )
            if alias not in aliases:
                raise DriverCardError(
                    f"DriverCard {card.name} endpoint.{section_name}."
                    f"{output_name} "
                    f"references unknown credential alias: {alias}",
                )
            if not field_name:
                raise DriverCardError(
                    f"DriverCard {card.name} endpoint.{section_name}."
                    f"{output_name} "
                    "credential source must name a field",
                )
            fmt = spec.get("format")
            if fmt is not None and not isinstance(fmt, str):
                raise DriverCardError(
                    f"DriverCard {card.name} endpoint.{section_name}."
                    f"{output_name} "
                    "format must be a string",
                )


def _validate_binding_mapping(
    card_name: str,
    section_name: str,
    field_name: str,
    value: Any,
) -> None:
    if value is None:
        return
    if not isinstance(value, dict):
        raise DriverCardError(
            f"DriverCard {card_name} endpoint.{section_name}.{field_name} "
            "must be a mapping",
        )
    for key, item in value.items():
        if not isinstance(key, str) or not key:
            raise DriverCardError(
                f"DriverCard {card_name} endpoint.{section_name}."
                f"{field_name} keys must be non-empty strings",
            )
        if not isinstance(item, str):
            raise DriverCardError(
                f"DriverCard {card_name} endpoint.{section_name}."
                f"{field_name}.{key} must be a string",
            )
