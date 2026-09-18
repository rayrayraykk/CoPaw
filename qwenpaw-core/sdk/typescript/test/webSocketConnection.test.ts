import assert from "node:assert/strict";
import { once } from "node:events";
import { createServer, type Server } from "node:http";
import { inspect } from "node:util";
import { type Socket } from "node:net";
import test, { type TestContext } from "node:test";
import WebSocket, { WebSocketServer } from "ws";

import { WebSocketConnection, type WebSocketConnectionOptions } from "../src";

const options: WebSocketConnectionOptions = {
  clientInfo: { name: "ws-fixture", title: "WS fixture", version: "1" },
  requestTimeoutMs: 1_000,
};
const token = "fixture-token-01234567890123456789";

function cleanupHttp(t: TestContext, server: Server): void {
  const sockets = new Set<Socket>();
  server.on("connection", (socket) => {
    sockets.add(socket);
    socket.on("close", () => sockets.delete(socket));
  });
  t.after(() => {
    for (const socket of sockets) socket.destroy();
    return new Promise<void>((resolve) => server.close(() => resolve()));
  });
}

async function listen(server: Server): Promise<string> {
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert.ok(address && typeof address !== "string");
  return `ws://127.0.0.1:${address.port}/app-protocol`;
}

async function fixture(t: TestContext, handler: (socket: WebSocket) => void) {
  const http = createServer();
  const server = new WebSocketServer({ server: http });
  server.on("connection", (socket) => {
    socket.on("error", () => undefined);
    handler(socket);
  });
  t.after(async () => {
    for (const socket of server.clients) socket.terminate();
    await new Promise<void>((resolve) =>
      server.close(() => http.close(() => resolve())),
    );
  });
  return { endpoint: await listen(http), server };
}

function initialize(socket: WebSocket, version = 3): void {
  socket.once("message", (data) => {
    const message = JSON.parse(data.toString());
    assert.equal(message.method, "initialize");
    socket.send(
      JSON.stringify(
        {
          id: message.id,
          result: {
            protocolVersion: version,
            serverInfo: { name: "fixture", version: "1" },
          },
        },
        null,
        2,
      ),
    );
  });
}

test("WS validates endpoint and token without including credentials", async () => {
  for (const endpoint of [
    "ws://localhost/",
    "ws://192.0.2.1/",
    "ws://[::]/",
    "https://localhost/",
    "wss://user:secret@localhost/",
    "wss://localhost/?secret",
    "wss://localhost/#secret",
    "wss://localhost/?",
    "wss://localhost/#",
  ]) {
    await assert.rejects(
      WebSocketConnection.connect(endpoint, options),
      (error: Error) => {
        assert.match(error.message, /Invalid Core endpoint/);
        assert.doesNotMatch(error.message, /secret/);
        return true;
      },
    );
  }
  for (const bearerToken of ["short", `${token}\r\n`, "x".repeat(4097)]) {
    await assert.rejects(
      WebSocketConnection.connect("wss://localhost/", {
        ...options,
        bearerToken,
      }),
      { message: "Invalid Core bearer token" },
    );
  }
});

