# QwenPaw App Protocol

> Status: MVP v2

## Purpose

The App Protocol isolates QwenPaw clients from the Rust runtime implementation.
VS Code initially connects over newline-delimited JSON on stdio. Desktop, WebUI,
and remote transports will use the same request and event model.

## Lifecycle

1. The client starts `qwenpaw-core app-server --stdio`.
2. The client sends exactly one `initialize` request.
3. The client creates or resumes a thread.
4. The client starts a turn with text and optional Workspace file references.
5. The server streams item, tool, approval, and turn notifications.
6. The client answers approval requests when a guarded tool is proposed.
7. The client may interrupt an active turn, including while approval is pending.

## Wire format

On stdio, every line is one UTF-8 JSON object. On WebSocket, every text frame is
one JSON object. Request IDs may be JSON strings or numbers. Field and enum
names use camelCase.

Request:

```json
{"id":1,"method":"initialize","params":{"clientInfo":{"name":"qwenpaw_vscode","version":"0.1.0"}}}
```

Response:

```json
{"id":1,"result":{"protocolVersion":2,"serverInfo":{"name":"qwenpaw-core","version":"0.1.0"}}}
```

Notification:

```json
{"method":"item/agentMessage/delta","params":{"threadId":"...","turnId":"...","itemId":"...","delta":"hello"}}
```

Errors use JSON-RPC-compatible integer codes without requiring a `jsonrpc`
field. Unknown methods use `-32601`, invalid parameters use `-32602`, and
invalid lifecycle state uses `-32000`.

## Transports

- `app-server --stdio` uses newline-delimited JSON for a single local client.
- `app-server --listen 127.0.0.1:8765` exposes `/app-protocol` as a WebSocket,
  plus `/healthz` and `/readyz` over HTTP.
- Every WebSocket connection owns its own initialize state while all
  connections share the Core and persistent Threads.
- The HTTP server rejects non-loopback listeners. WebSocket text frames are
  capped at 1 MiB. Missing `Origin` is accepted for native clients; browser
  clients require a loopback same-origin Host or an explicit
  `QWENPAW_ALLOWED_ORIGINS` entry.

## Stable MVP methods

- `initialize`
- `thread/start`
- `thread/resume`
- `thread/archive`
- `thread/list`
- `thread/read`
- `turn/start`
- `turn/interrupt`
- `tool/approval/respond`
- `model/list`
- `config/read`
- `config/write`
- `workspace/list`
- `workspace/read`

`thread/archive` is recoverable: it rejects active Turns, persists the archived
state, and removes the Thread from default `thread/list` results. Clients can
set `includeArchived` to inspect archived Threads and call `thread/resume` to
return one to the active list. `thread/read` remains available while archived,
but `turn/start` fails until the Thread is resumed.

`config/read` returns the effective model ID, OpenAI-compatible base URL, and
only a boolean indicating whether an API key is configured. `config/write`
validates and atomically persists only `baseUrl` and `defaultModel` in SQLite;
API keys are never accepted or returned by App Protocol. Stored non-secret
settings are loaded on restart, while a client that owns the Core process may
explicitly synchronize its settings after `initialize`.

`workspace/list` aggregates normalized roots already registered by Threads,
including total and archived Thread counts. `workspace/read` only resolves an
exact root returned by that registry. It does not canonicalize, enumerate, or
otherwise probe arbitrary filesystem paths.

## Stable MVP notifications

- `thread/started`
- `turn/started`
- `item/started`
- `item/agentMessage/delta`
- `item/completed`
- `tool/approval/requested`
- `tool/approval/resolved`
- `turn/completed`

## Generated contract artifacts

`qwenpaw-protocol` is the source of truth. Its generator produces:

- `sdk/typescript/src/protocol.ts` for TypeScript clients;
- `docs/api-contract/app-protocol-v2.schema.json` for machine-readable payload
  validation;
- `docs/api-contract/fixtures/app-protocol-v2.json` with typed examples for
  every stable request, response, and server notification;
- `docs/api-contract/app-protocol-inventory.md` from the same method registry.

Run `cargo run -p qwenpaw-protocol --example generate_contract` after changing
a protocol type. Rust tests compare all checked-in artifacts to fresh generator
output. The VS Code extension additionally locks its copied SDK by protocol
version and SHA-256.

## Workspace and tools

