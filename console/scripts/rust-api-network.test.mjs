import assert from "node:assert/strict";
import test from "node:test";
import ts from "typescript";

import { networkCallsForUrl } from "./rust-api-network.mjs";

function inspect(source) {
  const file = ts.createSourceFile(
    "fixture.ts",
    source,
    ts.ScriptTarget.Latest,
    true,
  );
  const result = [];
  function visit(node) {
    if (
      ts.isCallExpression(node) &&
      node.expression.getText(file) === "getApiUrl"
    ) {
      result.push(
        networkCallsForUrl(node, file).map(({ kind, node: consumer }) => ({
          kind,
          options: consumer.arguments?.[1]?.getText(file) ?? null,
        })),
      );
    }
    ts.forEachChild(node, visit);
  }
  visit(file);
  return result;
}

test("resolves distinct local URLs without mixing their HTTP methods", () => {
  assert.deepEqual(
    inspect(`
    async function create() { const url = getApiUrl('/backups/stream'); await fetch(url, { method: 'POST' }); }
    async function events() { const url = getApiUrl('/backups/jobs/j/events'); await fetch(url); }
    async function upload() { const url = getApiUrl('/backups/import'); await fetch(url, { method: 'POST' }); }
  `),
    [
      [{ kind: "fetch", options: "{ method: 'POST' }" }],
      [{ kind: "fetch", options: null }],
      [{ kind: "fetch", options: "{ method: 'POST' }" }],
    ],
  );
});

test("distinguishes lexical shadows and records every real consumer", () => {
  assert.deepEqual(
    inspect(`
    function run() {
      const url = getApiUrl('/jobs');
      { const url = '/unrelated'; fetch(url, { method: 'DELETE' }); }
      fetch(url);
      fetch(url, { method: 'POST' });
    }
  `),
    [
      [
        { kind: "fetch", options: null },
        { kind: "fetch", options: "{ method: 'POST' }" },
      ],
    ],
  );
});

test("retains directly nested transports and does not infer a mutable URL", () => {
  assert.deepEqual(
    inspect(`
    fetch(getApiUrl('/create'), { method: 'POST' });
    new EventSource(getApiUrl('/events'));
    const socketUrl = getApiUrl('/socket'); new WebSocket(socketUrl);
    let url = getApiUrl('/initial'); url = '/other'; fetch(url);
  `),
    [
      [{ kind: "fetch", options: "{ method: 'POST' }" }],
      [{ kind: "EventSource", options: null }],
      [{ kind: "WebSocket", options: null }],
      [],
    ],
  );
});
