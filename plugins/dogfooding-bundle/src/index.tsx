const qwenpaw = (window as any).QwenPaw;
if (!qwenpaw?.host?.React || !qwenpaw?.host?.antd) {
  throw new Error("window.QwenPaw.host not found");
}

const { React, antd, antdIcons } = qwenpaw.host;
const { Card, Button, Alert, Typography, Descriptions } = antd;
const { LoginOutlined } = antdIcons;
const { Text: AntText } = Typography;

const INTEGRATION_B_BASE = "https://proxy.agentscope.design";
const INTEGRATION_CLIENT_SECRET = "qwenpaw-proxy-v1.0";

const QWENPAW_AUTH_TOKEN_KEY = "qwenpaw_auth_token";

const PLUGIN_ROUTE_ID = "dogfooding-bundle";

console.info(`[${PLUGIN_ROUTE_ID}] frontend runtime detected`);

interface SsoTokenResponse {
  proxyApiKey?: string | null;
  name?: string | null;
  account?: string | null;
}
type PersistNotice =
  | null
  | { kind: "success"; path: string }
  | { kind: "skipped"; reason: string }
  | { kind: "error"; message: string };

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
): Promise<DogfoodingAccountSaveResponse> {
  const url = new URL("/api/dogfooding-account/", window.location.origin).href;
  const response = await fetch(url, {
    method: "POST",
    headers: buildQwenPawApiHeaders(),
    body: JSON.stringify({ user_account: userAccount }),
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
  const body = parsed as DogfoodingAccountSaveResponse | null;
  if (
    !body ||
    typeof body.ok !== "boolean" ||
    body.ok !== true ||
    typeof body.path !== "string"
  ) {
    throw new Error("保存接口返回格式异常（期望 { ok: true, path: string }）");
  }
  return body;
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

        const accountTrim = account?.trim();
        if (accountTrim) {
          try {
            const saved = await saveDogfoodingUserAccount(accountTrim);
            setPersistNotice({ kind: "success", path: saved.path });
          } catch (err) {
            setPersistNotice({
              kind: "error",
              message:
                err instanceof Error ? err.message : "调用本机保存工号接口失败",
            });
          }
        } else {
          setPersistNotice({
            kind: "skipped",
            reason: "SSO 返回中无工号，已跳过写入本机 dogfooding 用户文件",
          });
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
                message="保存工号到本机失败"
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
  }
}

new DogfoodingBundleFrontend().setup();