`thread/start` accepts an optional `workspaceRoot`. The server canonicalizes it
and stores the resolved path on the Thread. Relative file access is resolved
against that immutable root. The built-in `list_files`, `search_text`, and
`read_file` tools are read-only and do not require approval. They reject
canonical paths outside the root, do not follow symlinks during discovery, and
bound result counts and searched file sizes.

`turn/start` accepts text and structured `fileReference` inputs. A reference
must resolve to an existing regular file inside the Thread Workspace. Core
accepts at most 32 references, bounds each path to 4,096 bytes, and validates
optional paired 1-based inclusive line ranges. Core passes only normalized
Workspace-relative paths and line metadata to the model; it does not read file
contents while resolving references. The model can use `read_file` when it
needs content.

The built-in `replace_text`, `write_file`, and `shell` tools always emit
`tool/approval/requested` before execution. `write_file` validates the
canonical target or parent directory before replacing content. `replace_text`
requires exactly one occurrence of `oldText`, preventing ambiguous edits.
`shell` runs with the workspace as its working directory. The client must send a
`tool/approval/respond` request with `approved` or `denied`. A denial is
returned to the model as a tool error so the agent can explain or choose
another action. Pending approval is also denied by timeout and is cancelled by
`turn/interrupt`. Shell calls accept an optional `timeoutMs`; Core clamps it to
100 through 600,000 milliseconds, defaults to 120,000 milliseconds, and kills
the child process when that deadline expires.

## Runtime and context limits

Core rejects Turn input larger than 262,144 bytes and bounds each streamed
Agent response to 1,048,576 bytes. One model step accepts at most 16 tool calls;
tool-call identifiers, names, and serialized arguments are incrementally
bounded before execution.

The OpenAI-compatible transport disables redirects, bounds response-header and
stream-idle waits, validates `text/event-stream`, limits HTTP error bodies to
65,536 bytes, and decodes SSE with a 262,144-byte per-event cap before JSON
parsing. A successful stream must end with `[DONE]`; EOF without it is treated
as an interrupted model response rather than a successful partial answer.
Rate limits and all other transport failures complete and persist the Turn with
`failed` status and a bounded error message.

Model requests are built from complete user-turn groups. The system message and
the newest group, including adjacent assistant tool calls and tool results, are
retained. Older groups are selected newest-first within both a message count
and an actual serialized-message JSON byte limit. If the newest group's content
is too large, Core truncates content while preserving the message structure; if
required metadata cannot fit, the request fails closed. Operators may configure
the limits with `QWENPAW_MAX_CONTEXT_MESSAGES` and
`QWENPAW_MAX_CONTEXT_BYTES`.
Model timeout bounds can be configured with
`QWENPAW_MODEL_HEADER_TIMEOUT_MS` and
`QWENPAW_MODEL_STREAM_IDLE_TIMEOUT_MS`.

## MCP lifecycle

When `QWENPAW_MCP_CONFIG` points to a compatible MCP JSON file, Core lazily
connects enabled stdio, Streamable HTTP, or legacy SSE servers, completes the
MCP initialize handshake, discovers their whitelisted tools, and keeps the
connections alive across agent steps.
Model-facing names use `mcp__<client>__<tool>` so tools from different servers
cannot silently shadow one another. Every MCP invocation follows the existing
`tool/approval/requested` flow regardless of the server's annotations.

Remote clients support custom sensitive headers, Bearer access tokens, and
refresh-token renewal for existing OAuth grants. Streamable HTTP uses the
official rmcp transport. The bounded legacy SSE adapter requires its advertised
POST endpoint to stay on the configured origin and disables redirects. An
interactive browser OAuth grant is not yet exposed through App Protocol.

Core supports at most 32 configured clients, 64 tools per server, 65,536
serialized bytes per tool definition, 1,048,576 bytes for the complete MCP
catalog, and 1,048,576 bytes per result or SSE event. HTTP headers are limited
to 64 entries and 16,384 bytes; OAuth token responses are limited to 65,536
bytes. Startup and discovery use 15-second bounds; calls use a 120-second bound.
`turn/interrupt` immediately returns a cancellation result and closes the
connection. For stdio, the official transport additionally enforces a
three-second graceful child-process shutdown window before killing the process.

## Compatibility rules

- Clients must ignore unknown notification methods and unknown response fields.
- Servers reject requests before initialization, except `initialize`.
- Optional request fields serialize as explicit `null` when represented in a
  response model.
- A protocol version change is required before removing or changing a stable
  field.
- One thread supports at most one active turn in the MVP.
- Clients must treat approval IDs as single-use; late responses return
  `accepted: false`.
