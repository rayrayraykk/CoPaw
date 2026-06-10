# -*- coding: utf-8 -*-
from pathlib import Path

import pytest
import yaml

from qwenpaw.drivers.credentials.store import CredentialStore
from qwenpaw.drivers.credentials.types import CredentialRecord
from qwenpaw.drivers.errors import CredentialNotFoundError


@pytest.fixture(autouse=True)
def fake_secret_store(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(
        "qwenpaw.drivers.credentials.store.encrypt",
        lambda value: f"ENC:{value}",
    )
    monkeypatch.setattr(
        "qwenpaw.drivers.credentials.store.decrypt",
        lambda value: value.removeprefix("ENC:"),
    )
    monkeypatch.setattr(
        "qwenpaw.drivers.credentials.store.is_encrypted",
        lambda value: isinstance(value, str) and value.startswith("ENC:"),
    )


def test_put_encrypts_all_secret_fields_and_keeps_public_plain(
    tmp_path: Path,
) -> None:
    path = tmp_path / "credentials.yaml"
    store = CredentialStore(path)

    store.put(
        CredentialRecord(
            ref="demo",
            kind="static",
            public={"name": "visible", "token": "public-token"},
            secrets={"token": "plain", "api_key": "key"},
        ),
    )

    raw = yaml.safe_load(path.read_text(encoding="utf-8"))
    assert raw["version"] == 1
    assert raw["credentials"]["demo"]["public"]["token"] == "public-token"
    assert raw["credentials"]["demo"]["secrets"]["token"] == "ENC:plain"
    assert raw["credentials"]["demo"]["secrets"]["api_key"] == "ENC:key"
    assert raw["credentials"]["demo"]["public"]["name"] == "visible"


def test_get_decrypts_and_resolves_env(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    path = tmp_path / "credentials.yaml"
    path.write_text(
        "version: 1\n"
        "credentials:\n"
        "  demo:\n"
        "    kind: static\n"
        "    public:\n"
        "      url: ${env:DRIVER_TEST_URL}/v1\n"
        "    secrets:\n"
        "      token: ENC:secret\n"
        "    meta:\n"
        "      source: test\n",
        encoding="utf-8",
    )
    monkeypatch.setenv("DRIVER_TEST_URL", "https://example.test")

    record = CredentialStore(path).get("demo")

    assert record.ref == "demo"
    assert record.kind == "static"
    assert record.secrets["token"] == "secret"
    assert record.public["url"] == "${env:DRIVER_TEST_URL}/v1"
    assert record.meta["source"] == "test"


def test_env_ref_returns_token(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("DRIVER_TOKEN", "abc")

    record = CredentialStore(Path("unused")).get("env:DRIVER_TOKEN")

    assert record.ref == "env:DRIVER_TOKEN"
    assert record.kind == "env"
    assert record.secrets == {"value": "abc"}


def test_missing_ref_raises(tmp_path: Path) -> None:
    with pytest.raises(CredentialNotFoundError):
        CredentialStore(tmp_path / "credentials.yaml").get("missing")


def test_delete_and_list_refs(tmp_path: Path) -> None:
    store = CredentialStore(tmp_path / "credentials.yaml")
    store.put(CredentialRecord(ref="b", kind="static", secrets={"token": "2"}))
    store.put(CredentialRecord(ref="a", kind="static", secrets={"token": "1"}))

    assert store.list_refs() == ["a", "b"]

    store.delete("a")

    assert store.list_refs() == ["b"]


def test_put_rejects_non_string_secret_values(tmp_path: Path) -> None:
    store = CredentialStore(tmp_path / "credentials.yaml")

    with pytest.raises(ValueError, match="secret values must be strings"):
        store.put(
            CredentialRecord(
                ref="demo",
                kind="static",
                secrets={"token": {"nested": "no"}},
            ),
        )
