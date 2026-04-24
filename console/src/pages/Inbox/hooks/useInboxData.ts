import { useState } from "react";
import type {
  InboxSummary,
  PushMessage,
  HarvestInstance,
  ApprovalItem,
} from "../types";

const MOCK_APPROVALS: ApprovalItem[] = [
  {
    id: "approval-1",
    type: "tool_call",
    title: "Execute Shell Command",
    description: "Agent wants to run: npm install @testing-library/react",
    requestedBy: "Agent-001",
    requestedAt: new Date(Date.now() - 5 * 60 * 1000),
    priority: "normal",
    status: "pending",
  },
  {
    id: "approval-2",
    type: "file_access",
    title: "File Access Request",
    description: "Agent wants to read: /etc/config/database.yml",
    requestedBy: "Agent-002",
    requestedAt: new Date(Date.now() - 15 * 60 * 1000),
    priority: "high",
    status: "pending",
  },
  {
    id: "approval-3",
    type: "config_change",
    title: "Configuration Change",
    description: "Agent wants to modify API rate limit settings",
    requestedBy: "Agent-001",
    requestedAt: new Date(Date.now() - 30 * 60 * 1000),
    priority: "urgent",
    status: "pending",
  },
];

const MOCK_PUSH_MESSAGES: PushMessage[] = [
  {
    id: "msg-1",
    channelType: "wechat",
    channelName: "CoPaw-WeChat",
    title: "AI Analysis Complete",
    content:
      "Your tech trends analysis has been generated. Check out the latest insights on GPT-5, quantum computing, and more.",
    sender: {
      userId: "user-001",
      username: "Zhang San",
    },
    createdAt: new Date(Date.now() - 2 * 60 * 60 * 1000),
    read: false,
    metadata: {
      priority: "high",
    },
  },
  {
    id: "msg-2",
    channelType: "slack",
    channelName: "CoPaw-Slack",
    title: "Weekly Report Ready",
    content:
      "Your weekly industry insights report is ready for review. 5 key trends identified this week.",
    sender: {
      userId: "user-002",
      username: "Li Si",
    },
    createdAt: new Date(Date.now() - 5 * 60 * 60 * 1000),
    read: true,
  },
  {
    id: "msg-3",
    channelType: "telegram",
    channelName: "CoPaw-Telegram",
    title: "Competitor Update",
    content:
      "Major competitor announcement detected: Company X raised $100M Series C funding.",
    sender: {
      userId: "user-003",
      username: "Wang Wu",
    },
    createdAt: new Date(Date.now() - 8 * 60 * 60 * 1000),
    read: false,
    metadata: {
      priority: "urgent",
    },
  },
  {
    id: "msg-4",
    channelType: "discord",
    channelName: "CoPaw-Discord",
    title: "Paper Summary Available",
    content: "New ArXiv papers summarized: 3 breakthrough papers in NLP.",
    sender: {
      userId: "user-004",
      username: "Zhao Liu",
    },
    createdAt: new Date(Date.now() - 12 * 60 * 60 * 1000),
    read: true,
  },
];

const MOCK_HARVESTS: HarvestInstance[] = [
  {
    id: "harvest-1",
    name: "Tech Frontier Harvest",
    templateId: "tech-daily",
    emoji: "🚀",
    schedule: {
      cron: "0 9 * * *",
      timezone: "Asia/Shanghai",
      nextRun: new Date(Date.now() + 5 * 60 * 60 * 1000 + 23 * 60 * 1000),
    },
    status: "active",
    lastGenerated: {
      timestamp: new Date(Date.now() - 2 * 60 * 60 * 1000),
      success: true,
      previewUrl: "/mock/harvest-1",
    },
    stats: {
      totalGenerated: 23,
      successRate: 95.6,
      consecutiveDays: 7,
    },
    theme: "apple",
  },
  {
    id: "harvest-2",
    name: "Industry Intelligence",
    templateId: "industry-weekly",
    emoji: "📊",
    schedule: {
      cron: "0 10 * * 1",
      timezone: "Asia/Shanghai",
      nextRun: new Date(Date.now() + 2 * 24 * 60 * 60 * 1000),
    },
    status: "active",
    lastGenerated: {
      timestamp: new Date(Date.now() - 3 * 24 * 60 * 60 * 1000),
      success: true,
    },
    stats: {
      totalGenerated: 12,
      successRate: 100,
      consecutiveDays: 12,
    },
    theme: "deepblue",
  },
  {
    id: "harvest-3",
    name: "Competitor Watch",
    templateId: "competitor-daily",
    emoji: "🏢",
    schedule: {
      cron: "0 18 * * *",
      timezone: "Asia/Shanghai",
      nextRun: new Date(Date.now() + 8 * 60 * 60 * 1000),
    },
    status: "active",
    lastGenerated: {
      timestamp: new Date(Date.now() - 10 * 60 * 60 * 1000),
      success: true,
    },
    stats: {
      totalGenerated: 45,
      successRate: 97.8,
      consecutiveDays: 15,
    },
    theme: "minimal",
  },
  {
    id: "harvest-4",
    name: "Academic Frontier",
    templateId: "academic-weekly",
    emoji: "🎓",
    schedule: {
      cron: "0 11 * * 3",
      timezone: "Asia/Shanghai",
      nextRun: new Date(Date.now() + 4 * 24 * 60 * 60 * 1000),
    },
    status: "active",
    lastGenerated: {
      timestamp: new Date(Date.now() - 6 * 24 * 60 * 60 * 1000),
      success: true,
    },
    stats: {
      totalGenerated: 8,
      successRate: 100,
      consecutiveDays: 8,
    },
    theme: "autumn",
  },
  {
    id: "harvest-5",
    name: "Investment Opportunities",
    templateId: "investment-weekly",
    emoji: "💼",
    schedule: {
      cron: "0 14 * * 5",
      timezone: "Asia/Shanghai",
      nextRun: new Date(Date.now() + 3 * 24 * 60 * 60 * 1000),
    },
    status: "paused",
    lastGenerated: {
      timestamp: new Date(Date.now() - 14 * 24 * 60 * 60 * 1000),
      success: false,
    },
    stats: {
      totalGenerated: 5,
      successRate: 80,
      consecutiveDays: 0,
    },
    theme: "magazine",
  },
];

export const useInboxData = () => {
  const [summary] = useState<InboxSummary>({
    approvals: {
      total: 3,
      urgent: 1,
    },
    pushMessages: {
      total: 12,
      unread: 5,
    },
    harvests: {
      total: 5,
      active: 4,
    },
  });

  const [approvals] = useState<ApprovalItem[]>(MOCK_APPROVALS);
  const [pushMessages] = useState<PushMessage[]>(MOCK_PUSH_MESSAGES);
  const [harvests] = useState<HarvestInstance[]>(MOCK_HARVESTS);

  const markMessageAsRead = (messageId: string) => {
    // Mock implementation
    console.log("Marking message as read:", messageId);
  };

  const approveRequest = (approvalId: string) => {
    // Mock implementation
    console.log("Approving request:", approvalId);
  };

  const rejectRequest = (approvalId: string) => {
    // Mock implementation
    console.log("Rejecting request:", approvalId);
  };

  const triggerHarvest = (harvestId: string) => {
    // Mock implementation
    console.log("Triggering harvest:", harvestId);
  };

  return {
    summary,
    approvals,
    pushMessages,
    harvests,
    markMessageAsRead,
    approveRequest,
    rejectRequest,
    triggerHarvest,
  };
};
