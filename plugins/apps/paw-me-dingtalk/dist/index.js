const Se = window.QwenPaw.host.React, ne = Se.createElement, De = Se.forwardRef;
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Je = (n) => n.replace(/([a-z0-9])([A-Z])/g, "$1-$2").toLowerCase(), Ye = (n) => n.replace(
  /^([A-Z])|[\s-_]+(\w)/g,
  (s, v, t) => t ? t.toUpperCase() : v.toLowerCase()
), pe = (n) => {
  const s = Ye(n);
  return s.charAt(0).toUpperCase() + s.slice(1);
}, qe = (...n) => n.filter((s, v, t) => !!s && s.trim() !== "" && t.indexOf(s) === v).join(" ").trim(), et = (n) => {
  for (const s in n)
    if (s.startsWith("aria-") || s === "role" || s === "title")
      return !0;
};
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
var tt = {
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
const at = De(
  ({
    color: n = "currentColor",
    size: s = 24,
    strokeWidth: v = 2,
    absoluteStrokeWidth: t,
    className: N = "",
    children: l,
    iconNode: A,
    ...L
  }, P) => ne(
    "svg",
    {
      ref: P,
      ...tt,
      width: s,
      height: s,
      stroke: n,
      strokeWidth: t ? Number(v) * 24 / Number(s) : v,
      className: qe("lucide", N),
      ...!l && !et(L) && { "aria-hidden": "true" },
      ...L
    },
    [
      ...A.map(([y, o]) => ne(y, o)),
      ...Array.isArray(l) ? l : [l]
    ]
  )
);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const d = (n, s) => {
  const v = De(
    ({ className: t, ...N }, l) => ne(at, {
      ref: l,
      iconNode: s,
      className: qe(
        `lucide-${Je(pe(n))}`,
        `lucide-${n}`,
        t
      ),
      ...N
    })
  );
  return v.displayName = pe(n), v;
};
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const nt = [
  [
    "path",
    {
      d: "M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2",
      key: "169zse"
    }
  ]
], de = d("activity", nt);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const rt = [["path", { d: "M20 6 9 17l-5-5", key: "1gmf2c" }]], ue = d("check", rt);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const lt = [
  ["circle", { cx: "12", cy: "12", r: "10", key: "1mglay" }],
  ["line", { x1: "10", x2: "10", y1: "15", y2: "9", key: "c1nkhi" }],
  ["line", { x1: "14", x2: "14", y1: "15", y2: "9", key: "h65svq" }]
], it = d("circle-pause", lt);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const st = [["circle", { cx: "12", cy: "12", r: "10", key: "1mglay" }]], ct = d("circle", st);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const ot = [
  ["path", { d: "M12 6v6h4", key: "135r8i" }],
  ["circle", { cx: "12", cy: "12", r: "10", key: "1mglay" }]
], mt = d("clock-3", ot);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const pt = [
  ["path", { d: "M12 15V3", key: "m9g1x1" }],
  ["path", { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4", key: "ih7n3h" }],
  ["path", { d: "m7 10 5 5 5-5", key: "brsn70" }]
], ge = d("download", pt);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const dt = [
  ["path", { d: "M15 3h6v6", key: "1q9fwt" }],
  ["path", { d: "M10 14 21 3", key: "gplh6r" }],
  ["path", { d: "M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6", key: "a6xqqp" }]
], ye = d("external-link", dt);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const ut = [
  ["polyline", { points: "22 12 16 12 14 15 10 15 8 12 2 12", key: "o97t9d" }],
  [
    "path",
    {
      d: "M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z",
      key: "oot6mr"
    }
  ]
], Ee = d("inbox", ut);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const gt = [
  ["circle", { cx: "12", cy: "16", r: "1", key: "1au0dj" }],
  ["rect", { x: "3", y: "10", width: "18", height: "12", rx: "2", key: "6s8ecr" }],
  ["path", { d: "M7 10V7a5 5 0 0 1 10 0v3", key: "1pqi11" }]
], yt = d("lock-keyhole", gt);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Et = [
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
], ve = d("message-square-text", Et);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const vt = [
  ["path", { d: "M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8", key: "v9h5vc" }],
  ["path", { d: "M21 3v5h-5", key: "1q7to0" }],
  ["path", { d: "M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16", key: "3uifl3" }],
  ["path", { d: "M8 16H3v5", key: "1cv678" }]
], H = d("refresh-cw", vt);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const ht = [
  [
    "path",
    {
      d: "M14.536 21.686a.5.5 0 0 0 .937-.024l6.5-19a.496.496 0 0 0-.635-.635l-19 6.5a.5.5 0 0 0-.024.937l7.93 3.18a2 2 0 0 1 1.112 1.11z",
      key: "1ffxy3"
    }
  ],
  ["path", { d: "m21.854 2.147-10.94 10.939", key: "12cjpa" }]
], Y = d("send", ht);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const ft = [
  ["path", { d: "M14 17H5", key: "gfn3mx" }],
  ["path", { d: "M19 7h-9", key: "6i9tg" }],
  ["circle", { cx: "17", cy: "17", r: "3", key: "18b49y" }],
  ["circle", { cx: "7", cy: "7", r: "3", key: "dfmy0x" }]
], xt = d("settings-2", ft);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const bt = [
  [
    "path",
    {
      d: "M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z",
      key: "oel41y"
    }
  ],
  ["path", { d: "m9 12 2 2 4-4", key: "dzmm74" }]
], ee = d("shield-check", bt);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const wt = [
  ["path", { d: "M10 11v6", key: "nco0om" }],
  ["path", { d: "M14 11v6", key: "outv1u" }],
  ["path", { d: "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6", key: "miytrc" }],
  ["path", { d: "M3 6h18", key: "d0wm0j" }],
  ["path", { d: "M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2", key: "e791ji" }]
], he = d("trash-2", wt);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const _t = [
  ["path", { d: "m16 11 2 2 4-4", key: "9rsbq5" }],
  ["path", { d: "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2", key: "1yyitq" }],
  ["circle", { cx: "9", cy: "7", r: "4", key: "nufk8" }]
], fe = d("user-check", _t);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const kt = [
  ["path", { d: "M18 6 6 18", key: "1bl5f8" }],
  ["path", { d: "m6 6 12 12", key: "d8bk6v" }]
], Nt = d("x", kt), re = "paw-me-dingtalk", O = window.QwenPaw.host, e = O.React, { useEffect: Ct, useMemo: zt, useState: w } = e, {
  Alert: T,
  Badge: At,
  Button: c,
  Card: x,
  Col: V,
  Descriptions: j,
  Drawer: Mt,
  Empty: xe,
  Form: b,
  Input: It,
  InputNumber: be,
  List: _,
  Modal: we,
  Popconfirm: _e,
  Progress: $t,
  Row: Pt,
  Select: k,
  Space: C,
  Spin: ke,
  Switch: Ne,
  Table: St,
  Tabs: Dt,
  Tag: $,
  Timeline: qt,
  Typography: Tt
} = O.antd, { Text: p, Title: te } = Tt, Ce = `
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
@media(max-width:760px){.pm-page{padding:16px 12px 32px}.pm-header{flex-direction:column}.pm-actions{justify-content:flex-start}.pm-status-inner{align-items:flex-start;flex-direction:column}.pm-status-detail{white-space:normal}.pm-policy-grid,.pm-global-grid{grid-template-columns:1fr}.pm-onboarding{margin-top:18px}.pm-onboarding .ant-card-body{padding:20px}.pm-steps{grid-template-columns:1fr}.pm-source-head{align-items:flex-start;flex-direction:column}}
`, jt = {
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
  failed: "失败"
};
function Q(n) {
  return n ? new Date(n * 1e3).toLocaleString() : "—";
}
function ae(n) {
  return n === "oauth:dws-event" ? "钉钉 OAuth 事件" : n || "无可信来源";
}
function ze({ status: n }) {
  const s = n === "sent" ? "success" : n === "failed" || n === "blocked" ? "error" : n === "draft_ready" || n === "identity_required" || n === "needs_review" ? "warning" : "processing";
  return /* @__PURE__ */ e.createElement($, { color: s }, jt[n] || n);
}
function Ae() {
  const n = zt(() => {
    var a;
    return (a = window.QwenPaw.paw) == null ? void 0 : a.forApp(re);
  }, []), [s, v] = w([]), [t, N] = w(null), [l, A] = w(
    (n == null ? void 0 : n.host.getSelectedAgentId()) || "default"
  ), [L, P] = w(!0), [y, o] = w(!1), [B, g] = w(""), [Te, K] = w(!1), [m, U] = w(
    null
  ), [M, W] = w(null), [R, le] = w(""), [Z] = b.useForm(), [G] = b.useForm(), i = n == null ? void 0 : n.api, h = async (a = l, r = !1) => {
    if (!i) {
      g("当前 QwenPaw 版本未提供 PawApp SDK"), P(!1);
      return;
    }
    r || P(!0);
    try {
      const u = await i.get("/snapshot", {
        query: { agent_id: a }
      });
      N(u), u.settings.agent_id && u.settings.agent_id !== l && A(u.settings.agent_id), g("");
    } catch (u) {
      g(u instanceof Error ? u.message : "状态加载失败");
    } finally {
      r || P(!1);
    }
  };
  Ct(() => {
    let a = !1;
    (async () => {
      try {
        const E = await (O.fetch ? await O.fetch("/agents") : await fetch(O.getApiUrl("/agents"))).json();
        a || v(
          (E.agents || []).filter(
            (I) => I.enabled && I.available_in_chat !== !1
          )
        );
      } catch {
        a || v([]);
      }
      a || await h(l);
    })();
    const u = window.setInterval(
      () => void h(l, !0),
      2e3
    );
    return () => {
      a = !0, window.clearInterval(u);
    };
  }, [l]);
  const S = async (a) => {
    if (i) {
      o(!0);
      try {
        const r = await i.put("/settings", a, {
          query: { agent_id: String(a.agent_id) }
        });
        A(String(a.agent_id)), N(r), K(!1), await (n == null ? void 0 : n.host.toast("Paw Me 设置已保存", "success"));
      } catch (r) {
        g(r instanceof Error ? r.message : "设置保存失败");
      } finally {
        o(!1);
      }
    }
  }, je = async (a) => {
    t && await S({ ...t.settings, enabled: a, agent_id: l });
  }, ie = async (a, r) => {
    t && await S({
      ...t.settings,
      [a]: r,
      agent_id: l
    });
  }, Oe = async (a) => {
    A(a), t && await S({
      ...t.settings,
      agent_id: a
    });
  }, Le = () => {
    Z.setFieldsValue({ ...t == null ? void 0 : t.settings, agent_id: l }), K(!0);
  }, se = (a) => {
    G.setFieldsValue({
      policy: (t == null ? void 0 : t.settings.default_policy) || "draft"
    }), U(a);
  }, Be = async (a) => {
    if (!(!i || !m)) {
      o(!0);
      try {
        await i.post(`/work-items/${m.id}/authorize`, a), U(null), await h(l), await (n == null ? void 0 : n.host.toast("真实身份已授权", "success"));
      } catch (r) {
        g(r instanceof Error ? r.message : "身份授权失败");
      } finally {
        o(!1);
      }
    }
  }, D = async (a) => {
    if (i) {
      o(!0);
      try {
        await i.post(`/dws/${a}`), await h(l, !0);
      } catch (r) {
        g(r instanceof Error ? r.message : "钉钉连接失败");
      } finally {
        o(!1);
      }
    }
  }, Re = async () => {
    if (i) {
      o(!0);
      try {
        await i.post("/dws/cancel"), await h(l, !0);
      } catch (a) {
        g(a instanceof Error ? a.message : "取消操作失败");
      } finally {
        o(!1);
      }
    }
  }, Fe = async () => {
    if (i) {
      o(!0);
      try {
        const a = await i.post("/identity/confirm");
        N(a), await (n == null ? void 0 : n.host.toast("本人钉钉账号已确认", "success"));
      } catch (a) {
        g(a instanceof Error ? a.message : "账号确认失败");
      } finally {
        o(!1);
      }
    }
  }, ce = async () => {
    if (i) {
      o(!0);
      try {
        const a = await i.post("/identity/reconnect");
        N(a);
      } catch (a) {
        g(a instanceof Error ? a.message : "重新连接失败");
      } finally {
        o(!1);
      }
    }
  }, He = async (a) => {
    i && (await i.delete(`/principals/${a}`), await h(l));
  }, Ve = async (a, r) => {
    if (i)
      try {
        await i.patch(`/principals/${a}/policy`, { policy: r }), await h(l, !0);
      } catch (u) {
        g(u instanceof Error ? u.message : "策略更新失败");
      }
  }, Qe = async (a) => {
    if (i) {
      o(!0);
      try {
        await i.post(`/outbox/${a}/send`), await h(l);
      } catch (r) {
        g(r instanceof Error ? r.message : "发送失败");
      } finally {
        o(!1);
      }
    }
  }, Ke = async (a) => {
    i && (await i.delete(`/outbox/${a}`), await h(l));
  }, Ue = async () => {
    if (!(!i || !M || !R.trim())) {
      o(!0);
      try {
        await i.patch(`/outbox/${M.id}`, {
          text: R.trim()
        }), W(null), await h(l), await (n == null ? void 0 : n.host.toast("草稿已保存", "success"));
      } catch (a) {
        g(a instanceof Error ? a.message : "草稿保存失败");
      } finally {
        o(!1);
      }
    }
  };
  if (L && !t)
    return /* @__PURE__ */ e.createElement("div", { className: "pm-page" }, /* @__PURE__ */ e.createElement(ke, null));
  const X = (t == null ? void 0 : t.work_items.filter((a) => a.status === "identity_required")) || [], oe = (t == null ? void 0 : t.outbox.filter((a) => a.status !== "sent")) || [], f = !!(t != null && t.identity_provider.authenticated), z = !!(t != null && t.identity_provider.confirmed), q = !!(t != null && t.identity_provider.available), J = (t == null ? void 0 : t.runtime.integration_stage) || "idle", F = [
    "install",
    "downloading",
    "preparing",
    "installing",
    "verifying",
    "login"
  ].includes(J);
  if (!f || !z) {
    const a = q ? !f || !z ? 1 : 2 : 0, r = (E) => E < a ? /* @__PURE__ */ e.createElement(ue, { size: 17 }) : /* @__PURE__ */ e.createElement(ct, { size: 17 }), u = q ? f ? z ? "选择负责回复的 Agent" : "确认数字分身的本人账号" : "连接你的钉钉账号" : "准备钉钉连接组件", me = (t == null ? void 0 : t.runtime.integration_detail) || (q ? f ? z ? "任意已启用 Agent 都可以负责回复，认证由 Agent 自己管理。" : "启用前核对组织与账号，避免数字分身以错误身份发言。" : "浏览器将打开钉钉官方 OAuth；插件不会读取或保存账号密码。" : "组件安装在 Paw Me 的独立目录，不修改系统 PATH。");
    return /* @__PURE__ */ e.createElement("div", { className: "pm-page" }, /* @__PURE__ */ e.createElement("style", null, Ce), /* @__PURE__ */ e.createElement("header", { className: "pm-header" }, /* @__PURE__ */ e.createElement("div", { className: "pm-header-copy" }, /* @__PURE__ */ e.createElement("div", { className: "pm-eyebrow" }, /* @__PURE__ */ e.createElement(ee, { size: 15 }), "Paw Me · Digital Twin"), /* @__PURE__ */ e.createElement(te, { level: 1 }, "钉钉数字人分身"), /* @__PURE__ */ e.createElement(p, { type: "secondary" }, "首次配置只需要安装连接组件、完成钉钉授权并选择 Agent。"))), B ? /* @__PURE__ */ e.createElement(
      T,
      {
        closable: !0,
        type: "error",
        message: "操作未完成",
        description: B,
        onClose: () => g(""),
        style: { marginBottom: 16 }
      }
    ) : null, /* @__PURE__ */ e.createElement(x, { className: "pm-onboarding" }, /* @__PURE__ */ e.createElement("div", { className: "pm-onboarding-head" }, /* @__PURE__ */ e.createElement(te, { level: 2 }, "开始设置 Paw Me"), /* @__PURE__ */ e.createElement(p, { type: "secondary" }, "完成下面三个步骤后，消息监听、会话授权、草稿与发送会在同一页面运行。")), /* @__PURE__ */ e.createElement("div", { className: "pm-steps" }, ["安装连接组件", "钉钉 OAuth", "选择并启用 Agent"].map(
      (E, I) => /* @__PURE__ */ e.createElement(
        "div",
        {
          className: `pm-step ${I === a ? "pm-step-current" : ""} ${I < a ? "pm-step-done" : ""}`,
          key: E
        },
        /* @__PURE__ */ e.createElement("span", { className: "pm-step-icon" }, r(I)),
        /* @__PURE__ */ e.createElement("span", null, E)
      )
    )), /* @__PURE__ */ e.createElement("div", { className: "pm-onboarding-action" }, /* @__PURE__ */ e.createElement("h3", null, u), /* @__PURE__ */ e.createElement(p, { type: "secondary" }, me), F ? /* @__PURE__ */ e.createElement("div", { className: "pm-progress" }, /* @__PURE__ */ e.createElement(
      $t,
      {
        percent: (t == null ? void 0 : t.runtime.integration_progress) ?? 0,
        showInfo: (t == null ? void 0 : t.runtime.integration_progress) != null,
        status: "active"
      }
    ), (t == null ? void 0 : t.runtime.integration_progress) == null ? /* @__PURE__ */ e.createElement(C, { size: 8 }, /* @__PURE__ */ e.createElement(ke, { size: "small" }), /* @__PURE__ */ e.createElement(p, { type: "secondary" }, "正在执行当前阶段")) : null) : null, f && !z ? /* @__PURE__ */ e.createElement("div", { className: "pm-account" }, /* @__PURE__ */ e.createElement(j, { column: 1, size: "small" }, /* @__PURE__ */ e.createElement(j.Item, { label: "账号" }, (t == null ? void 0 : t.identity_provider.user_name) || "未返回显示名"), /* @__PURE__ */ e.createElement(j.Item, { label: "组织" }, (t == null ? void 0 : t.identity_provider.corp_name) || "未返回组织名"), /* @__PURE__ */ e.createElement(j.Item, { label: "真实 userId" }, /* @__PURE__ */ e.createElement("span", { className: "pm-id" }, (t == null ? void 0 : t.identity_provider.user_id) || "—")))) : null, z ? /* @__PURE__ */ e.createElement(
      k,
      {
        className: "pm-agent-select",
        value: l,
        options: s.map((E) => ({
          value: E.id,
          label: `${E.name || E.id} · ${E.backend || "agent"}`
        })),
        onChange: (E) => A(E)
      }
    ) : null, /* @__PURE__ */ e.createElement("div", { className: "pm-onboarding-buttons" }, q ? f ? z ? /* @__PURE__ */ e.createElement(
      c,
      {
        type: "primary",
        size: "large",
        icon: /* @__PURE__ */ e.createElement(ue, { size: 17 }),
        loading: y,
        disabled: !l,
        onClick: () => void S({
          enabled: !0,
          agent_id: l,
          default_policy: (t == null ? void 0 : t.settings.default_policy) || "draft",
          access_mode: (t == null ? void 0 : t.settings.access_mode) || "approval",
          quiet_seconds: (t == null ? void 0 : t.settings.quiet_seconds) ?? 4,
          max_wait_seconds: (t == null ? void 0 : t.settings.max_wait_seconds) ?? 20
        })
      },
      "启用数字人分身"
    ) : /* @__PURE__ */ e.createElement(e.Fragment, null, /* @__PURE__ */ e.createElement(
      c,
      {
        type: "primary",
        size: "large",
        icon: /* @__PURE__ */ e.createElement(fe, { size: 17 }),
        loading: y,
        onClick: () => void Fe()
      },
      "确认这是我"
    ), /* @__PURE__ */ e.createElement(
      c,
      {
        size: "large",
        icon: /* @__PURE__ */ e.createElement(H, { size: 17 }),
        disabled: y,
        onClick: () => void ce()
      },
      "不是我，重新连接"
    )) : /* @__PURE__ */ e.createElement(
      c,
      {
        type: "primary",
        size: "large",
        icon: /* @__PURE__ */ e.createElement(ye, { size: 17 }),
        disabled: F,
        onClick: () => void D("login")
      },
      "连接钉钉"
    ) : /* @__PURE__ */ e.createElement(
      c,
      {
        type: "primary",
        size: "large",
        icon: /* @__PURE__ */ e.createElement(ge, { size: 17 }),
        disabled: F,
        onClick: () => void D("install")
      },
      "安装并继续"
    ), F ? /* @__PURE__ */ e.createElement(
      c,
      {
        size: "large",
        icon: /* @__PURE__ */ e.createElement(Nt, { size: 17 }),
        loading: y,
        onClick: () => void Re()
      },
      "取消当前操作"
    ) : J === "failed" || J === "cancelled" ? /* @__PURE__ */ e.createElement(
      c,
      {
        size: "large",
        icon: /* @__PURE__ */ e.createElement(H, { size: 17 }),
        onClick: () => void D(q ? "login" : "install")
      },
      "重新尝试"
    ) : null))));
  }
  const We = /* @__PURE__ */ e.createElement(
    x,
    {
      className: "pm-panel",
      title: "消息批次",
      extra: /* @__PURE__ */ e.createElement(p, { type: "secondary" }, "连续消息只回复一次")
    },
    /* @__PURE__ */ e.createElement(
      _,
      {
        dataSource: (t == null ? void 0 : t.work_items) || [],
        locale: { emptyText: /* @__PURE__ */ e.createElement(xe, { description: "尚未捕获新消息" }) },
        renderItem: (a) => /* @__PURE__ */ e.createElement(
          _.Item,
          {
            actions: a.status === "identity_required" ? [
              /* @__PURE__ */ e.createElement(
                c,
                {
                  key: "authorize",
                  type: "primary",
                  onClick: () => se(a)
                },
                "审核并授权"
              )
            ] : []
          },
          /* @__PURE__ */ e.createElement(
            _.Item.Meta,
            {
              title: /* @__PURE__ */ e.createElement("div", { className: "pm-item-title" }, /* @__PURE__ */ e.createElement("span", null, a.conversation_alias), /* @__PURE__ */ e.createElement(ze, { status: a.status }), /* @__PURE__ */ e.createElement($, null, a.message_count, " 条已合并")),
              description: /* @__PURE__ */ e.createElement(e.Fragment, null, /* @__PURE__ */ e.createElement("span", null, a.agent_id, " · ", Q(a.updated_at)), /* @__PURE__ */ e.createElement("div", { className: "pm-id" }, a.subject_type === "person" ? "人员" : "群聊", " ·", " ", a.subject_id || "未获得真实 ID", " ·", " ", ae(a.id_source)), a.error ? /* @__PURE__ */ e.createElement("div", { className: "pm-error" }, a.error) : null, /* @__PURE__ */ e.createElement("div", { className: "pm-message-stack" }, a.messages.map((r) => /* @__PURE__ */ e.createElement("div", { className: "pm-message", key: r.id }, r.text))))
            }
          )
        )
      }
    )
  ), Ze = /* @__PURE__ */ e.createElement(x, { className: "pm-panel", title: "OAuth、身份与权限" }, /* @__PURE__ */ e.createElement("div", { className: "pm-setup" }, /* @__PURE__ */ e.createElement("div", { className: "pm-setup-copy" }, /* @__PURE__ */ e.createElement("div", { className: "pm-setup-title" }, f ? `${(t == null ? void 0 : t.identity_provider.user_name) || "钉钉账号"} 已连接` : t != null && t.identity_provider.available ? "连接组件已就绪，等待 OAuth 登录" : "安装钉钉连接组件"), /* @__PURE__ */ e.createElement(p, { type: "secondary" }, (t == null ? void 0 : t.runtime.integration_detail) || (t == null ? void 0 : t.identity_provider.detail) || "OAuth 由钉钉官方能力管理，插件不读取或保存令牌。"), f ? /* @__PURE__ */ e.createElement("div", { className: "pm-id" }, (t == null ? void 0 : t.identity_provider.corp_name) || "当前组织", " · userId", " ", (t == null ? void 0 : t.identity_provider.user_id) || "—") : null), t != null && t.identity_provider.available ? f ? /* @__PURE__ */ e.createElement(C, { wrap: !0 }, /* @__PURE__ */ e.createElement(
    c,
    {
      icon: /* @__PURE__ */ e.createElement(H, { size: 16 }),
      onClick: () => void h(l)
    },
    "刷新状态"
  ), /* @__PURE__ */ e.createElement(c, { onClick: () => void ce(), disabled: y }, "更换账号")) : /* @__PURE__ */ e.createElement(
    c,
    {
      type: "primary",
      icon: /* @__PURE__ */ e.createElement(ye, { size: 16 }),
      loading: y || (t == null ? void 0 : t.runtime.integration_stage) === "login",
      onClick: () => void D("login")
    },
    "使用钉钉 OAuth 登录"
  ) : /* @__PURE__ */ e.createElement(
    c,
    {
      type: "primary",
      icon: /* @__PURE__ */ e.createElement(ge, { size: 16 }),
      loading: y || (t == null ? void 0 : t.runtime.integration_stage) === "install",
      onClick: () => void D("install")
    },
    "安装连接组件"
  )), /* @__PURE__ */ e.createElement(
    T,
    {
      showIcon: !0,
      type: "info",
      message: "单会话规则只来自收到的真实事件",
      description: "人员 openDingTalkId 或群 openConversationId 由钉钉 OAuth 事件写入，界面不可手填。没有单会话规则时继承上方全局策略。",
      style: { marginBottom: 16 }
    }
  ), X.length ? /* @__PURE__ */ e.createElement(
    _,
    {
      header: /* @__PURE__ */ e.createElement("strong", null, "待授权会话"),
      dataSource: X,
      renderItem: (a) => /* @__PURE__ */ e.createElement(
        _.Item,
        {
          actions: [
            /* @__PURE__ */ e.createElement(
              c,
              {
                key: "authorize",
                type: "primary",
                onClick: () => se(a)
              },
              "审核并授权"
            )
          ]
        },
        /* @__PURE__ */ e.createElement(
          _.Item.Meta,
          {
            title: a.display_name || a.conversation_alias,
            description: /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "pm-id" }, a.subject_id), /* @__PURE__ */ e.createElement(p, { type: "secondary" }, a.subject_type === "person" ? "人员" : "群聊", " ·", " ", a.id_source))
          }
        )
      )
    }
  ) : null, /* @__PURE__ */ e.createElement(
    St,
    {
      rowKey: "id",
      pagination: !1,
      dataSource: (t == null ? void 0 : t.principals) || [],
      locale: { emptyText: "暂无已验证身份" },
      columns: [
        {
          title: "身份",
          render: (a, r) => /* @__PURE__ */ e.createElement(e.Fragment, null, /* @__PURE__ */ e.createElement("div", null, r.display_name), /* @__PURE__ */ e.createElement(p, { type: "secondary" }, r.subject_type === "person" ? "人员" : "群聊"))
        },
        {
          title: "真实 ID",
          render: (a, r) => /* @__PURE__ */ e.createElement(e.Fragment, null, /* @__PURE__ */ e.createElement("div", null, r.subject_id), /* @__PURE__ */ e.createElement(p, { type: "secondary" }, ae(r.id_source)))
        },
        { title: "会话", dataIndex: "conversation_alias" },
        {
          title: "策略",
          render: (a, r) => /* @__PURE__ */ e.createElement(
            k,
            {
              size: "small",
              value: r.policy,
              style: { width: 150 },
              options: [
                { value: "draft", label: "生成草稿" },
                { value: "automatic", label: "自动发送" },
                { value: "observe", label: "仅观察" },
                { value: "blocked", label: "阻止" }
              ],
              onChange: (u) => void Ve(r.id, u)
            }
          )
        },
        {
          title: "操作",
          render: (a, r) => /* @__PURE__ */ e.createElement(
            _e,
            {
              title: "删除此会话规则？后续消息将继承全局策略。",
              onConfirm: () => void He(r.id)
            },
            /* @__PURE__ */ e.createElement(c, { type: "text", danger: !0, icon: /* @__PURE__ */ e.createElement(he, { size: 15 }) }, "删除")
          )
        }
      ],
      scroll: { x: 760 }
    }
  )), Ge = /* @__PURE__ */ e.createElement(
    x,
    {
      className: "pm-panel",
      title: "待发送",
      extra: /* @__PURE__ */ e.createElement(p, { type: "secondary" }, "按 OAuth 真实 ID 精确发送")
    },
    /* @__PURE__ */ e.createElement(
      _,
      {
        dataSource: oe,
        locale: { emptyText: /* @__PURE__ */ e.createElement(xe, { description: "暂无待发送回复" }) },
        renderItem: (a) => /* @__PURE__ */ e.createElement(
          _.Item,
          {
            actions: [
              /* @__PURE__ */ e.createElement(
                c,
                {
                  key: "edit",
                  icon: /* @__PURE__ */ e.createElement(ve, { size: 15 }),
                  onClick: () => {
                    W(a), le(a.text);
                  }
                },
                "编辑"
              ),
              /* @__PURE__ */ e.createElement(
                c,
                {
                  key: "send",
                  type: "primary",
                  icon: /* @__PURE__ */ e.createElement(Y, { size: 15 }),
                  loading: y,
                  onClick: () => void Qe(a.id)
                },
                "发送"
              ),
              /* @__PURE__ */ e.createElement(
                _e,
                {
                  key: "delete",
                  title: "删除草稿？原始消息仍会保留。",
                  onConfirm: () => void Ke(a.id)
                },
                /* @__PURE__ */ e.createElement(c, { danger: !0, type: "text", icon: /* @__PURE__ */ e.createElement(he, { size: 15 }) }, "删除")
              )
            ]
          },
          /* @__PURE__ */ e.createElement(
            _.Item.Meta,
            {
              title: /* @__PURE__ */ e.createElement("div", { className: "pm-item-title" }, /* @__PURE__ */ e.createElement("span", null, a.conversation_alias), /* @__PURE__ */ e.createElement(ze, { status: a.status })),
              description: /* @__PURE__ */ e.createElement(e.Fragment, null, /* @__PURE__ */ e.createElement("div", { className: "pm-source" }, /* @__PURE__ */ e.createElement("div", { className: "pm-source-head" }, /* @__PURE__ */ e.createElement("strong", null, a.source_display_name || a.conversation_alias), /* @__PURE__ */ e.createElement(p, { type: "secondary" }, a.source_subject_type === "group" ? "群聊消息" : "单聊消息")), /* @__PURE__ */ e.createElement("div", { className: "pm-message-stack" }, (a.source_messages || []).map((r) => /* @__PURE__ */ e.createElement("div", { className: "pm-message", key: r.id }, r.text, /* @__PURE__ */ e.createElement("div", { className: "pm-meta" }, Q(r.received_at)))))), /* @__PURE__ */ e.createElement("div", { className: "pm-draft" }, /* @__PURE__ */ e.createElement(p, { type: "secondary" }, "准备发送的回复"), /* @__PURE__ */ e.createElement("p", { className: "pm-pre" }, a.text)), a.error ? /* @__PURE__ */ e.createElement("div", { className: "pm-error" }, a.error) : null, /* @__PURE__ */ e.createElement("div", { className: "pm-meta" }, Q(a.updated_at)))
            }
          )
        )
      }
    )
  ), Xe = /* @__PURE__ */ e.createElement(x, { className: "pm-panel", title: "运行记录" }, /* @__PURE__ */ e.createElement(
    qt,
    {
      items: ((t == null ? void 0 : t.activity) || []).map((a) => ({
        color: a.status === "failed" ? "red" : a.status === "sent" || a.status === "verified" ? "green" : "blue",
        children: /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "pm-item-title" }, /* @__PURE__ */ e.createElement("strong", null, a.title), /* @__PURE__ */ e.createElement($, null, a.status)), a.detail ? /* @__PURE__ */ e.createElement("div", { className: "pm-subtle" }, a.detail) : null, /* @__PURE__ */ e.createElement("div", { className: "pm-meta" }, Q(a.created_at)))
      }))
    }
  ));
  return /* @__PURE__ */ e.createElement("div", { className: "pm-page" }, /* @__PURE__ */ e.createElement("style", null, Ce), /* @__PURE__ */ e.createElement("header", { className: "pm-header" }, /* @__PURE__ */ e.createElement("div", { className: "pm-header-copy" }, /* @__PURE__ */ e.createElement("div", { className: "pm-eyebrow" }, /* @__PURE__ */ e.createElement(ee, { size: 15 }), "Paw Me · Digital Twin"), /* @__PURE__ */ e.createElement(te, { level: 1 }, "钉钉数字人分身"), /* @__PURE__ */ e.createElement(p, { type: "secondary" }, "使用所选 Agent 和本机钉钉 OAuth 登录态，在一个页面完成实时收件、 独立授权、上下文聚合、处理、草稿、发送与审计。")), /* @__PURE__ */ e.createElement("div", { className: "pm-actions" }, /* @__PURE__ */ e.createElement(
    k,
    {
      value: l,
      style: { minWidth: 190 },
      options: s.map((a) => ({
        value: a.id,
        label: `${a.name || a.id} · ${a.backend || "agent"}`
      })),
      onChange: (a) => void Oe(a)
    }
  ), /* @__PURE__ */ e.createElement(c, { icon: /* @__PURE__ */ e.createElement(xt, { size: 16 }), onClick: Le }, "设置"), /* @__PURE__ */ e.createElement(
    c,
    {
      icon: /* @__PURE__ */ e.createElement(H, { size: 16 }),
      onClick: () => void h(l)
    },
    "刷新"
  ), /* @__PURE__ */ e.createElement(C, null, /* @__PURE__ */ e.createElement(
    Ne,
    {
      checked: t == null ? void 0 : t.settings.enabled,
      disabled: !f,
      onChange: (a) => void je(a)
    }
  ), /* @__PURE__ */ e.createElement(p, null, t != null && t.settings.enabled ? "运行中" : "已停止")))), B ? /* @__PURE__ */ e.createElement(
    T,
    {
      closable: !0,
      type: "error",
      message: "操作未完成",
      description: B,
      onClose: () => g(""),
      style: { marginBottom: 16 }
    }
  ) : null, /* @__PURE__ */ e.createElement(x, { className: "pm-statusbar" }, /* @__PURE__ */ e.createElement("div", { className: "pm-status-inner" }, /* @__PURE__ */ e.createElement("div", { className: "pm-status-main" }, t != null && t.runtime.running ? /* @__PURE__ */ e.createElement(At, { status: "processing" }) : /* @__PURE__ */ e.createElement(it, { size: 18 }), /* @__PURE__ */ e.createElement("div", { className: "pm-status-text" }, /* @__PURE__ */ e.createElement("div", { className: "pm-status-title" }, (t == null ? void 0 : t.runtime.stage) || "stopped"), /* @__PURE__ */ e.createElement(p, { className: "pm-status-detail", type: "secondary" }, (t == null ? void 0 : t.runtime.detail) || "等待启动"))), /* @__PURE__ */ e.createElement(C, { wrap: !0 }, /* @__PURE__ */ e.createElement(
    $,
    {
      icon: /* @__PURE__ */ e.createElement(ee, { size: 13 }),
      color: f ? "success" : "warning"
    },
    f ? "钉钉 OAuth 已连接" : "等待钉钉 OAuth"
  ), /* @__PURE__ */ e.createElement($, { icon: /* @__PURE__ */ e.createElement(mt, { size: 13 }) }, "静默 ", (t == null ? void 0 : t.settings.quiet_seconds) ?? 4, " 秒"), t != null && t.runtime.current_conversation ? /* @__PURE__ */ e.createElement($, { icon: /* @__PURE__ */ e.createElement(ve, { size: 13 }) }, t.runtime.current_conversation) : null))), /* @__PURE__ */ e.createElement(x, { className: "pm-global", title: "全局访问与回复策略" }, /* @__PURE__ */ e.createElement("div", { className: "pm-global-grid" }, /* @__PURE__ */ e.createElement("div", { className: "pm-global-field" }, /* @__PURE__ */ e.createElement("div", { className: "pm-global-label" }, "新会话默认访问规则"), /* @__PURE__ */ e.createElement(
    k,
    {
      value: (t == null ? void 0 : t.settings.access_mode) || "approval",
      options: [
        {
          value: "approval",
          label: "逐个审批（推荐）"
        },
        { value: "allow_all", label: "全白名单" },
        { value: "block_all", label: "全黑名单" }
      ],
      onChange: (a) => void ie("access_mode", a)
    }
  ), /* @__PURE__ */ e.createElement(p, { type: "secondary" }, "单会话规则始终优先；删除单会话规则后恢复继承全局。")), /* @__PURE__ */ e.createElement("div", { className: "pm-global-field" }, /* @__PURE__ */ e.createElement("div", { className: "pm-global-label" }, "允许回复时的默认方式"), /* @__PURE__ */ e.createElement(
    k,
    {
      value: (t == null ? void 0 : t.settings.default_policy) || "draft",
      options: [
        { value: "draft", label: "先进入待发送" },
        { value: "automatic", label: "生成后自动发送" }
      ],
      onChange: (a) => void ie("default_policy", a)
    }
  ), /* @__PURE__ */ e.createElement(p, { type: "secondary" }, "即使选择自动发送，身份泄漏或元分析也会强制留在草稿。")))), /* @__PURE__ */ e.createElement(Pt, { gutter: [14, 14] }, /* @__PURE__ */ e.createElement(V, { xs: 12, lg: 6 }, /* @__PURE__ */ e.createElement(x, { className: "pm-metric" }, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-icon" }, /* @__PURE__ */ e.createElement(Ee, { size: 18 })), /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-value" }, (t == null ? void 0 : t.work_items.length) || 0), /* @__PURE__ */ e.createElement("div", { className: "pm-metric-label" }, "消息批次")))), /* @__PURE__ */ e.createElement(V, { xs: 12, lg: 6 }, /* @__PURE__ */ e.createElement(x, { className: "pm-metric" }, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-icon" }, /* @__PURE__ */ e.createElement(yt, { size: 18 })), /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-value" }, X.length), /* @__PURE__ */ e.createElement("div", { className: "pm-metric-label" }, "待绑定身份")))), /* @__PURE__ */ e.createElement(V, { xs: 12, lg: 6 }, /* @__PURE__ */ e.createElement(x, { className: "pm-metric" }, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-icon" }, /* @__PURE__ */ e.createElement(Y, { size: 18 })), /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-value" }, oe.length), /* @__PURE__ */ e.createElement("div", { className: "pm-metric-label" }, "待发送")))), /* @__PURE__ */ e.createElement(V, { xs: 12, lg: 6 }, /* @__PURE__ */ e.createElement(x, { className: "pm-metric" }, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-icon" }, /* @__PURE__ */ e.createElement(de, { size: 18 })), /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-value" }, (t == null ? void 0 : t.principals.length) || 0), /* @__PURE__ */ e.createElement("div", { className: "pm-metric-label" }, "已验证身份"))))), /* @__PURE__ */ e.createElement(
    Dt,
    {
      defaultActiveKey: "inbox",
      items: [
        {
          key: "inbox",
          label: /* @__PURE__ */ e.createElement(C, null, /* @__PURE__ */ e.createElement(Ee, { size: 15 }), "收件与处理"),
          children: We
        },
        {
          key: "permissions",
          label: /* @__PURE__ */ e.createElement(C, null, /* @__PURE__ */ e.createElement(fe, { size: 15 }), "身份与权限"),
          children: Ze
        },
        {
          key: "outbox",
          label: /* @__PURE__ */ e.createElement(C, null, /* @__PURE__ */ e.createElement(Y, { size: 15 }), "待发送"),
          children: Ge
        },
        {
          key: "activity",
          label: /* @__PURE__ */ e.createElement(C, null, /* @__PURE__ */ e.createElement(de, { size: 15 }), "运行记录"),
          children: Xe
        }
      ]
    }
  ), /* @__PURE__ */ e.createElement(
    Mt,
    {
      title: "运行设置",
      width: 420,
      open: Te,
      onClose: () => K(!1),
      destroyOnClose: !0,
      extra: /* @__PURE__ */ e.createElement(
        c,
        {
          type: "primary",
          loading: y,
          onClick: () => Z.submit()
        },
        "保存"
      )
    },
    /* @__PURE__ */ e.createElement(
      b,
      {
        form: Z,
        layout: "vertical",
        onFinish: S,
        initialValues: t == null ? void 0 : t.settings
      },
      /* @__PURE__ */ e.createElement(
        b.Item,
        {
          name: "agent_id",
          label: "回复消息的 Agent",
          rules: [{ required: !0 }]
        },
        /* @__PURE__ */ e.createElement(
          k,
          {
            options: s.map((a) => ({
              value: a.id,
              label: `${a.name || a.id} · ${a.backend || "agent"}`
            }))
          }
        )
      ),
      /* @__PURE__ */ e.createElement(
        b.Item,
        {
          name: "enabled",
          label: "数字人分身总开关",
          valuePropName: "checked"
        },
        /* @__PURE__ */ e.createElement(Ne, null)
      ),
      /* @__PURE__ */ e.createElement(b.Item, { name: "default_policy", label: "默认回复策略" }, /* @__PURE__ */ e.createElement(
        k,
        {
          options: [
            { value: "draft", label: "生成草稿，确认后发送" },
            { value: "automatic", label: "按身份策略自动发送" }
          ]
        }
      )),
      /* @__PURE__ */ e.createElement(b.Item, { name: "access_mode", label: "新会话默认访问规则" }, /* @__PURE__ */ e.createElement(
        k,
        {
          options: [
            { value: "approval", label: "逐个审批" },
            { value: "allow_all", label: "全白名单" },
            { value: "block_all", label: "全黑名单" }
          ]
        }
      )),
      /* @__PURE__ */ e.createElement(
        b.Item,
        {
          name: "quiet_seconds",
          label: "连续消息静默窗口（秒）",
          extra: "对方停止输入达到这个时间后，才合并调用一次 Agent。"
        },
        /* @__PURE__ */ e.createElement(be, { min: 1, max: 30, style: { width: "100%" } })
      ),
      /* @__PURE__ */ e.createElement(
        b.Item,
        {
          name: "max_wait_seconds",
          label: "最长聚合等待（秒）",
          extra: "持续聊天时也不会无限等待。"
        },
        /* @__PURE__ */ e.createElement(be, { min: 3, max: 120, style: { width: "100%" } })
      ),
      /* @__PURE__ */ e.createElement(
        T,
        {
          type: "info",
          showIcon: !0,
          message: "上下文不会因中断丢失",
          description: "每条原始消息先写入 SQLite。Agent 运行中新消息到达时，旧任务会停止，新任务在同一会话中携带完整批次继续。"
        }
      )
    )
  ), /* @__PURE__ */ e.createElement(
    we,
    {
      title: "授权真实钉钉会话",
      open: !!m,
      confirmLoading: y,
      onCancel: () => U(null),
      onOk: () => G.submit(),
      destroyOnClose: !0
    },
    /* @__PURE__ */ e.createElement(
      T,
      {
        type: "info",
        showIcon: !0,
        message: "ID 已由钉钉 OAuth 事件验证",
        description: "下列 ID 为只读值，不能手填或修改。授权后，相同真实 ID 的后续消息会按所选策略处理。",
        style: { marginBottom: 16 }
      }
    ),
    /* @__PURE__ */ e.createElement(
      j,
      {
        size: "small",
        column: 1,
        bordered: !0,
        style: { marginBottom: 18 },
        items: [
          {
            key: "name",
            label: "会话",
            children: (m == null ? void 0 : m.display_name) || (m == null ? void 0 : m.conversation_alias) || "—"
          },
          {
            key: "type",
            label: "类型",
            children: (m == null ? void 0 : m.subject_type) === "group" ? "群聊" : "人员"
          },
          {
            key: "id",
            label: "真实 ID",
            children: /* @__PURE__ */ e.createElement("span", { className: "pm-id" }, (m == null ? void 0 : m.subject_id) || "—")
          },
          {
            key: "source",
            label: "来源",
            children: ae(m == null ? void 0 : m.id_source)
          }
        ]
      }
    ),
    /* @__PURE__ */ e.createElement(
      b,
      {
        form: G,
        layout: "vertical",
        onFinish: Be
      },
      /* @__PURE__ */ e.createElement(
        b.Item,
        {
          name: "policy",
          label: "权限策略",
          rules: [{ required: !0 }]
        },
        /* @__PURE__ */ e.createElement(
          k,
          {
            options: [
              { value: "draft", label: "允许处理，生成草稿" },
              { value: "automatic", label: "允许处理并自动发送" },
              { value: "observe", label: "仅观察，不调用 Agent" },
              { value: "blocked", label: "阻止" }
            ]
          }
        )
      )
    )
  ), /* @__PURE__ */ e.createElement(
    we,
    {
      title: `编辑发给 ${(M == null ? void 0 : M.conversation_alias) || ""} 的草稿`,
      open: !!M,
      confirmLoading: y,
      okButtonProps: { disabled: !R.trim() },
      onCancel: () => W(null),
      onOk: () => void Ue(),
      destroyOnClose: !0
    },
    /* @__PURE__ */ e.createElement(
      It.TextArea,
      {
        autoSize: { minRows: 6, maxRows: 16 },
        value: R,
        onChange: (a) => le(a.target.value)
      }
    )
  ));
}
var Ie;
const Me = (Ie = window.QwenPaw.paw) == null ? void 0 : Ie.forApp(re);
var $e, Pe;
Me ? Me.ui.registerPage({
  path: "/apps/paw-me-dingtalk",
  label: "Paw Me · DingTalk",
  component: Ae
}) : (Pe = ($e = window.QwenPaw).registerRoutes) == null || Pe.call($e, re, [
  {
    path: "/apps/paw-me-dingtalk",
    component: Ae,
    label: "Paw Me · DingTalk"
  }
]);
