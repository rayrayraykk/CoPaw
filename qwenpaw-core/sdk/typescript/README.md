# QwenPaw TypeScript SDK

Typed Node.js client for `qwenpaw-core app-server`. The SDK owns App Protocol
transport, initialization, request correlation, and notifications. Agent logic
continues to run exclusively in Rust Core.

```typescript
import { QwenPaw } from "@qwenpaw/sdk";

const qwenpaw = await QwenPaw.start({
  clientInfo: {
    name: "example",
    title: "QwenPaw SDK Example",
    version: "0.2.0",
  },
});

const thread = await qwenpaw.startThread({ workspaceRoot: process.cwd() });
const result = await thread.run("Summarize this repository");
console.log(result.finalResponse);
await qwenpaw.close();
```

Run `npm run check` to build the package and verify it against the shared App
Protocol fixtures. Full Core acceptance uses Node 24 and `QWENPAW_CORE_BIN`
pointing to a source build; its shutdown test inspects SQLite read-only before
reopening Core. These test requirements do not add SQLite to the SDK runtime.

Use `await qwenpaw.close()` for normal shutdown. It stops new requests, sends
stdin EOF and drains output while Core interrupts active work and finishes its
shutdown handlers. Concurrent/repeated calls share the same result. Nonzero or
signal exits reject; after 30 seconds it force-terminates its owned child and
waits up to five more seconds, reporting failure rather than a successful save.
Synchronous `dispose()` remains immediate termination and cannot guarantee
persistence. Startup failure cleanup is also not graceful shutdown.

Current Core reports final Turn persistence failures through a nonzero exit;
`close()` therefore rejects those failures. Successful exit is still not an
unconditional acknowledgement of every storage operation.

Current CLI builds hold an exclusive data-directory lock before startup recovery.
A second process using the same `QWENPAW_HOME` fails instead of recovering a live
process's turns. Await the first owner's close before reopening. Do not delete
`.core-instance.lock` to bypass a running owner; the file remains after exit but
its OS lock is released. Explicitly independent processes need different data
directories. Automatic attachment to a shared host and full default Workspace
services/background scheduling are not implemented yet.

## Explicitly connect to an existing Core

```typescript
import { WebSocketConnection } from "@qwenpaw/sdk";

const connection = await WebSocketConnection.connect(
  "ws://127.0.0.1:8088/app-protocol",
  { clientInfo: { name: "example", title: "Example", version: "1" } },
);
const thread = await connection.startThread({ workspaceRoot: process.cwd() });
const result = await thread.run("Summarize this repository");
await connection.disconnect();
```

This Node.js connection does not spawn or stop a Core, discover endpoints, or
change global model configuration. It reuses the typed `AppServerClient` and
`QwenPawThread` APIs. Each connection performs its own initialize/version check.
`QwenPaw.start()` and the VS Code default remain owned stdio, not automatic
shared-host attachment. Rust and Python also offer explicit WS/WSS connections;
this does not imply complete cross-language or native-client feature parity.

Plaintext WS requires a literal loopback IP (`ws://localhost` is rejected).
For remote hosts use WSS, `bearerToken` and, for a private CA, optional PEM `ca`
roots (string, Buffer or array). Explicit CA roots replace default roots, never
disable certificate/hostname verification. Tokens must be 32–4096 printable
ASCII bytes. They are sent only in Authorization; URL user information, queries
and fragments are rejected. Connection inspection and transport errors do not
include credentials or reflected handshake responses. Application messages may
still contain sensitive data; do not log them indiscriminately.

The whole WS/TLS handshake is limited to 15 seconds; protocol initialization
uses `requestTimeoutMs` (15 seconds by default). Optional `signal` cancels
establishment/initialization, not subsequent turns. Messages are limited to
1 MiB and queued WS writes to 2 MiB. There is no redirect, TLS downgrade,
automatic reconnect or request replay. The runtime `ws` dependency is pure JS;
optional native accelerators are not required. This is not a browser SDK.

`disconnect()` immediately rejects new/pending protocol requests and shares one
completion/error across repeated calls. It waits up to five seconds for the WS
close handshake, then terminates only this connection and reports timeout.
`dispose()` terminates it immediately. Neither operation stops the host or
accepted WS turns, affects another client, nor acknowledges background saving.
Use `turn/interrupt` to cancel work deliberately. Original Console SSE
disconnect-cancels behavior is unchanged.

WS acceptance adds local fixtures and two real source Core checks. WSS tests
generate temporary certificates using OpenSSL; they never modify system roots.

For local images, use explicit workspace-relative paths:

```typescript
const result = await thread.run([
  { type: "image", path: "screenshots/example.png" },
  { type: "text", text: "Describe this screenshot" },
]);
```

Core performs the read and persists the snapshot; the SDK does not upload file
contents itself. Use forward slashes, including on Windows. Images must be
regular files inside the Thread workspace, with no symlink components or File
Guard exclusions. The default inline limit is 2 MiB per image. Larger images
produce a model-context omission notice. Ordinary `fileReference` inputs still
include only a path, never implicit image bytes. Public message history exposes
optional input metadata, not base64 snapshots.
