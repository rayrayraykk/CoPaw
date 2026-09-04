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

  public async close(): Promise<void> {
    if (this.process.exitCode !== null) {
      this.client.dispose();
      return;
    }
    const exited = new Promise<void>((resolve) => {
      this.process.once("exit", () => resolve());
      this.process.once("error", () => resolve());
    });
    if (!this.process.killed) {
      this.process.kill();
    }
    await exited;
    this.client.dispose();
  }
}
