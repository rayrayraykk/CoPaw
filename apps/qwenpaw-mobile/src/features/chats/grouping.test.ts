import assert from "node:assert/strict";
import test from "node:test";

import type { ChatGroup, ChatSpec } from "../../api/types";
import { buildChatSections } from "./grouping";

const chats: ChatSpec[] = [
  { id: "a", session_id: "a", user_id: "mobile", channel: "console", group_id: "work" },
  { id: "b", session_id: "b", user_id: "mobile", channel: "console" },
  { id: "c", session_id: "c", user_id: "mobile", channel: "console", group_id: "work" },
];

const groups: ChatGroup[] = [{
  id: "work",
  name: "工作",
  order: 1,
  kind: "custom",
  pinned: false,
}];

test("buildChatSections separates pinned grouped and ungrouped chats", () => {
  const sections = buildChatSections(chats, groups, "c");

  assert.deepEqual(sections.map((section) => section.title), [
    "置顶",
    "工作",
    "未分组",
  ]);
  assert.deepEqual(sections.map((section) => section.data.map((chat) => chat.id)), [
    ["c"],
    ["a"],
    ["b"],
  ]);
});

test("buildChatSections keeps empty groups visible", () => {
  const sections = buildChatSections([], groups, null);

  assert.deepEqual(sections.map((section) => section.title), [
    "工作",
    "未分组",
  ]);
  assert.deepEqual(sections.map((section) => section.data), [[], []]);
});
