import type { ChatGroup, ChatSpec } from "../../api/types";

export interface ChatSection {
  key: string;
  title: string;
  data: ChatSpec[];
  pinned?: boolean;
  group?: ChatGroup;
}

export function buildChatSections(
  chats: ChatSpec[],
  groups: ChatGroup[],
  pinnedChatId: string | null,
): ChatSection[] {
  const pinned = pinnedChatId
    ? chats.filter((chat) => chat.id === pinnedChatId)
    : [];
  const remaining = chats.filter((chat) => chat.id !== pinnedChatId);
  const sections: ChatSection[] = pinned.length
    ? [{ key: "pinned", title: "置顶", data: pinned, pinned: true }]
    : [];

  const orderedGroups = [...groups].sort((left, right) => left.order - right.order);
  for (const group of orderedGroups) {
    const data = remaining.filter((chat) => group.kind === "default"
      ? !chat.group_id || chat.group_id === group.id
      : chat.group_id === group.id);
    sections.push({
      key: group.id,
      title: groupTitle(group),
      data,
      group,
    });
  }
  const knownIds = new Set(groups.map((group) => group.id));
  const hasDefaultGroup = groups.some((group) => group.kind === "default");
  const ungrouped = remaining.filter(
    (chat) => (!chat.group_id && !hasDefaultGroup) ||
      Boolean(chat.group_id && !knownIds.has(chat.group_id)),
  );
  if (ungrouped.length || !hasDefaultGroup) {
    sections.push({ key: "ungrouped", title: "未分组", data: ungrouped });
  }
  return sections;
}

function groupTitle(group: ChatGroup): string {
  if (group.kind === "default") return "未分组";
  if (group.kind === "cron") return "定时任务";
  if (group.kind === "subagents") return "子智能体";
  return group.name;
}
