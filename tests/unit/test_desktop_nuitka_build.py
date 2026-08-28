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


def test_build_command_uses_multidist_entries(tmp_path, monkeypatch) -> None:
    repo_root = tmp_path / "repo"
    output_root = tmp_path / "output"
    monkeypatch.setattr(
        _MODULE,
        "_distribution_is_installed",
        lambda name: name == "qwenpaw",
    )
    monkeypatch.setattr(_MODULE.sys, "platform", "win32")

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
    entries_dir = repo_root / "scripts" / "pack-tauri" / "nuitka_entries"
    assert f"--main={entries_dir / 'qwenpaw-backend.py'}" in command
    assert f"--main={entries_dir / 'qwenpaw.py'}" in command


def test_build_command_uses_app_mode_on_macos(tmp_path, monkeypatch) -> None:
    monkeypatch.setattr(_MODULE.sys, "platform", "darwin")
    monkeypatch.setattr(
        _MODULE,
        "_distribution_is_installed",
        lambda name: False,
    )

    command = _MODULE.build_command(
        Path(sys.executable),
        tmp_path / "repo",
        tmp_path / "output",
    )

    assert "--mode=app" in command
    assert "--mode=standalone" not in command


def test_find_bundle_dir_requires_one_candidate(tmp_path, monkeypatch) -> None:
    monkeypatch.setattr(_MODULE.sys, "platform", "linux")
    standalone_dir = tmp_path / "entry.dist"
    standalone_dir.mkdir()

    assert _MODULE.find_bundle_dir(tmp_path) == standalone_dir


def test_find_bundle_dir_rejects_ambiguous_output(
    tmp_path,
    monkeypatch,
) -> None:
    monkeypatch.setattr(_MODULE.sys, "platform", "linux")
    (tmp_path / "first.dist").mkdir()
    (tmp_path / "second.dist").mkdir()

    try:
        _MODULE.find_bundle_dir(tmp_path)
    except RuntimeError as exc:
        assert "Expected one" in str(exc)
    else:
        raise AssertionError("Expected ambiguous output to fail")


def test_ensure_multidist_entrypoints_copies_cli_alias(
    tmp_path,
    monkeypatch,
) -> None:
    monkeypatch.setattr(_MODULE.sys, "platform", "linux")
    backend = tmp_path / "qwenpaw-backend"
    backend.write_bytes(b"backend")

    actual_backend, cli = _MODULE.ensure_multidist_entrypoints(tmp_path)

    assert actual_backend == backend
    assert cli.read_bytes() == b"backend"
