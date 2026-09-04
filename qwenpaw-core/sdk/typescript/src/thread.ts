import {
  AppServerClient,
  type AppProtocolNotification,
} from "./appServerClient";
import { type Item, type Thread, type Turn, type UserInput } from "./protocol";
import { type Disposable } from "./rpcClient";

export interface TurnResult {
  readonly finalResponse: string;
  readonly turn: Turn;
  readonly items: readonly Item[];
}

export class QwenPawThread {
  public constructor(
    private readonly client: AppServerClient,
    public readonly thread: Thread,
  ) {}

  public get id(): string {
    return this.thread.id;
  }

  public async startTurn(
    input: string | readonly UserInput[],
  ): Promise<TurnStream> {
    const queue = new NotificationQueue();
    const subscription = this.client.onEvent((notification) => {
      if (notificationThreadId(notification) === this.id) {
        queue.push(notification);
      }
    });
    const closeSubscription = this.client.onClose((error) => {
      queue.fail(error);
    });
    try {
      const response = await this.client.request("turn/start", {
        threadId: this.id,
        input:
          typeof input === "string"
            ? [{ type: "text", text: input }]
            : [...input],
      });
      return new TurnStream(this.client, response.turn, queue, [
        subscription,
        closeSubscription,
      ]);
    } catch (error) {
      subscription.dispose();
      closeSubscription.dispose();
      queue.close();
      throw error;
    }
  }

  public async run(input: string | readonly UserInput[]): Promise<TurnResult> {
    const stream = await this.startTurn(input);
    let finalResponse = "";
    let completed: Turn | undefined;
    for await (const notification of stream) {
      if (notification.method === "item/agentMessage/delta") {
        finalResponse += notification.params.delta;
      } else if (notification.method === "turn/completed") {
        completed = notification.params.turn;
      }
    }
    if (!completed) {
      throw new Error("QwenPaw turn ended without a completion");
    }
    if (completed.status !== "completed") {
      throw new Error(
        `QwenPaw turn ended with status ${completed.status}: ${
          completed.error?.message ?? "unknown error"
        }`,
      );
    }
    return {
      finalResponse,
      turn: completed,
      items: completed.items,
    };
  }
}

export class TurnStream implements AsyncIterable<AppProtocolNotification> {
  public constructor(
    private readonly client: AppServerClient,
    public readonly turn: Turn,
    private readonly queue: NotificationQueue,
    private readonly subscriptions: readonly Disposable[],
  ) {}

  public async interrupt(): Promise<boolean> {
    const response = await this.client.request("turn/interrupt", {
      threadId: this.turn.threadId,
      turnId: this.turn.id,
    });
    return response.accepted;
  }

  public async *[Symbol.asyncIterator](): AsyncIterator<AppProtocolNotification> {
    try {
      while (true) {
        const notification = await this.queue.next();
        yield notification;
        if (
          notification.method === "turn/completed" &&
          notification.params.turn.id === this.turn.id
        ) {
          return;
        }
      }
    } finally {
      for (const subscription of this.subscriptions) {
        subscription.dispose();
      }
      this.queue.close();
    }
  }
}

class NotificationQueue {
  private readonly values: AppProtocolNotification[] = [];
  private readonly waiters: Array<
    (value: AppProtocolNotification | undefined) => void
  > = [];
  private closed = false;
  private closeError: Error | undefined;

  public push(value: AppProtocolNotification): void {
    if (this.closed) {
      return;
    }
    const waiter = this.waiters.shift();
    if (waiter) {
      waiter(value);
    } else {
      this.values.push(value);
    }
  }

  public async next(): Promise<AppProtocolNotification> {
    const value = this.values.shift();
    if (value) {
      return value;
    }
    if (this.closed) {
      throw this.closedError();
    }
    const next = await new Promise<AppProtocolNotification | undefined>(
      (resolve) => this.waiters.push(resolve),
    );
    if (!next) {
      throw this.closedError();
    }
    return next;
  }

  public close(): void {
    this.closed = true;
    for (const waiter of this.waiters.splice(0)) {
      waiter(undefined);
    }
  }

  public fail(error: Error): void {
    this.closeError = error;
    this.close();
  }

  private closedError(): Error {
    return (
      this.closeError ?? new Error("QwenPaw notification stream is closed")
    );
  }
}

function notificationThreadId(
  notification: AppProtocolNotification,
): string | undefined {
  const params = notification.params as unknown as Record<string, unknown>;
  if (typeof params.threadId === "string") {
    return params.threadId;
  }
  const turn = params.turn;
  if (typeof turn === "object" && turn !== null) {
    const threadId = (turn as Record<string, unknown>).threadId;
    return typeof threadId === "string" ? threadId : undefined;
  }
  const thread = params.thread;
  if (typeof thread === "object" && thread !== null) {
    const id = (thread as Record<string, unknown>).id;
    return typeof id === "string" ? id : undefined;
  }
  return undefined;
}
