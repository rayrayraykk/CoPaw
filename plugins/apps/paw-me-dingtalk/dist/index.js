const Ae = window.QwenPaw.host.React, te = Ae.createElement, Me = Ae.forwardRef;
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Ke = (a) => a.replace(/([a-z0-9])([A-Z])/g, "$1-$2").toLowerCase(), Ue = (a) => a.replace(
  /^([A-Z])|[\s-_]+(\w)/g,
  (i, y, t) => t ? t.toUpperCase() : y.toLowerCase()
), se = (a) => {
  const i = Ue(a);
  return i.charAt(0).toUpperCase() + i.slice(1);
}, $e = (...a) => a.filter((i, y, t) => !!i && i.trim() !== "" && t.indexOf(i) === y).join(" ").trim(), We = (a) => {
  for (const i in a)
    if (i.startsWith("aria-") || i === "role" || i === "title")
      return !0;
};
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
var Ze = {
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
const Xe = Me(
  ({
    color: a = "currentColor",
    size: i = 24,
    strokeWidth: y = 2,
    absoluteStrokeWidth: t,
    className: N = "",
    children: l,
    iconNode: C,
    ...T
  }, I) => te(
    "svg",
    {
      ref: I,
      ...Ze,
      width: i,
      height: i,
      stroke: a,
      strokeWidth: t ? Number(y) * 24 / Number(i) : y,
      className: $e("lucide", N),
      ...!l && !We(T) && { "aria-hidden": "true" },
      ...T
    },
    [
      ...C.map(([f, d]) => te(f, d)),
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
const m = (a, i) => {
  const y = Me(
    ({ className: t, ...N }, l) => te(Xe, {
      ref: l,
      iconNode: i,
      className: $e(
        `lucide-${Ke(se(a))}`,
        `lucide-${a}`,
        t
      ),
      ...N
    })
  );
  return y.displayName = se(a), y;
};
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Ge = [
  [
    "path",
    {
      d: "M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2",
      key: "169zse"
    }
  ]
], ce = m("activity", Ge);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Je = [["path", { d: "M20 6 9 17l-5-5", key: "1gmf2c" }]], oe = m("check", Je);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Ye = [
  ["circle", { cx: "12", cy: "12", r: "10", key: "1mglay" }],
  ["line", { x1: "10", x2: "10", y1: "15", y2: "9", key: "c1nkhi" }],
  ["line", { x1: "14", x2: "14", y1: "15", y2: "9", key: "h65svq" }]
], et = m("circle-pause", Ye);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const tt = [["circle", { cx: "12", cy: "12", r: "10", key: "1mglay" }]], nt = m("circle", tt);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const at = [
  ["path", { d: "M12 6v6h4", key: "135r8i" }],
  ["circle", { cx: "12", cy: "12", r: "10", key: "1mglay" }]
], rt = m("clock-3", at);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const lt = [
  ["path", { d: "M12 15V3", key: "m9g1x1" }],
  ["path", { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4", key: "ih7n3h" }],
  ["path", { d: "m7 10 5 5 5-5", key: "brsn70" }]
], me = m("download", lt);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const it = [
  ["path", { d: "M15 3h6v6", key: "1q9fwt" }],
  ["path", { d: "M10 14 21 3", key: "gplh6r" }],
  ["path", { d: "M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6", key: "a6xqqp" }]
], pe = m("external-link", it);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const st = [
  ["polyline", { points: "22 12 16 12 14 15 10 15 8 12 2 12", key: "o97t9d" }],
  [
    "path",
    {
      d: "M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z",
      key: "oot6mr"
    }
  ]
], de = m("inbox", st);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const ct = [
  ["circle", { cx: "12", cy: "16", r: "1", key: "1au0dj" }],
  ["rect", { x: "3", y: "10", width: "18", height: "12", rx: "2", key: "6s8ecr" }],
  ["path", { d: "M7 10V7a5 5 0 0 1 10 0v3", key: "1pqi11" }]
], ot = m("lock-keyhole", ct);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const mt = [
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
], ue = m("message-square-text", mt);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const pt = [
  ["path", { d: "M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8", key: "v9h5vc" }],
  ["path", { d: "M21 3v5h-5", key: "1q7to0" }],
  ["path", { d: "M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16", key: "3uifl3" }],
  ["path", { d: "M8 16H3v5", key: "1cv678" }]
], Z = m("refresh-cw", pt);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const dt = [
  [
    "path",
    {
      d: "M14.536 21.686a.5.5 0 0 0 .937-.024l6.5-19a.496.496 0 0 0-.635-.635l-19 6.5a.5.5 0 0 0-.024.937l7.93 3.18a2 2 0 0 1 1.112 1.11z",
      key: "1ffxy3"
    }
  ],
  ["path", { d: "m21.854 2.147-10.94 10.939", key: "12cjpa" }]
], X = m("send", dt);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const ut = [
  ["path", { d: "M14 17H5", key: "gfn3mx" }],
  ["path", { d: "M19 7h-9", key: "6i9tg" }],
  ["circle", { cx: "17", cy: "17", r: "3", key: "18b49y" }],
  ["circle", { cx: "7", cy: "7", r: "3", key: "dfmy0x" }]
], gt = m("settings-2", ut);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const yt = [
  [
    "path",
    {
      d: "M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z",
      key: "oel41y"
    }
  ],
  ["path", { d: "m9 12 2 2 4-4", key: "dzmm74" }]
], G = m("shield-check", yt);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Et = [
  ["path", { d: "M10 11v6", key: "nco0om" }],
  ["path", { d: "M14 11v6", key: "outv1u" }],
  ["path", { d: "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6", key: "miytrc" }],
  ["path", { d: "M3 6h18", key: "d0wm0j" }],
  ["path", { d: "M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2", key: "e791ji" }]
], ge = m("trash-2", Et);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const ht = [
  ["path", { d: "m16 11 2 2 4-4", key: "9rsbq5" }],
  ["path", { d: "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2", key: "1yyitq" }],
  ["circle", { cx: "9", cy: "7", r: "4", key: "nufk8" }]
], vt = m("user-check", ht);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const ft = [
  ["path", { d: "M18 6 6 18", key: "1bl5f8" }],
  ["path", { d: "m6 6 12 12", key: "d8bk6v" }]
], xt = m("x", ft), ne = "paw-me-dingtalk", q = window.QwenPaw.host, e = q.React, { useEffect: wt, useMemo: bt, useState: x } = e, {
  Alert: D,
  Badge: _t,
  Button: o,
  Card: w,
  Col: B,
  Descriptions: kt,
  Drawer: Nt,
  Empty: ye,
  Form: b,
  Input: Ct,
  InputNumber: Ee,
  List: _,
  Modal: he,
  Popconfirm: ve,
  Progress: zt,
  Row: At,
  Select: M,
  Space: k,
  Spin: fe,
  Switch: xe,
  Table: Mt,
  Tabs: $t,
  Tag: $,
  Timeline: It,
  Typography: Pt
} = q.antd, { Text: g, Title: J } = Pt, we = `
.pm-page{max-width:1440px;margin:0 auto;padding:24px 28px 48px}
.pm-header{display:flex;align-items:flex-start;justify-content:space-between;gap:24px;margin-bottom:22px}
.pm-eyebrow{display:flex;align-items:center;gap:8px;margin-bottom:8px;color:var(--ant-color-text-secondary);font-size:12px;font-weight:600;letter-spacing:.08em;text-transform:uppercase}
.pm-header h1{margin:0 0 6px!important;font-size:30px!important;letter-spacing:-.035em}.pm-header-copy{max-width:720px}
.pm-actions{display:flex;align-items:center;justify-content:flex-end;gap:10px;flex-wrap:wrap}
.pm-statusbar{margin-bottom:18px}.pm-status-inner{display:flex;align-items:center;justify-content:space-between;gap:18px}.pm-status-main{display:flex;align-items:center;gap:12px;min-width:0}.pm-status-text{min-width:0}.pm-status-title{font-weight:600}.pm-status-detail{display:block;max-width:720px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.pm-metric{height:100%}.pm-metric .ant-card-body{display:flex;align-items:center;gap:14px;padding:18px}.pm-metric-icon{display:grid;place-items:center;width:38px;height:38px;border-radius:10px;background:var(--ant-color-fill-secondary);color:var(--ant-color-primary);flex:none}.pm-metric-value{font-size:20px;font-weight:650;line-height:1.2}.pm-metric-label{color:var(--ant-color-text-secondary);font-size:12px;margin-top:3px}
.pm-panel{margin-top:16px}.pm-panel .ant-card-head{min-height:52px}.pm-item-title{display:flex;align-items:center;gap:8px;flex-wrap:wrap}.pm-message-stack{display:grid;gap:8px;margin-top:12px}.pm-message{padding:9px 11px;border-radius:8px;background:var(--ant-color-fill-tertiary);white-space:pre-wrap}.pm-meta{font-size:12px;color:var(--ant-color-text-secondary)}
.pm-policy-grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:12px}.pm-subtle{color:var(--ant-color-text-secondary)}.pm-pre{white-space:pre-wrap;line-height:1.65;margin:0}.pm-error{color:var(--ant-color-error)}.pm-id{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;overflow-wrap:anywhere;font-size:12px}.pm-setup{display:flex;align-items:center;justify-content:space-between;gap:18px;padding:16px;border:1px solid var(--ant-color-border-secondary);border-radius:10px;margin-bottom:16px}.pm-setup-copy{min-width:0}.pm-setup-title{font-weight:650;margin-bottom:4px}
.pm-onboarding{max-width:880px;margin:42px auto 0}.pm-onboarding .ant-card-body{padding:32px}.pm-onboarding-head{max-width:650px;margin-bottom:30px}.pm-onboarding-head h2{margin:0 0 8px!important;font-size:26px!important;letter-spacing:-.025em}.pm-steps{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:10px;margin-bottom:24px}.pm-step{display:flex;align-items:center;gap:10px;padding:12px;border:1px solid var(--ant-color-border-secondary);border-radius:10px;color:var(--ant-color-text-secondary)}.pm-step-current{border-color:var(--ant-color-primary);color:var(--ant-color-text);background:var(--ant-color-primary-bg)}.pm-step-done{color:var(--ant-color-success)}.pm-step-icon{display:grid;place-items:center;flex:none}.pm-onboarding-action{padding:22px;border-radius:12px;background:var(--ant-color-fill-quaternary)}.pm-onboarding-action h3{margin:0 0 6px;font-size:18px}.pm-progress{margin:18px 0 6px}.pm-onboarding-buttons{display:flex;align-items:center;gap:10px;flex-wrap:wrap;margin-top:20px}.pm-agent-select{width:100%;max-width:420px;margin-top:16px}
@media(max-width:760px){.pm-page{padding:16px 12px 32px}.pm-header{flex-direction:column}.pm-actions{justify-content:flex-start}.pm-status-inner{align-items:flex-start;flex-direction:column}.pm-status-detail{white-space:normal}.pm-policy-grid{grid-template-columns:1fr}.pm-onboarding{margin-top:18px}.pm-onboarding .ant-card-body{padding:20px}.pm-steps{grid-template-columns:1fr}}
`, St = {
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
  failed: "失败"
};
function Y(a) {
  return a ? new Date(a * 1e3).toLocaleString() : "—";
}
function ee(a) {
  return a === "oauth:dws-event" ? "钉钉 OAuth 事件" : a || "无可信来源";
}
function be({ status: a }) {
  const i = a === "sent" ? "success" : a === "failed" || a === "blocked" ? "error" : a === "draft_ready" || a === "identity_required" ? "warning" : "processing";
  return /* @__PURE__ */ e.createElement($, { color: i }, St[a] || a);
}
function _e() {
  const a = bt(() => {
    var n;
    return (n = window.QwenPaw.paw) == null ? void 0 : n.forApp(ne);
  }, []), [i, y] = x([]), [t, N] = x(null), [l, C] = x(
    (a == null ? void 0 : a.host.getSelectedAgentId()) || "default"
  ), [T, I] = x(!0), [f, d] = x(!1), [O, E] = x(""), [Ie, F] = x(!1), [c, H] = x(
    null
  ), [z, V] = x(null), [j, ae] = x(""), [Q] = b.useForm(), [K] = b.useForm(), s = a == null ? void 0 : a.api, h = async (n = l, r = !1) => {
    if (!s) {
      E("当前 QwenPaw 版本未提供 PawApp SDK"), I(!1);
      return;
    }
    r || I(!0);
    try {
      const p = await s.get("/snapshot", {
        query: { agent_id: n }
      });
      N(p), p.settings.agent_id && p.settings.agent_id !== l && C(p.settings.agent_id), E("");
    } catch (p) {
      E(p instanceof Error ? p.message : "状态加载失败");
    } finally {
      r || I(!1);
    }
  };
  wt(() => {
    let n = !1;
    (async () => {
      try {
        const u = await (q.fetch ? await q.fetch("/agents") : await fetch(q.getApiUrl("/agents"))).json();
        n || y(
          (u.agents || []).filter(
            (A) => A.enabled && A.available_in_chat !== !1
          )
        );
      } catch {
        n || y([]);
      }
      n || await h(l);
    })();
    const p = window.setInterval(
      () => void h(l, !0),
      2e3
    );
    return () => {
      n = !0, window.clearInterval(p);
    };
  }, [l]);
  const L = async (n) => {
    if (s) {
      d(!0);
      try {
        const r = await s.put("/settings", n, {
          query: { agent_id: String(n.agent_id) }
        });
        C(String(n.agent_id)), N(r), F(!1), await (a == null ? void 0 : a.host.toast("Paw Me 设置已保存", "success"));
      } catch (r) {
        E(r instanceof Error ? r.message : "设置保存失败");
      } finally {
        d(!1);
      }
    }
  }, Pe = async (n) => {
    t && await L({ ...t.settings, enabled: n, agent_id: l });
  }, Se = async (n) => {
    C(n), t && await L({
      ...t.settings,
      agent_id: n
    });
  }, De = () => {
    Q.setFieldsValue({ ...t == null ? void 0 : t.settings, agent_id: l }), F(!0);
  }, re = (n) => {
    K.setFieldsValue({
      policy: (t == null ? void 0 : t.settings.default_policy) || "draft"
    }), H(n);
  }, qe = async (n) => {
    if (!(!s || !c)) {
      d(!0);
      try {
        await s.post(`/work-items/${c.id}/authorize`, n), H(null), await h(l), await (a == null ? void 0 : a.host.toast("真实身份已授权", "success"));
      } catch (r) {
        E(r instanceof Error ? r.message : "身份授权失败");
      } finally {
        d(!1);
      }
    }
  }, P = async (n) => {
    if (s) {
      d(!0);
      try {
        await s.post(`/dws/${n}`), await h(l, !0);
      } catch (r) {
        E(r instanceof Error ? r.message : "钉钉连接失败");
      } finally {
        d(!1);
      }
    }
  }, Te = async () => {
    if (s) {
      d(!0);
      try {
        await s.post("/dws/cancel"), await h(l, !0);
      } catch (n) {
        E(n instanceof Error ? n.message : "取消操作失败");
      } finally {
        d(!1);
      }
    }
  }, Oe = async (n) => {
    s && (await s.delete(`/principals/${n}`), await h(l));
  }, je = async (n, r) => {
    if (s)
      try {
        await s.patch(`/principals/${n}/policy`, { policy: r }), await h(l, !0);
      } catch (p) {
        E(p instanceof Error ? p.message : "策略更新失败");
      }
  }, Le = async (n) => {
    if (s) {
      d(!0);
      try {
        await s.post(`/outbox/${n}/send`), await h(l);
      } catch (r) {
        E(r instanceof Error ? r.message : "发送失败");
      } finally {
        d(!1);
      }
    }
  }, Re = async (n) => {
    s && (await s.delete(`/outbox/${n}`), await h(l));
  }, Be = async () => {
    if (!(!s || !z || !j.trim())) {
      d(!0);
      try {
        await s.patch(`/outbox/${z.id}`, {
          text: j.trim()
        }), V(null), await h(l), await (a == null ? void 0 : a.host.toast("草稿已保存", "success"));
      } catch (n) {
        E(n instanceof Error ? n.message : "草稿保存失败");
      } finally {
        d(!1);
      }
    }
  };
  if (T && !t)
    return /* @__PURE__ */ e.createElement("div", { className: "pm-page" }, /* @__PURE__ */ e.createElement(fe, null));
  const U = (t == null ? void 0 : t.work_items.filter((n) => n.status === "identity_required")) || [], le = (t == null ? void 0 : t.outbox.filter((n) => n.status !== "sent")) || [], v = !!(t != null && t.identity_provider.authenticated), S = !!(t != null && t.identity_provider.available), W = (t == null ? void 0 : t.runtime.integration_stage) || "idle", R = [
    "install",
    "downloading",
    "preparing",
    "installing",
    "verifying",
    "login"
  ].includes(W);
  if (!v || !(t != null && t.settings.enabled)) {
    const n = S ? v ? 2 : 1 : 0, r = (u) => u < n ? /* @__PURE__ */ e.createElement(oe, { size: 17 }) : /* @__PURE__ */ e.createElement(nt, { size: 17 }), p = S ? v ? "选择负责回复的 Agent" : "连接你的钉钉账号" : "准备钉钉连接组件", ie = (t == null ? void 0 : t.runtime.integration_detail) || (S ? v ? "任意已启用 Agent 都可以负责回复，认证由 Agent 自己管理。" : "浏览器将打开钉钉官方 OAuth；插件不会读取或保存账号密码。" : "组件安装在 Paw Me 的独立目录，不修改系统 PATH。");
    return /* @__PURE__ */ e.createElement("div", { className: "pm-page" }, /* @__PURE__ */ e.createElement("style", null, we), /* @__PURE__ */ e.createElement("header", { className: "pm-header" }, /* @__PURE__ */ e.createElement("div", { className: "pm-header-copy" }, /* @__PURE__ */ e.createElement("div", { className: "pm-eyebrow" }, /* @__PURE__ */ e.createElement(G, { size: 15 }), "Paw Me · Digital Twin"), /* @__PURE__ */ e.createElement(J, { level: 1 }, "钉钉数字人分身"), /* @__PURE__ */ e.createElement(g, { type: "secondary" }, "首次配置只需要安装连接组件、完成钉钉授权并选择 Agent。"))), O ? /* @__PURE__ */ e.createElement(
      D,
      {
        closable: !0,
        type: "error",
        message: "操作未完成",
        description: O,
        onClose: () => E(""),
        style: { marginBottom: 16 }
      }
    ) : null, /* @__PURE__ */ e.createElement(w, { className: "pm-onboarding" }, /* @__PURE__ */ e.createElement("div", { className: "pm-onboarding-head" }, /* @__PURE__ */ e.createElement(J, { level: 2 }, "开始设置 Paw Me"), /* @__PURE__ */ e.createElement(g, { type: "secondary" }, "完成下面三个步骤后，消息监听、会话授权、草稿与发送会在同一页面运行。")), /* @__PURE__ */ e.createElement("div", { className: "pm-steps" }, ["安装连接组件", "钉钉 OAuth", "选择并启用 Agent"].map(
      (u, A) => /* @__PURE__ */ e.createElement(
        "div",
        {
          className: `pm-step ${A === n ? "pm-step-current" : ""} ${A < n ? "pm-step-done" : ""}`,
          key: u
        },
        /* @__PURE__ */ e.createElement("span", { className: "pm-step-icon" }, r(A)),
        /* @__PURE__ */ e.createElement("span", null, u)
      )
    )), /* @__PURE__ */ e.createElement("div", { className: "pm-onboarding-action" }, /* @__PURE__ */ e.createElement("h3", null, p), /* @__PURE__ */ e.createElement(g, { type: "secondary" }, ie), R ? /* @__PURE__ */ e.createElement("div", { className: "pm-progress" }, /* @__PURE__ */ e.createElement(
      zt,
      {
        percent: (t == null ? void 0 : t.runtime.integration_progress) ?? 0,
        showInfo: (t == null ? void 0 : t.runtime.integration_progress) != null,
        status: "active"
      }
    ), (t == null ? void 0 : t.runtime.integration_progress) == null ? /* @__PURE__ */ e.createElement(k, { size: 8 }, /* @__PURE__ */ e.createElement(fe, { size: "small" }), /* @__PURE__ */ e.createElement(g, { type: "secondary" }, "正在执行当前阶段")) : null) : null, v ? /* @__PURE__ */ e.createElement(
      M,
      {
        className: "pm-agent-select",
        value: l,
        options: i.map((u) => ({
          value: u.id,
          label: `${u.name || u.id} · ${u.backend || "agent"}`
        })),
        onChange: (u) => C(u)
      }
    ) : null, /* @__PURE__ */ e.createElement("div", { className: "pm-onboarding-buttons" }, S ? v ? /* @__PURE__ */ e.createElement(
      o,
      {
        type: "primary",
        size: "large",
        icon: /* @__PURE__ */ e.createElement(oe, { size: 17 }),
        loading: f,
        disabled: !l,
        onClick: () => void L({
          enabled: !0,
          agent_id: l,
          default_policy: (t == null ? void 0 : t.settings.default_policy) || "draft",
          quiet_seconds: (t == null ? void 0 : t.settings.quiet_seconds) ?? 4,
          max_wait_seconds: (t == null ? void 0 : t.settings.max_wait_seconds) ?? 20
        })
      },
      "启用数字人分身"
    ) : /* @__PURE__ */ e.createElement(
      o,
      {
        type: "primary",
        size: "large",
        icon: /* @__PURE__ */ e.createElement(pe, { size: 17 }),
        disabled: R,
        onClick: () => void P("login")
      },
      "连接钉钉"
    ) : /* @__PURE__ */ e.createElement(
      o,
      {
        type: "primary",
        size: "large",
        icon: /* @__PURE__ */ e.createElement(me, { size: 17 }),
        disabled: R,
        onClick: () => void P("install")
      },
      "安装并继续"
    ), R ? /* @__PURE__ */ e.createElement(
      o,
      {
        size: "large",
        icon: /* @__PURE__ */ e.createElement(xt, { size: 17 }),
        loading: f,
        onClick: () => void Te()
      },
      "取消当前操作"
    ) : W === "failed" || W === "cancelled" ? /* @__PURE__ */ e.createElement(
      o,
      {
        size: "large",
        icon: /* @__PURE__ */ e.createElement(Z, { size: 17 }),
        onClick: () => void P(S ? "login" : "install")
      },
      "重新尝试"
    ) : null))));
  }
  const Fe = /* @__PURE__ */ e.createElement(
    w,
    {
      className: "pm-panel",
      title: "消息批次",
      extra: /* @__PURE__ */ e.createElement(g, { type: "secondary" }, "连续消息只回复一次")
    },
    /* @__PURE__ */ e.createElement(
      _,
      {
        dataSource: (t == null ? void 0 : t.work_items) || [],
        locale: { emptyText: /* @__PURE__ */ e.createElement(ye, { description: "尚未捕获新消息" }) },
        renderItem: (n) => /* @__PURE__ */ e.createElement(
          _.Item,
          {
            actions: n.status === "identity_required" ? [
              /* @__PURE__ */ e.createElement(
                o,
                {
                  key: "authorize",
                  type: "primary",
                  onClick: () => re(n)
                },
                "审核并授权"
              )
            ] : []
          },
          /* @__PURE__ */ e.createElement(
            _.Item.Meta,
            {
              title: /* @__PURE__ */ e.createElement("div", { className: "pm-item-title" }, /* @__PURE__ */ e.createElement("span", null, n.conversation_alias), /* @__PURE__ */ e.createElement(be, { status: n.status }), /* @__PURE__ */ e.createElement($, null, n.message_count, " 条已合并")),
              description: /* @__PURE__ */ e.createElement(e.Fragment, null, /* @__PURE__ */ e.createElement("span", null, n.agent_id, " · ", Y(n.updated_at)), /* @__PURE__ */ e.createElement("div", { className: "pm-id" }, n.subject_type === "person" ? "人员" : "群聊", " ·", " ", n.subject_id || "未获得真实 ID", " ·", " ", ee(n.id_source)), n.error ? /* @__PURE__ */ e.createElement("div", { className: "pm-error" }, n.error) : null, /* @__PURE__ */ e.createElement("div", { className: "pm-message-stack" }, n.messages.map((r) => /* @__PURE__ */ e.createElement("div", { className: "pm-message", key: r.id }, r.text))))
            }
          )
        )
      }
    )
  ), He = /* @__PURE__ */ e.createElement(w, { className: "pm-panel", title: "OAuth、身份与权限" }, /* @__PURE__ */ e.createElement("div", { className: "pm-setup" }, /* @__PURE__ */ e.createElement("div", { className: "pm-setup-copy" }, /* @__PURE__ */ e.createElement("div", { className: "pm-setup-title" }, v ? `${(t == null ? void 0 : t.identity_provider.user_name) || "钉钉账号"} 已连接` : t != null && t.identity_provider.available ? "连接组件已就绪，等待 OAuth 登录" : "安装钉钉连接组件"), /* @__PURE__ */ e.createElement(g, { type: "secondary" }, (t == null ? void 0 : t.runtime.integration_detail) || (t == null ? void 0 : t.identity_provider.detail) || "OAuth 由钉钉官方能力管理，插件不读取或保存令牌。"), v ? /* @__PURE__ */ e.createElement("div", { className: "pm-id" }, (t == null ? void 0 : t.identity_provider.corp_name) || "当前组织", " · userId", " ", (t == null ? void 0 : t.identity_provider.user_id) || "—") : null), t != null && t.identity_provider.available ? v ? /* @__PURE__ */ e.createElement(
    o,
    {
      icon: /* @__PURE__ */ e.createElement(Z, { size: 16 }),
      onClick: () => void h(l)
    },
    "刷新登录状态"
  ) : /* @__PURE__ */ e.createElement(
    o,
    {
      type: "primary",
      icon: /* @__PURE__ */ e.createElement(pe, { size: 16 }),
      loading: f || (t == null ? void 0 : t.runtime.integration_stage) === "login",
      onClick: () => void P("login")
    },
    "使用钉钉 OAuth 登录"
  ) : /* @__PURE__ */ e.createElement(
    o,
    {
      type: "primary",
      icon: /* @__PURE__ */ e.createElement(me, { size: 16 }),
      loading: f || (t == null ? void 0 : t.runtime.integration_stage) === "install",
      onClick: () => void P("install")
    },
    "安装连接组件"
  )), /* @__PURE__ */ e.createElement(
    D,
    {
      showIcon: !0,
      type: "info",
      message: "授权只来自收到的真实事件",
      description: "人员 openDingTalkId 或群 openConversationId 由钉钉 OAuth 事件写入，界面不可手填。未授权会话统一进入待审核，不会调用 Agent。",
      style: { marginBottom: 16 }
    }
  ), U.length ? /* @__PURE__ */ e.createElement(
    _,
    {
      header: /* @__PURE__ */ e.createElement("strong", null, "待授权会话"),
      dataSource: U,
      renderItem: (n) => /* @__PURE__ */ e.createElement(
        _.Item,
        {
          actions: [
            /* @__PURE__ */ e.createElement(
              o,
              {
                key: "authorize",
                type: "primary",
                onClick: () => re(n)
              },
              "审核并授权"
            )
          ]
        },
        /* @__PURE__ */ e.createElement(
          _.Item.Meta,
          {
            title: n.display_name || n.conversation_alias,
            description: /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "pm-id" }, n.subject_id), /* @__PURE__ */ e.createElement(g, { type: "secondary" }, n.subject_type === "person" ? "人员" : "群聊", " ·", " ", n.id_source))
          }
        )
      )
    }
  ) : null, /* @__PURE__ */ e.createElement(
    Mt,
    {
      rowKey: "id",
      pagination: !1,
      dataSource: (t == null ? void 0 : t.principals) || [],
      locale: { emptyText: "暂无已验证身份" },
      columns: [
        {
          title: "身份",
          render: (n, r) => /* @__PURE__ */ e.createElement(e.Fragment, null, /* @__PURE__ */ e.createElement("div", null, r.display_name), /* @__PURE__ */ e.createElement(g, { type: "secondary" }, r.subject_type === "person" ? "人员" : "群聊"))
        },
        {
          title: "真实 ID",
          render: (n, r) => /* @__PURE__ */ e.createElement(e.Fragment, null, /* @__PURE__ */ e.createElement("div", null, r.subject_id), /* @__PURE__ */ e.createElement(g, { type: "secondary" }, ee(r.id_source)))
        },
        { title: "会话", dataIndex: "conversation_alias" },
        {
          title: "策略",
          render: (n, r) => /* @__PURE__ */ e.createElement(
            M,
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
              onChange: (p) => void je(r.id, p)
            }
          )
        },
        {
          title: "操作",
          render: (n, r) => /* @__PURE__ */ e.createElement(
            ve,
            {
              title: "删除此身份策略？后续消息将重新进入待授权。",
              onConfirm: () => void Oe(r.id)
            },
            /* @__PURE__ */ e.createElement(o, { type: "text", danger: !0, icon: /* @__PURE__ */ e.createElement(ge, { size: 15 }) }, "删除")
          )
        }
      ],
      scroll: { x: 760 }
    }
  )), Ve = /* @__PURE__ */ e.createElement(
    w,
    {
      className: "pm-panel",
      title: "待发送",
      extra: /* @__PURE__ */ e.createElement(g, { type: "secondary" }, "按 OAuth 真实 ID 精确发送")
    },
    /* @__PURE__ */ e.createElement(
      _,
      {
        dataSource: le,
        locale: { emptyText: /* @__PURE__ */ e.createElement(ye, { description: "暂无待发送回复" }) },
        renderItem: (n) => /* @__PURE__ */ e.createElement(
          _.Item,
          {
            actions: [
              /* @__PURE__ */ e.createElement(
                o,
                {
                  key: "edit",
                  icon: /* @__PURE__ */ e.createElement(ue, { size: 15 }),
                  onClick: () => {
                    V(n), ae(n.text);
                  }
                },
                "编辑"
              ),
              /* @__PURE__ */ e.createElement(
                o,
                {
                  key: "send",
                  type: "primary",
                  icon: /* @__PURE__ */ e.createElement(X, { size: 15 }),
                  loading: f,
                  onClick: () => void Le(n.id)
                },
                "发送"
              ),
              /* @__PURE__ */ e.createElement(
                ve,
                {
                  key: "delete",
                  title: "删除草稿？原始消息仍会保留。",
                  onConfirm: () => void Re(n.id)
                },
                /* @__PURE__ */ e.createElement(o, { danger: !0, type: "text", icon: /* @__PURE__ */ e.createElement(ge, { size: 15 }) }, "删除")
              )
            ]
          },
          /* @__PURE__ */ e.createElement(
            _.Item.Meta,
            {
              title: /* @__PURE__ */ e.createElement("div", { className: "pm-item-title" }, /* @__PURE__ */ e.createElement("span", null, n.conversation_alias), /* @__PURE__ */ e.createElement(be, { status: n.status })),
              description: /* @__PURE__ */ e.createElement(e.Fragment, null, /* @__PURE__ */ e.createElement("p", { className: "pm-pre" }, n.text), n.error ? /* @__PURE__ */ e.createElement("div", { className: "pm-error" }, n.error) : null, /* @__PURE__ */ e.createElement("div", { className: "pm-meta" }, Y(n.updated_at)))
            }
          )
        )
      }
    )
  ), Qe = /* @__PURE__ */ e.createElement(w, { className: "pm-panel", title: "运行记录" }, /* @__PURE__ */ e.createElement(
    It,
    {
      items: ((t == null ? void 0 : t.activity) || []).map((n) => ({
        color: n.status === "failed" ? "red" : n.status === "sent" || n.status === "verified" ? "green" : "blue",
        children: /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "pm-item-title" }, /* @__PURE__ */ e.createElement("strong", null, n.title), /* @__PURE__ */ e.createElement($, null, n.status)), n.detail ? /* @__PURE__ */ e.createElement("div", { className: "pm-subtle" }, n.detail) : null, /* @__PURE__ */ e.createElement("div", { className: "pm-meta" }, Y(n.created_at)))
      }))
    }
  ));
  return /* @__PURE__ */ e.createElement("div", { className: "pm-page" }, /* @__PURE__ */ e.createElement("style", null, we), /* @__PURE__ */ e.createElement("header", { className: "pm-header" }, /* @__PURE__ */ e.createElement("div", { className: "pm-header-copy" }, /* @__PURE__ */ e.createElement("div", { className: "pm-eyebrow" }, /* @__PURE__ */ e.createElement(G, { size: 15 }), "Paw Me · Digital Twin"), /* @__PURE__ */ e.createElement(J, { level: 1 }, "钉钉数字人分身"), /* @__PURE__ */ e.createElement(g, { type: "secondary" }, "使用所选 Agent 和本机钉钉 OAuth 登录态，在一个页面完成实时收件、 独立授权、上下文聚合、处理、草稿、发送与审计。")), /* @__PURE__ */ e.createElement("div", { className: "pm-actions" }, /* @__PURE__ */ e.createElement(
    M,
    {
      value: l,
      style: { minWidth: 190 },
      options: i.map((n) => ({
        value: n.id,
        label: `${n.name || n.id} · ${n.backend || "agent"}`
      })),
      onChange: (n) => void Se(n)
    }
  ), /* @__PURE__ */ e.createElement(o, { icon: /* @__PURE__ */ e.createElement(gt, { size: 16 }), onClick: De }, "设置"), /* @__PURE__ */ e.createElement(
    o,
    {
      icon: /* @__PURE__ */ e.createElement(Z, { size: 16 }),
      onClick: () => void h(l)
    },
    "刷新"
  ), /* @__PURE__ */ e.createElement(k, null, /* @__PURE__ */ e.createElement(
    xe,
    {
      checked: t == null ? void 0 : t.settings.enabled,
      disabled: !v,
      onChange: (n) => void Pe(n)
    }
  ), /* @__PURE__ */ e.createElement(g, null, t != null && t.settings.enabled ? "运行中" : "已停止")))), O ? /* @__PURE__ */ e.createElement(
    D,
    {
      closable: !0,
      type: "error",
      message: "操作未完成",
      description: O,
      onClose: () => E(""),
      style: { marginBottom: 16 }
    }
  ) : null, /* @__PURE__ */ e.createElement(w, { className: "pm-statusbar" }, /* @__PURE__ */ e.createElement("div", { className: "pm-status-inner" }, /* @__PURE__ */ e.createElement("div", { className: "pm-status-main" }, t != null && t.runtime.running ? /* @__PURE__ */ e.createElement(_t, { status: "processing" }) : /* @__PURE__ */ e.createElement(et, { size: 18 }), /* @__PURE__ */ e.createElement("div", { className: "pm-status-text" }, /* @__PURE__ */ e.createElement("div", { className: "pm-status-title" }, (t == null ? void 0 : t.runtime.stage) || "stopped"), /* @__PURE__ */ e.createElement(g, { className: "pm-status-detail", type: "secondary" }, (t == null ? void 0 : t.runtime.detail) || "等待启动"))), /* @__PURE__ */ e.createElement(k, { wrap: !0 }, /* @__PURE__ */ e.createElement(
    $,
    {
      icon: /* @__PURE__ */ e.createElement(G, { size: 13 }),
      color: v ? "success" : "warning"
    },
    v ? "钉钉 OAuth 已连接" : "等待钉钉 OAuth"
  ), /* @__PURE__ */ e.createElement($, { icon: /* @__PURE__ */ e.createElement(rt, { size: 13 }) }, "静默 ", (t == null ? void 0 : t.settings.quiet_seconds) ?? 4, " 秒"), t != null && t.runtime.current_conversation ? /* @__PURE__ */ e.createElement($, { icon: /* @__PURE__ */ e.createElement(ue, { size: 13 }) }, t.runtime.current_conversation) : null))), /* @__PURE__ */ e.createElement(At, { gutter: [14, 14] }, /* @__PURE__ */ e.createElement(B, { xs: 12, lg: 6 }, /* @__PURE__ */ e.createElement(w, { className: "pm-metric" }, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-icon" }, /* @__PURE__ */ e.createElement(de, { size: 18 })), /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-value" }, (t == null ? void 0 : t.work_items.length) || 0), /* @__PURE__ */ e.createElement("div", { className: "pm-metric-label" }, "消息批次")))), /* @__PURE__ */ e.createElement(B, { xs: 12, lg: 6 }, /* @__PURE__ */ e.createElement(w, { className: "pm-metric" }, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-icon" }, /* @__PURE__ */ e.createElement(ot, { size: 18 })), /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-value" }, U.length), /* @__PURE__ */ e.createElement("div", { className: "pm-metric-label" }, "待绑定身份")))), /* @__PURE__ */ e.createElement(B, { xs: 12, lg: 6 }, /* @__PURE__ */ e.createElement(w, { className: "pm-metric" }, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-icon" }, /* @__PURE__ */ e.createElement(X, { size: 18 })), /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-value" }, le.length), /* @__PURE__ */ e.createElement("div", { className: "pm-metric-label" }, "待发送")))), /* @__PURE__ */ e.createElement(B, { xs: 12, lg: 6 }, /* @__PURE__ */ e.createElement(w, { className: "pm-metric" }, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-icon" }, /* @__PURE__ */ e.createElement(ce, { size: 18 })), /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-value" }, (t == null ? void 0 : t.principals.length) || 0), /* @__PURE__ */ e.createElement("div", { className: "pm-metric-label" }, "已验证身份"))))), /* @__PURE__ */ e.createElement(
    $t,
    {
      defaultActiveKey: "inbox",
      items: [
        {
          key: "inbox",
          label: /* @__PURE__ */ e.createElement(k, null, /* @__PURE__ */ e.createElement(de, { size: 15 }), "收件与处理"),
          children: Fe
        },
        {
          key: "permissions",
          label: /* @__PURE__ */ e.createElement(k, null, /* @__PURE__ */ e.createElement(vt, { size: 15 }), "身份与权限"),
          children: He
        },
        {
          key: "outbox",
          label: /* @__PURE__ */ e.createElement(k, null, /* @__PURE__ */ e.createElement(X, { size: 15 }), "待发送"),
          children: Ve
        },
        {
          key: "activity",
          label: /* @__PURE__ */ e.createElement(k, null, /* @__PURE__ */ e.createElement(ce, { size: 15 }), "运行记录"),
          children: Qe
        }
      ]
    }
  ), /* @__PURE__ */ e.createElement(
    Nt,
    {
      title: "运行设置",
      width: 420,
      open: Ie,
      onClose: () => F(!1),
      destroyOnClose: !0,
      extra: /* @__PURE__ */ e.createElement(
        o,
        {
          type: "primary",
          loading: f,
          onClick: () => Q.submit()
        },
        "保存"
      )
    },
    /* @__PURE__ */ e.createElement(
      b,
      {
        form: Q,
        layout: "vertical",
        onFinish: L,
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
          M,
          {
            options: i.map((n) => ({
              value: n.id,
              label: `${n.name || n.id} · ${n.backend || "agent"}`
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
        /* @__PURE__ */ e.createElement(xe, null)
      ),
      /* @__PURE__ */ e.createElement(b.Item, { name: "default_policy", label: "默认回复策略" }, /* @__PURE__ */ e.createElement(
        M,
        {
          options: [
            { value: "draft", label: "生成草稿，确认后发送" },
            { value: "automatic", label: "按身份策略自动发送" }
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
        /* @__PURE__ */ e.createElement(Ee, { min: 1, max: 30, style: { width: "100%" } })
      ),
      /* @__PURE__ */ e.createElement(
        b.Item,
        {
          name: "max_wait_seconds",
          label: "最长聚合等待（秒）",
          extra: "持续聊天时也不会无限等待。"
        },
        /* @__PURE__ */ e.createElement(Ee, { min: 3, max: 120, style: { width: "100%" } })
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
    he,
    {
      title: "授权真实钉钉会话",
      open: !!c,
      confirmLoading: f,
      onCancel: () => H(null),
      onOk: () => K.submit(),
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
      kt,
      {
        size: "small",
        column: 1,
        bordered: !0,
        style: { marginBottom: 18 },
        items: [
          {
            key: "name",
            label: "会话",
            children: (c == null ? void 0 : c.display_name) || (c == null ? void 0 : c.conversation_alias) || "—"
          },
          {
            key: "type",
            label: "类型",
            children: (c == null ? void 0 : c.subject_type) === "group" ? "群聊" : "人员"
          },
          {
            key: "id",
            label: "真实 ID",
            children: /* @__PURE__ */ e.createElement("span", { className: "pm-id" }, (c == null ? void 0 : c.subject_id) || "—")
          },
          {
            key: "source",
            label: "来源",
            children: ee(c == null ? void 0 : c.id_source)
          }
        ]
      }
    ),
    /* @__PURE__ */ e.createElement(
      b,
      {
        form: K,
        layout: "vertical",
        onFinish: qe
      },
      /* @__PURE__ */ e.createElement(
        b.Item,
        {
          name: "policy",
          label: "权限策略",
          rules: [{ required: !0 }]
        },
        /* @__PURE__ */ e.createElement(
          M,
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
    he,
    {
      title: `编辑发给 ${(z == null ? void 0 : z.conversation_alias) || ""} 的草稿`,
      open: !!z,
      confirmLoading: f,
      okButtonProps: { disabled: !j.trim() },
      onCancel: () => V(null),
      onOk: () => void Be(),
      destroyOnClose: !0
    },
    /* @__PURE__ */ e.createElement(
      Ct.TextArea,
      {
        autoSize: { minRows: 6, maxRows: 16 },
        value: j,
        onChange: (n) => ae(n.target.value)
      }
    )
  ));
}
var Ne;
const ke = (Ne = window.QwenPaw.paw) == null ? void 0 : Ne.forApp(ne);
var Ce, ze;
ke ? ke.ui.registerPage({
  path: "/apps/paw-me-dingtalk",
  label: "Paw Me · DingTalk",
  component: _e
}) : (ze = (Ce = window.QwenPaw).registerRoutes) == null || ze.call(Ce, ne, [
  {
    path: "/apps/paw-me-dingtalk",
    component: _e,
    label: "Paw Me · DingTalk"
  }
]);
