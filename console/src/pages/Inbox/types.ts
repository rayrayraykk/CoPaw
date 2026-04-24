export interface InboxSummary {
  approvals: {
    total: number;
    urgent: number;
  };
  pushMessages: {
    total: number;
    unread: number;
  };
  harvests: {
    total: number;
    active: number;
  };
}

export interface PushMessage {
  id: string;
  channelType: "wechat" | "slack" | "telegram" | "discord" | "email";
  channelName: string;
  title: string;
  content: string;
  sender: {
    userId: string;
    username: string;
  };
  createdAt: Date;
  read: boolean;
  metadata?: {
    messageId?: string;
    conversationId?: string;
    priority?: "low" | "normal" | "high" | "urgent";
  };
}

export interface HarvestInstance {
  id: string;
  name: string;
  templateId: string;
  emoji: string;
  schedule: {
    cron: string;
    timezone: string;
    nextRun: Date;
  };
  status: "active" | "paused" | "error";
  lastGenerated?: {
    timestamp: Date;
    success: boolean;
    previewUrl?: string;
  };
  stats: {
    totalGenerated: number;
    successRate: number;
    consecutiveDays: number;
  };
  theme: HarvestTheme;
}

export interface HarvestContent {
  id: string;
  harvestId: string;
  generatedAt: Date;
  title: string;
  subtitle?: string;
  coverImage?: string;
  sections: HarvestSection[];
  theme: HarvestTheme;
  metadata: {
    estimatedReadTime: number;
    articleCount: number;
    keywords: string[];
  };
}

export interface HarvestSection {
  id: string;
  type: "hero" | "article" | "quote" | "list" | "chart" | "divider";
  title?: string;
  content: string;
  sources?: Array<{
    title: string;
    url: string;
    publishedAt?: Date;
  }>;
  tldr?: string;
}

export type HarvestTheme =
  | "apple"
  | "deepblue"
  | "autumn"
  | "minimal"
  | "magazine"
  | "newspaper";

export interface HarvestThemeConfig {
  id: HarvestTheme;
  name: string;
  description: string;
  colors: {
    primary: string;
    secondary: string;
    background: string;
    text: string;
    accent: string;
  };
  fonts: {
    heading: string;
    body: string;
  };
  preview: string;
}

export interface HarvestTemplate {
  id: string;
  name: string;
  emoji: string;
  description: string;
  estimatedReadTime: number;
  defaultSchedule: {
    cron: string;
    timezone: string;
  };
  searchConfig: {
    keywords: string[];
    sources: string[];
    language: string;
    maxResults: number;
  };
  defaultTheme: HarvestTheme;
}

export interface ApprovalItem {
  id: string;
  type: "tool_call" | "config_change" | "file_access";
  title: string;
  description: string;
  requestedBy: string;
  requestedAt: Date;
  priority: "low" | "normal" | "high" | "urgent";
  status: "pending" | "approved" | "rejected";
  metadata?: Record<string, unknown>;
}
