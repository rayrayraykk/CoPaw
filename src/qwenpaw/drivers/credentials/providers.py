# -*- coding: utf-8 -*-
"""CredentialProvider strategies and factory."""

from __future__ import annotations

import asyncio
import base64
import hashlib
import hmac
import time
from abc import ABC, abstractmethod
from collections.abc import Callable
from typing import Any, Protocol

import httpx

from qwenpaw.drivers.credentials.store import CredentialStore
from qwenpaw.drivers.credentials.types import (
    CredentialRecord,
    ResolvedCredential,
)
from qwenpaw.drivers.errors import (
    DriverCredentialProviderError,
    OAuthRequiredError,
    UnsupportedCredentialKindError,
)
from qwenpaw.drivers.contracts import CredentialRef

_REFRESH_MARGIN_SECONDS = 300
CredentialProviderFactory = Callable[
    ["CredentialRef", CredentialStore],
    "CredentialProvider",
]
_PROVIDER_FACTORIES: dict[str, CredentialProviderFactory] = {}


class CredentialProvider(ABC):
    @abstractmethod
    async def resolve(self) -> ResolvedCredential:
        """Return runtime credential for DriverHandler._execute."""

    async def close(self) -> None:
        """Release cached token or HTTP clients."""

    def on_secrets_changed(
        self,
        secrets: dict[str, Any] | None = None,
    ) -> None:
        """Clear derived credential cache."""
        del secrets


class TokenExchanger(Protocol):
    async def exchange(self, secrets: dict[str, Any]) -> tuple[str, int]:
        """Return access_token and expires_in seconds."""


class StandardOAuth2Exchanger:
    """Small OAuth2 token exchanger used by default providers."""

    async def exchange(self, secrets: dict[str, Any]) -> tuple[str, int]:
        token_endpoint = str(secrets.get("token_endpoint") or "")
        if not token_endpoint:
            raise OAuthRequiredError(str(secrets.get("ref") or ""))

        payload: dict[str, Any]
        if secrets.get("refresh_token"):
            payload = {
                "grant_type": "refresh_token",
                "refresh_token": secrets["refresh_token"],
                "client_id": secrets.get("client_id", ""),
            }
        else:
            payload = {
                "grant_type": "client_credentials",
                "client_id": secrets.get("client_id", ""),
                "client_secret": secrets.get("client_secret", ""),
                "scope": secrets.get("scope", ""),
            }

        async with httpx.AsyncClient() as client:
            response = await client.post(token_endpoint, data=payload)
            response.raise_for_status()
            data = response.json()
        access_token = str(data.get("access_token") or "")
        if not access_token:
            reason = str(
                data.get("error_description")
                or data.get("error")
                or "missing access_token",
            )
            raise DriverCredentialProviderError(
                f"OAuth token endpoint did not return access_token: {reason}",
            )
        return access_token, int(data.get("expires_in", 3600))


class NoneProvider(CredentialProvider):
    async def resolve(self) -> ResolvedCredential:
        return ResolvedCredential.EMPTY


class DirectProvider(CredentialProvider):
    def __init__(self, ref: str, store: CredentialStore) -> None:
        self._ref = ref
        self._store = store

    async def resolve(self) -> ResolvedCredential:
        record = self._store.get(self._ref)
        return ResolvedCredential(
            kind=record.kind,
            public=dict(record.public),
            secrets=dict(record.secrets),
            meta=dict(record.meta),
        )


class OAuth2CCProvider(CredentialProvider):
    def __init__(
        self,
        ref: str,
        store: CredentialStore,
        exchanger: TokenExchanger | None = None,
    ) -> None:
        self._ref = ref
        self._store = store
        self._exchanger = exchanger or StandardOAuth2Exchanger()
        self._lock = asyncio.Lock()
        self._cached_token = ""
        self._expires_at = 0.0

    async def resolve(self) -> ResolvedCredential:
        now = time.time()
        if (
            self._cached_token
            and self._expires_at - now > _REFRESH_MARGIN_SECONDS
        ):
            return ResolvedCredential(
                kind="oauth2_cc",
                secrets={"access_token": self._cached_token},
            )

        async with self._lock:
            now = time.time()
            if (
                self._cached_token
                and self._expires_at - now > _REFRESH_MARGIN_SECONDS
            ):
                return ResolvedCredential(
                    kind="oauth2_cc",
                    secrets={"access_token": self._cached_token},
                )
            record = self._store.get(self._ref)
            values = record.values
            values["ref"] = self._ref
            token, expires_in = await self._exchanger.exchange(values)
            self._cached_token = token
            self._expires_at = time.time() + expires_in
            return ResolvedCredential(
                kind="oauth2_cc",
                secrets={"access_token": token},
            )

    def on_secrets_changed(
        self,
        secrets: dict[str, Any] | None = None,
    ) -> None:
        del secrets
        self._cached_token = ""
        self._expires_at = 0.0


