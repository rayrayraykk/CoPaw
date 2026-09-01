#!/usr/bin/env node

import { access, mkdtemp, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";

const WAIT_AFTER_LOAD_MS = 8_000;
const START_TIMEOUT_MS = 15_000;

function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

async function waitForExit(child, timeoutMs = 5_000) {
  if (child.exitCode !== null) return;
  await Promise.race([
    new Promise((resolve) => child.once("exit", resolve)),
    delay(timeoutMs),
  ]);
}

async function firstExecutable(candidates) {
  for (const candidate of candidates) {
    if (!candidate) continue;
    try {
      await access(candidate);
      return candidate;
    } catch {
      // Try the next platform path.
    }
  }
  throw new Error(
    "Chrome was not found; set QWENPAW_CHROME to a Chrome/Chromium executable",
  );
}

async function chromeExecutable() {
  const configured = process.env.QWENPAW_CHROME;
  if (process.platform === "darwin") {
    return firstExecutable([
      configured,
      "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
      "/Applications/Chromium.app/Contents/MacOS/Chromium",
    ]);
  }
  if (process.platform === "win32") {
    return firstExecutable([
      configured,
      process.env.PROGRAMFILES &&
        path.join(process.env.PROGRAMFILES, "Google/Chrome/Application/chrome.exe"),
      process.env["PROGRAMFILES(X86)"] &&
        path.join(
          process.env["PROGRAMFILES(X86)"],
          "Google/Chrome/Application/chrome.exe",
        ),
    ]);
  }
  return firstExecutable([
    configured,
    "/usr/bin/google-chrome",
    "/usr/bin/google-chrome-stable",
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
  ]);
}

function validateBaseUrl(value) {
  const parsed = new URL(value);
  const loopback =
    parsed.hostname === "localhost" ||
    parsed.hostname === "127.0.0.1" ||
    parsed.hostname === "[::1]";
  if (parsed.protocol !== "http:" || !loopback) {
    throw new Error("Console smoke URL must be a loopback HTTP origin");
  }
  return parsed.origin;
}

function waitForDevTools(child) {
  return new Promise((resolve, reject) => {
    let stderr = "";
    const timer = setTimeout(() => {
      reject(new Error(`Chrome DevTools did not start: ${stderr.slice(-2_000)}`));
    }, START_TIMEOUT_MS);
    child.stderr.on("data", (chunk) => {
      stderr += chunk.toString();
      const match = stderr.match(/DevTools listening on (ws:\/\/[^\s]+)/);
      if (match) {
        clearTimeout(timer);
        resolve(match[1]);
      }
    });
    child.once("exit", (code) => {
      clearTimeout(timer);
      reject(new Error(`Chrome exited before DevTools was ready (${code})`));
    });
  });
}

class DevToolsClient {
  constructor(socket) {
    this.socket = socket;
    this.nextId = 1;
    this.pending = new Map();
    this.listeners = new Map();
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
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }
}

async function connectDevTools(webSocketUrl) {
  const socket = new WebSocket(webSocketUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });
  return new DevToolsClient(socket);
}

async function createPage(browserWebSocketUrl) {
  const browserUrl = new URL(browserWebSocketUrl);
  const response = await fetch(
    `http://${browserUrl.host}/json/new?${encodeURIComponent("about:blank")}`,
    { method: "PUT" },
  );
  if (!response.ok) {
    throw new Error(`Chrome target creation failed with ${response.status}`);
  }
  return response.json();
}

function apiPath(url, origin) {
  const parsed = new URL(url);
  if (parsed.origin !== origin || !parsed.pathname.startsWith("/api/")) {
    return null;
  }
  return `${parsed.pathname}${parsed.search}`;
}

