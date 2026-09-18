import assert from "node:assert/strict";
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { EventEmitter, once } from "node:events";
import { PassThrough } from "node:stream";
import test from "node:test";
import { setImmediate } from "node:timers/promises";

import { OwnedCoreProcess } from "../src/ownedCoreProcess";

test("EOF drains delayed output and concurrent close waits for process closure", async () => {
  const child = spawn(process.execPath, ["-e", `
    process.stdin.resume();
    process.stdin.on('end', () => setTimeout(() => {
      process.stdout.write('x'.repeat(1024 * 1024));
      process.stderr.write('y'.repeat(1024 * 1024));
    }, 40));
  `], { stdio: "pipe" });
  const owned = new OwnedCoreProcess(child);
  let stdout = 0, stderr = 0;
  child.stdout.on("data", (chunk: Buffer) => { stdout += chunk.length; });
  child.stderr.on("data", (chunk: Buffer) => { stderr += chunk.length; });
  const closed = owned.close();
  assert.equal(owned.close(), closed);
  await closed;
  assert.deepEqual({ stdout, stderr, code: child.exitCode, signal: child.signalCode }, {
    stdout: 1024 * 1024, stderr: 1024 * 1024, code: 0, signal: null,
  });
});

test("nonzero exit before close remains an error", async () => {
  const child = spawn(process.execPath, ["-e", "process.exit(7)"], { stdio: "pipe" });
  const owned = new OwnedCoreProcess(child);
  await once(child, "close");
  const closing = owned.close();
  await assert.rejects(closing, { message: "QwenPaw Core exited with code 7" });
  assert.equal(owned.close(), closing);
});

for (const confirmsExit of [true, false]) {
  test(`timeout never reports success (exit confirmed: ${confirmsExit})`, async (t) => {
    const child = new EventEmitter() as ChildProcessWithoutNullStreams;
    Object.assign(child, {
      stdin: new PassThrough(), stdout: new PassThrough(), stderr: new PassThrough(),
    });
    const signals: string[] = [];
    child.kill = (signal) => {
      signals.push(String(signal));
      if (confirmsExit) child.emit("close", null, "SIGKILL");
      return true;
    };
    t.mock.timers.enable({ apis: ["setTimeout"] });
    const owned = new OwnedCoreProcess(child);
    const closing = assert.rejects(owned.close(), {
      message: confirmsExit
        ? "QwenPaw Core shutdown timed out; forced termination is not a successful save"
        : "QwenPaw Core shutdown timed out; process termination could not be confirmed",
    });
    await setImmediate();
    t.mock.timers.tick(30_000);
    await setImmediate();
    assert.deepEqual(signals, ["SIGKILL"]);
    t.mock.timers.tick(5_000);
    await closing;
  });
}

test("signal exit is a failure even when the output streams close", async () => {
  const child = new EventEmitter() as ChildProcessWithoutNullStreams;
  Object.assign(child, {
    stdin: new PassThrough(), stdout: new PassThrough(), stderr: new PassThrough(),
  });
  const owned = new OwnedCoreProcess(child);
  child.emit("close", null, "SIGTERM");
  await assert.rejects(owned.close(), { message: "QwenPaw Core exited with signal SIGTERM" });
});