class OAuth2AuthCodeProvider(CredentialProvider):
    def __init__(
        self,
        ref: str,
        store: CredentialStore,
        exchanger: TokenExchanger | None = None,
    ) -> None:
        self._ref = ref
        self._store = store
        self._exchanger = exchanger or StandardOAuth2Exchanger()
        self._lock = asyncio.Lock()

    async def resolve(self) -> ResolvedCredential:
        record = self._store.get(self._ref)
        values = record.values
        access_token = str(values.get("access_token") or "")
        expires_at = float(values.get("expires_at") or 0.0)
        if access_token and (
            expires_at <= 0
            or expires_at - time.time() > _REFRESH_MARGIN_SECONDS
        ):
            return ResolvedCredential(
                kind=record.kind,
                secrets={"access_token": access_token},
            )

        async with self._lock:
            record = self._store.get(self._ref)
            values = record.values
            access_token = str(values.get("access_token") or "")
            expires_at = float(values.get("expires_at") or 0.0)
            if access_token and (
                expires_at <= 0
                or expires_at - time.time() > _REFRESH_MARGIN_SECONDS
            ):
                return ResolvedCredential(
                    kind=record.kind,
                    secrets={"access_token": access_token},
                )
            if not values.get("refresh_token"):
                raise OAuthRequiredError(self._ref)
            values["ref"] = self._ref
            token, expires_in = await self._exchanger.exchange(values)
            public = dict(record.public)
            secrets = dict(record.secrets)
            public["expires_at"] = time.time() + expires_in
            secrets["access_token"] = token
            self._store.put(
                CredentialRecord(
                    ref=record.ref,
                    kind=record.kind,
                    public=public,
                    secrets=secrets,
                    meta=dict(record.meta),
                ),
            )
            return ResolvedCredential(
                kind=record.kind,
                secrets={"access_token": token},
            )


class AKSKProvider(CredentialProvider):
    def __init__(self, ref: str, store: CredentialStore) -> None:
        self._ref = ref
        self._store = store

    async def resolve(self) -> ResolvedCredential:
        record = self._store.get(self._ref)
        values = record.values
        access_key = str(values.get("access_key") or values.get("ak") or "")
        secret_key = str(values.get("secret_key") or values.get("sk") or "")
        timestamp = str(int(time.time()))
        payload = f"{access_key}:{timestamp}:{self._ref}".encode("utf-8")
        digest = hmac.new(
            secret_key.encode("utf-8"),
            payload,
            hashlib.sha256,
        ).digest()
        signature = base64.b64encode(digest).decode("ascii")
        return ResolvedCredential(
            kind=record.kind,
            secrets={
                "access_key": access_key,
                "timestamp": timestamp,
                "signature": signature,
            },
        )


def register_provider(
    kind: str,
    factory: CredentialProviderFactory,
    *,
    replace: bool = False,
) -> None:
    """Register a credential provider factory."""
    if not kind:
        raise UnsupportedCredentialKindError(kind)
    if kind in _PROVIDER_FACTORIES and not replace:
        raise DriverCredentialProviderError(
            f"Credential provider already registered: {kind}",
        )
    _PROVIDER_FACTORIES[kind] = factory


def unregister_provider(kind: str) -> None:
    """Remove a provider factory. Intended for tests and plugins unload."""
    _PROVIDER_FACTORIES.pop(kind, None)


def build_provider(
    credential_ref: CredentialRef,
    store: CredentialStore,
) -> CredentialProvider:
    """Factory for CredentialProvider by CredentialRef.kind."""
    try:
        factory = _PROVIDER_FACTORIES[credential_ref.kind]
    except KeyError as exc:
        raise UnsupportedCredentialKindError(credential_ref.kind) from exc
    return factory(credential_ref, store)


def _register_builtin_providers() -> None:
    register_provider("none", lambda ref, store: NoneProvider(), replace=True)
    register_provider(
        "static",
        lambda ref, store: DirectProvider(ref.ref, store),
        replace=True,
    )
    register_provider(
        "oauth2_cc",
        lambda ref, store: OAuth2CCProvider(ref.ref, store),
        replace=True,
    )
    register_provider(
        "oauth2_auth_code",
        lambda ref, store: OAuth2AuthCodeProvider(ref.ref, store),
        replace=True,
    )
    register_provider(
        "ak_sk",
        lambda ref, store: AKSKProvider(ref.ref, store),
        replace=True,
    )


_register_builtin_providers()
