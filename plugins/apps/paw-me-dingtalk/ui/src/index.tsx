import type * as ReactNS from "react";
import {
  Activity,
  Check,
  Clock3,
  Circle,
  Download,
  ExternalLink,
  Inbox,
  LockKeyhole,
  MessageSquareText,
  PauseCircle,
  RefreshCw,
  Send,
  Settings2,
  ShieldCheck,
  Trash2,
  UserCheck,
  X,
} from "lucide-react";

const APP_ID = "paw-me-dingtalk";
const host = window.QwenPaw.host;
const React: typeof ReactNS = host.React;
const { useEffect, useMemo, useState } = React;
const {
  Alert,
  Badge,
  Button,
  Card,
  Col,
  Descriptions,
  Drawer,
  Empty,
  Form,
  Input,
  InputNumber,
  List,
  Modal,
  Popconfirm,
  Progress,
  Row,
  Select,
  Space,
  Spin,
  Switch,
  Table,
  Tabs,
  Tag,
  Timeline,
  Typography,
} = host.antd as any;
const { Text, Title } = Typography;

type Agent = {
  id: string;
  name: string;
  backend?: string;
  enabled: boolean;
  available_in_chat?: boolean;
};

type Principal = {
  id: string;
  subject_type: "person" | "group";
  subject_id: string;
  id_source: string;
  display_name: string;
  conversation_alias: string;
  policy: "observe" | "draft" | "automatic" | "blocked";
};

type Message = {
  id: string;
  text: string;
  received_at: number;
  ordinal: number;
};

type WorkItem = {
  id: string;
  conversation_alias: string;
  subject_type: "person" | "group";
  subject_id?: string;
  id_source: string;
  display_name: string;
  status: string;
  agent_id: string;
  quiet_deadline: number;
  message_count: number;
  text: string;
  error: string;
  messages: Message[];
  updated_at: number;
};

type Outbox = {
  id: string;
  work_item_id: string;
  conversation_alias: string;
  text: string;
  status: string;
  error: string;
  updated_at: number;
  source_display_name: string;
  source_subject_type: "person" | "group";
  source_messages: Message[];
};

type ActivityItem = {
  id: number;
  kind: string;
  status: string;
  title: string;
  detail: string;
  created_at: number;
};

type OwnerProfile = {
  status: "absent" | "collecting" | "ready" | "partial" | "stale" | "failed";
  collected: {
    identity?: {
      name?: string;
      title?: string;
      departments?: string[];
      roles?: string[];
    };
    work_style?: {
      message_count?: number;
      average_message_length?: number;
      created_todo_subjects?: string[];
      calendar_subjects?: string[];
    };
    relationships?: Array<{
      subject_id: string;
      name: string;
      kinds: string[];
      interaction_count: number;
      shared_group_count: number;
      last_interaction_at: string;
    }>;
    coverage?: Record<string, number | boolean>;
  };
  approved: { notes?: string };
  error: string;
  refreshed_at: number;
  next_refresh_at: number;
  approved_at: number | null;
  revision: number;
};

type Snapshot = {
  settings: {
    enabled: boolean;
    agent_id: string;
    default_policy: string;
    access_mode: "approval" | "allow_all" | "block_all";
    quiet_seconds: number;
    max_wait_seconds: number;
  };
  principals: Principal[];
  work_items: WorkItem[];
  outbox: Outbox[];
  activity: ActivityItem[];
  owner_profile: OwnerProfile;
  identity_provider: {
    provider: string;
    available: boolean;
    authenticated: boolean;
    version: string;
    corp_id: string;
    corp_name: string;
    user_id: string;
    user_name: string;
    detail: string;
    confirmed: boolean;
  };
  runtime: {
    running: boolean;
    stage: string;
    detail: string;
    current_conversation: string;
    last_error: string;
    heartbeat_at: number;
    integration_stage: string;
    integration_detail: string;
    integration_progress: number | null;
    profile_stage: string;
    profile_detail: string;
    profile_progress: number | null;
  };
};

