import assert from "node:assert/strict";
import test from "node:test";

import { RpcRequestError } from "../src/rpcClient";
import { runWithThreadRecovery } from "../src/threadRecovery";

test("replaces a stale persisted thread and retries exactly once", async () => {
  const attempts: string[] = [];

  const threadId = await runWithThreadRecovery({
    initialThreadId: "thread-stale",
    startThread: async () => "thread-new",
    runTurn: async (candidate) => {
      attempts.push(candidate);
      if (candidate === "thread-stale") {
        throw new RpcRequestError(-32000, "thread not found: thread-stale");
      }
    },
  });

  assert.equal(threadId, "thread-new");
  assert.deepEqual(attempts, ["thread-stale", "thread-new"]);
});

test("does not retry unrelated core errors", async () => {
  let starts = 0;

  await assert.rejects(
    runWithThreadRecovery({
      initialThreadId: "thread-busy",
      startThread: async () => {
        starts += 1;
        return "thread-new";
      },
      runTurn: async () => {
        throw new RpcRequestError(
          -32000,
          "thread already has an active turn: thread-busy",
        );
      },
    }),
    /thread already has an active turn/,
  );
  assert.equal(starts, 0);
});

test("replaces a thread archived outside the current chat", async () => {
  const attempts: string[] = [];

  const threadId = await runWithThreadRecovery({
    initialThreadId: "thread-archived",
    startThread: async () => "thread-new",
    runTurn: async (candidate) => {
      attempts.push(candidate);
      if (candidate === "thread-archived") {
        throw new RpcRequestError(
          -32000,
          "thread is archived: thread-archived",
        );
      }
    },
  });

  assert.equal(threadId, "thread-new");
  assert.deepEqual(attempts, ["thread-archived", "thread-new"]);
});

test("does not hide failure for a newly created thread", async () => {
  let starts = 0;

  await assert.rejects(
    runWithThreadRecovery({
      initialThreadId: undefined,
      startThread: async () => {
        starts += 1;
        return "thread-new";
      },
      runTurn: async () => {
        throw new RpcRequestError(-32000, "thread not found: thread-new");
      },
    }),
    /thread not found/,
  );
  assert.equal(starts, 1);
});
