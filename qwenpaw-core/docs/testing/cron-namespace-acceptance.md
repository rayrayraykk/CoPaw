# Cron 任务命名空间验收

日期：2026-09-09。承接完整原功能目标、计划 §40 和 [Cron 命名空间设计](../architecture/cron-runtime.md)。本切片解决后续 Agent Copy 必须保留任务 ID 的前提，不将尚未接通的复制/非默认调度称为完成。

## 原契约与数据方案

原 `routers/agents.py::_copy_selected_workspace_files` 原样复制 jobs.json。`crons/models.py::JobsFile` 只有版本与任务规格；`repo/json_repo.py` 把历史另存在 jobs_history，复制逻辑不复制此目录。因此新 Agent 应保留任务 ID、规格和 enabled，但不继承源任务历史或活动执行声明。

此前 Rust v2 的全局 Job ID 同时充当存储键，不允许不同 Agent 保留同一 ID。本次新增内部 public_ids 映射：对外身份是 `(Agent, Job ID)`，内部唯一键继续关联状态、历史、游标、run claim 和运行并发。仅有差异时保存映射，映射格式使用 v3，旧仅支持 v1/v2 的 Core 必须拒绝读取。原生 v1/v2 不含映射的数据保持可读；不是导入 legacy Python 数据。

API 读取、编辑、启停、手动执行、状态/历史和删除在边界解析对外 ID；回包、trace、Inbox source/payload、稳定独立会话只用对外 ID。Job 请求里的 public_id/public_ids 不能覆盖内部映射，业务 meta 不能授予归属；内部缓存字段不进入 wire JSON。

范围备份同时筛选映射和全部关联记录。恢复时内部键碰撞可重分配新键，并重写所有关联键/claim 引用，保留逻辑 ID 和 Agent 归属；未选中数据不变。run ID 是独立 trace 身份，跨范围冲突仍在写入前拒绝。恢复后的 v3 数据不因只剩默认记录而向旧版本降级。

## 回归证据

先新增四项测试，在旧实现 **4/4 实际失败**：不接受支持同名任务的新格式、v2 忽略未知身份映射、范围备份不支持新格式、恢复拒绝可安全重分配的内部键碰撞。随后实现，四项全部通过。

新增六项普通测试覆盖：

- v1/v2 读取与编码完整结构不变；v3 不同 Agent 同一公开 ID 成功，同一 Agent 重复、悬空/空/控制字符映射及版本不匹配拒绝。
- 选择不同 Agent 的备份完整结构比较；恢复碰撞后 Job/owners/public_ids/状态/历史/时间槽/文本 in-flight/Agent claim 全部精确重键，原始输入不变。
- 两个同名 Job 的默认 HTTP 回包仍为原形状；原始内部键访问 404，伪造映射字段不起效，编辑、启停、文本手动运行和 History/Inbox 使用公开 ID，删除只移除 default 的内部记录；另一 Agent 完整记录不变。
- 映射后的 Agent 任务两次跨 Core 重开执行真实 write_file，复用同一公开独立会话，trace/Inbox 不包含内部 Job 键，状态/历史仍写入正确内部键。

原来要求跨 Agent Job 键一律冲突失败的测试已按原功能语义调整：保留 run ID 碰撞失败测试，改用新增完整结构重键测试验证 Job 碰撞；未知版本负例从 3 改为 4，没有删除安全检查或把失败简单改成成功。实际 ZIP 范围备份与联合恢复/回滚测试也使用 default/Writer 同一对外 ID 的 v3 数据，不只依靠纯数据函数验证。

Cron 定向普通测试 **47/47** 通过（2.29 秒，四项显式测试按原配置跳过）。后续完整 conda qwenpaw 工作区普通测试 **458/458** 通过，严格 App Server Clippy 通过。新增原 Cron 页面映射任务场景独立 **1/1** 通过（13.91 秒）：从持久化已有任务开始，通过原按钮启停、真实模型/工具执行、History、编辑、刷新、删除；未创建另一套 UI，也未通过 fetch 代替修改动作。删除后另一归属同名任务的完整存储结构未变。此场景不冒称测试了新建；原新建场景继续保留。

随后把命名空间 HTTP/浏览器 fixture 补充为实际注册 Writer Agent，而不是只有存储 owner 字段；HTTP 独立复测 1/1 通过（0.19 秒），该浏览器在后述整组中通过。最终严格 Clippy 再次通过（4.15 秒）。前端 `console/src` 零 diff，fmt/diff、驱动 Node 语法与 inventory 3/3/快照检查通过。

## Release 与范围

新 release 构建通过（49.17 秒），SHA-256：`0e157da62d123246b820ada6bb636dd1dcc7170029f863c69106e49e5775f619`。明确设置该 QWENPAW_CORE_BIN 后，TS SDK 4/4、conda Python SDK 5/5（1.040 秒）、VS Code 57/57 及编译通过，无跳过。

最终整组显式测试 **13/13** 通过（215.27 秒）：12 项原页面场景与原 APScheduler 差分，包括新增实际注册 Writer 的同名任务控制、原 text/Agent 创建控制、Inbox 跨 Agent 审批、备份活动刷新/取消及 24 页导航/恢复、Market 与五类模型场景。此次备份 roundtrip 导出 ZIP 21372 字节，全部恢复断言通过；没有复现以前的导航 fetch 间歇性失败，因此并未定位或关闭该根因。测试服务退出，fmt/diff 与前端零差异检查通过。

Agent Copy 仍复制非权威 jobs.json，尚未接入原生任务复制和联合失败回滚；非默认 Job HTTP 仍为 501，非默认执行/Agent 关闭删除生命周期、外部渠道和完整原控制语义仍待完成。不能把内部 namespace 支持当作多 Agent 调度已可用。

本切片未重建九类 QA 制品，旧 DMG/VSIX 不包含本轮变化；完整原生窗口、首次安装稳定性、生产签名/公证和最新跨平台实机仍未完成。仅使用本地 fixture 与临时数据，未使用生产 key、访问日常凭据或提交/推送。
