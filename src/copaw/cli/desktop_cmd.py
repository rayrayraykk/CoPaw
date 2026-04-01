# -*- coding: utf-8 -*-
"""CLI command: run CoPaw app on a free port in a native webview window."""
# pylint:disable=too-many-branches,too-many-statements,consider-using-with
from __future__ import annotations

import logging
import os
import socket
import subprocess
import sys
import threading
import time
import traceback
import webbrowser

import click

from ..constant import LOG_LEVEL_ENV
from ..utils.logging import setup_logger

try:
    import webview
except ImportError:
    webview = None  # type: ignore[assignment]

logger = logging.getLogger(__name__)


class WebViewAPI:
    """API exposed to the webview for handling external links."""

    def open_external_link(self, url: str) -> None:
        """Open URL in system's default browser."""
        if not url.startswith(("http://", "https://")):
            return
        webbrowser.open(url)


def _find_free_port(host: str = "127.0.0.1") -> int:
    """Bind to port 0 and return the OS-assigned free port."""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind((host, 0))
        sock.listen(1)
        return sock.getsockname()[1]


def _wait_for_http(host: str, port: int, timeout_sec: float = 300.0) -> bool:
    """Return True when something accepts TCP on host:port."""
    deadline = time.monotonic() + timeout_sec
    while time.monotonic() < deadline:
        try:
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                s.settimeout(2.0)
                s.connect((host, port))
                return True
        except (OSError, socket.error):
            time.sleep(1)
    return False


def _stream_reader(in_stream, out_stream) -> None:
    """Read from in_stream line by line and write to out_stream.

    Used on Windows to prevent subprocess buffer blocking. Runs in a
    background thread to continuously drain the subprocess output.
    """
    try:
        for line in iter(in_stream.readline, ""):
            if not line:
                break
            out_stream.write(line)
            out_stream.flush()
    except Exception:
        pass
    finally:
        try:
            in_stream.close()
        except Exception:
            pass


