"""Emit isolated CLI fixture bytes; never import or execute product code."""

import json
import os
import sys
import time
from pathlib import Path


def main():
    """Exercise independent stdout/stderr and explicit child termination."""
    mode = os.environ[f"QWENPAW_MCP_FIXTURE_MODE"]
    Path(f"mcp-started").write_text(f"{os.getpid()}", encoding=f"utf-8")
    if mode == f"success":
        print(
            json.dumps(
                [
                    {
                        f"name": f"fixture",
                        f"transport": {
                            f"type": f"stdio",
                            f"command": f"private",
                        },
                        f"enabled": True,
                        f"auth_status": f"supported",
                        f"env": {f"TOKEN": f"not-public"},
                    }
                ]
            )
        )
    elif mode == f"error":
        sys.stderr.buffer.write(
            bytes().join(
                (
                    f"  denied ".encode(f"utf-8"),
                    bytes([255]),
                    f" \n".encode(f"utf-8"),
                )
            )
        )
        sys.stderr.flush()
        sys.exit(7)
    elif mode == f"invalid":
        print(f"not json")
    elif mode == f"both":
        sys.stderr.write(f" " * (512 * 1024))
        sys.stderr.flush()
        print(f"[]")
    elif mode in (f"stdout-limit", f"stderr-limit"):
        stream, size = (
            (sys.stdout, 9 * 1024 * 1024)
            if mode == f"stdout-limit"
            else (sys.stderr, 2 * 1024 * 1024)
        )
        stream.write(f"x" * size)
        stream.flush()
        time.sleep(30)
    elif mode == f"wait":
        time.sleep(30)
    else:
        raise AssertionError(f"Unknown fixture mode: {mode}")


if __name__ == f"__main__":
    main()
