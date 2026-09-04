# QwenPaw Python SDK

Python client for `qwenpaw-core app-server`. The SDK starts the Rust runtime,
performs the App Protocol handshake, correlates JSON-RPC requests, streams
notifications, and exposes Thread/Turn helpers. Agent logic remains in Rust.

```python
from qwenpaw_sdk import QwenPaw

with QwenPaw() as qwenpaw:
    thread = qwenpaw.thread_start(workspace_root=f"/path/to/repository")
    result = thread.run(f"Summarize this repository")
    print(result.final_response)
```

The SDK looks for `qwenpaw-core` on `PATH`. Applications may instead pass an
explicit binary through `QwenPawConfig(core_bin=...)`.

Run the local checks in the repository's `qwenpaw` conda environment:

```shell
conda run -n qwenpaw python -m unittest discover -s tests
```
