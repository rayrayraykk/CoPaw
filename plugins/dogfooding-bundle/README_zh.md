# Dogfooding Bundle 插件

内部组织专属捆绑包，一次安装/卸载，相关能力全到位。

> English documentation: [README.md](README.md)

---

## 包含功能

| 能力 | 说明 |
|------|------|
| **AgentScope Dogfooding Provider** | 注册 `agentscope-dogfooding` LLM Provider，通过 AgentScope 代理路由请求 |
| **AgentTrack 启动 Hook** | 应用启动时自动初始化 AgentTrack SDK（`app_name="qwenpaw"`） |
| **/feedback 命令** | 将 `/feedback` 查询重写为 Agent Prompt，引导用户完成反馈表单 |
| **Dogfooding Account API** | 暴露 `POST /api/dogfooding-account/`，保存 dogfooding 用户账号 |

## 安装

> **前置条件**：AgentTrack SDK 需要阿里内部 PyPI 源，请确保网络可达
> `artlab.alibaba-inc.com`。

```bash
# 从本地目录安装
qwenpaw plugin install plugins/dogfooding-bundle

# 确认已加载
qwenpaw plugin list
qwenpaw plugin info dogfooding-bundle
```

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

安装后，在 QwenPaw 设置界面选择 **AgentScope Dogfooding** 作为 Provider，填入 API Key 即可。

默认模型：`qwen3.6-plus-dogfooding`（Qwen 3.6 Plus，支持多模态）

### AgentTrack 监控

无需任何操作，启动后自动运行。可在日志中确认：

```
INFO | AgentTrack initialized (app_name=qwenpaw)
```

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

保存当前 dogfooding 用户账号到 QwenPaw 工作目录：

```bash
curl -X POST http://127.0.0.1:8088/api/dogfooding-account/ \
  -H 'Content-Type: application/json' \
  -d '{"user_account":"example"}'
```

接口会写入 QwenPaw 工作目录下的 `dogfooding/user_account.json`。

## 目录结构

```
dogfooding-bundle/
├── plugin.json          # 插件清单（type: bundle）
├── plugin.py            # 入口，单个 register() 注册全部能力
├── query_rewriter.py    # /feedback 命令的 Prompt 重写逻辑
├── requirements.txt     # Python 依赖
├── README.md            # 英文文档
└── README_zh.md         # 本文档（中文）
```

## 启动日志示例

```
INFO | Dogfooding Bundle: AgentScope Dogfooding provider registered
INFO | Dogfooding Bundle: AgentTrack startup hook registered
INFO | Dogfooding Bundle: /feedback query-rewrite hook registered
INFO | Dogfooding account API registered at POST /api/dogfooding-account/
INFO | Dogfooding Bundle fully registered
INFO | AgentTrack initialized (app_name=qwenpaw)
INFO | Patched AgentRunner.query_handler for /feedback command
```

## 故障排查

### 插件未加载

```bash
qwenpaw plugin list
# 查看日志
tail -f ~/.qwenpaw/logs/qwenpaw.log | grep -i dogfooding
```

### AgentTrack 初始化失败

1. 检查 `agenttrack-sdk` 是否已安装（需要阿里内网 PyPI）
2. 查看日志中的 `Failed to import AgentTrack SDK` 错误
3. 初始化失败不会阻止 QwenPaw 启动，仅影响监控功能

### /feedback 命令不响应

1. 确认插件已安装并在日志中看到 `Patched AgentRunner.query_handler`
2. 命令区分大小写，必须以 `/feedback` 开头
