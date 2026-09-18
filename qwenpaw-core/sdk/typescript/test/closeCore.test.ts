import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { createServer } from "node:http";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { QwenPaw, type QwenPawOptions } from "../src/qwenpaw";

const corePath = process.env.QWENPAW_CORE_BIN;

for (const rejectFinal of [false, true]) {
  test(
    rejectFinal
      ? "close reports a real Core final persistence failure before startup recovery"
      : "close interrupts a real Core request and reopening retains the complete turn",
    {
      skip: corePath ? false : "QWENPAW_CORE_BIN is not set",
      timeout: 15_000,
    },
    async () => {
      assert.ok(corePath);
      const directory = await mkdtemp(
        path.join(os.tmpdir(), "qwenpaw-ts-eof-core-"),
      );
      let requested: () => void = () => undefined;
      const modelRequested = new Promise<void>((resolve) => {
        requested = resolve;
      });
      const server = createServer((request, response) => {
        request.resume();
        response.writeHead(200, { "content-type": "text/event-stream" });
        response.flushHeaders();
        requested();
        // Hold the stream until the real Core cancels its request during shutdown.
      });
      await new Promise<void>((resolve) =>
        server.listen(0, "127.0.0.1", resolve),
      );
      const address = server.address();
      assert.ok(address && typeof address !== "string");
      const options: QwenPawOptions = {
        corePath,
        cwd: directory,
        clientInfo: { name: "eof-core", title: "EOF Core Test", version: "1" },
        env: {
          ...process.env,
          QWENPAW_HOME: directory,
          QWENPAW_API_KEY: "close-fixture-key",
          QWENPAW_BASE_URL: `http://127.0.0.1:${address.port}/v1`,
        },
      };
      let app: QwenPaw | undefined;
      try {
        app = await QwenPaw.start(options);
        const thread = await app.startThread({ workspaceRoot: directory });
        await thread.startTurn("hold until shutdown");
        await modelRequested;
        const before = await app.client.request("thread/read", {
          threadId: thread.id,
        });
        assert.equal(before.turns.length, 1);
        assert.equal(before.turns[0]?.status, "inProgress");
        // This acceptance test uses Node 24; the SDK runtime still supports Node 18.
        const { DatabaseSync } = await import("node:sqlite");
        if (rejectFinal) {
          const fault = new DatabaseSync(
            path.join(directory, "threads.sqlite3"),
          );
          try {
            fault.exec(`CREATE TRIGGER reject_final BEFORE INSERT ON threads
          WHEN json_extract(NEW.snapshot, '$.turns[#-1].status') != 'inProgress'
          BEGIN SELECT RAISE(FAIL, 'fixture final write failure'); END;`);
          } finally {
            fault.close();
          }
          const error = {
            name: "Error",
            message: "QwenPaw Core exited with code 1",
          };
          await assert.rejects(app.close(), error);
          await assert.rejects(app.close(), error);
        } else {
          await app.close();
        }
        // Inspect before Core startup can recover an unsaved in-progress turn.
        const database = new DatabaseSync(
          path.join(directory, "threads.sqlite3"),
          {
            readOnly: true,
          },
        );
        let saved;
        try {
          const row = database
            .prepare("SELECT snapshot FROM threads WHERE id = ?")
            .get(thread.id);
          assert.ok(row && typeof row.snapshot === "string");
          saved = JSON.parse(row.snapshot);
        } finally {
          database.close();
        }
        if (rejectFinal) {
          assert.deepEqual(
            { thread: saved.thread, turns: saved.turns },
            before,
          );
          const fault = new DatabaseSync(
            path.join(directory, "threads.sqlite3"),
          );
          try {
            fault.exec("DROP TRIGGER reject_final");
          } finally {
            fault.close();
          }
        } else {
          assert.equal(saved.thread.status, "idle");
          assert.deepEqual(saved.turns, [
            { ...before.turns[0], status: "interrupted" },
          ]);
        }
        app = await QwenPaw.start(options);
        const after = await app.client.request("thread/read", {
          threadId: thread.id,
        });
        assert.ok(after.thread.updatedAt >= before.thread.updatedAt);
        assert.deepEqual(after, {
          thread: {
            ...before.thread,
            status: "idle",
            updatedAt: after.thread.updatedAt,
          },
          turns: [{ ...before.turns[0], status: "interrupted" }],
        });
        if (!rejectFinal) {
          assert.deepEqual(after, { thread: saved.thread, turns: saved.turns });
        }
        await app.close();
        app = await QwenPaw.start(options);
        assert.deepEqual(
          await app.client.request("thread/read", { threadId: thread.id }),
          after,
        );
        await app.close();
      } finally {
        app?.dispose();
        server.closeAllConnections();
        await new Promise<void>((resolve) => server.close(() => resolve()));
        await rm(directory, { recursive: true, force: true });
      }
    },
  );
}
