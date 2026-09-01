import { RpcRequestError } from "./rpcClient";

interface ThreadRecoveryOptions {
  readonly initialThreadId: string | undefined;
  readonly startThread: () => Promise<string>;
  readonly runTurn: (threadId: string) => Promise<void>;
}

export async function runWithThreadRecovery(
  options: ThreadRecoveryOptions,
): Promise<string> {
  const reusedExistingThread = options.initialThreadId !== undefined;
  let threadId = options.initialThreadId ?? (await options.startThread());
  try {
    await options.runTurn(threadId);
  } catch (error) {
    if (!reusedExistingThread || !isThreadUnavailable(error)) {
      throw error;
    }
    threadId = await options.startThread();
    await options.runTurn(threadId);
  }
  return threadId;
}

function isThreadUnavailable(error: unknown): boolean {
  return (
    error instanceof RpcRequestError &&
    error.code === -32000 &&
    (error.rpcMessage.startsWith("thread not found:") ||
      error.rpcMessage.startsWith("thread is archived:"))
  );
}
