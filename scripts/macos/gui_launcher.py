# -*- coding: utf-8 -*-
"""
macOS .app GUI entry: start CoPaw server in background and show Console in a
native window (pywebview). Close window to quit server and app.
"""
from __future__ import annotations

import os
import sys
import threading
import time
import traceback
import urllib.request

import uvicorn


def _log(msg: str) -> None:
    """Write to stderr and to support-dir log for .app runs."""
    print(msg, file=sys.stderr, flush=True)
    support = os.path.expanduser("~/Library/Application Support/CoPaw")
    log_dir = os.path.join(support, "logs")
    try:
        os.makedirs(log_dir, exist_ok=True)
        with open(
            os.path.join(log_dir, "gui_launcher.log"),
            "a",
            encoding="utf-8",
        ) as f:
            f.write(msg.rstrip() + "\n")
            f.flush()
    except OSError:
        pass


# Set working dir and init before importing copaw (uses COPAW_WORKING_DIR).
def _ensure_working_dir() -> None:
    support = os.path.expanduser(
        "~/Library/Application Support/CoPaw",
    )
    os.environ["COPAW_WORKING_DIR"] = support
    config = os.path.join(support, "config.json")
    if not os.path.isfile(config):
        from copaw.cli.init_cmd import init_cmd

        init_cmd.main(
            args=["--defaults", "--accept-security"],
            standalone_mode=False,
        )


def _wait_for_server(url: str, timeout: float = 15.0) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            with urllib.request.urlopen(url, timeout=1):
                return True
        except Exception:
            time.sleep(0.2)
    return False


def _run_server(server: "uvicorn.Server") -> None:
    import asyncio

    loop = asyncio.new_event_loop()
    asyncio.set_event_loop(loop)
    try:
        loop.run_until_complete(server.serve())
    finally:
        loop.close()


def main() -> None:
    _ensure_working_dir()

    from copaw.utils.logging import setup_logger

    setup_logger("info")
    config = uvicorn.Config(
        "copaw.app._app:app",
        host="0.0.0.0",
        port=8088,
        log_level="info",
    )
    server = uvicorn.Server(config)
    thread = threading.Thread(
        target=_run_server,
        args=(server,),
        daemon=True,
    )
    thread.start()

    if not _wait_for_server("http://127.0.0.1:8088/"):
        _log("CoPaw server failed to start (port 8088).")
        server.should_exit = True
        sys.exit(1)

    import webview

    webview.create_window(
        "CoPaw",
        "http://127.0.0.1:8088/",
        width=1200,
        height=800,
        min_size=(800, 600),
        resizable=True,
    )
    webview.start()
    server.should_exit = True


if __name__ == "__main__":
    try:
        main()
    except Exception:  # pylint: disable=broad-except
        _log(traceback.format_exc())
        raise
