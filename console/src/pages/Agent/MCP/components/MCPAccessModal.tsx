import React, { useEffect, useMemo, useState } from "react";
import { DeleteOutlined, PlusOutlined } from "@ant-design/icons";
import {
  Button,
  Empty,
  Input,
  Modal,
  Select,
  Tag,
} from "@agentscope-ai/design";
import { Segmented, Spin } from "antd";
import { useTranslation } from "react-i18next";
import api from "../../../../api";
import type {
  MCPAccessEffect,
  MCPAccessPolicy,
  MCPAccessSourceType,
  MCPAccessSubjectType,
  MCPClientInfo,
  MCPToolAccessOverride,
  MCPToolInfo,
} from "../../../../api/types";
import {
  MCP_APP_SOURCE_VALUES,
  MCP_CHANNEL_SOURCE_VALUES,
  addToolRule,
  buildMCPAccessToolGroups,
  removeToolRule,
  ruleIdentityKey,
  upsertToolRule,
} from "../accessPolicy";
import styles from "../index.module.less";

interface MCPAccessModalProps {
  client: MCPClientInfo;
  open: boolean;
  onClose: () => void;
  onSave: (policy: MCPAccessPolicy) => Promise<boolean>;
}

interface RuleTextInputProps {
  value: string;
  placeholder: string;
  className: string;
  onCommit: (value: string) => void;
}

const POLICY_SEGMENT_COLORS: Record<
  MCPAccessEffect,
  { bg: string; border: string; text: string }
> = {
  ask: {
    bg: "rgba(245, 158, 11, 0.24)",
    border: "rgba(217, 119, 6, 0.36)",
    text: "#8a4b00",
  },
  allow: {
    bg: "rgba(34, 197, 94, 0.22)",
    border: "rgba(22, 163, 74, 0.35)",
    text: "#17643a",
  },
  deny: {
    bg: "rgba(239, 68, 68, 0.2)",
    border: "rgba(220, 38, 38, 0.34)",
    text: "#9f1f26",
  },
};

function policySegmentStyle(effect: MCPAccessEffect): React.CSSProperties {
  const color = POLICY_SEGMENT_COLORS[effect];
  return {
    "--mcp-policy-segment-bg": color.bg,
    "--mcp-policy-segment-border": color.border,
    "--mcp-policy-segment-text": color.text,
  } as React.CSSProperties;
}

const RuleTextInput: React.FC<RuleTextInputProps> = ({
  value,
  placeholder,
  className,
  onCommit,
}) => {
  const [draft, setDraft] = useState(value);

  useEffect(() => {
    setDraft(value);
  }, [value]);

  return (
    <Input
      value={draft}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={() => onCommit(draft)}
      onPressEnter={() => onCommit(draft)}
      placeholder={placeholder}
      className={className}
    />
  );
};

function defaultSourceValue(sourceType: MCPAccessSourceType): string {
  return sourceType === "app" ? "Creator" : "console";
}

function defaultSubjectValue(subjectType: MCPAccessSubjectType): string {
  return subjectType === "user" ? "default" : "";
}

const CHANNEL_SOURCE_OPTIONS: { label: string; value: string }[] =
  MCP_CHANNEL_SOURCE_VALUES.map((value) => ({
    label:
      {
        console: "Console",
        dingtalk: "DingTalk",
        feishu: "Feishu",
        wechat: "WeChat",
        wecom: "WeCom",
        discord: "Discord",
        telegram: "Telegram",
        qq: "QQ",
        imessage: "iMessage",
        mattermost: "Mattermost",
        matrix: "Matrix",
        onebot: "OneBot",
        mqtt: "MQTT",
        voice: "Voice",
        sip: "SIP",
        xiaoyi: "XiaoYi",
      }[value] || value,
    value,
  }));

const APP_SOURCE_OPTIONS: { label: string; value: string }[] =
  MCP_APP_SOURCE_VALUES.map((value) => ({
    label: value,
    value,
  }));

function getSourceValueOptions(sourceType: MCPAccessSourceType) {
  return sourceType === "app" ? APP_SOURCE_OPTIONS : CHANNEL_SOURCE_OPTIONS;
}

