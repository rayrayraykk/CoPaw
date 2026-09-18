# 使用量 Workspace 归属与原统计范围验收

日期：2026-09-09；计划 §14.2.24.51。此处不是全功能或安装态完成声明。

## 实现和验收范围

- WorkspaceDataKey 由存储与 Core 共用，原 JSON 和 Agent ID 校验规则不变。SQLite 用量行使用嵌套 v2 信封；逻辑备份 v2 接受明确归属，旧 v1 无归属账本保留历史命名空间，不按当前同名 UUID 猜测。
- 可信宿主在 Turn 入场时固定事件发生时的 Agent 标签与 Workspace 数据标识；Console、Cron、默认 Heartbeat 使用经过校验的注册上下文。普通 Core 调用仍默认，不在客户端 wire 上开放内部身份字段。
- 对照原 Python `token_usage.py`、`agent_stats.py`、`agent_stats/service.py` 与 `get_agent_dirs()`：Token 汇总/详情全局；Agent 汇总的聊天按 Workspace 选择，但 Token overlay 全局；趋势遍历仍注册的 Workspace，包含禁用但未删除的注册，不扫描孤立聊天。前端源码不改。
- 范围导出按固定标识筛选；恢复显式映射来源到本机目标标识，历史 Agent 标签不重写。未选择账本保留完整结构，冲突 ID、无效绑定与格式混用拒绝。

## 测试及失败记录

- 首次旧 535 项普通回归在存储最后一组为 5/6：测试仍将现在支持的备份 v2 当成不支持版本。改为 v3，保留完整事务回滚断言。
- 单独编译存储/Core 发现 UUID serde 特性之前由 App Server 间接开启；在存储 crate 显式声明后，独立 Core **108/108**（2.60 秒）、存储 **9/9**（0.01 秒）通过。未改依赖版本，也没有用全 workspace 的特性合并掩盖独立构建问题。
- 新测试首次编译修正内部函数名、路由、宏导入及 Path 借用；没有扩大内部接口可见性。
- 新端到端测试前两轮失败均有具体 fixture 原因：自定义会话后缀含额外连字符，不符合原本地会话规则；默认 Agent 已保存独立初始模型；测试重开 token 不足 16 字节；未观察到缓存数据时的 eligible token 应为 0。修正模拟输入/配置和预期，不改变原接口或放宽校验。
- 新 App Server 用量定向 **5/5**（0.23 秒）通过：备份映射/冲突和旧格式两项；真实本机模型 SSE 的并发 Console、换注册/重开/删聊天两项；Cron 双 Agent 与默认 Heartbeat 一项。模型协议为 Anthropic/OpenAI-compatible，凭据仅隔离的假数据，无真实 key。
- 随后补充禁用但仍注册的趋势断言、现有跨 Core HTTP 范围恢复的完整账本映射/未选择保留/来源不变/重开断言；以下完整验证待执行。
- 增补后的首次 App Server 普通组 **319/320**（8.37 秒）：新禁用测试误用 POST，原路由为 PATCH，已修正。跨 Core HTTP 恢复断言通过。失败记录保留，不把该次执行改写为全通过。
- 严格检查指出新增代码使 Heartbeat 执行、两个测试函数超过 100 行，以及测试 JSON helper 未消费参数。按启动/重开/删除/等待职责拆分并使用明确的存储记录构造；没有关闭 lint 或削减断言。最终全部 target/feature Clippy `-D warnings` **9.11 秒**通过。

## 最新验证清单

- [x] 完整 Rust 工作区普通测试 **544/544**：App Server **320/320**（8.20 秒）、HTTP **36/36**（3.62 秒）、Core **108/108**（2.91 秒）、存储 **9/9**（0.01 秒），其他客户端/协议/MCP/工具测试通过。17 项显式场景不计入普通通过数。
- [x] 全 target/feature 严格 Clippy、fmt/diff、前端源码零 diff。
- [x] 在 conda qwenpaw 与 Node 24 中一次顺序执行全部 **17/17** 显式验收（270.66 秒，16 个原页面场景和 1 个调度参考测试）。调度参考内部 16 个日程匹配 APScheduler 3.11.3；不是分次补跑汇总为整组通过。历史偶发问题不因本次成功关闭。
- [x] 最新 Core release（52.47 秒）、TypeScript SDK 编译与 **4/4**（0.803 秒）、conda qwenpaw 中 Python SDK **5/5**（0.616 秒）、VS Code 编译与 **57/57**（3.011 秒）依次通过。Core SHA-256：`dde0c21f48f427e961eaaa5a4b024f66ae10bf2a304acfc9fcb2de518f547361`。最终 fmt/diff 通过，`console/src` 零 diff。
- [ ] 更新各端制品并进行安装态验证；不能用源码测试代替首次启动和原生 GUI。
  - [x] 九类 macOS QA 制品重新生成，校验和与原前端 1311 个文件逐一匹配；首次分发 22/24，同路径串行复验和安装的 Python SDK 后续通过，详见 [制品验收](qa-usage-packages-20260909.md)。首次 SIGKILL、完整 GUI、其他平台和剩余功能未关闭。

## 尚未完成

旧原生账本历史已写成 default 的记录无法凭当前注册反推真实 Agent。ACL、检查点存储目录代际、其余原功能、CLI/TUI 与全部平台安装态仍待完成；非默认公开 Cron 门禁保持。历史浏览器偶发问题和旧制品启动 SIGKILL 继续保留，成功的本机复验不关闭这些问题。
