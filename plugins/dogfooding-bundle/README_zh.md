# Dogfooding Bundle 插件

内部组织专属捆绑包，一次安装/卸载，相关能力全到位。

> English documentation: [README.md](README.md)

**版本：** 1.1.0 · **最低 QwenPaw：** 1.1.7

---

## 包含功能

| 能力 | 说明 |
|------|------|
| **AgentScope Dogfooding Provider** | 注册 `agentscope-dogfooding` LLM Provider，代理地址 `http://proxy.agentscope.design/v1`，默认模型 `qwen3.7-max-dogfooding`（Qwen3.7-Max-DogFooding） |
| **Console 反馈 UI** | 每条 AI 回复下方展示「糟糕 / 一般 / 优秀」；点「糟糕」弹出原因多选 |
| **DingTalk 反馈卡片** | AI 回复完成后自动发送 actionCard 评分卡片，支持二级原因收集 |
| **数据回流** | 按集团 AI 埋点规范回流 Q&A 详情与用户反馈；本地 jsonl + AgentTrack OTLP 平台上报 |
| **AgentTrack 启动 Hook** | 应用启动时自动初始化 AgentTrack SDK（`app_name="qwenpaw"`） |
| **集团账号登录** | Console 插件页支持阿里集团 SSO（新标签登录 + 回填 code） |
| **/feedback 命令** | 将 `/feedback` 查询重写为 Agent Prompt，引导用户完成反馈表单 |
| **Dogfooding Account API** | 暴露 `POST /api/dogfooding-account/`，保存 dogfooding 用户工号 |

## 安装

> **前置条件**：AgentTrack SDK 需要阿里内部 PyPI 源，请确保网络可达
> `artlab.alibaba-inc.com`。

```bash
# 从 zip 包安装
qwenpaw plugin install ~/Desktop/dogfooding-bundle-1.1.0.zip

# 或从本地目录安装
qwenpaw plugin install plugins/bundle/dogfooding-bundle

# 确认已加载
qwenpaw plugin list
qwenpaw plugin info dogfooding-bundle

# 重启应用使插件生效
qwenpaw app
```

> **注意：** 插件 Python 代码修改后需**重启** `qwenpaw app`（无热更新）。前端 `web/index.js` 修改后需硬刷新浏览器（`Cmd+Shift+R`）。

## 卸载

```bash
qwenpaw plugin uninstall dogfooding-bundle
```

## 依赖说明

`requirements.txt` 中声明的依赖会在安装时自动从阿里内部 PyPI 安装：

```
--index-url https://artlab.alibaba-inc.com/1/pypi/simple
agenttrack-sdk[agentscope]==0.9.4
harbor
wrapt<2.0.0
```

> **为什么需要 `wrapt<2.0.0`？**
> `agenttrack-sdk 0.9.4` 内部调用 `wrap_function_wrapper(module=..., name=..., wrapper=...)` 关键字形式，
> 而 `wrapt 2.x` 将该参数改为位置参数并移除了关键字支持，导致 AgentScope/OpenAI 埋点全部失效。
> 固定到 `wrapt<2.0.0`（即 1.17.x）可恢复所有 instrumentation。

## 使用方法

### AgentScope Dogfooding Provider

安装后，在 QwenPaw 插件页完成集团 SSO 登录即可；**API Key 会自动写入 AgentScope Dogfooding 模型配置**，无需再到「设置 → 模型」手动粘贴。

- 默认模型：`qwen3.7-max-dogfooding`（显示名 Qwen3.7-Max-DogFooding，支持多模态）
- 代理地址：`http://proxy.agentscope.design/v1`（安装时会自动迁移旧版 IP:8081 配置）

### Console 反馈

使用 Dogfooding 模型时，每条 AI 回复下方会出现评分按钮：

| 按钮 | `score_label` | `score` |
|------|---------------|---------|
| 糟糕 | `bad` | 1 |
| 一般 | `fine` | 2 |
| 优秀 | `good` | 3 |

点击「糟糕」需选择原因后提交。若刚流式完成的回复尚未带上 `trace_id`，后端会按会话自动回填。

### 集团账号登录（Console 插件页）

本地开发环境 SSO 回调为 `http://127.0.0.1`，服务端仅接受 `https://` 回调，因此采用**新标签页登录 + 回填 code**：

1. 在 dogfooding 插件页点击「阿里集团账号登录」
2. 在新标签页完成 SSO
3. 将地址栏中的 `code`（或整段回调 URL）粘贴回插件页，点击「完成登录」
4. 工号写入 `dogfooding/user_account.json`，**API Key 自动写入 AgentScope Dogfooding Provider 配置**（可在「设置 → 模型」确认）

### DingTalk 反馈

DingTalk channel 在 AI 回复完成后会发送 actionCard 评分卡片；选「糟糕」后引导选择原因。

### 数据回流 / 埋点

#### 本地备份

所有记录追加写入工作目录下的：

```
{WORKING_DIR}/dogfooding/backflow/records.jsonl
```

`WORKING_DIR` 默认为 `~/.copaw`（若存在）或 `~/.qwenpaw`，可通过环境变量 `QWENPAW_WORKING_DIR` 覆盖。

#### 平台上报（AgentTrack）

启动时自动 `AgentTrack.init(app_name="qwenpaw")`，通过 OTLP 将 span 推送到 EagleEye / Sunfire 采集端。

| 记录类型 | Span 名称 | 说明 |
|----------|-----------|------|
| 问答详情 | `dogfooding.qa_detail` | 每轮 dogfooding 对话完成后写入 |
| 用户反馈 | `fu.track.interaction.feedback` | 用户点击评分按钮后写入 |

反馈 span 会挂到对应 **chat span** 下（通过 `eagleeye.rpc_id` 关联），Trace 详情中应出现在 `chat qwen3.7-max-dogfooding` 节点内，而非「独立节点」。

