import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp, realpath, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { AddressInfo } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";

import { RpcClient } from "../src/rpcClient";

const executableName = process.platform === "win32" ? "qwenpaw-core.exe" : "qwenpaw-core";
const defaultCorePath = resolve(
  __dirname,
  "../../../../qwenpaw-core/target/debug",
  executableName,
);
const corePath = process.env.QWENPAW_CORE_BIN ?? defaultCorePath;

test(
  "connects to the real QwenPaw Core protocol",
  { skip: !existsSync(corePath) },
  async () => {
    const coreHome = await mkdtemp(resolve(tmpdir(), "qwenpaw-core-test-"));
    const child = spawn(corePath, ["app-server", "--stdio"], {
      env: {
        ...process.env,
        QWENPAW_API_KEY: "integration-secret",
        QWENPAW_BASE_URL: "https://bootstrap.test/v1",
        QWENPAW_HOME: coreHome,
        QWENPAW_MODEL: "qwen-test",
      },
      stdio: "pipe",
    });
    const rpc = new RpcClient(child.stdout, child.stdin, 5_000);
    try {
      const initialized = await rpc.request<{
        protocolVersion: number;
        serverInfo: { name: string; version: string };
      }>("initialize", {
        clientInfo: { name: "vscode_test", version: "0.1.0" },
      });
      assert.deepEqual(initialized, {
        protocolVersion: 2,
        serverInfo: { name: "qwenpaw-core", version: "0.1.0" },
      });

      const started = await rpc.request<{
        thread: {
          id: string;
          model: string;
          workspaceRoot: string;
          status: string;
          archived: boolean;
          createdAt: number;
          updatedAt: number;
        };
      }>("thread/start", { workspaceRoot: coreHome });
      assert.match(started.thread.id, /^thr_/);
      assert.equal(started.thread.status, "idle");
      assert.equal(started.thread.workspaceRoot, await realpath(coreHome));

      const models = await rpc.request<{
        data: Array<{
          id: string;
          displayName: string;
          isDefault: boolean;
        }>;
      }>("model/list", {});
      assert.deepEqual(models, {
        data: [
          {
            id: "qwen-test",
            displayName: "qwen-test",
            isDefault: true,
          },
        ],
      });

      const initialConfig = await rpc.request<{
        config: {
          baseUrl: string;
          defaultModel: string;
          apiKeyConfigured: boolean;
        };
      }>("config/read", {});
      assert.deepEqual(initialConfig, {
        config: {
          baseUrl: "https://bootstrap.test/v1",
          defaultModel: "qwen-test",
          apiKeyConfigured: true,
        },
      });
      const updatedConfig = await rpc.request<typeof initialConfig>(
        "config/write",
        {
          baseUrl: "https://configured.test/v1/",
          defaultModel: "qwen-configured",
        },
      );
      assert.deepEqual(updatedConfig, {
        config: {
          baseUrl: "https://configured.test/v1",
          defaultModel: "qwen-configured",
          apiKeyConfigured: true,
        },
      });

      const threads = await rpc.request<{
        data: typeof started.thread[];
        nextCursor: string | null;
      }>("thread/list", {
        cursor: null,
        limit: 200,
        includeArchived: false,
      });
      assert.deepEqual(threads, {
        data: [started.thread],
        nextCursor: null,
      });

      const workspace = {
        root: await realpath(coreHome),
        threadCount: 1,
        archivedThreadCount: 0,
        updatedAt: started.thread.updatedAt,
      };
      const workspaces = await rpc.request<{ data: Array<typeof workspace> }>(
        "workspace/list",
        {},
      );
      assert.deepEqual(workspaces, { data: [workspace] });
      assert.deepEqual(
        await rpc.request("workspace/read", { root: workspace.root }),
        { workspace },
      );

      const restored = await rpc.request<{
        thread: typeof started.thread;
        turns: unknown[];
      }>("thread/read", { threadId: started.thread.id });
      assert.deepEqual(restored, {
        thread: started.thread,
        turns: [],
      });

      const archived = await rpc.request<{
        thread: typeof started.thread;
      }>("thread/archive", { threadId: started.thread.id });
      assert.deepEqual(archived.thread, {
        ...started.thread,
        archived: true,
        updatedAt: archived.thread.updatedAt,
      });
      const hidden = await rpc.request<{
        data: Array<typeof started.thread>;
        nextCursor: string | null;
      }>("thread/list", {
        cursor: null,
        limit: 200,
        includeArchived: false,
      });
      assert.deepEqual(hidden, { data: [], nextCursor: null });
      const resumed = await rpc.request<{
        thread: typeof started.thread;
      }>("thread/resume", { threadId: started.thread.id });
      assert.deepEqual(resumed.thread, {
        ...archived.thread,
        archived: false,
        updatedAt: resumed.thread.updatedAt,
      });
    } finally {
      rpc.dispose();
      child.kill();
      await rm(coreHome, { recursive: true, force: true });
    }
  },
);

