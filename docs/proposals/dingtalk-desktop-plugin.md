# DingTalk Desktop Channel Plugin

## Goal

Add an installable QwenPaw channel plugin that lets a user bind the currently
selected Codex-backed agent to the locally signed-in DingTalk desktop app.
Replies are sent as the signed-in user through macOS Accessibility instead of
through a DingTalk robot or webhook.

The implementation targets QwenPaw `2.2.0b3` from upstream `main` at
`fe867c214d029f1830261c0ebf9420b88ad011ba`.

## Assumptions

- The first supported host is macOS.
- DingTalk is installed as `/Applications/iDingTalk.app` with bundle id
  `dd.work.exclusive4aliding`.
- The user grants Accessibility access to the QwenPaw desktop host.
- Codex authentication is owned by QwenPaw's existing Codex harness. The
  plugin calls the existing harness status/login endpoints and never reads or
  stores OpenAI credentials.
- DingTalk authentication is the existing local desktop session. DingTalk
  does not expose an OAuth grant for controlling a personal desktop account,
  and macOS Accessibility permission cannot be replaced by OAuth.

## Architecture

```text
DingTalk encrypted database mtime
             |
             v
low-cost change detector ----> macOS Accessibility bridge
                                      |
                                      v
                            dingtalk_desktop channel
                                      |
                                      v
                         selected QwenPaw Codex agent
                                      |
                                      v
                           draft store or AX send
```

The encrypted DingTalk database is used only as a change signal. The plugin
does not decrypt it, query private DingTalk network APIs, or reuse local
session secrets. Message text, direction, and conversation identity are read
from semantic Accessibility attributes after a change.

DingTalk 8.0.10 does not expose the left conversation list as semantic AX
rows. The first release therefore observes only the currently visible exact
allowlisted conversation. It never uses coordinates, OCR, synthesized mouse
clicks, or automatic conversation switching. Unknown Accessibility structures
fail closed.

Recent context is read with the native macOS AX API. The helper caps reads at
30 rows and sets a per-application messaging timeout so stale UI nodes cannot
block the channel indefinitely. The context contract labels incoming messages
as `[对方]` and signed-in-user messages as `[我]`; Codex learns tone and action
style only from `[我]` examples. Unclear requests must become a minimal,
style-matched clarification question.

For action-oriented requests, Codex emits user-visible plan, observable
progress, result, and final-response blocks. The channel sends those blocks in
order (or stores them as ordered drafts). Hidden chain-of-thought and internal
reasoning are never requested or forwarded.

The channel is configured per agent. The setup API resolves the agent from
QwenPaw's existing `X-Agent-Id` header, rejects non-Codex agents, verifies the
Codex OAuth state through the harness adapter, detects the local DingTalk
session, writes the channel config, and requests an agent hot reload.

## Safety model

- Default reply mode is `draft`; generated text is stored for review and is
  not inserted into DingTalk.
- Automatic mode is an explicit opt-in.
- An allowlist limits monitored conversations. Initial setup captures only the
  currently open conversation.
- A content fingerprint prevents duplicate replies and self-reply loops.
- The driver refuses to act if the expected bundle id, window structure, or
  conversation title does not match.
- Only a row marked `session msg receiving` is accepted as an inbound message;
  sending rows and unknown structures are ignored.
- The plugin never logs message bodies or OAuth/session credentials.

## Checklist

- [x] Base work on current upstream `main` in a dedicated branch/worktree.
- [x] Verify QwenPaw Codex app-server and ChatGPT OAuth support.
- [x] Verify the local iDingTalk bundle, login window, and Accessibility tree.
- [x] Define a QwenPaw-native channel plugin instead of a compatibility layer.
- [x] Implement macOS status, semantic observation, and exact-title send bridge.
- [x] Implement encrypted-database change detection and AX message capture.
- [x] Implement per-conversation sessions and duplicate/self-loop protection.
- [x] Implement draft-first replies and explicit draft approval.
- [x] Implement an agent-scoped one-click setup page.
- [x] Enforce Codex backend and OAuth readiness during setup.
- [x] Add allowlist and explicit automatic-reply opt-in.
- [x] Add recent semantic context and signed-in-user style grounding.
- [x] Add style-matched clarification and observable progress messages.
- [x] Add unit and contract tests with 100% pass rate for touched suites.
- [x] Run plugin validation, Python checks, frontend checks, and local probes.
- [x] Verify offline install/list/validate in an isolated QwenPaw home.
- [ ] Complete a local draft-mode end-to-end test without sending a message.

## Out of scope

- DingTalk robot, Stream, webhook, or OpenAPI compatibility.
- Reverse engineering or decrypting DingTalk's local database.
- Background operation while macOS is locked or DingTalk is signed out.
- Coordinate, OCR, or synthesized-mouse conversation navigation.
- Automatic replies for every conversation immediately after installation.
- Windows/Linux desktop automation in this first macOS-specific release.
