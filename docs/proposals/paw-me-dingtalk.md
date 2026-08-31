# Paw Me · DingTalk

## Decision

`paw-me-dingtalk` is a PawApp, not a QwenPaw Channel. It owns the complete
digital-twin workflow on one page and uses the PawApp SDK to invoke whichever
enabled Agent the user selects. No backend-specific authentication checks are
implemented by the app.

The previous `dingtalk-desktop` Channel is not a compatibility layer for this
application. It remains installed only until the PawApp has passed local
installation verification, then it is disabled separately.

## Safety invariants

1. DingTalk integration uses official DWS OAuth APIs only. Coordinates,
   Accessibility automation, OCR, and image-position clicks are forbidden.
2. A display name is never an identity. Access decisions use a verified
   person or group identifier and record its source.
3. An unresolved or ambiguous identity fails closed. It can be observed and
   reviewed, but it cannot invoke an Agent or send a reply.
4. Automatic sending is opt-in per identity. Draft review is the default.
5. Sending uses the exact OAuth-derived person or group ID and a stable
   idempotency key. It never targets a display name.
6. Observable Agent milestones may be shown or sent. Hidden reasoning and
   sensitive tool parameters are never exposed.

## Application boundary

The PawApp owns these concerns:

- official DWS installation and DingTalk OAuth/account metadata;
- identity resolution and per-identity trust policy;
- background real-time direct and group event streams;
- inbound work items and their state transitions;
- selected Agent binding and per-conversation Agent sessions;
- generated drafts, sending, retry, and deletion;
- runtime heartbeat, progress, errors, and audit events.

It does not use QwenPaw's Channel ACL or ChannelManager. Agent execution goes
through `PawAppContext.chat_stream()` with the selected `agent_id`.

## Identity model

Each principal has:

- `subject_type`: `person` or `group`;
- `subject_id`: a real DingTalk person ID or `openConversationId`;
- `id_source`: the DWS OAuth event that supplied the ID;
- `display_name`: mutable UI text only;
- `conversation_alias`: mutable display metadata only;
- `policy`: `observe`, `draft`, `automatic`, or `blocked`.

DWS events provide a person `openDingTalkId` or group `openConversationId`.
Those event-derived values are read-only in the UI. A display name is never
used for identity resolution, and legacy title-as-ID ACL entries are neither
imported nor trusted.

## Work-item state machine

```text
observed
  -> identity_required
  -> blocked
  -> collecting
  -> ready
  -> agent_running
  -> interrupt_requested
  -> collecting
  -> clarification_ready
  -> draft_ready
  -> sending
  -> sent
  -> failed
```

Every transition appends an audit event. Long operations also publish a live
runtime stage over SSE so the UI never presents an unexplained spinner.

## Lossless turn aggregation

People commonly send one thought as several consecutive DingTalk messages.
The app therefore persists every inbound message before scheduling work and
groups adjacent messages from the same verified conversation into one turn.

- A turn closes after a configurable quiet window. A hard deadline prevents
  a continuously active conversation from waiting forever.
- Deduplication uses the semantic row identity when available. If DingTalk
  does not expose one, the snapshot sequence plus message direction, text,
  and conversation binding is used. Text alone is never the deduplication
  key because repeated messages are valid input.
- Messages retain their original ordering and timestamps. The Agent prompt
  contains the complete ordered turn, not only its last message.
- Every verified conversation has one stable PawApp session ID. Completed
  Agent turns remain available through that session, while the SQLite event
  log remains the authoritative copy of captured DingTalk input.
- At most one Agent task can run for a conversation. If a new message arrives
  during execution, the current task is cancelled using QwenPaw's
  stop-and-chat semantics. The new message is appended transactionally and a
  replacement run starts only after the quiet window.
- The replacement prompt includes the complete persisted turn and marks the
  earlier attempt as interrupted. This prevents a partial response from
  becoming the final answer and ensures newly arrived context is not lost.
- A generated reply is linked to the exact closed turn and its full message
  list. Sending never deletes input history.

The UI exposes `collecting`, `agent_running`, `interrupt_requested`, and
`draft_ready` explicitly, including the number of merged messages and the
quiet-window deadline.

## Single-page information architecture

The `/apps/paw-me-dingtalk` route contains the whole loop:

1. Header: account, selected Agent, master switch, reply default, heartbeat.
2. Inbox: durable messages, identity-required items, and active Agent work.
3. Identity and permissions: OAuth setup, event-derived authorization, policy.
4. Outbox: edit, send, retry, copy, or delete generated drafts.
5. Activity: timestamped capture, identity, Agent, tool, draft, send, and error
   events.

These are in-page tabs/panels, not links to Channels or external ACL screens.
The UI uses QwenPaw's host Ant Design components and theme tokens, with Lucide
icons only.

## Persistence

Application state is stored in a plugin-owned SQLite database. SQLite provides
transactional transitions and ordering without inventing a second QwenPaw
configuration format. OAuth tokens remain owned by the OAuth provider/runtime;
the PawApp stores only account identity and connection status.

## DWS event observation

Two managed DWS streams listen to all direct and group message events for the
current OAuth user. DingTalk does not need to be focused or showing the target
conversation. Flattened event IDs, message IDs, sender IDs, and conversation
IDs are persisted before any downstream processing. Stream exits surface in
the UI and retry automatically.

## Verification

- Unit tests cover schema/store transitions, identity fail-closed behavior,
  prompt construction, and frontend bundle loading.
- DWS tests verify strict event parsing, real identity selection, OAuth status,
  history projection, and exact-ID send arguments.
- The final ZIP is validated and installed through QwenPaw's plugin flow.
