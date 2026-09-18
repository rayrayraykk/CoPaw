import assert from "node:assert/strict";
import test from "node:test";
import { EventEmitter } from "node:events";
import { attachShutdownDiagnostics } from "./console_browser_shutdown_diagnostics.mjs";

function fixture() {
  const child = Object.assign(new EventEmitter(), {
    pid: 27, exitCode: null, signalCode: null,
    kill() { assert.fail("diagnostics must not signal a process"); },
  });
  const socket = new EventTarget();
  socket.send = () => assert.fail("diagnostics must not send commands");
  socket.close = () => assert.fail("diagnostics must not close sockets");
  const emit = (name, value) => {
    const event = new Event(name);
    event.data = value;
    socket.dispatchEvent(event);
  };
  return { child, socket, emit, observer: attachShutdownDiagnostics(child) };
}

test("shutdown diagnostics observe exact phases without control side effects or protocol bodies", () => {
  const { child, socket, emit, observer } = fixture();
  observer.watchClose({ socket, nextId: 8 });
  emit("message", JSON.stringify({ id: 7, result: { token: "must-not-record" } }));
  emit("message", "malformed");
  emit("message", "null");
  emit("message", JSON.stringify({ id: 8, result: { token: "must-not-record" } }));
  emit("message", JSON.stringify({ id: 8, result: {} }));
  emit("close");
  child.emit("exit", 0, null);
  child.emit("close", 0, null);
  const report = observer.snapshot();
  assert.equal(report.pid, 27);
  assert.equal(Number.isNaN(Date.parse(report.startedAt)), false);
  assert.equal(report.dropped, 0);
  assert.deepEqual(report.events.map(({ elapsedMs, ...event }) => event), [
    { event: "process-observed", code: null, signal: null },
    { event: "close-request" }, { event: "close-response" },
    { event: "socket-close" }, { event: "process-exit", code: 0, signal: null },
    { event: "stdio-close", code: 0, signal: null },
  ]);
  assert(report.events.every((e, i) => e.elapsedMs >= (report.events[i - 1]?.elapsedMs ?? 0)));
  assert(!JSON.stringify(report).includes("must-not-record"));
  observer.dispose();
  observer.dispose();
  assert.deepEqual(child.eventNames(), []);
  emit("error"); child.emit("exit", 1, null);
  assert.deepEqual(observer.snapshot(), report);
  report.events[0].event = "changed externally";
  assert.equal(observer.snapshot().events[0].event, "process-observed");
});

test("shutdown diagnostics distinguish protocol failure and forced cleanup", () => {
  const { child, socket, emit, observer } = fixture();
  observer.watchClose({ socket, nextId: 1 });
  emit("message", JSON.stringify({ id: 1, error: { message: "private failure" } }));
  emit("error");
  observer.mark("close-rejected");
  observer.mark("cleanup-sigterm");
  child.emit("exit", null, "SIGTERM");
  assert.deepEqual(observer.snapshot().events.map(({ elapsedMs, ...e }) => e), [
    { event: "process-observed", code: null, signal: null },
    { event: "close-request" }, { event: "close-protocol-error" },
    { event: "socket-error" }, { event: "close-rejected" },
    { event: "cleanup-sigterm" }, { event: "process-exit", code: null, signal: "SIGTERM" },
  ]);
  observer.dispose();
});

test("shutdown diagnostics bound retained events", () => {
  const { observer } = fixture();
  for (let i = 0; i < 100; i += 1) observer.mark("phase");
  assert.equal(observer.snapshot().events.length, 64);
  assert.equal(observer.snapshot().dropped, 37);
  observer.dispose();
});
