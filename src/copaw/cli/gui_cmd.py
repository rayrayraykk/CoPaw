# -*- coding: utf-8 -*-
"""Desktop GUI: start app server in background and open a webview window."""
from __future__ import annotations

import logging
import os
import subprocess
import sys
import time

import click

from ..constant import LOG_LEVEL_ENV, WORKING_DIR
from ..utils.logging import setup_logger, add_copaw_file_handler
from ..utils.port import find_free_port

logger = logging.getLogger(__name__)


def _server_ready(host: str, port: int, timeout: float = 15.0) -> bool:
    """Return True when the server at host:port responds."""
    try:
        import httpx

        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                r = httpx.get(f"http://{host}:{port}/", timeout=1.0)
                if r.status_code < 500:
                    return True
            except Exception:
                time.sleep(0.2)
        return False
    except ImportError:
        # Fallback: try socket connect
        import socket

        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                    s.settimeout(1.0)
                    s.connect((host, port))
                return True
            except OSError:
                time.sleep(0.2)
        return False


def _run_webview_blocking(url: str) -> bool:
    """Open url in webview; block until closed. Return False if no webview."""
    try:
        import webview
    except ImportError:
        return False
    _ = webview.create_window("CoPaw", url, width=1200, height=800)
    webview.start(debug=False)
    return True


@click.command("gui")
@click.option(
    "--host",
    default="127.0.0.1",
    show_default=True,
    help="Bind host for the backend server",
)
@click.option(
    "--port",
    default=0,
    type=int,
    show_default=True,
    help="Bind port (0 = auto-select free port)",
)
def gui_cmd(host: str, port: int) -> None:
    """Run CoPaw in a desktop window (webview). Starts the app server and
    opens it in a standalone window. Logs are written to working dir.
    """
    os.environ[LOG_LEVEL_ENV] = os.environ.get(LOG_LEVEL_ENV, "info")
    setup_logger("info")
    # When packaged, write GUI process logs to working dir for debugging.
    if getattr(sys, "frozen", False) or "__compiled__" in globals():
        WORKING_DIR.mkdir(parents=True, exist_ok=True)
        add_copaw_file_handler(WORKING_DIR / "copaw_gui.log")

    if port == 0:
        port = find_free_port(host)
    url = f"http://{host}:{port}/"

    # Build server command (works when run as script or as Nuitka-frozen exe)
    if getattr(sys, "frozen", False) or "__compiled__" in globals():
        cmd = [sys.executable, "app", "--host", host, "--port", str(port)]
    else:
        cmd = [
            sys.executable,
            "-m",
            "copaw",
            "app",
            "--host",
            host,
            "--port",
            str(port),
        ]

    WORKING_DIR.mkdir(parents=True, exist_ok=True)
    # Server writes to WORKING_DIR/copaw.log via add_copaw_file_handler
    popen_kw: dict = {
        "env": {**os.environ},
        "start_new_session": True,
    }
    if getattr(sys, "frozen", False) or "__compiled__" in globals():
        popen_kw["cwd"] = str(WORKING_DIR)
    try:
        proc = subprocess.Popen(  # pylint: disable=consider-using-with
            cmd,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            **popen_kw,
        )
    except Exception as e:
        logger.exception("Failed to start server: %s", e)
        click.echo(f"Failed to start server: {e}", err=True)
        raise click.Abort()

    try:
        if not _server_ready(host, port):
            click.echo("Server did not become ready in time.", err=True)
            proc.terminate()
            proc.wait(timeout=5)
            raise click.Abort()
    except Exception:
        proc.terminate()
        proc.wait(timeout=5)
        raise

    if not _run_webview_blocking(url):
        click.echo(
            "pywebview not installed. Install with: pip install pywebview",
            err=True,
        )
        click.echo(f"Opening in browser: {url}")
        import webbrowser

        webbrowser.open(url)
        try:
            proc.wait()
        except KeyboardInterrupt:
            proc.terminate()
    else:
        proc.terminate()
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
