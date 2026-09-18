# 实际聊天图片输入本机验收

范围：计划 §14.2.24.33。保持 `console/src` 零改动，新增 Rust 图片输入、不可变小图片历史和三种原生请求格式。网络测试仅使用 loopback fixture 和假 key，不连接真实模型；这不是完整多模态或全部客户端安装态验收。

## 设计和协议依据

OpenAI Docs 用于核对 Chat Completions 的有序 `text/image_url` 数组和 data URI；Anthropic 使用 `image/source/base64`，Gemini 使用 `inlineData`，不通过 SDK 或 Python 转发推理。

- [OpenAI Images and vision](https://developers.openai.com/api/docs/guides/images-vision)
- [Anthropic Vision](https://platform.claude.com/docs/en/build-with-claude/vision)
- [Gemini image understanding](https://ai.google.dev/gemini-api/docs/image-understanding)
- [cap-fs-ext no-follow directory handles](https://docs.rs/cap-fs-ext/4.0.2/cap_fs_ext/trait.DirExt.html)

App Protocol v3 增加 `UserInput.image { path }` 和可选 `UserMessage.input`，只携带路径元数据。旧纯文本 Item 序列化结果不变。共享 schema、TypeScript SDK 与 VS Code 生成协议已同步；Python SDK 接受相同结构输入，不重复实现媒体读取。

Core 使用工作区相对路径（Windows 也用 `/`），拒绝越界、父路径、符号链接、非普通文件、File Guard 保护路径和不支持的文件头。capability 句柄逐目录 no-follow，普通文件采用有界非阻塞读取；文件读取期间大小/修改时间改变会拒绝本轮。只识别 PNG/JPEG/GIF/WebP 魔数，不宣称完整图片解码或每个模型支持所有这些格式。

保留原默认 2 MiB 每图片内联上限；小图片与有序文本一同写入 `StoredMessage.user_input`，工具迭代、checkpoint 和数据库重开使用同一快照。超大图片的模型请求为带大小的说明，Console 历史仍保留原上传引用。每轮最多 32 图片、16 MiB 原始快照；上下文另外限制 32 MiB 编码图片，既有文本预算继续生效。不会截断 base64，旧会话组仍可因上下文预算整体排除。

## 专项证据

| 层 | 已验证行为 |
| --- | --- |
| 协议 | 新图片输入、图片-only Item、旧 Item 读取与完全相同的序列化形状 |
| 媒体 | 中英文/空格文件名；有序文本/图片；内联边界、总量/数量拒绝；非法路径、伪图片、保护路径和文件/目录符号链接拒绝 |
| 模型 | 三种真实 request builder 生成原生图片内容，不泄漏 item ID、路径、快照字段；超大图使用各协议文字块 |
| Core | 图片-only 请求经 OpenAI HTTP fixture 完成工具往返；改写源文件后继续使用原字节；checkpoint 恢复、数据库重开和后续回合不丢图；输入验证失败不写入半条消息或更换 Thread 模型 |
| 上下文 | 图片有独立预算；最新轮不能容纳时失败，旧轮可整体排除；有序文字和图片不会被悄悄截断 |
| Console HTTP | 原 multipart 上传→Agent 模型选择→Gemini 原生工具往返→有序图片历史；文件改写不改变历史；非法图片 400 且历史不变、跨 Agent 历史 404；大图模型说明和原图预览同时保留 |
| 原浏览器 | 原模型配置/探测/选择，真实 file input 上传 PNG，缩略图加载和完成状态、发送、Read File 展开、刷新后图片解码及工具历史；两次模型请求均含相同图片 |
| TypeScript / Python SDK | 实际启动最新 release App Server，传图片路径、收到回复、验证薄事件结构；改写文件后第二轮远端请求仍含原快照 |

调试记录：原上传组件使用 `FileReader` 缩略图，测试已从错误的“必须请求预览 URL”断言改为原 `status-done` 图片加载断言；未修改前端。HTTP fixture 改为原时间戳格式的本地 session ID，并显式选择当前 Agent 模型，避免误用未知 Thread ID 或 Agent 已有模型配置。失败记录用于修正测试流程，不通过放宽后端契约绕过。

## 构建与回归

最新源码 release Core 已构建，SHA-256：

`af4cc2d9a3a6e7f88e16076a0a12bd75f745fbe2e42e90f484c0ca06ba50d392`

TypeScript SDK 4/4、qwenpaw conda Python SDK 5/5、VS Code 57/57 显式连接该 Core 并通过；两个新增 SDK 测试执行实际图片推理请求，VS Code 仍为已有协议/文件引用/审批/取消回归，不冒充原生图片 viewer 验收。TypeScript SDK 和扩展编译通过。严格 Clippy、fmt/diff、inventory 3/3 及快照校验通过；370 调用、38 未注册调用的清单未变。

最终 Rust 工作区 **396/396 普通测试**通过（新增 10 个；Core 94、App Server 190），另外 **7/7 原前端浏览器门禁**全部显式通过。最后一轮纠正媒体单测传入未规范化临时根路径的问题，并增加正常读取对照；Core 正式入口本来就在规范化后调用读取函数，未因此放宽保护规则。

本轮仅在 macOS arm64 实际执行；跨平台实现使用现成 capability 库，但不将本轮结果标记为 Windows/Linux 实机通过。Console 全套 2453 个单测及 production build 沿用此前相同前端源码的结果，本轮未重新运行该全量前端单测。

## 仍未完成

可配置 provider 内联上限、远程 URL、音频/视频/文档、高级图像选项、其他 provider 协议和非 Web 客户端图片展示均需继续验收。普通 file/audio/video 上传仍是路径引用，不能把图片支持称作所有媒体完成。

九类 QA 包尚未重建，旧 DMG/ZIP/VSIX 等不包含本轮图片功能。分发 Core 原生执行/签名和完整桌面安装态门禁仍保持未完成；不降低 macOS 安全策略。未提交推送，也不将总体“所有功能”目标标记完成。

后续更新：§34 已构建包含本功能的九类 QA 包，并完成解包后的图片请求、重启历史及 SDK/VSIX/WebUI 复测，见 [图片运行时 QA 制品](qa-image-packages-20260909.md)。上段记录的是本源码切片结束时的状态；当前仍未关闭首次启动稳定性、完整桌面 GUI 和生产发布门禁。
