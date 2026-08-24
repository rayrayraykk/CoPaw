import { create } from "zustand";

import { QwenPawClient } from "../api/client";
import type {
  AgentSummary,
  ChatSpec,
  Connection,
  ContentItem,
  DisplayMessage,
  WireMessage,
} from "../api/types";
import {
  clearConnection,
  loadConnection,
  saveConnection,
} from "../storage/connection";

interface AppState {
  status: "booting" | "disconnected" | "connecting" | "ready";
  connection: Connection | null;
  agents: AgentSummary[];
  chats: ChatSpec[];
  messages: Record<string, DisplayMessage[]>;
  activeAbort: AbortController | null;
  error: string | null;
  bootstrap: () => Promise<void>;
  connect: (connection: Connection) => Promise<void>;
  disconnect: () => Promise<void>;
  selectAgent: (agentId: string) => Promise<void>;
  refreshChats: () => Promise<void>;
  createChat: () => Promise<ChatSpec>;
  deleteChat: (chatId: string) => Promise<void>;
  loadChat: (chatId: string) => Promise<void>;
  send: (
    chat: ChatSpec,
    text: string,
    attachments?: ContentItem[],
  ) => Promise<void>;
  stop: (chatId: string) => Promise<void>;
  clearError: () => void;
}

export const useAppStore = create<AppState>((set, get) => ({
  status: "booting",
  connection: null,
  agents: [],
  chats: [],
  messages: {},
  activeAbort: null,
  error: null,

  bootstrap: async () => {
    const connection = await loadConnection();
    if (!connection) {
      set({ status: "disconnected" });
      return;
    }
    try {
      await get().connect(connection);
    } catch {
      await clearConnection();
      set({ status: "disconnected", connection: null });
    }
  },

  connect: async (connection) => {
    set({ status: "connecting", error: null });
    try {
      const client = new QwenPawClient(connection);
      await client.verify();
      const agents = await client.listAgents();
      const selected = agents.some((agent) => agent.id === connection.agentId)
        ? connection.agentId
        : agents[0]?.id ?? "default";
      const resolved = { ...connection, agentId: selected };
      await saveConnection(resolved);
      const resolvedClient = new QwenPawClient(resolved);
      const chats = await resolvedClient.listChats();
      set({
        status: "ready",
        connection: resolved,
        agents,
        chats,
        error: null,
      });
    } catch (error) {
      set({ status: "disconnected", error: errorMessage(error) });
      throw error;
    }
  },

  disconnect: async () => {
    get().activeAbort?.abort();
    await clearConnection();
    set({
      status: "disconnected",
      connection: null,
      agents: [],
      chats: [],
      messages: {},
      activeAbort: null,
    });
  },

  selectAgent: async (agentId) => {
    const current = requireConnection(get().connection);
    const connection = { ...current, agentId };
    await saveConnection(connection);
    const chats = await new QwenPawClient(connection).listChats();
    set({ connection, chats, messages: {} });
  },

  refreshChats: async () => {
    const client = new QwenPawClient(requireConnection(get().connection));
    set({ chats: await client.listChats() });
  },

  createChat: async () => {
    const client = new QwenPawClient(requireConnection(get().connection));
    const chat = await client.createChat();
    set((state) => ({ chats: [chat, ...state.chats] }));
    return chat;
  },

  deleteChat: async (chatId) => {
    const client = new QwenPawClient(requireConnection(get().connection));
    await client.deleteChat(chatId);
    set((state) => ({
      chats: state.chats.filter((chat) => chat.id !== chatId),
    }));
  },

  loadChat: async (chatId) => {
    const client = new QwenPawClient(requireConnection(get().connection));
    const history = await client.getChat(chatId);
    set((state) => ({
      messages: {
        ...state.messages,
        [chatId]: history.messages.map(toDisplayMessage),
      },
    }));
  },

  send: async (chat, text, attachments = []) => {
    const client = new QwenPawClient(requireConnection(get().connection));
    const controller = new AbortController();
    const userMessage: DisplayMessage = {
      id: createId(),
      role: "user",
      text,
    };
    const responseId = createId();
    const response: DisplayMessage = {
      id: responseId,
      role: "assistant",
      text: "",
      pending: true,
    };
    set((state) => ({
      activeAbort: controller,
      messages: {
        ...state.messages,
        [chat.id]: [
          ...(state.messages[chat.id] ?? []),
          userMessage,
          response,
        ],
      },
    }));
    try {
      await client.streamChat({
        sessionId: chat.session_id,
        text,
        attachments,
        signal: controller.signal,
        onText: (delta) => {
          set((state) => ({
            messages: {
              ...state.messages,
              [chat.id]: (state.messages[chat.id] ?? []).map((message) =>
                message.id === responseId
                  ? { ...message, text: `${message.text}${delta}` }
                  : message,
              ),
            },
          }));
        },
      });
    } catch (error) {
      if (!controller.signal.aborted) set({ error: errorMessage(error) });
    } finally {
      set((state) => ({
        activeAbort: null,
        messages: {
          ...state.messages,
          [chat.id]: (state.messages[chat.id] ?? []).map((message) =>
            message.id === responseId
              ? { ...message, pending: false }
              : message,
          ),
        },
      }));
      await get().refreshChats().catch(() => undefined);
    }
  },

  stop: async (chatId) => {
    get().activeAbort?.abort();
    const client = new QwenPawClient(requireConnection(get().connection));
    await client.stopChat(chatId).catch(() => undefined);
    set({ activeAbort: null });
  },

  clearError: () => set({ error: null }),
}));

function requireConnection(connection: Connection | null): Connection {
  if (!connection) throw new Error("QwenPaw is not connected.");
  return connection;
}

function toDisplayMessage(message: WireMessage, index: number): DisplayMessage {
  const role = message.role === "user"
    ? "user"
    : message.role === "tool"
      ? "tool"
      : "assistant";
  return {
    id: String(message.id ?? `${role}-${index}`),
    role,
    text: contentText(message.content),
  };
}

function contentText(content: WireMessage["content"]): string {
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    return content.map((item) => {
      if (typeof item.text === "string") return item.text;
      if (typeof item.name === "string") return `[${item.name}]`;
      return "";
    }).filter(Boolean).join("\n");
  }
  if (content && typeof content === "object") {
    const text = (content as { text?: unknown }).text;
    return typeof text === "string" ? text : JSON.stringify(content, null, 2);
  }
  return "";
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Something went wrong.";
}

function createId(): string {
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}