test(
  "WS reuses typed correlation, pretty JSON, notifications, ping and repeated detach",
  { timeout: 5_000 },
  async (t) => {
    const received: string[] = [];
    const requests: { id: number; params: unknown }[] = [];
    let pong!: () => void;
    const pongReceived = new Promise<void>((resolve) => {
      pong = resolve;
    });
    const { endpoint, server } = await fixture(t, (socket) => {
      initialize(socket);
      socket.on("pong", () => pong());
      socket.on("message", (data) => {
        const message = JSON.parse(data.toString());
        received.push(message.method);
        if (message.method === "initialized") socket.ping("fixture");
        if (message.method !== "thread/read") return;
        requests.push(message);
        if (requests.length === 2) {
          socket.send(
            JSON.stringify({ method: "fixture/event", params: { n: 1 } }),
          );
          for (const request of [...requests].reverse()) {
            socket.send(
              JSON.stringify({ id: request.id, result: request.params }),
            );
          }
        }
      });
    });
    let authorization: string | undefined;
    server.on("headers", (_headers, request) => {
      authorization = request.headers.authorization;
    });
    const connection = await WebSocketConnection.connect(endpoint, {
      ...options,
      bearerToken: token,
    });
    t.after(() => connection.dispose());
    assert.equal(authorization, `Bearer ${token}`);
    assert.doesNotMatch(inspect(connection, { depth: 10 }), new RegExp(token));
    const event = new Promise<unknown>((resolve) =>
      connection.client.onAnyNotification((method, params) =>
        resolve({ method, params }),
      ),
    );
    assert.deepEqual(
      await Promise.all([
        connection.client.request("thread/read", { threadId: "first" }),
        connection.client.request("thread/read", { threadId: "second" }),
      ]),
      [{ threadId: "first" }, { threadId: "second" }],
    );
    assert.deepEqual(await event, {
      method: "fixture/event",
      params: { n: 1 },
    });
    await pongReceived;
    const first = connection.disconnect();
    assert.equal(connection.disconnect(), first);
    await first;
    await assert.rejects(
      connection.client.request("config/read", {}),
      /closed/,
    );
    assert.deepEqual(received, [
      "initialize",
      "initialized",
      "thread/read",
      "thread/read",
    ]);
  },
);

test(
  "WS incompatible version closes before initialized",
  { timeout: 5_000 },
  async (t) => {
    const received: string[] = [];
    const { endpoint } = await fixture(t, (socket) => {
      initialize(socket, 0);
      socket.on("message", (data) =>
        received.push(JSON.parse(data.toString()).method),
      );
    });
    await assert.rejects(
      WebSocketConnection.connect(endpoint, options),
      /Unsupported QwenPaw protocol version: 0/,
    );
    assert.deepEqual(received, ["initialize"]);
  },
);

for (const frame of [
  Buffer.from([1]),
  "not JSON secret",
  "null",
  "x".repeat(1_048_577),
]) {
  test(
    `WS invalid inbound ${
      Buffer.isBuffer(frame) ? "binary" : frame.length
    } closes pending requests`,
    { timeout: 5_000 },
    async (t) => {
      const { endpoint } = await fixture(t, (socket) => {
        initialize(socket);
        socket.on("message", (data) => {
          if (JSON.parse(data.toString()).method === "config/read")
            socket.send(frame);
        });
      });
      const connection = await WebSocketConnection.connect(endpoint, options);
      t.after(() => connection.dispose());
      await assert.rejects(
        connection.client.request("config/read", {}),
        /closed/,
      );
      await assert.rejects(connection.disconnect(), (error: Error) => {
        assert.doesNotMatch(error.message, /secret/);
        assert.match(error.message, /WebSocket/);
        return true;
      });
    },
  );
}

test(
  "WS outgoing limit rejects before sending payload",
  { timeout: 5_000 },
  async (t) => {
    const methods: string[] = [];
    const { endpoint } = await fixture(t, (socket) => {
      initialize(socket);
      socket.on("message", (data) =>
        methods.push(JSON.parse(data.toString()).method),
      );
    });
    const connection = await WebSocketConnection.connect(endpoint, options);
    t.after(() => connection.dispose());
    await assert.rejects(
      connection.client.request("thread/read", {
        threadId: "x".repeat(1_048_576),
      }),
      /closed/,
    );
    await assert.rejects(connection.disconnect(), /outgoing message exceeds/);
    assert.ok(!methods.includes("thread/read"));
  },
);

