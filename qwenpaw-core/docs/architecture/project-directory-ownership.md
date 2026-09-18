# 项目目录：Agent 归属与原页面等价

本切片属于已批准的原前端完整交互计划，不改 `console/src`，不导入旧 Python
产品数据。已有创建/导入/ZIP/克隆实现，不能因旧 inventory 写着 deferred
而重新实现一套。当前实码的缺口是四种写入和列表固定使用 default Workspace。

## 契约与实现顺序

原 Python `_projects_base(workspace.workspace_dir)` 总是当前 Agent 的基础
Workspace/coding_projects，与当前已选项目、线程项目和默认 Agent 无关。
创建/导入/ZIP/clone 成功后仅更新该 Agent 的 `project_dir`；列表的 active
标记、切换/重置，以及 SSE log/done/error 继续由原前端消费。

1. 先用两个 Agent、同名项目和独立 sentinel 文件复现写入/列表串目录。
2. 每次请求解析一个 `AgentContext`；项目根、staging、列表和默认项目均
   来自这份已验证基础 Workspace。等待项目锁后重新验证绑定，不把排队时
   捕获的旧 Agent ID 重新解释为同名新 Agent。
3. 异步 clone 固定最初数据标识和目录；完成发布在注册锁内核对身份，
   身份失效时返回 error，不能激活同名新 Agent 或改默认 preferred project。
   失败已经产生的用户项目文件不擅自删除；需明确失败与残留范围。
4. GET/list/browse 不混用两次注册快照；保持默认 Agent 与非默认 Agent
   原项目选择、清除、重开和原页面行为。
5. 原浏览器多 Agent 流程、原后端参考、安全/失败回归、全库与制品分层验收。

不引入新路由、前端替代交互或 Python 运行时。导入只测试自己创建的临时
源目录；Git clone 使用本地临时仓库，不访问远程 Git 服务或用户凭据。
ZIP/本地导入现有敏感目录、体积与链接防护必须保留。跨文件系统崩溃原子性
和外部进程替换目录不能仅凭 `AgentContext` 快照宣称解决。

## Checklist

- [x] 核对原路由、原前端 API、已有 Rust 实现及固定 default 根的实际缺陷。
- [x] 两个 Agent 的四种写入及列表失败回归。
- [x] 以固定 Agent 基础 Workspace 计算项目根与 staging；列表不跨 Agent。
- [x] 排队/异步完成发布时身份变更拒绝，不改同名新 Agent 或默认选择。
  覆盖排队的真实 SSE 和捕获旧身份的发布入口；未声称覆盖所有运行中 Git 时序。
- [x] GET/list/browse/PUT 单快照、切换/重置/重开及失败一致性。
- [x] 原项目选择页面的创建/文件夹 ZIP 导入/clone/切换/打开目录 Chromium
  交互与原后端 GET/PUT/list 参考；不等同于原生 WebKit 全场景通过。
- [x] 严格检查、全库/原前端/客户端回归和新制品静态/隔离安装逐项验收。
- [ ] 分发 Core 运行、原生 WebKit/Desktop/VS Code 激活及跨平台验收。

本轮专项 14/14 通过（包含 1 个浏览器和 1 个 Python 参考用例），
详见 [验收记录](../testing/project-directory-ownership-acceptance.md)。

本轮前的 `qa-runtime-20260910-ZPQQT0` 是已验收的 Debug QA 批次，尚不包含
本切片修复。包内启动、原生界面、跨平台和未授权清理限制继续保留。
本轮新批次 `qa-runtime-20260910-vGGX8Z` 已完成构建与分层检查，见
[制品验收](../testing/qa-project-packages-20260910.md)。不将这些检查升级为运行态证明。