const styles = `
.pm-page{max-width:1440px;margin:0 auto;padding:24px 28px 48px}
.pm-header{display:flex;align-items:flex-start;justify-content:space-between;gap:24px;margin-bottom:22px}
.pm-eyebrow{display:flex;align-items:center;gap:8px;margin-bottom:8px;color:var(--ant-color-text-secondary);font-size:12px;font-weight:600;letter-spacing:.08em;text-transform:uppercase}
.pm-header h1{margin:0 0 6px!important;font-size:30px!important;letter-spacing:-.035em}.pm-header-copy{max-width:720px}
.pm-actions{display:flex;align-items:center;justify-content:flex-end;gap:10px;flex-wrap:wrap}
.pm-statusbar{margin-bottom:18px}.pm-status-inner{display:flex;align-items:center;justify-content:space-between;gap:18px}.pm-status-main{display:flex;align-items:center;gap:12px;min-width:0}.pm-status-text{min-width:0}.pm-status-title{font-weight:600}.pm-status-detail{display:block;max-width:720px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.pm-metric{height:100%}.pm-metric .ant-card-body{display:flex;align-items:center;gap:14px;padding:18px}.pm-metric-icon{display:grid;place-items:center;width:38px;height:38px;border-radius:10px;background:var(--ant-color-fill-secondary);color:var(--ant-color-primary);flex:none}.pm-metric-value{font-size:20px;font-weight:650;line-height:1.2}.pm-metric-label{color:var(--ant-color-text-secondary);font-size:12px;margin-top:3px}
.pm-panel{margin-top:16px}.pm-panel .ant-card-head{min-height:52px}.pm-item-title{display:flex;align-items:center;gap:8px;flex-wrap:wrap}.pm-message-stack{display:grid;gap:8px;margin-top:12px}.pm-message{padding:9px 11px;border-radius:8px;background:var(--ant-color-fill-tertiary);white-space:pre-wrap}.pm-meta{font-size:12px;color:var(--ant-color-text-secondary)}
.pm-global{margin-bottom:16px}.pm-global-grid{display:grid;grid-template-columns:minmax(220px,1fr) minmax(220px,1fr);gap:18px}.pm-global-field{display:grid;gap:7px}.pm-global-label{font-weight:650}.pm-source{margin:12px 0;padding:12px;border:1px solid var(--ant-color-border-secondary);border-radius:10px}.pm-source-head{display:flex;justify-content:space-between;gap:12px;align-items:center;margin-bottom:8px}.pm-draft{padding:12px;border-radius:10px;background:var(--ant-color-fill-quaternary)}.pm-account{margin-top:18px;padding:16px;border:1px solid var(--ant-color-border-secondary);border-radius:10px;background:var(--ant-color-bg-container)}
.pm-policy-grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:12px}.pm-subtle{color:var(--ant-color-text-secondary)}.pm-pre{white-space:pre-wrap;line-height:1.65;margin:0}.pm-error{color:var(--ant-color-error)}.pm-id{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;overflow-wrap:anywhere;font-size:12px}.pm-setup{display:flex;align-items:center;justify-content:space-between;gap:18px;padding:16px;border:1px solid var(--ant-color-border-secondary);border-radius:10px;margin-bottom:16px}.pm-setup-copy{min-width:0}.pm-setup-title{font-weight:650;margin-bottom:4px}
.pm-onboarding{max-width:880px;margin:42px auto 0}.pm-onboarding .ant-card-body{padding:32px}.pm-onboarding-head{max-width:650px;margin-bottom:30px}.pm-onboarding-head h2{margin:0 0 8px!important;font-size:26px!important;letter-spacing:-.025em}.pm-steps{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:10px;margin-bottom:24px}.pm-step{display:flex;align-items:center;gap:10px;padding:12px;border:1px solid var(--ant-color-border-secondary);border-radius:10px;color:var(--ant-color-text-secondary)}.pm-step-current{border-color:var(--ant-color-primary);color:var(--ant-color-text);background:var(--ant-color-primary-bg)}.pm-step-done{color:var(--ant-color-success)}.pm-step-icon{display:grid;place-items:center;flex:none}.pm-onboarding-action{padding:22px;border-radius:12px;background:var(--ant-color-fill-quaternary)}.pm-onboarding-action h3{margin:0 0 6px;font-size:18px}.pm-progress{margin:18px 0 6px}.pm-onboarding-buttons{display:flex;align-items:center;gap:10px;flex-wrap:wrap;margin-top:20px}.pm-agent-select{width:100%;max-width:420px;margin-top:16px}
.pm-profile-grid{display:grid;grid-template-columns:minmax(240px,.8fr) minmax(320px,1.2fr);gap:20px}.pm-profile-facts{display:grid;gap:10px}.pm-profile-relation{display:flex;align-items:center;justify-content:space-between;gap:12px;padding:9px 0;border-bottom:1px solid var(--ant-color-border-secondary)}.pm-profile-actions{display:flex;gap:10px;flex-wrap:wrap;margin-top:14px}.pm-profile-progress{margin:12px 0}.pm-profile-note{margin-top:12px}
@media(max-width:760px){.pm-page{padding:16px 12px 32px}.pm-header{flex-direction:column}.pm-actions{justify-content:flex-start}.pm-status-inner{align-items:flex-start;flex-direction:column}.pm-status-detail{white-space:normal}.pm-policy-grid,.pm-global-grid,.pm-profile-grid{grid-template-columns:1fr}.pm-onboarding{margin-top:18px}.pm-onboarding .ant-card-body{padding:20px}.pm-steps{grid-template-columns:1fr}.pm-source-head{align-items:flex-start;flex-direction:column}}
`;

const statusLabel: Record<string, string> = {
  identity_required: "待绑定真实身份",
  blocked: "已阻止",
  observed: "仅观察",
  collecting: "正在等待对方说完",
  ready: "等待处理",
  agent_running: "Agent 处理中",
  interrupt_requested: "正在停止旧回复并合并",
  draft_ready: "待发送",
  needs_review: "需要人工确认",
  sending: "发送中",
  sent: "已发送",
  failed: "失败",
};

function time(value?: number) {
  return value ? new Date(value * 1000).toLocaleString() : "—";
}

function sourceLabel(value?: string) {
  return value === "oauth:dws-event"
    ? "钉钉 OAuth 事件"
    : value || "无可信来源";
}

function StatusTag({ status }: { status: string }) {
  const color =
    status === "sent"
      ? "success"
      : status === "failed" || status === "blocked"
      ? "error"
      : status === "draft_ready" ||
        status === "identity_required" ||
        status === "needs_review"
      ? "warning"
      : "processing";
  return <Tag color={color}>{statusLabel[status] || status}</Tag>;
}

