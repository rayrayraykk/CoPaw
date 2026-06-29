const qwenpaw = (window as any).QwenPaw;
if (!qwenpaw?.host?.React || !qwenpaw?.host?.antd) {
  throw new Error("window.QwenPaw.host not found");
}

const { React, antd, antdIcons } = qwenpaw.host;
const {
  Card,
  Button,
  Alert,
  Typography,
  Descriptions,
  Modal,
  Checkbox,
  Input,
  Space,
  Divider,
} = antd;
const { LoginOutlined, LikeOutlined, MehOutlined, DislikeOutlined } = antdIcons;
const { Text: AntText } = Typography;

const INTEGRATION_B_BASE = "https://proxy.agentscope.design";
const INTEGRATION_CLIENT_SECRET = "qwenpaw-proxy-v1.0";

const QWENPAW_AUTH_TOKEN_KEY = "qwenpaw_auth_token";

const PLUGIN_ROUTE_ID = "dogfooding-bundle";
const DOGFOODING_META_KEY = "qwenpaw_dogfooding";
const FEEDBACK_STORAGE_KEY = "dogfooding_feedback_submitted";

const BAD_FEEDBACK_REASONS = [
  "没理解我的意图",
  "任务没有完成",
  "步骤太繁琐",
  "结果有误",
  "回复风格有问题",
  "存在安全风险",
  "响应太慢",
  "其他",
] as const;

type ScoreLabel = "bad" | "fine" | "good";

interface DogfoodingMeta {
  trace_id?: string;
  session_id?: string;
  model_id?: string;
  response_id?: string;
}

interface FeedbackSubmitPayload {
  trace_id: string;
  conversation_id: string;
  score_label: ScoreLabel;
  channel_type: string;
  feedback_reason?: string;
  feedback_comment?: string;
  response_id?: string;
}

console.info(`[${PLUGIN_ROUTE_ID}] frontend runtime detected`);

interface SsoTokenResponse {
  proxyApiKey?: string | null;
  name?: string | null;
  account?: string | null;
}
type PersistNotice =
  | null
  | { kind: "success"; path?: string; providerConfigured?: boolean }
  | { kind: "skipped"; reason: string }
  | { kind: "error"; message: string; scope?: "account" | "provider" };

interface LookupUserResponse {
  name?: string | null;
  account?: string | null;
  proxyApiKey?: string | null;
}

interface SsoInitResponse {
  state?: string;
  loginUrl?: string;
}

function formatFastApiErrorBody(parsed: unknown, fallback: string): string {
  if (!parsed || typeof parsed !== "object" || parsed === null) {
    return fallback;
  }
  const o = parsed as Record<string, unknown>;
  const { detail } = o;
  if (typeof detail === "string") return detail;
  if (Array.isArray(detail)) {
    const parts = detail.map((item) => {
      if (item && typeof item === "object" && "msg" in item) {
        return String((item as { msg?: unknown }).msg);
      }
      return JSON.stringify(item);
    });
    return parts.filter(Boolean).join("; ") || fallback;
  }
  if (typeof o.message === "string") return o.message;
  return fallback;
}
async function initIntegrationSsoLogin(redirectUri: string): Promise<string> {
  const url = `${INTEGRATION_B_BASE.replace(
    /\/$/,
    "",
  )}/v1/integration/sso/init`;
  const response = await fetch(url, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-Integration-Client-Secret": INTEGRATION_CLIENT_SECRET,
    },
    body: JSON.stringify({ redirectUri }),
  });
  const text = await response.text();
  let parsed: unknown = null;
  if (text) {
    try {
      parsed = JSON.parse(text);
    } catch {
      parsed = null;
    }
  }
  if (!response.ok) {
    const fallback = text || `HTTP ${response.status}`;
    throw new Error(formatFastApiErrorBody(parsed, fallback));
  }
  const body = (
    parsed && typeof parsed === "object" ? parsed : {}
  ) as SsoInitResponse;
  const loginUrl = body.loginUrl?.trim();
  if (!loginUrl) {
    throw new Error("SSO init 未返回 loginUrl");
  }
  return loginUrl;
}

