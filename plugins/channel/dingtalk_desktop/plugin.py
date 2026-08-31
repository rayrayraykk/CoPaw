# -*- coding: utf-8 -*-
"""DingTalk Desktop channel plugin entry point."""

from __future__ import annotations

from qwenpaw.plugins.api import PluginApi


class DingTalkDesktopPlugin:
    """Register the desktop channel and its agent-scoped setup API."""

    def register(self, api: PluginApi) -> None:
        """Register channel metadata and setup routes."""
        from .dingtalk_desktop.channel import DingTalkDesktopChannel
        from .dingtalk_desktop.router import build_router

        api.register_channel(
            channel_class=DingTalkDesktopChannel,
            label="DingTalk Desktop",
            description=(
                "Use the local signed-in account for draft-first "
                "personal replies."
            ),
            config_fields=[
                {
                    "name": "reply_mode",
                    "label": {
                        "zh-CN": "回复模式",
                        "en-US": "Reply Mode",
                    },
                    "type": "select",
                    "required": True,
                    "default": "draft",
                    "options": [
                        {"label": "Draft", "value": "draft"},
                        {"label": "Automatic", "value": "automatic"},
                    ],
                },
                {
                    "name": "allowed_conversations",
                    "label": {
                        "zh-CN": "会话白名单",
                        "en-US": "Conversation Allowlist",
                    },
                    "type": "text",
                    "required": True,
                    "help": {
                        "zh-CN": ("多个会话名称使用英文逗号分隔。"),
                        "en-US": (
                            "Separate exact conversation names with commas."
                        ),
                    },
                },
                {
                    "name": "poll_sec",
                    "label": {
                        "zh-CN": "检查间隔（秒）",
                        "en-US": "Poll Interval (seconds)",
                    },
                    "type": "number",
                    "required": False,
                    "default": 1.0,
                },
                {
                    "name": "bundle_id",
                    "label": "Bundle ID",
                    "type": "text",
                    "required": True,
                    "default": "dd.work.exclusive4aliding",
                },
                {
                    "name": "context_messages",
                    "label": "Recent context messages",
                    "type": "number",
                    "required": False,
                    "default": 16,
                },
            ],
        )
        api.register_http_router(
            build_router(),
            prefix="/dingtalk-desktop",
            tags=["dingtalk-desktop"],
        )


plugin = DingTalkDesktopPlugin()
