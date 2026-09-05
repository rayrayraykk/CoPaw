#!/usr/bin/env python3
"""Minimal llama-server contract fixture for browser smoke tests."""

from __future__ import annotations

import sys
from http.server import BaseHTTPRequestHandler
from http.server import HTTPServer


if "--version" in sys.argv:
    print("version: 8744")
    raise SystemExit(0)


port = int(sys.argv[sys.argv.index("--port") + 1])


class Handler(BaseHTTPRequestHandler):
    """Serve the llama.cpp readiness endpoint used by Rust Core."""

    def do_GET(self) -> None:  # noqa: N802
        """Return a healthy response only for the readiness endpoint."""
        self.send_response(200 if self.path == "/health" else 404)
        self.end_headers()
        self.wfile.write(b"ok")

    def log_message(self, *_args: object) -> None:
        """Keep browser smoke output deterministic."""


HTTPServer(("127.0.0.1", port), Handler).serve_forever()
