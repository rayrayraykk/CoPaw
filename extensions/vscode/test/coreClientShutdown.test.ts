import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";
import type * as vscode from "vscode";

import type { AppServerClient } from "../src/generated/appServerClient";
import type { CoreClient as CoreClientInstance } from "../src/coreClient";

// Only the VS Code host API is replaced. CoreClient, process ownership, RPC,
// Core, HTTP model traffic and SQLite are the real implementations.
const settings: Record<string, unknown> = {};
const workspaceFolders: { uri: { fsPath: string } }[] = [];
const hostApi = {
  workspace: {
    workspaceFolders,
    getConfiguration: () => ({ get: (key: string, fallback: unknown) => settings[key] ?? fallback }),
  },
};
const loader = require("node:module") as { _load: (...args: unknown[]) => unknown };
const originalLoad = loader._load;
let CoreClient: typeof import("../src/coreClient").CoreClient;
try {
  loader._load = (...args) => args[0] === "vscode"
    ? hostApi : originalLoad.apply(loader, args);
  CoreClient = (require("../src/coreClient") as typeof import("../src/coreClient")).CoreClient;
} finally {
  loader._load = originalLoad;
}

const corePath = process.env.QWENPAW_CORE_BIN ?? resolve(
  __dirname, "../../../../qwenpaw-core/target/debug",
  process.platform === "win32" ? "qwenpaw-core.exe" : "qwenpaw-core",
);

test("startup failure waits for EOF cleanup and preserves the protocol error", async () => {
  const directory = await mkdtemp(join(tmpdir(), "qwenpaw-vscode-failed-start-"));
  const marker = join(directory, "closed.txt");
  Object.assign(settings, {
    "core.path": process.execPath,
    "core.arguments": ["-e", `
      const lines = require('node:readline').createInterface({input:process.stdin});
      lines.on('line', line => {
        const request = JSON.parse(line);
        process.stdout.write(JSON.stringify({id:request.id,result:{protocolVersion:0}})+'\\n');
      });
      process.stdin.on('end', () => setTimeout(() => {
        require('node:fs').writeFileSync(${JSON.stringify(marker)}, 'eof');
        process.exit(9);
      }, 40));
    `],
  });
  workspaceFolders.splice(0, workspaceFolders.length, { uri: { fsPath: directory } });
  const output = { append: () => undefined, appendLine: () => undefined } as unknown as vscode.OutputChannel;
  const secrets = { get: async () => undefined } as unknown as vscode.SecretStorage;
  try {
    await assert.rejects(CoreClient.start(output, secrets, directory), {
      message: "Unsupported QwenPaw protocol version: 0; expected 3",
    });
    assert.equal(await readFile(marker, "utf8"), "eof");
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

for (const rejectFinal of [false, true]) {
  test(`CoreClient awaits real Core persistence (reject final: ${rejectFinal})`, {
    skip: !existsSync(corePath), timeout: 15_000,
  }, async () => {
    const directory = await mkdtemp(join(tmpdir(), "qwenpaw-vscode-close-"));
    const previousHome = process.env.QWENPAW_HOME;
    process.env.QWENPAW_HOME = directory;
    let requested!: () => void;
    const modelRequested = new Promise<void>((resolve) => { requested = resolve; });
    const model = createServer((request, response) => {
      request.resume();
      response.writeHead(200, { "content-type": "text/event-stream" });
      response.flushHeaders();
      requested();
    });
    await new Promise<void>((resolve) => model.listen(0, "127.0.0.1", resolve));
    const address = model.address();
    assert.ok(address && typeof address !== "string");
    Object.assign(settings, {
      "core.path": corePath, "core.arguments": ["app-server", "--stdio"],
      model: "vscode-close-model", baseUrl: `http://127.0.0.1:${address.port}/v1`,
    });
    workspaceFolders.splice(0, workspaceFolders.length, { uri: { fsPath: directory } });
    const logs: string[] = [];
    const output = {
      append: (value: string) => logs.push(value),
      appendLine: (value: string) => logs.push(value),
    } as unknown as vscode.OutputChannel;
    const secrets = { get: async () => "vscode-close-fixture-key" } as unknown as vscode.SecretStorage;
    let client: CoreClientInstance | undefined;
    try {
      client = await CoreClient.start(output, secrets, resolve(__dirname, "../.."));
      const threadId = await client.startThread(directory);
      const rpc = (client as unknown as { rpc: AppServerClient }).rpc;
      await rpc.request("turn/start", {
        threadId, input: [{ type: "text", text: "hold until shutdown" }],
      });
      await modelRequested;
      const before = await rpc.request("thread/read", { threadId });
      assert.equal(before.turns[0]?.status, "inProgress");
      const { DatabaseSync } = await import("node:sqlite");
      const databasePath = join(directory, "threads.sqlite3");
      if (rejectFinal) {
        const fault = new DatabaseSync(databasePath);
        try {
          fault.exec(`CREATE TRIGGER reject_final BEFORE INSERT ON threads
            WHEN json_extract(NEW.snapshot, '$.turns[#-1].status') != 'inProgress'
            BEGIN SELECT RAISE(FAIL, 'fixture final write failure'); END;`);
        } finally { fault.close(); }
      }
      const closing = client.dispose();
      assert.equal(client.dispose(), closing);
      await assert.rejects(client.startThread(directory), /connection is closed/);
      if (rejectFinal) {
        await assert.rejects(closing, { message: "QwenPaw Core exited with code 1" });
        await assert.rejects(client.dispose(), { message: "QwenPaw Core exited with code 1" });
      } else {
        await closing;
      }
      // Inspect before any new Core can perform startup recovery.
      const database = new DatabaseSync(databasePath, { readOnly: true });
      let saved;
      try {
        const row = database.prepare("SELECT snapshot FROM threads WHERE id = ?").get(threadId);
        assert.ok(row && typeof row.snapshot === "string");
        saved = JSON.parse(row.snapshot);
      } finally { database.close(); }
      if (rejectFinal) {
        assert.deepEqual({ thread: saved.thread, turns: saved.turns }, before);
        const fault = new DatabaseSync(databasePath);
        try { fault.exec("DROP TRIGGER reject_final"); } finally { fault.close(); }
      } else {
        assert.deepEqual({ thread: saved.thread, turns: saved.turns }, {
          thread: { ...before.thread, status: "idle", updatedAt: saved.thread.updatedAt },
          turns: [{ ...before.turns[0], status: "interrupted" }],
        });
        assert.ok(saved.thread.updatedAt >= before.thread.updatedAt);
      }
      client = await CoreClient.start(output, secrets, resolve(__dirname, "../.."));
      const reopened = await (client as unknown as { rpc: AppServerClient }).rpc.request(
        "thread/read", { threadId },
      );
      assert.deepEqual(reopened, {
        thread: { ...before.thread, status: "idle", updatedAt: reopened.thread.updatedAt },
        turns: [{ ...before.turns[0], status: "interrupted" }],
      });
      if (!rejectFinal) assert.deepEqual(reopened, { thread: saved.thread, turns: saved.turns });
      await client.dispose();
    } finally {
      await client?.dispose().catch(() => undefined);
      model.closeAllConnections();
      await new Promise<void>((resolve) => model.close(() => resolve()));
      if (previousHome === undefined) delete process.env.QWENPAW_HOME;
      else process.env.QWENPAW_HOME = previousHome;
      await rm(directory, { recursive: true, force: true });
    }
  });
}
