# -*- coding: utf-8 -*-
from __future__ import annotations

import base64

import pytest

from qwenpaw.drivers.credentials.bindings import (
    implicit_auth_headers,
    lookup_credential_value,
    resolve_binding,
    resolve_credentials,
)
from qwenpaw.drivers.credentials.providers import CredentialProvider
from qwenpaw.drivers.credentials.types import ResolvedCredential


def _credentials() -> dict[str, ResolvedCredential]:
    return {
        "static": ResolvedCredential(
            kind="static",
            public={"tenant": "public-tenant"},
            secrets={"api_key": "secret-key"},
        ),
        "oauth": ResolvedCredential(
            kind="oauth2_auth_code",
            secrets={"access_token": "oauth-token"},
        ),
    }


def test_resolve_binding_supports_literal_and_credential_sources() -> None:
    binding = {
        "PUBLIC_TENANT": {
            "source": "credential",
            "credential": "static",
            "field": "tenant",
        },
        "API_KEY": {
            "source": "credential",
            "credential": "static",
            "field": "api_key",
            "format": "Bearer {value}",
        },
        "MODE": {"source": "literal", "value": "debug"},
        "IGNORED": {"source": "missing"},
    }

    assert resolve_binding(binding, _credentials()) == {
        "PUBLIC_TENANT": "public-tenant",
        "API_KEY": "Bearer secret-key",
        "MODE": "debug",
    }


def test_resolve_binding_keeps_legacy_public_and_secret_refs() -> None:
    binding = {
        "public": {"MODE": "debug"},
        "secret_refs": {"API_KEY": "static.api_key"},
    }

    assert resolve_binding(binding, _credentials()) == {
        "MODE": "debug",
        "API_KEY": "secret-key",
    }


def test_lookup_credential_value_prefers_static_for_bare_reference() -> None:
    assert lookup_credential_value(_credentials(), "api_key") == "secret-key"
    assert (
        lookup_credential_value(_credentials(), "oauth.access_token")
        == "oauth-token"
    )
    assert lookup_credential_value(_credentials(), "missing") is None


def test_implicit_auth_headers_prefers_oauth_alias() -> None:
    assert implicit_auth_headers(_credentials(), {}) == {
        "Authorization": "Bearer oauth-token",
    }


def test_implicit_auth_headers_respects_existing_authorization() -> None:
    assert (
        implicit_auth_headers(
            _credentials(),
            {"authorization": "Bearer explicit-token"},
        )
        == {}
    )


def test_implicit_auth_headers_can_build_basic_auth() -> None:
    encoded = base64.b64encode(b"alice:secret").decode("ascii")
    credentials = {
        "default": ResolvedCredential(
            kind="basic_auth",
            public={"username": "alice"},
            secrets={"password": "secret"},
        ),
    }

    assert implicit_auth_headers(credentials, {}) == {
        "Authorization": f"Basic {encoded}",
    }


class StaticProvider(CredentialProvider):
    def __init__(self, credential: ResolvedCredential) -> None:
        self.credential = credential

    async def resolve(self) -> ResolvedCredential:
        return self.credential


@pytest.mark.asyncio
async def test_resolve_credentials_adds_default_alias_for_single_provider():
    credential = ResolvedCredential(kind="static", secrets={"token": "abc"})

    resolved = await resolve_credentials(
        {"static": StaticProvider(credential)},
    )

    assert resolved["static"] is credential
    assert resolved["default"] is credential
