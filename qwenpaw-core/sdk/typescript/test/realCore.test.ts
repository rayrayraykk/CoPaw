import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { QwenPaw } from "../src/qwenpaw";

const corePath = process.env.QWENPAW_CORE_BIN;

test(
  "starts the real App Server and creates a thread",
  { skip: corePath ? false : "QWENPAW_CORE_BIN is not set" },
  async () => {
    assert.ok(corePath);
    const home = await mkdtemp(path.join(os.tmpdir(), "qwenpaw-ts-sdk-"));
    const qwenpaw = await QwenPaw.start({
      corePath,
      env: { ...process.env, QWENPAW_HOME: home },
      clientInfo: {
        name: "qwenpaw_typescript_sdk_test",
        title: "QwenPaw TypeScript SDK Test",
        version: "0.2.0",
      },
    });
    try {
      const thread = await qwenpaw.startThread();
      assert.equal(thread.thread.status, "idle");
      assert.equal(thread.thread.archived, false);
    } finally {
      await qwenpaw.close();
      await rm(home, { recursive: true, force: true });
    }
  },
);
