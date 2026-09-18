import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { setTimeout as delay } from "node:timers/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { QwenPaw } from "../src/qwenpaw";

const fixture = String.raw`
const fs = require("node:fs");
const readline = require("node:readline");
const methods = [];
const lines = readline.createInterface({ input: process.stdin });
lines.on("line", (line) => {
  const request = JSON.parse(line);
  methods.push(request.method);
  if (request.method === "initialize") {
    process.stdout.write(JSON.stringify({ id: request.id, result: {
      protocolVersion: 3, serverInfo: { name: "fixture", version: "1" }
    } }) + "\n");
  }
});
lines.on("close", () => {
  const mode = process.argv[2];
  if (mode === "hold") {
    setInterval(() => {}, 1000);
    return;
  }
  const output = JSON.stringify({ method: "fixture/drain", params: {
    text: "x".repeat(2 * 1024 * 1024)
  } }) + "\n";
  process.stdout.write(output, () => {
    process.stderr.write("y".repeat(2 * 1024 * 1024), () => {
      setTimeout(() => {
        fs.writeFileSync(process.argv[1], JSON.stringify({ methods }));
        process.exitCode = Number(mode);
      }, 30);
    });
  });
});
`;

async function start(directory: string, mode = "0"): Promise<QwenPaw> {
  return QwenPaw.start({
    corePath: process.execPath,
    args: ["-e", fixture, path.join(directory, "finished.json"), mode],
    clientInfo: { name: "close-test", title: "Close Test", version: "1" },
  });
}

test("close sends EOF, drains both pipes and shares one completion", {
  timeout: 5000,
}, async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "qwenpaw-ts-close-"));
  const app = await start(directory);
  try {
    const pending = assert.rejects(
      app.client.request("thread/list", {
        cursor: null, limit: null, includeArchived: false,
      }), /disposed|closed/,
    );
    let reentrant: Promise<void> | undefined;
    app.client.onClose(() => { reentrant = app.close(); });
    const first = app.close();
    const second = app.close();
    await Promise.all([first, second, pending]);
    await assert.rejects(app.startThread(), /closed/);
    assert.deepEqual(JSON.parse(await readFile(
      path.join(directory, "finished.json"), "utf8",
    )), { methods: ["initialize", "initialized", "thread/list"] });
    assert.equal(first, second);
    assert.equal(first, reentrant);
    await app.close();
  } finally {
    app.dispose();
    await rm(directory, { recursive: true, force: true });
  }
});

test("close after synchronous dispose reports the exited signal without hanging", {
  timeout: 5000,
}, async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "qwenpaw-ts-dispose-"));
  const app = await start(directory);
  try {
    const closed = new Promise<void>((resolve) => app.client.onClose(() => resolve()));
    app.dispose();
    await closed;
    // Give the real child exit event a chance to precede close().
    await delay(100);
    await assert.rejects(app.close(), /exited with signal|exited with code/);
  } finally {
    app.dispose();
    await rm(directory, { recursive: true, force: true });
  }
});

test("close timeout terminates only its child and still reports failure", {
  timeout: 40_000,
}, async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "qwenpaw-ts-timeout-"));
  const app = await start(directory, "hold");
  try {
    const started = performance.now();
    await assert.rejects(app.close(), /shutdown timed out; forced termination/);
    assert.ok(performance.now() - started >= 29_500);
    await assert.rejects(app.close(), /shutdown timed out; forced termination/);
    await assert.rejects(readFile(path.join(directory, "finished.json")), {
      code: "ENOENT",
    });
  } finally {
    app.dispose();
    await rm(directory, { recursive: true, force: true });
  }
});

test("close reports a nonzero exit instead of claiming successful shutdown", {
  timeout: 5000,
}, async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "qwenpaw-ts-exit-"));
  const app = await start(directory, "7");
  try {
    await assert.rejects(app.close(), /exited with code 7/);
    await assert.rejects(app.close(), /exited with code 7/);
    assert.deepEqual(JSON.parse(await readFile(
      path.join(directory, "finished.json"), "utf8",
    )), { methods: ["initialize", "initialized"] });
  } finally {
    app.dispose();
    await rm(directory, { recursive: true, force: true });
  }
});
