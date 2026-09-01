import assert from "node:assert/strict";
import test from "node:test";

import { TurnProgressTracker, turnOutcome } from "../src/turnProgress";

test("classifies every protocol turn completion status", () => {
  assert.deepEqual(turnOutcome({ status: "completed", error: null }), {
    kind: "completed",
  });
  assert.deepEqual(
    turnOutcome({ status: "failed", error: { message: "model failed" } }),
    { kind: "failed", message: "model failed" },
  );
  assert.deepEqual(turnOutcome({ status: "failed", error: null }), {
    kind: "failed",
    message: "QwenPaw turn failed",
  });
  assert.deepEqual(turnOutcome({ status: "interrupted", error: null }), {
    kind: "interrupted",
  });
  assert.deepEqual(turnOutcome({ status: "inProgress", error: null }), {
    kind: "invalid",
    message: "QwenPaw Core returned a non-terminal completion",
  });
});

test("tracks a successful tool without exposing arguments or output", () => {
  const progress = new TurnProgressTracker();
  assert.equal(
    progress.itemStarted({
      type: "toolCall",
      id: "item-call",
      callId: "call-1",
      name: "shell",
      arguments: '{"command":"print-secret"}',
    }),
    "Starting shell",
  );
  assert.equal(
    progress.approvalRequested({
      threadId: "thread-1",
      turnId: "turn-1",
      approvalId: "approval-1",
      callId: "call-1",
      toolName: "shell",
      arguments: '{"command":"print-secret"}',
      workspaceRoot: "/workspace",
    }),
    "Waiting for approval: shell",
  );
  assert.equal(
    progress.approvalResolved({
      threadId: "thread-1",
      turnId: "turn-1",
      approvalId: "approval-1",
      decision: "approved",
    }),
    "Approved shell",
  );
  assert.equal(
    progress.itemCompleted({
      type: "toolResult",
      id: "item-result",
      callId: "call-1",
      content: "a-secret-tool-result",
      isError: false,
    }),
    "shell completed",
  );
});

test("reports denied and failed tools and sanitizes model-provided names", () => {
  const progress = new TurnProgressTracker();
  const unsafeName = `  remote\n tool ${"x".repeat(100)}  `;
  const started = progress.itemStarted({
    type: "toolCall",
    id: "item-call",
    callId: "call-2",
    name: unsafeName,
    arguments: "{}",
  });
  assert.equal(started?.includes("\n"), false);
  assert.equal(started?.length, "Starting ".length + 80);
  assert.equal(
    progress.approvalRequested({
      threadId: "thread-1",
      turnId: "turn-1",
      approvalId: "approval-2",
      callId: "call-2",
      toolName: "remote tool",
      arguments: "{}",
      workspaceRoot: "/workspace",
    }),
    "Waiting for approval: remote tool",
  );
  assert.equal(
    progress.approvalResolved({
      threadId: "thread-1",
      turnId: "turn-1",
      approvalId: "approval-2",
      decision: "denied",
    }),
    "Denied remote tool",
  );
  assert.equal(
    progress.itemCompleted({
      type: "toolResult",
      id: "item-result",
      callId: "call-2",
      content: "failure details",
      isError: true,
    }),
    "remote tool failed",
  );
});

test("ignores non-tool items and safely handles unmatched lifecycle events", () => {
  const progress = new TurnProgressTracker();
  assert.equal(
    progress.itemStarted({
      type: "agentMessage",
      id: "item-agent",
      text: "hello",
    }),
    undefined,
  );
  assert.equal(
    progress.itemCompleted({
      type: "toolResult",
      id: "item-result",
      callId: "unknown-call",
      content: "",
      isError: false,
    }),
    "tool completed",
  );
  assert.equal(
    progress.approvalResolved({
      threadId: "thread-1",
      turnId: "turn-1",
      approvalId: "unknown-approval",
      decision: "denied",
    }),
    "Denied tool",
  );
});
