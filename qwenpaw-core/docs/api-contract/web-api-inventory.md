# Existing QwenPaw Web API Inventory

> Snapshot: parent CoPaw repository on 2026-09-01

## Purpose and scope

This inventory records the HTTP, SSE, and WebSocket surfaces that the existing Desktop/WebUI can call. It is a migration map, not a promise that Rust Core already implements these routes.

The Python application mounts its primary routers below `/api`, also serves Console assets and SPA fallback routes, and exposes a small set of root-level external webhooks. Many agent-specific routers are mounted a second time below `/api/agents/{agentId}`.

The machine-generated Rust client contract is separate: see [App Protocol inventory](app-protocol-inventory.md) and [App Protocol v3 schema](app-protocol-v3.schema.json).

## Top-level surfaces

| Surface | Transport | Python source | Rust MVP disposition |
| --- | --- | --- | --- |
| `/`, `/console/**`, `/assets/**` | HTTP static/SPA | `src/qwenpaw/app/_app.py` | Implemented in default Desktop Rust mode with the unchanged Console build, no-cache HTML, and SPA fallback |
| `/api/version`, `/api/healthz` | HTTP | `_app.py`, `routers/healthz.py` | Implemented for the local Desktop bootstrap; Core also exposes `/healthz` and `/readyz` |
| `/api/doctor/runtime` | HTTP | `_app.py` | Python-specific; do not reproduce verbatim |
| `/api/desktop/shutdown` | HTTP | `_app.py` | Implemented with a per-process token passed by Tauri; invalid tokens fail closed as 404 |
| `/api/**` | HTTP/SSE/WebSocket | `app/routers`, `app/chats`, `app/crons` | Domain-by-domain migration required |
| `/api/agents/{agentId}/**` | HTTP/SSE | `routers/agent_scoped.py` | Multi-agent isolation not in Rust MVP |
| `/api/ws/**`, `/api/browser/chrome/**` | WebSocket/HTTP | `browser/control_link/chrome` | Browser control not in Rust MVP |
| `/voice/incoming`, `/voice/status-callback` | HTTP webhook | `routers/voice.py` | Channel runtime remains Python |
| `/voice/ws` | WebSocket | `routers/voice.py` | Channel runtime remains Python |
| `/api/{appId}/**` PawApp routes | HTTP | `pawapp` plus dynamic router mounting | Plugin app runtime remains Python |
| Hub control application `/api/hub/**` and proxy `/api/{path}` | HTTP/WebSocket | `hub/control_app.py` | Hub and multi-tenant runtime remain Python |

## Primary `/api` router inventory

“Phase 1 mapping” describes the currently tested subset. “Partial” means the listed route shapes work in Rust Desktop mode, while the rest of that Python domain remains unavailable and receives an explicit 404.

