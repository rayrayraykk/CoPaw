import { forwardRef as $, createElement as b } from "react";
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const T = (t) => t.replace(/([a-z0-9])([A-Z])/g, "$1-$2").toLowerCase(), j = (t) => t.replace(
  /^([A-Z])|[\s-_]+(\w)/g,
  (n, o, c) => c ? c.toUpperCase() : o.toLowerCase()
), N = (t) => {
  const n = j(t);
  return n.charAt(0).toUpperCase() + n.slice(1);
}, A = (...t) => t.filter((n, o, c) => !!n && n.trim() !== "" && c.indexOf(n) === o).join(" ").trim(), L = (t) => {
  for (const n in t)
    if (n.startsWith("aria-") || n === "role" || n === "title")
      return !0;
};
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
var P = {
  xmlns: "http://www.w3.org/2000/svg",
  width: 24,
  height: 24,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 2,
  strokeLinecap: "round",
  strokeLinejoin: "round"
};
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const q = $(
  ({
    color: t = "currentColor",
    size: n = 24,
    strokeWidth: o = 2,
    absoluteStrokeWidth: c,
    className: i = "",
    children: s,
    iconNode: g,
    ...l
  }, p) => b(
    "svg",
    {
      ref: p,
      ...P,
      width: n,
      height: n,
      stroke: t,
      strokeWidth: c ? Number(o) * 24 / Number(n) : o,
      className: A("lucide", i),
      ...!s && !L(l) && { "aria-hidden": "true" },
      ...l
    },
    [
      ...g.map(([k, x]) => b(k, x)),
      ...Array.isArray(s) ? s : [s]
    ]
  )
);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const d = (t, n) => {
  const o = $(
    ({ className: c, ...i }, s) => b(q, {
      ref: s,
      iconNode: n,
      className: A(
        `lucide-${T(N(t))}`,
        `lucide-${t}`,
        c
      ),
      ...i
    })
  );
  return o.displayName = N(t), o;
};
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const B = [
  ["path", { d: "M12 8V4H8", key: "hb8ula" }],
  ["rect", { width: "16", height: "12", x: "4", y: "8", rx: "2", key: "enze0r" }],
  ["path", { d: "M2 14h2", key: "vft8re" }],
  ["path", { d: "M20 14h2", key: "4cs60a" }],
  ["path", { d: "M15 13v2", key: "1xurst" }],
  ["path", { d: "M9 13v2", key: "rq6x2g" }]
], O = d("bot", B);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const H = [["path", { d: "M20 6 9 17l-5-5", key: "1gmf2c" }]], U = d("check", H);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const V = [
  ["circle", { cx: "12", cy: "12", r: "10", key: "1mglay" }],
  ["line", { x1: "12", x2: "12", y1: "8", y2: "12", key: "1pkeuh" }],
  ["line", { x1: "12", x2: "12.01", y1: "16", y2: "16", key: "4dfq90" }]
], S = d("circle-alert", V);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const D = [
  ["path", { d: "M15 3h6v6", key: "1q9fwt" }],
  ["path", { d: "M10 14 21 3", key: "gplh6r" }],
  ["path", { d: "M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6", key: "a6xqqp" }]
], R = d("external-link", D);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const I = [
  [
    "path",
    {
      d: "M18 5a2 2 0 0 1 2 2v8.526a2 2 0 0 0 .212.897l1.068 2.127a1 1 0 0 1-.9 1.45H3.62a1 1 0 0 1-.9-1.45l1.068-2.127A2 2 0 0 0 4 15.526V7a2 2 0 0 1 2-2z",
      key: "1pdavp"
    }
  ],
  ["path", { d: "M20.054 15.987H3.946", key: "14rxg9" }]
], J = d("laptop", I);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Q = [["path", { d: "M21 12a9 9 0 1 1-6.219-8.56", key: "13zald" }]], z = d("loader-circle", Q);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const K = [
  ["circle", { cx: "12", cy: "16", r: "1", key: "1au0dj" }],
  ["rect", { x: "3", y: "10", width: "18", height: "12", rx: "2", key: "6s8ecr" }],
  ["path", { d: "M7 10V7a5 5 0 0 1 10 0v3", key: "1pqi11" }]
], W = d("lock-keyhole", K);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Z = [
  [
    "path",
    {
      d: "M22 17a2 2 0 0 1-2 2H6.828a2 2 0 0 0-1.414.586l-2.202 2.202A.71.71 0 0 1 2 21.286V5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2z",
      key: "18887p"
    }
  ],
  ["path", { d: "M7 11h10", key: "1twpyw" }],
  ["path", { d: "M7 15h6", key: "d9of3u" }],
  ["path", { d: "M7 7h8", key: "af5zfr" }]
], F = d("message-square-text", Z);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const G = [
  ["path", { d: "M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8", key: "v9h5vc" }],
  ["path", { d: "M21 3v5h-5", key: "1q7to0" }],
  ["path", { d: "M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16", key: "3uifl3" }],
  ["path", { d: "M8 16H3v5", key: "1cv678" }]
], Y = d("refresh-cw", G);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const X = [
  [
    "path",
    {
      d: "M14.536 21.686a.5.5 0 0 0 .937-.024l6.5-19a.496.496 0 0 0-.635-.635l-19 6.5a.5.5 0 0 0-.024.937l7.93 3.18a2 2 0 0 1 1.112 1.11z",
      key: "1ffxy3"
    }
  ],
  ["path", { d: "m21.854 2.147-10.94 10.939", key: "12cjpa" }]
], C = d("send", X);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const ee = [
  [
    "path",
    {
      d: "M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z",
      key: "oel41y"
    }
  ],
  ["path", { d: "m9 12 2 2 4-4", key: "dzmm74" }]
], te = d("shield-check", ee);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const ae = [
  ["path", { d: "M10 11v6", key: "nco0om" }],
  ["path", { d: "M14 11v6", key: "outv1u" }],
  ["path", { d: "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6", key: "miytrc" }],
  ["path", { d: "M3 6h18", key: "d0wm0j" }],
  ["path", { d: "M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2", key: "e791ji" }]
], ne = d("trash-2", ae), m = window.QwenPaw.host, e = m.React, { useEffect: oe, useState: u } = e, re = `
.dt-shell{min-height:100%;background:#f5f4ef;color:#17211d;padding:clamp(20px,4vw,56px);font-family:Inter,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}
.dt-wrap{max-width:1080px;margin:0 auto}.dt-hero{display:flex;align-items:flex-end;justify-content:space-between;gap:24px;margin-bottom:32px}
.dt-kicker{display:flex;align-items:center;gap:8px;color:#547064;font-size:12px;font-weight:700;letter-spacing:.12em;text-transform:uppercase}
.dt-title{font-size:clamp(32px,5vw,56px);line-height:1.02;letter-spacing:-.055em;margin:12px 0;color:#14201b}.dt-sub{max-width:630px;color:#62706a;font-size:15px;line-height:1.7;margin:0}
.dt-button{border:1px solid #c8cec8;background:#fff;color:#17211d;border-radius:12px;padding:11px 16px;font-weight:650;display:inline-flex;align-items:center;justify-content:center;gap:8px;cursor:pointer;transition:transform .18s,box-shadow .18s,border-color .18s}
.dt-button:hover{transform:translateY(-1px);border-color:#8fa098;box-shadow:0 8px 24px rgba(20,32,27,.08)}.dt-button:disabled{opacity:.48;cursor:not-allowed;transform:none;box-shadow:none}.dt-primary{background:#173f34;color:#fff;border-color:#173f34}.dt-danger{color:#9b3e35}
.dt-grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:16px}.dt-card{background:rgba(255,255,255,.78);border:1px solid #dfe2dc;border-radius:20px;padding:22px;box-shadow:0 16px 50px rgba(32,45,39,.045)}
.dt-card-head{display:flex;align-items:flex-start;justify-content:space-between;gap:16px}.dt-icon{width:40px;height:40px;border-radius:12px;background:#e7eee9;color:#285b4a;display:grid;place-items:center}.dt-state{display:flex;align-items:center;gap:7px;font-size:12px;font-weight:700;color:#68736e}.dt-state.ok{color:#267352}.dt-card h2{font-size:17px;margin:18px 0 6px;letter-spacing:-.02em}.dt-card p{color:#6d7772;font-size:13px;line-height:1.55;margin:0}.dt-wide{grid-column:1/-1}
.dt-actions{display:flex;gap:10px;flex-wrap:wrap;margin-top:22px}.dt-notice{display:flex;gap:12px;margin-top:18px;padding:14px;border-radius:13px;background:#f4eee3;color:#725d37;font-size:13px;line-height:1.5}
.dt-section{margin-top:28px}.dt-section-top{display:flex;align-items:center;justify-content:space-between;margin-bottom:12px}.dt-section h2{font-size:20px;letter-spacing:-.03em}.dt-draft{display:grid;grid-template-columns:minmax(150px,220px) 1fr auto;gap:18px;align-items:start}.dt-draft+.dt-draft{margin-top:12px}.dt-meta{font-size:12px;color:#6d7772}.dt-conversation{font-weight:700;margin-bottom:5px}.dt-copy{white-space:pre-wrap;font-size:14px;line-height:1.65;color:#33413b}.dt-error{margin-top:18px;color:#973b33;font-size:13px}.dt-empty{text-align:center;padding:34px;color:#7c8581}
@media(max-width:720px){.dt-shell{padding:20px 14px}.dt-hero{align-items:flex-start;flex-direction:column}.dt-grid{grid-template-columns:1fr}.dt-wide{grid-column:auto}.dt-draft{grid-template-columns:1fr}.dt-draft .dt-actions{margin-top:0}}
`;
async function h(t, n) {
  const o = m.fetch ? await m.fetch(t, n) : await fetch(m.getApiUrl(t), {
    ...n,
    headers: {
      "Content-Type": "application/json",
      ...(n == null ? void 0 : n.headers) || {},
      ...m.getApiToken() ? { Authorization: `Bearer ${m.getApiToken()}` } : {}
    }
  }), c = await o.json().catch(() => ({}));
  if (!o.ok)
    throw new Error(c.detail || `HTTP ${o.status}`);
  return c;
}
function E({ ok: t, text: n }) {
  return /* @__PURE__ */ e.createElement("span", { className: `dt-state ${t ? "ok" : ""}` }, t ? /* @__PURE__ */ e.createElement(U, { size: 14 }) : /* @__PURE__ */ e.createElement(S, { size: 14 }), n);
}
function ce() {
  const [t, n] = u(null), [o, c] = u([]), [i, s] = u(""), [g, l] = u(""), p = async () => {
    l("");
    try {
      const [a, r] = await Promise.all([
        h("/dingtalk-desktop/status"),
        h("/dingtalk-desktop/drafts")
      ]);
      n(a), c(r.drafts);
    } catch (a) {
      l(a instanceof Error ? a.message : "加载失败");
    }
  };
  oe(() => {
    p();
  }, []);
  const k = async () => {
    s("login"), l("");
    const a = window.open("about:blank", "_blank");
    try {
      const r = await h(
        "/harnesses/codex/login",
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ device_code: !1, settings: {} })
        }
      );
      r.authUrl && a ? a.location.href = r.authUrl : r.authUrl ? window.open(r.authUrl, "_blank", "noopener,noreferrer") : a == null || a.close();
    } catch (r) {
      a == null || a.close(), l(r instanceof Error ? r.message : "OAuth 启动失败");
    } finally {
      s("");
    }
  }, x = async (a) => {
    s(a), l("");
    try {
      await h("/dingtalk-desktop/setup", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ reply_mode: a })
      }), await p();
    } catch (r) {
      l(r instanceof Error ? r.message : "连接失败");
    } finally {
      s("");
    }
  }, w = async (a, r) => {
    s(a.id), l("");
    try {
      await h(
        `/dingtalk-desktop/drafts/${a.id}${r === "send" ? "/send" : ""}`,
        { method: r === "send" ? "POST" : "DELETE" }
      ), await p();
    } catch (v) {
      l(v instanceof Error ? v.message : "操作失败");
    } finally {
      s("");
    }
  }, f = !!((t == null ? void 0 : t.backend) === "codex" && t.codex.authenticated), y = !!(t != null && t.desktop.logged_in && t.desktop.accessibility);
  return /* @__PURE__ */ e.createElement("div", { className: "dt-shell" }, /* @__PURE__ */ e.createElement("style", null, re), /* @__PURE__ */ e.createElement("main", { className: "dt-wrap" }, /* @__PURE__ */ e.createElement("header", { className: "dt-hero" }, /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "dt-kicker" }, /* @__PURE__ */ e.createElement(te, { size: 15 }), " Personal channel"), /* @__PURE__ */ e.createElement("h1", { className: "dt-title" }, "Codex，接管当前阿里钉会话"), /* @__PURE__ */ e.createElement("p", { className: "dt-sub" }, "使用 QwenPaw 已有的 Codex OAuth 与本机阿里钉登录态。没有机器人、 没有 webhook，也不读取或保存任何账号凭证。")), /* @__PURE__ */ e.createElement("button", { className: "dt-button", onClick: () => void p() }, /* @__PURE__ */ e.createElement(Y, { size: 16 }), " 刷新状态")), /* @__PURE__ */ e.createElement("section", { className: "dt-grid" }, /* @__PURE__ */ e.createElement("article", { className: "dt-card" }, /* @__PURE__ */ e.createElement("div", { className: "dt-card-head" }, /* @__PURE__ */ e.createElement("div", { className: "dt-icon" }, /* @__PURE__ */ e.createElement(O, { size: 20 })), /* @__PURE__ */ e.createElement(E, { ok: f, text: f ? "已认证" : "未就绪" })), /* @__PURE__ */ e.createElement("h2", null, "当前 Codex Agent"), /* @__PURE__ */ e.createElement("p", null, t ? `${t.agent_id} · ${t.backend}` : "正在检查"), !(t != null && t.codex.authenticated) && /* @__PURE__ */ e.createElement("div", { className: "dt-actions" }, /* @__PURE__ */ e.createElement("button", { className: "dt-button", onClick: () => void k() }, i === "login" ? /* @__PURE__ */ e.createElement(z, { size: 16 }) : /* @__PURE__ */ e.createElement(R, { size: 16 }), "通过 ChatGPT OAuth 登录"))), /* @__PURE__ */ e.createElement("article", { className: "dt-card" }, /* @__PURE__ */ e.createElement("div", { className: "dt-card-head" }, /* @__PURE__ */ e.createElement("div", { className: "dt-icon" }, /* @__PURE__ */ e.createElement(J, { size: 20 })), /* @__PURE__ */ e.createElement(
    E,
    {
      ok: y,
      text: y ? "本机已连接" : "需要检查"
    }
  )), /* @__PURE__ */ e.createElement("h2", null, "阿里钉桌面端"), /* @__PURE__ */ e.createElement("p", null, t != null && t.desktop.version ? `版本 ${t.desktop.version} · 本机登录态` : "请打开阿里钉并完成登录")), /* @__PURE__ */ e.createElement("article", { className: "dt-card dt-wide" }, /* @__PURE__ */ e.createElement("div", { className: "dt-card-head" }, /* @__PURE__ */ e.createElement("div", { className: "dt-icon" }, /* @__PURE__ */ e.createElement(W, { size: 20 })), /* @__PURE__ */ e.createElement(
    E,
    {
      ok: !!(t != null && t.configured),
      text: t != null && t.configured ? "已绑定" : "等待绑定"
    }
  )), /* @__PURE__ */ e.createElement("h2", null, "绑定当前打开的会话"), /* @__PURE__ */ e.createElement("p", null, "插件只读取当前可见且标题完全匹配的白名单会话，不使用坐标， 不自动点击或切换会话。发送前会再次验证会话标题。"), /* @__PURE__ */ e.createElement("div", { className: "dt-actions" }, /* @__PURE__ */ e.createElement(
    "button",
    {
      className: "dt-button dt-primary",
      disabled: !f || !y || !!i,
      onClick: () => void x("draft")
    },
    i === "draft" ? /* @__PURE__ */ e.createElement(z, { size: 16 }) : /* @__PURE__ */ e.createElement(F, { size: 16 }),
    "一键连接并使用草稿"
  ), /* @__PURE__ */ e.createElement(
    "button",
    {
      className: "dt-button",
      disabled: !f || !y || !!i,
      onClick: () => void x("automatic")
    },
    /* @__PURE__ */ e.createElement(C, { size: 16 }),
    " 明确启用自动回复"
  )), /* @__PURE__ */ e.createElement("div", { className: "dt-notice" }, /* @__PURE__ */ e.createElement(S, { size: 18 }), /* @__PURE__ */ e.createElement("span", null, "建议先使用草稿模式。自动回复只对绑定时当前打开的会话生效。")))), /* @__PURE__ */ e.createElement("section", { className: "dt-section" }, /* @__PURE__ */ e.createElement("div", { className: "dt-section-top" }, /* @__PURE__ */ e.createElement("h2", null, "待审批草稿"), /* @__PURE__ */ e.createElement("span", { className: "dt-meta" }, o.length, " 条")), o.length === 0 ? /* @__PURE__ */ e.createElement("div", { className: "dt-card dt-empty" }, "暂无待审批草稿") : o.map((a) => /* @__PURE__ */ e.createElement("article", { className: "dt-card dt-draft", key: a.id }, /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "dt-conversation" }, a.conversation), /* @__PURE__ */ e.createElement("div", { className: "dt-meta" }, new Date(a.created_at * 1e3).toLocaleString())), /* @__PURE__ */ e.createElement("div", { className: "dt-copy" }, a.text), /* @__PURE__ */ e.createElement("div", { className: "dt-actions" }, /* @__PURE__ */ e.createElement(
    "button",
    {
      className: "dt-button dt-primary",
      disabled: i === a.id,
      onClick: () => void w(a, "send")
    },
    /* @__PURE__ */ e.createElement(C, { size: 15 }),
    " 发送"
  ), /* @__PURE__ */ e.createElement(
    "button",
    {
      className: "dt-button dt-danger",
      disabled: i === a.id,
      onClick: () => void w(a, "delete")
    },
    /* @__PURE__ */ e.createElement(ne, { size: 15 }),
    " 删除"
  ))))), g && /* @__PURE__ */ e.createElement("div", { className: "dt-error" }, g)));
}
var _, M;
(M = (_ = window.QwenPaw).registerRoutes) == null || M.call(_, "dingtalk-desktop", [
  {
    path: "/plugin/dingtalk-desktop",
    component: ce,
    label: "阿里钉 · Codex",
    icon: "message-square-text",
    priority: 44
  }
]);
