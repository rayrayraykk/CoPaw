# Gemini 原多模态探测验收

日期：2026-09-09。计划 §14.2.24.32。仅使用 loopback HTTP、临时目录、隔离 Chrome 和测试凭据。

## 实现与原版对应

`desktop_gemini_probe.rs` 由现有模型探测入口按 Gemini 协议分派。复用有界 HTTP/JSON transport、原生路径与认证，没有改变 OpenAI/Anthropic 探测流程。

| 行为 | 原版与 Rust 对应 |
| --- | --- |
| 图片 | 原 32×32 红色 PNG，经 `inlineData` 提交；同一颜色问题，20 个输出 token |
| 视频 | 原公共样例 URL 经 `fileData` 提交；询问是否包含运动内容，10 个输出 token |
| 独立执行 | 图片失败仍测视频；不是 OpenAI 的图片失败跳过视频逻辑 |
| 视频判定 | 按原 provider 检查小写回答中的 `yes`；不使用蓝色答案或格式重试 |
| 凭据 | 原生 `x-goog-api-key`；URL 不包含 key；公开诊断过滤 key 与自定义 header 值，包括回答被小写后的值 |
| 状态 | 更新 supports_image/video/multimodal 与 probe_source；重复探测和读回 registry 文件验证持久化 |

测试服务只接收样例 URL 字符串，不下载该视频、不读取用户媒体、不访问 Google。探测中的能力成功来自受控 fixture，不等于真实模型能力认证。

## 专项检查

- 两个新增普通测试覆盖完整图片/视频 payload、请求头、API 响应结构和文件持久化；重复探测从双模态变为图片，再变为纯文本。
- 图片 HTTP 400/401、被安全拒绝、只有 thought 文本、无效 JSON、无有效答案，均继续执行视频探测；每次恰好两次请求。视频 400 不重试。
- 故意回显的测试 key/header 不进入公开结果；先判定实际答案，再过滤公开诊断，避免 `[REDACTED]` 中的 `red` 被当作图片正确答案。
- 扩展原 Gemini 浏览器门禁：手动添加模型后，在原列表搜索、点击 Test Multimodal、看到 Multimodal 标签、刷新并重新打开模型管理确认标签保留，随后继续原 Chat 选择/工具/历史测试。
- 初次浏览器脚本仅等待页面时间戳变化，误在 `LOADING CONSOLE` 时点击 Cloud Providers；改为等待原页面完成加载后，专项通过。没有修改 Console 源码或用接口写入伪造探测结果。

## 回归与制品状态

Gemini 4/4 普通专项测试和扩展后的原页面浏览器专项通过。最终工作区 386/386 普通测试、7/7 显式浏览器门禁通过；后者包括两项备份、Market、Anthropic、Gemini、OAuth、OpenRouter，既有 Anthropic 场景仍保持原断言。Core/App Server/Storage/MCP 全目标全特性严格 Clippy、格式/diff、Console inventory 3/3 与快照通过，`console/src` 零改动。全部测试服务已结束。

源码 release Core 已重建；TypeScript SDK 3/3、Python SDK 4/4、VS Code 57/57 对新 Core 的既有回归通过。它们不是新增媒体输入的证明。本轮未重建九类 QA 制品；旧包 `dist/qa-runtime-20260908-yv7Wee/` 不包含本轮或上一轮 Gemini 实现，macOS 分发 Core/完整安装态门禁仍未关闭。

源码 release Core SHA-256：`bfd4a1b0c7ecea0ece72214c90e126d3526323425a5d848a71db4434bb5ef595`。该文件位于 `qwenpaw-core/target/release/qwenpaw-core`，不是已签名公证的分发制品。

## 实际聊天附件仍未完成

当前 App Protocol `UserInput` 只有 Text 和 FileReference；Console 的 image/video/audio/file 块在 `desktop_files::console_user_input` 被转换为 FileReference。Core `compose_user_input` 仅生成路径文本，并以字符串写入 `StoredMessage` 和 UserMessage item。需要继续实现媒体输入、受工作区/文件安全规则约束的读取、有界编码、跨 provider 请求、持久化/恢复和原聊天历史显示；不能通过模型能力标签将这些功能标记完成。

依据：原 `src/qwenpaw/providers/gemini_provider.py`、`multimodal_prober.py`，官方 [GenerateContent Part/Blob/FileData](https://ai.google.dev/api/generate-content)。当前官方通用图片/视频指南已大量采用另一 API 的示例，因此本切片仍以原产品使用的 GenerateContent REST 契约为准，不改变 API 体系。
