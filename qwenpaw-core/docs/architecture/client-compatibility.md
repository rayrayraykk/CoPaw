# Existing Client Compatibility Boundary

> Status: required non-regression boundary for the App Server SDK refactor

The SDK refactor changes how clients reach Core. It does not remove existing
product entry points. A client may switch to Rust App Protocol only after its
current capability set has an equivalent implementation and regression tests.

## Existing entry points

| Entry point | Current implementation | Preservation rule | Rust target |
| --- | --- | --- | --- |
| `qwenpaw` / `copaw` | Python Click CLI | Keep both console scripts and existing commands runnable | Commands progressively use Python or Rust SDK clients without changing their public CLI contract |
| Bare `qwenpaw` and `qwenpaw tui` | Python terminal UI | Keep interactive chat, session selection, streaming, tools, and permission UI available | TUI talks to App Server through a client SDK after parity tests pass |
| `qwenpaw app` / `desktop` | Python service and Tauri Desktop | Old Python release remains runnable; new Desktop remains Rust-only | Both are explicit products; neither silently falls back to the other runtime |
| `qwenpaw hub` and remote runtime bridge | Python Hub, HTTP/WebSocket proxy, reverse tunnel | Keep existing Hub deployment and remote control behavior available | Authenticated App Protocol WSS is additive until Hub parity is reviewed |
| `qwenpaw acp` and harness integrations | Python ACP and external-agent adapters | Preserve ACP process and protocol behavior | Add an App Server adapter only after ACP contract tests exist |
| `qwenpaw task` | Python headless task command | Keep headless execution and output contract | May adopt Python SDK when task semantics are represented in App Protocol |
| Administrative CLI | `auth`, `agents`, `auto`, `channels`, `chats`, `clean`, `cron`, `daemon`, `doctor`, `env`, `init`, `models`, `plugin`, `skills`, `shutdown`, `uninstall`, `update` | Do not remove, rename, or redirect a command to an incomplete Rust API | Migrate command groups independently behind contract tests |
| Message channels | Console, DingTalk, Discord, Feishu, iMessage, Matrix, Mattermost, MQTT, OneBot, QQ, SIP, Slack, Telegram, voice, WeChat, WeCom, Xiaoyi, Yuanbao | Existing Python release continues to own channel lifecycle until a Rust-backed equivalent is complete | Channel adapters become App Server clients; Core remains channel-agnostic |

## Non-regression gates

- The Python package keeps both `qwenpaw` and `copaw` console entry points.
- Bare CLI invocation keeps launching the TUI in the Python release.
- Existing command names remain present in `qwenpaw --help`.
- The new Rust Desktop continues to start only Rust Core and never mutates the
  old Python data directory.
- Existing Python releases continue to run independently against their own
  storage and channel configuration.
- Remote WSS remains additive and cannot replace Hub/reverse-tunnel behavior
  without a separately approved parity matrix.
- SDK packages must use their own namespaces and must not shadow the existing
  `qwenpaw` Python package.
- Client migration is one entry point at a time; removing the previous path
  requires automated parity evidence and explicit review.
