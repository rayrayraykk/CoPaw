# Anthropic 原生推理本机验收

原 `AnthropicChatModel` / `auth_mode` 设置进入 Rust `ModelRequestOptions`。Core 原生调用 Messages，不调用 Python AgentScope、不修改 `console/src`、不把推理搬到 SDK。

## 依据

请求结构对照原 `src/qwenpaw/providers/anthropic_provider.py` 和 [Messages API](https://platform.claude.com/docs/en/api/messages/create)。事件生命周期、工具分片和累计用量对照 [官方 streaming 文档](https://platform.claude.com/docs/en/build-with-claude/streaming)，工具 schema 对照 [官方工具定义](https://platform.claude.com/docs/en/agents-and-tools/tool-use/define-tools)。测试不使用生产 key 或真实模型账号。

## 实现与测试

- 原模型 API 保存、添加模型和全局选择后，实际请求 `/proxy/v1/messages`。覆盖 root 和 `/v1` 两种地址；API key 模式、重启后的 bearer 模式，以及 bearer 模式强制剔除自定义 `x-api-key`。
- system 独立转换、连续同角色消息合并、工具 schema、tool_use/tool_result 对应关系和失败标记；max_tokens、thinking_enable/budget 和 disable_thinking 转换。
- 本地 HTTP 服务将 SSE 切成 11 字节网络片段，包含中文文本和分片 JSON。Core 实际读取临时 workspace 文件，再携带工具结果请求最终回答；重复于重开数据库及 detached restore 后的原会话。
- thinking/signature 原样保存、重开、回传；缓存 read/write/input 和多次累计 output usage 正确记录，provider_id 为实际选中的 `anthropic`，不再一律记成 `openai-compatible`。
- message_stop 才结束流；ping/未知事件可容忍；乱序、重复块/工具 ID、缺字段、非法输入 JSON、负数 token、过大流、断流和闲置超时被拒绝。SSE error 不回显上游私密错误文本；Core 中断真实流后完成 Interrupted。
- 对 OpenAI 请求完整结构断言：provider_content 和 tool_error 不上网。签名内容块在上下文裁剪时只能整体保留或明确返回超限，不能截断后继续发送。

执行示例（`qwenpaw-core` 目录）：

```sh
cargo test -p qwenpaw-core --all-features anthropic
cargo test -p qwenpaw-core --all-features native
cargo test -p qwenpaw-app-server --all-features anthropic
```

## 边界

2026-09-09：新增 9 个测试；工作区 371/371 普通测试、现有 5/5 原前端浏览器门禁通过。最后将 HTTP fixture 的 thinking budget/max_tokens 调整为 1024/2048 后，专项测试再次通过。严格 Clippy、格式/diff、inventory 3/3 与快照校验通过，`console/src` 零改动。新 release Core 通过 TypeScript SDK 3/3、Python SDK 4/4 和 VS Code 57/57 回归（真实连接未跳过）；这些客户端回归使用原兼容协议 fixture，不等于已验证跨进程 Anthropic 配置继承。

以上是 §14.2.24.28 的初始后端验收。它不是 Anthropic 全部功能或原 Models/Chat 页全部操作的等价证明。原 UI reasoning 展示、图片/文档输入、服务端工具/其他高级能力、其他 Agent 级设置、真实账号和安装态仍待完成。

## 原页面与 Agent 模型路由（§14.2.24.29）

新增显式 Chrome 门禁 `original_anthropic_browser_configures_selects_chats_and_reloads`。使用原 `console/dist`，依次在 Available Providers 配置 Anthropic、执行原连接检查、添加并检查模型，在 Chat 选择当前 Agent 模型，然后发送消息、展开 Completed steps 和 Read File 卡片检查真实文件输出，刷新后再次展开历史。所有写操作经原 UI；读取 API 仅用于确认持久化。Rust fixture 断言实际两次 Messages 请求的 key/model，以及全局 provider 没被选择操作覆盖。

浏览器检查发现 Chat 原有 `scope=agent` 选择此前只保存设置，推理仍使用全局模型。后端现按每个 Console turn 解析 Agent 有效模型，并向 Core 提供独立快照；工具步骤不再随全局配置变化而切换 provider/key。补充普通测试覆盖并发 Agent 与全局回退、删除 Agent 模型后的旧会话回退、推理中修改全局配置，以及无效 turn 设置不改变历史/全局状态。

并发测试还复现了非默认 Agent 直接首次聊天缺少内置分组而返回 422，已在创建聊天元数据时初始化该 Agent 的分组；测试不预先调用分组页面。全局回退额外检查 Core 运行时与应用 registry 不一致时仍使用当前 Core 配置，避免重用过时 URL/key。这不等于跨进程 registry 已自动同步。

```sh
cargo test -p qwenpaw-app-server --all-features anthropic_tests
# Requires Node 24+ and Chrome, plus the unchanged built Console.
cargo test -p qwenpaw-app-server --all-features original_anthropic_browser -- --ignored
cargo test -p qwenpaw-core --all-features invalid_turn_model_settings
```

本切片新增 4 个普通测试和 1 个浏览器门禁。2026-09-09 最终工作区 375/375 普通测试、6/6 原前端浏览器门禁通过；严格 Clippy、格式/diff、inventory 3/3 与快照校验通过，`console/src` 零改动。release Core 重建后，TypeScript SDK 3/3、Python SDK 4/4、VS Code 57/57 实际连接回归通过；TypeScript SDK/扩展编译通过。SDK 测试仍是通用协议/OpenAI 兼容 fixture，不冒充跨进程 Anthropic registry 继承测试。现有 QA DMG/ZIP/archives/SDK 包/VSIX/legacy wheel 未重建，不能标记为含本次修复。

SDK 协议结构未变，公开聊天事件仍是既有文本、工具和完成事件。单独启动的 SDK 进程不会自动继承 Desktop provider registry。后续 §14.2.24.30 已将本次修复纳入[新一组九类 QA 制品](qa-runtime-packages-20260909.md)，但包内 Core 原生执行及正式 macOS 签名/公证门禁仍未关闭。