function PawMePage() {
  const sdk = useMemo(() => window.QwenPaw.paw?.forApp(APP_ID), []);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [data, setData] = useState<Snapshot | null>(null);
  const [agentId, setAgentId] = useState(
    sdk?.host.getSelectedAgentId() || "default",
  );
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [authorizationItem, setAuthorizationItem] = useState<WorkItem | null>(
    null,
  );
  const [draftEditor, setDraftEditor] = useState<Outbox | null>(null);
  const [draftText, setDraftText] = useState("");
  const [profileNotes, setProfileNotes] = useState("");
  const [settingsForm] = Form.useForm();
  const [authorizationForm] = Form.useForm();

  const api = sdk?.api;
  const refresh = async (selected = agentId, quiet = false) => {
    if (!api) {
      setError("当前 QwenPaw 版本未提供 PawApp SDK");
      setLoading(false);
      return;
    }
    if (!quiet) setLoading(true);
    try {
      const snapshot = await api.get<Snapshot>("/snapshot", {
        query: { agent_id: selected },
      });
      setData(snapshot);
      if (
        snapshot.settings.agent_id &&
        snapshot.settings.agent_id !== agentId
      ) {
        setAgentId(snapshot.settings.agent_id);
      }
      setError("");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "状态加载失败");
    } finally {
      if (!quiet) setLoading(false);
    }
  };

  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      try {
        const response = host.fetch
          ? await host.fetch("/agents")
          : await fetch(host.getApiUrl("/agents"));
        const payload = await response.json();
        if (!cancelled) {
          setAgents(
            (payload.agents || []).filter(
              (agent: Agent) =>
                agent.enabled && agent.available_in_chat !== false,
            ),
          );
        }
      } catch {
        if (!cancelled) setAgents([]);
      }
      if (!cancelled) await refresh(agentId);
    };
    void load();
    const interval = window.setInterval(
      () => void refresh(agentId, true),
      2000,
    );
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [agentId]);

  useEffect(() => {
    setProfileNotes(data?.owner_profile.approved.notes || "");
  }, [data?.owner_profile.revision]);

  const saveSettings = async (values: Record<string, unknown>) => {
    if (!api) return;
    setSaving(true);
    try {
      const next = await api.put<Snapshot>("/settings", values, {
        query: { agent_id: String(values.agent_id) },
      });
      setAgentId(String(values.agent_id));
      setData(next);
      setSettingsOpen(false);
      await sdk?.host.toast("Paw Me 设置已保存", "success");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "设置保存失败");
    } finally {
      setSaving(false);
    }
  };

  const toggleRuntime = async (enabled: boolean) => {
    if (!data) return;
    await saveSettings({ ...data.settings, enabled, agent_id: agentId });
  };

  const saveGlobalSetting = async (
    key: "access_mode" | "default_policy",
    value: string,
  ) => {
    if (!data) return;
    await saveSettings({
      ...data.settings,
      [key]: value,
      agent_id: agentId,
    });
  };

  const selectAgent = async (selected: string) => {
    setAgentId(selected);
    if (!data) return;
    await saveSettings({
      ...data.settings,
      agent_id: selected,
    });
  };

  const openSettings = () => {
    settingsForm.setFieldsValue({ ...data?.settings, agent_id: agentId });
    setSettingsOpen(true);
  };

  const openAuthorization = (item: WorkItem) => {
    authorizationForm.setFieldsValue({
      policy: data?.settings.default_policy || "draft",
    });
    setAuthorizationItem(item);
  };

  const saveAuthorization = async (values: Record<string, unknown>) => {
    if (!api || !authorizationItem) return;
    setSaving(true);
    try {
      await api.post(`/work-items/${authorizationItem.id}/authorize`, values);
      setAuthorizationItem(null);
      await refresh(agentId);
      await sdk?.host.toast("真实身份已授权", "success");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "身份授权失败");
    } finally {
      setSaving(false);
    }
  };

  const runIntegration = async (action: "install" | "login") => {
    if (!api) return;
    setSaving(true);
    try {
      await api.post(`/dws/${action}`);
      await refresh(agentId, true);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "钉钉连接失败");
    } finally {
      setSaving(false);
    }
  };

  const cancelIntegration = async () => {
    if (!api) return;
    setSaving(true);
    try {
      await api.post("/dws/cancel");
      await refresh(agentId, true);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "取消操作失败");
    } finally {
      setSaving(false);
    }
  };

  const confirmIdentity = async () => {
    if (!api) return;
    setSaving(true);
    try {
      const next = await api.post<Snapshot>("/identity/confirm");
      setData(next);
      await sdk?.host.toast("本人钉钉账号已确认", "success");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "账号确认失败");
    } finally {
      setSaving(false);
    }
  };

  const reconnectIdentity = async () => {
    if (!api) return;
    setSaving(true);
    try {
      const next = await api.post<Snapshot>("/identity/reconnect");
      setData(next);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "重新连接失败");
    } finally {
      setSaving(false);
    }
  };

  const refreshProfile = async () => {
    if (!api) return;
    try {
      setData(await api.post<Snapshot>("/profile/refresh"));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "画像更新失败");
    }
  };

  const cancelProfile = async () => {
    if (!api) return;
    setData(await api.post<Snapshot>("/profile/cancel"));
  };

  const approveProfile = async () => {
    if (!api) return;
    try {
      setData(
        await api.post<Snapshot>("/profile/approve", {
          notes: profileNotes,
        }),
      );
      await sdk?.host.toast("本人画像已审核", "success");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "画像审核失败");
    }
  };

  const removePrincipal = async (id: string) => {
    if (!api) return;
    await api.delete(`/principals/${id}`);
    await refresh(agentId);
  };

  const updatePrincipalPolicy = async (
    id: string,
    policy: Principal["policy"],
  ) => {
    if (!api) return;
    try {
      await api.patch(`/principals/${id}/policy`, { policy });
      await refresh(agentId, true);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "策略更新失败");
    }
  };

  const sendDraft = async (id: string) => {
    if (!api) return;
    setSaving(true);
    try {
      await api.post(`/outbox/${id}/send`);
      await refresh(agentId);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "发送失败");
    } finally {
      setSaving(false);
    }
  };

  const deleteDraft = async (id: string) => {
    if (!api) return;
    await api.delete(`/outbox/${id}`);
    await refresh(agentId);
  };

  const saveDraft = async () => {
    if (!api || !draftEditor || !draftText.trim()) return;
    setSaving(true);
    try {
      await api.patch(`/outbox/${draftEditor.id}`, {
        text: draftText.trim(),
      });
      setDraftEditor(null);
      await refresh(agentId);
      await sdk?.host.toast("草稿已保存", "success");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "草稿保存失败");
    } finally {
      setSaving(false);
    }
  };

  if (loading && !data) {
    return (
      <div className="pm-page">
        <Spin />
      </div>
    );
  }

  const identityRequired =
    data?.work_items.filter((item) => item.status === "identity_required") ||
    [];
  const pendingDrafts =
    data?.outbox.filter((item) => item.status !== "sent") || [];
  const oauthReady = Boolean(data?.identity_provider.authenticated);
  const identityConfirmed = Boolean(data?.identity_provider.confirmed);
  const connectorReady = Boolean(data?.identity_provider.available);
  const integrationStage = data?.runtime.integration_stage || "idle";
  const integrationBusy = [
    "install",
    "downloading",
    "preparing",
    "installing",
    "verifying",
    "login",
  ].includes(integrationStage);
  const profile = data?.owner_profile;
  const profileBusy = profile?.status === "collecting";
  const profileApproved = Boolean(profile?.approved_at);

  if (!oauthReady || !identityConfirmed) {
    const currentStep = !connectorReady
      ? 0
      : !oauthReady || !identityConfirmed
      ? 1
      : 2;
    const stepIcon = (index: number) => {
      if (index < currentStep) return <Check size={17} />;
      return <Circle size={17} />;
    };
    const setupTitle = !connectorReady
      ? "准备钉钉连接组件"
      : !oauthReady
      ? "连接你的钉钉账号"
      : !identityConfirmed
      ? "确认数字分身的本人账号"
      : "选择负责回复的 Agent";
    const setupDetail =
      data?.runtime.integration_detail ||
      (!connectorReady
        ? "组件安装在 Paw Me 的独立目录，不修改系统 PATH。"
        : !oauthReady
        ? "浏览器将打开钉钉官方 OAuth；插件不会读取或保存账号密码。"
        : !identityConfirmed
        ? "启用前核对组织与账号，避免数字分身以错误身份发言。"
        : "任意已启用 Agent 都可以负责回复，认证由 Agent 自己管理。");

    return (
      <div className="pm-page">
        <style>{styles}</style>
        <header className="pm-header">
          <div className="pm-header-copy">
            <div className="pm-eyebrow">
              <ShieldCheck size={15} />
              Paw Me · Digital Twin
            </div>
            <Title level={1}>钉钉数字人分身</Title>
            <Text type="secondary">
              首次配置只需要安装连接组件、完成钉钉授权并选择 Agent。
            </Text>
          </div>
        </header>

        {error ? (
          <Alert
            closable
            type="error"
            message="操作未完成"
            description={error}
            onClose={() => setError("")}
            style={{ marginBottom: 16 }}
          />
        ) : null}

        <Card className="pm-onboarding">
          <div className="pm-onboarding-head">
            <Title level={2}>开始设置 Paw Me</Title>
            <Text type="secondary">
              完成下面三个步骤后，消息监听、会话授权、草稿与发送会在同一页面运行。
            </Text>
          </div>
          <div className="pm-steps">
            {["安装连接组件", "钉钉 OAuth", "选择并启用 Agent"].map(
              (label, index) => (
                <div
                  className={`pm-step ${
                    index === currentStep ? "pm-step-current" : ""
                  } ${index < currentStep ? "pm-step-done" : ""}`}
                  key={label}
                >
                  <span className="pm-step-icon">{stepIcon(index)}</span>
                  <span>{label}</span>
                </div>
              ),
            )}
          </div>
          <div className="pm-onboarding-action">
            <h3>{setupTitle}</h3>
            <Text type="secondary">{setupDetail}</Text>

            {integrationBusy ? (
              <div className="pm-progress">
                <Progress
                  percent={data?.runtime.integration_progress ?? 0}
                  showInfo={data?.runtime.integration_progress != null}
                  status="active"
                />
                {data?.runtime.integration_progress == null ? (
                  <Space size={8}>
                    <Spin size="small" />
                    <Text type="secondary">正在执行当前阶段</Text>
                  </Space>
                ) : null}
              </div>
            ) : null}

            {oauthReady && !identityConfirmed ? (
              <div className="pm-account">
                <Descriptions column={1} size="small">
                  <Descriptions.Item label="账号">
                    {data?.identity_provider.user_name || "未返回显示名"}
                  </Descriptions.Item>
                  <Descriptions.Item label="组织">
                    {data?.identity_provider.corp_name || "未返回组织名"}
                  </Descriptions.Item>
                  <Descriptions.Item label="真实 userId">
                    <span className="pm-id">
                      {data?.identity_provider.user_id || "—"}
                    </span>
                  </Descriptions.Item>
                </Descriptions>
              </div>
            ) : null}

            {identityConfirmed ? (
              <Select
                className="pm-agent-select"
                value={agentId}
                options={agents.map((agent) => ({
                  value: agent.id,
                  label: `${agent.name || agent.id} · ${
                    agent.backend || "agent"
                  }`,
                }))}
                onChange={(value: string) => setAgentId(value)}
              />
            ) : null}

            <div className="pm-onboarding-buttons">
              {!connectorReady ? (
                <Button
                  type="primary"
                  size="large"
                  icon={<Download size={17} />}
                  disabled={integrationBusy}
                  onClick={() => void runIntegration("install")}
                >
                  安装并继续
                </Button>
              ) : !oauthReady ? (
                <Button
                  type="primary"
                  size="large"
                  icon={<ExternalLink size={17} />}
                  disabled={integrationBusy}
                  onClick={() => void runIntegration("login")}
                >
                  连接钉钉
                </Button>
              ) : !identityConfirmed ? (
                <>
                  <Button
                    type="primary"
                    size="large"
                    icon={<UserCheck size={17} />}
                    loading={saving}
                    onClick={() => void confirmIdentity()}
                  >
                    确认这是我
                  </Button>
                  <Button
                    size="large"
                    icon={<RefreshCw size={17} />}
                    disabled={saving}
                    onClick={() => void reconnectIdentity()}
                  >
                    不是我，重新连接
                  </Button>
                </>
              ) : (
                <Button
                  type="primary"
                  size="large"
                  icon={<Check size={17} />}
                  loading={saving}
                  disabled={!agentId}
                  onClick={() =>
                    void saveSettings({
                      enabled: true,
                      agent_id: agentId,
                      default_policy: data?.settings.default_policy || "draft",
                      access_mode: data?.settings.access_mode || "approval",
                      quiet_seconds: data?.settings.quiet_seconds ?? 4,
                      max_wait_seconds: data?.settings.max_wait_seconds ?? 20,
                    })
                  }
                >
                  启用数字人分身
                </Button>
              )}
              {integrationBusy ? (
                <Button
                  size="large"
                  icon={<X size={17} />}
                  loading={saving}
                  onClick={() => void cancelIntegration()}
                >
                  取消当前操作
                </Button>
              ) : integrationStage === "failed" ||
                integrationStage === "cancelled" ? (
                <Button
                  size="large"
                  icon={<RefreshCw size={17} />}
                  onClick={() =>
                    void runIntegration(connectorReady ? "login" : "install")
                  }
                >
                  重新尝试
                </Button>
              ) : null}
            </div>
          </div>
        </Card>
      </div>
    );
  }

  const inbox = (
    <Card
      className="pm-panel"
      title="消息批次"
      extra={<Text type="secondary">连续消息只回复一次</Text>}
    >
      <List
        dataSource={data?.work_items || []}
        locale={{ emptyText: <Empty description="尚未捕获新消息" /> }}
        renderItem={(item: WorkItem) => (
          <List.Item
            actions={
              item.status === "identity_required"
                ? [
                    <Button
                      key="authorize"
                      type="primary"
                      onClick={() => openAuthorization(item)}
                    >
                      审核并授权
                    </Button>,
                  ]
                : []
            }
          >
            <List.Item.Meta
              title={
                <div className="pm-item-title">
                  <span>{item.conversation_alias}</span>
                  <StatusTag status={item.status} />
                  <Tag>{item.message_count} 条已合并</Tag>
                </div>
              }
              description={
                <>
                  <span>
                    {item.agent_id} · {time(item.updated_at)}
                  </span>
                  <div className="pm-id">
                    {item.subject_type === "person" ? "人员" : "群聊"} ·{" "}
                    {item.subject_id || "未获得真实 ID"} ·{" "}
                    {sourceLabel(item.id_source)}
                  </div>
                  {item.error ? (
                    <div className="pm-error">{item.error}</div>
                  ) : null}
                  <div className="pm-message-stack">
                    {item.messages.map((message) => (
                      <div className="pm-message" key={message.id}>
                        {message.text}
                      </div>
                    ))}
                  </div>
                </>
              }
            />
          </List.Item>
        )}
      />
    </Card>
  );

  const permissions = (
    <Card className="pm-panel" title="OAuth、身份与权限">
      <div className="pm-setup">
        <div className="pm-setup-copy">
          <div className="pm-setup-title">
            {oauthReady
              ? `${data?.identity_provider.user_name || "钉钉账号"} 已连接`
              : data?.identity_provider.available
              ? "连接组件已就绪，等待 OAuth 登录"
              : "安装钉钉连接组件"}
          </div>
          <Text type="secondary">
            {data?.runtime.integration_detail ||
              data?.identity_provider.detail ||
              "OAuth 由钉钉官方能力管理，插件不读取或保存令牌。"}
          </Text>
          {oauthReady ? (
            <div className="pm-id">
              {data?.identity_provider.corp_name || "当前组织"} · userId{" "}
              {data?.identity_provider.user_id || "—"}
            </div>
          ) : null}
        </div>
        {!data?.identity_provider.available ? (
          <Button
            type="primary"
            icon={<Download size={16} />}
            loading={saving || data?.runtime.integration_stage === "install"}
            onClick={() => void runIntegration("install")}
          >
            安装连接组件
          </Button>
        ) : !oauthReady ? (
          <Button
            type="primary"
            icon={<ExternalLink size={16} />}
            loading={saving || data?.runtime.integration_stage === "login"}
            onClick={() => void runIntegration("login")}
          >
            使用钉钉 OAuth 登录
          </Button>
        ) : (
          <Space wrap>
            <Button
              icon={<RefreshCw size={16} />}
              onClick={() => void refresh(agentId)}
            >
              刷新状态
            </Button>
            <Button onClick={() => void reconnectIdentity()} disabled={saving}>
              更换账号
            </Button>
          </Space>
        )}
      </div>
      <Alert
        showIcon
        type="info"
        message="单会话规则只来自收到的真实事件"
        description="人员 openDingTalkId 或群 openConversationId 由钉钉 OAuth 事件写入，界面不可手填。没有单会话规则时继承上方全局策略。"
        style={{ marginBottom: 16 }}
      />
      {identityRequired.length ? (
        <List
          header={<strong>待授权会话</strong>}
          dataSource={identityRequired}
          renderItem={(item: WorkItem) => (
            <List.Item
              actions={[
                <Button
                  key="authorize"
                  type="primary"
                  onClick={() => openAuthorization(item)}
                >
                  审核并授权
                </Button>,
              ]}
            >
              <List.Item.Meta
                title={item.display_name || item.conversation_alias}
                description={
                  <div>
                    <div className="pm-id">{item.subject_id}</div>
                    <Text type="secondary">
                      {item.subject_type === "person" ? "人员" : "群聊"} ·{" "}
                      {item.id_source}
                    </Text>
                  </div>
                }
              />
            </List.Item>
          )}
        />
      ) : null}
      <Table
        rowKey="id"
        pagination={false}
        dataSource={data?.principals || []}
        locale={{ emptyText: "暂无已验证身份" }}
        columns={[
          {
            title: "身份",
            render: (_: unknown, row: Principal) => (
              <>
                <div>{row.display_name}</div>
                <Text type="secondary">
                  {row.subject_type === "person" ? "人员" : "群聊"}
                </Text>
              </>
            ),
          },
          {
            title: "真实 ID",
            render: (_: unknown, row: Principal) => (
              <>
                <div>{row.subject_id}</div>
                <Text type="secondary">{sourceLabel(row.id_source)}</Text>
              </>
            ),
          },
          { title: "会话", dataIndex: "conversation_alias" },
          {
            title: "策略",
            render: (_: unknown, row: Principal) => (
              <Select
                size="small"
                value={row.policy}
                style={{ width: 150 }}
                options={[
                  { value: "draft", label: "生成草稿" },
                  { value: "automatic", label: "自动发送" },
                  { value: "observe", label: "仅观察" },
                  { value: "blocked", label: "阻止" },
                ]}
                onChange={(value: Principal["policy"]) =>
                  void updatePrincipalPolicy(row.id, value)
                }
              />
            ),
          },
          {
            title: "操作",
            render: (_: unknown, row: Principal) => (
              <Popconfirm
                title="删除此会话规则？后续消息将继承全局策略。"
                onConfirm={() => void removePrincipal(row.id)}
              >
                <Button type="text" danger icon={<Trash2 size={15} />}>
                  删除
                </Button>
              </Popconfirm>
            ),
          },
        ]}
        scroll={{ x: 760 }}
      />
    </Card>
  );

  const outbox = (
    <Card
      className="pm-panel"
      title="待发送"
      extra={<Text type="secondary">按 OAuth 真实 ID 精确发送</Text>}
    >
      <List
        dataSource={pendingDrafts}
        locale={{ emptyText: <Empty description="暂无待发送回复" /> }}
        renderItem={(item: Outbox) => (
          <List.Item
            actions={[
              <Button
                key="edit"
                icon={<MessageSquareText size={15} />}
                onClick={() => {
                  setDraftEditor(item);
                  setDraftText(item.text);
                }}
              >
                编辑
              </Button>,
              <Button
                key="send"
                type="primary"
                icon={<Send size={15} />}
                loading={saving}
                onClick={() => void sendDraft(item.id)}
              >
                发送
              </Button>,
              <Popconfirm
                key="delete"
                title="删除草稿？原始消息仍会保留。"
                onConfirm={() => void deleteDraft(item.id)}
              >
                <Button danger type="text" icon={<Trash2 size={15} />}>
                  删除
                </Button>
              </Popconfirm>,
            ]}
          >
            <List.Item.Meta
              title={
                <div className="pm-item-title">
                  <span>{item.conversation_alias}</span>
                  <StatusTag status={item.status} />
                </div>
              }
              description={
                <>
                  <div className="pm-source">
                    <div className="pm-source-head">
                      <strong>
                        {item.source_display_name || item.conversation_alias}
                      </strong>
                      <Text type="secondary">
                        {item.source_subject_type === "group"
                          ? "群聊消息"
                          : "单聊消息"}
                      </Text>
                    </div>
                    <div className="pm-message-stack">
                      {(item.source_messages || []).map((message) => (
                        <div className="pm-message" key={message.id}>
                          {message.text}
                          <div className="pm-meta">
                            {time(message.received_at)}
                          </div>
                        </div>
                      ))}
                    </div>
                  </div>
                  <div className="pm-draft">
                    <Text type="secondary">准备发送的回复</Text>
                    <p className="pm-pre">{item.text}</p>
                  </div>
                  {item.error ? (
                    <div className="pm-error">{item.error}</div>
                  ) : null}
                  <div className="pm-meta">{time(item.updated_at)}</div>
                </>
              }
            />
          </List.Item>
        )}
      />
    </Card>
  );

  const activity = (
    <Card className="pm-panel" title="运行记录">
      <Timeline
        items={(data?.activity || []).map((item) => ({
          color:
            item.status === "failed"
              ? "red"
              : item.status === "sent" || item.status === "verified"
              ? "green"
              : "blue",
          children: (
            <div>
              <div className="pm-item-title">
                <strong>{item.title}</strong>
                <Tag>{item.status}</Tag>
              </div>
              {item.detail ? (
                <div className="pm-subtle">{item.detail}</div>
              ) : null}
              <div className="pm-meta">{time(item.created_at)}</div>
            </div>
          ),
        }))}
      />
    </Card>
  );

  return (
    <div className="pm-page">
      <style>{styles}</style>
      <header className="pm-header">
        <div className="pm-header-copy">
          <div className="pm-eyebrow">
            <ShieldCheck size={15} />
            Paw Me · Digital Twin
          </div>
          <Title level={1}>钉钉数字人分身</Title>
          <Text type="secondary">
            使用所选 Agent 和本机钉钉 OAuth 登录态，在一个页面完成实时收件、
            独立授权、上下文聚合、处理、草稿、发送与审计。
          </Text>
        </div>
        <div className="pm-actions">
          <Select
            value={agentId}
            style={{ minWidth: 190 }}
            options={agents.map((agent) => ({
              value: agent.id,
              label: `${agent.name || agent.id} · ${agent.backend || "agent"}`,
            }))}
            onChange={(value: string) => void selectAgent(value)}
          />
          <Button icon={<Settings2 size={16} />} onClick={openSettings}>
            设置
          </Button>
          <Button
            icon={<RefreshCw size={16} />}
            onClick={() => void refresh(agentId)}
          >
            刷新
          </Button>
          <Space>
            <Switch
              checked={data?.settings.enabled}
              disabled={!oauthReady || !profileApproved}
              onChange={(value: boolean) => void toggleRuntime(value)}
            />
            <Text>{data?.settings.enabled ? "运行中" : "已停止"}</Text>
          </Space>
        </div>
      </header>

      {error ? (
        <Alert
          closable
          type="error"
          message="操作未完成"
          description={error}
          onClose={() => setError("")}
          style={{ marginBottom: 16 }}
        />
      ) : null}

      <Card className="pm-statusbar">
        <div className="pm-status-inner">
          <div className="pm-status-main">
            {data?.runtime.running ? (
              <Badge status="processing" />
            ) : (
              <PauseCircle size={18} />
            )}
            <div className="pm-status-text">
              <div className="pm-status-title">
                {data?.runtime.stage || "stopped"}
              </div>
              <Text className="pm-status-detail" type="secondary">
                {data?.runtime.detail || "等待启动"}
              </Text>
            </div>
          </div>
          <Space wrap>
            <Tag
              icon={<ShieldCheck size={13} />}
              color={oauthReady ? "success" : "warning"}
            >
              {oauthReady ? "钉钉 OAuth 已连接" : "等待钉钉 OAuth"}
            </Tag>
            <Tag icon={<Clock3 size={13} />}>
              静默 {data?.settings.quiet_seconds ?? 4} 秒
            </Tag>
            {data?.runtime.current_conversation ? (
              <Tag icon={<MessageSquareText size={13} />}>
                {data.runtime.current_conversation}
              </Tag>
            ) : null}
          </Space>
        </div>
      </Card>

      <Card
        className="pm-panel"
        title="本人画像与人物关系"
        extra={
          <Tag color={profileApproved ? "success" : "warning"}>
            {profileApproved ? "已审核" : "启用前需审核"}
          </Tag>
        }
      >
        <Alert
          type={profile?.status === "failed" ? "error" : "info"}
          showIcon
          message={data?.runtime.profile_detail || "等待初始化"}
          description="首次初始化和后台定期更新才访问钉钉；日常回复只读取本地快照。不会保存他人的私聊正文，也不会推断私人关系。"
        />
        {profileBusy ? (
          <div className="pm-profile-progress">
            <Progress
              percent={data?.runtime.profile_progress ?? 0}
              status="active"
            />
          </div>
        ) : null}
        <div className="pm-profile-grid" style={{ marginTop: 16 }}>
          <div className="pm-profile-facts">
            <Descriptions column={1} size="small" bordered>
              <Descriptions.Item label="本人">
                {profile?.collected.identity?.name || "待采集"}
              </Descriptions.Item>
              <Descriptions.Item label="部门">
                {profile?.collected.identity?.departments?.join("、") || "—"}
              </Descriptions.Item>
              <Descriptions.Item label="职位 / 角色">
                {[
                  profile?.collected.identity?.title,
                  ...(profile?.collected.identity?.roles || []),
                ]
                  .filter(Boolean)
                  .join(" · ") || "—"}
              </Descriptions.Item>
              <Descriptions.Item label="表达样本">
                {profile?.collected.work_style?.message_count || 0} 条本人消息
              </Descriptions.Item>
              <Descriptions.Item label="最近更新">
                {time(profile?.refreshed_at)}
              </Descriptions.Item>
            </Descriptions>
            {profile?.error ? (
              <Text type="warning">部分数据未完成：{profile.error}</Text>
            ) : null}
          </div>
          <div>
            <Text strong>近期协作关系</Text>
            {(profile?.collected.relationships || []).slice(0, 6).map((row) => (
              <div className="pm-profile-relation" key={row.subject_id}>
                <div>
                  <div>{row.name}</div>
                  <Text type="secondary">
                    互动 {row.interaction_count} 次 · 共同群{" "}
                    {row.shared_group_count} 个
                  </Text>
                </div>
                <Tag>有来源</Tag>
              </div>
            ))}
            {!profile?.collected.relationships?.length ? (
              <Empty
                image={Empty.PRESENTED_IMAGE_SIMPLE}
                description="暂无关系数据"
              />
            ) : null}
            <Input.TextArea
              className="pm-profile-note"
              rows={3}
              value={profileNotes}
              placeholder="可补充：我的职责、做事方式、称呼习惯，以及明确的人物关系。"
              onChange={(event: React.ChangeEvent<HTMLTextAreaElement>) =>
                setProfileNotes(event.target.value)
              }
            />
          </div>
        </div>
        <div className="pm-profile-actions">
          <Button
            type="primary"
            icon={<UserCheck size={16} />}
            disabled={
              profileBusy ||
              !["ready", "partial", "stale"].includes(profile?.status || "")
            }
            onClick={() => void approveProfile()}
          >
            审核并保存画像
          </Button>
          <Button
            icon={<RefreshCw size={16} />}
            disabled={profileBusy}
            onClick={() => void refreshProfile()}
          >
            立即更新
          </Button>
          {profileBusy ? (
            <Button icon={<X size={16} />} onClick={() => void cancelProfile()}>
              取消更新
            </Button>
          ) : null}
        </div>
      </Card>

      <Card className="pm-global" title="全局访问与回复策略">
        <div className="pm-global-grid">
          <div className="pm-global-field">
            <div className="pm-global-label">新会话默认访问规则</div>
            <Select
              value={data?.settings.access_mode || "approval"}
              options={[
                {
                  value: "approval",
                  label: "逐个审批（推荐）",
                },
                { value: "allow_all", label: "全白名单" },
                { value: "block_all", label: "全黑名单" },
              ]}
              onChange={(value: string) =>
                void saveGlobalSetting("access_mode", value)
              }
            />
            <Text type="secondary">
              单会话规则始终优先；删除单会话规则后恢复继承全局。
            </Text>
          </div>
          <div className="pm-global-field">
            <div className="pm-global-label">允许回复时的默认方式</div>
            <Select
              value={data?.settings.default_policy || "draft"}
              options={[
                { value: "draft", label: "先进入待发送" },
                { value: "automatic", label: "生成后自动发送" },
              ]}
              onChange={(value: string) =>
                void saveGlobalSetting("default_policy", value)
              }
            />
            <Text type="secondary">
              即使选择自动发送，身份泄漏或元分析也会强制留在草稿。
            </Text>
          </div>
        </div>
      </Card>

      <Row gutter={[14, 14]}>
        <Col xs={12} lg={6}>
          <Card className="pm-metric">
            <div className="pm-metric-icon">
              <Inbox size={18} />
            </div>
            <div>
              <div className="pm-metric-value">
                {data?.work_items.length || 0}
              </div>
              <div className="pm-metric-label">消息批次</div>
            </div>
          </Card>
        </Col>
        <Col xs={12} lg={6}>
          <Card className="pm-metric">
            <div className="pm-metric-icon">
              <LockKeyhole size={18} />
            </div>
            <div>
              <div className="pm-metric-value">{identityRequired.length}</div>
              <div className="pm-metric-label">待绑定身份</div>
            </div>
          </Card>
        </Col>
        <Col xs={12} lg={6}>
          <Card className="pm-metric">
            <div className="pm-metric-icon">
              <Send size={18} />
            </div>
            <div>
              <div className="pm-metric-value">{pendingDrafts.length}</div>
              <div className="pm-metric-label">待发送</div>
            </div>
          </Card>
        </Col>
        <Col xs={12} lg={6}>
          <Card className="pm-metric">
            <div className="pm-metric-icon">
              <Activity size={18} />
            </div>
            <div>
              <div className="pm-metric-value">
                {data?.principals.length || 0}
              </div>
              <div className="pm-metric-label">已验证身份</div>
            </div>
          </Card>
        </Col>
      </Row>

      <Tabs
        defaultActiveKey="inbox"
        items={[
          {
            key: "inbox",
            label: (
              <Space>
                <Inbox size={15} />
                收件与处理
              </Space>
            ),
            children: inbox,
          },
          {
            key: "permissions",
            label: (
              <Space>
                <UserCheck size={15} />
                身份与权限
              </Space>
            ),
            children: permissions,
          },
          {
            key: "outbox",
            label: (
              <Space>
                <Send size={15} />
                待发送
              </Space>
            ),
            children: outbox,
          },
          {
            key: "activity",
            label: (
              <Space>
                <Activity size={15} />
                运行记录
              </Space>
            ),
            children: activity,
          },
        ]}
      />

      <Drawer
        title="运行设置"
        width={420}
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        destroyOnClose
        extra={
          <Button
            type="primary"
            loading={saving}
            onClick={() => settingsForm.submit()}
          >
            保存
          </Button>
        }
      >
        <Form
          form={settingsForm}
          layout="vertical"
          onFinish={saveSettings}
          initialValues={data?.settings}
        >
          <Form.Item
            name="agent_id"
            label="回复消息的 Agent"
            rules={[{ required: true }]}
          >
            <Select
              options={agents.map((agent) => ({
                value: agent.id,
                label: `${agent.name || agent.id} · ${
                  agent.backend || "agent"
                }`,
              }))}
            />
          </Form.Item>
          <Form.Item
            name="enabled"
            label="数字人分身总开关"
            valuePropName="checked"
          >
            <Switch />
          </Form.Item>
          <Form.Item name="default_policy" label="默认回复策略">
            <Select
              options={[
                { value: "draft", label: "生成草稿，确认后发送" },
                { value: "automatic", label: "按身份策略自动发送" },
              ]}
            />
          </Form.Item>
          <Form.Item name="access_mode" label="新会话默认访问规则">
            <Select
              options={[
                { value: "approval", label: "逐个审批" },
                { value: "allow_all", label: "全白名单" },
                { value: "block_all", label: "全黑名单" },
              ]}
            />
          </Form.Item>
          <Form.Item
            name="quiet_seconds"
            label="连续消息静默窗口（秒）"
            extra="对方停止输入达到这个时间后，才合并调用一次 Agent。"
          >
            <InputNumber min={1} max={30} style={{ width: "100%" }} />
          </Form.Item>
          <Form.Item
            name="max_wait_seconds"
            label="最长聚合等待（秒）"
            extra="持续聊天时也不会无限等待。"
          >
            <InputNumber min={3} max={120} style={{ width: "100%" }} />
          </Form.Item>
          <Alert
            type="info"
            showIcon
            message="上下文不会因中断丢失"
            description="每条原始消息先写入 SQLite。Agent 运行中新消息到达时，旧任务会停止，新任务在同一会话中携带完整批次继续。"
          />
        </Form>
      </Drawer>

      <Modal
        title="授权真实钉钉会话"
        open={Boolean(authorizationItem)}
        confirmLoading={saving}
        onCancel={() => setAuthorizationItem(null)}
        onOk={() => authorizationForm.submit()}
        destroyOnClose
      >
        <Alert
          type="info"
          showIcon
          message="ID 已由钉钉 OAuth 事件验证"
          description="下列 ID 为只读值，不能手填或修改。授权后，相同真实 ID 的后续消息会按所选策略处理。"
          style={{ marginBottom: 16 }}
        />
        <Descriptions
          size="small"
          column={1}
          bordered
          style={{ marginBottom: 18 }}
          items={[
            {
              key: "name",
              label: "会话",
              children:
                authorizationItem?.display_name ||
                authorizationItem?.conversation_alias ||
                "—",
            },
            {
              key: "type",
              label: "类型",
              children:
                authorizationItem?.subject_type === "group" ? "群聊" : "人员",
            },
            {
              key: "id",
              label: "真实 ID",
              children: (
                <span className="pm-id">
                  {authorizationItem?.subject_id || "—"}
                </span>
              ),
            },
            {
              key: "source",
              label: "来源",
              children: sourceLabel(authorizationItem?.id_source),
            },
          ]}
        />
        <Form
          form={authorizationForm}
          layout="vertical"
          onFinish={saveAuthorization}
        >
          <Form.Item
            name="policy"
            label="权限策略"
            rules={[{ required: true }]}
          >
            <Select
              options={[
                { value: "draft", label: "允许处理，生成草稿" },
                { value: "automatic", label: "允许处理并自动发送" },
                { value: "observe", label: "仅观察，不调用 Agent" },
                { value: "blocked", label: "阻止" },
              ]}
            />
          </Form.Item>
        </Form>
      </Modal>

      <Modal
        title={`编辑发给 ${draftEditor?.conversation_alias || ""} 的草稿`}
        open={Boolean(draftEditor)}
        confirmLoading={saving}
        okButtonProps={{ disabled: !draftText.trim() }}
        onCancel={() => setDraftEditor(null)}
        onOk={() => void saveDraft()}
        destroyOnClose
      >
        <Input.TextArea
          autoSize={{ minRows: 6, maxRows: 16 }}
          value={draftText}
          onChange={(event: ReactNS.ChangeEvent<HTMLTextAreaElement>) =>
            setDraftText(event.target.value)
          }
        />
      </Modal>
    </div>
  );
}

const paw = window.QwenPaw.paw?.forApp(APP_ID);
if (paw) {
  paw.ui.registerPage({
    path: "/apps/paw-me-dingtalk",
    label: "Paw Me · DingTalk",
    component: PawMePage,
  });
} else {
  window.QwenPaw.registerRoutes?.(APP_ID, [
    {
      path: "/apps/paw-me-dingtalk",
      component: PawMePage,
      label: "Paw Me · DingTalk",
    },
  ]);
}
