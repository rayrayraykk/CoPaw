const Ge = window.QwenPaw.host.React, ce = Ge.createElement, Ze = Ge.forwardRef;
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const vt = (n) => n.replace(/([a-z0-9])([A-Z])/g, "$1-$2").toLowerCase(), ft = (n) => n.replace(
  /^([A-Z])|[\s-_]+(\w)/g,
  (c, f, t) => t ? t.toUpperCase() : f.toLowerCase()
), Ne = (n) => {
  const c = ft(n);
  return c.charAt(0).toUpperCase() + c.slice(1);
}, Xe = (...n) => n.filter((c, f, t) => !!c && c.trim() !== "" && t.indexOf(c) === f).join(" ").trim(), ht = (n) => {
  for (const c in n)
    if (c.startsWith("aria-") || c === "role" || c === "title")
      return !0;
};
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
var xt = {
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
const bt = Ze(
  ({
    color: n = "currentColor",
    size: c = 24,
    strokeWidth: f = 2,
    absoluteStrokeWidth: t,
    className: w = "",
    children: s,
    iconNode: $,
    ...F
  }, j) => ce(
    "svg",
    {
      ref: j,
      ...xt,
      width: c,
      height: c,
      stroke: n,
      strokeWidth: t ? Number(f) * 24 / Number(c) : f,
      className: Xe("lucide", w),
      ...!s && !ht(F) && { "aria-hidden": "true" },
      ...F
    },
    [
      ...$.map(([E, p]) => ce(E, p)),
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
const u = (n, c) => {
  const f = Ze(
    ({ className: t, ...w }, s) => ce(bt, {
      ref: s,
      iconNode: c,
      className: Xe(
        `lucide-${vt(Ne(n))}`,
        `lucide-${n}`,
        t
      ),
      ...w
    })
  );
  return f.displayName = Ne(n), f;
};
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const wt = [
  [
    "path",
    {
      d: "M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2",
      key: "169zse"
    }
  ]
], Ce = u("activity", wt);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const _t = [["path", { d: "M20 6 9 17l-5-5", key: "1gmf2c" }]], ze = u("check", _t);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const kt = [
  ["circle", { cx: "12", cy: "12", r: "10", key: "1mglay" }],
  ["line", { x1: "10", x2: "10", y1: "15", y2: "9", key: "c1nkhi" }],
  ["line", { x1: "14", x2: "14", y1: "15", y2: "9", key: "h65svq" }]
], Nt = u("circle-pause", kt);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Ct = [["circle", { cx: "12", cy: "12", r: "10", key: "1mglay" }]], zt = u("circle", Ct);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const At = [
  ["path", { d: "M12 6v6h4", key: "135r8i" }],
  ["circle", { cx: "12", cy: "12", r: "10", key: "1mglay" }]
], It = u("clock-3", At);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Mt = [
  ["path", { d: "M12 15V3", key: "m9g1x1" }],
  ["path", { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4", key: "ih7n3h" }],
  ["path", { d: "m7 10 5 5 5-5", key: "brsn70" }]
], Ae = u("download", Mt);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const $t = [
  ["path", { d: "M15 3h6v6", key: "1q9fwt" }],
  ["path", { d: "M10 14 21 3", key: "gplh6r" }],
  ["path", { d: "M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6", key: "a6xqqp" }]
], Ie = u("external-link", $t);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Pt = [
  ["polyline", { points: "22 12 16 12 14 15 10 15 8 12 2 12", key: "o97t9d" }],
  [
    "path",
    {
      d: "M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z",
      key: "oot6mr"
    }
  ]
], Me = u("inbox", Pt);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const St = [
  ["circle", { cx: "12", cy: "16", r: "1", key: "1au0dj" }],
  ["rect", { x: "3", y: "10", width: "18", height: "12", rx: "2", key: "6s8ecr" }],
  ["path", { d: "M7 10V7a5 5 0 0 1 10 0v3", key: "1pqi11" }]
], Dt = u("lock-keyhole", St);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const jt = [
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
], $e = u("message-square-text", jt);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Tt = [
  ["path", { d: "M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8", key: "v9h5vc" }],
  ["path", { d: "M21 3v5h-5", key: "1q7to0" }],
  ["path", { d: "M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16", key: "3uifl3" }],
  ["path", { d: "M8 16H3v5", key: "1cv678" }]
], B = u("refresh-cw", Tt);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const qt = [
  [
    "path",
    {
      d: "M14.536 21.686a.5.5 0 0 0 .937-.024l6.5-19a.496.496 0 0 0-.635-.635l-19 6.5a.5.5 0 0 0-.024.937l7.93 3.18a2 2 0 0 1 1.112 1.11z",
      key: "1ffxy3"
    }
  ],
  ["path", { d: "m21.854 2.147-10.94 10.939", key: "12cjpa" }]
], ne = u("send", qt);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Ot = [
  ["path", { d: "M14 17H5", key: "gfn3mx" }],
  ["path", { d: "M19 7h-9", key: "6i9tg" }],
  ["circle", { cx: "17", cy: "17", r: "3", key: "18b49y" }],
  ["circle", { cx: "7", cy: "7", r: "3", key: "dfmy0x" }]
], Bt = u("settings-2", Ot);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Lt = [
  [
    "path",
    {
      d: "M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z",
      key: "oel41y"
    }
  ],
  ["path", { d: "m9 12 2 2 4-4", key: "dzmm74" }]
], re = u("shield-check", Lt);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Rt = [
  ["path", { d: "M10 11v6", key: "nco0om" }],
  ["path", { d: "M14 11v6", key: "outv1u" }],
  ["path", { d: "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6", key: "miytrc" }],
  ["path", { d: "M3 6h18", key: "d0wm0j" }],
  ["path", { d: "M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2", key: "e791ji" }]
], Pe = u("trash-2", Rt);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Ft = [
  ["path", { d: "m16 11 2 2 4-4", key: "9rsbq5" }],
  ["path", { d: "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2", key: "1yyitq" }],
  ["circle", { cx: "9", cy: "7", r: "4", key: "nufk8" }]
], le = u("user-check", Ft);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Ht = [
  ["path", { d: "M18 6 6 18", key: "1bl5f8" }],
  ["path", { d: "m6 6 12 12", key: "d8bk6v" }]
], Se = u("x", Ht), oe = "paw-me-dingtalk", R = window.QwenPaw.host, e = R.React, { useEffect: De, useMemo: Vt, useState: _ } = e, {
  Alert: D,
  Badge: Qt,
  Button: o,
  Card: b,
  Col: U,
  Descriptions: k,
  Drawer: Kt,
  Empty: W,
  Form: N,
  Input: je,
  InputNumber: Te,
  List: C,
  Modal: qe,
  Popconfirm: Oe,
  Progress: Be,
  Row: Ut,
  Select: z,
  Space: A,
  Spin: Le,
  Switch: Re,
  Table: Wt,
  Tabs: Gt,
  Tag: I,
  Timeline: Zt,
  Typography: Xt
} = R.antd, { Text: m, Title: ie } = Xt, Fe = `
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
`, Jt = {
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
function L(n) {
  return n ? new Date(n * 1e3).toLocaleString() : "—";
}
function se(n) {
  return n === "oauth:dws-event" ? "钉钉 OAuth 事件" : n || "无可信来源";
}
function He({ status: n }) {
  const c = n === "sent" ? "success" : n === "failed" || n === "blocked" ? "error" : n === "draft_ready" || n === "identity_required" || n === "needs_review" ? "warning" : "processing";
  return /* @__PURE__ */ e.createElement(I, { color: c }, Jt[n] || n);
}
function Ve() {
  var ve, fe, he, xe, be, we, _e;
  const n = Vt(() => {
    var a;
    return (a = window.QwenPaw.paw) == null ? void 0 : a.forApp(oe);
  }, []), [c, f] = _([]), [t, w] = _(null), [s, $] = _(
    (n == null ? void 0 : n.host.getSelectedAgentId()) || "default"
  ), [F, j] = _(!0), [E, p] = _(!1), [H, g] = _(""), [Je, G] = _(!1), [d, Z] = _(
    null
  ), [P, X] = _(null), [V, me] = _(""), [pe, de] = _(""), [J] = N.useForm(), [Y] = N.useForm(), i = n == null ? void 0 : n.api, h = async (a = s, r = !1) => {
    if (!i) {
      g("当前 QwenPaw 版本未提供 PawApp SDK"), j(!1);
      return;
    }
    r || j(!0);
    try {
      const y = await i.get("/snapshot", {
        query: { agent_id: a }
      });
      w(y), y.settings.agent_id && y.settings.agent_id !== s && $(y.settings.agent_id), g("");
    } catch (y) {
      g(y instanceof Error ? y.message : "状态加载失败");
    } finally {
      r || j(!1);
    }
  };
  De(() => {
    let a = !1;
    (async () => {
      try {
        const v = await (R.fetch ? await R.fetch("/agents") : await fetch(R.getApiUrl("/agents"))).json();
        a || f(
          (v.agents || []).filter(
            (S) => S.enabled && S.available_in_chat !== !1
          )
        );
      } catch {
        a || f([]);
      }
      a || await h(s);
    })();
    const y = window.setInterval(
      () => void h(s, !0),
      2e3
    );
    return () => {
      a = !0, window.clearInterval(y);
    };
  }, [s]), De(() => {
    de((t == null ? void 0 : t.owner_profile.approved.notes) || "");
  }, [t == null ? void 0 : t.owner_profile.revision]);
  const T = async (a) => {
    if (i) {
      p(!0);
      try {
        const r = await i.put("/settings", a, {
          query: { agent_id: String(a.agent_id) }
        });
        $(String(a.agent_id)), w(r), G(!1), await (n == null ? void 0 : n.host.toast("Paw Me 设置已保存", "success"));
      } catch (r) {
        g(r instanceof Error ? r.message : "设置保存失败");
      } finally {
        p(!1);
      }
    }
  }, Ye = async (a) => {
    t && await T({ ...t.settings, enabled: a, agent_id: s });
  }, ue = async (a, r) => {
    t && await T({
      ...t.settings,
      [a]: r,
      agent_id: s
    });
  }, et = async (a) => {
    $(a), t && await T({
      ...t.settings,
      agent_id: a
    });
  }, tt = () => {
    J.setFieldsValue({ ...t == null ? void 0 : t.settings, agent_id: s }), G(!0);
  }, ge = (a) => {
    Y.setFieldsValue({
      policy: (t == null ? void 0 : t.settings.default_policy) || "draft"
    }), Z(a);
  }, at = async (a) => {
    if (!(!i || !d)) {
      p(!0);
      try {
        await i.post(`/work-items/${d.id}/authorize`, a), Z(null), await h(s), await (n == null ? void 0 : n.host.toast("真实身份已授权", "success"));
      } catch (r) {
        g(r instanceof Error ? r.message : "身份授权失败");
      } finally {
        p(!1);
      }
    }
  }, q = async (a) => {
    if (i) {
      p(!0);
      try {
        await i.post(`/dws/${a}`), await h(s, !0);
      } catch (r) {
        g(r instanceof Error ? r.message : "钉钉连接失败");
      } finally {
        p(!1);
      }
    }
  }, nt = async () => {
    if (i) {
      p(!0);
      try {
        await i.post("/dws/cancel"), await h(s, !0);
      } catch (a) {
        g(a instanceof Error ? a.message : "取消操作失败");
      } finally {
        p(!1);
      }
    }
  }, rt = async () => {
    if (i) {
      p(!0);
      try {
        const a = await i.post("/identity/confirm");
        w(a), await (n == null ? void 0 : n.host.toast("本人钉钉账号已确认", "success"));
      } catch (a) {
        g(a instanceof Error ? a.message : "账号确认失败");
      } finally {
        p(!1);
      }
    }
  }, ye = async () => {
    if (i) {
      p(!0);
      try {
        const a = await i.post("/identity/reconnect");
        w(a);
      } catch (a) {
        g(a instanceof Error ? a.message : "重新连接失败");
      } finally {
        p(!1);
      }
    }
  }, lt = async () => {
    if (i)
      try {
        w(await i.post("/profile/refresh"));
      } catch (a) {
        g(a instanceof Error ? a.message : "画像更新失败");
      }
  }, it = async () => {
    i && w(await i.post("/profile/cancel"));
  }, st = async () => {
    if (i)
      try {
        w(
          await i.post("/profile/approve", {
            notes: pe
          })
        ), await (n == null ? void 0 : n.host.toast("本人画像已审核", "success"));
      } catch (a) {
        g(a instanceof Error ? a.message : "画像审核失败");
      }
  }, ct = async (a) => {
    i && (await i.delete(`/principals/${a}`), await h(s));
  }, ot = async (a, r) => {
    if (i)
      try {
        await i.patch(`/principals/${a}/policy`, { policy: r }), await h(s, !0);
      } catch (y) {
        g(y instanceof Error ? y.message : "策略更新失败");
      }
  }, mt = async (a) => {
    if (i) {
      p(!0);
      try {
        await i.post(`/outbox/${a}/send`), await h(s);
      } catch (r) {
        g(r instanceof Error ? r.message : "发送失败");
      } finally {
        p(!1);
      }
    }
  }, pt = async (a) => {
    i && (await i.delete(`/outbox/${a}`), await h(s));
  }, dt = async () => {
    if (!(!i || !P || !V.trim())) {
      p(!0);
      try {
        await i.patch(`/outbox/${P.id}`, {
          text: V.trim()
        }), X(null), await h(s), await (n == null ? void 0 : n.host.toast("草稿已保存", "success"));
      } catch (a) {
        g(a instanceof Error ? a.message : "草稿保存失败");
      } finally {
        p(!1);
      }
    }
  };
  if (F && !t)
    return /* @__PURE__ */ e.createElement("div", { className: "pm-page" }, /* @__PURE__ */ e.createElement(Le, null));
  const ee = (t == null ? void 0 : t.work_items.filter((a) => a.status === "identity_required")) || [], Ee = (t == null ? void 0 : t.outbox.filter((a) => a.status !== "sent")) || [], x = !!(t != null && t.identity_provider.authenticated), M = !!(t != null && t.identity_provider.confirmed), O = !!(t != null && t.identity_provider.available), te = (t == null ? void 0 : t.runtime.integration_stage) || "idle", Q = [
    "install",
    "downloading",
    "preparing",
    "installing",
    "verifying",
    "login"
  ].includes(te), l = t == null ? void 0 : t.owner_profile, K = (l == null ? void 0 : l.status) === "collecting", ae = !!(l != null && l.approved_at);
  if (!x || !M) {
    const a = O ? !x || !M ? 1 : 2 : 0, r = (v) => v < a ? /* @__PURE__ */ e.createElement(ze, { size: 17 }) : /* @__PURE__ */ e.createElement(zt, { size: 17 }), y = O ? x ? M ? "选择负责回复的 Agent" : "确认数字分身的本人账号" : "连接你的钉钉账号" : "准备钉钉连接组件", ke = (t == null ? void 0 : t.runtime.integration_detail) || (O ? x ? M ? "任意已启用 Agent 都可以负责回复，认证由 Agent 自己管理。" : "启用前核对组织与账号，避免数字分身以错误身份发言。" : "浏览器将打开钉钉官方 OAuth；插件不会读取或保存账号密码。" : "组件安装在 Paw Me 的独立目录，不修改系统 PATH。");
    return /* @__PURE__ */ e.createElement("div", { className: "pm-page" }, /* @__PURE__ */ e.createElement("style", null, Fe), /* @__PURE__ */ e.createElement("header", { className: "pm-header" }, /* @__PURE__ */ e.createElement("div", { className: "pm-header-copy" }, /* @__PURE__ */ e.createElement("div", { className: "pm-eyebrow" }, /* @__PURE__ */ e.createElement(re, { size: 15 }), "Paw Me · Digital Twin"), /* @__PURE__ */ e.createElement(ie, { level: 1 }, "钉钉数字人分身"), /* @__PURE__ */ e.createElement(m, { type: "secondary" }, "首次配置只需要安装连接组件、完成钉钉授权并选择 Agent。"))), H ? /* @__PURE__ */ e.createElement(
      D,
      {
        closable: !0,
        type: "error",
        message: "操作未完成",
        description: H,
        onClose: () => g(""),
        style: { marginBottom: 16 }
      }
    ) : null, /* @__PURE__ */ e.createElement(b, { className: "pm-onboarding" }, /* @__PURE__ */ e.createElement("div", { className: "pm-onboarding-head" }, /* @__PURE__ */ e.createElement(ie, { level: 2 }, "开始设置 Paw Me"), /* @__PURE__ */ e.createElement(m, { type: "secondary" }, "完成下面三个步骤后，消息监听、会话授权、草稿与发送会在同一页面运行。")), /* @__PURE__ */ e.createElement("div", { className: "pm-steps" }, ["安装连接组件", "钉钉 OAuth", "选择并启用 Agent"].map(
      (v, S) => /* @__PURE__ */ e.createElement(
        "div",
        {
          className: `pm-step ${S === a ? "pm-step-current" : ""} ${S < a ? "pm-step-done" : ""}`,
          key: v
        },
        /* @__PURE__ */ e.createElement("span", { className: "pm-step-icon" }, r(S)),
        /* @__PURE__ */ e.createElement("span", null, v)
      )
    )), /* @__PURE__ */ e.createElement("div", { className: "pm-onboarding-action" }, /* @__PURE__ */ e.createElement("h3", null, y), /* @__PURE__ */ e.createElement(m, { type: "secondary" }, ke), Q ? /* @__PURE__ */ e.createElement("div", { className: "pm-progress" }, /* @__PURE__ */ e.createElement(
      Be,
      {
        percent: (t == null ? void 0 : t.runtime.integration_progress) ?? 0,
        showInfo: (t == null ? void 0 : t.runtime.integration_progress) != null,
        status: "active"
      }
    ), (t == null ? void 0 : t.runtime.integration_progress) == null ? /* @__PURE__ */ e.createElement(A, { size: 8 }, /* @__PURE__ */ e.createElement(Le, { size: "small" }), /* @__PURE__ */ e.createElement(m, { type: "secondary" }, "正在执行当前阶段")) : null) : null, x && !M ? /* @__PURE__ */ e.createElement("div", { className: "pm-account" }, /* @__PURE__ */ e.createElement(k, { column: 1, size: "small" }, /* @__PURE__ */ e.createElement(k.Item, { label: "账号" }, (t == null ? void 0 : t.identity_provider.user_name) || "未返回显示名"), /* @__PURE__ */ e.createElement(k.Item, { label: "组织" }, (t == null ? void 0 : t.identity_provider.corp_name) || "未返回组织名"), /* @__PURE__ */ e.createElement(k.Item, { label: "真实 userId" }, /* @__PURE__ */ e.createElement("span", { className: "pm-id" }, (t == null ? void 0 : t.identity_provider.user_id) || "—")))) : null, M ? /* @__PURE__ */ e.createElement(
      z,
      {
        className: "pm-agent-select",
        value: s,
        options: c.map((v) => ({
          value: v.id,
          label: `${v.name || v.id} · ${v.backend || "agent"}`
        })),
        onChange: (v) => $(v)
      }
    ) : null, /* @__PURE__ */ e.createElement("div", { className: "pm-onboarding-buttons" }, O ? x ? M ? /* @__PURE__ */ e.createElement(
      o,
      {
        type: "primary",
        size: "large",
        icon: /* @__PURE__ */ e.createElement(ze, { size: 17 }),
        loading: E,
        disabled: !s,
        onClick: () => void T({
          enabled: !0,
          agent_id: s,
          default_policy: (t == null ? void 0 : t.settings.default_policy) || "draft",
          access_mode: (t == null ? void 0 : t.settings.access_mode) || "approval",
          quiet_seconds: (t == null ? void 0 : t.settings.quiet_seconds) ?? 4,
          max_wait_seconds: (t == null ? void 0 : t.settings.max_wait_seconds) ?? 20
        })
      },
      "启用数字人分身"
    ) : /* @__PURE__ */ e.createElement(e.Fragment, null, /* @__PURE__ */ e.createElement(
      o,
      {
        type: "primary",
        size: "large",
        icon: /* @__PURE__ */ e.createElement(le, { size: 17 }),
        loading: E,
        onClick: () => void rt()
      },
      "确认这是我"
    ), /* @__PURE__ */ e.createElement(
      o,
      {
        size: "large",
        icon: /* @__PURE__ */ e.createElement(B, { size: 17 }),
        disabled: E,
        onClick: () => void ye()
      },
      "不是我，重新连接"
    )) : /* @__PURE__ */ e.createElement(
      o,
      {
        type: "primary",
        size: "large",
        icon: /* @__PURE__ */ e.createElement(Ie, { size: 17 }),
        disabled: Q,
        onClick: () => void q("login")
      },
      "连接钉钉"
    ) : /* @__PURE__ */ e.createElement(
      o,
      {
        type: "primary",
        size: "large",
        icon: /* @__PURE__ */ e.createElement(Ae, { size: 17 }),
        disabled: Q,
        onClick: () => void q("install")
      },
      "安装并继续"
    ), Q ? /* @__PURE__ */ e.createElement(
      o,
      {
        size: "large",
        icon: /* @__PURE__ */ e.createElement(Se, { size: 17 }),
        loading: E,
        onClick: () => void nt()
      },
      "取消当前操作"
    ) : te === "failed" || te === "cancelled" ? /* @__PURE__ */ e.createElement(
      o,
      {
        size: "large",
        icon: /* @__PURE__ */ e.createElement(B, { size: 17 }),
        onClick: () => void q(O ? "login" : "install")
      },
      "重新尝试"
    ) : null))));
  }
  const ut = /* @__PURE__ */ e.createElement(
    b,
    {
      className: "pm-panel",
      title: "消息批次",
      extra: /* @__PURE__ */ e.createElement(m, { type: "secondary" }, "连续消息只回复一次")
    },
    /* @__PURE__ */ e.createElement(
      C,
      {
        dataSource: (t == null ? void 0 : t.work_items) || [],
        locale: { emptyText: /* @__PURE__ */ e.createElement(W, { description: "尚未捕获新消息" }) },
        renderItem: (a) => /* @__PURE__ */ e.createElement(
          C.Item,
          {
            actions: a.status === "identity_required" ? [
              /* @__PURE__ */ e.createElement(
                o,
                {
                  key: "authorize",
                  type: "primary",
                  onClick: () => ge(a)
                },
                "审核并授权"
              )
            ] : []
          },
          /* @__PURE__ */ e.createElement(
            C.Item.Meta,
            {
              title: /* @__PURE__ */ e.createElement("div", { className: "pm-item-title" }, /* @__PURE__ */ e.createElement("span", null, a.conversation_alias), /* @__PURE__ */ e.createElement(He, { status: a.status }), /* @__PURE__ */ e.createElement(I, null, a.message_count, " 条已合并")),
              description: /* @__PURE__ */ e.createElement(e.Fragment, null, /* @__PURE__ */ e.createElement("span", null, a.agent_id, " · ", L(a.updated_at)), /* @__PURE__ */ e.createElement("div", { className: "pm-id" }, a.subject_type === "person" ? "人员" : "群聊", " ·", " ", a.subject_id || "未获得真实 ID", " ·", " ", se(a.id_source)), a.error ? /* @__PURE__ */ e.createElement("div", { className: "pm-error" }, a.error) : null, /* @__PURE__ */ e.createElement("div", { className: "pm-message-stack" }, a.messages.map((r) => /* @__PURE__ */ e.createElement("div", { className: "pm-message", key: r.id }, r.text))))
            }
          )
        )
      }
    )
  ), gt = /* @__PURE__ */ e.createElement(b, { className: "pm-panel", title: "OAuth、身份与权限" }, /* @__PURE__ */ e.createElement("div", { className: "pm-setup" }, /* @__PURE__ */ e.createElement("div", { className: "pm-setup-copy" }, /* @__PURE__ */ e.createElement("div", { className: "pm-setup-title" }, x ? `${(t == null ? void 0 : t.identity_provider.user_name) || "钉钉账号"} 已连接` : t != null && t.identity_provider.available ? "连接组件已就绪，等待 OAuth 登录" : "安装钉钉连接组件"), /* @__PURE__ */ e.createElement(m, { type: "secondary" }, (t == null ? void 0 : t.runtime.integration_detail) || (t == null ? void 0 : t.identity_provider.detail) || "OAuth 由钉钉官方能力管理，插件不读取或保存令牌。"), x ? /* @__PURE__ */ e.createElement("div", { className: "pm-id" }, (t == null ? void 0 : t.identity_provider.corp_name) || "当前组织", " · userId", " ", (t == null ? void 0 : t.identity_provider.user_id) || "—") : null), t != null && t.identity_provider.available ? x ? /* @__PURE__ */ e.createElement(A, { wrap: !0 }, /* @__PURE__ */ e.createElement(
    o,
    {
      icon: /* @__PURE__ */ e.createElement(B, { size: 16 }),
      onClick: () => void h(s)
    },
    "刷新状态"
  ), /* @__PURE__ */ e.createElement(o, { onClick: () => void ye(), disabled: E }, "更换账号")) : /* @__PURE__ */ e.createElement(
    o,
    {
      type: "primary",
      icon: /* @__PURE__ */ e.createElement(Ie, { size: 16 }),
      loading: E || (t == null ? void 0 : t.runtime.integration_stage) === "login",
      onClick: () => void q("login")
    },
    "使用钉钉 OAuth 登录"
  ) : /* @__PURE__ */ e.createElement(
    o,
    {
      type: "primary",
      icon: /* @__PURE__ */ e.createElement(Ae, { size: 16 }),
      loading: E || (t == null ? void 0 : t.runtime.integration_stage) === "install",
      onClick: () => void q("install")
    },
    "安装连接组件"
  )), /* @__PURE__ */ e.createElement(
    D,
    {
      showIcon: !0,
      type: "info",
      message: "单会话规则只来自收到的真实事件",
      description: "人员 openDingTalkId 或群 openConversationId 由钉钉 OAuth 事件写入，界面不可手填。没有单会话规则时继承上方全局策略。",
      style: { marginBottom: 16 }
    }
  ), ee.length ? /* @__PURE__ */ e.createElement(
    C,
    {
      header: /* @__PURE__ */ e.createElement("strong", null, "待授权会话"),
      dataSource: ee,
      renderItem: (a) => /* @__PURE__ */ e.createElement(
        C.Item,
        {
          actions: [
            /* @__PURE__ */ e.createElement(
              o,
              {
                key: "authorize",
                type: "primary",
                onClick: () => ge(a)
              },
              "审核并授权"
            )
          ]
        },
        /* @__PURE__ */ e.createElement(
          C.Item.Meta,
          {
            title: a.display_name || a.conversation_alias,
            description: /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "pm-id" }, a.subject_id), /* @__PURE__ */ e.createElement(m, { type: "secondary" }, a.subject_type === "person" ? "人员" : "群聊", " ·", " ", a.id_source))
          }
        )
      )
    }
  ) : null, /* @__PURE__ */ e.createElement(
    Wt,
    {
      rowKey: "id",
      pagination: !1,
      dataSource: (t == null ? void 0 : t.principals) || [],
      locale: { emptyText: "暂无已验证身份" },
      columns: [
        {
          title: "身份",
          render: (a, r) => /* @__PURE__ */ e.createElement(e.Fragment, null, /* @__PURE__ */ e.createElement("div", null, r.display_name), /* @__PURE__ */ e.createElement(m, { type: "secondary" }, r.subject_type === "person" ? "人员" : "群聊"))
        },
        {
          title: "真实 ID",
          render: (a, r) => /* @__PURE__ */ e.createElement(e.Fragment, null, /* @__PURE__ */ e.createElement("div", null, r.subject_id), /* @__PURE__ */ e.createElement(m, { type: "secondary" }, se(r.id_source)))
        },
        { title: "会话", dataIndex: "conversation_alias" },
        {
          title: "策略",
          render: (a, r) => /* @__PURE__ */ e.createElement(
            z,
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
              onChange: (y) => void ot(r.id, y)
            }
          )
        },
        {
          title: "操作",
          render: (a, r) => /* @__PURE__ */ e.createElement(
            Oe,
            {
              title: "删除此会话规则？后续消息将继承全局策略。",
              onConfirm: () => void ct(r.id)
            },
            /* @__PURE__ */ e.createElement(o, { type: "text", danger: !0, icon: /* @__PURE__ */ e.createElement(Pe, { size: 15 }) }, "删除")
          )
        }
      ],
      scroll: { x: 760 }
    }
  )), yt = /* @__PURE__ */ e.createElement(
    b,
    {
      className: "pm-panel",
      title: "待发送",
      extra: /* @__PURE__ */ e.createElement(m, { type: "secondary" }, "按 OAuth 真实 ID 精确发送")
    },
    /* @__PURE__ */ e.createElement(
      C,
      {
        dataSource: Ee,
        locale: { emptyText: /* @__PURE__ */ e.createElement(W, { description: "暂无待发送回复" }) },
        renderItem: (a) => /* @__PURE__ */ e.createElement(
          C.Item,
          {
            actions: [
              /* @__PURE__ */ e.createElement(
                o,
                {
                  key: "edit",
                  icon: /* @__PURE__ */ e.createElement($e, { size: 15 }),
                  onClick: () => {
                    X(a), me(a.text);
                  }
                },
                "编辑"
              ),
              /* @__PURE__ */ e.createElement(
                o,
                {
                  key: "send",
                  type: "primary",
                  icon: /* @__PURE__ */ e.createElement(ne, { size: 15 }),
                  loading: E,
                  onClick: () => void mt(a.id)
                },
                "发送"
              ),
              /* @__PURE__ */ e.createElement(
                Oe,
                {
                  key: "delete",
                  title: "删除草稿？原始消息仍会保留。",
                  onConfirm: () => void pt(a.id)
                },
                /* @__PURE__ */ e.createElement(o, { danger: !0, type: "text", icon: /* @__PURE__ */ e.createElement(Pe, { size: 15 }) }, "删除")
              )
            ]
          },
          /* @__PURE__ */ e.createElement(
            C.Item.Meta,
            {
              title: /* @__PURE__ */ e.createElement("div", { className: "pm-item-title" }, /* @__PURE__ */ e.createElement("span", null, a.conversation_alias), /* @__PURE__ */ e.createElement(He, { status: a.status })),
              description: /* @__PURE__ */ e.createElement(e.Fragment, null, /* @__PURE__ */ e.createElement("div", { className: "pm-source" }, /* @__PURE__ */ e.createElement("div", { className: "pm-source-head" }, /* @__PURE__ */ e.createElement("strong", null, a.source_display_name || a.conversation_alias), /* @__PURE__ */ e.createElement(m, { type: "secondary" }, a.source_subject_type === "group" ? "群聊消息" : "单聊消息")), /* @__PURE__ */ e.createElement("div", { className: "pm-message-stack" }, (a.source_messages || []).map((r) => /* @__PURE__ */ e.createElement("div", { className: "pm-message", key: r.id }, r.text, /* @__PURE__ */ e.createElement("div", { className: "pm-meta" }, L(r.received_at)))))), /* @__PURE__ */ e.createElement("div", { className: "pm-draft" }, /* @__PURE__ */ e.createElement(m, { type: "secondary" }, "准备发送的回复"), /* @__PURE__ */ e.createElement("p", { className: "pm-pre" }, a.text)), a.error ? /* @__PURE__ */ e.createElement("div", { className: "pm-error" }, a.error) : null, /* @__PURE__ */ e.createElement("div", { className: "pm-meta" }, L(a.updated_at)))
            }
          )
        )
      }
    )
  ), Et = /* @__PURE__ */ e.createElement(b, { className: "pm-panel", title: "运行记录" }, /* @__PURE__ */ e.createElement(
    Zt,
    {
      items: ((t == null ? void 0 : t.activity) || []).map((a) => ({
        color: a.status === "failed" ? "red" : a.status === "sent" || a.status === "verified" ? "green" : "blue",
        children: /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "pm-item-title" }, /* @__PURE__ */ e.createElement("strong", null, a.title), /* @__PURE__ */ e.createElement(I, null, a.status)), a.detail ? /* @__PURE__ */ e.createElement("div", { className: "pm-subtle" }, a.detail) : null, /* @__PURE__ */ e.createElement("div", { className: "pm-meta" }, L(a.created_at)))
      }))
    }
  ));
  return /* @__PURE__ */ e.createElement("div", { className: "pm-page" }, /* @__PURE__ */ e.createElement("style", null, Fe), /* @__PURE__ */ e.createElement("header", { className: "pm-header" }, /* @__PURE__ */ e.createElement("div", { className: "pm-header-copy" }, /* @__PURE__ */ e.createElement("div", { className: "pm-eyebrow" }, /* @__PURE__ */ e.createElement(re, { size: 15 }), "Paw Me · Digital Twin"), /* @__PURE__ */ e.createElement(ie, { level: 1 }, "钉钉数字人分身"), /* @__PURE__ */ e.createElement(m, { type: "secondary" }, "使用所选 Agent 和本机钉钉 OAuth 登录态，在一个页面完成实时收件、 独立授权、上下文聚合、处理、草稿、发送与审计。")), /* @__PURE__ */ e.createElement("div", { className: "pm-actions" }, /* @__PURE__ */ e.createElement(
    z,
    {
      value: s,
      style: { minWidth: 190 },
      options: c.map((a) => ({
        value: a.id,
        label: `${a.name || a.id} · ${a.backend || "agent"}`
      })),
      onChange: (a) => void et(a)
    }
  ), /* @__PURE__ */ e.createElement(o, { icon: /* @__PURE__ */ e.createElement(Bt, { size: 16 }), onClick: tt }, "设置"), /* @__PURE__ */ e.createElement(
    o,
    {
      icon: /* @__PURE__ */ e.createElement(B, { size: 16 }),
      onClick: () => void h(s)
    },
    "刷新"
  ), /* @__PURE__ */ e.createElement(A, null, /* @__PURE__ */ e.createElement(
    Re,
    {
      checked: t == null ? void 0 : t.settings.enabled,
      disabled: !x || !ae,
      onChange: (a) => void Ye(a)
    }
  ), /* @__PURE__ */ e.createElement(m, null, t != null && t.settings.enabled ? "运行中" : "已停止")))), H ? /* @__PURE__ */ e.createElement(
    D,
    {
      closable: !0,
      type: "error",
      message: "操作未完成",
      description: H,
      onClose: () => g(""),
      style: { marginBottom: 16 }
    }
  ) : null, /* @__PURE__ */ e.createElement(b, { className: "pm-statusbar" }, /* @__PURE__ */ e.createElement("div", { className: "pm-status-inner" }, /* @__PURE__ */ e.createElement("div", { className: "pm-status-main" }, t != null && t.runtime.running ? /* @__PURE__ */ e.createElement(Qt, { status: "processing" }) : /* @__PURE__ */ e.createElement(Nt, { size: 18 }), /* @__PURE__ */ e.createElement("div", { className: "pm-status-text" }, /* @__PURE__ */ e.createElement("div", { className: "pm-status-title" }, (t == null ? void 0 : t.runtime.stage) || "stopped"), /* @__PURE__ */ e.createElement(m, { className: "pm-status-detail", type: "secondary" }, (t == null ? void 0 : t.runtime.detail) || "等待启动"))), /* @__PURE__ */ e.createElement(A, { wrap: !0 }, /* @__PURE__ */ e.createElement(
    I,
    {
      icon: /* @__PURE__ */ e.createElement(re, { size: 13 }),
      color: x ? "success" : "warning"
    },
    x ? "钉钉 OAuth 已连接" : "等待钉钉 OAuth"
  ), /* @__PURE__ */ e.createElement(I, { icon: /* @__PURE__ */ e.createElement(It, { size: 13 }) }, "静默 ", (t == null ? void 0 : t.settings.quiet_seconds) ?? 4, " 秒"), t != null && t.runtime.current_conversation ? /* @__PURE__ */ e.createElement(I, { icon: /* @__PURE__ */ e.createElement($e, { size: 13 }) }, t.runtime.current_conversation) : null))), /* @__PURE__ */ e.createElement(
    b,
    {
      className: "pm-panel",
      title: "本人画像与人物关系",
      extra: /* @__PURE__ */ e.createElement(I, { color: ae ? "success" : "warning" }, ae ? "已审核" : "启用前需审核")
    },
    /* @__PURE__ */ e.createElement(
      D,
      {
        type: (l == null ? void 0 : l.status) === "failed" ? "error" : "info",
        showIcon: !0,
        message: (t == null ? void 0 : t.runtime.profile_detail) || "等待初始化",
        description: "首次初始化和后台定期更新才访问钉钉；日常回复只读取本地快照。不会保存他人的私聊正文，也不会推断私人关系。"
      }
    ),
    K ? /* @__PURE__ */ e.createElement("div", { className: "pm-profile-progress" }, /* @__PURE__ */ e.createElement(
      Be,
      {
        percent: (t == null ? void 0 : t.runtime.profile_progress) ?? 0,
        status: "active"
      }
    )) : null,
    /* @__PURE__ */ e.createElement("div", { className: "pm-profile-grid", style: { marginTop: 16 } }, /* @__PURE__ */ e.createElement("div", { className: "pm-profile-facts" }, /* @__PURE__ */ e.createElement(k, { column: 1, size: "small", bordered: !0 }, /* @__PURE__ */ e.createElement(k.Item, { label: "本人" }, ((ve = l == null ? void 0 : l.collected.identity) == null ? void 0 : ve.name) || "待采集"), /* @__PURE__ */ e.createElement(k.Item, { label: "部门" }, ((he = (fe = l == null ? void 0 : l.collected.identity) == null ? void 0 : fe.departments) == null ? void 0 : he.join("、")) || "—"), /* @__PURE__ */ e.createElement(k.Item, { label: "职位 / 角色" }, [
      (xe = l == null ? void 0 : l.collected.identity) == null ? void 0 : xe.title,
      ...((be = l == null ? void 0 : l.collected.identity) == null ? void 0 : be.roles) || []
    ].filter(Boolean).join(" · ") || "—"), /* @__PURE__ */ e.createElement(k.Item, { label: "表达样本" }, ((we = l == null ? void 0 : l.collected.work_style) == null ? void 0 : we.message_count) || 0, " 条本人消息"), /* @__PURE__ */ e.createElement(k.Item, { label: "最近更新" }, L(l == null ? void 0 : l.refreshed_at))), l != null && l.error ? /* @__PURE__ */ e.createElement(m, { type: "warning" }, "部分数据未完成：", l.error) : null), /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement(m, { strong: !0 }, "近期协作关系"), ((l == null ? void 0 : l.collected.relationships) || []).slice(0, 6).map((a) => /* @__PURE__ */ e.createElement("div", { className: "pm-profile-relation", key: a.subject_id }, /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", null, a.name), /* @__PURE__ */ e.createElement(m, { type: "secondary" }, "互动 ", a.interaction_count, " 次 · 共同群", " ", a.shared_group_count, " 个")), /* @__PURE__ */ e.createElement(I, null, "有来源"))), (_e = l == null ? void 0 : l.collected.relationships) != null && _e.length ? null : /* @__PURE__ */ e.createElement(
      W,
      {
        image: W.PRESENTED_IMAGE_SIMPLE,
        description: "暂无关系数据"
      }
    ), /* @__PURE__ */ e.createElement(
      je.TextArea,
      {
        className: "pm-profile-note",
        rows: 3,
        value: pe,
        placeholder: "可补充：我的职责、做事方式、称呼习惯，以及明确的人物关系。",
        onChange: (a) => de(a.target.value)
      }
    ))),
    /* @__PURE__ */ e.createElement("div", { className: "pm-profile-actions" }, /* @__PURE__ */ e.createElement(
      o,
      {
        type: "primary",
        icon: /* @__PURE__ */ e.createElement(le, { size: 16 }),
        disabled: K || !["ready", "partial", "stale"].includes((l == null ? void 0 : l.status) || ""),
        onClick: () => void st()
      },
      "审核并保存画像"
    ), /* @__PURE__ */ e.createElement(
      o,
      {
        icon: /* @__PURE__ */ e.createElement(B, { size: 16 }),
        disabled: K,
        onClick: () => void lt()
      },
      "立即更新"
    ), K ? /* @__PURE__ */ e.createElement(o, { icon: /* @__PURE__ */ e.createElement(Se, { size: 16 }), onClick: () => void it() }, "取消更新") : null)
  ), /* @__PURE__ */ e.createElement(b, { className: "pm-global", title: "全局访问与回复策略" }, /* @__PURE__ */ e.createElement("div", { className: "pm-global-grid" }, /* @__PURE__ */ e.createElement("div", { className: "pm-global-field" }, /* @__PURE__ */ e.createElement("div", { className: "pm-global-label" }, "新会话默认访问规则"), /* @__PURE__ */ e.createElement(
    z,
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
      onChange: (a) => void ue("access_mode", a)
    }
  ), /* @__PURE__ */ e.createElement(m, { type: "secondary" }, "单会话规则始终优先；删除单会话规则后恢复继承全局。")), /* @__PURE__ */ e.createElement("div", { className: "pm-global-field" }, /* @__PURE__ */ e.createElement("div", { className: "pm-global-label" }, "允许回复时的默认方式"), /* @__PURE__ */ e.createElement(
    z,
    {
      value: (t == null ? void 0 : t.settings.default_policy) || "draft",
      options: [
        { value: "draft", label: "先进入待发送" },
        { value: "automatic", label: "生成后自动发送" }
      ],
      onChange: (a) => void ue("default_policy", a)
    }
  ), /* @__PURE__ */ e.createElement(m, { type: "secondary" }, "即使选择自动发送，身份泄漏或元分析也会强制留在草稿。")))), /* @__PURE__ */ e.createElement(Ut, { gutter: [14, 14] }, /* @__PURE__ */ e.createElement(U, { xs: 12, lg: 6 }, /* @__PURE__ */ e.createElement(b, { className: "pm-metric" }, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-icon" }, /* @__PURE__ */ e.createElement(Me, { size: 18 })), /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-value" }, (t == null ? void 0 : t.work_items.length) || 0), /* @__PURE__ */ e.createElement("div", { className: "pm-metric-label" }, "消息批次")))), /* @__PURE__ */ e.createElement(U, { xs: 12, lg: 6 }, /* @__PURE__ */ e.createElement(b, { className: "pm-metric" }, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-icon" }, /* @__PURE__ */ e.createElement(Dt, { size: 18 })), /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-value" }, ee.length), /* @__PURE__ */ e.createElement("div", { className: "pm-metric-label" }, "待绑定身份")))), /* @__PURE__ */ e.createElement(U, { xs: 12, lg: 6 }, /* @__PURE__ */ e.createElement(b, { className: "pm-metric" }, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-icon" }, /* @__PURE__ */ e.createElement(ne, { size: 18 })), /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-value" }, Ee.length), /* @__PURE__ */ e.createElement("div", { className: "pm-metric-label" }, "待发送")))), /* @__PURE__ */ e.createElement(U, { xs: 12, lg: 6 }, /* @__PURE__ */ e.createElement(b, { className: "pm-metric" }, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-icon" }, /* @__PURE__ */ e.createElement(Ce, { size: 18 })), /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-value" }, (t == null ? void 0 : t.principals.length) || 0), /* @__PURE__ */ e.createElement("div", { className: "pm-metric-label" }, "已验证身份"))))), /* @__PURE__ */ e.createElement(
    Gt,
    {
      defaultActiveKey: "inbox",
      items: [
        {
          key: "inbox",
          label: /* @__PURE__ */ e.createElement(A, null, /* @__PURE__ */ e.createElement(Me, { size: 15 }), "收件与处理"),
          children: ut
        },
        {
          key: "permissions",
          label: /* @__PURE__ */ e.createElement(A, null, /* @__PURE__ */ e.createElement(le, { size: 15 }), "身份与权限"),
          children: gt
        },
        {
          key: "outbox",
          label: /* @__PURE__ */ e.createElement(A, null, /* @__PURE__ */ e.createElement(ne, { size: 15 }), "待发送"),
          children: yt
        },
        {
          key: "activity",
          label: /* @__PURE__ */ e.createElement(A, null, /* @__PURE__ */ e.createElement(Ce, { size: 15 }), "运行记录"),
          children: Et
        }
      ]
    }
  ), /* @__PURE__ */ e.createElement(
    Kt,
    {
      title: "运行设置",
      width: 420,
      open: Je,
      onClose: () => G(!1),
      destroyOnClose: !0,
      extra: /* @__PURE__ */ e.createElement(
        o,
        {
          type: "primary",
          loading: E,
          onClick: () => J.submit()
        },
        "保存"
      )
    },
    /* @__PURE__ */ e.createElement(
      N,
      {
        form: J,
        layout: "vertical",
        onFinish: T,
        initialValues: t == null ? void 0 : t.settings
      },
      /* @__PURE__ */ e.createElement(
        N.Item,
        {
          name: "agent_id",
          label: "回复消息的 Agent",
          rules: [{ required: !0 }]
        },
        /* @__PURE__ */ e.createElement(
          z,
          {
            options: c.map((a) => ({
              value: a.id,
              label: `${a.name || a.id} · ${a.backend || "agent"}`
            }))
          }
        )
      ),
      /* @__PURE__ */ e.createElement(
        N.Item,
        {
          name: "enabled",
          label: "数字人分身总开关",
          valuePropName: "checked"
        },
        /* @__PURE__ */ e.createElement(Re, null)
      ),
      /* @__PURE__ */ e.createElement(N.Item, { name: "default_policy", label: "默认回复策略" }, /* @__PURE__ */ e.createElement(
        z,
        {
          options: [
            { value: "draft", label: "生成草稿，确认后发送" },
            { value: "automatic", label: "按身份策略自动发送" }
          ]
        }
      )),
      /* @__PURE__ */ e.createElement(N.Item, { name: "access_mode", label: "新会话默认访问规则" }, /* @__PURE__ */ e.createElement(
        z,
        {
          options: [
            { value: "approval", label: "逐个审批" },
            { value: "allow_all", label: "全白名单" },
            { value: "block_all", label: "全黑名单" }
          ]
        }
      )),
      /* @__PURE__ */ e.createElement(
        N.Item,
        {
          name: "quiet_seconds",
          label: "连续消息静默窗口（秒）",
          extra: "对方停止输入达到这个时间后，才合并调用一次 Agent。"
        },
        /* @__PURE__ */ e.createElement(Te, { min: 1, max: 30, style: { width: "100%" } })
      ),
      /* @__PURE__ */ e.createElement(
        N.Item,
        {
          name: "max_wait_seconds",
          label: "最长聚合等待（秒）",
          extra: "持续聊天时也不会无限等待。"
        },
        /* @__PURE__ */ e.createElement(Te, { min: 3, max: 120, style: { width: "100%" } })
      ),
      /* @__PURE__ */ e.createElement(
        D,
        {
          type: "info",
          showIcon: !0,
          message: "上下文不会因中断丢失",
          description: "每条原始消息先写入 SQLite。Agent 运行中新消息到达时，旧任务会停止，新任务在同一会话中携带完整批次继续。"
        }
      )
    )
  ), /* @__PURE__ */ e.createElement(
    qe,
    {
      title: "授权真实钉钉会话",
      open: !!d,
      confirmLoading: E,
      onCancel: () => Z(null),
      onOk: () => Y.submit(),
      destroyOnClose: !0
    },
    /* @__PURE__ */ e.createElement(
      D,
      {
        type: "info",
        showIcon: !0,
        message: "ID 已由钉钉 OAuth 事件验证",
        description: "下列 ID 为只读值，不能手填或修改。授权后，相同真实 ID 的后续消息会按所选策略处理。",
        style: { marginBottom: 16 }
      }
    ),
    /* @__PURE__ */ e.createElement(
      k,
      {
        size: "small",
        column: 1,
        bordered: !0,
        style: { marginBottom: 18 },
        items: [
          {
            key: "name",
            label: "会话",
            children: (d == null ? void 0 : d.display_name) || (d == null ? void 0 : d.conversation_alias) || "—"
          },
          {
            key: "type",
            label: "类型",
            children: (d == null ? void 0 : d.subject_type) === "group" ? "群聊" : "人员"
          },
          {
            key: "id",
            label: "真实 ID",
            children: /* @__PURE__ */ e.createElement("span", { className: "pm-id" }, (d == null ? void 0 : d.subject_id) || "—")
          },
          {
            key: "source",
            label: "来源",
            children: se(d == null ? void 0 : d.id_source)
          }
        ]
      }
    ),
    /* @__PURE__ */ e.createElement(
      N,
      {
        form: Y,
        layout: "vertical",
        onFinish: at
      },
      /* @__PURE__ */ e.createElement(
        N.Item,
        {
          name: "policy",
          label: "权限策略",
          rules: [{ required: !0 }]
        },
        /* @__PURE__ */ e.createElement(
          z,
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
    qe,
    {
      title: `编辑发给 ${(P == null ? void 0 : P.conversation_alias) || ""} 的草稿`,
      open: !!P,
      confirmLoading: E,
      okButtonProps: { disabled: !V.trim() },
      onCancel: () => X(null),
      onOk: () => void dt(),
      destroyOnClose: !0
    },
    /* @__PURE__ */ e.createElement(
      je.TextArea,
      {
        autoSize: { minRows: 6, maxRows: 16 },
        value: V,
        onChange: (a) => me(a.target.value)
      }
    )
  ));
}
var Ke;
const Qe = (Ke = window.QwenPaw.paw) == null ? void 0 : Ke.forApp(oe);
var Ue, We;
Qe ? Qe.ui.registerPage({
  path: "/apps/paw-me-dingtalk",
  label: "Paw Me · DingTalk",
  component: Ve
}) : (We = (Ue = window.QwenPaw).registerRoutes) == null || We.call(Ue, oe, [
  {
    path: "/apps/paw-me-dingtalk",
    component: Ve,
    label: "Paw Me · DingTalk"
  }
]);
