# QwenPaw Rust Core System Overview

> Status: Rust Core + VS Code MVP, 2026-09-01

## Objective

QwenPaw currently stages the reusable runtime under `qwenpaw-core/` in the
existing CoPaw repository so the product keeps its GitHub history and stars.
The directory is designed as an extraction-ready repository boundary:

- `qwenpaw-core/`: Rust workspace, App Protocol, persistence, model loop, tools, and native release artifacts;
- the CoPaw repository root: existing product, unchanged Console/Desktop frontend, Python compatibility runtime during migration, and the VS Code extension.

The first supported client is VS Code. Desktop and WebUI remain on the existing Python HTTP service until a separately reviewed compatibility layer is ready. Normal runtime should eventually stop requiring Python, but the migration does not remove working Python features before their replacements have contract and data-migration coverage.

## Runtime topology

```text
VS Code Chat
    │ JSONL over child-process stdio
    ▼
qwenpaw-core app-server ───────────────┐
    │ App Protocol v2                  │ optional loopback HTTP health
    ▼                                  │ and WebSocket transport
Core runtime                           │
    ├── bounded agent loop             │
    ├── approvals and cancellation     │
    ├── Workspace tools                │
    ├── MCP clients                    │
    ├── SQLite thread/config storage   │
    └── OpenAI-compatible model API    │
                                       │
Existing Desktop/WebUI ── HTTP/SSE ── Python QwenPaw service
    └── unchanged Console assets and all not-yet-migrated product domains
```

The two server paths are deliberately separate in the MVP. VS Code never calls the Python Web API, and the existing Console never calls App Protocol yet.

## Component ownership

| Component | Repository | Responsibility | Current status |
| --- | --- | --- | --- |
| `qwenpaw-cli` | `qwenpaw-core/` | Native `qwenpaw-core` executable, configuration bootstrap, logging, process entrypoints | Implemented |
| `qwenpaw-app-server` | `qwenpaw-core/` | JSONL stdio and loopback HTTP/WebSocket transports, initialization lifecycle, health probes | Implemented |
| `qwenpaw-protocol` | `qwenpaw-core/` | Rust protocol types, version, JSON Schema, fixtures, inventory, TypeScript SDK | Implemented, App Protocol v2 |
| `qwenpaw-core` | `qwenpaw-core/` | Thread/Turn state machine, bounded context, model streaming, tool loop, approval and cancellation orchestration | Implemented for MVP |
| `qwenpaw-storage` | `qwenpaw-core/` | SQLite snapshots and non-secret effective configuration | Implemented for MVP |
| `qwenpaw-tools` | `qwenpaw-core/` | Workspace path boundary and built-in file/Shell tools | Implemented for MVP |
| `qwenpaw-mcp` | `qwenpaw-core/` | stdio, Streamable HTTP, legacy SSE, existing-token OAuth refresh, bounded MCP execution | Implemented except interactive OAuth |
| `extensions/vscode` | Product | Native Chat Participant, Core process ownership, settings/secrets, protocol rendering, thread/model/workspace commands | Implemented for MVP |
| `console` | Product | Existing React WebUI and Tauri frontend | Source unchanged |
| `src/qwenpaw` | Product | Existing Python REST/SSE service and non-migrated domains | Compatibility runtime |
| `references/codex` | Core | Read-only architecture and implementation reference | Not linked into production artifacts |

## Why the current crate split is intentionally small

The design plan originally named conceptual `domain`, `models`, `governance`, and `platform` crates. The MVP does not create empty crates for those names:

- protocol-facing domain records live in `qwenpaw-protocol` because both transports and clients consume them;
- model transport and the agent loop remain in `qwenpaw-core` while there is only one model adapter;
- approval policy remains in the Core state machine while only one-time guarded tool approval exists;
- cross-platform path and process behavior stays next to the tool or CLI code that owns it.

A component moves into a new crate only when it has an independently testable public boundary and at least two real consumers. This avoids circular dependencies and speculative abstraction while preserving room to split later.

## Client request lifecycle

