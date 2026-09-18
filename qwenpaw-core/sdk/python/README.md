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

Normal `close()` (including context-manager exit) sends stdin EOF, keeps reading
stdout and waits for the owned Core. Concurrent calls share the result; nonzero
or signal exits raise `ShutdownError`. Normal cleanup has a 30-second deadline;
forced termination and cleanup get five more seconds and still report failure,
not a successful save. Repeated close retains that failure. Create a new client
to reconnect; a closed instance cannot start another process.

Reader callbacks can request close without waiting for themselves. A callback
that owns shutdown hands remaining output to a drainer. A reader callback that
encounters another active closer returns so reading can continue; an ordinary
caller waits for the shared outcome. Close notification callbacks run after the
outcome is published. Do not use a reentrant callback call as an independent
persistence acknowledgement. Arbitrary blocked user callbacks cannot be forcibly
stopped as Python threads; incomplete reader cleanup raises an error and retains
the handle instead of closing its stream from another thread.

Existing startup or context-body exceptions are preserved if cleanup also fails.
Current Core returns a nonzero exit after final Turn persistence failures, which
`close()` propagates as a shutdown failure. Successful exit is still not an
unconditional acknowledgement of every storage operation.

Current CLI builds hold an exclusive data-directory lock before startup recovery.
A second process using the same `QWENPAW_HOME` fails instead of recovering a live
process's turns. Close the first owner before reopening. Do not delete
`.core-instance.lock` to bypass a running owner; the file remains after exit but
its OS lock is released. Explicitly independent processes need different data
directories. Automatic shared-host attachment and full default Workspace
services/background scheduling are not implemented yet.

## Connect to an existing Core

```python
from pathlib import Path

from qwenpaw_sdk import WebSocketConnection, WebSocketOptions

with WebSocketConnection.connect(
    f"ws://127.0.0.1:8088/app-protocol",
    WebSocketOptions(client_name=f"example"),
) as connection:
    thread = connection.thread_start(workspace_root=Path.cwd())
    result = thread.run(f"Summarize this repository")
```

Connected mode owns only its network connection and reuses the existing request,
notification and Thread implementation. It never spawns a Core, discovers hosts,
reads model keys from the environment or changes global model configuration.
The existing `QwenPaw` / `AppServerClient.start()` defaults remain owned stdio.

Plain WS accepts literal loopback IPs only, not `ws://localhost`. Use WSS remotely,
with `WebSocketOptions(bearer_token=..., ca_pem=...)` when required. A private CA
PEM replaces default trust roots without disabling certificate or hostname checks.
Tokens must be 32–4096 printable ASCII characters. URL user information, query,
fragment and control characters are rejected. Transport errors and option repr
do not disclose credentials; library debug logging is disabled on a private logger.
Application responses may contain sensitive data and must not be logged blindly.
Automatic proxies, compression, redirects, reconnect and replay are disabled.

`connect(..., cancel=threading.Event())` supports cooperative cancellation until
initialization completes. TCP/TLS/WS establishment is bounded to 15 seconds;
initialization uses `request_timeout` (15 seconds by default). Operations use a
dedicated asyncio I/O thread behind the synchronous API. A separate protocol
reader dispatches callbacks so blocking callbacks do not block network cleanup.
Messages are limited to 1 MiB; the WS transport checks a 2 MiB outgoing buffer
budget. These checks do not cap memory owned by arbitrary concurrent callers.

`disconnect()` stops admission, wakes pending requests, performs the WS close
handshake and stops local workers. It has a five-second disconnect budget and
up to one additional second for emergency I/O-thread cleanup. Repeated/concurrent
calls retain the result; reentrant calls from the closing owner, protocol reader
or close callback do not self-wait. A reader callback that initiated disconnect exits after that
callback returns. An unrelated blocked callback cannot be killed safely: close
reports incomplete reader/callback cleanup instead of claiming success. Explicit
close notifications run on a separate callback thread within the remaining budget.
`dispose()` aborts network I/O without a close handshake and gives close callbacks
up to one second, reporting incomplete cleanup if they remain blocked. It does not
wait for an already running protocol-reader callback. Keep callbacks short; Python
cannot safely kill arbitrary user threads. Use the context manager or disconnect for
normal cleanup, not garbage collection. Context-body exceptions remain primary.

Disconnecting never stops the shared host or its accepted WS turns and does not
acknowledge background persistence. Use `Thread.interrupt()` explicitly to cancel
a turn. The original Console's cancel-on-SSE-disconnect behavior is unchanged.
Explicit connections in all three SDKs are not automatic shared-host discovery,
default Workspace wiring, or full native-client integration.

Run the local checks in the repository's `qwenpaw` conda environment:

```shell
conda run -n qwenpaw python -m unittest discover -s tests
```

Set `QWENPAW_CORE_BIN` to a source build to include real-Core tests. Full checks
assert no skipped tests and inspect SQLite read-only before restarting Core.
The suite includes a real 30-second timeout case. Two additional fault-injection
tests shorten only their private test deadlines to exercise blocked callbacks
and write locks; production defaults and the real timeout test stay unchanged.
WS fixtures shorten only private test deadlines or inject stalled coroutines and
buffer measurements; they do not emulate every OS TCP buffer. Real WSS checks
require OpenSSL for temporary certificates. Set `QWENPAW_TS_SDK_ENTRY` to the built
TypeScript SDK's `dist/src/index.js` to also run the mixed-language Core test;
Node must be on PATH. This is a test-only dependency, not a Python runtime dependency.
