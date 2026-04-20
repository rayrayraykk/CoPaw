import { useState, useEffect } from "react";
import { Card, Select, Alert, message } from "@agentscope-ai/design";
import { useTranslation } from "react-i18next";
import { Shield, AlertCircle } from "lucide-react";
import styles from "../index.module.less";

const APPROVAL_LEVELS = [
  { value: "OFF", label: "OFF" },
  { value: "STRICT", label: "STRICT" },
  { value: "AUTO", label: "AUTO" },
  { value: "SMART", label: "SMART" },
];

interface ApprovalLevelConfig {
  approval_level: string;
  approval_level_cron: string | null;
  approval_level_agent_cli: string | null;
  approval_level_agent_tool: string | null;
  available_levels?: string[];
}

export function ApprovalConfigCard() {
  const { t } = useTranslation();
  const [approvalLevel, setApprovalLevel] = useState<string>("AUTO");
  const [approvalLevelCron, setApprovalLevelCron] = useState<string | null>(
    null,
  );
  const [approvalLevelAgentCli, setApprovalLevelAgentCli] = useState<
    string | null
  >(null);
  const [approvalLevelAgentTool, setApprovalLevelAgentTool] = useState<
    string | null
  >(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Fetch configurations on mount
  useEffect(() => {
    fetchConfigs();
  }, []);

  const fetchConfigs = async () => {
    setLoading(true);
    setError(null);
    try {
      // Fetch approval level (x-agent-id is automatically sent in header)
      const res = await fetch("/api/config/approval-level");
      if (!res.ok) {
        throw new Error("Failed to fetch approval level");
      }
      const data: ApprovalLevelConfig = await res.json();
      setApprovalLevel(data.approval_level);
      setApprovalLevelCron(data.approval_level_cron);
      setApprovalLevelAgentCli(data.approval_level_agent_cli);
      setApprovalLevelAgentTool(data.approval_level_agent_tool);
    } catch (err) {
      const errorMsg =
        err instanceof Error ? err.message : "Unknown error occurred";
      setError(errorMsg);
      message.error(t("agentConfig.approval.fetchError"));
    } finally {
      setLoading(false);
    }
  };

  const saveAllLevels = async (updates: Partial<ApprovalLevelConfig>) => {
    setSaving(true);
    try {
      const res = await fetch("/api/config/approval-level", {
        method: "PUT",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          approval_level: updates.approval_level ?? approvalLevel,
          approval_level_cron: updates.approval_level_cron ?? approvalLevelCron,
          approval_level_agent_cli:
            updates.approval_level_agent_cli ?? approvalLevelAgentCli,
          approval_level_agent_tool:
            updates.approval_level_agent_tool ?? approvalLevelAgentTool,
        }),
      });

      if (!res.ok) {
        throw new Error("Failed to update approval level");
      }

      const data: ApprovalLevelConfig = await res.json();
      setApprovalLevel(data.approval_level);
      setApprovalLevelCron(data.approval_level_cron);
      setApprovalLevelAgentCli(data.approval_level_agent_cli);
      setApprovalLevelAgentTool(data.approval_level_agent_tool);

      message.success(t("agentConfig.approval.saved"));
    } catch (err) {
      message.error(t("agentConfig.approval.saveError"));
    } finally {
      setSaving(false);
    }
  };

  const handleApprovalLevelChange = async (value: string) => {
    await saveAllLevels({ approval_level: value });
  };

  const handleCronLevelChange = async (value: string) => {
    const newValue = value === "default" ? null : value;
    await saveAllLevels({ approval_level_cron: newValue });
  };

  const handleAgentCliLevelChange = async (value: string) => {
    const newValue = value === "default" ? null : value;
    await saveAllLevels({ approval_level_agent_cli: newValue });
  };

  const handleAgentToolLevelChange = async (value: string) => {
    const newValue = value === "default" ? null : value;
    await saveAllLevels({ approval_level_agent_tool: newValue });
  };

  const getLevelDescription = (level: string): string => {
    switch (level) {
      case "OFF":
        return t("agentConfig.approval.level.off");
      case "STRICT":
        return t("agentConfig.approval.level.strict");
      case "AUTO":
        return t("agentConfig.approval.level.auto");
      case "SMART":
        return t("agentConfig.approval.level.smart");
      default:
        return "";
    }
  };

  if (loading) {
    return (
      <Card className={styles.formCard} title={t("agentConfig.approvalTitle")}>
        <div style={{ textAlign: "center", padding: "24px" }}>
          {t("common.loading")}
        </div>
      </Card>
    );
  }

  if (error) {
    return (
      <Card className={styles.formCard} title={t("agentConfig.approvalTitle")}>
        <Alert
          message={t("common.error")}
          description={error}
          type="error"
          showIcon
        />
      </Card>
    );
  }

  return (
    <Card
      className={styles.formCard}
      title={
        <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
          <Shield size={18} />
          <span>{t("agentConfig.approvalTitle")}</span>
        </div>
      }
    >
      <div style={{ display: "flex", flexDirection: "column", gap: "24px" }}>
        {/* Default Approval Level */}
        <div>
          <div
            style={{
              marginBottom: "8px",
              fontWeight: 500,
              display: "flex",
              alignItems: "center",
              gap: "6px",
            }}
          >
            <Shield size={16} />
            {t("agentConfig.approval.level.default")}
          </div>
          <Select
            value={approvalLevel}
            options={APPROVAL_LEVELS}
            onChange={handleApprovalLevelChange}
            loading={saving}
            disabled={saving}
            style={{ width: "100%", maxWidth: "320px" }}
          />
          <div
            style={{
              marginTop: "8px",
              fontSize: "13px",
              color: "#6b7280",
            }}
          >
            {getLevelDescription(approvalLevel)}
          </div>
        </div>

        {/* Cron Job Approval Level */}
        <div>
          <div
            style={{
              marginBottom: "8px",
              fontWeight: 500,
              display: "flex",
              alignItems: "center",
              gap: "6px",
            }}
          >
            <Shield size={16} />
            {t("agentConfig.approval.level.cron")}
          </div>
          <Select
            value={approvalLevelCron || "default"}
            options={[
              { value: "default", label: t("agentConfig.approval.useDefault") },
              ...APPROVAL_LEVELS,
            ]}
            onChange={handleCronLevelChange}
            loading={saving}
            disabled={saving}
            style={{ width: "100%", maxWidth: "320px" }}
          />
          <div
            style={{
              marginTop: "8px",
              fontSize: "13px",
              color: "#6b7280",
            }}
          >
            {approvalLevelCron
              ? getLevelDescription(approvalLevelCron)
              : t("agentConfig.approval.usingDefault") + ": " + approvalLevel}
          </div>
        </div>

        {/* Agent Chat CLI Approval Level */}
        <div>
          <div
            style={{
              marginBottom: "8px",
              fontWeight: 500,
              display: "flex",
              alignItems: "center",
              gap: "6px",
            }}
          >
            <Shield size={16} />
            {t("agentConfig.approval.level.agentCli")}
          </div>
          <Select
            value={approvalLevelAgentCli || "default"}
            options={[
              { value: "default", label: t("agentConfig.approval.useDefault") },
              ...APPROVAL_LEVELS,
            ]}
            onChange={handleAgentCliLevelChange}
            loading={saving}
            disabled={saving}
            style={{ width: "100%", maxWidth: "320px" }}
          />
          <div
            style={{
              marginTop: "8px",
              fontSize: "13px",
              color: "#6b7280",
            }}
          >
            {approvalLevelAgentCli
              ? getLevelDescription(approvalLevelAgentCli)
              : t("agentConfig.approval.usingDefault") + ": " + approvalLevel}
          </div>
        </div>

        {/* Agent Chat Tool Approval Level */}
        <div>
          <div
            style={{
              marginBottom: "8px",
              fontWeight: 500,
              display: "flex",
              alignItems: "center",
              gap: "6px",
            }}
          >
            <Shield size={16} />
            {t("agentConfig.approval.level.agentTool")}
          </div>
          <Select
            value={approvalLevelAgentTool || "default"}
            options={[
              { value: "default", label: t("agentConfig.approval.useDefault") },
              ...APPROVAL_LEVELS,
            ]}
            onChange={handleAgentToolLevelChange}
            loading={saving}
            disabled={saving}
            style={{ width: "100%", maxWidth: "320px" }}
          />
          <div
            style={{
              marginTop: "8px",
              fontSize: "13px",
              color: "#6b7280",
            }}
          >
            {approvalLevelAgentTool
              ? getLevelDescription(approvalLevelAgentTool)
              : t("agentConfig.approval.usingDefault") + ": " + approvalLevel}
          </div>
        </div>

        {/* Reference Card */}
        <Alert
          message={
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: "6px",
              }}
            >
              <AlertCircle size={16} />
              {t("agentConfig.approval.reference")}
            </div>
          }
          description={
            <div
              style={{
                display: "flex",
                flexDirection: "column",
                gap: "8px",
                marginTop: "8px",
              }}
            >
              <div>
                <strong>OFF</strong>: {t("agentConfig.approval.level.off")}
              </div>
              <div>
                <strong>STRICT</strong>:{" "}
                {t("agentConfig.approval.level.strict")}
              </div>
              <div>
                <strong>AUTO</strong>: {t("agentConfig.approval.level.auto")}
              </div>
              <div>
                <strong>SMART</strong>: {t("agentConfig.approval.level.smart")}
              </div>
            </div>
          }
          type="info"
          showIcon={false}
          style={{ marginTop: "8px" }}
        />
      </div>
    </Card>
  );
}
