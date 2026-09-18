import assert from "node:assert/strict";
import { createServer } from "node:http";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { QwenPaw } from "../src/qwenpaw";

const corePath = process.env.QWENPAW_CORE_BIN;
const png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";

test("image paths reach the real Core and survive another turn", {
  skip: corePath ? false : "QWENPAW_CORE_BIN is not set", timeout: 30_000,
}, async () => {
  assert.ok(corePath);
  const directory = await mkdtemp(path.join(os.tmpdir(), "qwenpaw-ts-image-"));
  const requests: any[] = [];
  const server = createServer(async (request, response) => {
    let body = "";
    for await (const chunk of request) body += chunk;
    requests.push(JSON.parse(body));
    response.writeHead(200, { "content-type": "text/event-stream" });
    response.end('data: {"choices":[{"delta":{"content":"red"}}]}\n\ndata: [DONE]\n\n');
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert.ok(address && typeof address !== "string");
  let app: QwenPaw | undefined;
  try {
    await writeFile(path.join(directory, "red.png"), Buffer.from(png, "base64"));
    app = await QwenPaw.start({ corePath,
      clientInfo: { name: "image-test", title: "Image Test", version: "0.2.0" },
      env: { ...process.env,
      QWENPAW_HOME: directory, QWENPAW_API_KEY: "image-fixture-key",
      QWENPAW_BASE_URL: `http://127.0.0.1:${address.port}` } });
    const thread = await app.startThread({ workspaceRoot: directory });
    const result = await thread.run([{ type: "image", path: "red.png" }]);
    assert.equal(result.finalResponse, "red");
    assert.deepEqual(result.items[0], { type: "userMessage", id: result.items[0]?.id,
      text: "", input: [{ type: "image", path: "red.png" }] });
    await writeFile(path.join(directory, "red.png"), "changed");
    assert.equal((await thread.run("Recall the image")).finalResponse, "red");
    assert.equal(requests.length, 2);
    for (const request of requests) {
      assert.deepEqual(request.messages[1], { role: "user", content: [
        { type: "image_url", image_url: { url: `data:image/png;base64,${png}` } },
      ] });
    }
  } finally {
    await app?.close();
    await new Promise<void>((resolve) => server.close(() => resolve()));
    await rm(directory, { recursive: true, force: true });
  }
});
