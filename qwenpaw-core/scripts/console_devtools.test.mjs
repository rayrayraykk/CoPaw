import assert from "node:assert/strict";
import test from "node:test";
import { EventEmitter } from "node:events";
import { DevToolsClient, closeBrowser } from "./console_devtools.mjs";

class Socket extends EventTarget {
  readyState = 1;
  sent = [];
  send(value) { this.sent.push(JSON.parse(value)); }
  emit(type, data) {
    const event = new Event(type);
    if (data) event.data = JSON.stringify(data);
    this.dispatchEvent(event);
  }
  close() { this.readyState = 3; this.emit("close"); }
}

async function settled(promises) {
  return Promise.race([
    Promise.allSettled(promises).then(results => results.map(result => ({
      status: result.status,
      ...(result.status === "fulfilled"
        ? { value: result.value } : { message: result.reason.message }),
    }))),
    new Promise(resolve => setImmediate(() => resolve("still pending"))),
  ]);
}

test("transport preserves response IDs, protocol errors and events", async () => {
  const socket = new Socket();
  const client = new DevToolsClient(socket);
  const events = [];
  client.on("Page.loadEventFired", value => events.push(value));
  const first = client.send("Page.enable");
  const second = client.send("Runtime.evaluate", { expression: "1" });
  const result = settled([first, second]);
  socket.emit("message", { id: 2, error: { message: "protocol failure" } });
  socket.emit("message", { method: "Page.loadEventFired", params: { timestamp: 1 } });
  socket.emit("message", { id: 1, result: { enabled: true } });
  assert.deepEqual(await result, [
    { status: "fulfilled", value: { enabled: true } },
    { status: "rejected", message: "protocol failure" },
  ]);
  assert.deepEqual(events, [{ timestamp: 1 }]);
  assert.deepEqual(socket.sent, [
    { id: 1, method: "Page.enable", params: {} },
    { id: 2, method: "Runtime.evaluate", params: { expression: "1" } },
  ]);
  assert.equal(client.pending.size, 0);
});

for (const termination of ["close", "error"]) {
  test(`transport ${termination} rejects every pending command`, async () => {
    const socket = new Socket();
    const client = new DevToolsClient(socket);
    const result = settled([client.send("Page.enable"), client.send("Runtime.evaluate")]);
    socket.emit(termination);
    assert.deepEqual(await result, [
      { status: "rejected", message: `DevTools connection ${termination}` },
      { status: "rejected", message: `DevTools connection ${termination}` },
    ]);
    assert.equal(client.pending.size, 0);
  });
}

test("explicit close rejects pending and prevents later sends", async () => {
  const socket = new Socket();
  const client = new DevToolsClient(socket);
  const pending = client.send("Page.enable");
  client.close();
  assert.deepEqual(await settled([pending, client.send("Runtime.evaluate")]), [
    { status: "rejected", message: "DevTools connection close" },
    { status: "rejected", message: "DevTools connection close" },
  ]);
  assert.equal(client.pending.size, 0);
  assert.equal(socket.sent.length, 1);
});

test("synchronous send failure does not retain the request", async () => {
  const socket = new Socket();
  socket.send = () => { throw new Error("send failed"); };
  const client = new DevToolsClient(socket);
  assert.deepEqual(await settled([client.send("Page.enable")]), [
    { status: "rejected", message: "send failed" },
  ]);
  assert.equal(client.pending.size, 0);
});

class Browser extends EventEmitter {
  exitCode = null;
  signalCode = null;
  exit(code, signal = null) {
    this.exitCode = code;
    this.signalCode = signal;
    this.emit("exit", code, signal);
  }
}

test("browser shutdown verifies clean exit after protocol acknowledgement", async () => {
  const socket = new Socket();
  const browser = new Browser();
  const result = closeBrowser(new DevToolsClient(socket), browser);
  socket.emit("message", { id: 1, result: {} });
  await Promise.resolve();
  assert.equal(browser.listenerCount("exit"), 1);
  browser.exit(0);
  await result;
  assert.deepEqual(socket.sent, [{ id: 1, method: "Browser.close", params: {} }]);
  assert.equal(browser.listenerCount("exit"), 0);
});

test("browser shutdown rejects abnormal exit even after acknowledgement", async () => {
  const socket = new Socket();
  const browser = new Browser();
  const result = assert.rejects(closeBrowser(new DevToolsClient(socket), browser), {
    message: "Browser exited abnormally (1, null)",
  });
  socket.emit("message", { id: 1, result: {} });
  browser.exit(1);
  await result;
  assert.equal(browser.listenerCount("exit"), 0);
});

for (const alreadyExited of [false, true]) {
  test(`browser shutdown verifies clean exit after disconnect (${alreadyExited})`, async () => {
    const socket = new Socket();
    const browser = new Browser();
    const result = closeBrowser(new DevToolsClient(socket), browser);
    if (alreadyExited) browser.exit(0);
    socket.close();
    await Promise.resolve();
    if (!alreadyExited) {
      assert.equal(browser.listenerCount("exit"), 1);
      browser.exit(0);
    }
    await result;
    assert.equal(browser.listenerCount("exit"), 0);
  });
}

test("browser shutdown rejects abnormal process exit after disconnect", async () => {
  for (const [code, signal] of [[1, null], [null, "SIGKILL"]]) {
    const socket = new Socket();
    const browser = new Browser();
    const result = assert.rejects(closeBrowser(new DevToolsClient(socket), browser), {
      message: `Browser exited abnormally (${code}, ${signal})`,
    });
    socket.close();
    browser.exit(code, signal);
    await result;
    assert.equal(browser.listenerCount("exit"), 0);
  }
});

test("browser shutdown does not swallow protocol errors even after clean exit", async () => {
  const socket = new Socket();
  const browser = new Browser();
  const result = assert.rejects(closeBrowser(new DevToolsClient(socket), browser), {
    message: "close denied",
  });
  browser.exit(0);
  socket.emit("message", { id: 1, error: { message: "close denied" } });
  await result;
  assert.equal(browser.listenerCount("exit"), 0);
});

test("browser shutdown rejects a socket error instead of treating it as clean close", async () => {
  const socket = new Socket();
  const browser = new Browser();
  const result = assert.rejects(closeBrowser(new DevToolsClient(socket), browser), {
    message: "DevTools connection error",
  });
  browser.exit(0);
  socket.emit("error");
  await result;
});

test("browser shutdown has a bounded wait when the process remains alive", async t => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const socket = new Socket();
  const browser = new Browser();
  const result = assert.rejects(closeBrowser(new DevToolsClient(socket), browser), {
    message: "Browser did not exit after close request",
  });
  socket.close();
  await Promise.resolve();
  assert.equal(browser.listenerCount("exit"), 1);
  t.mock.timers.tick(2_000);
  await result;
  assert.equal(browser.listenerCount("exit"), 0);
  assert.equal(browser.exitCode, null);
  assert.equal(browser.signalCode, null);
});
