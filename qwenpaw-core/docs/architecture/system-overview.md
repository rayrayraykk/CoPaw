# QwenPaw Rust Core System Overview

> Status: Rust Core + VS Code MVP and Desktop sidecar foundation, 2026-09-01

## Objective

QwenPaw currently stages the reusable runtime under `qwenpaw-core/` in the
existing CoPaw repository so the product keeps its GitHub history and stars.
The directory is designed as an extraction-ready repository boundary:

- `qwenpaw-core/`: Rust workspace, App Protocol, persistence, model loop, tools, and native release artifacts;
- the CoPaw repository root: existing product, unchanged Console/Desktop frontend, retained legacy Python source, and the VS Code extension.

VS Code and Desktop now start the same Rust Core. Desktop serves the unchanged Console build, preserves the existing ready/version/shutdown lifecycle, and covers the observed bootstrap, Chat, file, model, OAuth, and navigation-read contracts. New Desktop packages contain only Rust Core and do not recognize a Python-backend switch. Rust Core always uses a new, isolated database and never imports the Python product's data.

## Runtime topology

```text
VS Code Chat
    │ JSONL over child-process stdio
    ▼
qwenpaw-core app-server ───────────────┐
    │ App Protocol v3                  │ optional loopback HTTP health
    ▼                                  │ and WebSocket transport
Core runtime                           │
    ├── bounded agent loop             │
    ├── approvals and cancellation     │
    ├── Workspace tools                │
    ├── MCP clients                    │
    ├── SQLite thread/config storage   │
    └── OpenAI-compatible model API    │
                                       │
Desktop ───────── random loopback HTTP ┘
    ├── unchanged Console assets and SPA fallback
    ├── version, health, authenticated shutdown
    ├── bootstrap/model/Workspace APIs, files/watch, attachments, Chat SSE, stop, and one-time approval
    └── explicit 404 for unsupported `/api` routes

Legacy Python release ── HTTP/SSE ── Python QwenPaw service
    └── retained separately for users who deliberately run the old product

Remote native client ── TLS + bearer-authenticated WSS ── Core App Protocol
```

The two server paths are deliberately separate in the MVP. VS Code never calls the Python Web API. The existing Console uses a narrow HTTP/SSE adapter in Rust Desktop mode; that adapter translates at the transport edge into the same Core Thread/Turn/approval model used by App Protocol.

## Component ownership

| Component | Repository | Responsibility | Current status |
| --- | --- | --- | --- |
| `qwenpaw-cli` | `qwenpaw-core/` | Native `qwenpaw-core` executable, configuration bootstrap, logging, process entrypoints | Implemented |
| `qwenpaw-app-server` | `qwenpaw-core/` | JSONL stdio, loopback HTTP/WebSocket, authenticated remote WSS, Desktop static serving/lifecycle, initialization, health probes, and Console compatibility adapter | Implemented for App Protocol plus the first Desktop bootstrap/chat/approval slice |
| `qwenpaw-protocol` | `qwenpaw-core/` | Rust protocol types, version, JSON Schema, fixtures, inventory, TypeScript SDK | Implemented, App Protocol v3 |
| `qwenpaw-core` | `qwenpaw-core/` | Thread/Turn state machine, bounded context, model streaming, tool loop, approval and cancellation orchestration | Implemented for MVP |
| `qwenpaw-storage` | `qwenpaw-core/` | SQLite snapshots and non-secret effective configuration | Implemented for MVP |
| `qwenpaw-tools` | `qwenpaw-core/` | Workspace path boundary and built-in file/Shell tools | Implemented for MVP |
| `qwenpaw-mcp` | `qwenpaw-core/` | stdio, Streamable HTTP, legacy SSE, interactive OAuth/refresh, secure token storage, bounded MCP execution | Implemented for the current MCP client scope |
| `extensions/vscode` | Product | Native Chat Participant, Core process ownership, settings/secrets, protocol rendering, thread/model/workspace/MCP OAuth commands | Implemented for MVP |
| `console` | Product | Existing React WebUI and Tauri frontend | React business source unchanged; Tauri packages and starts only Rust Core |
| `src/qwenpaw` | Product | Existing Python REST/SSE service and non-migrated domains | Retained legacy source; not packaged or started by the new Desktop |
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
3. Client and server negotiate exact App Protocol version 3 through `initialize`.
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
| Model API key | Client/Desktop credential store | VS Code SecretStorage, inherited environment, or macOS Keychain/Windows Credential Manager/Linux Secret Service | Never persisted in SQLite, returned, or logged; Desktop reads/writes it only through the system credential store |
| Workspace files | User filesystem | Existing files | Canonical path must remain within immutable Thread Workspace root |
| MCP configuration | User-selected JSON | Existing file | Path passed at Core startup; sensitive headers are not logged |
| MCP OAuth access/refresh token | Core MCP manager | macOS Keychain, Windows Credential Manager, or Linux Secret Service | Browser grants and refreshes remain in the system credential store; protocol and Console responses expose status only |
| Remote App Protocol bearer token | Deployment operator | Permission-restricted external file | Re-read for every WSS handshake; never accepted as a CLI value or written to logs/SQLite |
| VS Code selected Thread/Workspace | VS Code extension | Chat metadata and in-memory one-shot selection | Workspace selection is restricted to open folders |
| Existing Console and Python state | Python product runtime | Existing QwenPaw storage | Kept separate and untouched; the Rust version starts with a new Core database as described in the [fresh-start notice](../release/fresh-start.md) |

