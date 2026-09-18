import { isIP } from "node:net";
import { PassThrough, Writable } from "node:stream";
import { inspect } from "node:util";
import WebSocket from "ws";

import {
  AppServerClient,
  type AppServerConnectionOptions,
} from "./appServerClient";
import { type ThreadStartParams } from "./protocol";
import { type Disposable } from "./rpcClient";
import { QwenPawThread } from "./thread";

const MAX_MESSAGE_BYTES = 1_048_576;
const CONNECT_TIMEOUT_MS = 15_000;
const DISCONNECT_TIMEOUT_MS = 5_000;

export interface WebSocketConnectionOptions extends AppServerConnectionOptions {
  readonly bearerToken?: string;
  /** Explicit PEM roots replace default roots; verification remains enabled. */
  readonly ca?: string | Buffer | readonly (string | Buffer)[];
  /** Cancels connection establishment and initialization, not accepted turns. */
  readonly signal?: AbortSignal;
}

/** Owns only a connection to an existing Core, never its process. */
export class WebSocketConnection implements Disposable {
  #closing: Promise<void> | undefined;
  readonly #transport: WebSocketTransport;

  private constructor(
    public readonly client: AppServerClient,
    transport: WebSocketTransport,
  ) {
    this.#transport = transport;
  }

  public static async connect(
    endpoint: string,
    options: WebSocketConnectionOptions,
  ): Promise<WebSocketConnection> {
    const url = validatedEndpoint(endpoint, options.bearerToken);
    if (options.signal?.aborted)
      throw new Error("QwenPaw connection was aborted");
    const ca = options.ca;
    const socket = new WebSocket(url, {
      headers: options.bearerToken
        ? { Authorization: `Bearer ${options.bearerToken}` }
        : undefined,
      ca:
        ca === undefined || typeof ca === "string" || Buffer.isBuffer(ca)
          ? ca
          : [...ca],
      rejectUnauthorized: true,
      followRedirects: false,
      handshakeTimeout: CONNECT_TIMEOUT_MS,
      maxPayload: MAX_MESSAGE_BYTES,
      perMessageDeflate: false,
    });
    const transport = new WebSocketTransport(socket);
    const abort = (): void =>
      transport.fail(new Error("QwenPaw connection was aborted"));
    options.signal?.addEventListener("abort", abort, { once: true });
    try {
      await transport.opened;
      transport.assertOpen();
      const client = await AppServerClient.connect(
        transport.input,
        transport.output,
        options,
      );
      if (options.signal?.aborted) {
        client.dispose();
        throw new Error("QwenPaw connection was aborted");
      }
      return new WebSocketConnection(client, transport);
    } catch (error) {
      transport.dispose();
      if (options.signal?.aborted)
        throw new Error("QwenPaw connection was aborted");
      throw error;
    } finally {
      options.signal?.removeEventListener("abort", abort);
    }
  }

  public async startThread(
    options: Partial<ThreadStartParams> = {},
  ): Promise<QwenPawThread> {
    const response = await this.client.request("thread/start", {
      model: options.model ?? null,
      workspaceRoot: options.workspaceRoot ?? null,
    });
    return new QwenPawThread(this.client, response.thread);
  }

  public async resumeThread(threadId: string): Promise<QwenPawThread> {
    const response = await this.client.request("thread/resume", { threadId });
    return new QwenPawThread(this.client, response.thread);
  }

  /** Disconnects this client; does not interrupt turns or acknowledge saving. */
  public disconnect(): Promise<void> {
    if (!this.#closing) {
      this.#closing = Promise.resolve().then(() => this.#transport.close());
      this.client.dispose();
    }
    return this.#closing;
  }

  /** Immediate local cleanup. Use disconnect() to await graceful closure. */
  public dispose(): void {
    this.client.dispose();
    this.#transport.dispose();
  }

  public [inspect.custom](): string {
    return "WebSocketConnection { credentials: [REDACTED] }";
  }
}

function validatedEndpoint(endpoint: string, token: string | undefined): URL {
  const invalid = (): Error =>
    new Error(
      "Invalid Core endpoint; use WSS or literal loopback WS without URL credentials, query or fragment",
    );
  let url: URL;
  try {
    url = new URL(endpoint);
  } catch {
    throw invalid();
  }
  const host = url.hostname.replace(/^\[|\]$/g, "");
  const loopback =
    host === "::1" || (isIP(host) === 4 && host.startsWith("127."));
  if (
    !["ws:", "wss:"].includes(url.protocol) ||
    (url.protocol === "ws:" && !loopback) ||
    url.username ||
    url.password ||
    url.search ||
    url.hash ||
    endpoint.includes("?") ||
    endpoint.includes("#")
  )
    throw invalid();
  if (token !== undefined && !/^[\x21-\x7e]{32,4096}$/.test(token)) {
    throw new Error("Invalid Core bearer token");
  }
  return url;
}

