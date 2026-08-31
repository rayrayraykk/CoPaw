import type * as ReactNS from "react";
import {
  Activity,
  Clock3,
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
};

type ActivityItem = {
  id: number;
  kind: string;
  status: string;
  title: string;
  detail: string;
  created_at: number;
};

type Snapshot = {
  settings: {
    enabled: boolean;
    agent_id: string;
    default_policy: string;
    quiet_seconds: number;
    max_wait_seconds: number;
  };
  principals: Principal[];
  work_items: WorkItem[];
  outbox: Outbox[];
  activity: ActivityItem[];
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
.pm-policy-grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:12px}.pm-subtle{color:var(--ant-color-text-secondary)}.pm-pre{white-space:pre-wrap;line-height:1.65;margin:0}.pm-error{color:var(--ant-color-error)}.pm-id{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;overflow-wrap:anywhere;font-size:12px}.pm-setup{display:flex;align-items:center;justify-content:space-between;gap:18px;padding:16px;border:1px solid var(--ant-color-border-secondary);border-radius:10px;margin-bottom:16px}.pm-setup-copy{min-width:0}.pm-setup-title{font-weight:650;margin-bottom:4px}
@media(max-width:760px){.pm-page{padding:16px 12px 32px}.pm-header{flex-direction:column}.pm-actions{justify-content:flex-start}.pm-status-inner{align-items:flex-start;flex-direction:column}.pm-status-detail{white-space:normal}.pm-policy-grid{grid-template-columns:1fr}}
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
  sending: "发送中",
  sent: "已发送",
  failed: "失败",
};

function time(value?: number) {
  return value ? new Date(value * 1000).toLocaleString() : "—";
}

function StatusTag({ status }: { status: string }) {
  const color =
    status === "sent"
      ? "success"
      : status === "failed" || status === "blocked"
      ? "error"
      : status === "draft_ready" || status === "identity_required"
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
      setError(reason instanceof Error ? reason.message : "DWS 配置失败");
    } finally {
      setSaving(false);
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
                    {item.id_source || "无可信来源"}
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
              ? "DWS 已安装，等待 OAuth 登录"
              : "安装钉钉官方 DWS"}
          </div>
          <Text type="secondary">
            {data?.runtime.integration_detail ||
              data?.identity_provider.detail ||
              "OAuth 由钉钉官方 DWS 管理，插件不读取或保存令牌。"}
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
            一键安装 DWS
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
          <Button
            icon={<RefreshCw size={16} />}
            onClick={() => void refresh(agentId)}
          >
            刷新登录状态
          </Button>
        )}
      </div>
      <Alert
        showIcon
        type="info"
        message="授权只来自收到的真实事件"
        description="人员 openDingTalkId 或群 openConversationId 由 DWS OAuth 事件写入，界面不可手填。未授权会话统一进入待审核，不会调用 Agent。"
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
                <Text type="secondary">{row.id_source}</Text>
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
                title="删除此身份策略？后续消息将重新进入待授权。"
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
                  <p className="pm-pre">{item.text}</p>
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
              disabled={!oauthReady}
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
              {oauthReady ? "DWS OAuth 已连接" : "等待 DWS OAuth"}
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
          message="ID 已由 DWS OAuth 事件验证"
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
              children: authorizationItem?.id_source || "—",
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
