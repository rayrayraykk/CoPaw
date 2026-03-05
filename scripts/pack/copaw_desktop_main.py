# -*- coding: utf-8 -*-
"""Entry point for packaged desktop app (.app on macOS, exe on Windows).
When run with no arguments, defaults to 'gui' (webview window).

When frozen (Nuitka), sets COPAW_WORKING_DIR to the platform app-data dir
so config, skills, tokens, and logs live in the correct place.
"""
from __future__ import annotations

import os
import sys
from pathlib import Path

# When launched from .app or desktop shortcut, argv is often [exe_path].
# Default to 'gui' so the window opens.
if len(sys.argv) == 1:
    sys.argv.append("gui")

# Set working dir for packaged app before any copaw import (skills, token,
# config paths are under WORKING_DIR).
if getattr(sys, "frozen", False):
    if sys.platform == "darwin":
        app_support = Path.home() / "Library" / "Application Support"
        os.environ.setdefault(
            "COPAW_WORKING_DIR",
            str((app_support / "CoPaw").resolve()),
        )
    elif sys.platform == "win32":
        appdata = os.environ.get("APPDATA", "")
        if not appdata:
            appdata = str(Path.home() / "AppData" / "Roaming")
        os.environ.setdefault(
            "COPAW_WORKING_DIR",
            str(Path(appdata) / "CoPaw"),
        )

from copaw.cli.main import cli  # pylint: disable=wrong-import-position

if __name__ == "__main__":
    cli()  # pylint: disable=no-value-for-parameter