## Trust boundaries

- Stdio is local and single-client; Core rejects requests before `initialize` and enforces exact protocol compatibility.
- Plain HTTP/WebSocket listens only on loopback. Remote mode exposes WSS only,
  requires a TLS certificate/private key and a permission-restricted bearer
  token file, and re-reads that file for each handshake. Browser origins still
  require an explicit allowlist; native clients may omit `Origin`.
- Model redirects are disabled. Response-header, idle-stream, event, error-body, context, and output sizes are bounded.
- Every Workspace path is canonicalized. File references must be existing regular files; discovery avoids symlink traversal.
- Read-only tools do not require approval. File mutation, Shell, and all MCP calls require a fresh one-time approval.
- Tool names, schemas, arguments, results, loop steps, subprocess duration, and MCP transport payloads are bounded.
- The extension does not display tool arguments or results in progress messages and does not store API keys in normal settings.
- A Core crash invalidates only that process generation; replacement is on demand, not an uncontrolled restart loop.

These MVP controls do not constitute a complete security parity claim with the Python product. Browser governance, sandbox policy, plugin trust, remote multi-user authorization, and tenant isolation remain outside this phase.

## Build and release boundary

Core-specific `qwenpaw-core-v*` tags produce native archives for macOS arm64/x64, Linux x64, and Windows x64. The product locks one Core version, protocol version, tag, and asset name in `extensions/vscode/core-release.json`. Target-specific VSIX builds stage one matching binary and verify version and SHA-256 before packaging. macOS release artifacts fail closed unless Developer ID signing, notarization, and Gatekeeper verification succeed.

Desktop build scripts stage the release Core binary and `console/dist` under the Tauri resource directory. They do not build or package the PyInstaller backend, Python runtime, or the Node runtime that existed only for that backend. QA builds apply a final ad-hoc macOS signing pass over the app, embedded Core, and native helper Mach-O files. Production builds instead let Tauri perform Developer ID signing, notarization, and stapling, then run read-only `codesign`, `stapler`, and Gatekeeper checks; no post-notarization re-sign is allowed. Windows install/process cleanup recognizes `qwenpaw-core.exe` and the native helper. Native bundle and notarization workflows remain release gates rather than claims made from a local build.

Local development uses a thin VSIX and a Core binary from an explicit setting or `PATH`. This keeps a developer's native binary out of portable extension packages.

## Migration boundary

The Rust MVP is not yet a drop-in replacement for the Python `/api` server. App Protocol is the new stable client contract; the compatibility adapter now covers local bootstrap reads, Thread-backed chat list/history/archive, Chat SSE and cancellation, one-time approval polling/actions, and the first single-Workspace file/attachment surface. Exact route status is tracked in [Web API inventory](../api-contract/web-api-inventory.md). In Desktop mode, unknown `/api` paths deliberately return 404 instead of falling through to the SPA or silently pretending compatibility. Capability status and the conditions for retiring each Python area are in [Python to Rust migration matrix](../migration/python-to-rust-matrix.md).

The Desktop cutover keeps the React business source unchanged. The single OpenAI-compatible provider can update its base URL/model and store its API key in the OS credential store. Desktop owns a versioned default Workspace, persists local selection, Coding Mode, and the validated global UI language, accepts the Console's first-turn `session_project_dirs`, and can rebind an idle Thread between turns. Its local file surface rejects traversal and escaping symlinks, bounds text and multipart payloads, streams downloads, applies ETag preconditions to saves, emits recursive native file-change SSE, and copies opaque uploaded attachments into the current Workspace before passing a Core file reference. The selected Workspace also exposes the Console's complete current Git surface through bounded parameterized subprocesses: status, branches, checkout/create, diff, stage/unstage, commit/log, discard, commit diff, and revert. A missing or inherited repository is initialized at the exact Workspace root, but user content stays untracked until the user explicitly stages it. A headless-Chrome matrix visits all 24 built-in navigation pages and rejects API 4xx/5xx, network failures, JavaScript exceptions, and console errors; a Coding Mode variant opens Source Control and observes the Rust Git reads, while an isolated-profile check proves a persisted Rust language choice drives the existing Console localization on startup. Unsupported product domains expose truthful new-install empty or disabled read states; their mutation routes remain unavailable until implemented. Direct multimodal model input, multi-root, memory/profile resources, checkpoints, channels, schedules, backups, and settings without corresponding Rust runtime semantics are not yet feature-complete. Production cutover starts with an empty, versioned Rust Core data directory; old Python data remains untouched and is reachable only by deliberately running an old Python-based release.
