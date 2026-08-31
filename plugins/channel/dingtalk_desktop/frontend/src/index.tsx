import type * as ReactNS from "react";
import {
  Bot,
  Check,
  CircleAlert,
  ExternalLink,
  Laptop,
  LoaderCircle,
  LockKeyhole,
  MessageSquareText,
  RefreshCw,
  Send,
  ShieldCheck,
  Trash2,
} from "lucide-react";

const host = window.QwenPaw.host;
const React: typeof ReactNS = host.React;
const { useEffect, useState } = React;

type DesktopStatus = {
  installed: boolean;
  running: boolean;
  accessibility: boolean;
  logged_in: boolean;
  version: string;
  detail: string;
};

type CodexStatus = {
  installed: boolean;
  authenticated: boolean;
  error?: string | null;
};

type Status = {
  agent_id: string;
  backend: string;
  configured: boolean;
  desktop: DesktopStatus;
  codex: CodexStatus;
  draft_count: number;
};

type Draft = {
  id: string;
  conversation: string;
  text: string;
  created_at: number;
};

const styles = `
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

async function requestJson<T>(path: string, init?: RequestInit): Promise<T> {
  const response = host.fetch
    ? await host.fetch(path, init)
    : await fetch(host.getApiUrl(path), {
        ...init,
        headers: {
          "Content-Type": "application/json",
          ...(init?.headers || {}),
          ...(host.getApiToken()
            ? { Authorization: `Bearer ${host.getApiToken()}` }
            : {}),
        },
      });
  const payload = await response.json().catch(() => ({}));
  if (!response.ok) {
    throw new Error(payload.detail || `HTTP ${response.status}`);
  }
  return payload as T;
}

function State({ ok, text }: { ok: boolean; text: string }) {
  return (
    <span className={`dt-state ${ok ? "ok" : ""}`}>
      {ok ? <Check size={14} /> : <CircleAlert size={14} />}
      {text}
    </span>
  );
}

function DingTalkDesktopPage() {
  const [status, setStatus] = useState<Status | null>(null);
  const [drafts, setDrafts] = useState<Draft[]>([]);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");

  const refresh = async () => {
    setError("");
    try {
      const [nextStatus, nextDrafts] = await Promise.all([
        requestJson<Status>("/dingtalk-desktop/status"),
        requestJson<{ drafts: Draft[] }>("/dingtalk-desktop/drafts"),
      ]);
      setStatus(nextStatus);
      setDrafts(nextDrafts.drafts);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "加载失败");
    }
  };

  useEffect(() => {
    void refresh();
  }, []);

  const login = async () => {
    setBusy("login");
    setError("");
    const popup = window.open("about:blank", "_blank");
    try {
      const result = await requestJson<{ authUrl?: string }>(
        "/harnesses/codex/login",
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ device_code: false, settings: {} }),
        },
      );
      if (result.authUrl && popup) {
        popup.location.href = result.authUrl;
      } else if (result.authUrl) {
        window.open(result.authUrl, "_blank", "noopener,noreferrer");
      } else {
        popup?.close();
      }
    } catch (reason) {
      popup?.close();
      setError(reason instanceof Error ? reason.message : "OAuth 启动失败");
    } finally {
      setBusy("");
    }
  };

  const setup = async (replyMode: "draft" | "automatic") => {
    setBusy(replyMode);
    setError("");
    try {
      await requestJson("/dingtalk-desktop/setup", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ reply_mode: replyMode }),
      });
      await refresh();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "连接失败");
    } finally {
      setBusy("");
    }
  };

  const resolveDraft = async (draft: Draft, action: "send" | "delete") => {
    setBusy(draft.id);
    setError("");
    try {
      await requestJson(
        `/dingtalk-desktop/drafts/${draft.id}${
          action === "send" ? "/send" : ""
        }`,
        { method: action === "send" ? "POST" : "DELETE" },
      );
      await refresh();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "操作失败");
    } finally {
      setBusy("");
    }
  };

  const codexReady = Boolean(
    status?.backend === "codex" && status.codex.authenticated,
  );
  const desktopReady = Boolean(
    status?.desktop.logged_in && status.desktop.accessibility,
  );

  return (
    <div className="dt-shell">
      <style>{styles}</style>
      <main className="dt-wrap">
        <header className="dt-hero">
          <div>
            <div className="dt-kicker">
              <ShieldCheck size={15} /> Personal channel
            </div>
            <h1 className="dt-title">Codex，接管当前阿里钉会话</h1>
            <p className="dt-sub">
              使用 QwenPaw 已有的 Codex OAuth 与本机阿里钉登录态。没有机器人、
              没有 webhook，也不读取或保存任何账号凭证。
            </p>
          </div>
          <button className="dt-button" onClick={() => void refresh()}>
            <RefreshCw size={16} /> 刷新状态
          </button>
        </header>

        <section className="dt-grid">
          <article className="dt-card">
            <div className="dt-card-head">
              <div className="dt-icon">
                <Bot size={20} />
              </div>
              <State ok={codexReady} text={codexReady ? "已认证" : "未就绪"} />
            </div>
            <h2>当前 Codex Agent</h2>
            <p>
              {status ? `${status.agent_id} · ${status.backend}` : "正在检查"}
            </p>
            {!status?.codex.authenticated && (
              <div className="dt-actions">
                <button className="dt-button" onClick={() => void login()}>
                  {busy === "login" ? (
                    <LoaderCircle size={16} />
                  ) : (
                    <ExternalLink size={16} />
                  )}
                  通过 ChatGPT OAuth 登录
                </button>
              </div>
            )}
          </article>

          <article className="dt-card">
            <div className="dt-card-head">
              <div className="dt-icon">
                <Laptop size={20} />
              </div>
              <State
                ok={desktopReady}
                text={desktopReady ? "本机已连接" : "需要检查"}
              />
            </div>
            <h2>阿里钉桌面端</h2>
            <p>
              {status?.desktop.version
                ? `版本 ${status.desktop.version} · 本机登录态`
                : "请打开阿里钉并完成登录"}
            </p>
          </article>

          <article className="dt-card dt-wide">
            <div className="dt-card-head">
              <div className="dt-icon">
                <LockKeyhole size={20} />
              </div>
              <State
                ok={Boolean(status?.configured)}
                text={status?.configured ? "已绑定" : "等待绑定"}
              />
            </div>
            <h2>绑定当前打开的会话</h2>
            <p>
              插件只读取当前可见且标题完全匹配的白名单会话，不使用坐标，
              不自动点击或切换会话。发送前会再次验证会话标题。
            </p>
            <div className="dt-actions">
              <button
                className="dt-button dt-primary"
                disabled={!codexReady || !desktopReady || Boolean(busy)}
                onClick={() => void setup("draft")}
              >
                {busy === "draft" ? (
                  <LoaderCircle size={16} />
                ) : (
                  <MessageSquareText size={16} />
                )}
                一键连接并使用草稿
              </button>
              <button
                className="dt-button"
                disabled={!codexReady || !desktopReady || Boolean(busy)}
                onClick={() => void setup("automatic")}
              >
                <Send size={16} /> 明确启用自动回复
              </button>
            </div>
            <div className="dt-notice">
              <CircleAlert size={18} />
              <span>
                建议先使用草稿模式。自动回复只对绑定时当前打开的会话生效。
              </span>
            </div>
          </article>
        </section>

        <section className="dt-section">
          <div className="dt-section-top">
            <h2>待审批草稿</h2>
            <span className="dt-meta">{drafts.length} 条</span>
          </div>
          {drafts.length === 0 ? (
            <div className="dt-card dt-empty">暂无待审批草稿</div>
          ) : (
            drafts.map((draft) => (
              <article className="dt-card dt-draft" key={draft.id}>
                <div>
                  <div className="dt-conversation">{draft.conversation}</div>
                  <div className="dt-meta">
                    {new Date(draft.created_at * 1000).toLocaleString()}
                  </div>
                </div>
                <div className="dt-copy">{draft.text}</div>
                <div className="dt-actions">
                  <button
                    className="dt-button dt-primary"
                    disabled={busy === draft.id}
                    onClick={() => void resolveDraft(draft, "send")}
                  >
                    <Send size={15} /> 发送
                  </button>
                  <button
                    className="dt-button dt-danger"
                    disabled={busy === draft.id}
                    onClick={() => void resolveDraft(draft, "delete")}
                  >
                    <Trash2 size={15} /> 删除
                  </button>
                </div>
              </article>
            ))
          )}
        </section>
        {error && <div className="dt-error">{error}</div>}
      </main>
    </div>
  );
}

window.QwenPaw.registerRoutes?.("dingtalk-desktop", [
  {
    path: "/plugin/dingtalk-desktop",
    component: DingTalkDesktopPage,
    label: "阿里钉 · Codex",
    icon: "message-square-text",
    priority: 44,
  },
]);
