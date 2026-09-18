import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import test from "node:test";
import { attachBrowserDiagnostics } from "./console_browser_diagnostics.mjs";

const origin = "http://127.0.0.1:12345";
function request(client, id, path = "/api/agents") {
  client.emit("Network.requestWillBeSent", {
    requestId: id,
    request: { url: `${origin}${path}?secret=never-record`,
      headers: { Authorization: "never-record" }, postData: "never-record" },
    documentURL: `${origin}/files`, loaderId: "files-loader", type: "Fetch",
  });
}

test("navigation diagnostics retain the source of aborted requests and errors", () => {
  const client = new EventEmitter();
  const diagnostics = attachBrowserDiagnostics(client, origin);
  diagnostics.begin("/files");
  request(client, "old");
  client.emit("Runtime.executionContextCreated", {
    context: { id: 1, auxData: { frameId: "main", isDefault: true } },
  });
  const report = diagnostics.begin("/inbox");
  client.emit("Runtime.executionContextDestroyed", { executionContextId: 1 });
  client.emit("Network.loadingFailed", {
    requestId: "old", errorText: "net::ERR_ABORTED", canceled: true,
  });
  let existingErrors = 0;
  client.on("Runtime.consoleAPICalled", () => { existingErrors += 1; });
  client.emit("Runtime.consoleAPICalled", {
    type: "error", executionContextId: 1,
    args: [{ value: "Failed to load agents" }],
  });
  const source = { requestId: "old", path: "/api/agents",
    navigationPath: "/files", documentPath: "/files",
    loaderId: "files-loader", type: "Fetch" };
  assert.deepEqual(report, {
    pendingBeforeNavigation: [source], documents: [],
    failedRequests: [{ ...source, errorText: "net::ERR_ABORTED", canceled: true }],
    errorContexts: [{ kind: "console", contextId: 1,
      navigationPath: "/files", frameId: "main", isDefault: true }],
  });
  assert.equal(existingErrors, 1);
  assert.equal(JSON.stringify(report).includes("never-record"), false);
  assert.deepEqual(diagnostics.begin("/market").pendingBeforeNavigation, []);
});

test("finished requests disappear and current document failures retain their loader", () => {
  const client = new EventEmitter();
  const diagnostics = attachBrowserDiagnostics(client, origin);
  diagnostics.begin("/files");
  request(client, "done");
  client.emit("Network.loadingFinished", { requestId: "done" });
  const report = diagnostics.begin("/inbox");
  client.emit("Page.frameNavigated", {
    frame: { id: "main", url: `${origin}/inbox`, loaderId: "inbox-loader" },
  });
  client.emit("Runtime.executionContextCreated", {
    context: { id: 2, auxData: { frameId: "main", isDefault: true } },
  });
  client.emit("Runtime.exceptionThrown", {
    exceptionDetails: { executionContextId: 2, text: "original failure" },
  });
  assert.deepEqual(report, {
    pendingBeforeNavigation: [], failedRequests: [],
    documents: [{ path: "/inbox", frameId: "main", loaderId: "inbox-loader" }],
    errorContexts: [{ kind: "exception", contextId: 2,
      navigationPath: "/inbox", frameId: "main", isDefault: true }],
  });
});

test("non-API and foreign-origin requests are not recorded", () => {
  const client = new EventEmitter();
  const diagnostics = attachBrowserDiagnostics(client, origin);
  diagnostics.begin("/files");
  request(client, "asset", "/assets/app.js");
  client.emit("Network.requestWillBeSent", {
    requestId: "foreign", request: { url: "https://example.com/api/agents" },
  });
  assert.deepEqual(diagnostics.begin("/inbox"), {
    pendingBeforeNavigation: [], documents: [], failedRequests: [], errorContexts: [],
  });
});