test(
  "maps a real Core file reference without reading its content",
  { skip: !existsSync(corePath), timeout: 10_000 },
  async () => {
    const requests: Array<Record<string, unknown>> = [];
    const modelServer = createServer((request, response) => {
      let body = "";
      request.setEncoding("utf8");
      request.on("data", (chunk: string) => {
        body += chunk;
      });
      request.on("end", () => {
        requests.push(JSON.parse(body) as Record<string, unknown>);
        response.writeHead(200, { "content-type": "text/event-stream" });
        response.end(
          [
            'data: {"choices":[{"delta":{"content":"Reference received"}}]}',
            "",
            "data: [DONE]",
            "",
          ].join("\n"),
        );
      });
    });
    await new Promise<void>((resolve) =>
      modelServer.listen(0, "127.0.0.1", resolve),
    );
    const address = modelServer.address() as AddressInfo;
    const coreHome = await mkdtemp(resolve(tmpdir(), "qwenpaw-core-ref-test-"));
    const sourceDirectory = join(coreHome, "src");
    const sourcePath = join(sourceDirectory, "main.rs");
    const fileSecret = "FILE_CONTENT_MUST_NOT_BE_EAGERLY_READ";
    await mkdir(sourceDirectory);
    await writeFile(sourcePath, `${fileSecret}\nsecond\nthird\n`);
    const child = spawn(corePath, ["app-server", "--stdio"], {
      env: {
        ...process.env,
        QWENPAW_BASE_URL: `http://127.0.0.1:${address.port}`,
        QWENPAW_HOME: coreHome,
        QWENPAW_MODEL: "qwen-test",
      },
      stdio: "pipe",
    });
    const rpc = new RpcClient(child.stdout, child.stdin, 5_000);
    try {
      await rpc.request("initialize", {
        clientInfo: { name: "vscode_reference_test", version: "0.1.0" },
      });
      const started = await rpc.request<{ thread: { id: string } }>(
        "thread/start",
        { workspaceRoot: coreHome },
      );
      let resolveCompletion: (() => void) | undefined;
      let rejectCompletion: ((error: Error) => void) | undefined;
      const completion = new Promise<void>((resolve, reject) => {
        resolveCompletion = resolve;
        rejectCompletion = reject;
      });
      const notifications = rpc.onNotification((method, params) => {
        if (method !== "turn/completed") {
          return;
        }
        const turn = (params as { turn: { status: string; error?: string } })
          .turn;
        if (turn.status === "completed") {
          resolveCompletion?.();
        } else {
          rejectCompletion?.(
            new Error(`file reference turn ended as ${turn.status}: ${turn.error}`),
          );
        }
      });
      try {
        await rpc.request("turn/start", {
          threadId: started.thread.id,
          input: [
            { type: "text", text: "Inspect this file" },
            {
              type: "fileReference",
              path: sourcePath,
              startLine: 2,
              endLine: 3,
            },
          ],
        });
        await completion;
      } finally {
        notifications.dispose();
      }
      assert.equal(requests.length, 1);
      const requestBody = requests[0];
      assert.ok(requestBody);
      const messages = requestBody.messages as Array<Record<string, unknown>>;
      assert.deepEqual(messages[messages.length - 1], {
        role: "user",
        content: [
          "Inspect this file",
          "",
          "Workspace file references (contents are not included; use read_file when needed):",
          '[{"endLine":3,"path":"src/main.rs","startLine":2}]',
        ].join("\n"),
      });
      assert.equal(JSON.stringify(requestBody).includes(fileSecret), false);
    } finally {
      rpc.dispose();
      child.kill();
      modelServer.closeAllConnections();
      modelServer.close();
      await rm(coreHome, { recursive: true, force: true });
    }
  },
);

