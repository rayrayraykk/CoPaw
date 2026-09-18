// DevTools transport for the isolated original-Console acceptance browser.
class DevToolsClosedError extends Error {
  constructor() { super("DevTools connection close"); }
}

export class DevToolsClient {
  constructor(socket) {
    this.socket = socket;
    this.nextId = 1;
    this.pending = new Map();
    this.listeners = new Map();
    this.closedError = null;
    socket.addEventListener("close", () => this.fail(new DevToolsClosedError()));
    socket.addEventListener("error", () => this.fail(new Error("DevTools connection error")));
    socket.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      if (message.id) {
        const pending = this.pending.get(message.id);
        if (!pending) return;
        this.pending.delete(message.id);
        if (message.error) pending.reject(new Error(message.error.message));
        else pending.resolve(message.result);
        return;
      }
      for (const listener of this.listeners.get(message.method) ?? []) {
        listener(message.params);
      }
    });
  }

  on(method, listener) {
    const listeners = this.listeners.get(method) ?? [];
    listeners.push(listener);
    this.listeners.set(method, listeners);
  }

  send(method, params = {}) {
    if (this.closedError) return Promise.reject(this.closedError);
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      try {
        this.socket.send(JSON.stringify({ id, method, params }));
      } catch (error) {
        this.pending.delete(id);
        reject(error);
      }
    });
  }

  fail(error) {
    if (this.closedError) return;
    this.closedError = error;
    for (const pending of this.pending.values()) pending.reject(error);
    this.pending.clear();
  }

  close() {
    this.fail(new DevToolsClosedError());
    this.socket.close();
  }
}

export async function closeBrowser(client, child) {
  try {
    await client.send("Browser.close");
  } catch (error) {
    // Chrome can close the socket before acknowledging Browser.close. Only
    // an observed clean process exit makes that specific outcome successful.
    if (!(error instanceof DevToolsClosedError)) throw error;
  }
  // A protocol acknowledgement alone does not prove that Chrome terminated.
  await new Promise((resolve, reject) => {
    let timer;
    const cleanup = () => {
      clearTimeout(timer);
      child.removeListener("exit", exited);
    };
    const exited = (code, signal) => {
      cleanup();
      if (code === 0 && !signal) resolve();
      else reject(new Error(`Browser exited abnormally (${code}, ${signal})`));
    };
    if (child.exitCode !== null || child.signalCode !== null) {
      exited(child.exitCode, child.signalCode);
      return;
    }
    child.once("exit", exited);
    timer = setTimeout(() => {
      cleanup();
      reject(new Error("Browser did not exit after close request"));
    }, 2_000);
  });
}
