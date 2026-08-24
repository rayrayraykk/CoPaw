export interface Connection {
  baseUrl: string;
  token: string;
  username: string;
  agentId: string;
}

export interface AgentSummary {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
  available_in_chat: boolean;
  startup_status: string;
}

export interface ChatSpec {
  id: string;
  session_id: string;
  user_id: string;
  channel: string;
  name?: string;
  updated_at?: string | null;
  status?: "idle" | "running";
}

export interface ContentItem {
  type: string;
  text?: string;
  image_url?: string;
  video_url?: string;
  file_url?: string;
  file_name?: string;
  data?: string;
  [key: string]: unknown;
}

export interface WireMessage {
  role: string;
  content: string | ContentItem[] | Record<string, unknown>;
  [key: string]: unknown;
}

export interface ChatHistory {
  messages: WireMessage[];
  status?: "idle" | "running";
}

export interface DisplayMessage {
  id: string;
  role: "user" | "assistant" | "tool";
  text: string;
  pending?: boolean;
}

export interface UploadResult {
  url: string;
  file_name: string;
  size: number;
}