test(
  "paginates real Core threads over stdio",
  { skip: !existsSync(corePath) },
  async () => {
    const coreHome = await mkdtemp(resolve(tmpdir(), "qwenpaw-core-page-test-"));
    const child = spawn(corePath, ["app-server", "--stdio"], {
      env: {
        ...process.env,
        QWENPAW_BASE_URL: "https://pagination.test/v1",
        QWENPAW_HOME: coreHome,
        QWENPAW_MODEL: "qwen-test",
      },
      stdio: "pipe",
    });
    const rpc = new RpcClient(child.stdout, child.stdin, 5_000);
    try {
      await rpc.request("initialize", {
        clientInfo: { name: "vscode_pagination_test", version: "0.1.0" },
      });
      type TestThread = {
        id: string;
        model: string;
        workspaceRoot: string;
        status: string;
        archived: boolean;
        createdAt: number;
        updatedAt: number;
      };
      const threads: TestThread[] = [];
      for (let index = 0; index < 3; index += 1) {
        const started = await rpc.request<{ thread: TestThread }>(
          "thread/start",
          { workspaceRoot: coreHome },
        );
        threads.push(started.thread);
      }
      const expected = threads.slice().sort((left, right) => {
        if (left.updatedAt !== right.updatedAt) {
          return right.updatedAt - left.updatedAt;
        }
        return right.id > left.id ? 1 : right.id < left.id ? -1 : 0;
      });

      const first = await rpc.request<{
        data: TestThread[];
        nextCursor: string | null;
      }>("thread/list", {
        cursor: null,
        limit: 2,
        includeArchived: false,
      });
      assert.deepEqual(first, {
        data: expected.slice(0, 2),
        nextCursor: "2",
      });
      assert.deepEqual(
        await rpc.request("thread/list", {
          cursor: first.nextCursor,
          limit: 2,
          includeArchived: false,
        }),
        {
          data: expected.slice(2),
          nextCursor: null,
        },
      );
    } finally {
      rpc.dispose();
      child.kill();
      await rm(coreHome, { recursive: true, force: true });
    }
  },
);

test(
  "approves a real Core shell tool over stdio",
  { skip: !existsSync(corePath), timeout: 10_000 },
  async () => {
    const requests: unknown[] = [];
    const modelServer = createServer((request, response) => {
      let body = "";
      request.setEncoding("utf8");
      request.on("data", (chunk: string) => {
        body += chunk;
      });
      request.on("end", () => {
        requests.push(JSON.parse(body) as unknown);
        response.writeHead(200, { "content-type": "text/event-stream" });
        if (requests.length === 1) {
          response.end(
            [
              'data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_shell","function":{"name":"shell","arguments":"{\\"command\\":\\"echo approved\\"}"}}]}}]}',
              "",
              "data: [DONE]",
              "",
            ].join("\n"),
          );
        } else {
          response.end(
            [
              'data: {"choices":[{"delta":{"content":"Command completed"}}]}',
              "",
              "data: [DONE]",
              "",
            ].join("\n"),
          );
        }
      });
    });
    await new Promise<void>((resolve) => modelServer.listen(0, "127.0.0.1", resolve));
    const address = modelServer.address() as AddressInfo;
    const coreHome = await mkdtemp(resolve(tmpdir(), "qwenpaw-core-tool-test-"));
    const child = spawn(corePath, ["app-server", "--stdio"], {
      env: {
        ...process.env,
        QWENPAW_BASE_URL: `http://127.0.0.1:${address.port}`,
        QWENPAW_HOME: coreHome,
        QWENPAW_MODEL: "qwen-test",
      },
      stdio: "pipe",
    });
    const rpc = new RpcClient(child.stdout, child.stdin, 5_000);
    try {
      await rpc.request("initialize", {
        clientInfo: { name: "vscode_tool_test", version: "0.1.0" },
      });
      const started = await rpc.request<{ thread: { id: string } }>(
        "thread/start",
        { workspaceRoot: coreHome },
      );
      let responseText = "";
      let activeTurnId: string | undefined;
      let resolveCompletion: (() => void) | undefined;
      let rejectCompletion: ((error: Error) => void) | undefined;
      const completion = new Promise<void>((resolve, reject) => {
        resolveCompletion = resolve;
        rejectCompletion = reject;
      });
      const notifications = rpc.onNotification((method, params) => {
        const payload = params as Record<string, unknown>;
        if (method === "tool/approval/requested") {
          void rpc
            .request<{ accepted: boolean }>("tool/approval/respond", {
              approvalId: payload.approvalId,
              decision: "approved",
            })
            .then((result) => assert.equal(result.accepted, true))
            .catch((error: unknown) => rejectCompletion?.(new Error(String(error))));
        } else if (method === "item/agentMessage/delta") {
          responseText += String(payload.delta);
        } else if (method === "turn/completed") {
          const turn = payload.turn as { id: string; status: string };
          if (turn.id === activeTurnId && turn.status === "completed") {
            resolveCompletion?.();
          }
        }
      });
      try {
        const turn = await rpc.request<{ turn: { id: string } }>("turn/start", {
          threadId: started.thread.id,
          input: [{ type: "text", text: "Run echo approved" }],
        });
        activeTurnId = turn.turn.id;
        await completion;
      } finally {
        notifications.dispose();
      }
      assert.equal(responseText, "Command completed");
      assert.equal(requests.length, 2);
    } finally {
      rpc.dispose();
      child.kill();
      modelServer.close();
      await rm(coreHome, { recursive: true, force: true });
    }
  },
);

