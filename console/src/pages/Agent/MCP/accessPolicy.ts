import type {
  MCPAccessPolicy,
  MCPAccessSourceType,
  MCPAccessSubjectType,
  MCPToolAccessOverride,
  MCPToolInfo,
} from "../../../api/types";

export interface MCPAccessToolGroup {
  toolName: string;
  description: string;
  inputSchema: Record<string, unknown>;
  stale: boolean;
  rules: MCPToolAccessOverride[];
}

type RuleIdentity = Pick<
  MCPToolAccessOverride,
  | "tool_name"
  | "source_type"
  | "source_value"
  | "subject_type"
  | "subject_value"
>;

const DEFAULT_CHANNEL_SOURCE = "console";
const DEFAULT_APP_SOURCE = "Creator";

export const MCP_CHANNEL_SOURCE_VALUES = [
  "console",
  "dingtalk",
  "feishu",
  "wechat",
  "wecom",
  "discord",
  "telegram",
  "qq",
  "imessage",
  "mattermost",
  "matrix",
  "onebot",
  "mqtt",
  "voice",
  "sip",
  "xiaoyi",
] as const;

export const MCP_APP_SOURCE_VALUES = ["Creator", "Insight"] as const;

export function normalizeMCPToolRule(
  rule: MCPToolAccessOverride,
): MCPToolAccessOverride {
  const sourceType = normalizeSourceType(rule.source_type);
  const subjectType = normalizeSubjectType(rule.subject_type);
  return {
    tool_name: rule.tool_name || "*",
    source_type: sourceType,
    source_value: normalizeSourceValue(sourceType, rule.source_value),
    subject_type: subjectType,
    subject_value:
      subjectType === "all" ? "" : (rule.subject_value || "").trim(),
    effect: rule.effect,
  };
}

export function buildMCPAccessToolGroups(
  tools: MCPToolInfo[],
  policy: MCPAccessPolicy,
): MCPAccessToolGroup[] {
  const rulesByTool = new Map<string, MCPToolAccessOverride[]>();
  policy.tool_overrides.forEach((override) => {
    const rule = normalizeMCPToolRule(override);
    const rules = rulesByTool.get(rule.tool_name) || [];
    rules.push(rule);
    rulesByTool.set(rule.tool_name, rules);
  });

  const currentToolNames = new Set(tools.map((tool) => tool.name));
  const currentGroups: MCPAccessToolGroup[] = tools.map((tool) => ({
    toolName: tool.name,
    description: tool.description,
    inputSchema: tool.input_schema,
    stale: false,
    rules: sortToolRules(rulesByTool.get(tool.name) || []),
  }));

  const staleGroups: MCPAccessToolGroup[] = Array.from(rulesByTool.entries())
    .filter(([toolName]) => toolName !== "*" && !currentToolNames.has(toolName))
    .map(([toolName, rules]) => ({
      toolName,
      description: "",
      inputSchema: {},
      stale: true,
      rules: sortToolRules(rules),
    }));

  return [...currentGroups, ...staleGroups];
}

export function addToolRule(
  policy: MCPAccessPolicy,
  toolName: string,
): MCPAccessPolicy {
  return upsertToolRule(policy, {
    tool_name: toolName,
    source_type: "channel",
    source_value: nextDefaultSourceValue(policy, toolName),
    subject_type: "all",
    subject_value: "",
    effect: "ask",
  });
}

export function upsertToolRule(
  policy: MCPAccessPolicy,
  rule: MCPToolAccessOverride,
  previousRule?: RuleIdentity,
): MCPAccessPolicy {
  const nextRule = normalizeMCPToolRule(rule);
  const previousKey = previousRule
    ? ruleIdentityKey(previousRule)
    : ruleIdentityKey(nextRule);
  const nextKey = ruleIdentityKey(nextRule);
  const nextOverrides = policy.tool_overrides.filter((item) => {
    const itemKey = ruleIdentityKey(normalizeMCPToolRule(item));
    return itemKey !== previousKey && itemKey !== nextKey;
  });
  nextOverrides.push(nextRule);
  return {
    ...policy,
    tool_overrides: sortToolRules(nextOverrides),
  };
}

export function removeToolRule(
  policy: MCPAccessPolicy,
  rule: RuleIdentity,
): MCPAccessPolicy {
  const targetKey = ruleIdentityKey(rule);
  return {
    ...policy,
    tool_overrides: policy.tool_overrides.filter(
      (item) => ruleIdentityKey(normalizeMCPToolRule(item)) !== targetKey,
    ),
  };
}

export function ruleIdentityKey(rule: RuleIdentity): string {
  const normalized = normalizeRuleIdentity(rule);
  return [
    normalized.tool_name,
    normalized.source_type,
    normalized.source_value,
    normalized.subject_type,
    normalized.subject_value,
  ].join("\u0000");
}

function normalizeRuleIdentity(rule: RuleIdentity): RuleIdentity {
  const sourceType = normalizeSourceType(rule.source_type);
  const subjectType = normalizeSubjectType(rule.subject_type);
  return {
    tool_name: rule.tool_name || "*",
    source_type: sourceType,
    source_value: normalizeSourceValue(sourceType, rule.source_value),
    subject_type: subjectType,
    subject_value:
      subjectType === "all" ? "" : (rule.subject_value || "").trim(),
  };
}

function nextDefaultSourceValue(
  policy: MCPAccessPolicy,
  toolName: string,
): string {
  const used = new Set(
    policy.tool_overrides
      .filter((item) => item.tool_name === toolName)
      .map((item) => ruleIdentityKey(normalizeMCPToolRule(item))),
  );
  for (const sourceValue of MCP_CHANNEL_SOURCE_VALUES) {
    const candidate = ruleIdentityKey({
      tool_name: toolName,
      source_type: "channel",
      source_value: sourceValue,
      subject_type: "all",
      subject_value: "",
    });
    if (!used.has(candidate)) return sourceValue;
  }
  return DEFAULT_CHANNEL_SOURCE;
}

function defaultSourceValue(sourceType: MCPAccessSourceType): string {
  return sourceType === "app" ? DEFAULT_APP_SOURCE : DEFAULT_CHANNEL_SOURCE;
}

function normalizeSourceValue(
  sourceType: MCPAccessSourceType,
  sourceValue: string,
): string {
  const trimmed = (sourceValue || "").trim();
  const allowedValues =
    sourceType === "app" ? MCP_APP_SOURCE_VALUES : MCP_CHANNEL_SOURCE_VALUES;
  return allowedValues.some((value) => value === trimmed)
    ? trimmed
    : defaultSourceValue(sourceType);
}

function normalizeSourceType(
  sourceType: MCPAccessSourceType,
): MCPAccessSourceType {
  return sourceType === "app" ? "app" : "channel";
}

function normalizeSubjectType(
  subjectType: MCPAccessSubjectType,
): MCPAccessSubjectType {
  return subjectType === "user" ? "user" : "all";
}

function sortToolRules(
  rules: MCPToolAccessOverride[],
): MCPToolAccessOverride[] {
  return [...rules].map(normalizeMCPToolRule).sort((a, b) => {
    const sourceOrder =
      a.source_type.localeCompare(b.source_type) ||
      a.source_value.localeCompare(b.source_value);
    const subjectOrder =
      a.subject_type.localeCompare(b.subject_type) ||
      a.subject_value.localeCompare(b.subject_value);
    return (
      a.tool_name.localeCompare(b.tool_name) || sourceOrder || subjectOrder
    );
  });
}
