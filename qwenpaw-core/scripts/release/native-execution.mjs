import { spawn } from "node:child_process";

// Capture native startup independently of SDK handshake timeouts. No shell,
// signing changes, trust changes, or inherited application credentials.
export function probeNativeExecution(binary, {
  args = ["--version"], timeoutMs = 45_000, graceMs = 2_000,
  expectedOutput = "qwenpaw-core 0.2.0",
} = {}) {
  return new Promise((resolve) => {
    const started = Date.now();
    const env = Object.fromEntries(Object.entries(process.env).filter(
      ([name]) => !/KEY|TOKEN|SECRET|PASSWORD|QWENPAW|COPAW|MCP|PROXY/i.test(name),
    ));
    const child = spawn(binary, args, { env, stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "", stderr = "", spawnError = null;
    let timedOut = false, outputLimitExceeded = false, escalation;
    let stopping = false;
    const stop = () => {
      if (stopping) return;
      stopping = true;
      child.kill("SIGTERM");
      escalation = setTimeout(() => child.kill("SIGKILL"), graceMs);
    };
    const timer = setTimeout(() => { timedOut = true; stop(); }, timeoutMs);
    const collect = (kind, chunk) => {
      const previous = kind === "stdout" ? stdout : stderr;
      const next = previous + chunk.toString("utf8");
      if (next.length > 16_384) {
        outputLimitExceeded = true;
        stop();
      }
      if (kind === "stdout") stdout = next.slice(0, 16_384);
      else stderr = next.slice(0, 16_384);
    };
    child.stdout.on("data", chunk => collect("stdout", chunk));
    child.stderr.on("data", chunk => collect("stderr", chunk));
    child.on("error", error => { spawnError = error.message; });
    child.on("close", (code, signal) => {
      clearTimeout(timer);
      clearTimeout(escalation);
      const finished = Date.now();
      resolve({
        binary, passed: code === 0 && !timedOut && !outputLimitExceeded
          && spawnError === null && stdout.trim() === expectedOutput,
        pid: child.pid ?? null,
        startedAt: new Date(started).toISOString(),
        finishedAt: new Date(finished).toISOString(),
        code, signal, timedOut, outputLimitExceeded, spawnError,
        terminatedByProbe: stopping, elapsedMs: finished - started,
        stdout, stderr,
      });
    });
  });
}