/** WS messages are normalized to one JSON line for the existing protocol client. */
class WebSocketTransport implements Disposable {
  public readonly input = new PassThrough();
  public readonly output: Writable;
  public readonly opened: Promise<void>;
  private readonly closed: Promise<void>;
  private failure: Error | undefined;
  private ended = false;

  public constructor(private readonly socket: WebSocket) {
    // ws's request timeout is inactivity-based; also bound the whole handshake.
    const handshakeTimer = setTimeout(() => {
      this.fail(new Error("Core WebSocket handshake timed out"));
    }, CONNECT_TIMEOUT_MS);
    socket.once("open", () => clearTimeout(handshakeTimer));
    socket.once("close", () => clearTimeout(handshakeTimer));
    this.output = new Writable({
      write: (chunk: Buffer, _encoding, done) => {
        if (!this.ended) {
          if (chunk.length > MAX_MESSAGE_BYTES + 1) {
            this.fail(new Error("Core outgoing message exceeds 1 MiB"));
          } else if (
            socket.bufferedAmount + chunk.length >
            MAX_MESSAGE_BYTES * 2
          ) {
            this.fail(new Error("Core WebSocket write buffer limit exceeded"));
          } else {
            socket.send(chunk.toString("utf8").trimEnd(), (error) => {
              if (error) this.fail(new Error("Core WebSocket write failed"));
            });
          }
        }
        done();
      },
    });
    this.output.on("error", () =>
      this.fail(new Error("Core WebSocket write failed")),
    );
    this.opened = new Promise<void>((resolve, reject) => {
      socket.once("open", resolve);
      socket.once("error", () =>
        reject(this.failure ?? new Error("Core WebSocket handshake failed")),
      );
      socket.once("close", () =>
        reject(
          this.failure ?? new Error("Core WebSocket closed during handshake"),
        ),
      );
      socket.once("unexpected-response", (_request, response) => {
        const error = new Error(
          `Core WebSocket handshake returned HTTP ${response.statusCode}`,
        );
        reject(error);
        response.destroy();
        this.fail(error);
      });
    });
    this.closed = new Promise<void>((resolve) =>
      socket.once("close", (code) => {
        if (![1000, 1001, 1005].includes(code)) {
          this.failure ??= new Error("Core WebSocket closed abnormally");
        }
        this.end();
        resolve();
      }),
    );
    socket.on("error", () =>
      this.fail(new Error("Core WebSocket transport failed")),
    );
    socket.on("message", (data, binary) => {
      if (this.ended) return;
      if (binary) {
        this.fail(new Error("Core WebSocket expected a text message"));
        return;
      }
      try {
        const value: unknown = JSON.parse(data.toString());
        if (value === null || typeof value !== "object" || Array.isArray(value))
          throw new Error();
        this.input.write(`${JSON.stringify(value)}\n`);
      } catch {
        this.fail(new Error("Core WebSocket returned invalid JSON message"));
      }
    });
  }

  public fail(error: Error): void {
    this.failure ??= error;
    this.end();
    this.socket.terminate();
  }

  public assertOpen(): void {
    if (this.ended || this.socket.readyState !== WebSocket.OPEN) {
      throw this.failure ?? new Error("Core WebSocket connection is closed");
    }
  }

  public dispose(): void {
    this.fail(new Error("Core WebSocket connection was disposed"));
  }

  public async close(): Promise<void> {
    this.end();
    let timer: NodeJS.Timeout | undefined;
    try {
      const timedOut = new Promise<never>((_resolve, reject) => {
        timer = setTimeout(() => {
          const error = new Error("Core WebSocket disconnect timed out");
          this.fail(error);
          reject(error);
        }, DISCONNECT_TIMEOUT_MS);
      });
      if (this.socket.readyState === WebSocket.OPEN) this.socket.close(1000);
      await Promise.race([this.closed, timedOut]);
      if (this.failure) throw this.failure;
    } finally {
      clearTimeout(timer);
    }
  }

  private end(): void {
    if (this.ended) return;
    this.ended = true;
    this.input.end();
    this.output.end();
  }
}
