# -*- coding: utf-8 -*-
"""Real subprocess fixture for synchronous SDK shutdown tests."""

from __future__ import annotations

import json
import sys
import threading
import time
from pathlib import Path

marker = Path(sys.argv[1])
mode = sys.argv[2]
methods = []

for line in sys.stdin:
    request = json.loads(line)
    method = request[f"method"]
    methods.append(method)
    if method == f"initialize":
        response = {
            f"id": request[f"id"],
            f"result": {
                f"protocolVersion": 99 if mode == f"badinit" else 3,
                f"serverInfo": {f"name": f"fixture", f"version": f"1"},
            },
        }
    elif method in {f"fixture/notify", f"fixture/wait"}:
        response = {f"method": f"fixture/received", f"params": {}}
    else:
        continue
    sys.stdout.write(f"{json.dumps(response)}\n")
    sys.stdout.flush()

if mode == f"hold":
    threading.Event().wait()

payload = {
    f"method": f"fixture/drain",
    f"params": {f"text": f"x" * (2 * 1024 * 1024)},
}
sys.stdout.write(f"{json.dumps(payload)}\n")
sys.stdout.flush()
time.sleep(0.05)
marker.write_text(json.dumps({f"methods": methods}), encoding=f"utf-8")
sys.exit(7 if mode in {f"7", f"badinit"} else 0)
