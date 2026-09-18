import assert from "node:assert/strict";
import test from "node:test";
import { probeNativeExecution } from "./native-execution.mjs";

const probe = (script, options = {}) => probeNativeExecution(process.execPath, {
  args: ["-e", script], timeoutMs: 5_000, expectedOutput: "fixture", ...options,
});

test("native execution requires the exact successful version", async () => {
  const result = await probe('console.log("fixture");console.error(process.pid)');
  assert.ok(Number.isInteger(result.pid) && result.pid > 0);
  assert.equal(Date.parse(result.finishedAt) - Date.parse(result.startedAt), result.elapsedMs);
  assert.deepEqual(result, {
    binary: process.execPath, passed: true, pid: result.pid,
    startedAt: result.startedAt, finishedAt: result.finishedAt,
    code: 0, signal: null, timedOut: false, outputLimitExceeded: false,
    spawnError: null, terminatedByProbe: false, elapsedMs: result.elapsedMs,
    stdout: "fixture\n", stderr: `${result.pid}\n`,
  });
  assert.equal((await probe('console.log("wrong")')).passed, false);
  assert.equal((await probe('console.log("fixture");process.exit(2)')).passed, false);
});

test("native termination is distinct from probe timeout", async () => {
  const result = await probe('process.kill(process.pid, "SIGKILL")');
  assert.equal(result.passed, false);
  assert.equal(result.signal, "SIGKILL");
  assert.equal(result.timedOut, false);
  assert.equal(result.terminatedByProbe, false);
  const timeout = await probe('setInterval(()=>{},1000)', { timeoutMs: 100 });
  assert.equal(timeout.passed, false);
  assert.equal(timeout.timedOut, true);
  assert.equal(timeout.terminatedByProbe, true);
});

test("missing executables and excessive output cannot pass", async () => {
  const missing = await probeNativeExecution("/nonexistent/qwenpaw-fixture", { timeoutMs: 500 });
  assert.equal(missing.passed, false);
  assert.equal(missing.pid, null);
  assert.match(missing.spawnError, /ENOENT/);
  const noisy = await probe('process.stdout.write("x".repeat(20000));setInterval(()=>{},1000)');
  assert.equal(noisy.outputLimitExceeded, true);
  assert.equal(noisy.passed, false);
  assert.ok(noisy.stdout.length <= 16_384);
});
