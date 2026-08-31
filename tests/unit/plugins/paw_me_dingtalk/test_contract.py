# -*- coding: utf-8 -*-
"""Tests for the PawApp and DWS-only safety contracts."""

from __future__ import annotations

import json

from conftest import PLUGIN_ROOT


def test_manifest_registers_a_single_page_pawapp():
    """The digital twin is packaged as an App rather than a Channel."""
    manifest = json.loads(
        (PLUGIN_ROOT / "plugin.json").read_text(encoding="utf-8"),
    )

    assert manifest["id"] == "paw-me-dingtalk"
    assert manifest["type"] == "app"
    assert manifest["entry"]["backend"] == "backend/main.py"
    assert manifest["meta"]["pawapp"]["entry_page"] == (
        "/apps/paw-me-dingtalk"
    )


def test_plugin_has_no_desktop_automation_surface():
    """The app contains no coordinate or Accessibility automation."""
    backend = PLUGIN_ROOT / "backend"
    names = {path.name for path in backend.iterdir()}

    assert "bridge.js" not in names
    assert "ax_snapshot.swift" not in names
    assert "desktop.py" not in names
    assert "dws.py" in names


def test_frontend_bundle_uses_pawapp_route_and_host_react():
    """The Blob-loaded bundle has no bare React dependency."""
    bundle = (PLUGIN_ROOT / "dist" / "index.js").read_text(encoding="utf-8")

    assert 'from"react"' not in bundle
    assert 'from "react"' not in bundle
    assert "/apps/paw-me-dingtalk" in bundle
    assert "forApp" in bundle
