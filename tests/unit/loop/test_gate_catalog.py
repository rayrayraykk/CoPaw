# -*- coding: utf-8 -*-
"""Unit tests for GateCatalog singleton and registration."""
# pylint: disable=redefined-outer-name,protected-access,unused-argument
from __future__ import annotations

import pytest

from qwenpaw.loop.gate_catalog import (
    GateCatalog,
    GateCatalogEntry,
)


@pytest.fixture(autouse=True)
def _reset_catalog():
    """Reset singleton before each test."""
    GateCatalog._reset()
    yield
    GateCatalog._reset()


def _dummy_factory(params):
    """Stub factory for testing."""
    return None


def _make_entry(
    gate_type: str = "test_gate",
    **kwargs,
) -> GateCatalogEntry:
    defaults = {
        "type": gate_type,
        "name": "Test Gate",
        "description": "A test gate",
        "category": "safety",
        "default_priority": 10,
        "params_schema": {},
        "factory": _dummy_factory,
    }
    defaults.update(kwargs)
    return GateCatalogEntry(**defaults)


class TestGateCatalogSingleton:
    """Verify singleton behavior."""

    def test_same_instance(self):
        a = GateCatalog()
        b = GateCatalog()
        assert a is b

    def test_reset_creates_new_instance(self):
        a = GateCatalog()
        GateCatalog._reset()
        b = GateCatalog()
        assert a is not b


class TestGateCatalogRegistration:
    """Verify register / get / unregister / list."""

    def test_register_and_get(self):
        catalog = GateCatalog()
        entry = _make_entry("my_gate")
        catalog.register(entry)
        assert catalog.get("my_gate") is entry

    def test_get_missing_returns_none(self):
        catalog = GateCatalog()
        assert catalog.get("nonexistent") is None

    def test_unregister(self):
        catalog = GateCatalog()
        catalog.register(_make_entry("x"))
        assert catalog.get("x") is not None
        catalog.unregister("x")
        assert catalog.get("x") is None

    def test_unregister_missing_is_noop(self):
        catalog = GateCatalog()
        catalog.unregister("missing")

    def test_list_entries(self):
        catalog = GateCatalog()
        catalog.register(_make_entry("a"))
        catalog.register(_make_entry("b"))
        types = {e.type for e in catalog.list_entries()}
        assert types == {"a", "b"}

    def test_register_overwrites(self):
        catalog = GateCatalog()
        catalog.register(
            _make_entry("x", name="First"),
        )
        catalog.register(
            _make_entry("x", name="Second"),
        )
        assert catalog.get("x").name == "Second"
        assert len(catalog.list_entries()) == 1


class TestGateCatalogCreate:
    """Verify gate instantiation."""

    def test_create_unknown_raises(self):
        catalog = GateCatalog()
        with pytest.raises(KeyError, match="Unknown"):
            catalog.create("missing")

    def test_create_calls_factory(self):
        calls = []

        def factory(params):
            calls.append(params)
            return "gate_instance"

        catalog = GateCatalog()
        catalog.register(
            _make_entry("my", factory=factory),
        )
        result = catalog.create(
            "my",
            {"max": 10},
        )
        assert result == "gate_instance"
        assert calls == [{"max": 10}]

    def test_create_defaults_empty_params(self):
        calls = []

        def factory(params):
            calls.append(params)
            return "ok"

        catalog = GateCatalog()
        catalog.register(
            _make_entry("my", factory=factory),
        )
        catalog.create("my")
        assert calls == [{}]


class TestEnsureBuiltins:
    """Verify builtin gate registration."""

    def test_registers_builtin_gates(self):
        catalog = GateCatalog()
        catalog.ensure_builtins()
        types = {e.type for e in catalog.list_entries()}
        assert "iteration" in types
        assert "doom_loop" in types
        assert "rubric" in types
        assert "budget" in types

    def test_idempotent(self):
        catalog = GateCatalog()
        catalog.ensure_builtins()
        count1 = len(catalog.list_entries())
        catalog.ensure_builtins()
        count2 = len(catalog.list_entries())
        assert count1 == count2


class TestToApiResponse:
    """Verify API serialization."""

    def test_response_fields(self):
        catalog = GateCatalog()
        catalog.ensure_builtins()
        resp = catalog.to_api_response()
        assert len(resp) >= 4
        for item in resp:
            assert "type" in item
            assert "name" in item
            assert "description" in item
            assert "category" in item
            assert "params_schema" in item
            assert "factory" not in item
