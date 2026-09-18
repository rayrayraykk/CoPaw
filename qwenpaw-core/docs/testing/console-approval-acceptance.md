# Console 审批归属验收

日期：2026-09-09。承接原功能等价目标、计划 §39 与 [Cron 审批前置清单](../architecture/cron-runtime.md)。保持原 Console 源码、API 调用方式与 App Protocol 不变；不是将全局 Inbox 改成按当前 Agent 隔离的页面。

## 原交互与实现

原 Python `/console/push-messages` 始终汇总所有待审批，只有推送消息按 session 消费；原 Inbox/Chat 动作提交 request ID 和 root session ID，不切换 Agent。此前清单中按请求 Agent 限制审批的表述已修正：当前界面的 Agent header 不决定全局审批归属，不能据它拒绝 Inbox 的跨 Agent 操作。

Rust 现在从持久化聊天目录取得 Agent、session 和 root session；不信任业务 meta，不依赖重启后消失的反向 alias。既有无 Console 目录记录的 SDK/App Protocol 线程仅属 default。目录读取/验证失败时，由 Core 拒绝该工具，不发布一个冒充 default 的待审批。

全局轮询继续返回所有请求，`/api/approval/list` 使用同一 pending store 并支持根会话过滤：未提供/空参数返回全部，session 参数匹配 root 而非子会话。批准/拒绝仍绑定准确 request ID 和 root，错 root 不消费请求，过期/已消费请求不重复执行。列表形状包含 count、身份与工具展示字段；没有改成另一个客户端协议。

启动已有 Console 线程前，直接 ID 和 alias 命中都检查持久化归属，然后才允许修改 Workspace/执行。跨 Agent 请求不能仅凭线程 ID 使用不同模型/路径操作原线程。这个检查是 Agent 数据归属边界，不将 Agent header 宣称为不同登录用户的认证机制。

## 失败优先与定向验收

最初新增测试误把同步 `backup_snapshot(max_bytes)` 当成无参数 async 方法，编译失败；修正测试调用后，在旧生产实现实际复现 **3/3 失败**：

1. default 与 Writer 同名会话在重开后，审批均显示 default 且 session/root 退化成内部 thread ID。
2. Writer 传入 default 的线程 ID 被接受，能够进入 Workspace 变更分支。
3. 聊天目录损坏后仍生成默认归属待审批，没有拒绝真实工具。

修复后新增 **4 项普通测试**通过。双 Agent 测试确实启动原 Console HTTP SSE 和本地模型的 write_file 工具：错根会话 403 且整个 pending 列表不变；保持 default header 批准 Writer、保持 writer header 拒绝 default；只 Writer 工作区生成完整预期文件，default 无文件，两条 SSE 均 completed，重复批准返回 404，pending 清空。全局 push 与 list 的过滤/计数/完整字段另作结构比较。恶意 alias/直接 ID 检查整个 Core snapshot 不变且模型请求为零；无目录 SDK 线程保留默认访问。目录损坏测试观察真实 Turn 完成且工具未写文件。

新增原 Inbox 浏览器场景通过：从 Core 重开后的两条真实待审批开始，原页面显示 Writer 归属，重载后两张审批卡仍在；点击原 Deny 和 Approve，不切换当前 Agent，校验第一步只移除 default，第二步清空 Writer，最终真实文件结果与四次模型请求均符合预期。驱动 fetch 仅验证状态，动作由原按钮触发，没有伪造 API 成功或修改前端。首次四项普通测试与该浏览器联合 **5/5** 通过（12.27 秒）。

## 回归与 release

最终 conda qwenpaw 工作区普通测试 **452/452** 通过；严格 App Server Clippy 通过（11.42 秒）。初次新代码 lint 提示 let-else 和内部 struct 同后缀字段，按规范修正，没有添加 lint 豁免。一次 fmt 检查发现最后的列表测试格式需要重排，随后执行格式化；不将该首次检查记作通过。

release 构建通过（50.07 秒），Core SHA-256：`3c9275f533dba098ee220344eae2740375fb55991fc479dd8be89eb4fc137cdd`。显式连接该二进制的 TypeScript SDK **4/4**、conda Python SDK **5/5**（1.105 秒）、VS Code **57/57** 通过，无跳过，SDK/扩展编译通过。原前端源码零 diff，inventory 单测 **3/3** 且快照一致：370 个调用，仍有 38 处调用没有已注册 Rust 路由；注册路由总数现为 354。不能将此视为全部接口等价。

整组显式测试 **12/12** 通过（203.21 秒）：11 项原页面场景与 1 项原 APScheduler 差分。包括备份活动刷新/取消、24 页导航与备份恢复、新 Inbox 跨 Agent 审批、两类 Cron、Market、Anthropic/Gemini/OAuth/OpenRouter/Responses。原备份 roundtrip 此次完整通过，导出 ZIP 21376 字节；未复现此前 fetch 错误，所以此前间歇性失败根因仍未关闭。最终 fmt/diff 检查通过，前端源码仍为零 diff。

## 未完成边界

- 非默认 Cron 的归属赋值、真实执行、Agent 关闭/删除生命周期和 Job Copy 尚未接通；现有 Job 路由限制仍在。
- 持久/泛化审批规则、跨 Agent 子任务委派的 owner/executor 区分、多客户端完整路由与全部原控制语义未完成。
- 本切片未重建九类 QA 包，旧 WmuXFt DMG 不包含本次源码变化。原页面 Chrome 验收不等于 Tauri WKWebView 原生窗口验收；正式签名/公证、首次安装稳定性和各平台最新实机仍待完成。
- 仅使用本地模型、临时工作区与内存凭据 store；未访问日常 keychain、使用生产 key、提交/推送或发布制品。
