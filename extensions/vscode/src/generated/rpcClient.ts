import * as readline from "node:readline";

export interface Disposable {
  dispose(): void;
}

export type NotificationHandler = (method: string, params: unknown) => void;
export type CloseHandler = (error: Error) => void;

interface PendingRequest {
  readonly resolve: (value: unknown) => void;
  readonly reject: (error: Error) => void;
  readonly timeout: NodeJS.Timeout;
}

interface RpcErrorPayload {
  readonly code: number;
  readonly message: string;
}

interface RpcMessage {
  readonly id?: number;
  readonly result?: unknown;
  readonly error?: RpcErrorPayload;
  readonly method?: string;
  readonly params?: unknown;
}

export class RpcRequestError extends Error {
  public override readonly name = "RpcRequestError";

  public constructor(
    public readonly code: number,
    public readonly rpcMessage: string,
  ) {
    super(`QwenPaw Core error ${code}: ${rpcMessage}`);
  }
}

export class RpcClient implements Disposable {
  private readonly pending = new Map<number, PendingRequest>();
  private readonly notificationHandlers = new Set<NotificationHandler>();
  private readonly closeHandlers = new Set<CloseHandler>();
  private readonly lines: readline.Interface;
  private nextId = 1;
  private disposed = false;

  public constructor(
    input: NodeJS.ReadableStream,
    private readonly output: NodeJS.WritableStream,
    private readonly requestTimeoutMs = 15_000,
  ) {
    this.lines = readline.createInterface({ input });
    this.lines.on("line", (line) => this.handleLine(line));
    this.lines.on("close", () => {
      this.disposeWithError(new Error("QwenPaw Core closed its output"));
    });
  }

  public request<T>(method: string, params: unknown): Promise<T> {
    if (this.disposed) {
      return Promise.reject(new Error("QwenPaw Core connection is closed"));
    }
    const id = this.nextId;
    this.nextId += 1;
    return new Promise<T>((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`QwenPaw Core request timed out: ${method}`));
      }, this.requestTimeoutMs);
      this.pending.set(id, {
        resolve: (value) => resolve(value as T),
        reject,
        timeout,
      });
      const message = JSON.stringify({ id, method, params });
      this.output.write(`${message}\n`, (error?: Error | null) => {
        if (error) {
          this.rejectPending(id, error);
        }
      });
    });
  }

  public notify(method: string, params: unknown): void {
    if (!this.disposed) {
      this.output.write(`${JSON.stringify({ method, params })}\n`);
    }
  }

  public onNotification(handler: NotificationHandler): Disposable {
    this.notificationHandlers.add(handler);
    return {
      dispose: () => this.notificationHandlers.delete(handler),
    };
  }

  public onClose(handler: CloseHandler): Disposable {
    if (this.disposed) {
      handler(new Error("QwenPaw Core connection is closed"));
      return { dispose: () => undefined };
    }
    this.closeHandlers.add(handler);
    return {
      dispose: () => this.closeHandlers.delete(handler),
    };
  }

  public dispose(): void {
    this.disposeWithError(new Error("QwenPaw Core connection was disposed"));
  }

  private handleLine(line: string): void {
    let message: RpcMessage;
    try {
      message = JSON.parse(line) as RpcMessage;
    } catch (error) {
      this.disposeWithError(
        new Error(`QwenPaw Core returned invalid JSON: ${String(error)}`),
      );
      return;
    }

    if (typeof message.id === "number") {
      const pending = this.pending.get(message.id);
      if (!pending) {
        return;
      }
      this.pending.delete(message.id);
      clearTimeout(pending.timeout);
      if (message.error) {
        pending.reject(
          new RpcRequestError(message.error.code, message.error.message),
        );
      } else {
        pending.resolve(message.result);
      }
      return;
    }

    if (message.method) {
      for (const handler of this.notificationHandlers) {
        handler(message.method, message.params);
      }
    }
  }

  private rejectPending(id: number, error: Error): void {
    const pending = this.pending.get(id);
    if (!pending) {
      return;
    }
    this.pending.delete(id);
    clearTimeout(pending.timeout);
    pending.reject(error);
  }

  private disposeWithError(error: Error): void {
    if (this.disposed) {
      return;
    }
    this.disposed = true;
    this.lines.removeAllListeners();
    for (const [id, pending] of this.pending) {
      clearTimeout(pending.timeout);
      pending.reject(error);
      this.pending.delete(id);
    }
    for (const handler of this.closeHandlers) {
      handler(error);
    }
    this.closeHandlers.clear();
    this.notificationHandlers.clear();
  }
}
