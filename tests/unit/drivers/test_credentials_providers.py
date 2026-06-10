# -*- coding: utf-8 -*-
import asyncio
from typing import Any

import pytest

import qwenpaw.drivers.credentials.providers as providers_module
from qwenpaw.drivers.credentials.providers import (
    AKSKProvider,
    DirectProvider,
    OAuth2AuthCodeProvider,
    OAuth2CCProvider,
    build_provider,
    register_provider,
    unregister_provider,
)
from qwenpaw.drivers.credentials.types import (
    CredentialRecord,
    ResolvedCredential,
)
from qwenpaw.drivers.errors import (
    DriverCredentialProviderError,
    UnsupportedCredentialKindError,
)
from qwenpaw.drivers.contracts import CredentialRef


class FakeStore:
    def __init__(self, data: dict[str, CredentialRecord]) -> None:
        self.data = data
        self.put_calls: list[CredentialRecord] = []

    def get(self, ref: str) -> CredentialRecord:
        record = self.data[ref]
        return CredentialRecord(
            ref=record.ref,
            kind=record.kind,
            public=dict(record.public),
            secrets=dict(record.secrets),
            meta=dict(record.meta),
        )

    def put(self, record: CredentialRecord) -> None:
        self.put_calls.append(record)
        self.data[record.ref] = record


class FakeExchanger:
    def __init__(self) -> None:
        self.calls = 0

    async def exchange(self, secrets: dict[str, Any]) -> tuple[str, int]:
        del secrets
        self.calls += 1
        await asyncio.sleep(0)
        return f"token-{self.calls}", 3600


@pytest.mark.asyncio
async def test_none_provider_returns_empty() -> None:
    provider = build_provider(CredentialRef(kind="none"), FakeStore({}))

    assert await provider.resolve() is ResolvedCredential.EMPTY


@pytest.mark.asyncio
async def test_direct_provider_returns_store_values() -> None:
    store = FakeStore(
        {
            "demo": CredentialRecord(
                ref="demo",
                kind="static",
                public={"name": "visible"},
                secrets={"token": "abc"},
            ),
        },
    )

    credential = await DirectProvider("demo", store).resolve()

    assert credential.kind == "static"
    assert credential.public == {"name": "visible"}
    assert credential.secrets == {"token": "abc"}
    assert credential.values == {"name": "visible", "token": "abc"}


@pytest.mark.asyncio
async def test_oauth2_cc_provider_caches_token() -> None:
    store = FakeStore(
        {
            "demo": CredentialRecord(
                ref="demo",
                kind="oauth2_cc",
                public={"client_id": "id"},
                secrets={"client_secret": "secret"},
            ),
        },
    )
    exchanger = FakeExchanger()
    provider = OAuth2CCProvider("demo", store, exchanger)

    first = await provider.resolve()
    second = await provider.resolve()

    assert first.values == {"access_token": "token-1"}
    assert second.values == {"access_token": "token-1"}
    assert exchanger.calls == 1


@pytest.mark.asyncio
async def test_oauth2_cc_concurrent_resolve_refreshes_once() -> None:
    store = FakeStore(
        {
            "demo": CredentialRecord(
                ref="demo",
                kind="oauth2_cc",
                public={"client_id": "id"},
                secrets={"client_secret": "secret"},
            ),
        },
    )
    exchanger = FakeExchanger()
    provider = OAuth2CCProvider("demo", store, exchanger)

    results = await asyncio.gather(
        provider.resolve(),
        provider.resolve(),
        provider.resolve(),
    )

    assert [item.values["access_token"] for item in results] == [
        "token-1",
        "token-1",
        "token-1",
    ]
    assert exchanger.calls == 1


@pytest.mark.asyncio
async def test_oauth2_auth_code_refreshes_and_persists(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(providers_module.time, "time", lambda: 1000.0)
    store = FakeStore(
        {
            "demo": CredentialRecord(
                ref="demo",
                kind="oauth2_auth_code",
                public={"expires_at": 900.0},
                secrets={
                    "access_token": "old",
                    "refresh_token": "refresh",
                },
            ),
        },
    )
    exchanger = FakeExchanger()
    provider = OAuth2AuthCodeProvider("demo", store, exchanger)

    credential = await provider.resolve()

    assert credential.values == {"access_token": "token-1"}
    updated = store.put_calls[0]
    assert updated.ref == "demo"
    assert updated.secrets["access_token"] == "token-1"
    assert updated.public["expires_at"] == 4600.0


@pytest.mark.asyncio
async def test_aksk_provider_generates_signature_each_resolve(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    times = iter([1000.0, 1001.0])
    monkeypatch.setattr(providers_module.time, "time", lambda: next(times))
    store = FakeStore(
        {
            "demo": CredentialRecord(
                ref="demo",
                kind="ak_sk",
                secrets={"access_key": "ak", "secret_key": "sk"},
            ),
        },
    )
    provider = AKSKProvider("demo", store)

    first = await provider.resolve()
    second = await provider.resolve()

    assert first.values["timestamp"] == "1000"
    assert second.values["timestamp"] == "1001"
    assert first.values["signature"] != second.values["signature"]


def test_build_provider_rejects_unknown_kind() -> None:
    with pytest.raises(UnsupportedCredentialKindError):
        build_provider(CredentialRef(kind="unknown"), FakeStore({}))


@pytest.mark.asyncio
async def test_custom_provider_factory_can_be_registered() -> None:
    class CustomProvider(DirectProvider):
        pass

    register_provider(
        "unit_custom",
        lambda ref, store: CustomProvider(ref.ref, store),
    )
    try:
        provider = build_provider(
            CredentialRef("unit_custom", "demo"),
            FakeStore({}),
        )
    finally:
        unregister_provider("unit_custom")

    assert isinstance(provider, CustomProvider)


def test_register_provider_rejects_duplicate_kind() -> None:
    register_provider(
        "unit_duplicate",
        lambda ref, store: DirectProvider(ref.ref, store),
    )
    try:
        with pytest.raises(
            DriverCredentialProviderError,
            match="already registered",
        ):
            register_provider(
                "unit_duplicate",
                lambda ref, store: DirectProvider(ref.ref, store),
            )
    finally:
        unregister_provider("unit_duplicate")
