# -*- coding: utf-8 -*-
"""Tests for the DingTalk Desktop plugin registration contract."""

from __future__ import annotations

import importlib.util
import sys

from conftest import PLUGIN_ROOT


def load_plugin_class():
    """Load the entry point with the package context used by the host."""
    spec = importlib.util.spec_from_file_location(
        "dingtalk_desktop_plugin_entry",
        PLUGIN_ROOT / "plugin.py",
        submodule_search_locations=[str(PLUGIN_ROOT)],
    )
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module.DingTalkDesktopPlugin


class FakePluginApi:
    """Capture registrations without starting the host application."""

    def __init__(self):
        self.channel = None

    def register_channel(self, **kwargs):
        """Capture channel metadata."""
        self.channel = kwargs

    def register_http_router(self, *_args, **_kwargs):
        """Accept the plugin-owned router registration."""


def test_reply_mode_options_follow_host_string_contract():
    """Select options stay renderable by the current QwenPaw console."""
    api = FakePluginApi()

    load_plugin_class()().register(api)

    fields = api.channel["config_fields"]
    reply_mode = next(
        field for field in fields if field["name"] == "reply_mode"
    )
    assert reply_mode["options"] == ["draft", "automatic"]
    assert all(field["name"] != "allowed_conversations" for field in fields)
