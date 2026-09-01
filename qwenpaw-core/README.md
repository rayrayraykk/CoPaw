# QwenPaw Core

QwenPaw Core is the Rust runtime shared by the QwenPaw VS Code extension,
desktop application, and WebUI. It is currently staged in the existing CoPaw
repository under `qwenpaw-core/`; the code and release boundary remain
self-contained so it can be extracted to a dedicated repository later.

The repository is under active development. VS Code uses the stdio app-server
protocol, while Desktop always starts Rust Core and serves the unchanged
Console over a random loopback HTTP port. New Desktop bundles do not package or
start the legacy Python backend.
The built-in Desktop agent persists Coding Mode in Core SQLite and uses the
unchanged Files/Source Control UI against Rust-owned Workspace Git endpoints.
The unchanged language switcher also persists its seven current UI language
choices through Rust Core and restores the preference after a Core restart.

## Architecture and migration

- [System overview](docs/architecture/system-overview.md)
- [App Protocol](docs/architecture/app-protocol.md)
- [Generated App Protocol inventory](docs/api-contract/app-protocol-inventory.md)
- [Existing QwenPaw Web API inventory](docs/api-contract/web-api-inventory.md)
- [Python to Rust migration matrix](docs/migration/python-to-rust-matrix.md)
- [Reviewed execution plan and checklist](docs/plans/rust-core-refactor-plan.md)

## Development

Requirements:

- Rust 1.88 or newer;
- Node.js 20 or newer for the VS Code extension.

Build and test the Rust workspace:

```shell
cargo build --workspace
cargo test --workspace
```

Regenerate the versioned App Protocol schema, fixtures, inventory, and
TypeScript SDK after changing a protocol type:

```shell
cargo run -p qwenpaw-protocol --example generate_contract
```

Protocol tests fail when these checked-in artifacts drift from the Rust types.
The VS Code repository consumes the generated SDK instead of maintaining
parallel handwritten interfaces.

Tagged Core releases build native archives for macOS arm64/x64, Linux x64, and
Windows x64. macOS jobs require Developer ID signing and notarization secrets;
the release fails closed when they are unavailable. The product repository
pins the resulting tag and asset names in `extensions/vscode/core-release.json`.

Run the stdio app server:

```shell
cargo run -p qwenpaw-cli -- app-server --stdio
```

Run the same App Protocol over a loopback WebSocket, with HTTP health probes:

```shell
cargo run -p qwenpaw-cli -- app-server --listen 127.0.0.1:8765
```

The WebSocket endpoint is `/app-protocol`; `/healthz` and `/readyz` return JSON
status responses. Non-loopback listeners fail closed. Browser origins must be
same-origin or explicitly listed in `QWENPAW_ALLOWED_ORIGINS` as a comma-
separated development allowlist.

Expose App Protocol remotely only with WSS and bearer authentication:

```shell
cargo run -p qwenpaw-cli -- app-server \
  --listen 0.0.0.0:8765 \
  --remote \
  --tls-cert /secure/server-cert.pem \
  --tls-key /secure/server-key.pem \
  --auth-token-file /secure/qwenpaw-token
```

The token file must contain 32 through 4096 printable ASCII bytes. On Unix the
token file and TLS private key must not be accessible by group or other users.
Core re-reads the token file for each WSS handshake, so an atomic file
replacement rotates the token for new connections without putting the secret
in process arguments or logs. Existing authenticated connections remain active
until they disconnect.

Build the VS Code extension from the parent CoPaw repository:

```shell
cd ../extensions/vscode
npm install
npm run check
```

## Runtime configuration

The first model provider uses an OpenAI-compatible Chat Completions endpoint:

