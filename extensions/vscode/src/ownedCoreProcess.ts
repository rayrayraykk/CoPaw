import type { ChildProcessWithoutNullStreams } from "node:child_process";

interface Exit {
  readonly code: number | null;
  readonly signal: NodeJS.Signals | null;
}

/** Owns one spawned process, including the end of its output streams. */
export class OwnedCoreProcess {
  private readonly exited: Promise<Exit>;
  private closing: Promise<void> | undefined;

  public constructor(private readonly child: ChildProcessWithoutNullStreams) {
    this.exited = new Promise((resolve) => {
      child.once("close", (code, signal) => resolve({ code, signal }));
    });
    // A child can exit between a protocol write and EOF. Exit status remains
    // authoritative; avoid an unhandled EPIPE during cleanup.
    child.stdin.on("error", () => undefined);
  }

  public close(): Promise<void> {
    this.closing ??= Promise.resolve().then(() => this.drain());
    return this.closing;
  }

  private async drain(): Promise<void> {
    this.child.stdout.resume();
    this.child.stderr.resume();
    this.child.stdin.end();
    const exit = await this.wait(30_000);
    if (!exit) {
      this.child.kill("SIGKILL");
      const stopped = await this.wait(5_000);
      throw new Error(stopped
        ? "QwenPaw Core shutdown timed out; forced termination is not a successful save"
        : "QwenPaw Core shutdown timed out; process termination could not be confirmed");
    }
    if (exit.signal !== null) {
      throw new Error(`QwenPaw Core exited with signal ${exit.signal}`);
    }
    if (exit.code !== 0) {
      throw new Error(`QwenPaw Core exited with code ${exit.code}`);
    }
  }

  private async wait(milliseconds: number): Promise<Exit | undefined> {
    let timer: NodeJS.Timeout | undefined;
    try {
      return await Promise.race([
        this.exited,
        new Promise<undefined>((resolve) => {
          timer = setTimeout(() => resolve(undefined), milliseconds);
        }),
      ]);
    } finally {
      clearTimeout(timer);
    }
  }
}
