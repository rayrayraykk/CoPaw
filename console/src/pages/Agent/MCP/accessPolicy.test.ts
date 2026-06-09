import { describe, expect, it } from "vitest";
import type { MCPAccessPolicy, MCPToolInfo } from "../../../api/types";
import {
  addToolRule,
  buildMCPAccessToolGroups,
  removeToolRule,
  upsertToolRule,
} from "./accessPolicy";

const tools: MCPToolInfo[] = [
  {
    name: "echo",
    description: "Echo text",
    input_schema: { type: "object" },
  },
  {
    name: "search",
    description: "Search",
    input_schema: {},
  },
];

const consoleEchoRule = {
  tool_name: "echo",
  source_type: "channel" as const,
  source_value: "console",
  subject_type: "all" as const,
  subject_value: "",
  effect: "allow" as const,
};

const appSearchRule = {
  tool_name: "search",
  source_type: "app" as const,
  source_value: "Creator",
  subject_type: "all" as const,
  subject_value: "",
  effect: "deny" as const,
};

const policy: MCPAccessPolicy = {
  default_effect: "ask",
  tool_overrides: [
    consoleEchoRule,
    {
      ...consoleEchoRule,
      tool_name: "old_tool",
      effect: "deny",
    },
    appSearchRule,
  ],
  unmanaged_rules_count: 1,
};

describe("MCP access policy helpers", () => {
  it("groups current tools and stale saved rules by tool", () => {
    const groups = buildMCPAccessToolGroups(tools, policy);

    expect(groups).toEqual([
      expect.objectContaining({
        toolName: "echo",
        description: "Echo text",
        stale: false,
        rules: [consoleEchoRule],
      }),
      expect.objectContaining({
        toolName: "search",
        stale: false,
        rules: [appSearchRule],
      }),
      expect.objectContaining({
        toolName: "old_tool",
        stale: true,
        rules: [
          {
            ...consoleEchoRule,
            tool_name: "old_tool",
            effect: "deny",
          },
        ],
      }),
    ]);
  });

  it("adds a default console source rule under the selected tool", () => {
    const next = addToolRule(policy, "search");

    expect(next.tool_overrides).toContainEqual({
      tool_name: "search",
      source_type: "channel",
      source_value: "console",
      subject_type: "all",
      subject_value: "",
      effect: "ask",
    });
  });

  it("updates a rule selector or effect without duplicating the same tool rule", () => {
    const renamed = upsertToolRule(
      policy,
      {
        ...appSearchRule,
        source_type: "channel",
        source_value: "dingtalk",
        subject_type: "user",
        subject_value: "alice",
        effect: "allow",
      },
      appSearchRule,
    );

    expect(renamed.tool_overrides).not.toContainEqual(appSearchRule);
    expect(renamed.tool_overrides).toContainEqual({
      tool_name: "search",
      source_type: "channel",
      source_value: "dingtalk",
      subject_type: "user",
      subject_value: "alice",
      effect: "allow",
    });

    const changedEffect = upsertToolRule(renamed, {
      tool_name: "search",
      source_type: "channel",
      source_value: "dingtalk",
      subject_type: "user",
      subject_value: "alice",
      effect: "deny",
    });
    expect(
      changedEffect.tool_overrides.filter(
        (item) =>
          item.tool_name === "search" &&
          item.source_type === "channel" &&
          item.source_value === "dingtalk" &&
          item.subject_type === "user" &&
          item.subject_value === "alice",
      ),
    ).toEqual([
      {
        tool_name: "search",
        source_type: "channel",
        source_value: "dingtalk",
        subject_type: "user",
        subject_value: "alice",
        effect: "deny",
      },
    ]);
  });

  it("removes one structured rule from a tool", () => {
    const next = removeToolRule(policy, consoleEchoRule);

    expect(next.tool_overrides).not.toContainEqual(consoleEchoRule);
    expect(next.tool_overrides).toContainEqual(appSearchRule);
  });
});
