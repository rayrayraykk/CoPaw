const A = window.QwenPaw;
if (!A?.host?.React || !A?.host?.antd)
  throw new Error("window.QwenPaw.host not found");
const { React: o, antd: q, antdIcons: Q } = A.host, { Card: G, Button: x, Alert: p, Typography: H, Descriptions: _, Modal: M, Checkbox: L, Input: z, Space: W, Divider: Y } = q, { LoginOutlined: X, LikeOutlined: V, MehOutlined: Z, DislikeOutlined: ee } = Q, { Text: N } = H, B = "https://proxy.agentscope.design", K = "qwenpaw-proxy-v1.0", te = "qwenpaw_auth_token", b = "dogfooding-bundle", R = "qwenpaw_dogfooding", D = "dogfooding_feedback_submitted", ne = [
  "没理解我的意图",
  "任务没有完成",
  "步骤太繁琐",
  "结果有误",
  "回复风格有问题",
  "存在安全风险",
  "响应太慢",
  "其他"
];
console.info(`[${b}] frontend runtime detected`);
function k(e, t) {
  if (!e || typeof e != "object" || e === null)
    return t;
  const n = e, { detail: r } = n;
  return typeof r == "string" ? r : Array.isArray(r) ? r.map((a) => a && typeof a == "object" && "msg" in a ? String(a.msg) : JSON.stringify(a)).filter(Boolean).join("; ") || t : typeof n.message == "string" ? n.message : t;
}
async function oe(e) {
  const t = `${B.replace(
    /\/$/,
    ""
  )}/v1/integration/sso/init`, n = await fetch(t, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-Integration-Client-Secret": K
    },
    body: JSON.stringify({ redirectUri: e })
  }), r = await n.text();
  let s = null;
  if (r)
    try {
      s = JSON.parse(r);
    } catch {
      s = null;
    }
  if (!n.ok) {
    const l = r || `HTTP ${n.status}`;
    throw new Error(k(s, l));
  }
  const i = (s && typeof s == "object" ? s : {}).loginUrl?.trim();
  if (!i)
    throw new Error("SSO init 未返回 loginUrl");
  return i;
}
async function re(e, t) {
  const n = `${B.replace(
    /\/$/,
    ""
  )}/v1/integration/sso/token`, r = await fetch(n, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-Integration-Client-Secret": K
    },
    body: JSON.stringify({ code: e, state: t })
  }), s = await r.text();
  let a = null;
  if (s)
    try {
      a = JSON.parse(s);
    } catch {
      a = null;
    }
  if (!r.ok) {
    const i = s || `HTTP ${r.status}`;
    throw new Error(k(a, i));
  }
  return a && typeof a == "object" ? a : {};
}
function se() {
  const e = new URL(window.location.href);
  return e.searchParams.delete("code"), e.searchParams.delete("state"), e.toString();
}
function ae() {
  const { search: e, hash: t } = window.location;
  if (e && e.length > 1)
    return new URLSearchParams(e);
  const n = t.indexOf("?");
  return n !== -1 ? new URLSearchParams(t.slice(n + 1)) : null;
}
function ie() {
  const e = ae();
  if (!e) return null;
  const t = e.get("code")?.trim() ?? "", n = e.get("state")?.trim() ?? "";
  return !t || !n ? null : { code: t, state: n };
}
function ce() {
  const e = new URL(window.location.href);
  let t = !1;
  (e.searchParams.has("code") || e.searchParams.has("state")) && (e.searchParams.delete("code"), e.searchParams.delete("state"), t = !0);
  const n = e.hash, r = n.indexOf("?");
  if (r !== -1) {
    const i = new URLSearchParams(n.slice(r + 1));
    if (i.has("code") || i.has("state")) {
      i.delete("code"), i.delete("state");
      const l = n.slice(0, r), c = i.toString();
      e.hash = c ? `${l}?${c}` : l, t = !0;
    }
  }
  if (!t) return;
  const s = e.searchParams.toString(), a = s ? `?${s}` : "";
  window.history.replaceState(
    {},
    "",
    `${e.origin}${e.pathname}${a}${e.hash}`
  );
}
function v() {
  const e = {
    "Content-Type": "application/json"
  };
  try {
    const t = localStorage.getItem(te);
    t && (e.Authorization = `Bearer ${t}`);
  } catch {
  }
  return e;
}
async function le(e, t) {
  const n = new URL("/api/dogfooding-account/", window.location.origin).href, r = { user_account: e }, s = t?.trim();
  s && (r.proxy_api_key = s);
  const a = await fetch(n, {
    method: "POST",
    headers: v(),
    body: JSON.stringify(r)
  }), i = await a.text();
  let l = null;
  if (i)
    try {
      l = JSON.parse(i);
    } catch {
      l = null;
    }
  if (!a.ok) {
    const h = i || `HTTP ${a.status}`;
    throw new Error(k(l, h));
  }
  const c = l;
  if (!c || typeof c.ok != "boolean" || c.ok !== !0 || typeof c.path != "string")
    throw new Error(
      "保存接口返回格式异常（期望 { ok: true, path: string }）"
    );
  return c;
}
async function ue(e) {
  const t = new URL(
    "/api/dogfooding-account/configure-provider",
    window.location.origin
  ).href, n = await fetch(t, {
    method: "POST",
    headers: v(),
    body: JSON.stringify({ proxy_api_key: e.trim() })
  }), r = await n.text();
  let s = null;
  if (r)
    try {
      s = JSON.parse(r);
    } catch {
      s = null;
    }
  if (!n.ok) {
    const i = r || `HTTP ${n.status}`;
    throw new Error(k(s, i));
  }
  const a = s;
  if (!a || typeof a.ok != "boolean" || a.ok !== !0 || typeof a.provider_id != "string")
    throw new Error(
      "Provider 配置接口返回格式异常（期望 { ok: true, provider_id: string }）"
    );
  return a;
}
async function de(e, t) {
  const n = e?.trim() ?? "", r = t?.trim() ?? "";
  if (!n && !r)
    return {
      kind: "skipped",
      reason: "SSO 返回中无工号与 API Key，已跳过写入"
    };
  let s, a = !1;
  if (n)
    try {
      const i = await le(
        n,
        r || void 0
      );
      s = i.path, a = !!i.provider_configured;
    } catch (i) {
      return {
        kind: "error",
        scope: "account",
        message: i instanceof Error ? i.message : "调用本机保存工号接口失败"
      };
    }
  if (r && !a)
    try {
      await ue(r), a = !0;
    } catch (i) {
      return {
        kind: "error",
        scope: "provider",
        message: i instanceof Error ? i.message : "写入 AgentScope Dogfooding 模型配置失败"
      };
    }
  return {
    kind: "success",
    path: s,
    providerConfigured: a
  };
}
function U() {
  try {
    const e = localStorage.getItem(D);
    if (!e) return {};
    const t = JSON.parse(e);
    return t && typeof t == "object" ? t : {};
  } catch {
    return {};
  }
}
function ge(e, t) {
  const n = U();
  n[e] = t;
  try {
    localStorage.setItem(D, JSON.stringify(n));
  } catch {
  }
}
function J(e) {
  const t = e?.output;
  if (!Array.isArray(t)) return null;
  for (let n = t.length - 1; n >= 0; n -= 1) {
    const s = t[n]?.metadata;
    if (!s) continue;
    const a = s.metadata, i = s[R] || a?.[R];
    if (i?.trace_id) return i;
  }
  return null;
}
function fe(e) {
  if (J(e)?.trace_id) return !0;
  const n = e?.usage;
  return String(n?.model_name || "").toLowerCase().includes("dogfooding");
}
function $(e) {
  const t = e?.output;
  if (Array.isArray(t))
    for (let n = t.length - 1; n >= 0; n -= 1) {
      const s = t[n]?.original_id;
      if (typeof s == "string" && s) return s;
    }
  return String(e?.id || "");
}
async function me(e) {
  const t = await fetch("/api/dogfooding-feedback/", {
    method: "POST",
    headers: v(),
    body: JSON.stringify(e)
  }), n = await t.text();
  let r = null;
  if (n)
    try {
      r = JSON.parse(n);
    } catch {
      r = null;
    }
  if (!t.ok) {
    const s = n || `HTTP ${t.status}`;
    throw new Error(k(r, s));
  }
}
function pe({ data: e }) {
  const t = J(e), n = t?.trace_id || "", r = t?.session_id || A.host.getCurrentSessionId?.() || "", s = n || $(e), [a, i] = o.useState(!1), [l, c] = o.useState(
    null
  ), [h, d] = o.useState(""), [I, w] = o.useState(!1), [g, m] = o.useState(
    []
  ), [y, f] = o.useState("");
  if (o.useEffect(() => {
    if (!s) return;
    const u = U()[s];
    u && c(u);
  }, [s]), !fe(e) || !r)
    return null;
  const S = async (u, F = "", j = "") => {
    i(!0), d("");
    try {
      await me({
        trace_id: n,
        conversation_id: String(r),
        score_label: u,
        channel_type: "web",
        feedback_reason: F,
        feedback_comment: j,
        response_id: t?.response_id || $(e)
      }), ge(s, u), c(u), w(!1), m([]), f("");
    } catch (C) {
      d(C instanceof Error ? C.message : "反馈提交失败");
    } finally {
      i(!1);
    }
  }, E = (u) => {
    if (!(l || a)) {
      if (u === "bad") {
        w(!0);
        return;
      }
      S(u);
    }
  }, T = () => {
    if (!g.length) {
      d("请至少选择一个问题原因");
      return;
    }
    S("bad", g.join("；"), y.trim());
  }, P = l === "good" ? "优秀" : l === "fine" ? "一般" : l === "bad" ? "糟糕" : "";
  return /* @__PURE__ */ o.createElement("div", { style: { marginTop: 8 } }, /* @__PURE__ */ o.createElement(Y, { style: { margin: "8px 0" } }), /* @__PURE__ */ o.createElement("div", { style: { fontSize: 13, color: "#666", marginBottom: 8 } }, "这个回答对你有帮助吗？"), l ? /* @__PURE__ */ o.createElement(
    p,
    {
      type: "success",
      showIcon: !0,
      message: `已反馈：${P}`,
      style: { marginBottom: 0 }
    }
  ) : /* @__PURE__ */ o.createElement(W, { wrap: !0 }, /* @__PURE__ */ o.createElement(
    x,
    {
      icon: /* @__PURE__ */ o.createElement(ee, null),
      loading: a,
      onClick: () => E("bad")
    },
    "糟糕"
  ), /* @__PURE__ */ o.createElement(
    x,
    {
      icon: /* @__PURE__ */ o.createElement(Z, null),
      loading: a,
      onClick: () => E("fine")
    },
    "一般"
  ), /* @__PURE__ */ o.createElement(
    x,
    {
      icon: /* @__PURE__ */ o.createElement(V, null),
      loading: a,
      onClick: () => E("good")
    },
    "优秀"
  )), h ? /* @__PURE__ */ o.createElement(
    p,
    {
      style: { marginTop: 8 },
      type: "error",
      showIcon: !0,
      message: h
    }
  ) : null, /* @__PURE__ */ o.createElement(
    M,
    {
      title: "请告诉我们哪里不好",
      open: I,
      okText: "提交反馈",
      cancelText: "取消",
      confirmLoading: a,
      onOk: T,
      onCancel: () => {
        w(!1), m([]), f(""), d("");
      }
    },
    /* @__PURE__ */ o.createElement(
      L.Group,
      {
        style: { display: "flex", flexDirection: "column", gap: 8 },
        value: g,
        onChange: (u) => m(u)
      },
      ne.map((u) => /* @__PURE__ */ o.createElement(L, { key: u, value: u }, u))
    ),
    /* @__PURE__ */ o.createElement(
      z.TextArea,
      {
        style: { marginTop: 12 },
        rows: 3,
        placeholder: "补充说明（可选）",
        value: y,
        onChange: (u) => f(u.target.value)
      }
    )
  ));
}
function O(e) {
  return e == null || e === "" ? "—" : String(e);
}
function ye(e) {
  const t = e.trim();
  if (!t) return "";
  if (t.length <= 11) return `${t.slice(0, 4)}****`;
  const n = t.startsWith("sk-as-") ? 10 : 6, r = 4, s = "*".repeat(
    Math.min(12, Math.max(4, t.length - n - r))
  );
  return `${t.slice(0, n)}${s}${t.slice(-r)}`;
}
function he() {
  const [e, t] = o.useState(!1), [n, r] = o.useState(""), [s, a] = o.useState(!1), [i, l] = o.useState(""), [c, h] = o.useState(
    null
  ), [d, I] = o.useState(
    null
  );
  o.useEffect(() => {
    const g = ie();
    if (!g) return;
    const m = `dogfooding_sso:${g.state}`;
    try {
      const f = sessionStorage.getItem(m);
      if (f === "done" || f === "pending") return;
      sessionStorage.setItem(m, "pending");
    } catch {
    }
    let y = !1;
    return (async () => {
      a(!0), l("");
      try {
        const f = await re(
          g.code,
          g.state
        );
        if (y) return;
        ce();
        const S = f.proxyApiKey?.trim() ?? "", E = f.name ?? null, T = f.account ?? null;
        h({
          name: E,
          account: T,
          proxyApiKey: S || null
        });
        const P = await de(T, S);
        y || I(P);
        try {
          sessionStorage.setItem(m, "done");
        } catch {
        }
      } catch (f) {
        try {
          sessionStorage.removeItem(m);
        } catch {
        }
        y || l(
          f instanceof Error ? f.message : "SSO token 交换失败"
        );
      } finally {
        y || a(!1);
      }
    })(), () => {
      y = !0;
      try {
        sessionStorage.getItem(m) === "pending" && sessionStorage.removeItem(m);
      } catch {
      }
    };
  }, []);
  const w = async () => {
    t(!0), r("");
    try {
      const g = se(), m = await oe(g);
      window.location.assign(m);
    } catch (g) {
      r(
        g instanceof Error ? g.message : "发起集团账号登录失败"
      );
    } finally {
      t(!1);
    }
  };
  return /* @__PURE__ */ o.createElement("div", { style: { padding: 24, maxWidth: 820, margin: "0 auto" } }, /* @__PURE__ */ o.createElement(G, null, /* @__PURE__ */ o.createElement(
    x,
    {
      type: "primary",
      style: { marginTop: 0, marginBottom: 12 },
      loading: e,
      onClick: w
    },
    "阿里集团账号登录"
  ), n ? /* @__PURE__ */ o.createElement(
    p,
    {
      style: { marginBottom: 12 },
      type: "error",
      message: n
    }
  ) : null, s ? /* @__PURE__ */ o.createElement(
    p,
    {
      style: { marginBottom: 12 },
      type: "info",
      message: "正在使用登录回调参数换取 API 密钥…"
    }
  ) : null, i ? /* @__PURE__ */ o.createElement(
    p,
    {
      style: { marginBottom: 12 },
      type: "error",
      message: i
    }
  ) : null, c ? /* @__PURE__ */ o.createElement("div", { style: { marginTop: 16 } }, /* @__PURE__ */ o.createElement(_, { bordered: !0, size: "small", column: 1 }, /* @__PURE__ */ o.createElement(_.Item, { label: "API 密钥" }, c?.proxyApiKey ? /* @__PURE__ */ o.createElement(N, { code: !0, copyable: { text: c.proxyApiKey } }, ye(c.proxyApiKey)) : O(c?.proxyApiKey)), /* @__PURE__ */ o.createElement(_.Item, { label: "花名/姓名" }, O(c?.name)), /* @__PURE__ */ o.createElement(_.Item, { label: "工号" }, O(c?.account))), d?.kind === "success" ? /* @__PURE__ */ o.createElement(o.Fragment, null, d.path ? /* @__PURE__ */ o.createElement(
    p,
    {
      type: "success",
      showIcon: !0,
      style: { marginBottom: 12 },
      message: "已写入本机 dogfooding 用户文件",
      description: /* @__PURE__ */ o.createElement(N, { code: !0, copyable: !0 }, d.path)
    }
  ) : null, d.providerConfigured ? /* @__PURE__ */ o.createElement(
    p,
    {
      type: "success",
      showIcon: !0,
      style: { marginBottom: 12 },
      message: "API Key 已自动写入 AgentScope Dogfooding 模型配置",
      description: "可在「设置 → 模型」中确认；无需再手动复制粘贴。"
    }
  ) : null) : null, d?.kind === "skipped" ? /* @__PURE__ */ o.createElement(
    p,
    {
      type: "warning",
      showIcon: !0,
      style: { marginBottom: 12 },
      message: d.reason
    }
  ) : null, d?.kind === "error" ? /* @__PURE__ */ o.createElement(
    p,
    {
      type: "error",
      showIcon: !0,
      style: { marginBottom: 12 },
      message: d.scope === "provider" ? "写入模型配置失败" : "保存工号到本机失败",
      description: d.message
    }
  ) : null) : null));
}
class we {
  constructor() {
    this.id = b;
  }
  setup() {
    if (typeof window.QwenPaw.registerRoutes != "function") {
      console.error(`[${b}] registerRoutes is not available`);
      return;
    }
    window.QwenPaw.registerRoutes?.(this.id, [
      {
        path: "/join-dogfooding",
        component: he,
        label: "Join dogfooding plan",
        icon: /* @__PURE__ */ o.createElement(X, { size: 14 }),
        priority: 1
      }
    ]), window.QwenPaw.chat?.response?.append?.(
      b,
      ({ data: n }) => !n || typeof n != "object" ? null : /* @__PURE__ */ o.createElement(pe, { data: n }),
      { id: `${b}.feedback-bar`, order: 10 }
    );
  }
}
new we().setup();
