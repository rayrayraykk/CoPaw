# -*- coding: utf-8 -*-
"""Per-workspace YAML credential store."""

from __future__ import annotations

import os
import tempfile
import threading
from pathlib import Path
from typing import Any

import yaml

from qwenpaw.drivers.credentials.types import CredentialRecord
from qwenpaw.drivers.errors import CredentialNotFoundError, DriverCardError
from qwenpaw.security.secret_store import decrypt, encrypt, is_encrypted

_STORE_VERSION = 1


class CredentialStore:
    """Per-workspace YAML credential store."""

    def __init__(self, credentials_path: Path) -> None:
        self._path = credentials_path
        self._lock = threading.RLock()

    def get(self, ref: str) -> CredentialRecord:
        """Read one CredentialRecord and decrypt values under secrets."""
        if ref.startswith("env:"):
            var = ref[len("env:") :]
            return CredentialRecord(
                ref=ref,
                kind="env",
                secrets={"value": os.environ.get(var, "")},
            )

        credentials = self._read_credentials()
        if ref not in credentials:
            raise CredentialNotFoundError(ref)
        value = credentials[ref]
        if not isinstance(value, dict):
            raise DriverCardError(f"Credential entry must be a mapping: {ref}")
        public = value.get("public", {})
        secrets = value.get("secrets", {})
        meta = value.get("meta", {})
        if not isinstance(public, dict):
            raise DriverCardError(
                f"Credential public must be a mapping: {ref}",
            )
        if not isinstance(secrets, dict):
            raise DriverCardError(
                f"Credential secrets must be a mapping: {ref}",
            )
        if not isinstance(meta, dict):
            raise DriverCardError(f"Credential meta must be a mapping: {ref}")
        return CredentialRecord(
            ref=ref,
            kind=str(value.get("kind") or ""),
            public=dict(public),
            secrets=self._decrypt_secrets(dict(secrets)),
            meta=dict(meta),
        )

    def put(self, record: CredentialRecord) -> None:
        """Encrypt all string secrets and atomically write YAML."""
        if record.ref.startswith("env:"):
            raise DriverCardError("Cannot persist env: credential refs")
        if not record.ref:
            raise DriverCardError("CredentialRecord.ref must be non-empty")
        if not record.kind:
            raise DriverCardError("CredentialRecord.kind must be non-empty")
        self._validate_secret_values(record.secrets)

        with self._lock:
            root = self._read_root()
            credentials = dict(root["credentials"])
            credentials[record.ref] = {
                "kind": record.kind,
                "public": dict(record.public),
                "secrets": self._encrypt_secrets(dict(record.secrets)),
                "meta": dict(record.meta),
            }
            root["credentials"] = credentials
            self._write_root(root)

    def delete(self, ref: str) -> None:
        """Remove one credential entry if present."""
        with self._lock:
            root = self._read_root()
            credentials = dict(root["credentials"])
            if ref in credentials:
                credentials.pop(ref, None)
                root["credentials"] = credentials
                self._write_root(root)

    def list_refs(self) -> list[str]:
        """Return sorted credential refs."""
        return sorted(self._read_credentials().keys())

    def _read_root(self) -> dict[str, Any]:
        if not self._path.is_file():
            return {"version": _STORE_VERSION, "credentials": {}}
        try:
            data = yaml.safe_load(self._path.read_text(encoding="utf-8"))
        except OSError as exc:
            raise DriverCardError(
                f"Failed to read credentials {self._path}: {exc}",
            ) from exc
        except yaml.YAMLError as exc:
            raise DriverCardError(
                f"Failed to parse credentials {self._path}: {exc}",
            ) from exc
        if data is None:
            return {"version": _STORE_VERSION, "credentials": {}}
        if not isinstance(data, dict):
            raise DriverCardError("Credential store root must be a mapping")
        if data.get("version") != _STORE_VERSION:
            raise DriverCardError(
                f"Credential store version must be {_STORE_VERSION}",
            )
        credentials = data.get("credentials")
        if not isinstance(credentials, dict):
            raise DriverCardError(
                "Credential store credentials must be a mapping",
            )
        return {"version": _STORE_VERSION, "credentials": dict(credentials)}

    def _read_credentials(self) -> dict[str, Any]:
        return dict(self._read_root()["credentials"])

    def _write_root(self, data: dict[str, Any]) -> None:
        self._path.parent.mkdir(parents=True, exist_ok=True)
        tmp_name = ""
        try:
            with tempfile.NamedTemporaryFile(
                "w",
                encoding="utf-8",
                dir=str(self._path.parent),
                prefix=f".{self._path.name}.",
                suffix=".tmp",
                delete=False,
            ) as tmp:
                tmp_name = tmp.name
                yaml.safe_dump(
                    data,
                    tmp,
                    allow_unicode=False,
                    sort_keys=False,
                )
                tmp.flush()
                os.fsync(tmp.fileno())
            os.replace(tmp_name, self._path)
            try:
                os.chmod(self._path, 0o600)
            except OSError:
                pass
        except Exception as exc:
            if tmp_name:
                try:
                    Path(tmp_name).unlink(missing_ok=True)
                except OSError:
                    pass
            raise DriverCardError(
                f"Failed to write credentials {self._path}: {exc}",
            ) from exc

    @classmethod
    def _validate_secret_values(cls, data: dict[str, Any]) -> None:
        for key, value in data.items():
            if not isinstance(key, str) or not key:
                raise ValueError("Credential secret keys must be strings")
            if not isinstance(value, str):
                raise ValueError("Credential secret values must be strings")

    @classmethod
    def _encrypt_secrets(cls, data: dict[str, str]) -> dict[str, str]:
        return {
            key: value if is_encrypted(value) else encrypt(value)
            for key, value in data.items()
        }

    @classmethod
    def _decrypt_secrets(cls, data: dict[str, Any]) -> dict[str, Any]:
        return {
            key: decrypt(value) if isinstance(value, str) else value
            for key, value in data.items()
        }
