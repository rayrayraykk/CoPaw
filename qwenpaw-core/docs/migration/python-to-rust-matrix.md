# Python to Rust Capability Migration Matrix

> Baseline: 2026-09-01. Rust Core + VS Code is the first complete target; Desktop has an opt-in Rust sidecar foundation while its production API authority remains Python.

## Status definitions

- **Rust MVP**: implemented and exercised through the App Protocol/VS Code path.
- **Partial**: a Rust capability exists, but not with full Python behavior or Web API compatibility.
- **Deferred**: intentionally remains in Python for this phase.
- **Exit gate**: evidence required before the Python implementation can stop being the authority.

## Matrix

| Capability | Existing Python authority | Rust status | Exit gate before Python removal |
| --- | --- | --- | --- |
| Native process and CLI | Python CLI/runtime bootstrap | Rust MVP: native CLI and app server | Native release artifacts pass macOS, Linux, Windows installation and lifecycle tests |
| Client protocol | REST/SSE/WebSocket surfaces | Rust MVP: App Protocol v2 over stdio and loopback WebSocket | Each migrating client uses versioned generated types and has reconnect/error fixtures |
| Thread and Turn lifecycle | Chat/session services | Rust MVP: start/read/list/archive/resume, interrupt, persisted terminal state | Migration maps IDs/history/metadata and supports rollback without dual writers |
| Conversation persistence | Existing QwenPaw stores | Rust SQLite snapshots for new Core Threads; legacy data is intentionally not imported | New-version backup, restore, retention, corruption recovery, and schema upgrades pass |
| Model provider | Provider manager and many adapters | Partial: one OpenAI-compatible Chat Completions/SSE adapter | Required providers, auth methods, model metadata, retry/rate-limit behavior have parity tests |
| Context management | Python Agent context and memory systems | Partial: bounded recent complete turns and newest tool chain | Memory, summarization, token accounting, and long-session golden tests meet product requirements |
| Agent loop | Python modes/loop/hooks | Rust MVP for bounded model/tool iterations | Required hooks, modes, policies, observability, and failure semantics are explicitly mapped |
| Built-in file tools | Python tool system/governance | Rust MVP: list/search/read/write/replace inside one Workspace | Tool schema/output fixtures and Windows/Linux/macOS filesystem tests pass |
| Shell execution | Python tools/sandbox | Partial: bounded subprocess, Workspace cwd, one-time approval | Sandbox/resource policy, platform command semantics, environment filtering, and audit parity pass |
| Tool approvals | Python governance/access control | Partial: one-time approval through App Protocol and Console HTTP polling/actions | Persistent/generalized policies, multi-client routing, remote auth, audit, timeout, and recovery are covered |
| MCP stdio | Python MCP drivers | Rust MVP with discovery, whitelist, approval, cancellation | Configuration compatibility and representative server fixtures pass on all targets |
| MCP HTTP/SSE | Python MCP drivers | Rust MVP for Streamable HTTP and legacy SSE | Proxy/TLS/auth interoperability and failure/reconnect suites pass |
| MCP OAuth | Python browser OAuth routes/store | Partial: existing access token and refresh grant only | Discovery, PKCE/browser callback, secure credential persistence/revocation, and UI flow pass |
| Workspace identity | Python Workspace/project-directory services | Partial: canonical Thread root, persisted Desktop default selection, browse/direct-child creation, and explicit idle-Thread rebinding | Multiple simultaneous roots, watch/upload/download, permissions and path parity pass |
| Chat file references | Console rich references | VS Code file URI/location metadata plus bounded Desktop attachment upload/preview copied into the active Workspace as Core file references | Direct multimodal input and durable rich attachment history pass model/provider and Console fixtures; no eager content leakage |
| Non-secret configuration | Python config APIs/files | Partial: base URL/default model in Core SQLite | Full config ownership, schema migration, precedence, import/export, and rollback rules are approved |
| Secret management | Python credentials/provider config | Partial: VS Code SecretStorage plus Desktop OS credential store with masked reads and process-only Core injection | Native credential-store tests, rotation/revocation, and remote/multi-user boundaries pass review |
| VS Code client | No original equivalent | Rust MVP: Chat Participant and native commands | Native target VSIX CI succeeds on macOS/Linux/Windows; signed macOS artifact validated |
| Core local WebSocket | Python HTTP service | Rust MVP for App Protocol loopback transport | A real client consumes it; authentication design exists before any remote exposure |
| Console REST/SSE compatibility | Python FastAPI | Partial: observed Chat-page startup reads, Thread-backed chats, Chat SSE/stop, bounded attachments, single-Workspace files/watch, and one-time approvals; unchanged Chat UI passes a real browser smoke test | Remaining navigation call sites, settings/multi-root behavior, and golden fixtures are complete |
| Desktop sidecar lifecycle | Python sidecar plus Tauri commands | Partial: explicit Rust switch, ready marker, authenticated graceful shutdown, Python fallback | Native packaged Tauri starts/stops/updates a signed Rust sidecar on every supported runner and compatibility routes pass |
| WebUI static serving | Python FastAPI | Partial: Rust Desktop mode serves the unchanged build with no-cache headers and SPA fallback | Required legacy API/auth/streaming routes pass differential tests and production cutover is approved |
| Agents and multi-agent isolation | Python agent manager/scoped routers | Deferred | Agent CRUD, per-agent storage/config, scoped routes, approval routing, and isolation tests pass |
| Channels and mail | Python channel/mail services | Deferred | Each external webhook/socket, credential, allowlist, retry, and delivery contract has parity fixtures |
| Browser/computer use | Python browser runtime/governance plus Tauri bridge | Deferred | Protocol, sandbox, platform bridge, approvals, observation, and security tests pass |
| Cron/background work | Python cron services | Deferred | Durable scheduling, restart recovery, timezone, concurrency, audit, and UI contracts pass |
| Memory/index/graph | Python memory and Workspace services | Deferred | New-store schema, retrieval quality, rebuild, durability, and UI graph contracts pass |
| Skills/plugins/PawApps/market | Python plugin runtime and dynamic routes | Deferred | Package trust, sandboxing, lifecycle, dynamic API/frontend loading, and rollback are specified/tested |
| Harnesses/ACP/external coding agents | Python harness and ACP services | Deferred | Process lifecycle, protocol versioning, MCP/session interop, cancellation, and security pass |
| Git/checkpoints/coding mode | Python Workspace routers | Deferred | Cross-platform repository fixtures, destructive-action policy, checkpoint recovery, and UI contracts pass |
| Backups and restore | Python backup service | Deferred | Rust backup includes every authoritative store, validates archives, supports rollback, and passes recovery drills |
| Authentication/Hub/multi-tenancy | Python auth and Hub control app | Deferred | Threat model, tenant isolation, session/token lifecycle, proxy rules, audit, and remote TLS tests pass |
| Metrics/token usage/observability | Python stats and observability | Deferred | Stable event model, redaction, usage accuracy, retention, and diagnostics are accepted |

