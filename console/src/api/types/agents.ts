// Multi-agent management types

import type { ModelSlotConfig } from "./provider";

export type AgentType = "native" | "external_acp";

export interface AgentSummary {
  id: string;
  name: string;
  description: string;
  workspace_dir: string;
  enabled: boolean;
  active_model?: ModelSlotConfig | null;
  type?: AgentType;
}

export interface AgentListResponse {
  agents: AgentSummary[];
}

export interface ReorderAgentsResponse {
  success: boolean;
  agent_ids: string[];
}

export interface AgentProfileConfig {
  id: string;
  name: string;
  description?: string;
  workspace_dir?: string;
  approval_level?: string;
  active_model?: ModelSlotConfig | null;
  channels?: unknown;
  mcp?: unknown;
  heartbeat?: unknown;
  running?: unknown;
  llm_routing?: unknown;
  system_prompt_files?: string[];
  tools?: unknown;
  security?: unknown;
}

export interface CreateAgentRequest {
  id?: string;
  name: string;
  description?: string;
  workspace_dir?: string;
  language?: string;
  skill_names?: string[];
  active_model?: ModelSlotConfig | null;
  type?: AgentType;
  acp_agent_id?: string;
}

export interface ACPAgentOption {
  id: string;
  command: string;
  enabled: boolean;
}

export interface AgentProfileRef {
  id: string;
  workspace_dir: string;
}
