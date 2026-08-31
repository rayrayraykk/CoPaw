const Ee = window.QwenPaw.host.React, V = Ee.createElement, he = Ee.forwardRef;
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Pe = (n) => n.replace(/([a-z0-9])([A-Z])/g, "$1-$2").toLowerCase(), qe = (n) => n.replace(
  /^([A-Z])|[\s-_]+(\w)/g,
  (i, p, a) => a ? a.toUpperCase() : p.toLowerCase()
), X = (n) => {
  const i = qe(n);
  return i.charAt(0).toUpperCase() + i.slice(1);
}, fe = (...n) => n.filter((i, p, a) => !!i && i.trim() !== "" && a.indexOf(i) === p).join(" ").trim(), je = (n) => {
  for (const i in n)
    if (i.startsWith("aria-") || i === "role" || i === "title")
      return !0;
};
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
var Te = {
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
const Oe = he(
  ({
    color: n = "currentColor",
    size: i = 24,
    strokeWidth: p = 2,
    absoluteStrokeWidth: a,
    className: b = "",
    children: l,
    iconNode: A,
    ...D
  }, M) => V(
    "svg",
    {
      ref: M,
      ...Te,
      width: i,
      height: i,
      stroke: n,
      strokeWidth: a ? Number(p) * 24 / Number(i) : p,
      className: fe("lucide", b),
      ...!l && !je(D) && { "aria-hidden": "true" },
      ...D
    },
    [
      ...A.map(([w, d]) => V(w, d)),
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
const m = (n, i) => {
  const p = he(
    ({ className: a, ...b }, l) => V(Oe, {
      ref: l,
      iconNode: i,
      className: fe(
        `lucide-${Pe(X(n))}`,
        `lucide-${n}`,
        a
      ),
      ...b
    })
  );
  return p.displayName = X(n), p;
};
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Le = [
  [
    "path",
    {
      d: "M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2",
      key: "169zse"
    }
  ]
], Y = m("activity", Le);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Re = [
  ["circle", { cx: "12", cy: "12", r: "10", key: "1mglay" }],
  ["line", { x1: "10", x2: "10", y1: "15", y2: "9", key: "c1nkhi" }],
  ["line", { x1: "14", x2: "14", y1: "15", y2: "9", key: "h65svq" }]
], Fe = m("circle-pause", Re);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Be = [
  ["path", { d: "M12 6v6h4", key: "135r8i" }],
  ["circle", { cx: "12", cy: "12", r: "10", key: "1mglay" }]
], We = m("clock-3", Be);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Ve = [
  ["path", { d: "M12 15V3", key: "m9g1x1" }],
  ["path", { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4", key: "ih7n3h" }],
  ["path", { d: "m7 10 5 5 5-5", key: "brsn70" }]
], He = m("download", Ve);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Qe = [
  ["path", { d: "M15 3h6v6", key: "1q9fwt" }],
  ["path", { d: "M10 14 21 3", key: "gplh6r" }],
  ["path", { d: "M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6", key: "a6xqqp" }]
], Ke = m("external-link", Qe);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Ue = [
  ["polyline", { points: "22 12 16 12 14 15 10 15 8 12 2 12", key: "o97t9d" }],
  [
    "path",
    {
      d: "M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z",
      key: "oot6mr"
    }
  ]
], ee = m("inbox", Ue);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Ze = [
  ["circle", { cx: "12", cy: "16", r: "1", key: "1au0dj" }],
  ["rect", { x: "3", y: "10", width: "18", height: "12", rx: "2", key: "6s8ecr" }],
  ["path", { d: "M7 10V7a5 5 0 0 1 10 0v3", key: "1pqi11" }]
], Ge = m("lock-keyhole", Ze);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Je = [
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
], te = m("message-square-text", Je);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Xe = [
  ["path", { d: "M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8", key: "v9h5vc" }],
  ["path", { d: "M21 3v5h-5", key: "1q7to0" }],
  ["path", { d: "M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16", key: "3uifl3" }],
  ["path", { d: "M8 16H3v5", key: "1cv678" }]
], ae = m("refresh-cw", Xe);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const Ye = [
  [
    "path",
    {
      d: "M14.536 21.686a.5.5 0 0 0 .937-.024l6.5-19a.496.496 0 0 0-.635-.635l-19 6.5a.5.5 0 0 0-.024.937l7.93 3.18a2 2 0 0 1 1.112 1.11z",
      key: "1ffxy3"
    }
  ],
  ["path", { d: "m21.854 2.147-10.94 10.939", key: "12cjpa" }]
], B = m("send", Ye);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const et = [
  ["path", { d: "M14 17H5", key: "gfn3mx" }],
  ["path", { d: "M19 7h-9", key: "6i9tg" }],
  ["circle", { cx: "17", cy: "17", r: "3", key: "18b49y" }],
  ["circle", { cx: "7", cy: "7", r: "3", key: "dfmy0x" }]
], tt = m("settings-2", et);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const at = [
  [
    "path",
    {
      d: "M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z",
      key: "oel41y"
    }
  ],
  ["path", { d: "m9 12 2 2 4-4", key: "dzmm74" }]
], ne = m("shield-check", at);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const nt = [
  ["path", { d: "M10 11v6", key: "nco0om" }],
  ["path", { d: "M14 11v6", key: "outv1u" }],
  ["path", { d: "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6", key: "miytrc" }],
  ["path", { d: "M3 6h18", key: "d0wm0j" }],
  ["path", { d: "M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2", key: "e791ji" }]
], re = m("trash-2", nt);
/**
 * @license lucide-react v0.562.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const rt = [
  ["path", { d: "m16 11 2 2 4-4", key: "9rsbq5" }],
  ["path", { d: "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2", key: "1yyitq" }],
  ["circle", { cx: "9", cy: "7", r: "4", key: "nufk8" }]
], lt = m("user-check", rt), H = "paw-me-dingtalk", S = window.QwenPaw.host, e = S.React, { useEffect: it, useMemo: st, useState: E } = e, {
  Alert: $,
  Badge: ct,
  Button: u,
  Card: f,
  Col: P,
  Descriptions: ot,
  Drawer: mt,
  Empty: le,
  Form: h,
  Input: pt,
  InputNumber: ie,
  List: v,
  Modal: se,
  Popconfirm: ce,
  Row: dt,
  Select: z,
  Space: N,
  Spin: ut,
  Switch: oe,
  Table: yt,
  Tabs: gt,
  Tag: C,
  Timeline: Et,
  Typography: ht
} = S.antd, { Text: x, Title: ft } = ht, vt = `
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
`, xt = {
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
function W(n) {
  return n ? new Date(n * 1e3).toLocaleString() : "—";
}
function me({ status: n }) {
  const i = n === "sent" ? "success" : n === "failed" || n === "blocked" ? "error" : n === "draft_ready" || n === "identity_required" ? "warning" : "processing";
  return /* @__PURE__ */ e.createElement(C, { color: i }, xt[n] || n);
}
function pe() {
  const n = st(() => {
    var t;
    return (t = window.QwenPaw.paw) == null ? void 0 : t.forApp(H);
  }, []), [i, p] = E([]), [a, b] = E(null), [l, A] = E(
    (n == null ? void 0 : n.host.getSelectedAgentId()) || "default"
  ), [D, M] = E(!0), [w, d] = E(!1), [Q, g] = E(""), [ve, q] = E(!1), [s, j] = E(
    null
  ), [_, T] = E(null), [I, K] = E(""), [O] = h.useForm(), [L] = h.useForm(), c = n == null ? void 0 : n.api, y = async (t = l, r = !1) => {
    if (!c) {
      g("当前 QwenPaw 版本未提供 PawApp SDK"), M(!1);
      return;
    }
    r || M(!0);
    try {
      const o = await c.get("/snapshot", {
        query: { agent_id: t }
      });
      b(o), o.settings.agent_id && o.settings.agent_id !== l && A(o.settings.agent_id), g("");
    } catch (o) {
      g(o instanceof Error ? o.message : "状态加载失败");
    } finally {
      r || M(!1);
    }
  };
  it(() => {
    let t = !1;
    (async () => {
      try {
        const $e = await (S.fetch ? await S.fetch("/agents") : await fetch(S.getApiUrl("/agents"))).json();
        t || p(
          ($e.agents || []).filter(
            (J) => J.enabled && J.available_in_chat !== !1
          )
        );
      } catch {
        t || p([]);
      }
      t || await y(l);
    })();
    const o = window.setInterval(
      () => void y(l, !0),
      2e3
    );
    return () => {
      t = !0, window.clearInterval(o);
    };
  }, [l]);
  const R = async (t) => {
    if (c) {
      d(!0);
      try {
        const r = await c.put("/settings", t, {
          query: { agent_id: String(t.agent_id) }
        });
        A(String(t.agent_id)), b(r), q(!1), await (n == null ? void 0 : n.host.toast("Paw Me 设置已保存", "success"));
      } catch (r) {
        g(r instanceof Error ? r.message : "设置保存失败");
      } finally {
        d(!1);
      }
    }
  }, xe = async (t) => {
    a && await R({ ...a.settings, enabled: t, agent_id: l });
  }, we = async (t) => {
    A(t), a && await R({
      ...a.settings,
      agent_id: t
    });
  }, be = () => {
    O.setFieldsValue({ ...a == null ? void 0 : a.settings, agent_id: l }), q(!0);
  }, U = (t) => {
    L.setFieldsValue({
      policy: (a == null ? void 0 : a.settings.default_policy) || "draft"
    }), j(t);
  }, _e = async (t) => {
    if (!(!c || !s)) {
      d(!0);
      try {
        await c.post(`/work-items/${s.id}/authorize`, t), j(null), await y(l), await (n == null ? void 0 : n.host.toast("真实身份已授权", "success"));
      } catch (r) {
        g(r instanceof Error ? r.message : "身份授权失败");
      } finally {
        d(!1);
      }
    }
  }, Z = async (t) => {
    if (c) {
      d(!0);
      try {
        await c.post(`/dws/${t}`), await y(l, !0);
      } catch (r) {
        g(r instanceof Error ? r.message : "DWS 配置失败");
      } finally {
        d(!1);
      }
    }
  }, ke = async (t) => {
    c && (await c.delete(`/principals/${t}`), await y(l));
  }, Ne = async (t, r) => {
    if (c)
      try {
        await c.patch(`/principals/${t}/policy`, { policy: r }), await y(l, !0);
      } catch (o) {
        g(o instanceof Error ? o.message : "策略更新失败");
      }
  }, Ce = async (t) => {
    if (c) {
      d(!0);
      try {
        await c.post(`/outbox/${t}/send`), await y(l);
      } catch (r) {
        g(r instanceof Error ? r.message : "发送失败");
      } finally {
        d(!1);
      }
    }
  }, Ae = async (t) => {
    c && (await c.delete(`/outbox/${t}`), await y(l));
  }, Me = async () => {
    if (!(!c || !_ || !I.trim())) {
      d(!0);
      try {
        await c.patch(`/outbox/${_.id}`, {
          text: I.trim()
        }), T(null), await y(l), await (n == null ? void 0 : n.host.toast("草稿已保存", "success"));
      } catch (t) {
        g(t instanceof Error ? t.message : "草稿保存失败");
      } finally {
        d(!1);
      }
    }
  };
  if (D && !a)
    return /* @__PURE__ */ e.createElement("div", { className: "pm-page" }, /* @__PURE__ */ e.createElement(ut, null));
  const F = (a == null ? void 0 : a.work_items.filter((t) => t.status === "identity_required")) || [], G = (a == null ? void 0 : a.outbox.filter((t) => t.status !== "sent")) || [], k = !!(a != null && a.identity_provider.authenticated), ze = /* @__PURE__ */ e.createElement(
    f,
    {
      className: "pm-panel",
      title: "消息批次",
      extra: /* @__PURE__ */ e.createElement(x, { type: "secondary" }, "连续消息只回复一次")
    },
    /* @__PURE__ */ e.createElement(
      v,
      {
        dataSource: (a == null ? void 0 : a.work_items) || [],
        locale: { emptyText: /* @__PURE__ */ e.createElement(le, { description: "尚未捕获新消息" }) },
        renderItem: (t) => /* @__PURE__ */ e.createElement(
          v.Item,
          {
            actions: t.status === "identity_required" ? [
              /* @__PURE__ */ e.createElement(
                u,
                {
                  key: "authorize",
                  type: "primary",
                  onClick: () => U(t)
                },
                "审核并授权"
              )
            ] : []
          },
          /* @__PURE__ */ e.createElement(
            v.Item.Meta,
            {
              title: /* @__PURE__ */ e.createElement("div", { className: "pm-item-title" }, /* @__PURE__ */ e.createElement("span", null, t.conversation_alias), /* @__PURE__ */ e.createElement(me, { status: t.status }), /* @__PURE__ */ e.createElement(C, null, t.message_count, " 条已合并")),
              description: /* @__PURE__ */ e.createElement(e.Fragment, null, /* @__PURE__ */ e.createElement("span", null, t.agent_id, " · ", W(t.updated_at)), /* @__PURE__ */ e.createElement("div", { className: "pm-id" }, t.subject_type === "person" ? "人员" : "群聊", " ·", " ", t.subject_id || "未获得真实 ID", " ·", " ", t.id_source || "无可信来源"), t.error ? /* @__PURE__ */ e.createElement("div", { className: "pm-error" }, t.error) : null, /* @__PURE__ */ e.createElement("div", { className: "pm-message-stack" }, t.messages.map((r) => /* @__PURE__ */ e.createElement("div", { className: "pm-message", key: r.id }, r.text))))
            }
          )
        )
      }
    )
  ), Se = /* @__PURE__ */ e.createElement(f, { className: "pm-panel", title: "OAuth、身份与权限" }, /* @__PURE__ */ e.createElement("div", { className: "pm-setup" }, /* @__PURE__ */ e.createElement("div", { className: "pm-setup-copy" }, /* @__PURE__ */ e.createElement("div", { className: "pm-setup-title" }, k ? `${(a == null ? void 0 : a.identity_provider.user_name) || "钉钉账号"} 已连接` : a != null && a.identity_provider.available ? "DWS 已安装，等待 OAuth 登录" : "安装钉钉官方 DWS"), /* @__PURE__ */ e.createElement(x, { type: "secondary" }, (a == null ? void 0 : a.runtime.integration_detail) || (a == null ? void 0 : a.identity_provider.detail) || "OAuth 由钉钉官方 DWS 管理，插件不读取或保存令牌。"), k ? /* @__PURE__ */ e.createElement("div", { className: "pm-id" }, (a == null ? void 0 : a.identity_provider.corp_name) || "当前组织", " · userId", " ", (a == null ? void 0 : a.identity_provider.user_id) || "—") : null), a != null && a.identity_provider.available ? k ? /* @__PURE__ */ e.createElement(
    u,
    {
      icon: /* @__PURE__ */ e.createElement(ae, { size: 16 }),
      onClick: () => void y(l)
    },
    "刷新登录状态"
  ) : /* @__PURE__ */ e.createElement(
    u,
    {
      type: "primary",
      icon: /* @__PURE__ */ e.createElement(Ke, { size: 16 }),
      loading: w || (a == null ? void 0 : a.runtime.integration_stage) === "login",
      onClick: () => void Z("login")
    },
    "使用钉钉 OAuth 登录"
  ) : /* @__PURE__ */ e.createElement(
    u,
    {
      type: "primary",
      icon: /* @__PURE__ */ e.createElement(He, { size: 16 }),
      loading: w || (a == null ? void 0 : a.runtime.integration_stage) === "install",
      onClick: () => void Z("install")
    },
    "一键安装 DWS"
  )), /* @__PURE__ */ e.createElement(
    $,
    {
      showIcon: !0,
      type: "info",
      message: "授权只来自收到的真实事件",
      description: "人员 openDingTalkId 或群 openConversationId 由 DWS OAuth 事件写入，界面不可手填。未授权会话统一进入待审核，不会调用 Agent。",
      style: { marginBottom: 16 }
    }
  ), F.length ? /* @__PURE__ */ e.createElement(
    v,
    {
      header: /* @__PURE__ */ e.createElement("strong", null, "待授权会话"),
      dataSource: F,
      renderItem: (t) => /* @__PURE__ */ e.createElement(
        v.Item,
        {
          actions: [
            /* @__PURE__ */ e.createElement(
              u,
              {
                key: "authorize",
                type: "primary",
                onClick: () => U(t)
              },
              "审核并授权"
            )
          ]
        },
        /* @__PURE__ */ e.createElement(
          v.Item.Meta,
          {
            title: t.display_name || t.conversation_alias,
            description: /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "pm-id" }, t.subject_id), /* @__PURE__ */ e.createElement(x, { type: "secondary" }, t.subject_type === "person" ? "人员" : "群聊", " ·", " ", t.id_source))
          }
        )
      )
    }
  ) : null, /* @__PURE__ */ e.createElement(
    yt,
    {
      rowKey: "id",
      pagination: !1,
      dataSource: (a == null ? void 0 : a.principals) || [],
      locale: { emptyText: "暂无已验证身份" },
      columns: [
        {
          title: "身份",
          render: (t, r) => /* @__PURE__ */ e.createElement(e.Fragment, null, /* @__PURE__ */ e.createElement("div", null, r.display_name), /* @__PURE__ */ e.createElement(x, { type: "secondary" }, r.subject_type === "person" ? "人员" : "群聊"))
        },
        {
          title: "真实 ID",
          render: (t, r) => /* @__PURE__ */ e.createElement(e.Fragment, null, /* @__PURE__ */ e.createElement("div", null, r.subject_id), /* @__PURE__ */ e.createElement(x, { type: "secondary" }, r.id_source))
        },
        { title: "会话", dataIndex: "conversation_alias" },
        {
          title: "策略",
          render: (t, r) => /* @__PURE__ */ e.createElement(
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
              onChange: (o) => void Ne(r.id, o)
            }
          )
        },
        {
          title: "操作",
          render: (t, r) => /* @__PURE__ */ e.createElement(
            ce,
            {
              title: "删除此身份策略？后续消息将重新进入待授权。",
              onConfirm: () => void ke(r.id)
            },
            /* @__PURE__ */ e.createElement(u, { type: "text", danger: !0, icon: /* @__PURE__ */ e.createElement(re, { size: 15 }) }, "删除")
          )
        }
      ],
      scroll: { x: 760 }
    }
  )), De = /* @__PURE__ */ e.createElement(
    f,
    {
      className: "pm-panel",
      title: "待发送",
      extra: /* @__PURE__ */ e.createElement(x, { type: "secondary" }, "按 OAuth 真实 ID 精确发送")
    },
    /* @__PURE__ */ e.createElement(
      v,
      {
        dataSource: G,
        locale: { emptyText: /* @__PURE__ */ e.createElement(le, { description: "暂无待发送回复" }) },
        renderItem: (t) => /* @__PURE__ */ e.createElement(
          v.Item,
          {
            actions: [
              /* @__PURE__ */ e.createElement(
                u,
                {
                  key: "edit",
                  icon: /* @__PURE__ */ e.createElement(te, { size: 15 }),
                  onClick: () => {
                    T(t), K(t.text);
                  }
                },
                "编辑"
              ),
              /* @__PURE__ */ e.createElement(
                u,
                {
                  key: "send",
                  type: "primary",
                  icon: /* @__PURE__ */ e.createElement(B, { size: 15 }),
                  loading: w,
                  onClick: () => void Ce(t.id)
                },
                "发送"
              ),
              /* @__PURE__ */ e.createElement(
                ce,
                {
                  key: "delete",
                  title: "删除草稿？原始消息仍会保留。",
                  onConfirm: () => void Ae(t.id)
                },
                /* @__PURE__ */ e.createElement(u, { danger: !0, type: "text", icon: /* @__PURE__ */ e.createElement(re, { size: 15 }) }, "删除")
              )
            ]
          },
          /* @__PURE__ */ e.createElement(
            v.Item.Meta,
            {
              title: /* @__PURE__ */ e.createElement("div", { className: "pm-item-title" }, /* @__PURE__ */ e.createElement("span", null, t.conversation_alias), /* @__PURE__ */ e.createElement(me, { status: t.status })),
              description: /* @__PURE__ */ e.createElement(e.Fragment, null, /* @__PURE__ */ e.createElement("p", { className: "pm-pre" }, t.text), t.error ? /* @__PURE__ */ e.createElement("div", { className: "pm-error" }, t.error) : null, /* @__PURE__ */ e.createElement("div", { className: "pm-meta" }, W(t.updated_at)))
            }
          )
        )
      }
    )
  ), Ie = /* @__PURE__ */ e.createElement(f, { className: "pm-panel", title: "运行记录" }, /* @__PURE__ */ e.createElement(
    Et,
    {
      items: ((a == null ? void 0 : a.activity) || []).map((t) => ({
        color: t.status === "failed" ? "red" : t.status === "sent" || t.status === "verified" ? "green" : "blue",
        children: /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "pm-item-title" }, /* @__PURE__ */ e.createElement("strong", null, t.title), /* @__PURE__ */ e.createElement(C, null, t.status)), t.detail ? /* @__PURE__ */ e.createElement("div", { className: "pm-subtle" }, t.detail) : null, /* @__PURE__ */ e.createElement("div", { className: "pm-meta" }, W(t.created_at)))
      }))
    }
  ));
  return /* @__PURE__ */ e.createElement("div", { className: "pm-page" }, /* @__PURE__ */ e.createElement("style", null, vt), /* @__PURE__ */ e.createElement("header", { className: "pm-header" }, /* @__PURE__ */ e.createElement("div", { className: "pm-header-copy" }, /* @__PURE__ */ e.createElement("div", { className: "pm-eyebrow" }, /* @__PURE__ */ e.createElement(ne, { size: 15 }), "Paw Me · Digital Twin"), /* @__PURE__ */ e.createElement(ft, { level: 1 }, "钉钉数字人分身"), /* @__PURE__ */ e.createElement(x, { type: "secondary" }, "使用所选 Agent 和本机钉钉 OAuth 登录态，在一个页面完成实时收件、 独立授权、上下文聚合、处理、草稿、发送与审计。")), /* @__PURE__ */ e.createElement("div", { className: "pm-actions" }, /* @__PURE__ */ e.createElement(
    z,
    {
      value: l,
      style: { minWidth: 190 },
      options: i.map((t) => ({
        value: t.id,
        label: `${t.name || t.id} · ${t.backend || "agent"}`
      })),
      onChange: (t) => void we(t)
    }
  ), /* @__PURE__ */ e.createElement(u, { icon: /* @__PURE__ */ e.createElement(tt, { size: 16 }), onClick: be }, "设置"), /* @__PURE__ */ e.createElement(
    u,
    {
      icon: /* @__PURE__ */ e.createElement(ae, { size: 16 }),
      onClick: () => void y(l)
    },
    "刷新"
  ), /* @__PURE__ */ e.createElement(N, null, /* @__PURE__ */ e.createElement(
    oe,
    {
      checked: a == null ? void 0 : a.settings.enabled,
      disabled: !k,
      onChange: (t) => void xe(t)
    }
  ), /* @__PURE__ */ e.createElement(x, null, a != null && a.settings.enabled ? "运行中" : "已停止")))), Q ? /* @__PURE__ */ e.createElement(
    $,
    {
      closable: !0,
      type: "error",
      message: "操作未完成",
      description: Q,
      onClose: () => g(""),
      style: { marginBottom: 16 }
    }
  ) : null, /* @__PURE__ */ e.createElement(f, { className: "pm-statusbar" }, /* @__PURE__ */ e.createElement("div", { className: "pm-status-inner" }, /* @__PURE__ */ e.createElement("div", { className: "pm-status-main" }, a != null && a.runtime.running ? /* @__PURE__ */ e.createElement(ct, { status: "processing" }) : /* @__PURE__ */ e.createElement(Fe, { size: 18 }), /* @__PURE__ */ e.createElement("div", { className: "pm-status-text" }, /* @__PURE__ */ e.createElement("div", { className: "pm-status-title" }, (a == null ? void 0 : a.runtime.stage) || "stopped"), /* @__PURE__ */ e.createElement(x, { className: "pm-status-detail", type: "secondary" }, (a == null ? void 0 : a.runtime.detail) || "等待启动"))), /* @__PURE__ */ e.createElement(N, { wrap: !0 }, /* @__PURE__ */ e.createElement(
    C,
    {
      icon: /* @__PURE__ */ e.createElement(ne, { size: 13 }),
      color: k ? "success" : "warning"
    },
    k ? "DWS OAuth 已连接" : "等待 DWS OAuth"
  ), /* @__PURE__ */ e.createElement(C, { icon: /* @__PURE__ */ e.createElement(We, { size: 13 }) }, "静默 ", (a == null ? void 0 : a.settings.quiet_seconds) ?? 4, " 秒"), a != null && a.runtime.current_conversation ? /* @__PURE__ */ e.createElement(C, { icon: /* @__PURE__ */ e.createElement(te, { size: 13 }) }, a.runtime.current_conversation) : null))), /* @__PURE__ */ e.createElement(dt, { gutter: [14, 14] }, /* @__PURE__ */ e.createElement(P, { xs: 12, lg: 6 }, /* @__PURE__ */ e.createElement(f, { className: "pm-metric" }, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-icon" }, /* @__PURE__ */ e.createElement(ee, { size: 18 })), /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-value" }, (a == null ? void 0 : a.work_items.length) || 0), /* @__PURE__ */ e.createElement("div", { className: "pm-metric-label" }, "消息批次")))), /* @__PURE__ */ e.createElement(P, { xs: 12, lg: 6 }, /* @__PURE__ */ e.createElement(f, { className: "pm-metric" }, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-icon" }, /* @__PURE__ */ e.createElement(Ge, { size: 18 })), /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-value" }, F.length), /* @__PURE__ */ e.createElement("div", { className: "pm-metric-label" }, "待绑定身份")))), /* @__PURE__ */ e.createElement(P, { xs: 12, lg: 6 }, /* @__PURE__ */ e.createElement(f, { className: "pm-metric" }, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-icon" }, /* @__PURE__ */ e.createElement(B, { size: 18 })), /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-value" }, G.length), /* @__PURE__ */ e.createElement("div", { className: "pm-metric-label" }, "待发送")))), /* @__PURE__ */ e.createElement(P, { xs: 12, lg: 6 }, /* @__PURE__ */ e.createElement(f, { className: "pm-metric" }, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-icon" }, /* @__PURE__ */ e.createElement(Y, { size: 18 })), /* @__PURE__ */ e.createElement("div", null, /* @__PURE__ */ e.createElement("div", { className: "pm-metric-value" }, (a == null ? void 0 : a.principals.length) || 0), /* @__PURE__ */ e.createElement("div", { className: "pm-metric-label" }, "已验证身份"))))), /* @__PURE__ */ e.createElement(
    gt,
    {
      defaultActiveKey: "inbox",
      items: [
        {
          key: "inbox",
          label: /* @__PURE__ */ e.createElement(N, null, /* @__PURE__ */ e.createElement(ee, { size: 15 }), "收件与处理"),
          children: ze
        },
        {
          key: "permissions",
          label: /* @__PURE__ */ e.createElement(N, null, /* @__PURE__ */ e.createElement(lt, { size: 15 }), "身份与权限"),
          children: Se
        },
        {
          key: "outbox",
          label: /* @__PURE__ */ e.createElement(N, null, /* @__PURE__ */ e.createElement(B, { size: 15 }), "待发送"),
          children: De
        },
        {
          key: "activity",
          label: /* @__PURE__ */ e.createElement(N, null, /* @__PURE__ */ e.createElement(Y, { size: 15 }), "运行记录"),
          children: Ie
        }
      ]
    }
  ), /* @__PURE__ */ e.createElement(
    mt,
    {
      title: "运行设置",
      width: 420,
      open: ve,
      onClose: () => q(!1),
      destroyOnClose: !0,
      extra: /* @__PURE__ */ e.createElement(
        u,
        {
          type: "primary",
          loading: w,
          onClick: () => O.submit()
        },
        "保存"
      )
    },
    /* @__PURE__ */ e.createElement(
      h,
      {
        form: O,
        layout: "vertical",
        onFinish: R,
        initialValues: a == null ? void 0 : a.settings
      },
      /* @__PURE__ */ e.createElement(
        h.Item,
        {
          name: "agent_id",
          label: "回复消息的 Agent",
          rules: [{ required: !0 }]
        },
        /* @__PURE__ */ e.createElement(
          z,
          {
            options: i.map((t) => ({
              value: t.id,
              label: `${t.name || t.id} · ${t.backend || "agent"}`
            }))
          }
        )
      ),
      /* @__PURE__ */ e.createElement(
        h.Item,
        {
          name: "enabled",
          label: "数字人分身总开关",
          valuePropName: "checked"
        },
        /* @__PURE__ */ e.createElement(oe, null)
      ),
      /* @__PURE__ */ e.createElement(h.Item, { name: "default_policy", label: "默认回复策略" }, /* @__PURE__ */ e.createElement(
        z,
        {
          options: [
            { value: "draft", label: "生成草稿，确认后发送" },
            { value: "automatic", label: "按身份策略自动发送" }
          ]
        }
      )),
      /* @__PURE__ */ e.createElement(
        h.Item,
        {
          name: "quiet_seconds",
          label: "连续消息静默窗口（秒）",
          extra: "对方停止输入达到这个时间后，才合并调用一次 Agent。"
        },
        /* @__PURE__ */ e.createElement(ie, { min: 1, max: 30, style: { width: "100%" } })
      ),
      /* @__PURE__ */ e.createElement(
        h.Item,
        {
          name: "max_wait_seconds",
          label: "最长聚合等待（秒）",
          extra: "持续聊天时也不会无限等待。"
        },
        /* @__PURE__ */ e.createElement(ie, { min: 3, max: 120, style: { width: "100%" } })
      ),
      /* @__PURE__ */ e.createElement(
        $,
        {
          type: "info",
          showIcon: !0,
          message: "上下文不会因中断丢失",
          description: "每条原始消息先写入 SQLite。Agent 运行中新消息到达时，旧任务会停止，新任务在同一会话中携带完整批次继续。"
        }
      )
    )
  ), /* @__PURE__ */ e.createElement(
    se,
    {
      title: "授权真实钉钉会话",
      open: !!s,
      confirmLoading: w,
      onCancel: () => j(null),
      onOk: () => L.submit(),
      destroyOnClose: !0
    },
    /* @__PURE__ */ e.createElement(
      $,
      {
        type: "info",
        showIcon: !0,
        message: "ID 已由 DWS OAuth 事件验证",
        description: "下列 ID 为只读值，不能手填或修改。授权后，相同真实 ID 的后续消息会按所选策略处理。",
        style: { marginBottom: 16 }
      }
    ),
    /* @__PURE__ */ e.createElement(
      ot,
      {
        size: "small",
        column: 1,
        bordered: !0,
        style: { marginBottom: 18 },
        items: [
          {
            key: "name",
            label: "会话",
            children: (s == null ? void 0 : s.display_name) || (s == null ? void 0 : s.conversation_alias) || "—"
          },
          {
            key: "type",
            label: "类型",
            children: (s == null ? void 0 : s.subject_type) === "group" ? "群聊" : "人员"
          },
          {
            key: "id",
            label: "真实 ID",
            children: /* @__PURE__ */ e.createElement("span", { className: "pm-id" }, (s == null ? void 0 : s.subject_id) || "—")
          },
          {
            key: "source",
            label: "来源",
            children: (s == null ? void 0 : s.id_source) || "—"
          }
        ]
      }
    ),
    /* @__PURE__ */ e.createElement(
      h,
      {
        form: L,
        layout: "vertical",
        onFinish: _e
      },
      /* @__PURE__ */ e.createElement(
        h.Item,
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
    se,
    {
      title: `编辑发给 ${(_ == null ? void 0 : _.conversation_alias) || ""} 的草稿`,
      open: !!_,
      confirmLoading: w,
      okButtonProps: { disabled: !I.trim() },
      onCancel: () => T(null),
      onOk: () => void Me(),
      destroyOnClose: !0
    },
    /* @__PURE__ */ e.createElement(
      pt.TextArea,
      {
        autoSize: { minRows: 6, maxRows: 16 },
        value: I,
        onChange: (t) => K(t.target.value)
      }
    )
  ));
}
var ue;
const de = (ue = window.QwenPaw.paw) == null ? void 0 : ue.forApp(H);
var ye, ge;
de ? de.ui.registerPage({
  path: "/apps/paw-me-dingtalk",
  label: "Paw Me · DingTalk",
  component: pe
}) : (ge = (ye = window.QwenPaw).registerRoutes) == null || ge.call(ye, H, [
  {
    path: "/apps/paw-me-dingtalk",
    component: pe,
    label: "Paw Me · DingTalk"
  }
]);
