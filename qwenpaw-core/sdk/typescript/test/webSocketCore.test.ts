import assert from "node:assert/strict";
import { spawn, execFileSync } from "node:child_process";
import { mkdtemp, readFile, rm, chmod, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import os from "node:os";
import path from "node:path";
import test, { type TestContext } from "node:test";

import { WebSocketConnection, type WebSocketConnectionOptions } from "../src";

const corePath = process.env.QWENPAW_CORE_BIN;
const options: WebSocketConnectionOptions = {
  clientInfo: {
    name: "ts-shared-core",
    title: "Shared Core test",
    version: "1",
  },
};
const token = "fixture-core-token-01234567890123456789";

async function directory(): Promise<string> {
  const home = await mkdtemp(path.join(os.tmpdir(), "qwenpaw-ts-ws-"));
  // Registered by callers after their child and server cleanup.
  return home;
}

async function startCore(
  t: TestContext,
  home: string,
  args: string[] = [],
  env: NodeJS.ProcessEnv = {},
) {
  assert.ok(corePath);
  const child = spawn(
    corePath,
    ["app-server", "--listen", "127.0.0.1:0", ...args],
    {
      cwd: home,
      env: { ...process.env, QWENPAW_HOME: home, RUST_LOG: "info", ...env },
      stdio: "pipe",
    },
  );
  child.stdout.resume();
  const exit = new Promise<{
    code: number | null;
    signal: NodeJS.Signals | null;
  }>((resolve) => {
    child.once("close", (code, signal) => resolve({ code, signal }));
  });
  t.after(async () => {
    if (child.exitCode === null && child.signalCode === null)
      child.kill("SIGTERM");
    await exit;
    await rm(home, { recursive: true, force: true });
  });
  const port = await new Promise<string>((resolve, reject) => {
    let logs = "";
    const timer = setTimeout(
      () => reject(new Error("source Core listen timed out")),
      5_000,
    );
    const finish = (): void => clearTimeout(timer);
    child.once("error", (error) => {
      finish();
      reject(error);
    });
    child.once("close", () => {
      finish();
      reject(new Error("source Core exited before listening"));
    });
    child.stderr.on("data", (chunk: Buffer) => {
      logs += chunk.toString();
      const match = /address=127\.0\.0\.1:(\d+)/.exec(logs);
      if (match?.[1]) {
        finish();
        resolve(match[1]);
      }
    });
  });
  return { endpoint: `ws://127.0.0.1:${port}/app-protocol`, child };
}

test(
  "WS real Core shares threads, survives detach and completes an accepted turn",
  {
    skip: corePath ? false : "QWENPAW_CORE_BIN is not set",
    timeout: 15_000,
  },
  async (t) => {
    const home = await directory();
    let release!: () => void;
    let announce!: () => void;
    const requested = new Promise<void>((resolve) => {
      announce = resolve;
    });
    const model = createServer((request, response) => {
      assert.equal(request.headers.authorization, "Bearer local-model-key");
      request.resume();
      release = () => {
        response.writeHead(200, { "content-type": "text/event-stream" });
        response.end(
          'data: {"choices":[{"delta":{"content":"reply after detach"},"finish_reason":null}]}\n\ndata: [DONE]\n\n',
        );
      };
      announce();
    });
    await new Promise<void>((resolve) => model.listen(0, "127.0.0.1", resolve));
    t.after(() => {
      model.closeAllConnections();
      return new Promise<void>((resolve) => model.close(() => resolve()));
    });
    const address = model.address();
    assert.ok(address && typeof address !== "string");
    const { endpoint, child } = await startCore(t, home, [], {
      QWENPAW_API_KEY: "local-model-key",
      QWENPAW_BASE_URL: `http://127.0.0.1:${address.port}/v1`,
    });
    const first = await WebSocketConnection.connect(endpoint, options);
    const second = await WebSocketConnection.connect(endpoint, options);
    t.after(() => {
      first.dispose();
      second.dispose();
    });
    const config = await second.client.request("config/read", {});
    const thread = await first.startThread({ workspaceRoot: home });
    const resumed = await second.resumeThread(thread.id);
    assert.deepEqual(resumed.thread, thread.thread);
    await thread.startTurn("wait for detach");
    await requested;
    const before = await first.client.request("thread/read", {
      threadId: thread.id,
    });
    assert.equal(before.turns[0]?.status, "inProgress");
    await first.disconnect();
    assert.deepEqual(
      await second.client.request("thread/read", { threadId: thread.id }),
      before,
    );
    release();
    let after;
    do {
      await new Promise<void>((resolve) => setTimeout(resolve, 10));
      after = await second.client.request("thread/read", {
        threadId: thread.id,
      });
    } while (after.turns[0]?.status === "inProgress");
    assert.equal(after.turns[0]?.status, "completed");
    assert.match(JSON.stringify(after), /reply after detach/);
    assert.deepEqual(await second.client.request("config/read", {}), config);
    await second.disconnect();
    assert.equal(child.exitCode, null);
    assert.equal(child.signalCode, null);
    const third = await WebSocketConnection.connect(endpoint, options);
    try {
      assert.deepEqual(
        await third.client.request("thread/read", { threadId: thread.id }),
        after,
      );
    } finally {
      await third.disconnect();
    }
  },
);

test(
  "WSS real Core enforces CA, hostname and token without changing the host",
  {
    skip: corePath ? false : "QWENPAW_CORE_BIN is not set",
    timeout: 15_000,
  },
  async (t) => {
    const home = await directory();
    const certificate = path.join(home, "cert.pem"),
      key = path.join(home, "key.pem"),
      tokenPath = path.join(home, "token");
    execFileSync(
      "openssl",
      [
        "req",
        "-x509",
        "-newkey",
        "rsa:2048",
        "-nodes",
        "-days",
        "1",
        "-subj",
        "/CN=localhost",
        "-addext",
        "subjectAltName=DNS:localhost",
        "-keyout",
        key,
        "-out",
        certificate,
      ],
      { stdio: "ignore" },
    );
    await chmod(key, 0o600);
    await writeFile(tokenPath, token, { mode: 0o600 });
    const { endpoint, child } = await startCore(t, home, [
      "--remote",
      "--tls-cert",
      certificate,
      "--tls-key",
      key,
      "--auth-token-file",
      tokenPath,
    ]);
    const ca = await readFile(certificate);
    const url = endpoint
      .replace("ws:", "wss:")
      .replace("127.0.0.1", "localhost");
    await assert.rejects(
      WebSocketConnection.connect(url, { ...options, bearerToken: token }),
      /handshake failed/,
    );
    await assert.rejects(
      WebSocketConnection.connect(endpoint.replace("ws:", "wss:"), {
        ...options,
        bearerToken: token,
        ca,
      }),
      /handshake failed/,
    );
    for (const bearerToken of [
      undefined,
      "wrong-token-0123456789012345678901234",
    ]) {
      await assert.rejects(
        WebSocketConnection.connect(url, { ...options, ca, bearerToken }),
        {
          message: "Core WebSocket handshake returned HTTP 401",
        },
      );
    }
    const first = await WebSocketConnection.connect(url, {
      ...options,
      bearerToken: token,
      ca,
    });
    const second = await WebSocketConnection.connect(url, {
      ...options,
      bearerToken: token,
      ca: [ca],
    });
    t.after(() => {
      first.dispose();
      second.dispose();
    });
    const config = await first.client.request("config/read", {});
    await first.disconnect();
    assert.deepEqual(await second.client.request("config/read", {}), config);
    await second.disconnect();
    assert.equal(child.exitCode, null);
    assert.equal(child.signalCode, null);
  },
);