test(
  "interrupts a real Core turn waiting for model response headers",
  { skip: !existsSync(corePath), timeout: 10_000 },
  async () => {
    let stalledResponse: import("node:http").ServerResponse | undefined;
    let resolveModelRequest: (() => void) | undefined;
    const modelRequested = new Promise<void>((resolve) => {
      resolveModelRequest = resolve;
    });
    const modelServer = createServer((request, response) => {
      request.resume();
      stalledResponse = response;
      resolveModelRequest?.();
    });
    await new Promise<void>((resolve) =>
      modelServer.listen(0, "127.0.0.1", resolve),
    );
    const address = modelServer.address() as AddressInfo;
    const coreHome = await mkdtemp(resolve(tmpdir(), "qwenpaw-core-cancel-test-"));
    const child = spawn(corePath, ["app-server", "--stdio"], {
      env: {
        ...process.env,
        QWENPAW_BASE_URL: `http://127.0.0.1:${address.port}`,
        QWENPAW_HOME: coreHome,
        QWENPAW_MODEL: "qwen-test",
      },
      stdio: "pipe",
    });
    const rpc = new RpcClient(child.stdout, child.stdin, 5_000);
    try {
      await rpc.request("initialize", {
        clientInfo: { name: "vscode_cancel_test", version: "0.1.0" },
      });
      const started = await rpc.request<{ thread: { id: string } }>(
        "thread/start",
        { workspaceRoot: coreHome },
      );
      let activeTurnId: string | undefined;
      let resolveCompletion:
        | ((turn: { id: string; status: string; error: unknown }) => void)
        | undefined;
      const completion = new Promise<{
        id: string;
        status: string;
        error: unknown;
      }>((resolve) => {
        resolveCompletion = resolve;
      });
      const notifications = rpc.onNotification((method, params) => {
        if (method !== "turn/completed") {
          return;
        }
        const turn = (params as { turn: { id: string; status: string; error: unknown } })
          .turn;
        if (turn.id === activeTurnId) {
          resolveCompletion?.(turn);
        }
      });
      try {
        const turn = await rpc.request<{
          turn: {
            id: string;
            threadId: string;
            status: string;
            items: unknown[];
            error: unknown;
          };
        }>("turn/start", {
          threadId: started.thread.id,
          input: [{ type: "text", text: "Wait for the model" }],
        });
        activeTurnId = turn.turn.id;
        await modelRequested;
        assert.deepEqual(
          await rpc.request("turn/interrupt", {
            threadId: started.thread.id,
            turnId: activeTurnId,
          }),
          { accepted: true },
        );
        assert.deepEqual(await completion, {
          ...turn.turn,
          status: "interrupted",
        });
      } finally {
        notifications.dispose();
      }
    } finally {
      rpc.dispose();
      child.kill();
      stalledResponse?.destroy();
      modelServer.closeAllConnections();
      modelServer.close();
      await rm(coreHome, { recursive: true, force: true });
    }
  },
);
