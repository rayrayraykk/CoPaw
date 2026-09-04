import assert from "node:assert/strict";
import { PassThrough } from "node:stream";
import test from "node:test";

import { AppServerClient } from "../src/appServerClient";
import { PROTOCOL_VERSION } from "../src/protocol";

test("initializes and exposes typed requests and notifications", async () => {
  const input = new PassThrough();
  const output = new PassThrough();
  let written = "";
  output.setEncoding("utf8");
  output.on("data", (chunk: string) => {
    written += chunk;
    const lines = written.split("\n");
    if (lines.length < 2) {
      return;
    }
    const request = JSON.parse(lines[0] ?? "{}") as { id: number };
    input.write(
      `${JSON.stringify({
        id: request.id,
        result: {
          protocolVersion: PROTOCOL_VERSION,
          serverInfo: { name: "qwenpaw-core", version: "0.2.0" },
        },
      })}\n`,
    );
  });
  const client = await AppServerClient.connect(input, output, {
    clientInfo: {
      name: "typescript-sdk-test",
      title: "TypeScript SDK Test",
      version: "0.2.0",
    },
  });
  const notification = new Promise<string>((resolve) => {
    client.onNotification("item/agentMessage/delta", (event) => {
      resolve(event.delta);
    });
  });
  input.write(
    `${JSON.stringify({
      method: "item/agentMessage/delta",
      params: {
        threadId: "thread",
        turnId: "turn",
        itemId: "item",
        delta: "hello",
      },
    })}\n`,
  );
  assert.equal(await notification, "hello");
  assert.match(written, /"method":"initialize"/);
  assert.match(written, /"method":"initialized"/);
  client.dispose();
});