def _create_loading_html(
    host: str,
    port: int,
) -> str:
    """Create loading page HTML with progress bar animation."""
    # pylint: disable=unused-argument
    # SVG paths for CoPaw logo (split to avoid line length issues)
    svg_circle = (
        "M150.41,91.07c-7.76-7.58-16.99-13.51-27.67-17.84-10.69-4.33"
        "-22.28-6.5-34.76-6.5s-24.39,2.21-35.07,6.64c-10.69,4.43-19.95,"
        "10.48-27.81,18.13-7.86,7.68-14.01,16.58-18.44,26.77-4.44,10.16"
        "-6.66,21.11-6.66,32.8v.59c0,11.71,2.21,22.63,6.66,32.82,4.43,"
        "10.16,10.53,19.04,18.29,26.6,7.75,7.55,16.98,13.51,27.65,17.84,"
        "10.69,4.33,22.28,6.5,34.78,6.5s24.38-2.21,35.06-6.64c10.69-4.43,"
        "19.95-10.48,27.81-18.13,7.86-7.68,14.01-16.58,18.45-26.77,4.43"
        "-10.16,6.64-21.11,6.64-32.8v-.62c0-11.69-2.21-22.61-6.64-32.8"
        "-4.44-10.19-10.54-19.04-18.29-26.6ZM130,151.67c0,6.05-.96,11.76"
        "-2.88,17.1s-4.69,10.01-8.3,14.05c-3.63,4.03-8.02,7.21-13.15,"
        "9.52-5.14,2.31-11.05,3.47-17.69,3.47s-12.25-1.21-17.38-3.62c"
        "-5.14-2.41-9.63-5.68-13.46-9.82-3.83-4.13-6.75-8.88-8.77-14.22s"
        "-3.01-11.02-3.01-17.07v-.62c0-6.03.95-11.74,2.87-17.07s4.69"
        "-10.01,8.32-14.05c3.63-4.03,8.01-7.21,13.15-9.52,5.14-2.34,"
        "11.03-3.49,17.69-3.49,6.24,0,11.98,1.21,17.22,3.64,5.24,2.41,"
        "9.78,5.68,13.61,9.82,3.83,4.13,6.75,8.88,8.77,14.22,2.02,5.34,"
        "3.03,11.02,3.03,17.07v.59Z"
    )
    svg_paw1 = (
        "M71.01,38.34c2.06,2.01,4.51,3.59,7.35,4.74,2.84,1.15,5.92,"
        "1.73,9.24,1.73s6.48-.59,9.31-1.76c2.84-1.18,5.3-2.78,7.39-4.82,"
        "2.09-2.04,3.72-4.41,4.9-7.11,1.18-2.7,1.76-5.61,1.76-8.71v-.16c0"
        "-3.1-.59-6.01-1.76-8.71-1.18-2.71-2.8-5.06-4.86-7.07-2.06-2.01"
        "-4.51-3.59-7.35-4.74-2.84-1.15-5.92-1.73-9.24-1.73s-6.48.59"
        "-9.32,1.76c-2.84,1.18-5.3,2.78-7.39,4.82-2.09,2.04-3.72,4.4"
        "-4.9,7.11-1.18,2.7-1.77,5.61-1.77,8.71v.16c0,3.11.59,6.01,1.77,"
        "8.72,1.18,2.7,2.8,5.06,4.86,7.06Z"
    )
    svg_paw2 = (
        "M14.35,54.93c2.06,2.01,4.51,3.59,7.35,4.74,2.84,1.15,5.92,"
        "1.73,9.24,1.73s6.48-.59,9.31-1.77c2.84-1.18,5.3-2.78,7.39-4.82,"
        "2.09-2.04,3.72-4.4,4.9-7.11,1.18-2.7,1.77-5.61,1.77-8.71v-.16c0"
        "-3.1-.59-6-1.77-8.71-1.18-2.71-2.8-5.06-4.86-7.06-2.06-2.01"
        "-4.51-3.59-7.35-4.74-2.84-1.15-5.92-1.73-9.24-1.73s-6.48.59"
        "-9.32,1.77c-2.84,1.18-5.3,2.78-7.39,4.82-2.09,2.04-3.72,4.41"
        "-4.9,7.11-1.18,2.7-1.77,5.61-1.77,8.71v.16c0,3.11.59,6.01,1.77,"
        "8.72,1.18,2.7,2.8,5.06,4.86,7.07Z"
    )
    svg_paw3 = (
        "M127.67,54.93c2.06,2.01,4.51,3.59,7.35,4.74,2.84,1.15,5.92,"
        "1.73,9.24,1.73s6.48-.59,9.31-1.77c2.84-1.18,5.3-2.78,7.39-4.82,"
        "2.09-2.04,3.72-4.4,4.9-7.11,1.18-2.7,1.76-5.61,1.76-8.71v-.16c0"
        "-3.1-.59-6-1.76-8.71-1.18-2.71-2.8-5.06-4.86-7.06-2.06-2.01"
        "-4.51-3.59-7.35-4.74-2.84-1.15-5.92-1.73-9.23-1.73s-6.48.59"
        "-9.32,1.77c-2.84,1.18-5.3,2.78-7.39,4.82-2.09,2.04-3.72,4.41"
        "-4.9,7.11-1.18,2.7-1.77,5.61-1.77,8.71v.16c0,3.11.59,6.01,1.77,"
        "8.72,1.18,2.7,2.8,5.06,4.86,7.07Z"
    )
    return f"""<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>CoPaw - Starting...</title>
    <style>
        * {{
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }}

        body {{
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI",
                        "Roboto", "Helvetica Neue", Arial, sans-serif;
            background: #f9f8f4;
            display: flex;
            align-items: center;
            justify-content: center;
            min-height: 100vh;
            color: rgba(0, 0, 0, 0.85);
            overflow: hidden;
        }}

        .loading-content {{
            text-align: center;
            max-width: 500px;
            padding: 40px;
        }}

        .loading-logo {{
            width: 80px;
            height: 80px;
            margin: 0 auto 32px;
            animation: pulse 2s ease-in-out infinite;
        }}

        .loading-title {{
            font-size: 36px;
            font-weight: 600;
            margin-bottom: 12px;
            letter-spacing: 0.5px;
            color: rgba(0, 0, 0, 0.88);
        }}

        .loading-subtitle {{
            font-size: 15px;
            opacity: 0.55;
            margin-bottom: 56px;
            font-weight: 400;
        }}

        .progress-container {{
            width: 100%;
            height: 4px;
            background: rgba(0, 0, 0, 0.06);
            border-radius: 2px;
            overflow: hidden;
            margin-bottom: 16px;
        }}

        .progress-bar {{
            height: 100%;
            background: #FF7F16;
            border-radius: 2px;
            width: 0%;
            transition: width 0.3s ease-out;
        }}

        .progress-text {{
            font-size: 13px;
            opacity: 0.55;
            font-weight: 400;
            margin-bottom: 8px;
        }}

        .elapsed-time {{
            font-size: 12px;
            opacity: 0.35;
            font-weight: 400;
            margin-bottom: 32px;
        }}

        .tip-text {{
            font-size: 13px;
            color: rgba(0, 0, 0, 0.55);
            text-align: center;
            min-height: 20px;
            transition: opacity 0.3s ease;
        }}

        .tip-link {{
            color: #FF7F16;
            text-decoration: none;
            cursor: pointer;
            border-bottom: 1px solid transparent;
            transition: border-color 0.2s ease;
        }}

        .tip-link:hover {{
            border-bottom-color: #FF7F16;
        }}

        .fade-out {{
            animation: fadeOut 0.8s ease-out forwards;
        }}

        @keyframes pulse {{
            0%, 100% {{
                opacity: 1;
                transform: scale(1);
            }}
            50% {{
                opacity: 0.9;
                transform: scale(1.03);
            }}
        }}

        @keyframes fadeOut {{
            to {{
                opacity: 0;
                transform: translateY(-10px);
            }}
        }}
    </style>
</head>
<body>
    <div class="loading-content" id="loading-content">
        <svg class="loading-logo" viewBox="0 0 175.35 235.42"
             xmlns="http://www.w3.org/2000/svg">
            <path d="{svg_circle}" fill="rgba(0,0,0,0.85)"/>
            <path d="{svg_paw1}" fill="#FF9D4D"/>
            <path d="{svg_paw2}" fill="#FF9D4D"/>
            <path d="{svg_paw3}" fill="#FF9D4D"/>
        </svg>
        <h1 class="loading-title">CoPaw</h1>
        <p class="loading-subtitle">Starting up...</p>

        <div class="progress-container">
            <div class="progress-bar" id="progress-bar"></div>
        </div>
        <div class="progress-text" id="progress-text">Initializing...</div>
        <div class="elapsed-time" id="elapsed-time">0s</div>

        <div class="tip-text" id="tip-text"></div>
    </div>

    <script>
        let progress = 0;
        let backendReady = false;
        const startTime = Date.now();
        let currentTipIndex = 0;

        const tips = [
            {{
                text: '你可以在<a class="tip-link" href="#" \
data-url="https://copaw.agentscope.io/docs/quickstart">\
快速开始</a>查看安装和配置指南',
            }},
            {{
                text: '访问<a class="tip-link" href="#" \
data-url="https://copaw.agentscope.io/docs/console">\
控制台文档</a>了解如何使用Web界面',
            }},
            {{
                text: '在<a class="tip-link" href="#" \
data-url="https://copaw.agentscope.io/docs/channels">\
频道配置</a>中接入钉钉、飞书等应用',
            }},
            {{
                text: '浏览<a class="tip-link" href="#" \
data-url="https://copaw.agentscope.io/docs/skills">\
技能文档</a>探索CoPaw的各种能力',
            }},
            {{
                text: '查看<a class="tip-link" href="#" \
data-url="https://copaw.agentscope.io/docs/models">\
模型配置</a>了解如何设置API Key或本地模型',
            }},
        ];

        function openInBrowser(url) {{
            if (window.pywebview && window.pywebview.api) {{
                window.pywebview.api.open_external_link(url);
            }} else {{
                window.open(url, '_blank');
            }}
        }}

        function updateTip() {{
            const tipElement = document.getElementById('tip-text');
            tipElement.style.opacity = '0';

            setTimeout(() => {{
                tipElement.innerHTML = tips[currentTipIndex].text;
                const links = tipElement.querySelectorAll('.tip-link');
                links.forEach(link => {{
                    link.addEventListener('click', (e) => {{
                        e.preventDefault();
                        const url = e.target.getAttribute('data-url');
                        openInBrowser(url);
                    }});
                }});
                tipElement.style.opacity = '1';
                currentTipIndex = (currentTipIndex + 1) % tips.length;
            }}, 300);
        }}

        updateTip();
        setInterval(updateTip, 5000);

        function updateElapsed() {{
            const elapsed = Math.floor((Date.now() - startTime) / 1000);
            document.getElementById('elapsed-time').textContent =
                elapsed + 's';
        }}

        setInterval(updateElapsed, 1000);

        function updateProgress(value, text) {{
            progress = Math.min(value, 100);
            document.getElementById('progress-bar').style.width =
                progress + '%';
            if (text) {{
                document.getElementById('progress-text').textContent = text;
            }}
        }}

        function simulateProgress() {{
            if (progress < 90 && !backendReady) {{
                const increment = Math.random() * 3 + 1;
                const newProgress = Math.min(progress + increment, 90);

                let statusText = 'Initializing...';
                if (newProgress > 20) statusText = 'Loading components...';
                if (newProgress > 50) statusText = 'Starting services...';
                if (newProgress > 75) statusText = 'Almost ready...';

                updateProgress(newProgress, statusText);

                const delay = 200 + Math.random() * 300;
                setTimeout(simulateProgress, delay);
            }}
        }}

        function triggerReadyAnimation() {{
            backendReady = true;

            const currentProgress = progress;
            const step = (100 - currentProgress) / 20;
            let animProgress = currentProgress;

            const animateToFull = setInterval(() => {{
                animProgress += step;
                if (animProgress >= 100) {{
                    animProgress = 100;
                    clearInterval(animateToFull);
                    updateProgress(100, 'Ready!');

                    setTimeout(() => {{
                        document.getElementById('loading-content')
                            .classList.add('fade-out');
                    }}, 300);
                }} else {{
                    updateProgress(animProgress, 'Ready!');
                }}
            }}, 50);
        }}

        updateProgress(1, 'Initializing...');
        setTimeout(simulateProgress, 500);
    </script>
</body>
</html>"""


