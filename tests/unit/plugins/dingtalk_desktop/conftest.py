# -*- coding: utf-8 -*-
"""Import the source form of the DingTalk Desktop plugin."""

from __future__ import annotations

import sys
from pathlib import Path

PLUGIN_ROOT = (
    Path(__file__).resolve().parents[4]
    / "plugins"
    / "channel"
    / "dingtalk_desktop"
)

if str(PLUGIN_ROOT) not in sys.path:
    sys.path.insert(0, str(PLUGIN_ROOT))
