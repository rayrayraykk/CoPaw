# DingTalk Desktop

Connect the currently selected Codex-backed QwenPaw agent to the locally
signed-in DingTalk desktop client on macOS. The plugin acts as the signed-in
user through semantic Accessibility attributes; it does not create a DingTalk
robot, call a webhook, reuse session secrets, click coordinates, or switch
conversations.

## Install

Install the `plugins/channel/dingtalk_desktop` directory through QwenPaw's
plugin manager. Open **阿里钉 · Codex** after installation and complete the
four checks shown on the page:

1. Select a Codex-backed agent.
2. Complete the existing QwenPaw Codex ChatGPT OAuth flow.
3. Open and sign in to `/Applications/iDingTalk.app`.
4. Grant Accessibility access to the QwenPaw desktop host in macOS.

Open the exact DingTalk conversation you want to bind, then choose **一键连接并使用草稿**.
The plugin authorizes that title through QwenPaw's existing channel access
control. Messages from another visible conversation enter the same pending
approval flow used by other protected channels. Removing a conversation from
that whitelist immediately prevents new drafts and sends.

## Reply modes

- `draft` is the default. Agent replies are written to an agent-scoped file
  with owner-only permissions and appear on the plugin page for approval.
- `automatic` must be selected explicitly. It sends only when the exact
  authorized conversation remains visible.

Only rows carrying DingTalk's semantic `session msg receiving` Accessibility
description are treated as inbound. Sending rows and unknown UI structures
are ignored.

The plugin gives Codex up to 16 recent semantically directed messages by
default. It learns tone only from messages sent by the signed-in user. When a
request is unclear, Codex asks a minimal clarification in that style. Tasks
with actions are split into ordered, user-visible plan/progress/result messages
without exposing hidden reasoning.

## Current boundary

The first release intentionally monitors one visible conversation. DingTalk
8.0.10 does not expose the left conversation list as semantic Accessibility
rows. The plugin therefore does not attempt background switching, OCR, or
coordinate-based interaction.