| Prefix/domain | Python source | Representative responsibility | Phase 1 mapping |
| --- | --- | --- | --- |
| `/agents` | `routers/agents.py` | Agent CRUD, visibility, model and memory views | Partial: read-only singleton `default` agent bootstrap response; CRUD and multi-agent isolation deferred |
| `/agent-status` | `routers/agent_status.py` | Scoped agent readiness | Deferred |
| `/agent-stats` | `routers/agent_stats.py` | Agent execution statistics | Deferred |
| `/auth` | `routers/auth.py` | Register, login, verify, profile | Partial: local no-auth bootstrap status/verify only; accounts and remote/multi-user auth deferred |
| `/chats` | `app/chats/api.py` | Session CRUD, groups, archive, project dirs, history | Partial compatibility: list, history, fixed bootstrap groups, archive/unarchive, and persisted single-Workspace project-dir reads/rebinding map to Core Threads; multi-root and other CRUD/metadata deferred |
| `/console` | `routers/console.py` | Chat streaming, upload, push messages, inbox, traces | Partial compatibility: Chat SSE, local-session aliases, stop, bounded attachment upload/preview, Workspace file references, approval polling, and empty inbox; direct multimodal model input, traces, and reconnection deferred |
| `/approval` | `routers/approval.py` | Approve, deny, status | Partial compatibility: one-time approve/deny with session validation; generalized `similar` approval and policy persistence return unsupported |
| `/tool-calls` | `routers/tool_calls.py` | Tool status, output, offload, cancel | Partial lifecycle events only; offload and HTTP output APIs deferred |
| `/models` | `routers/providers.py` | Provider/model config and discovery | Partial compatibility: active/list, base URL, model selection/add, and API-key set/delete for one OpenAI-compatible provider; secrets use the OS credential store and are always masked; discovery and other providers deferred |
| `/providers` | `routers/provider_oauth.py` | Provider browser OAuth | Deferred |
| `/local-models` | `routers/local_models.py` | Local model lifecycle/download | Deferred |
| `/config` | `routers/config.py` | Channels, heartbeat, agent and product settings | Partial only for non-secret model configuration |
| `/settings` | `routers/settings.py` | Language, upload limit, offload policy | Partial compatibility: global UI language GET/PUT is validated against all seven unchanged Console choices and persisted in Rust SQLite; the upload limit is the actually enforced fixed 32 MiB Rust limit. Offload policy and other settings remain deferred because their runtime semantics do not exist yet |
| `/workspace` | `routers/workspace.py` | File tree/content/watch/upload, memory, prompt/config resources | Partial compatibility: single-root paginated tree, metadata, bounded UTF-8 reads, ETag-guarded saves, streamed downloads, HTML URI resolution, conflict-aware uploads, native recursive SSE file watch, and the actual `AUTO` approval running-config field; memory/profile resources, multi-root, and remaining Coding APIs deferred |
| `/workspace/project-directory` | `routers/project_directory.py` | Select/create/clone/import/browse project roots | Partial compatibility: persisted local default selection, recent list, bounded directory browse, and direct-child creation; project clone/import/upload and multi-root remain deferred |
| `/workspace/git` | `routers/git.py` | Status, branches, checkout, stage/unstage, commit/log/diff, discard, revert | Partial compatibility: all 11 unchanged Console calls operate on the selected single Workspace through bounded, non-shell Git subprocesses; Coding Mode is persisted by Rust and a missing/inherited repository is initialized at the Workspace root without automatically staging user content. Multi-root/chat isolation fixtures, larger-repository limits, hook policy, and native runner coverage remain release gates |
| `/workspace/checkpoints` | `routers/checkpoints.py` | Checkpoint create/list/restore | Deferred |
| `/coding-mode` | `routers/coding_mode.py` | Coding mode status and activation | Implemented for the built-in Rust agent with SQLite persistence; enabling it exposes the existing Files/Git UI without changing React source |
| `/loops` | `routers/loops.py` | Loop catalog and configuration | Partial bootstrap contract: one actual standard guarded loop and idle status; custom modes and writes deferred |
| `/tools` | `routers/tools.py` | Tool catalog, toggles, async mode, config | Partial built-in/MCP tool execution; management contract deferred |
| `/mcp` | `routers/mcp.py`, `routers/mcp_oauth.py` | MCP config, discovery, OAuth start/callback/status | Partial management contract plus native stdio/HTTP/SSE transports and interactive OAuth discovery, PKCE loopback callback, secure persistence, refresh and revoke; client CRUD/access-policy writes remain deferred |
| `/skills` | `routers/skills.py`, `routers/skills_stream.py` | Skill CRUD, install/import/sync/security scan streams | Partial bootstrap contract returns an empty catalog because Desktop Skills are not implemented; lifecycle and streams deferred |
| `/plugins` | `routers/plugins.py` | Plugin lifecycle and configuration | Deferred |
| `/frontend_plugin` | `routers/frontend_plugin.py` | Frontend plugin manifest/assets | Partial bootstrap response returns an empty list; plugin lifecycle/assets deferred |
| `/pawapps` | `routers/pawapps.py` | PawApp list/detail/iframe/install lifecycle | Deferred |
| `/market` | `routers/market.py` | Marketplace queries | Deferred |
| `/harnesses` | `routers/harnesses.py` | External coding harness configuration | Deferred |
| `/cron` | `app/crons/api.py` | Job CRUD, pause/resume/run/history | Deferred |
| `/messages` | `routers/messages.py` | Direct message dispatch | Deferred |
| `/mail-access-control` | `routers/mail_access_control.py` | Mail rules, approvals, audit | Deferred |
| `/access-control` | `routers/access_control.py` | Channel access rules and approvals | Deferred |
| `/envs` | `routers/envs.py` | Environment inspection/configuration | Deferred |
| `/token-usage` | `routers/token_usage.py` | Usage summaries | Deferred |
| `/backups` | `routers/backup.py` | Preview/create/import/export/restore, SSE progress | Deferred; Rust needs backup/restore for new data, while old Python data remains untouched |
| `/fork` | `routers/fork.py` | Fork Agent | Deferred |
| `/files/preview/{path}` | `routers/files.py` | Guarded file preview/download | Partial compatibility for opaque Rust attachment names stored below the isolated Core data directory; arbitrary filesystem preview remains unsupported |

