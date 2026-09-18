# 插件后端运行时：实现前需确认的边界

状态：待确认；2026-09-15。延续“已有功能完整保留、原前端不改”的目标，不修改已确认的 D3，不将目录读取或安装按钮的成功响应当作插件可运行。

## 当前源码证据

本机只读审计遍历仓库 `plugins/**/plugin.json`：14 个 manifest 全部声明实际存在的 Python 后端文件。审计记录每份 manifest 和后端入口的 SHA-256，未导入插件、安装依赖、读取日常插件目录或调用网络。证据：`dist/qa-plugin-runtime-decision-20260915-0JypCL/audit.json`。范围仅限仓库，不代表外部市场全部插件。

| 目录组 | 插件 ID | Python 后端数量 |
| --- | --- | --- |
| apps | agent-kanban、qwenpaw-creator、qwenpaw-data | 3 |
| bundle | chrome、cloudpaw、computer-use、omp-workflows、qwenpaw-pet | 5 |
| channel | azure-bot | 1 |
| middleware-demo | middleware-demo-thinking-log、middleware-demo-tracing | 2 |
| tool | gpt-image2-tool、qwen-image-tool、wan27-tool | 3 |

原 `src/qwenpaw/plugins/loader.py::_load_backend_module` 使用 Python importlib 执行入口模块，并调用导出的 `plugin.register(api)` 或 `app.register(api)`。这不是只靠 JSON manifest 就能复现的声明式功能。

原 `src/qwenpaw/plugins/api.py` 允许注册 Python 工具、Provider、HTTP router、渠道、命令、生命周期与运行时 hook 等。原 `routers/plugins.py` 的安装/上传会加载代码、整合注册项并安排 Agent 重载；卸载还需清理注册项和 Agent 配置，并与同 ID 重装共用生命周期锁。直接拷贝/删除文件不能满足这些契约。

例如 Agent Kanban 后端依赖 FastAPI、PawApp context、chat 与 SSEChannel，包含真实任务、状态和流式输出；其 UI 加载成功不能证明任务可执行。

已批准方案 §13 D3 要求新版正常运行完全去 Python、不提供 sidecar fallback。早期风险表的“临时 sidecar”是历史建议，不能据此覆盖后来的 D3。

## 两条路线及影响

### A：维持纯 Rust，逐个重写现有插件后端

符合 D3，建议作为默认路线。原 Console 和插件前端保持不变，HTTP/SSE、工具配置和执行结果通过原实现对照。插件业务在新 Rust 运行时实现，不能让两个独立后端拥有同一份会话或运行状态。

需要明确：功能等价不等于任意旧 Python 插件 ZIP 原包兼容。已有 14 个插件需要逐个实现、测试并重新提供可运行的分发包；第三方 Python 代码没有通用的“直接转 Rust”加载方式。原包格式、版本与新运行时入口的最终设计需单独评审，不能用固定插件 ID 白名单声称完整动态插件生态。

### B：Rust Core + 可选 Python 插件兼容进程

Core、模型循环、会话与 SDK 仍由 Rust 持有，Python 仅作为插件执行宿主；这不是恢复整个旧 Python 后端。它可以成为保持旧 Python 插件源码兼容的路线，但仅增加一个子进程也不够：需桥接原 PluginApi/PawApp、生命周期、流式输出、取消、权限、Agent 配置和异常隔离。

这会改变 D3 的运行时与分发依赖约束，增加 Python/原生依赖的跨平台打包和维护成本，必须用户明确批准才能实施。不能承诺任意旧插件立即兼容。

## 确认后的实现与验收 checklist

- [x] 查明安装/上传/卸载不是磁盘 CRUD，核对原 loader、PluginApi 和后处理流程。
- [x] 审计仓库 14 个 Python 后端及入口文件，保留可复核清单与哈希。
- [ ] 用户确认 A 或 B，记录是否要求“旧 Python 插件原包直接安装运行”，而不只要求功能/交互等价。
- [ ] 完成选定运行时的设计：注册所有权、同 ID 生命周期锁、安装失败回滚、强制重装、重启恢复、取消/卸载清理及权限。
- [ ] 先完成一个真实插件纵向链路：原安装入口 → 真正运行 → 原 UI 操作和模型调用 → 重启 → 卸载；禁止虚构 loaded 状态。
- [ ] 按审计清单逐个覆盖其余插件，保留工具/路由/渠道/hook 行为，外部服务先用本地协议替身，真实账号验收另列。
- [ ] 原安装弹窗、本地目录、ZIP 上传、URL 安装、冲突/force、错误提示和卸载确认全过程浏览器对照。
- [ ] 更新各客户端分发包并逐个测试；原生和跨平台门禁不因本地单测通过而关闭。

本轮没有运行时、前端、包或依赖变更，未 commit/push。当前读取接口测试证据仍见 [读取验收](../testing/plugin-management-reads-acceptance.md)，不是插件执行验收。
