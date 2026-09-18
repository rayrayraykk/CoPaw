# 聊天 Workspace 归属验收

日期：2026-09-09；计划 §14.2.24.49，设计见 [Workspace 数据身份](../architecture/workspace-data-identity.md)。

## 实现与验证范围

- ChatCatalog v2 保存聊天/分组的类型化归属；API 不暴露新内部字段。原生 v1 默认与历史绑定兼容，不根据同名新注册或项目路径猜测归属。
- 原目录 Writer 删除后注册为 Editor，另建 Writer 使用新目录：原聊天完整响应、分组和 Cron 共享会话保留；新 Writer 的列表为空，直接读写删除和分组修改无法访问原数据。重启后 session 解析、审批的当前 Agent 与历史 root session 分别验证。
- 未登记 Console 元数据的 SDK Thread 即使使用相同项目路径，也只由默认运行环境接回。停止、文件项目和工具控制共用持久归属；实际待审批聊天不能由另一 Workspace 停止，完整 Core 快照和 pending 记录保持，合法停止保留原 `canceled` SSE 状态。
- 持久别名按原 `ChatManager.get_chat_id_by_session` 的最近更新时间选择，在缓存指向较早候选、Core 重开后仍一致。没有引入新的歧义报错或自动重发。
- 新 Console Thread 创建和目录发布持有聊天锁；注入 SQLite 元数据写入失败，Thread 回滚且不发布别名，整个 Core 快照保持。
- 新格式校验拒绝缺失/nil 标识、同一归属的重复分组和向旧版本夹带标识。相同历史来源名可在不同 Workspace 下存在；旧非默认聊天没有历史绑定时保留但不能由同名 UUID 注册领取。
- 范围导出/恢复重映射源与本机目标标识，真实两套 Core 的 HTTP 恢复增加聊天与分组完整结构断言；未选中对象和源 Core 不变。内部 ID 与未选中聊天冲突拒绝，空范围保留原字节。
- 检查点会话筛选不再按项目路径单独授权；同项目的另一 Workspace 会话由默认 HTTP 拒绝，其真实自动检查点仍可创建，并继续验证默认范围归档不携带该记录。

## 本轮结果与失败记录

- 新增 8 项普通测试。补充创建发布顺序和失败回滚后的最新完整工作区 **528/528**：App Server **308/308**（7.86 秒）、HTTP **36/36**（6.44 秒）；旧普通测试没有跳过。
- 首轮 296/300：三个旧原生逻辑合并测试的预期缺少新绑定/版本，一个范围导出测试仍预期导出未选中默认占位组。更新为完整 v2 结构，同时新增专门旧格式测试，没有删除未选中数据断言。
- 新取消测试初次期待内部 `interrupted` 字符串，但原 SSE 将其映射为 `canceled`；对照原取消集成测试后，保留接口行为并校正断言。新格式校验调整为先验证聊天标识，再检查分组引用。
- 最新严格 Clippy 通过（4.91 秒）；最终 fmt/diff 通过，前端业务源码保持零 diff。
- 原页面初轮 **14/15**（273.94 秒）：Anthropic 切到聊天后没有发出 POST，API 无失败，最终字数 0，原仅记录 sender 事件的数组为空。这与此前字数 27 的现场不同，不能将两者都归因于 IME 或服务端。诊断增加全部目标的有界 focusin/focusout、输入/键事件，以及原输入节点是否仍挂载/持有焦点；不记录输入内容，不自动重发、不加固定等待，不改前端。偶发根因仍未关闭。
- 新增原分组页显式测试单独 **1/1**（14.38 秒）：实际点击创建、重命名、置顶、刷新与确认删除，最终完整聊天目录与操作前相同，另一 Workspace 数据保持。完整显式组现在为 16 项，正在顺序复验。
- 新分组测试所在完整组 **15/16**（259.96 秒）发现验收脚本的异步断言错误：删除 API 已完成，脚本立即断言 DOM 节点消失，赶在 React 更新/退出动画之前。改为等待原节点实际消失（仍保留完整目录断言），没有改前端、延长超时或添加固定休眠。该轮失败保留，修正后重跑完整组。该组 Anthropic 成功样本显示输入与 Enter 均落在原已挂载 textarea，无 composition 事件；不据此关闭偶发故障。
- 后续完整组 **15/16**（256.55 秒）中，分组场景通过，但备份 roundtrip 的导航矩阵在 `/inbox` 出现取消请求和 console error；诊断可见 `/api/mail-access-control/pending/all`、`canceled: true`、`net::ERR_ABORTED` 与 `/inbox` document/context，失败 HTTP 状态列表为空。备份自身创建/导入/恢复/删除步骤通过。导航故障与先前 [Cron 验收的 fetch 问题](cron-runtime-acceptance.md) 分开保留现场，当前证据不足以确认是离页取消、请求生命周期还是环境时序。没有放宽 console error 的失败判定。
- 焦点诊断的成功样本显示 `originalActive: true` 时 `documentFocused: false`，到后续 focusin 才变为 true。驱动增加隔离 headless 页的 `Page.bringToFront` 和 `document.hasFocus()` 前置检查，不改 DOM/前端状态、不重发消息、不增加固定睡眠。它修正真实观察到的键盘输入前置条件，尚不能证明空事件或字数 27 的历史失败都由这一原因造成，偶发根因继续保持未关闭。
- 修正验收脚本后，最新完整显式组 **16/16**（258.02 秒）通过，包含新增原聊天分组控件、24 页导航、备份全流程、原任务/审批/复制/启停、模型聊天与调度参考对照。Anthropic 样本在 focus 和输入前均为 `documentFocused: true`。本结果不关闭历史输入或导航故障。
- `cargo build --locked --release -p qwenpaw-cli` 通过（50.41 秒）；本批 Core SHA-256：`e9361fcfdc4dd1a0551dc047060ddabcf42d9ceabe32c6f6f2d9cd0b266d0445`。
- 使用上述 release 顺序复验：TypeScript SDK 编译与 **4/4**（1.037 秒），conda `qwenpaw` 中 Python SDK **5/5**（0.599 秒），VS Code 插件编译与 **57/57**（0.196 秒）。这些是源码客户端/真实 Core 协议测试，不代替安装包首次启动或原生窗口验收。

## 未完成边界

普通 Console 运行跨关闭/删除的取消与固定执行身份、使用量/ACL 等归属、检查点目录代际/图/GC 和完整多 Agent 路由仍需继续验收，非默认公开 Cron 门禁没有解除。

本轮不重建九类旧候选包，不以源码测试代证安装态。旧包首次 SIGKILL、原 Anthropic 偶发输入未发送、最新 Windows/Linux 实机、完整原生桌面/VS Code 窗口与生产签名仍未完成。
