# -*- coding: utf-8 -*-
"""Tests for the experimental Nuitka backend builder."""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path


_SCRIPT_PATH = (
    Path(__file__).parents[2]
    / "scripts"
    / "pack-tauri"
    / "build_nuitka_backend.py"
)
_SPEC = importlib.util.spec_from_file_location(
    "build_nuitka_backend",
    _SCRIPT_PATH,
)
assert _SPEC is not None
assert _SPEC.loader is not None
_MODULE = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(_MODULE)


def test_build_command_uses_standalone_mode(tmp_path, monkeypatch) -> None:
    repo_root = tmp_path / "repo"
    output_root = tmp_path / "output"
    monkeypatch.setattr(
        _MODULE,
        "_distribution_is_installed",
        lambda name: name == "qwenpaw",
    )

    command = _MODULE.build_command(
        Path(sys.executable),
        repo_root,
        output_root,
    )

    assert "--mode=standalone" in command
    assert "--include-package=qwenpaw" in command
    assert "--include-package=qwenpawmail_mcp" in command
    assert "--include-distribution-metadata=qwenpaw" in command
    assert "--include-distribution-metadata=agentscope" not in command
    assert (
        f"--include-data-dir={repo_root / 'console' / 'dist'}="
        "qwenpaw/console"
    ) in command
    assert command[-1] == str(
        repo_root / "src" / "qwenpaw" / "tauri" / "entry.py",
    )


def test_find_standalone_dir_requires_one_candidate(tmp_path) -> None:
    standalone_dir = tmp_path / "entry.dist"
    standalone_dir.mkdir()

    assert _MODULE.find_standalone_dir(tmp_path) == standalone_dir


def test_find_standalone_dir_rejects_ambiguous_output(tmp_path) -> None:
    (tmp_path / "first.dist").mkdir()
    (tmp_path / "second.dist").mkdir()

    try:
        _MODULE.find_standalone_dir(tmp_path)
    except RuntimeError as exc:
        assert "Expected one" in str(exc)
    else:
        raise AssertionError("Expected ambiguous output to fail")
