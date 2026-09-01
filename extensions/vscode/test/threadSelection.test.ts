import assert from "node:assert/strict";
import test from "node:test";

import {
  resolveInitialThreadId,
  resolveNewThreadWorkspaceRoot,
} from "../src/threadSelection";

test("uses the thread selected for the next request", () => {
  assert.equal(
    resolveInitialThreadId("thread-history", {
      kind: "existing",
      threadId: "thread-selected",
    }),
    "thread-selected",
  );
});

test("lets an explicit new-thread selection override chat history", () => {
  assert.equal(
    resolveInitialThreadId("thread-history", { kind: "new" }),
    undefined,
  );
});

test("falls back to the thread stored in chat history", () => {
  assert.equal(
    resolveInitialThreadId("thread-history", undefined),
    "thread-history",
  );
});

test("uses an available workspace selected for the next new thread", () => {
  assert.equal(
    resolveNewThreadWorkspaceRoot(
      { kind: "new", workspaceRoot: "/workspace/b" },
      ["/workspace/a", "/workspace/b"],
      "/workspace/a",
    ),
    "/workspace/b",
  );
});

test("falls back when a selected workspace is no longer available", () => {
  assert.equal(
    resolveNewThreadWorkspaceRoot(
      { kind: "new", workspaceRoot: "/workspace/removed" },
      ["/workspace/a"],
      "/workspace/a",
    ),
    "/workspace/a",
  );
});

test("does not reuse a workspace selection for an existing thread", () => {
  assert.equal(
    resolveNewThreadWorkspaceRoot(
      { kind: "existing", threadId: "thread-selected" },
      ["/workspace/a"],
      "/workspace/a",
    ),
    "/workspace/a",
  );
});
