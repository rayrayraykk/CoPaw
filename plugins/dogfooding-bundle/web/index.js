const E = window.QwenPaw;
if (!E?.host?.React || !E?.host?.antd)
  throw new Error("window.QwenPaw.host not found");
const { React: s, antd: U, antdIcons: N } = E.host, { Card: C, Button: B, Alert: f, Typography: _, Descriptions: y } = U, { LoginOutlined: v } = N, { Text: T } = _, x = "https://proxy.agentscope.design", A = "qwenpaw-proxy-v1.0", K = "qwenpaw_auth_token", b = "dogfooding-bundle";
console.info(`[${b}] frontend runtime detected`);
function k(e, t) {
  if (!e || typeof e != "object" || e === null)
    return t;
  const n = e, { detail: o } = n;
  return typeof o == "string" ? o : Array.isArray(o) ? o.map((r) => r && typeof r == "object" && "msg" in r ? String(r.msg) : JSON.stringify(r)).filter(Boolean).join("; ") || t : typeof n.message == "string" ? n.message : t;
}
async function j(e) {
  const t = `${x.replace(
    /\/$/,
    ""
  )}/v1/integration/sso/init`, n = await fetch(t, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-Integration-Client-Secret": A
    },
    body: JSON.stringify({ redirectUri: e })
  }), o = await n.text();
  let a = null;
  if (o)
    try {
      a = JSON.parse(o);
    } catch {
      a = null;
    }
  if (!n.ok) {
    const g = o || `HTTP ${n.status}`;
    throw new Error(k(a, g));
  }
  const i = (a && typeof a == "object" ? a : {}).loginUrl?.trim();
  if (!i)
    throw new Error("SSO init 未返回 loginUrl");
  return i;
}
async function J(e, t) {
  const n = `${x.replace(
    /\/$/,
    ""
  )}/v1/integration/sso/token`, o = await fetch(n, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-Integration-Client-Secret": A
    },
    body: JSON.stringify({ code: e, state: t })
  }), a = await o.text();
  let r = null;
  if (a)
    try {
      r = JSON.parse(a);
    } catch {
      r = null;
    }
  if (!o.ok) {
    const i = a || `HTTP ${o.status}`;
    throw new Error(k(r, i));
  }
  return r && typeof r == "object" ? r : {};
}
function q() {
  const e = new URL(window.location.href);
  return e.searchParams.delete("code"), e.searchParams.delete("state"), e.toString();
}
function Q() {
  const { search: e, hash: t } = window.location;
  if (e && e.length > 1)
    return new URLSearchParams(e);
  const n = t.indexOf("?");
  return n !== -1 ? new URLSearchParams(t.slice(n + 1)) : null;
}
function D() {
  const e = Q();
  if (!e) return null;
  const t = e.get("code")?.trim() ?? "", n = e.get("state")?.trim() ?? "";
  return !t || !n ? null : { code: t, state: n };
}
function F() {
  const e = new URL(window.location.href);
  let t = !1;
  (e.searchParams.has("code") || e.searchParams.has("state")) && (e.searchParams.delete("code"), e.searchParams.delete("state"), t = !0);
  const n = e.hash, o = n.indexOf("?");
  if (o !== -1) {
    const i = new URLSearchParams(n.slice(o + 1));
    if (i.has("code") || i.has("state")) {
      i.delete("code"), i.delete("state");
      const g = n.slice(0, o), c = i.toString();
      e.hash = c ? `${g}?${c}` : g, t = !0;
    }
  }
  if (!t) return;
  const a = e.searchParams.toString(), r = a ? `?${a}` : "";
  window.history.replaceState(
    {},
    "",
    `${e.origin}${e.pathname}${r}${e.hash}`
  );
}
function H() {
  const e = {
    "Content-Type": "application/json"
  };
  try {
    const t = localStorage.getItem(K);
    t && (e.Authorization = `Bearer ${t}`);
  } catch {
  }
  return e;
}
async function W(e) {
  const t = new URL("/api/dogfooding-account/", window.location.origin).href, n = await fetch(t, {
    method: "POST",
    headers: H(),
    body: JSON.stringify({ user_account: e })
  }), o = await n.text();
  let a = null;
  if (o)
    try {
      a = JSON.parse(o);
    } catch {
      a = null;
    }
  if (!n.ok) {
    const i = o || `HTTP ${n.status}`;
    throw new Error(k(a, i));
  }
  const r = a;
  if (!r || typeof r.ok != "boolean" || r.ok !== !0 || typeof r.path != "string")
    throw new Error("保存接口返回格式异常（期望 { ok: true, path: string }）");
  return r;
}
function S(e) {
  return e == null || e === "" ? "—" : String(e);
}
function z(e) {
  const t = e.trim();
  if (!t) return "";
  if (t.length <= 11) return `${t.slice(0, 4)}****`;
  const n = t.startsWith("sk-as-") ? 10 : 6, o = 4, a = "*".repeat(
    Math.min(12, Math.max(4, t.length - n - o))
  );
  return `${t.slice(0, n)}${a}${t.slice(-o)}`;
}
function G() {
  const [e, t] = s.useState(!1), [n, o] = s.useState(""), [a, r] = s.useState(!1), [i, g] = s.useState(""), [c, $] = s.useState(
    null
  ), [p, w] = s.useState(
    null
  );
  s.useEffect(() => {
    const l = D();
    if (!l) return;
    const d = `dogfooding_sso:${l.state}`;
    try {
      const u = sessionStorage.getItem(d);
      if (u === "done" || u === "pending") return;
      sessionStorage.setItem(d, "pending");
    } catch {
    }
    let m = !1;
    return (async () => {
      r(!0), g("");
      try {
        const u = await J(
          l.code,
          l.state
        );
        if (m) return;
        F();
        const L = u.proxyApiKey?.trim() ?? "", R = u.name ?? null, P = u.account ?? null;
        $({
          name: R,
          account: P,
          proxyApiKey: L || null
        });
        const I = P?.trim();
        if (I)
          try {
            const h = await W(I);
            w({ kind: "success", path: h.path });
          } catch (h) {
            w({
              kind: "error",
              message: h instanceof Error ? h.message : "调用本机保存工号接口失败"
            });
          }
        else
          w({
            kind: "skipped",
            reason: "SSO 返回中无工号，已跳过写入本机 dogfooding 用户文件"
          });
        try {
          sessionStorage.setItem(d, "done");
        } catch {
        }
      } catch (u) {
        try {
          sessionStorage.removeItem(d);
        } catch {
        }
        m || g(
          u instanceof Error ? u.message : "SSO token 交换失败"
        );
      } finally {
        m || r(!1);
      }
    })(), () => {
      m = !0;
      try {
        sessionStorage.getItem(d) === "pending" && sessionStorage.removeItem(d);
      } catch {
      }
    };
  }, []);
  const O = async () => {
    t(!0), o("");
    try {
      const l = q(), d = await j(l);
      window.location.assign(d);
    } catch (l) {
      o(
        l instanceof Error ? l.message : "发起集团账号登录失败"
      );
    } finally {
      t(!1);
    }
  };
  return /* @__PURE__ */ s.createElement("div", { style: { padding: 24, maxWidth: 820, margin: "0 auto" } }, /* @__PURE__ */ s.createElement(C, null, /* @__PURE__ */ s.createElement(
    B,
    {
      type: "primary",
      style: { marginTop: 0, marginBottom: 12 },
      loading: e,
      onClick: O
    },
    "阿里集团账号登录"
  ), n ? /* @__PURE__ */ s.createElement(
    f,
    {
      style: { marginBottom: 12 },
      type: "error",
      message: n
    }
  ) : null, a ? /* @__PURE__ */ s.createElement(
    f,
    {
      style: { marginBottom: 12 },
      type: "info",
      message: "正在使用登录回调参数换取 API 密钥…"
    }
  ) : null, i ? /* @__PURE__ */ s.createElement(
    f,
    {
      style: { marginBottom: 12 },
      type: "error",
      message: i
    }
  ) : null, c ? /* @__PURE__ */ s.createElement("div", { style: { marginTop: 16 } }, /* @__PURE__ */ s.createElement(y, { bordered: !0, size: "small", column: 1 }, /* @__PURE__ */ s.createElement(y.Item, { label: "API 密钥" }, c?.proxyApiKey ? /* @__PURE__ */ s.createElement(T, { code: !0, copyable: { text: c.proxyApiKey } }, z(c.proxyApiKey)) : S(c?.proxyApiKey)), /* @__PURE__ */ s.createElement(y.Item, { label: "花名/姓名" }, S(c?.name)), /* @__PURE__ */ s.createElement(y.Item, { label: "工号" }, S(c?.account))), p?.kind === "success" ? /* @__PURE__ */ s.createElement(
    f,
    {
      type: "success",
      showIcon: !0,
      style: { marginBottom: 12 },
      message: "已写入本机 dogfooding 用户文件",
      description: /* @__PURE__ */ s.createElement(T, { code: !0, copyable: !0 }, p.path)
    }
  ) : null, p?.kind === "skipped" ? /* @__PURE__ */ s.createElement(
    f,
    {
      type: "warning",
      showIcon: !0,
      style: { marginBottom: 12 },
      message: p.reason
    }
  ) : null, p?.kind === "error" ? /* @__PURE__ */ s.createElement(
    f,
    {
      type: "error",
      showIcon: !0,
      style: { marginBottom: 12 },
      message: "保存工号到本机失败",
      description: p.message
    }
  ) : null) : null));
}
class M {
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
        component: G,
        label: "Join dogfooding plan",
        icon: /* @__PURE__ */ s.createElement(v, { size: 14 }),
        priority: 1
      }
    ]);
  }
}
new M().setup();
