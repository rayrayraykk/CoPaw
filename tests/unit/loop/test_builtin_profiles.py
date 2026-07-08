# -*- coding: utf-8 -*-
"""Unit tests for builtin_profiles structure integrity."""
from __future__ import annotations

from qwenpaw.loop.builtin_profiles import (
    BUILTIN_PROFILES,
    BuiltinProfileSpec,
    GateSpec,
    get_builtin_profile_names,
    is_builtin_profile,
)


class TestBuiltinProfileStructure:
    """Verify all builtin profiles are well-formed."""

    def test_expected_profiles_exist(self):
        names = set(BUILTIN_PROFILES.keys())
        assert "default" in names
        assert "goal" in names
        assert "mission" in names

    def test_all_specs_are_correct_type(self):
        for name, spec in BUILTIN_PROFILES.items():
            assert isinstance(
                spec,
                BuiltinProfileSpec,
            ), f"Profile '{name}' is not BuiltinProfileSpec"

    def test_name_matches_key(self):
        for key, spec in BUILTIN_PROFILES.items():
            assert spec.name == key, f"Key '{key}' != spec.name '{spec.name}'"

    def test_scope_is_set(self):
        for name, spec in BUILTIN_PROFILES.items():
            assert spec.scope, f"Profile '{name}' has empty scope"

    def test_description_is_nonempty(self):
        for name, spec in BUILTIN_PROFILES.items():
            assert spec.description, f"Profile '{name}' has empty description"

    def test_gates_are_nonempty(self):
        for name, spec in BUILTIN_PROFILES.items():
            assert len(spec.gates) > 0, f"Profile '{name}' has no gates"

    def test_all_gates_are_gate_spec(self):
        for name, spec in BUILTIN_PROFILES.items():
            for gs in spec.gates:
                assert isinstance(
                    gs,
                    GateSpec,
                ), f"Gate in '{name}' is not GateSpec"

    def test_gate_types_are_valid(self):
        valid_types = {
            "iteration",
            "doom_loop",
            "rubric",
            "budget",
        }
        for name, spec in BUILTIN_PROFILES.items():
            for gs in spec.gates:
                assert gs.type in valid_types, (
                    f"Gate type '{gs.type}' in " f"'{name}' is not valid"
                )

    def test_priorities_are_positive(self):
        for name, spec in BUILTIN_PROFILES.items():
            for gs in spec.gates:
                assert gs.priority > 0, (
                    f"Gate '{gs.type}' in '{name}'"
                    f" has non-positive priority"
                )

    def test_default_params_are_dicts(self):
        for name, spec in BUILTIN_PROFILES.items():
            for gs in spec.gates:
                assert isinstance(
                    gs.default_params,
                    dict,
                ), (
                    f"Gate '{gs.type}' in '{name}'" f" params is not a dict"
                )


class TestDefaultProfileSpecific:
    """Verify the default profile has expected gates."""

    def test_has_iteration_gate(self):
        spec = BUILTIN_PROFILES["default"]
        types = {g.type for g in spec.gates}
        assert "iteration" in types

    def test_has_doom_loop_gate(self):
        spec = BUILTIN_PROFILES["default"]
        types = {g.type for g in spec.gates}
        assert "doom_loop" in types

    def test_has_rubric_gate(self):
        spec = BUILTIN_PROFILES["default"]
        types = {g.type for g in spec.gates}
        assert "rubric" in types

    def test_iteration_default_max(self):
        spec = BUILTIN_PROFILES["default"]
        gate = next(g for g in spec.gates if g.type == "iteration")
        assert gate.default_params["max_iterations"] == 100

    def test_doom_loop_has_stages(self):
        spec = BUILTIN_PROFILES["default"]
        gate = next(g for g in spec.gates if g.type == "doom_loop")
        stages = gate.default_params.get("stages", [])
        assert len(stages) >= 2
        assert stages[0]["action"] == "modify_prompt"
        assert stages[-1]["action"] == "stop"


class TestHelperFunctions:
    """Test utility functions."""

    def test_get_builtin_profile_names(self):
        names = get_builtin_profile_names()
        assert "default" in names
        assert "goal" in names
        assert "mission" in names

    def test_is_builtin_true(self):
        assert is_builtin_profile("default") is True
        assert is_builtin_profile("goal") is True

    def test_is_builtin_false(self):
        assert is_builtin_profile("custom") is False
        assert is_builtin_profile("") is False
