# QwenPaw for VS Code

The extension connects VS Code Chat to the local `qwenpaw-core` process. It
does not call a model API directly and does not duplicate agent runtime logic.

## Development

Build QwenPaw Core from the CoPaw repository:

```shell
cd ../../qwenpaw-core
cargo build -p qwenpaw-cli
```

Install and check the extension:

```shell
npm install
npm run check
npm run package
```

When App Protocol types change in the Core workspace, regenerate the
Core artifacts first and then sync the extension snapshot:

```shell
cd ../../qwenpaw-core
cargo run -p qwenpaw-protocol --example generate_contract
cd ../qwenpaw/extensions/vscode
npm run sync:protocol
npm run check
```

The extension imports this generated SDK directly. Its test suite checks the
protocol version and SHA-256 recorded in `protocol-lock.json`.

Target-specific release VSIX packages contain a Core binary for their platform.
The extension verifies its version metadata and SHA-256 digest before starting
it. If no bundled binary is present, Core is resolved from `PATH`. Set
`qwenpaw.core.path` only when an explicit executable should override both
locations.

Configure the model API key through the **QwenPaw: Set API Key** command. The value is stored in VS Code
SecretStorage and injected only into the Core child process. Environment
variables `QWENPAW_API_KEY` and `OPENAI_API_KEY` remain supported when no
stored key is present. Use **QwenPaw: Clear Stored API Key** to return to the
environment-variable behavior; secrets are never written to normal settings.

Build a platform VSIX by setting `QWENPAW_CORE_BIN` to the matching release
binary and `QWENPAW_VSCODE_TARGET` to one of `darwin-arm64`, `darwin-x64`,
`linux-x64`, or `win32-x64`, then run:

```shell
npm run package:bundled
```

The staging step copies only that target's binary, records its version,
protocol version, and checksum in `resources/core/manifest.json`, and asks VSCE
to mark the resulting package for the same target.

The `VS Code Platform Packages` workflow downloads the exact Core tag and asset
declared in `core-release.json`, then creates separate `darwin-arm64`,
`darwin-x64`, `linux-x64`, and `win32-x64` VSIX artifacts on native runners.
Update that lock file only after the matching Core release archives exist.

macOS bundles must use a Developer ID-signed and notarized Core binary. The
staging script verifies both `codesign` and Gatekeeper assessment before and
after copying, and refuses ad-hoc linker signatures. Local macOS development
should use the thin VSIX and point `qwenpaw.core.path` at the Cargo build until
release-signing credentials are configured.

The Core release workflow requires `APPLE_CERTIFICATE_P12` (base64-encoded),
`APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`,
`APPLE_TEAM_ID`, and `APPLE_APP_PASSWORD` repository secrets. A missing secret,
failed notarization, or failed Gatekeeper assessment stops the release; the
workflow never publishes an unsigned macOS archive.

## MCP

Set `qwenpaw.mcp.configPath` to either a legacy QwenPaw `agent.json` containing
`mcp.clients`, or a JSON file whose root contains `clients`. Core supports
`stdio`, `streamable_http`/`http`, and legacy `sse` clients. Remote entries use
`url`, optional sensitive `headers`, and existing OAuth access/refresh tokens;
stdio entries use `command`, `args`, `env`, and `cwd`. All transports support an
optional `tools` whitelist. MCP tools are namespaced as
`mcp__<client>__<tool>` and always require an **Allow once** approval.
Set `"oauth": {}` on a remote entry to enable **QwenPaw: Authorize MCP Server**
and **QwenPaw: Revoke MCP Authorization**. Core opens authorization in the
system browser and keeps tokens out of the extension protocol.

Open VS Code Chat and invoke `@qwenpaw` to start or continue a thread. New
threads are bound to the first open workspace folder. Shell calls are shown in
a native modal and do not execute unless the user selects **Allow once**. If a
Chat still references a Thread that was removed from Core storage, the
extension creates a replacement Thread and retries that prompt once; unrelated
Core errors are returned without retry.

Tool start, approval, completion, and failure states appear through native Chat
progress messages. These messages use a whitespace-normalized, bounded tool
name and never include tool arguments or output. Completed, failed, interrupted,
and invalid non-terminal completion states are handled separately; cancelling a
Chat request interrupts the matching Core Turn. A failed Core handshake or
initial configuration sync also closes the RPC connection and terminates the
child process before startup is retried. If Core exits during an active Turn,
the Chat request fails immediately instead of waiting indefinitely; restart and
extension disposal also tolerate an already failed startup promise.

An unexpected Core exit invalidates only that process generation. The extension
does not start a background crash loop; the next command or Chat request starts
one replacement Core on demand. Concurrent requests share the same pending
startup, and a manual restart waits for the previous generation to be disposed
before publishing its replacement.

Use **QwenPaw: Select Thread** to continue a persisted Core thread, or to force
the next `@qwenpaw` request to start a new thread. The selection is consumed by
the next request; subsequent turns use the thread metadata stored in VS Code
Chat history. Use **QwenPaw: Select Model** to choose the Core model for new
threads. The command also accepts a model ID exposed by the configured
OpenAI-compatible endpoint and applies it live through Core configuration after
changing the workspace setting. Existing threads retain the model recorded
when they were created.

Thread selection and archival follow Core cursors until all pages are loaded,
with explicit 10,000-item and 100-page safety limits and repeated-cursor
rejection. In a multi-root VS Code window, a new Thread defaults to the
Workspace folder containing the active editor, then falls back to the first
folder. Use **QwenPaw: Select Workspace** to force the next request to create a
new Thread in another open folder. That choice is consumed once and overrides
Chat history; if the folder has since been removed, the extension safely falls
back to the current default folder.

The extension synchronizes `qwenpaw.model` and `qwenpaw.baseUrl` through Core's
validated `config/write` method after startup and whenever either setting
changes. Use **QwenPaw: Show Core Configuration** to inspect the effective
non-secret values and whether a key is configured; the key itself is never
returned. Changes to the Core path, Core arguments, or MCP config path still
restart the child process.

Use **QwenPaw: Archive Thread** to hide an idle Thread without deleting its
history. Archived Threads remain visible in **QwenPaw: Select Thread** with an
archived label; selecting one resumes it before the next request. If an open
Chat still points to a Thread archived elsewhere, its next request starts a new
Thread rather than remaining stuck on an unavailable conversation.

Use **QwenPaw: Show Workspaces** to inspect normalized Workspace roots already
registered by Core Threads. The command shows total and archived Thread counts
and does not scan arbitrary directories.
