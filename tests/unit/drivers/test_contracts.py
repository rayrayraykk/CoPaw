# -*- coding: utf-8 -*-
import pytest

from qwenpaw.drivers.capabilities import (
    format_capability_id,
    parse_capability_id,
)
from qwenpaw.drivers.contracts import (
    CredentialRef,
    DriverCard,
    PolicyRule,
    validate_card,
)
from qwenpaw.drivers.errors import DriverCardError


def test_driver_card_minimal_valid_model() -> None:
    card = DriverCard(
        name="demo",
        protocol="custom/protocol",
        endpoint={"transport": "stdio", "command": "demo"},
        credential=CredentialRef(kind="none"),
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
    card = DriverCard(
        name="demo",
        protocol="mcp",
        endpoint={},
        credential=CredentialRef(kind="none"),
        policy=[PolicyRule(subject="user:alice", effect="maybe")],
    )

    with pytest.raises(DriverCardError, match="invalid policy effect"):
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
        credential=CredentialRef(kind="none"),
    )

    with pytest.raises(DriverCardError, match="DriverCard.name"):
        validate_card(card)


def test_validate_card_allows_dynamic_credential_kind() -> None:
    card = DriverCard(
        name="demo",
        protocol="mcp",
        endpoint={},
        credential=CredentialRef(kind="custom_provider", ref="demo"),
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
        credential=CredentialRef(kind="static", ref="mcp/demo"),
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
        credential=CredentialRef(kind="static", ref="mcp/demo"),
    )

    with pytest.raises(DriverCardError, match="secret_refs"):
        validate_card(card)


def test_validate_card_does_not_enum_lock_protocol() -> None:
    card = DriverCard(
        name="demo",
        protocol="vendor.experimental/v1",
        endpoint={"url": "https://example.test"},
        credential=CredentialRef(kind="none"),
    )

    validate_card(card)
