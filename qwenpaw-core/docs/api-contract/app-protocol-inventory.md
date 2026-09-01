# App Protocol v2 inventory

This file is generated from `qwenpaw-protocol`.

## Requests

| Method | Params | Result |
| --- | --- | --- |
| `initialize` | `InitializeParams` | `InitializeResponse` |
| `thread/start` | `ThreadStartParams` | `ThreadStartResponse` |
| `thread/resume` | `ThreadResumeParams` | `ThreadResumeResponse` |
| `thread/archive` | `ThreadArchiveParams` | `ThreadArchiveResponse` |
| `thread/list` | `ThreadListParams` | `ThreadListResponse` |
| `thread/read` | `ThreadReadParams` | `ThreadReadResponse` |
| `turn/start` | `TurnStartParams` | `TurnStartResponse` |
| `turn/interrupt` | `TurnInterruptParams` | `TurnInterruptResponse` |
| `tool/approval/respond` | `ToolApprovalRespondParams` | `ToolApprovalRespondResponse` |
| `model/list` | `ModelListParams` | `ModelListResponse` |
| `config/read` | `ConfigReadParams` | `ConfigReadResponse` |
| `config/write` | `ConfigWriteParams` | `ConfigWriteResponse` |
| `workspace/list` | `WorkspaceListParams` | `WorkspaceListResponse` |
| `workspace/read` | `WorkspaceReadParams` | `WorkspaceReadResponse` |

## Client notifications

- `initialized`

## Server notifications

| Method | Payload |
| --- | --- |
| `thread/started` | `ThreadStartedNotification` |
| `turn/started` | `TurnStartedNotification` |
| `item/started` | `ItemStartedNotification` |
| `item/agentMessage/delta` | `AgentMessageDeltaNotification` |
| `item/completed` | `ItemCompletedNotification` |
| `tool/approval/requested` | `ToolApprovalRequestedNotification` |
| `tool/approval/resolved` | `ToolApprovalResolvedNotification` |
| `turn/completed` | `TurnCompletedNotification` |