#### 埋点字段规范

**公共字段**

- `sam` = `idealab_talk.chat.{conversation_id}.{trace_id}`
- `trace_id` — 每轮请求唯一 ID（EagleEye 格式）
- `modelId` — `Qwen3.7-Max-DogFooding`
- `product_code` — `qwenpaw`
- `product_version` — 当前 QwenPaw 版本

**用户反馈（`fu.track.interaction.feedback`）**

- `gmkey` = `CLK`
- `logkey` = `fu.track.interaction.feedback`
- `score` / `score_label` — 见上表
- `feedback_reason` / `feedback_comment` — 点「糟糕」时的原因与补充说明

**问答详情（`qa_detail`）**

- `prompt_message` / `response_message` — 本轮用户输入与助手回复
- `channel_type` — `console` / `web` / `dingtalk` 等

#### 反馈 API

```bash
curl -X POST http://127.0.0.1:8088/api/dogfooding-feedback/ \
  -H 'Content-Type: application/json' \
  -d '{
    "trace_id": "7f00000117827167540001001aa7be00",
    "conversation_id": "1782710278779-tvd77qt",
    "score_label": "bad",
    "channel_type": "web",
    "feedback_reason": "结果有误"
  }'
```

`trace_id` 可省略，后端会按 `conversation_id` 回填最近一轮的 trace。

### AgentTrack 监控

无需额外配置，启动后自动运行。可在日志中确认：

```
INFO | AgentTrack initialized (app_name=qwenpaw)
INFO | Bundle SpanProcessor registered
```

在 AgentTrack / EagleEye 平台按 `trace_id` 检索，可看到 LLM chat span 及其下的 `dogfooding.qa_detail`、`fu.track.interaction.feedback` 子 span，`score` / `score_label` 在 span attributes 中。

### /feedback 命令

**交互模式**（不带参数）：

```
用户: /feedback
Agent: 感谢您的反馈！请对本次对话进行评价：...
```

**快速模式**（带参数）：

```
用户: /feedback 结果有误，代码逻辑错误
Agent: 根据您的描述，我理解您的评价是：糟糕...
```

### Dogfooding Account API

保存当前 dogfooding 用户工号到工作目录：

```bash
curl -X POST http://127.0.0.1:8088/api/dogfooding-account/ \
  -H 'Content-Type: application/json' \
  -d '{"user_account":"287738"}'
```

写入 `{WORKING_DIR}/dogfooding/user_account.json`，用于埋点 `user_id` / `alibaba.base.emp_id`。

## 前端开发

修改 `src/index.tsx` 后需重新构建：

```bash
cd plugins/bundle/dogfooding-bundle
npm install
npm run build   # 输出 web/index.js
```

## 目录结构

```
dogfooding-bundle/
├── plugin.json          # 插件清单（type: bundle）
├── plugin.py            # 入口，注册 provider / 埋点 / 反馈 API / hooks
├── tracking.py          # 埋点、本地 jsonl、AgentTrack span 上报
├── feedback_service.py  # 反馈业务逻辑与 DingTalk 卡片 payload
├── channel_hooks.py     # DingTalk channel 反馈卡片 hook
├── query_rewriter.py    # /feedback 命令的 Prompt 重写逻辑
├── requirements.txt     # Python 依赖
├── src/index.tsx        # Console 前端源码（反馈 UI + SSO 登录）
├── web/index.js         # 构建产物（plugin.json entry.frontend）
├── README.md            # 英文文档
└── README_zh.md         # 本文档（中文）
```

## 启动日志示例

```
INFO | Dogfooding Bundle: AgentScope Dogfooding provider registered
INFO | Dogfooding Bundle: AgentTrack startup hook registered
INFO | Dogfooding feedback runtime hook registered
INFO | Dogfooding feedback API registered at POST /api/dogfooding-feedback/
INFO | Dogfooding account API registered at POST /api/dogfooding-account/
INFO | Dogfooding Bundle fully registered
INFO | AgentTrack initialized (app_name=qwenpaw)
INFO | Bundle SpanProcessor registered
INFO | Patched finalize_console_turn_usage for QA backflow
```

## 故障排查

### 插件未加载

```bash
qwenpaw plugin list
tail -f ~/.copaw/qwenpaw.log | grep -i dogfooding
```

### 反馈按钮不显示

1. 确认当前模型为 **Qwen3.7-Max-DogFooding**（Provider 选 AgentScope Dogfooding）
2. 硬刷新浏览器（`Cmd+Shift+R`）
3. 确认 `web/index.js` 已重新构建并安装

### 平台上看不到 score / score_label

1. 确认 `agenttrack-sdk` 已安装且日志中有 `AgentTrack initialized`
2. 确认已重启 `qwenpaw app` 后再提交反馈
3. 在 Trace 详情中查找 span `fu.track.interaction.feedback`，检查 attributes

### 反馈出现在「独立节点」而非 chat 下

需使用含 `eagleeye.rpc_id` 关联的最新版插件，并重启应用后再测。

### AgentTrack 初始化失败

1. 检查 `agenttrack-sdk` 是否已安装（需要阿里内网 PyPI）
2. 查看日志中的 `Failed to import AgentTrack SDK` 错误
3. 初始化失败不会阻止 QwenPaw 启动，但平台上报与 LLM 自动埋点会降级

### /feedback 命令不响应

1. 确认插件已安装并在日志中看到 `Patched AgentRunner.query_handler`
2. 命令区分大小写，必须以 `/feedback` 开头

### 模型连接失败

Dogfooding Provider 应使用 `http://proxy.agentscope.design/v1`，勿使用旧版 `121.43.136.192:8081`（部分网络环境下会被拦截）。