async function exchangeIntegrationSsoToken(
  code: string,
  state: string,
): Promise<SsoTokenResponse> {
  const url = `${INTEGRATION_B_BASE.replace(
    /\/$/,
    "",
  )}/v1/integration/sso/token`;
  const response = await fetch(url, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-Integration-Client-Secret": INTEGRATION_CLIENT_SECRET,
    },
    body: JSON.stringify({ code, state }),
  });
  const text = await response.text();
  let parsed: unknown = null;
  if (text) {
    try {
      parsed = JSON.parse(text);
    } catch {
      parsed = null;
    }
  }
  if (!response.ok) {
    const fallback = text || `HTTP ${response.status}`;
    throw new Error(formatFastApiErrorBody(parsed, fallback));
  }
  return (
    parsed && typeof parsed === "object" ? parsed : {}
  ) as SsoTokenResponse;
}

/** 发起 SSO 时传给后端的 redirectUri，去掉可能残留的 code/state */
function buildRedirectUriForSsoInit(): string {
  const u = new URL(window.location.href);
  u.searchParams.delete("code");
  u.searchParams.delete("state");
  return u.toString();
}

/** 从当前地址读取查询串：优先 ?query，其次 #/path?query（部分 SPA 把回调写在 hash 里） */
function getOAuthQueryParams(): URLSearchParams | null {
  const { search, hash } = window.location;
  if (search && search.length > 1) {
    return new URLSearchParams(search);
  }
  const q = hash.indexOf("?");
  if (q !== -1) {
    return new URLSearchParams(hash.slice(q + 1));
  }
  return null;
}

function readSsoCallbackFromUrl(): { code: string; state: string } | null {
  const sp = getOAuthQueryParams();
  if (!sp) return null;
  const code = sp.get("code")?.trim() ?? "";
  const state = sp.get("state")?.trim() ?? "";
  if (!code || !state) return null;
  return { code, state };
}

/** 从地址栏去掉 code/state（search 与 hash 内 query 都会处理） */
function stripSsoCallbackParamsFromUrl(): void {
  const u = new URL(window.location.href);
  let changed = false;

  if (u.searchParams.has("code") || u.searchParams.has("state")) {
    u.searchParams.delete("code");
    u.searchParams.delete("state");
    changed = true;
  }

  const h = u.hash;
  const qIdx = h.indexOf("?");
  if (qIdx !== -1) {
    const qp = new URLSearchParams(h.slice(qIdx + 1));
    if (qp.has("code") || qp.has("state")) {
      qp.delete("code");
      qp.delete("state");
      const pathPart = h.slice(0, qIdx);
      const rest = qp.toString();
      u.hash = rest ? `${pathPart}?${rest}` : pathPart;
      changed = true;
    }
  }

  if (!changed) return;

  const q = u.searchParams.toString();
  const searchStr = q ? `?${q}` : "";
  window.history.replaceState(
    {},
    "",
    `${u.origin}${u.pathname}${searchStr}${u.hash}`,
  );
}

interface DogfoodingAccountSaveResponse {
  ok: boolean;
  path: string;
  provider_configured?: boolean;
}

interface DogfoodingProviderConfigResponse {
  ok: boolean;
  provider_id: string;
}

/** 与 DogfoodingAccountPayload 一致：字段名 user_account，非空由调用方保证 */
function buildQwenPawApiHeaders(): Record<string, string> {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };
  try {
    const token = localStorage.getItem(QWENPAW_AUTH_TOKEN_KEY);
    if (token) {
      headers.Authorization = `Bearer ${token}`;
    }
  } catch {
    /* ignore */
  }
  return headers;
}

async function saveDogfoodingUserAccount(
  userAccount: string,
  proxyApiKey?: string,
): Promise<DogfoodingAccountSaveResponse> {
  const url = new URL("/api/dogfooding-account/", window.location.origin).href;
  const body: Record<string, string> = { user_account: userAccount };
  const trimmedKey = proxyApiKey?.trim();
  if (trimmedKey) {
    body.proxy_api_key = trimmedKey;
  }
  const response = await fetch(url, {
    method: "POST",
    headers: buildQwenPawApiHeaders(),
    body: JSON.stringify(body),
  });
  const text = await response.text();
  let parsed: unknown = null;
  if (text) {
    try {
      parsed = JSON.parse(text);
    } catch {
      parsed = null;
    }
  }
  if (!response.ok) {
    const fallback = text || `HTTP ${response.status}`;
    throw new Error(formatFastApiErrorBody(parsed, fallback));
  }
  const saved = parsed as DogfoodingAccountSaveResponse | null;
  if (
    !saved ||
    typeof saved.ok !== "boolean" ||
    saved.ok !== true ||
    typeof saved.path !== "string"
  ) {
    throw new Error("保存接口返回格式异常（期望 { ok: true, path: string }）");
  }
  return saved;
}

