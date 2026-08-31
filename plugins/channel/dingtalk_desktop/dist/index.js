const S = window.QwenPaw.host.React, N = S.createElement, P = S.forwardRef;
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const R = (r) => r.replace(/([a-z0-9])([A-Z])/g, "$1-$2").toLowerCase(), q = (r) => r.replace(
  /^([A-Z])|[\s-_]+(\w)/g,
  (n, o, c) => c ? c.toUpperCase() : o.toLowerCase()
), _ = (r) => {
  const n = q(r);
  return n.charAt(0).toUpperCase() + n.slice(1);
}, j = (...r) => r.filter((n, o, c) => !!n && n.trim() !== "" && c.indexOf(n) === o).join(" ").trim(), D = (r) => {
  for (const n in r)
    if (n.startsWith("aria-") || n === "role" || n === "title")
      return !0;
};
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
var H = {
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
const V = P(
  ({
    color: r = "currentColor",
    size: n = 24,
    strokeWidth: o = 2,
    absoluteStrokeWidth: c,
    className: t = "",
    children: s,
    iconNode: f,
    ...x
  }, p) => N(
    "svg",
    {
      ref: p,
      ...H,
      width: n,
      height: n,
      stroke: r,
      strokeWidth: c ? Number(o) * 24 / Number(n) : o,
      className: j("lucide", t),
      ...!s && !D(x) && { "aria-hidden": "true" },
      ...x
    },
    [
      ...f.map(([m, y]) => N(m, y)),
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
const l = (r, n) => {
  const o = P(
    ({ className: c, ...t }, s) => N(V, {
      ref: s,
      iconNode: n,
      className: j(
        `lucide-${R(_(r))}`,
        `lucide-${r}`,
        c
      ),
      ...t
    })
  );
  return o.displayName = _(r), o;
};
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Q = [
  ["path", { d: "M12 8V4H8", key: "hb8ula" }],
  ["rect", { width: "16", height: "12", x: "4", y: "8", rx: "2", key: "enze0r" }],
  ["path", { d: "M2 14h2", key: "vft8re" }],
  ["path", { d: "M20 14h2", key: "4cs60a" }],
  ["path", { d: "M15 13v2", key: "1xurst" }],
  ["path", { d: "M9 13v2", key: "rq6x2g" }]
], O = l("bot", Q);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const U = [["path", { d: "M20 6 9 17l-5-5", key: "1gmf2c" }]], F = l("check", U);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const J = [
  ["circle", { cx: "12", cy: "12", r: "10", key: "1mglay" }],
  ["line", { x1: "12", x2: "12", y1: "8", y2: "12", key: "1pkeuh" }],
  ["line", { x1: "12", x2: "12.01", y1: "16", y2: "16", key: "4dfq90" }]
], L = l("circle-alert", J);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const K = [
  [
    "path",
    {
      d: "M18 5a2 2 0 0 1 2 2v8.526a2 2 0 0 0 .212.897l1.068 2.127a1 1 0 0 1-.9 1.45H3.62a1 1 0 0 1-.9-1.45l1.068-2.127A2 2 0 0 0 4 15.526V7a2 2 0 0 1 2-2z",
      key: "1pdavp"
    }
  ],
  ["path", { d: "M20.054 15.987H3.946", key: "14rxg9" }]
], W = l("laptop", K);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Z = [["path", { d: "M21 12a9 9 0 1 1-6.219-8.56", key: "13zald" }]], I = l("loader-circle", Z);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const X = [
  ["circle", { cx: "12", cy: "16", r: "1", key: "1au0dj" }],
  ["rect", { x: "3", y: "10", width: "18", height: "12", rx: "2", key: "6s8ecr" }],
  ["path", { d: "M7 10V7a5 5 0 0 1 10 0v3", key: "1pqi11" }]
], Y = l("lock-keyhole", X);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const G = [
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
], ee = l("message-square-text", G);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const te = [
  ["path", { d: "M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8", key: "v9h5vc" }],
  ["path", { d: "M21 3v5h-5", key: "1q7to0" }],
  ["path", { d: "M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16", key: "3uifl3" }],
  ["path", { d: "M8 16H3v5", key: "1cv678" }]
], ae = l("refresh-cw", te);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const ne = [
  [
    "path",
    {
      d: "M14.536 21.686a.5.5 0 0 0 .937-.024l6.5-19a.496.496 0 0 0-.635-.635l-19 6.5a.5.5 0 0 0-.024.937l7.93 3.18a2 2 0 0 1 1.112 1.11z",
      key: "1ffxy3"
    }
  ],
  ["path", { d: "m21.854 2.147-10.94 10.939", key: "12cjpa" }]
], C = l("send", ne);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const oe = [
  [
    "path",
    {
      d: "M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z",
      key: "oel41y"
    }
  ],
  ["path", { d: "m9 12 2 2 4-4", key: "dzmm74" }]
], re = l("shield-check", oe);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const ce = [
  ["path", { d: "M10 11v6", key: "nco0om" }],
  ["path", { d: "M14 11v6", key: "outv1u" }],
  ["path", { d: "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6", key: "miytrc" }],
  ["path", { d: "M3 6h18", key: "d0wm0j" }],
  ["path", { d: "M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2", key: "e791ji" }]
], se = l("trash-2", ce), h = window.QwenPaw.host, e = h.React, { useEffect: de, useState: g } = e, le = `
.dt-shell{min-height:100%;background:#f5f4ef;color:#17211d;padding:clamp(20px,4vw,56px);font-family:Inter,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}
.dt-wrap{max-width:1080px;margin:0 auto}.dt-hero{display:flex;align-items:flex-end;justify-content:space-between;gap:24px;margin-bottom:32px}
.dt-kicker{display:flex;align-items:center;gap:8px;color:#547064;font-size:12px;font-weight:700;letter-spacing:.12em;text-transform:uppercase}
.dt-title{font-size:clamp(32px,5vw,56px);line-height:1.02;letter-spacing:-.055em;margin:12px 0;color:#14201b}.dt-sub{max-width:630px;color:#62706a;font-size:15px;line-height:1.7;margin:0}
.dt-button{border:1px solid #c8cec8;background:#fff;color:#17211d;border-radius:12px;padding:11px 16px;font-weight:650;display:inline-flex;align-items:center;justify-content:center;gap:8px;cursor:pointer;transition:transform .18s,box-shadow .18s,border-color .18s}
.dt-button:hover{transform:translateY(-1px);border-color:#8fa098;box-shadow:0 8px 24px rgba(20,32,27,.08)}.dt-button:disabled{opacity:.48;cursor:not-allowed;transform:none;box-shadow:none}.dt-primary{background:#173f34;color:#fff;border-color:#173f34}.dt-danger{color:#9b3e35}
.dt-grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:16px}.dt-card{background:rgba(255,255,255,.78);border:1px solid #dfe2dc;border-radius:20px;padding:22px;box-shadow:0 16px 50px rgba(32,45,39,.045)}
.dt-card-head{display:flex;align-items:flex-start;justify-content:space-between;gap:16px}.dt-icon{width:40px;height:40px;border-radius:12px;background:#e7eee9;color:#285b4a;display:grid;place-items:center}.dt-state{display:flex;align-items:center;gap:7px;font-size:12px;font-weight:700;color:#68736e}.dt-state.ok{color:#267352}.dt-card h2{font-size:17px;margin:18px 0 6px;letter-spacing:-.02em}.dt-card p{color:#6d7772;font-size:13px;line-height:1.55;margin:0}.dt-wide{grid-column:1/-1}
.dt-field{display:grid;gap:7px;margin-top:14px}.dt-label{color:#53625b;font-size:12px;font-weight:700}.dt-select{width:100%;min-height:44px;border:1px solid #c8cec8;border-radius:11px;background:#fff;color:#17211d;padding:0 12px;font:inherit}.dt-select:focus-visible,.dt-button:focus-visible{outline:3px solid rgba(40,91,74,.25);outline-offset:2px}
.dt-actions{display:flex;gap:10px;flex-wrap:wrap;margin-top:22px}.dt-notice{display:flex;gap:12px;margin-top:18px;padding:14px;border-radius:13px;background:#f4eee3;color:#725d37;font-size:13px;line-height:1.5}
.dt-section{margin-top:28px}.dt-section-top{display:flex;align-items:center;justify-content:space-between;margin-bottom:12px}.dt-section h2{font-size:20px;letter-spacing:-.03em}.dt-draft{display:grid;grid-template-columns:minmax(150px,220px) 1fr auto;gap:18px;align-items:start}.dt-draft+.dt-draft{margin-top:12px}.dt-meta{font-size:12px;color:#6d7772}.dt-conversation{font-weight:700;margin-bottom:5px}.dt-copy{white-space:pre-wrap;font-size:14px;line-height:1.65;color:#33413b}.dt-error{margin-top:18px;color:#973b33;font-size:13px}.dt-empty{text-align:center;padding:34px;color:#7c8581}
@media(max-width:720px){.dt-shell{padding:20px 14px}.dt-hero{align-items:flex-start;flex-direction:column}.dt-grid{grid-template-columns:1fr}.dt-wide{grid-column:auto}.dt-draft{grid-template-columns:1fr}.dt-draft .dt-actions{margin-top:0}}
`;
async function u(r, n, o) {
  const c = {
    ...n,
    headers: {
      "Content-Type": "application/json",
      ...(n == null ? void 0 : n.headers) || {},
      ...o ? { "X-Agent-Id": o } : {}
    }
  }, t = h.fetch ? await h.fetch(r, c) : await fetch(h.getApiUrl(r), {
    ...c,
    headers: {
      ...c.headers,
      ...h.getApiToken() ? { Authorization: `Bearer ${h.getApiToken()}` } : {}
    }
  }), s = await t.json().catch(() => ({}));
  if (!t.ok)
    throw new Error(s.detail || `HTTP ${t.status}`);
  return s;
}
function v({ ok: r, text: n }) {
  return /* @__PURE__ */ e.createElement("span", { className: `dt-state ${r ? "ok" : ""}` }, r ? /* @__PURE__ */ e.createElement(F, { size: 14 }) : /* @__PURE__ */ e.createElement(L, { size: 14 }), n);
}
function ie() {
  const [r, n] = g([]), [o, c] = g(""), [t, s] = g(null), [f, x] = g([]), [p, m] = g(""), [y, i] = g(""), B = async () => {
    try {
      const a = await u("/agents");
      n(a.agents.filter((d) => d.enabled));
    } catch (a) {
      i(a instanceof Error ? a.message : "Agent 加载失败");
    }
  }, k = async (a = o) => {
    if (a) {
      i("");
      try {
        const [d, w] = await Promise.all([
          u("/dingtalk-desktop/status", void 0, a),
          u(
            "/dingtalk-desktop/drafts",
            void 0,
            a
          )
        ]);
        s(d), x(w.drafts);
      } catch (d) {
        i(d instanceof Error ? d.message : "加载失败");
      }
    }
  };
  de(() => {
    B();
  }, []);
  const T = (a) => {
    c(a), s(null), x([]), i(""), a && k(a);
  }, z = async (a) => {
    m(a), i("");
    try {
      await u(
        "/dingtalk-desktop/setup",
        {
          method: "POST",
          body: JSON.stringify({ reply_mode: a })
        },
        o
      ), await k(o);
    } catch (d) {
      i(d instanceof Error ? d.message : "连接失败");
    } finally {
      m("");
    }
  }, A = async (a, d) => {
    m(a.id), i("");
    try {
      await u(
        `/dingtalk-desktop/drafts/${a.id}${d === "send" ? "/send" : ""}`,
        { method: d === "send" ? "POST" : "DELETE" },
        o
      ), await k(o);
    } catch (w) {
      i(w instanceof Error ? w.message : "操作失败");
    } finally {
      m("");
    }
  }, b = !!(o && t), E = !!(t != null && t.desktop.logged_in && t.desktop.accessibility);
  return /* @__PURE__ */ e.createElement("div", { className: "dt-shell" }, /* @__PURE__ */ e.createElement("style", null, le), /* @__PURE__ */ e.createElement("main", { className: "dt-wrap" }, /* @__PURE__ */ e.createElement("header", { className: "dt-hero" }, /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "dt-kicker" }, /* @__PURE__ */ e.createElement(re, { size: 15 }), " Personal channel"), /* @__PURE__ */ e.createElement("h1", { className: "dt-title" }, "让所选 Agent 接管当前阿里钉会话"), /* @__PURE__ */ e.createElement("p", { className: "dt-sub" }, "使用所选 Agent 与本机阿里钉登录态。Agent backend 的安装和认证完全由 QwenPaw 现有运行时管理；插件不重复登录，也不读取或保存任何账号凭证。")), /* @__PURE__ */ e.createElement(
    "button",
    {
      className: "dt-button",
      disabled: !o,
      onClick: () => void k(o)
    },
    /* @__PURE__ */ e.createElement(ae, { size: 16 }),
    " 刷新状态"
  )), /* @__PURE__ */ e.createElement("section", { className: "dt-grid" }, /* @__PURE__ */ e.createElement("article", { className: "dt-card" }, /* @__PURE__ */ e.createElement("div", { className: "dt-card-head" }, /* @__PURE__ */ e.createElement("div", { className: "dt-icon" }, /* @__PURE__ */ e.createElement(O, { size: 20 })), /* @__PURE__ */ e.createElement(
    v,
    {
      ok: b,
      text: b ? "Agent 已就绪" : o ? "未就绪" : "先选择 Agent"
    }
  )), /* @__PURE__ */ e.createElement("h2", null, "选择回复消息的 Agent"), /* @__PURE__ */ e.createElement("div", { className: "dt-field" }, /* @__PURE__ */ e.createElement("label", { className: "dt-label", htmlFor: "dt-agent" }, "回复消息的 Agent"), /* @__PURE__ */ e.createElement(
    "select",
    {
      id: "dt-agent",
      className: "dt-select",
      value: o,
      onChange: (a) => T(a.target.value)
    },
    /* @__PURE__ */ e.createElement("option", { value: "" }, "请选择 Agent"),
    r.map((a) => /* @__PURE__ */ e.createElement("option", { key: a.id, value: a.id }, a.name || a.id))
  )), /* @__PURE__ */ e.createElement("p", null, t ? `${t.agent_id} · ${t.backend}` : o ? "正在检查 Agent 状态" : "配置、审批和草稿都会严格归属所选 Agent")), /* @__PURE__ */ e.createElement("article", { className: "dt-card" }, /* @__PURE__ */ e.createElement("div", { className: "dt-card-head" }, /* @__PURE__ */ e.createElement("div", { className: "dt-icon" }, /* @__PURE__ */ e.createElement(W, { size: 20 })), /* @__PURE__ */ e.createElement(
    v,
    {
      ok: E,
      text: E ? "本机已连接" : "需要检查"
    }
  )), /* @__PURE__ */ e.createElement("h2", null, "阿里钉桌面端"), /* @__PURE__ */ e.createElement("p", null, t != null && t.desktop.version ? `版本 ${t.desktop.version} · 本机登录态` : (t == null ? void 0 : t.desktop.detail) || "请打开阿里钉并完成登录")), /* @__PURE__ */ e.createElement("article", { className: "dt-card dt-wide" }, /* @__PURE__ */ e.createElement("div", { className: "dt-card-head" }, /* @__PURE__ */ e.createElement("div", { className: "dt-icon" }, /* @__PURE__ */ e.createElement(Y, { size: 20 })), /* @__PURE__ */ e.createElement(
    v,
    {
      ok: !!(t != null && t.configured),
      text: t != null && t.configured ? "访问控制已启用" : "等待连接"
    }
  )), /* @__PURE__ */ e.createElement("h2", null, "连接当前会话并授权"), /* @__PURE__ */ e.createElement("p", null, "插件不使用坐标，也不会自动点击或切换会话。连接时，当前会话将写入 QwenPaw 现有的渠道访问控制；其他会话会进入统一的待审批列表。"), /* @__PURE__ */ e.createElement("div", { className: "dt-actions" }, /* @__PURE__ */ e.createElement(
    "button",
    {
      className: "dt-button dt-primary",
      disabled: !b || !E || !!p,
      onClick: () => void z("draft")
    },
    p === "draft" ? /* @__PURE__ */ e.createElement(I, { size: 16 }) : /* @__PURE__ */ e.createElement(ee, { size: 16 }),
    "一键连接并使用草稿"
  ), /* @__PURE__ */ e.createElement(
    "button",
    {
      className: "dt-button",
      disabled: !b || !E || !!p,
      onClick: () => void z("automatic")
    },
    /* @__PURE__ */ e.createElement(C, { size: 16 }),
    " 明确启用自动回复"
  )), /* @__PURE__ */ e.createElement("div", { className: "dt-notice" }, /* @__PURE__ */ e.createElement(L, { size: 18 }), /* @__PURE__ */ e.createElement("span", null, "建议先使用草稿模式。已授权", " ", (t == null ? void 0 : t.access_control.whitelist_count) ?? 0, " 个会话，待审批", " ", (t == null ? void 0 : t.access_control.pending_count) ?? 0, " 个；请在渠道页顶部的 待审批入口统一处理。")))), /* @__PURE__ */ e.createElement("section", { className: "dt-section" }, /* @__PURE__ */ e.createElement("div", { className: "dt-section-top" }, /* @__PURE__ */ e.createElement("h2", null, "待审批草稿"), /* @__PURE__ */ e.createElement("span", { className: "dt-meta" }, f.length, " 条")), f.length === 0 ? /* @__PURE__ */ e.createElement("div", { className: "dt-card dt-empty" }, "暂无待审批草稿") : f.map((a) => /* @__PURE__ */ e.createElement("article", { className: "dt-card dt-draft", key: a.id }, /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "dt-conversation" }, a.conversation), /* @__PURE__ */ e.createElement("div", { className: "dt-meta" }, new Date(a.created_at * 1e3).toLocaleString())), /* @__PURE__ */ e.createElement("div", { className: "dt-copy" }, a.text), /* @__PURE__ */ e.createElement("div", { className: "dt-actions" }, /* @__PURE__ */ e.createElement(
    "button",
    {
      className: "dt-button dt-primary",
      disabled: p === a.id,
      onClick: () => void A(a, "send")
    },
    /* @__PURE__ */ e.createElement(C, { size: 15 }),
    " 发送"
  ), /* @__PURE__ */ e.createElement(
    "button",
    {
      className: "dt-button dt-danger",
      disabled: p === a.id,
      onClick: () => void A(a, "delete")
    },
    /* @__PURE__ */ e.createElement(se, { size: 15 }),
    " 删除"
  ))))), y && /* @__PURE__ */ e.createElement("div", { className: "dt-error" }, y)));
}
var M, $;
($ = (M = window.QwenPaw).registerRoutes) == null || $.call(M, "dingtalk-desktop", [
  {
    path: "/plugin/dingtalk-desktop",
    component: ie,
    label: "阿里钉 · Agent",
    icon: "message-square-text",
    priority: 44
  }
]);
