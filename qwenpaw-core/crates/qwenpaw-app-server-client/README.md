# `QwenPaw` Rust App Server client

This crate owns typed App Protocol requests, response correlation and transport
lifecycle. Agent/model/tool execution stays in the Core process.

For a child process owned by the SDK, await shutdown before leaving its scope:

```rust
use qwenpaw_app_server_client::{ClientIdentity, StdioAppServer};
use std::path::Path;

async fn example(executable: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let server = StdioAppServer::spawn(executable)?;
    server.client().initialize(ClientIdentity::new("example", "1")).await?;
    // Send typed requests through server.client().
    server.shutdown().await?;
    Ok(())
}
```

`StdioAppServer::shutdown(self)` stops admission through every cloned client,
sends stdin EOF, drains stdout and waits for the owned Core to finish. If
`spawn_command` was configured with piped stderr, that pipe is drained too;
inherited stderr and file redirection retain the caller's configuration.
Nonzero/signal exits and transport failures return errors. After 30 seconds,
shutdown requests forced termination and waits up to five more seconds, still
returning an error rather than claiming persistence succeeded.

Dropping the owner or cancelling its shutdown future is emergency cleanup, not
graceful shutdown: it requests child termination and aborts transport readers.
Only the awaited path confirms a wait/reap result. `AppServerClient::shutdown`
alone remains a transport-only close for arbitrary byte streams; it does not
require an external peer process to exit. Raw notification subscriptions do not
constitute a storage acknowledgement.

Current Core reports final Turn persistence failures through a nonzero exit,
which owned-process shutdown propagates. Successful shutdown still is not an
unconditional acknowledgement of every storage operation.

Current CLI builds hold an exclusive data-directory lock before startup recovery.
A second process using the same `QWENPAW_HOME` fails instead of recovering a live
process's turns. Await the first owner's shutdown before reopening. Do not delete
`.core-instance.lock` to bypass a running owner; the file remains after exit but
its OS lock is released. Explicitly independent processes need different data
directories. This CLI protection does not apply to direct embedded Core instances.
Automatic shared-host attachment and full default Workspace services/background
scheduling are not implemented yet.

## Connect to an existing Core

`WebSocketConnection` owns only a local connection. It never starts, stops or
discovers a Core process, and never changes its model configuration:

```rust
use qwenpaw_app_server_client::{ClientIdentity, WebSocketConnection, WebSocketOptions};

async fn connect_example(endpoint: &str) -> Result<(), Box<dyn std::error::Error>> {
    let connection = WebSocketConnection::connect(endpoint, WebSocketOptions::default()).await?;
    connection.client().initialize(ClientIdentity::new("example", "1")).await?;
    // Send the same typed requests through connection.client().
    connection.disconnect().await?;
    Ok(())
}
```

Use a literal loopback IP for plaintext WS, for example
`ws://127.0.0.1:8088/app-protocol`; even `ws://localhost` is rejected.
Remote connections require WSS with certificate and hostname verification.
Supply `WebSocketOptions::bearer_token` for authenticated hosts. For a private
CA, provide a `rustls::RootCertStore` in `tls_roots`; this replaces public roots,
not verification. Tokens must be 32–4096 printable ASCII bytes and are sent only
as a sensitive Authorization header. URL user information, query strings and
fragments are rejected. Handshake errors expose only redacted categories or
HTTP status, not response headers/body. Redirects and automatic reconnect/replay
are not supported. Do not log application responses containing sensitive data.

Connection establishment is limited to 15 seconds, messages to 1 MiB and
`disconnect(self)` to five seconds. Disconnect refuses new requests through all
clones and wakes pending requests. Dropping the connection or cancelling its
disconnect future aborts the local worker. It does not interrupt accepted turns,
change another client's state or acknowledge background persistence. Use explicit
`turn/interrupt` when cancellation is intended. These semantics do not change
the original Console's cancel-on-SSE-disconnect behavior or owned stdio shutdown.

Existing client defaults still use owned stdio. TypeScript and Python also
offer explicit WS/WSS connections; this does not implement automatic
shared-host attachment or complete three-language parity.

Run the crate tests with:

```sh
CARGO_INCREMENTAL=0 cargo test -p qwenpaw-app-server-client
```

Lifecycle fixtures require Node on `PATH` and include
a real 30-second timeout case. The CLI's `sdk_client` integration target checks
a real source Core and reads its storage before startup recovery.