@click.command("desktop")
@click.option(
    "--host",
    default="127.0.0.1",
    show_default=True,
    help="Bind host for the app server.",
)
@click.option(
    "--log-level",
    default="info",
    type=click.Choice(
        ["critical", "error", "warning", "info", "debug", "trace"],
        case_sensitive=False,
    ),
    show_default=True,
    help="Log level for the app process.",
)
def desktop_cmd(
    host: str,
    log_level: str,
) -> None:
    """Run CoPaw app on an auto-selected free port in a webview window.

    Starts the FastAPI app in a subprocess on a free port, then opens a
    native webview window loading that URL. Use for a dedicated desktop
    window without conflicting with an existing CoPaw app instance.
    """
    # Setup logger for desktop command (separate from backend subprocess)
    setup_logger(log_level)

    port = _find_free_port(host)
    url = f"http://{host}:{port}"
    click.echo(f"Starting CoPaw app on {url} (port {port})")
    logger.info("Server subprocess starting...")

    env = os.environ.copy()
    env[LOG_LEVEL_ENV] = log_level

    if "SSL_CERT_FILE" in env:
        cert_file = env["SSL_CERT_FILE"]
        if os.path.exists(cert_file):
            logger.info(f"SSL certificate: {cert_file}")
        else:
            logger.warning(
                f"SSL_CERT_FILE set but not found: {cert_file}",
            )
    else:
        logger.warning("SSL_CERT_FILE not set on environment")

    is_windows = sys.platform == "win32"
    proc = None
    manually_terminated = False

    try:
        loading_html = _create_loading_html(host, port)

        proc = subprocess.Popen(
            [
                sys.executable,
                "-m",
                "copaw",
                "app",
                "--host",
                host,
                "--port",
                str(port),
                "--log-level",
                log_level,
            ],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE if is_windows else sys.stdout,
            stderr=subprocess.PIPE if is_windows else sys.stderr,
            env=env,
            bufsize=1,
            universal_newlines=True,
        )
        logger.info("Backend subprocess started")

        try:
            if is_windows:
                stdout_thread = threading.Thread(
                    target=_stream_reader,
                    args=(proc.stdout, sys.stdout),
                    daemon=True,
                )
                stderr_thread = threading.Thread(
                    target=_stream_reader,
                    args=(proc.stderr, sys.stderr),
                    daemon=True,
                )
                stdout_thread.start()
                stderr_thread.start()

            logger.info("Creating loading window immediately...")
            api = WebViewAPI()
            window = webview.create_window(
                "CoPaw Desktop",
                html=loading_html,
                width=1280,
                height=800,
                text_select=True,
                js_api=api,
            )

            def wait_and_switch():
                """Wait for backend, then trigger animation and switch."""
                logger.info("Waiting for backend to be ready...")
                if _wait_for_http(host, port, timeout_sec=300.0):
                    logger.info("Backend ready, triggering animation...")
                    try:
                        window.evaluate_js("triggerReadyAnimation();")
                        time.sleep(2.5)
                        logger.info("Switching to main app...")
                        window.load_url(url)
                    except Exception as e:
                        logger.error(f"Failed to switch window: {e}")
                else:
                    logger.error("Backend did not become ready in time")

            switch_thread = threading.Thread(
                target=wait_and_switch,
                daemon=True,
            )
            switch_thread.start()

            logger.info(
                "Starting webview (will auto-switch after animation)...",
            )
            webview.start(private_mode=False)
            logger.info("webview.start() returned (window closed).")
        except KeyboardInterrupt:
            pass
        finally:
            # Ensure backend process is always cleaned up
            # Wrap all cleanup operations to handle race conditions:
            # - Process may exit between poll() and terminate()
            # - terminate()/kill() may raise ProcessLookupError/OSError
            # - We must not let cleanup exceptions mask the original error
            if proc and proc.poll() is None:  # process still running
                logger.info("Terminating backend server...")
                manually_terminated = (
                    True  # Mark that we're intentionally terminating
                )
                try:
                    proc.terminate()
                    try:
                        proc.wait(timeout=5.0)
                        logger.info("Backend server terminated cleanly.")
                    except subprocess.TimeoutExpired:
                        logger.warning(
                            "Backend did not exit in 5s, force killing...",
                        )
                        try:
                            proc.kill()
                            proc.wait()
                            logger.info("Backend server force killed.")
                        except (ProcessLookupError, OSError) as e:
                            # Process already exited, which is fine
                            logger.debug(
                                f"kill() raised {e.__class__.__name__} "
                                f"(process already exited)",
                            )
                except (ProcessLookupError, OSError) as e:
                    # Process already exited between poll() and terminate()
                    logger.debug(
                        f"terminate() raised {e.__class__.__name__} "
                        f"(process already exited)",
                    )
            elif proc:
                logger.info(
                    f"Backend already exited with code {proc.returncode}",
                )

        # Only report errors if process exited unexpectedly
        # (not manually terminated)
        # On Windows, terminate() doesn't use signals so exit codes vary
        # (1, 259, etc.)
        # On Unix/Linux/macOS, terminate() sends SIGTERM (exit code -15)
        # Using a flag is more reliable than checking specific exit codes
        if proc and proc.returncode != 0 and not manually_terminated:
            logger.error(
                f"Backend process exited unexpectedly with code "
                f"{proc.returncode}",
            )
            # Follow POSIX convention for exit codes:
            # - Negative (signal): 128 + signal_number
            # - Positive (normal): use as-is
            # Example: -15 (SIGTERM) -> 143 (128+15), -11 (SIGSEGV) ->
            # 139 (128+11)
            if proc.returncode < 0:
                sys.exit(128 + abs(proc.returncode))
            else:
                sys.exit(proc.returncode or 1)
    except KeyboardInterrupt:
        logger.warning("KeyboardInterrupt in main, cleaning up...")
        raise
    except Exception as e:
        logger.error(f"Exception: {e!r}")
        traceback.print_exc(file=sys.stderr)
        sys.stderr.flush()
        raise
