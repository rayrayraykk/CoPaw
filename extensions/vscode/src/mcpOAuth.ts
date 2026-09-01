export type McpOAuthWaitResult = "authorized" | "cancelled" | "timedOut";

interface McpOAuthWaitOptions {
  readonly readAuthorized: () => Promise<boolean>;
  readonly isCancelled: () => boolean;
  readonly sleep?: (milliseconds: number) => Promise<void>;
  readonly intervalMilliseconds?: number;
  readonly maxAttempts?: number;
}

export async function waitForMcpAuthorization(
  options: McpOAuthWaitOptions,
): Promise<McpOAuthWaitResult> {
  const sleep = options.sleep ?? defaultSleep;
  const interval = options.intervalMilliseconds ?? 2_000;
  const maxAttempts = options.maxAttempts ?? 300;
  for (let attempt = 0; attempt < maxAttempts; attempt += 1) {
    if (options.isCancelled()) {
      return "cancelled";
    }
    if (await options.readAuthorized()) {
      return "authorized";
    }
    if (attempt + 1 < maxAttempts) {
      await sleep(interval);
    }
  }
  return "timedOut";
}

function defaultSleep(milliseconds: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}
