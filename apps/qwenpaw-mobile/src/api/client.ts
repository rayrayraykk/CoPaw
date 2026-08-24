import { fetch as expoFetch } from "expo/fetch";

import { eventText, SseParser } from "./sse";
import type {
  AgentSummary,
  ChatHistory,
  ChatSpec,
  Connection,
  ContentItem,
  UploadResult,
} from "./types";

interface PlatformTokens {
  accessToken: string;
  refreshToken: string;
  expiresIn: number;
}

interface JsonObject {
  [key: string]: unknown;
}

export class QwenPawClient {
  constructor(private readonly connection: Connection) {}

  async verify(): Promise<void> {
    await this.request("/auth/verify");
  }

  async listAgents(): Promise<AgentSummary[]> {
    const response = await this.request<{ agents: AgentSummary[] }>("/agents");
    return response.agents.filter(
      (agent) => agent.enabled && agent.available_in_chat,
    );
  }

  async listChats(): Promise<ChatSpec[]> {
    return this.request<ChatSpec[]>("/chats?archived=false");
  }

  async getChat(chatId: string): Promise<ChatHistory> {
    return this.request<ChatHistory>(`/chats/${encodeURIComponent(chatId)}`);
  }

  async createChat(name = "New Chat"): Promise<ChatSpec> {
    const sessionId = `mobile:${createId()}`;
    return this.request<ChatSpec>("/chats", {
      method: "POST",
      body: JSON.stringify({
        name,
        session_id: sessionId,
        user_id: "mobile",
        channel: "console",
      }),
    });
  }

  async deleteChat(chatId: string): Promise<void> {
    await this.request(`/chats/${encodeURIComponent(chatId)}`, {
      method: "DELETE",
    });
  }

  async stopChat(chatId: string): Promise<void> {
    await this.request(`/console/chat/stop?chat_id=${encodeURIComponent(chatId)}`,
      { method: "POST" });
  }

  async upload(
    file: { uri: string; name: string; mimeType?: string | null },
  ): Promise<UploadResult> {
    const form = new FormData();
    form.append("file", {
      uri: file.uri,
      name: file.name,
      type: file.mimeType ?? "application/octet-stream",
    } as unknown as Blob);
    return this.request<UploadResult>("/console/upload", {
      method: "POST",
      body: form,
    });
  }

  async streamChat(options: {
    sessionId: string;
    text: string;
    attachments?: ContentItem[];
    signal: AbortSignal;
    onText: (text: string) => void;
  }): Promise<void> {
    const response = await expoFetch(this.url("/console/chat"), {
      method: "POST",
      headers: this.headers({ "Content-Type": "application/json" }),
      body: JSON.stringify({
        input: [{
          role: "user",
          content: [
            { type: "text", text: options.text },
            ...(options.attachments ?? []),
          ],
        }],
        session_id: options.sessionId,
        user_id: "mobile",
        channel: "console",
        stream: true,
      }),
      signal: options.signal,
    });
    if (!response.ok || !response.body) {
      throw await responseError(response);
    }
    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    const parser = new SseParser();
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      for (const event of parser.push(decoder.decode(value, { stream: true }))) {
        const text = eventText(event);
        if (text) options.onText(text);
      }
    }
    for (const event of parser.finish()) {
      const text = eventText(event);
      if (text) options.onText(text);
    }
  }

  private async request<T = unknown>(
    path: string,
    init: RequestInit = {},
  ): Promise<T> {
    const headers = this.headers(init.headers);
    if (init.body && !(init.body instanceof FormData)) {
      headers.set("Content-Type", "application/json");
    }
    const response = await fetch(this.url(path), { ...init, headers });
    if (!response.ok) throw await responseError(response);
    if (response.status === 204) return undefined as T;
    return response.json() as Promise<T>;
  }

  private headers(extra?: HeadersInit): Headers {
    const headers = new Headers(extra);
    if (this.connection.token) {
      headers.set("Authorization", `Bearer ${this.connection.token}`);
    }
    headers.set("X-Agent-Id", this.connection.agentId || "default");
    return headers;
  }

  private url(path: string): string {
    return `${this.connection.baseUrl}/api${path}`;
  }
}

