// Observation only: never send commands, signal children, or decide test success.
export function attachShutdownDiagnostics(child) {
  const started = performance.now();
  const startedAt = new Date().toISOString();
  const events = [];
  const detach = [];
  let dropped = 0;
  const record = (event, details = {}) => {
    if (events.length === 64) { dropped += 1; return; }
    events.push({ event, elapsedMs: performance.now() - started, ...details });
  };
  const listen = (name, listener) => {
    child.on(name, listener);
    detach.push(() => child.removeListener(name, listener));
  };
  record("process-observed", { code: child.exitCode, signal: child.signalCode });
  listen("exit", (code, signal) => record("process-exit", { code, signal }));
  listen("close", (code, signal) => record("stdio-close", { code, signal }));
  listen("error", () => record("process-error"));
  return {
    mark: event => record(event),
    watchClose(client) {
      const id = client.nextId;
      let responded = false;
      record("close-request");
      const listeners = {
        message(event) {
          let message;
          try { message = JSON.parse(event.data); } catch { return; }
          if (!message || message.id !== id || responded) return;
          responded = true;
          record(message.error ? "close-protocol-error" : "close-response");
        },
        close: () => record("socket-close"),
        error: () => record("socket-error"),
      };
      for (const [name, listener] of Object.entries(listeners)) {
        client.socket.addEventListener(name, listener);
        detach.push(() => client.socket.removeEventListener(name, listener));
      }
    },
    snapshot: () => ({ startedAt, pid: child.pid, events: events.map(e => ({ ...e })), dropped }),
    dispose() { for (const remove of detach.splice(0)) remove(); },
  };
}