test(
  "WS redirect error is redacted and not followed",
  { timeout: 5_000 },
  async (t) => {
    let requests = 0;
    const http = createServer((_request, response) => {
      requests += 1;
      response.writeHead(302, { location: "/secret", "x-token": token });
      response.end(token);
    });
    cleanupHttp(t, http);
    await assert.rejects(
      WebSocketConnection.connect(await listen(http), options),
      {
        message: "Core WebSocket handshake returned HTTP 302",
      },
    );
    assert.equal(requests, 1);
  },
);

test(
  "WS abort during handshake destroys the connection",
  { timeout: 5_000 },
  async (t) => {
    const http = createServer();
    cleanupHttp(t, http);
    const controller = new AbortController();
    const requested = once(http, "upgrade");
    const connect = WebSocketConnection.connect(await listen(http), {
      ...options,
      signal: controller.signal,
    });
    const rejected = assert.rejects(connect, /aborted/);
    const [, socket] = await requested;
    const closed = once(socket, "close");
    const ended = once(socket, "end");
    socket.resume();
    controller.abort();
    await rejected;
    await ended;
    socket.end();
    await closed;
  },
);

test(
  "WS absolute handshake timeout clears the socket",
  { timeout: 5_000 },
  async (t) => {
    const http = createServer();
    cleanupHttp(t, http);
    t.mock.timers.enable({ apis: ["setTimeout"] });
    const requested = once(http, "upgrade");
    const connect = WebSocketConnection.connect(await listen(http), options);
    const rejected = assert.rejects(connect, /handshake timed out/);
    const [, socket] = await requested;
    const closed = once(socket, "close");
    const ended = once(socket, "end");
    socket.resume();
    t.mock.timers.tick(15_000);
    await rejected;
    await ended;
    socket.end();
    await closed;
  },
);

test(
  "WS disconnect timeout terminates an unresponsive peer",
  { timeout: 5_000 },
  async (t) => {
    let peer!: WebSocket;
    const { endpoint } = await fixture(t, (socket) => {
      peer = socket;
      initialize(socket);
    });
    const connection = await WebSocketConnection.connect(endpoint, options);
    t.after(() => connection.dispose());
    peer.pause();
    t.mock.timers.enable({ apis: ["setTimeout"] });
    const closed = once(peer, "close");
    const closing = connection.disconnect();
    const rejected = assert.rejects(closing, /disconnect timed out/);
    await Promise.resolve();
    t.mock.timers.tick(5_000);
    await rejected;
    peer.resume();
    await closed;
    assert.equal(connection.disconnect(), closing);
  },
);

test(
  "WS abort during initialize closes the peer and pending handshake",
  { timeout: 5_000 },
  async (t) => {
    let initialized!: () => void;
    const requested = new Promise<void>((resolve) => {
      initialized = resolve;
    });
    let peerClosed!: Promise<unknown>;
    const { endpoint } = await fixture(t, (socket) => {
      peerClosed = once(socket, "close");
      socket.once("message", () => initialized());
    });
    const controller = new AbortController();
    const rejected = assert.rejects(
      WebSocketConnection.connect(endpoint, {
        ...options,
        signal: controller.signal,
      }),
      { message: "QwenPaw connection was aborted" },
    );
    await requested;
    controller.abort("sensitive reason must not be reflected");
    await rejected;
    await peerClosed;
  },
);

test(
  "WS dispose wakes pending and buffer budget fails closed",
  { timeout: 5_000 },
  async (t) => {
    const { endpoint } = await fixture(t, (socket) => initialize(socket));
    const first = await WebSocketConnection.connect(endpoint, options);
    const pending = assert.rejects(
      first.client.request("config/read", {}),
      /disposed/,
    );
    first.dispose();
    await pending;
    const second = await WebSocketConnection.connect(endpoint, options);
    t.after(() => second.dispose());
    // Inject the buffer measurement, not a claim about OS-specific TCP capacity.
    t.mock.getter(WebSocket.prototype, "bufferedAmount", () => 2_097_152);
    await assert.rejects(second.client.request("config/read", {}), /closed/);
    await assert.rejects(second.disconnect(), /write buffer limit/);
  },
);