export const MCPAccessModal: React.FC<MCPAccessModalProps> = ({
  client,
  open,
  onClose,
  onSave,
}) => {
  const { t } = useTranslation();
  const [policy, setPolicy] = useState<MCPAccessPolicy | null>(null);
  const [tools, setTools] = useState<MCPToolInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [toolsError, setToolsError] = useState("");

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    const load = async () => {
      setLoading(true);
      setTools([]);
      setToolsError("");
      try {
        const savedPolicy = await api.getMCPPolicy(client.key);
        if (!cancelled) {
          setPolicy(savedPolicy);
        }

        if (!client.enabled) {
          if (!cancelled) {
            setToolsError(t("mcp.access.disabledTools"));
          }
          return;
        }

        try {
          const currentTools = await api.listMCPTools(client.key);
          if (!cancelled) {
            setTools(currentTools);
          }
        } catch (err: any) {
          if (!cancelled) {
            setToolsError(err?.message || t("mcp.toolsLoadError"));
          }
        }
      } catch {
        if (!cancelled) {
          setPolicy(null);
          setToolsError(t("mcp.access.loadError"));
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    };
    load();
    return () => {
      cancelled = true;
    };
  }, [open, client.key, client.enabled, t]);

  const groups = useMemo(
    () => (policy ? buildMCPAccessToolGroups(tools, policy) : []),
    [tools, policy],
  );

  const effectLabel = (effect: MCPAccessEffect) =>
    t(`mcp.access.effect.${effect}`);

  const setDefaultEffect = (effect: MCPAccessEffect) => {
    setPolicy((prev) =>
      prev
        ? {
            ...prev,
            default_effect: effect,
          }
        : prev,
    );
  };

  const addRule = (toolName: string) => {
    setPolicy((prev) => (prev ? addToolRule(prev, toolName) : prev));
  };

  const updateRule = (
    rule: MCPToolAccessOverride,
    patch: Partial<MCPToolAccessOverride>,
  ) => {
    const nextRule = { ...rule, ...patch };
    if (patch.source_type) {
      nextRule.source_value = defaultSourceValue(patch.source_type);
    }
    if (patch.subject_type) {
      nextRule.subject_value = defaultSubjectValue(patch.subject_type);
    }
    setPolicy((prev) =>
      prev
        ? upsertToolRule(prev, nextRule, {
            tool_name: rule.tool_name,
            source_type: rule.source_type,
            source_value: rule.source_value,
            subject_type: rule.subject_type,
            subject_value: rule.subject_value,
          })
        : prev,
    );
  };

  const setRuleEffect = (
    rule: MCPToolAccessOverride,
    effect: MCPAccessEffect,
  ) => {
    setPolicy((prev) =>
      prev ? upsertToolRule(prev, { ...rule, effect }) : prev,
    );
  };

  const deleteRule = (rule: MCPToolAccessOverride) => {
    setPolicy((prev) => (prev ? removeToolRule(prev, rule) : prev));
  };

  const sourceTypeOptions = [
    { label: t("mcp.access.source.channel"), value: "channel" },
    { label: t("mcp.access.source.app"), value: "app" },
  ];
  const subjectTypeOptions = [
    { label: t("mcp.access.subjectTypeOption.all"), value: "all" },
    { label: t("mcp.access.subjectTypeOption.user"), value: "user" },
  ];

  const handleSave = async () => {
    if (!policy) return;
    setSaving(true);
    try {
      const ok = await onSave(policy);
      if (ok) {
        onClose();
      }
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      title={`${client.name} - ${t("mcp.tools")}`}
      open={open}
      onCancel={onClose}
      width={920}
      footer={
        <div style={{ textAlign: "right" }}>
          <Button onClick={onClose} style={{ marginRight: 8 }}>
            {t("common.cancel")}
          </Button>
          <Button
            type="primary"
            onClick={handleSave}
            loading={saving}
            disabled={!policy || loading}
          >
            {t("common.save")}
          </Button>
        </div>
      }
    >
      {loading && !policy ? (
        <div className={styles.toolsLoading}>
          <Spin />
        </div>
      ) : policy ? (
        <div className={styles.accessModalBody}>
          <div className={styles.accessDefaultRow}>
            <span className={styles.accessDefaultLabel}>
              {t("mcp.access.default")}
            </span>
            <Segmented
              className={styles.accessPolicySegmented}
              style={policySegmentStyle(policy.default_effect)}
              value={policy.default_effect}
              onChange={(value) => setDefaultEffect(value as MCPAccessEffect)}
              options={[
                { label: effectLabel("ask"), value: "ask" },
                { label: effectLabel("allow"), value: "allow" },
                { label: effectLabel("deny"), value: "deny" },
              ]}
            />
          </div>

          {toolsError && <div className={styles.toolsError}>{toolsError}</div>}

          {groups.length === 0 ? (
            <Empty description={t("mcp.noTools")} />
          ) : (
            <div className={styles.accessToolGroups}>
              {groups.map((group) => (
                <div key={group.toolName} className={styles.accessToolGroup}>
                  <div className={styles.accessToolGroupHeader}>
                    <div className={styles.accessToolInfo}>
                      <div className={styles.accessToolTitle}>
                        <Tag color={group.stale ? "default" : "blue"}>
                          {group.toolName}
                        </Tag>
                        {group.stale && (
                          <Tag color="orange">{t("mcp.access.stale")}</Tag>
                        )}
                      </div>
                      {group.description && (
                        <div className={styles.accessToolDescription}>
                          {group.description}
                        </div>
                      )}
                      {group.inputSchema &&
                        Object.keys(group.inputSchema).length > 0 && (
                          <details className={styles.toolSchema}>
                            <summary>{t("mcp.toolSchema")}</summary>
                            <pre className={styles.toolSchemaContent}>
                              {JSON.stringify(group.inputSchema, null, 2)}
                            </pre>
                          </details>
                        )}
                    </div>
                    <Button
                      icon={<PlusOutlined />}
                      onClick={() => addRule(group.toolName)}
                    >
                      {t("mcp.access.addRule")}
                    </Button>
                  </div>

                  {group.rules.length === 0 ? (
                    <div className={styles.accessNoRules}>
                      {t("mcp.access.noRules")}
                    </div>
                  ) : (
                    <div className={styles.accessRuleList}>
                      {group.rules.map((rule) => (
                        <div
                          key={ruleIdentityKey(rule)}
                          className={styles.accessRuleRow}
                        >
                          <div className={styles.accessRuleField}>
                            <span className={styles.accessRuleFieldLabel}>
                              {t("mcp.access.sourceType")}
                            </span>
                            <Select
                              className={styles.accessRuleSourceType}
                              value={rule.source_type}
                              onChange={(value) =>
                                updateRule(rule, {
                                  source_type: value as MCPAccessSourceType,
                                })
                              }
                              options={sourceTypeOptions}
                            />
                          </div>
                          <div className={styles.accessRuleField}>
                            <span className={styles.accessRuleFieldLabel}>
                              {t("mcp.access.sourceValue")}
                            </span>
                            <Select
                              className={styles.accessRuleSourceValue}
                              value={rule.source_value}
                              onChange={(sourceValue) =>
                                updateRule(rule, {
                                  source_value: String(sourceValue),
                                })
                              }
                              options={getSourceValueOptions(rule.source_type)}
                            />
                          </div>
                          <div className={styles.accessRuleField}>
                            <span className={styles.accessRuleFieldLabel}>
                              {t("mcp.access.subjectType")}
                            </span>
                            <Select
                              className={styles.accessRuleSubjectType}
                              value={rule.subject_type}
                              onChange={(value) =>
                                updateRule(rule, {
                                  subject_type: value as MCPAccessSubjectType,
                                })
                              }
                              options={subjectTypeOptions}
                            />
                          </div>
                          <div className={styles.accessRuleField}>
                            <span className={styles.accessRuleFieldLabel}>
                              {t("mcp.access.subjectValue")}
                            </span>
                            {rule.subject_type === "user" ? (
                              <RuleTextInput
                                value={rule.subject_value}
                                placeholder={t(
                                  "mcp.access.subjectValuePlaceholder",
                                )}
                                className={styles.accessRuleSubjectValue}
                                onCommit={(subjectValue) =>
                                  updateRule(rule, {
                                    subject_value: subjectValue,
                                  })
                                }
                              />
                            ) : (
                              <Input
                                className={styles.accessRuleSubjectValue}
                                value={t("mcp.access.subjectValueAll")}
                                disabled
                              />
                            )}
                          </div>
                          <div className={styles.accessRuleField}>
                            <span className={styles.accessRuleFieldLabel}>
                              {t("mcp.access.effectLabel")}
                            </span>
                            <Select
                              className={styles.accessRuleEffect}
                              value={rule.effect}
                              onChange={(value) =>
                                setRuleEffect(rule, value as MCPAccessEffect)
                              }
                              options={[
                                { label: effectLabel("allow"), value: "allow" },
                                { label: effectLabel("ask"), value: "ask" },
                                { label: effectLabel("deny"), value: "deny" },
                              ]}
                            />
                          </div>
                          <Button
                            className={styles.accessRuleDeleteButton}
                            icon={<DeleteOutlined />}
                            onClick={() => deleteRule(rule)}
                            title={t("mcp.access.deleteRule")}
                          />
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      ) : (
        <div className={styles.toolsError}>{t("mcp.access.loadError")}</div>
      )}
    </Modal>
  );
};
