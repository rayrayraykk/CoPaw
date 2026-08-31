# Paw Me · DingTalk

Paw Me is a QwenPaw PawApp that lets the selected Agent act as the user's
governed DingTalk digital twin. It receives and sends messages as the current
OAuth user through an app-managed DingTalk Workspace runtime. The runtime is
an implementation detail: Paw Me installs it into its own data directory and
never changes the system PATH. Paw Me is not a robot, webhook, or QwenPaw
Channel, and it performs no coordinate or desktop UI automation.

## Single-page workflow

Open `/apps/paw-me-dingtalk` from QwenPaw. The page provides the complete loop:

1. first-screen connector installation with progress and browser OAuth login;
2. visible OAuth organization/account confirmation, followed by arbitrary
   enabled QwenPaw Agent selection;
3. real-time direct and group message capture while DingTalk is in the
   background;
4. global allow-all, approval, or block-all access policy, with stronger
   per-person or per-group overrides;
5. lossless split-message aggregation and visible Agent state;
6. editable drafts with the sender and source messages visible, exact-ID
   sending, retry, deletion, and audit history.

IDs cannot be typed into the UI. A person `openDingTalkId` or group
`openConversationId` must arrive in an official DWS OAuth event before the
operator can create a per-conversation override. In approval mode, unknown
identities fail closed and never invoke an Agent. An identity-leaking or
meta-analytical Agent reply always fails closed to a review draft, even when
automatic sending is enabled.

## Context guarantee

Every raw DWS event is committed to the Paw Me SQLite database before history
loading, batching, authorization, or Agent execution. Consecutive messages are
kept in original order and grouped by a quiet window; repeated text remains
separate input.

Each real conversation ID maps to one stable PawApp session. If a new message
arrives while the Agent is running, Paw Me invokes QwenPaw's native stop
operation, cancels the incomplete local task, appends the new event, and runs
again after the quiet window with the complete persisted batch. A stopped
partial response is never sent.

Recent DingTalk history is persisted separately for tone and factual context.
It is refreshed again immediately before every Agent turn, so messages seen
during a temporary stream reconnect still participate in the reply context.
Outgoing messages sent by Paw Me are appended to the same context. Their DWS
event echoes are filtered by the OAuth owner's real ID and a bounded outbound
fingerprint, preventing a sent reply from triggering another Agent run. OAuth
tokens remain in the official runtime's secure storage and are never read by
the plugin. Installation and OAuth tasks can be cancelled and retried; browser
OAuth is bounded to two minutes instead of leaving the page spinning.

## Local requirements

- the selected QwenPaw Agent must already be ready;
- the DingTalk organization must allow personal OAuth message access;
- OAuth login is completed in the official browser authorization page.

Build the frontend with `npm ci && npm run build` from `ui`, then install the
plugin ZIP through QwenPaw's plugin installer.