## Repeated agent-scoped surface

`routers/agent_scoped.py` remounts the following domain routers below `/api/agents/{agentId}`:

- agent status;
- chats;
- config;
- cron;
- MCP and MCP OAuth;
- skills;
- tools;
- workspace;
- console;
- plugins;
- checkpoints.

Consumers may also pass `X-Agent-Id`, and approval routing can use `X-Root-Session-Id`. Any WebUI compatibility layer must preserve the intended isolation and header semantics or explicitly version a replacement. A simple path rewrite to App Protocol would be unsafe.

## Streaming and callback contracts requiring dedicated fixtures

Before a Rust compatibility endpoint replaces the corresponding Python path, capture request/response/error fixtures for:

- Console Chat stream ordering, terminal events, uploads, and reconnection;
- approval request routing and concurrent pending approvals;
- tool-call output streaming, cancellation, and offload state;
- Workspace watch streams, uploads/downloads, path errors, and Windows path forms;
- skill install/security-scan progress streams;
- backup creation/restoration progress and downloadable archives;
- MCP and provider OAuth browser callbacks and status polling;
- Browser Chrome WebSocket handshake and protocol compatibility;
- Twilio request signature, single-use WebSocket token, and status callback;
- Hub proxy authentication, WebSocket forwarding, and tenant isolation.

## Compatibility rules

1. Do not point the unchanged Console at Core until every route it uses has a tested compatibility disposition.
2. Similar names are not compatibility. App Protocol `thread/archive`, for example, does not automatically satisfy every `/api/chats` archive/group behavior.
3. Preserve authentication and filesystem boundaries before response-shape compatibility.
4. Introduce adapters at the transport edge; do not pollute Core domain state with Python response objects.
5. Never point Rust Core at the old Python data directory; the new version owns a separate, initially empty store.
6. Remove a Python router only after frontend call-site inventory, golden fixtures, cutover/rollback procedure, and cross-platform tests pass.

## Source-of-truth maintenance

When the Python API changes, review:

- `src/qwenpaw/app/_app.py` for mounts and lifecycle;
- `src/qwenpaw/app/routers/__init__.py` for primary routers;
- `src/qwenpaw/app/routers/agent_scoped.py` for duplicated scoped routes;
- `src/qwenpaw/app/chats/api.py` and `src/qwenpaw/app/crons/api.py`;
- `src/qwenpaw/browser/control_link` and `src/qwenpaw/app/routers/voice.py`;
- `src/qwenpaw/hub/control_app.py` and `src/qwenpaw/pawapp`;
- `console/src/api` for actual frontend consumers.

The final WebUI migration inventory should be generated from the Python OpenAPI document plus explicit WebSocket, SSE, dynamically mounted PawApp, and root-level webhook surfaces. Static decorator scanning alone is not sufficient.