async function configureDogfoodingProviderApiKey(
  proxyApiKey: string,
): Promise<DogfoodingProviderConfigResponse> {
  const url = new URL(
    "/api/dogfooding-account/configure-provider",
    window.location.origin,
  ).href;
  const response = await fetch(url, {
    method: "POST",
    headers: buildQwenPawApiHeaders(),
    body: JSON.stringify({ proxy_api_key: proxyApiKey.trim() }),
  });
  const text = await response.text();
  let parsed: unknown = null;
  if (text) {
    try {
      parsed = JSON.parse(text);
    } catch {
      parsed = null;
    }
  }
  if (!response.ok) {
    const fallback = text || `HTTP ${response.status}`;
    throw new Error(formatFastApiErrorBody(parsed, fallback));
  }
  const body = parsed as DogfoodingProviderConfigResponse | null;
  if (
    !body ||
    typeof body.ok !== "boolean" ||
    body.ok !== true ||
    typeof body.provider_id !== "string"
  ) {
    throw new Error(
      "Provider 配置接口返回格式异常（期望 { ok: true, provider_id: string }）",
    );
  }
  return body;
}

async function persistDogfoodingLoginResult(
  account: string | null | undefined,
  proxyApiKey: string | null | undefined,
): Promise<PersistNotice> {
  const accountTrim = account?.trim() ?? "";
  const apiKeyTrim = proxyApiKey?.trim() ?? "";

  if (!accountTrim && !apiKeyTrim) {
    return {
      kind: "skipped",
      reason: "SSO 返回中无工号与 API Key，已跳过写入",
    };
  }

  let path: string | undefined;
  let providerConfigured = false;

  if (accountTrim) {
    try {
      const saved = await saveDogfoodingUserAccount(
        accountTrim,
        apiKeyTrim || undefined,
      );
      path = saved.path;
      providerConfigured = Boolean(saved.provider_configured);
    } catch (err) {
      return {
        kind: "error",
        scope: "account",
        message:
          err instanceof Error ? err.message : "调用本机保存工号接口失败",
      };
    }
  }

  if (apiKeyTrim && !providerConfigured) {
    try {
      await configureDogfoodingProviderApiKey(apiKeyTrim);
      providerConfigured = true;
    } catch (err) {
      return {
        kind: "error",
        scope: "provider",
        message:
          err instanceof Error
            ? err.message
            : "写入 AgentScope Dogfooding 模型配置失败",
      };
    }
  }

  return {
    kind: "success",
    path,
    providerConfigured,
  };
}

