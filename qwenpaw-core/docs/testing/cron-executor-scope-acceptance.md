# Cron 执行归属与 Agent 停机验收

日期：2026-09-09。对应计划 §42。此切片验证原生执行器和原关闭/删除入口，不表示非默认 Job HTTP 与后台调度已开放，更不表示所有原功能或最新安装包已完成。

## 实现

入队从持久化 Job owners 获取真实 Agent，固定在 live lease 与运行注册中。prepare 的配置和模型来自该 Agent，模型复用同一次配置快照；Workspace、持久化会话、运行步数限制、trace 与 Inbox 使用同一归属。业务 meta.agent_id 不覆盖归属。运行和完成还检查内部任务归属，避免向另一归属提交状态。

原 Agent toggle/delete 在注册表写入成功后、仍持 Cron 锁时捕获并取消该 Agent 的 live runs。释放 Cron/Agent 锁后等待这些运行各自的完成令牌，10 秒内未排空返回明确错误；不等待随后启动的新 run。finish/审批清理完成后才释放 lease。运行中、同 Job 排队与等待共享会话的任务都被取消，其他 Agent 不受影响。删除保留 Workspace 文件和任务规格，不在本切片隐式删除用户数据。

文本任务仍保持原全局 push 形状和消费语义，未强加新 Agent 字段/过滤；其可选 Inbox 正确使用任务归属。全局审批继续由 request ID 和根会话响应，当前页面所选 Agent 不替代审批身份。

## 原模型语义与回归纠正

第一项新增并行回归先在旧执行器真实失败：Writer 的模型请求为 0 次而应为 2 次，实际误用了 default。第二项缺失 Provider 回归在旧模型解析真实失败：本应失败的 Writer 请求打到了默认模型。

初步把所有目录外模型都拒绝后，完整普通回归出现 **255 通过、1 失败**：旧 Rust Anthropic 测试要求移除 Agent 所选模型的目录项后回到全局模型。只读核对原 `agents/model_factory.py::create_model_and_formatter`、`providers/anthropic_provider.py::get_chat_model_instance`、`openai_provider.py` 和 `Provider.get_effective_generate_kwargs`，确认该测试并非原版行为：

- Agent 有完整显式 slot 时，原工厂选择其 Provider；Provider 不存在会报错。
- Provider 构造器直接使用配置的 model ID，不以目录成员资格为执行权限；目录移除不清除 Agent 配置。
- 模型不在目录时仍使用 Provider 级生成参数；null/空 slot 才使用全局模型。

因此保留显式 Provider/model/凭据，而不是为通过测试新增“删除后切全局”规则。原 Anthropic 测试改为验证目录移除后仍发送 agent-claude、agent-key 和对应自定义头，同时保持全局 Core 配置不变。新增 Cron 场景验证目录移除后仍请求 writer-model，主动设为空 slot 后才使用全局模型，Workspace 始终为 Writer。

读取当前模型也保留显式选择，防止原 UI 显示全局模型而执行另一模型。对于不在目录的模型，移植原 context_windows 的 32 项静态匹配和 Ollama opt-out；执行原 Python 纯模块，确认 32 项表完全一致，6 个未知/边界/Unicode/最长匹配案例与 Rust 测试一致。这是目录缺失时的显示元数据，不宣称完整上下文压缩和所有元数据优先级已迁移。

## 验证

本切片新增 11 项执行器普通测试及 1 项静态元数据测试：不同模型/Workspace/同名会话/同名公开 Job、真实工具副作用、trace/Inbox、全局审批、各 Agent 运行步数限制、缺失 Provider、禁用/已删除归属不回退、文本原形状、目录移除/空 slot、运行/排队/共享会话停机和取消完成令牌隔离。测试比较完整相关状态或消息结构，不以 HTTP 200 代替执行验证。

新增原 Agent 页面显式场景：三个真实 Agent 都有任务待审批，Writer 另有同 Job 排队任务；通过原表格按钮选择/禁用 Writer、删除 Editor，再刷新。验证选中项返回 default、Writer/Editor 对应任务取消并排空，默认审批和任务状态保持，Editor 文件/规格未删除。浏览器 fetch 只做读取验证，未代替修改动作。前置联合定向 **9/9** 通过（含该页面，12.91 秒）；新增后续普通场景后完整工作区 **480/480** 通过，App Server 260/260（7.11 秒）。

测试开发修正了历史存储顺序的断言、push 的原公开字段形状及 lint/import 问题，没有删除原安全断言。最终严格 App Server Clippy 通过（6.85 秒），fmt/diff 和 Node 驱动语法检查通过，console/src 零 diff。API inventory 3/3 与快照检查通过，仍为 370 项调用、38 项没有注册 Rust 路由。

最终显式整组 **15/15** 通过（249.83 秒）：14 项原页面场景和原 APScheduler 16 案例差分，含本次 Agent 停机、复制、全局审批、Cron 原控制、备份及 24 页导航。此次没有复现旧的导航 fetch 间歇性错误，不据本次通过关闭其根因。此前 252.76 秒的显式通过发生在模型目录语义最后纠正前，以本次最终整组为准。

最终源码 release 构建通过（54.18 秒），`target/release/qwenpaw-core` SHA-256 为 `3a07d06a5be8915b8847d18e190f5018a3bf82ff9c77b23504c23507b6fc1c93`。明确设置该 QWENPAW_CORE_BIN 后，TS SDK **4/4**、conda Python SDK **5/5**（0.619 秒）、VS Code **57/57** 与编译全部通过，无跳过。测试进程均正常退出；没有把这些源码构建描述为已重打安装包。

## 未完成与制品边界

非默认 Job HTTP 仍 501，后台仍跳过非默认任务。本切片通过内部入队启动 scoped fixture，并用真实原 Agent/审批接口停机；不能冒称用户已能通过 Cron 页面创建/调度非默认任务。

重新启用要按原调度器语义重建游标；删除保留数据后，还需解决重新注册同 ID/同 Workspace 与同 ID/其他 Workspace 的隔离，不能让新绑定继承旧任务。联合写入失败、并发生命周期代际、完整 HTTP/后台/外部投递以及各端原安装态仍待完成。原聊天/其他后台服务的完整 Agent 停机也不由本 Cron 测试代证。

所有模型调用只使用本地 fixture/假凭据，未使用生产 key 或日常 keychain；未提交/推送。旧九类 QA DMG/VSIX 未重建，不包含本切片。