| Environment variable | Default |
|---|---|
| `QWENPAW_API_KEY` | Falls back to `OPENAI_API_KEY` |
| `QWENPAW_BASE_URL` | `https://dashscope.aliyuncs.com/compatible-mode/v1` |
| `QWENPAW_MODEL` | `qwen3-coder-plus` |
| `QWENPAW_MCP_CONFIG` | Unset; optional MCP JSON or legacy `agent.json` path |
| `QWENPAW_ALLOWED_ORIGINS` | Unset; optional browser Origin allowlist for the loopback WebSocket |
| `QWENPAW_MAX_CONTEXT_MESSAGES` | `128`, clamped to `32` through `512` |
| `QWENPAW_MAX_CONTEXT_BYTES` | `4194304`, clamped to `65536` through `67108864` |
| `QWENPAW_MODEL_HEADER_TIMEOUT_MS` | `60000`, clamped to `100` through `300000` |
| `QWENPAW_MODEL_STREAM_IDLE_TIMEOUT_MS` | `60000`, clamped to `100` through `300000` |

`QWENPAW_BASE_URL` and `QWENPAW_MODEL` bootstrap a new Core data directory.
Successful App Protocol `config/write` calls persist their replacements in the
same SQLite database and those values win on later Core restarts. A client that
owns the process, such as the VS Code extension, synchronizes its current
window settings immediately after initialization. API keys are never persisted
by Core and must continue to come from the process environment or a client's
secure credential store. Desktop UI language, Coding Mode, and the preferred
Workspace are separate non-secret Core settings in that database.

Logs are written to stderr so that stdout remains a clean JSONL protocol
transport.

Before each model request, persisted messages are grouped into complete user
turns. Core always retains the system prompt and the newest tool-call chain,
then keeps as many recent complete turns as the configured serialized-message
limits allow. Oversized content in the newest turn is truncated against its
actual JSON representation. Core fails the request when required protocol
metadata alone cannot fit instead of sending an unbounded request.

Model HTTP requests disable redirects so credentials cannot be forwarded to a
different endpoint. Core bounds response-header and stream-idle waits, requires
`text/event-stream`, limits individual SSE events to 262,144 bytes and HTTP
error bodies to 65,536 bytes, and requires an explicit `[DONE]` event. A timeout,
rate limit, malformed stream, oversized event, or premature disconnect is
persisted as a failed Turn and surfaced through `turn/completed`.

## Current agent tools

- `list_files` enumerates workspace source files while skipping generated
  dependency and build directories;
- `search_text` performs bounded literal searches over UTF-8 workspace files;
- `read_file` reads files whose canonical path remains inside the Thread's
  Workspace Root;
- `replace_text` performs a guarded single-match edit after a one-time client
  approval;
- `write_file` replaces a file inside an existing Workspace directory after a
  one-time client approval;
- `shell` runs in that Workspace Root after a one-time client approval;
- `shell.timeoutMs` defaults to 120 seconds and is clamped to the Core-enforced
  range of 100 milliseconds through 10 minutes;
- tool results are persisted in the Thread and returned to the model for the
  next bounded agent-loop step.

## MCP transports

The first MCP slice uses the official Rust MCP SDK and accepts the original
QwenPaw client shape under either `{"clients": ...}` or
`{"mcp":{"clients": ...}}`. Enabled stdio clients support `command`, `args`,
`env`, `cwd`, and an optional `tools` whitelist. Remote clients support
`streamable_http` (including `http` aliases) and legacy `sse`, with `url`,
`headers`, and the same whitelist. Environment placeholders such as `${TOKEN}`
are resolved only when the client starts.

Discovered tools are exposed as `mcp__<client>__<tool>` and always require a
one-time client approval. Core bounds client and tool counts, serialized tool
definitions, results, HTTP headers, SSE events, OAuth token responses, startup,
discovery, and call duration. Remote redirects are disabled; legacy SSE POST
endpoints must remain same-origin. HTTP headers are held as sensitive values,
and remote transport errors do not log endpoint URLs. Interrupting a Turn
cancels the MCP connection and returns immediately while transport cleanup
continues.

Bearer credentials may be provided as an `Authorization` header or through
the legacy-compatible `oauth.accessToken`. An expired token is refreshed when
`oauth.clientId`, `oauth.refreshToken`, and `oauth.tokenEndpoint` are present.
The response is bounded to 64 KiB and must return a Bearer token. A remote
client with `"oauth": {}` explicitly enables interactive browser OAuth through
the Console or App Protocol. Access and refresh tokens stay in the operating
system credential store; plain HTTP MCP clients never access that store.