function readSubmittedFeedback(): Record<string, ScoreLabel> {
  try {
    const raw = localStorage.getItem(FEEDBACK_STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw);
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

function markFeedbackSubmitted(traceId: string, label: ScoreLabel): void {
  const map = readSubmittedFeedback();
  map[traceId] = label;
  try {
    localStorage.setItem(FEEDBACK_STORAGE_KEY, JSON.stringify(map));
  } catch {
    /* ignore */
  }
}

function extractDogfoodingMeta(
  data: Record<string, unknown>,
): DogfoodingMeta | null {
  const output = data?.output;
  if (!Array.isArray(output)) return null;
  for (let i = output.length - 1; i >= 0; i -= 1) {
    const item = output[i] as Record<string, unknown> | undefined;
    const metadata = item?.metadata as Record<string, unknown> | undefined;
    if (!metadata) continue;
    // The host wraps the assistant message metadata one level deeper on the
    // response card (item.metadata.metadata), so check both shapes.
    const nested = metadata.metadata as Record<string, unknown> | undefined;
    const meta =
      (metadata[DOGFOODING_META_KEY] as DogfoodingMeta | undefined) ||
      (nested?.[DOGFOODING_META_KEY] as DogfoodingMeta | undefined);
    if (meta?.trace_id) return meta;
  }
  return null;
}

function isDogfoodingResponse(data: Record<string, unknown>): boolean {
  const meta = extractDogfoodingMeta(data);
  if (meta?.trace_id) return true;
  const usage = data?.usage as Record<string, unknown> | undefined;
  const modelName = String(usage?.model_name || "").toLowerCase();
  return modelName.includes("dogfooding");
}

// Stable per-message identifier, used to remember the submitted feedback
// state for replies that don't yet carry the trace_id meta (live-streamed
// replies only get the persisted meta after the turn finalizes).
function responseMessageKey(data: Record<string, unknown>): string {
  const output = data?.output;
  if (Array.isArray(output)) {
    for (let i = output.length - 1; i >= 0; i -= 1) {
      const item = output[i] as Record<string, unknown> | undefined;
      const oid = item?.original_id;
      if (typeof oid === "string" && oid) return oid;
    }
  }
  return String(data?.id || "");
}

async function submitDogfoodingFeedback(
  payload: FeedbackSubmitPayload,
): Promise<void> {
  const response = await fetch("/api/dogfooding-feedback/", {
    method: "POST",
    headers: buildQwenPawApiHeaders(),
    body: JSON.stringify(payload),
  });
  const text = await response.text();
  let parsed: unknown = null;
  if (text) {
    try {
      parsed = JSON.parse(text);
    } catch {
      parsed = null;
    }
  }
  if (!response.ok) {
    const fallback = text || `HTTP ${response.status}`;
    throw new Error(formatFastApiErrorBody(parsed, fallback));
  }
}

function FeedbackBar({ data }: { data: Record<string, unknown> }) {
  const meta = extractDogfoodingMeta(data);
  const traceId = meta?.trace_id || "";
  const conversationId =
    meta?.session_id || qwenpaw.host.getCurrentSessionId?.() || "";
  // Remember the submitted state per message. Prefer the real trace_id, but
  // fall back to a stable message id so freshly-streamed replies (whose meta
  // isn't persisted yet) still behave correctly.
  const feedbackKey = traceId || responseMessageKey(data);
  const [submitting, setSubmitting] = React.useState(false);
  const [submittedLabel, setSubmittedLabel] = React.useState(
    null as ScoreLabel | null,
  );
  const [error, setError] = React.useState("");
  const [reasonOpen, setReasonOpen] = React.useState(false);
  const [selectedReasons, setSelectedReasons] = React.useState([] as string[]);
  const [comment, setComment] = React.useState("");

  React.useEffect(() => {
    if (!feedbackKey) return;
    const saved = readSubmittedFeedback()[feedbackKey];
    if (saved) setSubmittedLabel(saved);
  }, [feedbackKey]);

  // Show the bar for any dogfooding reply. The trace_id may be missing on a
  // just-streamed reply (it's attached to the persisted message at turn
  // finalize); in that case the backend backfills it from the conversation.
  if (!isDogfoodingResponse(data) || !conversationId) {
    return null;
  }

  const submit = async (label: ScoreLabel, reason = "", extraComment = "") => {
    setSubmitting(true);
    setError("");
    try {
      await submitDogfoodingFeedback({
        trace_id: traceId,
        conversation_id: String(conversationId),
        score_label: label,
        channel_type: "web",
        feedback_reason: reason,
        feedback_comment: extraComment,
        response_id: meta?.response_id || responseMessageKey(data),
      });
      markFeedbackSubmitted(feedbackKey, label);
      setSubmittedLabel(label);
      setReasonOpen(false);
      setSelectedReasons([]);
      setComment("");
    } catch (err) {
      setError(err instanceof Error ? err.message : "反馈提交失败");
    } finally {
      setSubmitting(false);
    }
  };

  const onPickScore = (label: ScoreLabel) => {
    if (submittedLabel || submitting) return;
    if (label === "bad") {
      setReasonOpen(true);
      return;
    }
    void submit(label);
  };

  const onConfirmBad = () => {
    if (!selectedReasons.length) {
      setError("请至少选择一个问题原因");
      return;
    }
    void submit("bad", selectedReasons.join("；"), comment.trim());
  };

  const labelText =
    submittedLabel === "good"
      ? "优秀"
      : submittedLabel === "fine"
      ? "一般"
      : submittedLabel === "bad"
      ? "糟糕"
      : "";

  return (
    <div style={{ marginTop: 8 }}>
      <Divider style={{ margin: "8px 0" }} />
      <div style={{ fontSize: 13, color: "#666", marginBottom: 8 }}>
        这个回答对你有帮助吗？
      </div>
      {submittedLabel ? (
        <Alert
          type="success"
          showIcon
          message={`已反馈：${labelText}`}
          style={{ marginBottom: 0 }}
        />
      ) : (
        <Space wrap>
          <Button
            icon={<DislikeOutlined />}
            loading={submitting}
            onClick={() => onPickScore("bad")}
          >
            糟糕
          </Button>
          <Button
            icon={<MehOutlined />}
            loading={submitting}
            onClick={() => onPickScore("fine")}
          >
            一般
          </Button>
          <Button
            icon={<LikeOutlined />}
            loading={submitting}
            onClick={() => onPickScore("good")}
          >
            优秀
          </Button>
        </Space>
      )}
      {error ? (
        <Alert style={{ marginTop: 8 }} type="error" showIcon message={error} />
      ) : null}
      <Modal
        title="请告诉我们哪里不好"
        open={reasonOpen}
        okText="提交反馈"
        cancelText="取消"
        confirmLoading={submitting}
        onOk={onConfirmBad}
        onCancel={() => {
          setReasonOpen(false);
          setSelectedReasons([]);
          setComment("");
          setError("");
        }}
      >
        <Checkbox.Group
          style={{ display: "flex", flexDirection: "column", gap: 8 }}
          value={selectedReasons}
          onChange={(values) => setSelectedReasons(values as string[])}
        >
          {BAD_FEEDBACK_REASONS.map((reason) => (
            <Checkbox key={reason} value={reason}>
              {reason}
            </Checkbox>
          ))}
        </Checkbox.Group>
        <Input.TextArea
          style={{ marginTop: 12 }}
          rows={3}
          placeholder="补充说明（可选）"
          value={comment}
          onChange={(e: { target: { value: string } }) =>
            setComment(e.target.value)
          }
        />
      </Modal>
    </div>
  );
}

function formatCell(v: string | null | undefined): string {
  if (v === null || v === undefined || v === "") return "—";
  return String(v);
}

/** 列表展示用：保留前缀与末尾，中间脱敏（复制仍用完整密钥） */
function maskProxyApiKey(key: string): string {
  const t = key.trim();
  if (!t) return "";
  if (t.length <= 11) return `${t.slice(0, 4)}****`;
  const prefixLen = t.startsWith("sk-as-") ? 10 : 6;
  const suffixLen = 4;
  const stars = "*".repeat(
    Math.min(12, Math.max(4, t.length - prefixLen - suffixLen)),
  );
  return `${t.slice(0, prefixLen)}${stars}${t.slice(-suffixLen)}`;
}

function DogfoodingJoinPage() {
  const [loginLoading, setLoginLoading] = React.useState(false);
  const [loginError, setLoginError] = React.useState("");
  const [ssoCallbackLoading, setSsoCallbackLoading] = React.useState(false);
  const [ssoCallbackError, setSsoCallbackError] = React.useState("");
  const [lookupResult, setLookupResult] = React.useState(
    null as LookupUserResponse | null,
  );
  const [persistNotice, setPersistNotice] = React.useState(
    null as PersistNotice,
  );

  /** 登录回调：URL 带 code、state 时换 token 并展示密钥 / 花名 / 工号 */
  React.useEffect(() => {
    const params = readSsoCallbackFromUrl();
    if (!params) return undefined;

    const dedupeKey = `dogfooding_sso:${params.state}`;
    try {
      const prev = sessionStorage.getItem(dedupeKey);
      if (prev === "done" || prev === "pending") return undefined;
      sessionStorage.setItem(dedupeKey, "pending");
    } catch {
      /* 无痕模式等可能不可用，忽略防重 */
    }

    let cancelled = false;
    (async () => {
      setSsoCallbackLoading(true);
      setSsoCallbackError("");
      try {
        const data = await exchangeIntegrationSsoToken(
          params.code,
          params.state,
        );
        if (cancelled) return;

        stripSsoCallbackParamsFromUrl();

        const proxyApiKey = data.proxyApiKey?.trim() ?? "";
        const name = data.name ?? null;
        const account = data.account ?? null;

        setLookupResult({
          name,
          account,
          proxyApiKey: proxyApiKey || null,
        });

        const notice = await persistDogfoodingLoginResult(account, proxyApiKey);
        if (!cancelled) {
          setPersistNotice(notice);
        }
        try {
          sessionStorage.setItem(dedupeKey, "done");
        } catch {
          /* ignore */
        }
      } catch (error) {
        try {
          sessionStorage.removeItem(dedupeKey);
        } catch {
          /* ignore */
        }
        if (!cancelled) {
          setSsoCallbackError(
            error instanceof Error ? error.message : "SSO token 交换失败",
          );
        }
      } finally {
        if (!cancelled) setSsoCallbackLoading(false);
      }
    })();

    return () => {
      cancelled = true;
      try {
        if (sessionStorage.getItem(dedupeKey) === "pending") {
          sessionStorage.removeItem(dedupeKey);
        }
      } catch {
        /* ignore */
      }
    };
  }, []);

  const handleAlibabaLogin = async () => {
    setLoginLoading(true);
    setLoginError("");
    try {
      const redirectUri = buildRedirectUriForSsoInit();
      const loginUrl = await initIntegrationSsoLogin(redirectUri);
      window.location.assign(loginUrl);
    } catch (error) {
      setLoginError(
        error instanceof Error ? error.message : "发起集团账号登录失败",
      );
    } finally {
      setLoginLoading(false);
    }
  };

  return (
    <div style={{ padding: 24, maxWidth: 820, margin: "0 auto" }}>
      <Card>
        <Button
          type="primary"
          style={{ marginTop: 0, marginBottom: 12 }}
          loading={loginLoading}
          onClick={handleAlibabaLogin}
        >
          阿里集团账号登录
        </Button>

        {loginError ? (
          <Alert
            style={{ marginBottom: 12 }}
            type="error"
            message={loginError}
          />
        ) : null}

        {ssoCallbackLoading ? (
          <Alert
            style={{ marginBottom: 12 }}
            type="info"
            message="正在使用登录回调参数换取 API 密钥…"
          />
        ) : null}

        {ssoCallbackError ? (
          <Alert
            style={{ marginBottom: 12 }}
            type="error"
            message={ssoCallbackError}
          />
        ) : null}

        {lookupResult ? (
          <div style={{ marginTop: 16 }}>
            <Descriptions bordered size="small" column={1}>
              <Descriptions.Item label="API 密钥">
                {lookupResult?.proxyApiKey ? (
                  <AntText code copyable={{ text: lookupResult.proxyApiKey }}>
                    {maskProxyApiKey(lookupResult.proxyApiKey)}
                  </AntText>
                ) : (
                  formatCell(lookupResult?.proxyApiKey)
                )}
              </Descriptions.Item>
              <Descriptions.Item label="花名/姓名">
                {formatCell(lookupResult?.name)}
              </Descriptions.Item>
              <Descriptions.Item label="工号">
                {formatCell(lookupResult?.account)}
              </Descriptions.Item>
            </Descriptions>
            {persistNotice?.kind === "success" ? (
              <>
                {persistNotice.path ? (
                  <Alert
                    type="success"
                    showIcon
                    style={{ marginBottom: 12 }}
                    message="已写入本机 dogfooding 用户文件"
                    description={
                      <AntText code copyable>
                        {persistNotice.path}
                      </AntText>
                    }
                  />
                ) : null}
                {persistNotice.providerConfigured ? (
                  <Alert
                    type="success"
                    showIcon
                    style={{ marginBottom: 12 }}
                    message="API Key 已自动写入 AgentScope Dogfooding 模型配置"
                    description="可在「设置 → 模型」中确认；无需再手动复制粘贴。"
                  />
                ) : null}
              </>
            ) : null}
            {persistNotice?.kind === "skipped" ? (
              <Alert
                type="warning"
                showIcon
                style={{ marginBottom: 12 }}
                message={persistNotice.reason}
              />
            ) : null}
            {persistNotice?.kind === "error" ? (
              <Alert
                type="error"
                showIcon
                style={{ marginBottom: 12 }}
                message={
                  persistNotice.scope === "provider"
                    ? "写入模型配置失败"
                    : "保存工号到本机失败"
                }
                description={persistNotice.message}
              />
            ) : null}
          </div>
        ) : null}
      </Card>
    </div>
  );
}

class DogfoodingBundleFrontend {
  readonly id = PLUGIN_ROUTE_ID;

  setup(): void {
    if (typeof (window as any).QwenPaw.registerRoutes !== "function") {
      console.error(`[${PLUGIN_ROUTE_ID}] registerRoutes is not available`);
      return;
    }
    (window as any).QwenPaw.registerRoutes?.(this.id, [
      {
        path: "/join-dogfooding",
        component: DogfoodingJoinPage,
        label: "Join dogfooding plan",
        icon: <LoginOutlined size={14} />,
        priority: 1,
      },
    ]);

    const chat = (window as any).QwenPaw.chat;
    chat?.response?.append?.(
      PLUGIN_ROUTE_ID,
      ({ data }: { data?: Record<string, unknown> }) => {
        if (!data || typeof data !== "object") return null;
        return <FeedbackBar data={data} />;
      },
      { id: `${PLUGIN_ROUTE_ID}.feedback-bar`, order: 10 },
    );
  }
}

new DogfoodingBundleFrontend().setup();
