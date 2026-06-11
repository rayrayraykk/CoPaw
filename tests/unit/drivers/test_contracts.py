# -*- coding: utf-8 -*-
import pytest

from qwenpaw.drivers.capabilities import (
    format_capability_id,
    parse_capability_id,
)
from qwenpaw.drivers.contracts import (
    CredentialRef,
    DriverCard,
    DriverPolicy,
    PolicyPrincipal,
    PolicyRule,
    PolicyTarget,
    coerce_card,
    validate_card,
)
from qwenpaw.drivers.errors import DriverCardError


def test_driver_card_minimal_valid_model() -> None:
    card = DriverCard(
        name="demo",
        protocol="custom/protocol",
        endpoint={"transport": "stdio", "command": "demo"},
    )

    validate_card(card)


def test_capability_id_uses_uri_shape_and_round_trips() -> None:
    capability_id = format_capability_id(
        "mcp",
        "file:server",
        "tool",
        "invoke",
        "read/file",
    )

    assert (
        capability_id == "driver://mcp/file%3Aserver/tools/read%2Ffile#invoke"
    )
    assert parse_capability_id(capability_id) == (
        "mcp",
        "file:server",
        "tool",
        "invoke",
        "read/file",
    )


def test_capability_id_parser_rejects_legacy_colon_shape() -> None:
    with pytest.raises(ValueError, match="Invalid Driver capability id"):
        parse_capability_id("driver:mcp:file%3Aserver:tool:invoke:read/file")


def test_validate_card_rejects_invalid_policy_effect() -> None:
    with pytest.raises(DriverCardError, match="invalid policy effect"):
        DriverCard(
            name="demo",
            protocol="mcp",
            endpoint={},
            policy=[
                PolicyRule(
                    subject="user:alice",
                    effect="maybe",  # type: ignore[arg-type]
                ),
            ],
        )


@pytest.mark.parametrize("rules", [{"tool": "allow"}, "allow"])
def test_validate_card_rejects_non_list_policy_rules(rules: object) -> None:
    with pytest.raises(DriverCardError, match="DriverPolicy.rules"):
        DriverCard(
            name="demo",
            protocol="mcp",
            endpoint={},
            policy={"rules": rules},  # type: ignore[arg-type]
        )


def test_validate_card_accepts_null_policy_rules_as_empty() -> None:
    card = DriverCard(
        name="demo",
        protocol="mcp",
        endpoint={},
        policy={"rules": None},  # type: ignore[arg-type]
    )

    assert card.policy == DriverPolicy()


def test_validate_card_rejects_invalid_policy_target_kind() -> None:
    card = DriverCard(
        name="demo",
        protocol="mcp",
        endpoint={},
        policy=[
            PolicyRule(
                subject="*",
                effect="ask",
                target=PolicyTarget(
                    kind="database",  # type: ignore[arg-type]
                    name="demo",
                ),
            ),
        ],
    )

    with pytest.raises(DriverCardError, match="target.kind"):
        validate_card(card)


def test_validate_card_rejects_user_principal_without_value() -> None:
    card = DriverCard(
        name="demo",
        protocol="mcp",
        endpoint={},
        policy=DriverPolicy(
            rules=[
                PolicyRule(
                    subject="*",
                    effect="allow",
                    target=PolicyTarget(kind="tool", name="echo"),
                    principal=PolicyPrincipal(
                        source_type="channel",
                        source_value="console",
                        subject_type="user",
                        subject_value="",
                    ),
                ),
            ],
        ),
    )

    with pytest.raises(DriverCardError, match="subject_value"):
        validate_card(card)


@pytest.mark.parametrize(
    "name",
    ["../escape", "..", "nested/name", "nested\\name", "bad\x00name"],
)
def test_validate_card_rejects_unsafe_name(name: str) -> None:
    card = DriverCard(
        name=name,
        protocol="mcp",
        endpoint={},
    )

    with pytest.raises(DriverCardError, match="DriverCard.name"):
        validate_card(card)


def test_validate_card_allows_dynamic_credential_kind() -> None:
    card = DriverCard(
        name="demo",
        protocol="mcp",
        endpoint={},
        credentials={
            "default": CredentialRef(kind="custom_provider", ref="demo"),
        },
    )

    validate_card(card)


def test_validate_card_accepts_public_and_secret_ref_binding() -> None:
    card = DriverCard(
        name="demo",
        protocol="mcp",
        endpoint={
            "transport": "streamable_http",
            "headers": {
                "public": {"X-Client": "qwenpaw"},
                "secret_refs": {"Authorization": "authorization"},
            },
        },
        credentials={"default": CredentialRef(kind="static", ref="mcp/demo")},
    )

    validate_card(card)


def test_validate_card_rejects_malformed_binding() -> None:
    card = DriverCard(
        name="demo",
        protocol="mcp",
        endpoint={
            "headers": {
                "public": {"X-Client": "qwenpaw"},
                "secret_refs": ["Authorization"],
            },
        },
        credentials={"default": CredentialRef(kind="static", ref="mcp/demo")},
    )

    with pytest.raises(DriverCardError, match="secret_refs"):
        validate_card(card)


def test_validate_card_does_not_enum_lock_protocol() -> None:
    card = DriverCard(
        name="demo",
        protocol="vendor.experimental/v1",
        endpoint={"url": "https://example.test"},
    )

    validate_card(card)


def test_validate_card_does_not_mutate_card() -> None:
    card = DriverCard(
        name="demo",
        protocol="mcp",
        endpoint={"transport": "stdio", "command": "demo"},
        policy=DriverPolicy(
            default_effect="ask",
            rules=[
                PolicyRule(
                    subject="user:alice",
                    effect="allow",
                    target=PolicyTarget(kind="tool", name="echo"),
                ),
            ],
        ),
    )
    before = DriverCard(
        name=card.name,
        protocol=card.protocol,
        endpoint=dict(card.endpoint),
        credentials=dict(card.credentials),
        config=dict(card.config),
        enabled=card.enabled,
        policy=DriverPolicy(
            default_effect=card.policy.default_effect,
            rules=list(card.policy.rules),
        ),
    )

    validate_card(card)

    assert card == before


def test_coerce_card_returns_normalized_copy_without_mutating_input() -> None:
    card = DriverCard(
        name="demo",
        protocol="mcp",
        endpoint={"transport": "stdio", "command": "demo"},
    )
    card.policy = [  # type: ignore[assignment]
        PolicyRule(subject="*", effect="allow"),
    ]

    normalized = coerce_card(card)

    assert isinstance(normalized.policy, DriverPolicy)
    assert isinstance(card.policy, list)