export async function loginQwenPaw(
  baseUrl: string,
  username: string,
  password: string,
): Promise<Connection> {
  const response = await fetch(`${baseUrl}/api/auth/login`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ username, password, expires_in: 30 * 24 * 3600 }),
  });
  if (!response.ok) throw await responseError(response);
  const data = await response.json() as { token: string; username: string };
  return { baseUrl, token: data.token, username: data.username, agentId: "default" };
}

export async function redeemPairing(
  baseUrl: string,
  ticket: string,
): Promise<Connection> {
  const response = await fetch(`${baseUrl}/api/auth/pairing/redeem`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ ticket }),
  });
  if (!response.ok) throw await responseError(response);
  const data = await response.json() as { token: string; username: string };
  return { baseUrl, token: data.token, username: data.username, agentId: "default" };
}

export async function discoverPlatformQwenPaw(
  account: string,
  password: string,
): Promise<string> {
  const loginResponse = await fetch(
    "https://platform.agentscope.io/api/v1/auth/login",
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ account, password }),
    },
  );
  if (!loginResponse.ok) throw await responseError(loginResponse);
  const loginBody = await loginResponse.json() as JsonObject;
  const tokens = extractPlatformTokens(loginBody);
  const headers = { Authorization: `Bearer ${tokens.accessToken}` };
  const listResponse = await fetch(
    "https://platform.agentscope.io/api/v1/app/list",
    { headers },
  );
  if (!listResponse.ok) throw await responseError(listResponse);
  const listBody = await listResponse.json() as JsonObject;
  const apps = extractApps(listBody);
  if (!apps.length) throw new Error("No QwenPaw deployment was found.");
  const appId = String(apps[0].appId ?? apps[0].id ?? "");
  const statusUrl = "https://platform.agentscope.io/api/v1/app/get" +
    `?appId=${encodeURIComponent(appId)}`;
  let status = await platformJson(statusUrl, headers);
  let state = String(status.status ?? "");
  if (state === "sleeping" || state === "stopped") {
    await platformJson("https://platform.agentscope.io/api/v1/app/start", headers,
      { method: "POST", body: JSON.stringify({ appId }) });
    for (let attempt = 0; attempt < 12; attempt += 1) {
      await new Promise((resolve) => setTimeout(resolve, 2500));
      status = await platformJson(statusUrl, headers);
      state = String(status.status ?? "");
      if (state === "running") break;
    }
  }
  const accessUrl = String(status.accessUrl ?? "").replace(/\/$/, "");
  if (!accessUrl) throw new Error("The QwenPaw deployment is not ready yet.");
  return accessUrl;
}

function extractPlatformTokens(payload: JsonObject): PlatformTokens {
  const first = objectValue(payload.data) ?? payload;
  const data = objectValue(first.data) ?? first;
  const accessToken = String(data.accessToken ?? "");
  if (!accessToken) throw new Error("AgentScope Platform login did not return a token.");
  return {
    accessToken,
    refreshToken: String(data.refreshToken ?? ""),
    expiresIn: Number(data.expiresIn ?? 0),
  };
}

function extractApps(payload: JsonObject): JsonObject[] {
  const first = objectValue(payload.data) ?? payload;
  const data = objectValue(first.data) ?? first;
  const value = data.apps ?? data.list ?? first.apps ?? first.list ?? data;
  return Array.isArray(value)
    ? value.filter((item): item is JsonObject => Boolean(objectValue(item)))
    : [];
}

async function platformJson(
  url: string,
  headers: HeadersInit,
  init: RequestInit = {},
): Promise<JsonObject> {
  const response = await fetch(url, { ...init, headers: { ...headers, ...init.headers } });
  if (!response.ok) throw await responseError(response);
  const payload = await response.json() as JsonObject;
  return objectValue(payload.data) ?? payload;
}

function objectValue(value: unknown): JsonObject | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as JsonObject
    : null;
}

async function responseError(response: Response): Promise<Error> {
  let message = `${response.status} ${response.statusText}`;
  try {
    const body = await response.json() as { detail?: string; message?: string };
    message = body.detail || body.message || message;
  } catch {
    // Keep the HTTP status when the response is not JSON.
  }
  return new Error(message);
}

function createId(): string {
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}