1. The VS Code extension resolves an explicit, verified bundled, or `PATH` Core executable.
2. It starts `qwenpaw-core app-server --stdio`, keeping stdout exclusively for JSONL and inheriting stderr for logs.
3. Client and server negotiate exact App Protocol version 2 through `initialize`.
4. The extension synchronizes non-secret model configuration and injects an API key from SecretStorage only into the child environment.
5. A new Thread is bound to one canonical Workspace root, or an existing persisted Thread is resumed.
6. A Chat request becomes text plus optional structured file references. Core validates and normalizes references without reading their contents.
7. Core persists the user input, calls the configured model, streams Agent deltas, and executes bounded tool-loop steps.
8. Guarded tools pause for a one-time client approval. Cancellation interrupts model, approval, Shell, and MCP waits.
9. Core persists the terminal Turn and emits `turn/completed`; the extension renders the result through native Chat APIs.

The exact method and notification set is generated in [App Protocol inventory](../api-contract/app-protocol-inventory.md). Wire semantics and security bounds are documented in [App Protocol](app-protocol.md).

## State and secret ownership

| Data | Owner | Persistence | Exposure rule |
| --- | --- | --- | --- |
| Threads, Turns, messages, tool lifecycle | Core | SQLite under `QWENPAW_HOME` | Available only through typed App Protocol methods/events |
| Effective base URL and default model | Core | SQLite | Validated, non-secret, readable through `config/read` |
| Model API key | Client/process environment | VS Code SecretStorage or inherited environment | Never accepted by `config/write`, persisted, returned, or logged |
| Workspace files | User filesystem | Existing files | Canonical path must remain within immutable Thread Workspace root |
| MCP configuration | User-selected JSON | Existing file | Path passed at Core startup; sensitive headers are not logged |
| MCP OAuth access/refresh token | Current MCP config/process | Existing configuration only | Core can refresh an existing grant but does not persist a new browser grant |
| VS Code selected Thread/Workspace | VS Code extension | Chat metadata and in-memory one-shot selection | Workspace selection is restricted to open folders |
| Existing Console and Python state | Python product runtime | Existing QwenPaw storage | Not read or migrated by Rust MVP |

## Trust boundaries

- Stdio is local and single-client; Core rejects requests before `initialize` and enforces exact protocol compatibility.
- HTTP/WebSocket listens only on loopback. Browser origins must be loopback same-origin or explicitly allowlisted for development.
- Model redirects are disabled. Response-header, idle-stream, event, error-body, context, and output sizes are bounded.
- Every Workspace path is canonicalized. File references must be existing regular files; discovery avoids symlink traversal.
- Read-only tools do not require approval. File mutation, Shell, and all MCP calls require a fresh one-time approval.
- Tool names, schemas, arguments, results, loop steps, subprocess duration, and MCP transport payloads are bounded.
- The extension does not display tool arguments or results in progress messages and does not store API keys in normal settings.
- A Core crash invalidates only that process generation; replacement is on demand, not an uncontrolled restart loop.

These MVP controls do not constitute a complete security parity claim with the Python product. Browser governance, sandbox policy, plugin trust, remote authentication, and multi-tenant isolation remain outside this phase.

## Build and release boundary

Core-specific `qwenpaw-core-v*` tags produce native archives for macOS arm64/x64, Linux x64, and Windows x64. The product locks one Core version, protocol version, tag, and asset name in `extensions/vscode/core-release.json`. Target-specific VSIX builds stage one matching binary and verify version and SHA-256 before packaging. macOS release artifacts fail closed unless Developer ID signing, notarization, and Gatekeeper verification succeed.

Local development uses a thin VSIX and a Core binary from an explicit setting or `PATH`. This keeps a developer's native binary out of portable extension packages.

## Migration boundary

The Rust MVP is not a drop-in replacement for the Python `/api` server. App Protocol is the new stable client contract; legacy Web API compatibility is tracked separately in [Web API inventory](../api-contract/web-api-inventory.md). Capability status and the conditions for retiring each Python area are in [Python to Rust migration matrix](../migration/python-to-rust-matrix.md).

Desktop/WebUI migration may begin only after its API subset, streaming behavior, authentication, filesystem behavior, and stored-data compatibility have contract tests. Until then, the existing Console and Python service remain the production path for those clients.
