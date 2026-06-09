"""Read-only helpers for extracting channel credential candidates."""

from __future__ import annotations

from typing import Any

CHANNEL_CREDENTIAL_FIELDS: dict[str, tuple[str, ...]] = {
    "dingtalk": ("client_id", "client_secret", "app_key", "app_secret"),
    "feishu": ("app_id", "app_secret", "verification_token"),
    "telegram": ("bot_token",),
    "discord": ("bot_token", "token"),
    "mattermost": ("token", "bot_token"),
    "mqtt": ("username", "password"),
    "xiaoyi": ("token", "client_secret"),
    "qq": ("app_id", "token", "secret"),
    "onebot": ("access_token", "token"),
    "matrix": ("access_token", "password"),
    "wechat": ("app_id", "app_secret", "token", "encoding_aes_key"),
    "wecom": ("corp_id", "corp_secret", "token", "encoding_aes_key"),
}


def extract_channel_credentials(
    channel: str,
    config: Any,
) -> list[tuple[str, str, dict[str, Any]]]:
    """Return credential candidates as (ref, kind, data), without writing."""
    channel_name = str(channel).strip().lower()
    fields = CHANNEL_CREDENTIAL_FIELDS.get(channel_name, ())
    data: dict[str, Any] = {}
    for field in fields:
        value = _read_value(config, field)
        if value not in (None, ""):
            data[field] = value
    if not data:
        return []
    return [(f"channel-{channel_name}", "static", data)]


def _read_value(config: Any, field: str) -> Any:
    if isinstance(config, dict):
        return config.get(field)
    return getattr(config, field, None)
