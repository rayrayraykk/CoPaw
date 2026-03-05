# -*- coding: utf-8 -*-
"""Find a free port for binding the app server."""
from __future__ import annotations

import socket


def find_free_port(
    host: str = "127.0.0.1",
    start: int = 8088,
    max_tries: int = 20,
) -> int:
    """Return a port number that is free to bind on the given host.

    Tries start, start+1, ... up to max_tries attempts.
    """
    for i in range(max_tries):
        port = start + i
        try:
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
                sock.bind((host, port))
                return port
        except OSError:
            continue
    raise RuntimeError(f"No free port in range [{start}, {start + max_tries})")