async function inspectConsole(client, origin) {
  const apiResponses = new Map();
  const requestFailures = [];
  const browserErrors = [];
  client.on("Network.responseReceived", ({ response }) => {
    const requestPath = apiPath(response.url, origin);
    if (requestPath) apiResponses.set(requestPath, response.status);
  });
  client.on("Network.loadingFailed", ({ errorText, type }) => {
    if (type !== "EventSource") requestFailures.push(`${type}: ${errorText}`);
  });
  client.on("Runtime.exceptionThrown", ({ exceptionDetails }) => {
    browserErrors.push(
      exceptionDetails.exception?.description ?? exceptionDetails.text,
    );
  });
  client.on("Runtime.consoleAPICalled", ({ type, args }) => {
    if (type !== "error") return;
    browserErrors.push(
      args.map((argument) => argument.value ?? argument.description).join(" "),
    );
  });
  await Promise.all([
    client.send("Network.enable"),
    client.send("Page.enable"),
    client.send("Runtime.enable"),
  ]);
  await client.send("Page.navigate", { url: `${origin}/` });
  await delay(WAIT_AFTER_LOAD_MS);
  const evaluation = await client.send("Runtime.evaluate", {
    expression: `JSON.stringify({
      bodyText: document.body?.innerText ?? "",
      rootChildren: document.querySelector("#root")?.childElementCount ?? 0,
      hasComposer: Boolean(
        document.querySelector("textarea") ||
        document.querySelector('[contenteditable="true"]')
      ),
      loadingError: Boolean(
        [...document.querySelectorAll("body *")].some((element) =>
          /backend detection failed|authentication failed|retry/i.test(
            element.textContent ?? ""
          )
        )
      )
    })`,
    returnByValue: true,
  });
  const page = JSON.parse(evaluation.result.value);
  const responses = [...apiResponses]
    .map(([requestPath, status]) => ({ path: requestPath, status }))
    .sort((left, right) => left.path.localeCompare(right.path));
  const missing = responses.filter(({ status }) => status === 404);
  const serverErrors = responses.filter(({ status }) => status >= 500);
  const failures = [];
  if (page.rootChildren === 0) failures.push("#root did not render");
  if (!page.hasComposer) failures.push("chat composer did not render");
  if (page.loadingError) failures.push("backend loading/error page rendered");
  failures.push(...requestFailures, ...browserErrors, ...serverErrors.map(
    ({ path: requestPath, status }) => `${requestPath} returned ${status}`,
  ));
  return {
    ok: failures.length === 0,
    page: {
      rootChildren: page.rootChildren,
      hasComposer: page.hasComposer,
      textSample: page.bodyText.slice(0, 300),
    },
    apiResponses: responses,
    missingApi: missing,
    failures,
  };
}

async function main() {
  const origin = validateBaseUrl(process.argv[2] ?? "");
  const executable = await chromeExecutable();
  const profile = await mkdtemp(path.join(os.tmpdir(), "qwenpaw-chrome-smoke-"));
  const child = spawn(
    executable,
    [
      "--headless=new",
      "--disable-gpu",
      "--no-first-run",
      "--no-default-browser-check",
      "--no-sandbox",
      "--remote-debugging-port=0",
      `--user-data-dir=${profile}`,
      "about:blank",
    ],
    { stdio: ["ignore", "ignore", "pipe"] },
  );
  try {
    const browserWebSocketUrl = await waitForDevTools(child);
    const target = await createPage(browserWebSocketUrl);
    const client = await connectDevTools(target.webSocketDebuggerUrl);
    const result = await inspectConsole(client, origin);
    process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
    const browser = await connectDevTools(browserWebSocketUrl);
    await browser.send("Browser.close");
    if (!result.ok) process.exitCode = 1;
  } finally {
    if (child.exitCode === null) child.kill("SIGTERM");
    await waitForExit(child);
    await rm(profile, {
      recursive: true,
      force: true,
      maxRetries: 10,
      retryDelay: 200,
    });
  }
}

main().catch((error) => {
  process.stderr.write(`${error.stack ?? error}\n`);
  process.exitCode = 1;
});
