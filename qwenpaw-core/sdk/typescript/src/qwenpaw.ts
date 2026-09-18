import { type ChildProcessWithoutNullStreams, spawn } from "node:child_process";

import {
  AppServerClient,
  type AppServerConnectionOptions,
} from "./appServerClient";
import { type Disposable } from "./rpcClient";
import { type ThreadStartParams } from "./protocol";
import { QwenPawThread } from "./thread";

export interface QwenPawOptions extends AppServerConnectionOptions {
  readonly corePath?: string;
  readonly args?: readonly string[];
  readonly cwd?: string;
  readonly env?: NodeJS.ProcessEnv;
  readonly onStderr?: (chunk: string) => void;
}

export class QwenPaw implements Disposable {
  private closing: Promise<void> | undefined;

  private constructor(
    public readonly client: AppServerClient,
    private readonly process: ChildProcessWithoutNullStreams,
  ) {}

  public static async start(options: QwenPawOptions): Promise<QwenPaw> {
    const child = spawn(
      options.corePath ?? "qwenpaw-core",
      [...(options.args ?? ["app-server", "--stdio"])],
      {
        cwd: options.cwd,
        env: options.env ?? process.env,
        stdio: "pipe",
      },
    );
    child.stderr.setEncoding("utf8");
    if (options.onStderr) {
      child.stderr.on("data", options.onStderr);
    }
    let rejectSpawn: (error: Error) => void = () => undefined;
    const spawnError = new Promise<never>((_resolve, reject) => {
      rejectSpawn = reject;
    });
    const handleSpawnError = (error: Error): void => rejectSpawn(error);
    child.once("error", handleSpawnError);
    try {
      const client = await Promise.race([
        AppServerClient.connect(child.stdout, child.stdin, {
          clientInfo: options.clientInfo,
          requestTimeoutMs: options.requestTimeoutMs,
        }),
        spawnError,
      ]);
      child.removeListener("error", handleSpawnError);
      child.on("error", () => client.dispose());
      child.stdin.on("error", () => client.dispose());
      child.on("exit", () => client.dispose());
      return new QwenPaw(client, child);
    } catch (error) {
      child.removeListener("error", handleSpawnError);
      child.kill();
      throw error;
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

  public dispose(): void {
    if (!this.process.killed) {
      this.process.kill();
    }
    this.client.dispose();
  }

  /** Stop admission, send EOF and wait for the owned Core to finish. */
  public close(): Promise<void> {
    if (!this.closing) {
      // Publish before notifying close handlers, which may call close again.
      this.closing = Promise.resolve().then(() => this.closeProcess());
      this.client.dispose();
    }
    return this.closing;
  }

  private async closeProcess(): Promise<void> {
    // Keep pipes flowing even after protocol callbacks have been disposed.
    this.process.stdout.resume();
    this.process.stderr.resume();
    this.process.stdin.end();
    if (!(await this.waitForExit(30_000))) {
      this.process.kill("SIGKILL");
      const stopped = await this.waitForExit(5_000);
      throw new Error(stopped
        ? "QwenPaw Core shutdown timed out; forced termination is not a successful save"
        : "QwenPaw Core shutdown timed out; process termination could not be confirmed");
    }
    if (this.process.signalCode !== null) {
      throw new Error(`QwenPaw Core exited with signal ${this.process.signalCode}`);
    }
    if (this.process.exitCode !== 0) {
      throw new Error(`QwenPaw Core exited with code ${this.process.exitCode}`);
    }
  }

  private waitForExit(timeoutMs: number): Promise<boolean> {
    if (this.process.exitCode !== null || this.process.signalCode !== null) {
      return Promise.resolve(true);
    }
    return new Promise<boolean>((resolve) => {
      const finish = (exited: boolean): void => {
        clearTimeout(timer);
        this.process.removeListener("exit", onExit);
        resolve(exited);
      };
      const onExit = (): void => finish(true);
      const timer = setTimeout(() => finish(false), timeoutMs);
      this.process.once("exit", onExit);
    });
  }
}
