import assert from "node:assert/strict";
import { PassThrough } from "node:stream";
import test from "node:test";

import { RpcClient, RpcRequestError } from "../src/rpcClient";

test("correlates a response with its request", async () => {
  const fromServer = new PassThrough();
  const toServer = new PassThrough();
  const client = new RpcClient(fromServer, toServer);
  toServer.once("data", (chunk: Buffer) => {
    const request = JSON.parse(chunk.toString()) as { id: number };
    fromServer.write(
      `${JSON.stringify({ id: request.id, result: { value: 42 } })}\n`,
    );
  });

  const result = await client.request<{ value: number }>("test/read", {});

  assert.deepEqual(result, { value: 42 });
  client.dispose();
});

test("delivers server notifications", async () => {
  const fromServer = new PassThrough();
  const toServer = new PassThrough();
  const client = new RpcClient(fromServer, toServer);
  const notification = new Promise<{ method: string; params: unknown }>(
    (resolve) => {
      client.onNotification((method, params) => resolve({ method, params }));
    },
  );

  fromServer.write(
    `${JSON.stringify({ method: "turn/started", params: { id: "turn-1" } })}\n`,
  );

  assert.deepEqual(await notification, {
    method: "turn/started",
    params: { id: "turn-1" },
  });
  client.dispose();
});

test("rejects protocol errors", async () => {
  const fromServer = new PassThrough();
  const toServer = new PassThrough();
  const client = new RpcClient(fromServer, toServer);
  toServer.once("data", (chunk: Buffer) => {
    const request = JSON.parse(chunk.toString()) as { id: number };
    fromServer.write(
      `${JSON.stringify({
        id: request.id,
        error: { code: -32601, message: "method not found" },
      })}\n`,
    );
  });

  await assert.rejects(client.request("missing/read", {}), (error: unknown) => {
    assert.ok(error instanceof RpcRequestError);
    assert.equal(error.code, -32601);
    assert.equal(error.rpcMessage, "method not found");
    assert.match(error.message, /QwenPaw Core error -32601/);
    return true;
  });
  client.dispose();
});

test("reports a connection close exactly once", async () => {
  const fromServer = new PassThrough();
  const toServer = new PassThrough();
  const client = new RpcClient(fromServer, toServer);
  const errors: string[] = [];
  const closed = new Promise<void>((resolve) => {
    client.onClose((error) => {
      errors.push(error.message);
      resolve();
    });
  });

  fromServer.end();
  await closed;
  client.dispose();

  assert.deepEqual(errors, ["QwenPaw Core closed its output"]);
});

test("reports an already closed connection immediately", () => {
  const fromServer = new PassThrough();
  const toServer = new PassThrough();
  const client = new RpcClient(fromServer, toServer);
  client.dispose();
  const errors: string[] = [];

  const subscription = client.onClose((error) => errors.push(error.message));
  subscription.dispose();

  assert.deepEqual(errors, ["QwenPaw Core connection is closed"]);
});