## Data-authority rules

1. New VS Code Threads are owned only by Rust Core SQLite.
2. Default Desktop/WebUI sessions remain owned only by Python; an opt-in Rust Desktop session is owned only by the new Rust store.
3. No import, background synchronization, or shared writer is introduced between the two stores.
4. The Rust product starts from a new data directory and does not scan Python `chats.json`, `sessions/`, memory, or configuration.
5. Rollback means launching the old product against its unchanged Python data, not writing Rust state back into it.
6. Secrets are configured again through the new client's secure storage and are never copied from Python files into SQLite or logs.

## Recommended migration order after VS Code MVP

1. Freeze and capture the Console's actual API call graph and streaming fixtures.
2. Define a Rust Web compatibility edge that translates HTTP/SSE to Core domain operations without changing Console source.
3. Migrate read-only health, model, configuration, session-list, and Workspace metadata routes first.
4. Migrate Chat streaming and approval lifecycle with dual-run comparison but a single writer.
5. Add Desktop sidecar start/stop/update integration and signed native packaging.
6. Implement file operations, Git/checkpoints, uploads, and backup/restore against the new Rust-owned storage before cutover.
7. Move higher-risk channel, Browser, plugin, scheduler, memory, and multi-tenant domains independently.
8. Delete the Python proxy/runtime only after production rollback drills and a release acceptance checklist pass.

## Crate extraction triggers

The current crates match executable ownership rather than every conceptual architecture box. Future extraction should be evidence-driven:

| Candidate crate | Extract when |
| --- | --- |
| `qwenpaw-domain` | Domain records must be shared independently of App Protocol serialization by multiple runtimes |
| `qwenpaw-models` | A second materially different model adapter needs the same streaming/tool abstraction |
| `qwenpaw-governance` | Approval, policy, audit, and sandbox rules have multiple transports/clients and their own persistence |
| `qwenpaw-platform` | Shared OS-specific process, keychain, update, or sandbox code has more than one owning component |
| `qwenpaw-web-compat` | Desktop/WebUI contract fixtures exist and the adapter can be tested without frontend changes |

Creating these crates before the trigger would add dependency edges without isolating behavior. Their absence in the MVP is intentional, not an omitted implementation.

## Definition of “Python-free normal runtime”

The migration objective is met only when:

- VS Code, Desktop, and WebUI start and operate without a Python interpreter or Python sidecar;
- every shipped client path uses Rust-owned, versioned contracts;
- the new version clearly starts fresh and never mutates the old Python data directory;
- secrets, approvals, filesystem access, external callbacks, and remote connections pass security review;
- native install/update/uninstall works on supported macOS, Linux, and Windows targets;
- backup/restore and rollback have been exercised against release artifacts;
- the Python process is absent from production process trees and packaging, not merely unused by the happy path.

Until all conditions hold, documentation and release notes must describe the runtime as hybrid rather than fully migrated.
