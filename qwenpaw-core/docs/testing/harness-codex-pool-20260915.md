# Codex 指纹客户端池本地验收

2026-09-15；对应 [架构与 checklist](../architecture/harness-runtime-parity.md)。本轮完成 Provider 内部的配置投影→按能力隔离的进程→技能根目录初始化与会话绑定，不代表完整 Harness/Agent/原页面已接线。

## 实现范围

- 新增 `codex/runtime.rs`，池与 Provider 控制连接分离，按有效能力 fingerprint 复用所有者。同会话同指纹/同 generation 跳过重复 roots；其他会话共享该进程但重新设置原 roots 请求。
- 配置变化创建独立进程，旧配置继续服务其他会话；切回可复用。已刷新的凭据 revision 参与指纹，显示名称不影响复用。完整能力 resolver 尚未实现，不能用陈旧 revision 自动感知凭据变化。
- 成功完成 `skills/extraRoots/set` 才更新会话绑定。失败或超时保留旧绑定，可在原所有者上重试；启动了新 generation 时重新初始化 roots。
- prepare/forget/stop/shutdown 使用有界命令队列，已接受操作不因调用方取消等待而被中断。forget 不删持久 thread。没有添加驱逐活跃会话的 LRU 或进程数量上限。
- Provider stop/shutdown 同时等待控制连接、独立 MCP 发现和池；池尝试所有进程回收后锁存首个错误。Drop 触发异步清理，只有显式 shutdown 成功返回证明回收完成。

依据原 `src/qwenpaw/harnesses/codex/adapter.py::_prepare_runtime`、`stop`。正常复用/绑定顺序保持原契约；串行化、重启补 roots、停止失败后继续回收其他所有者是明确强化。池内停止按指纹排序，不宣称异常时序和原字典遍历/首错中止完全相同。超时分别作用于握手和 RPC，不包含排队/文件发现的总截止时间。

## 验证结果

| 检查 | 结果 |
| --- | --- |
| 新增池测试 | 12/12，通过当前 Rust 测试二进制子进程 |
| 新增 Provider 测试 | 1/1；另扩展缺失 runtime、MCP+控制面统一停止测试 |
| Harness 普通组件 | 96 passed / 0 failed / 6 ignored |
| 带空格目录独立构建后运行 | 96 passed / 0 failed / 6 ignored |
| 既有 Python 显式参考 | 6/6，无跳过；控制/发现/完整状态/Skills/MCP/投影 |
| 整个 Cargo workspace | 967 passed / 0 failed / 48 ignored，包含 2 个 doctest |
| workspace fmt / 严格 Clippy | 全部通过，无新增 lint suppression |

新增测试覆盖同配置跨会话复用、切换/切回、forget、12 个并发 prepare、投影参数和真实子进程环境、roots 失败/超时重试、进程退出恢复、取消 prepare/shutdown 等待、空能力、独立池隔离、stop/restart、关闭克隆入口、Drop 和清理故障锁存。清理故障通过提前关闭生命周期注入，不是复现真实 OS 回收失败。

池的参数/行为是按原源码建立 Rust fixture 完整断言；本轮没有新增执行原 `_prepare_runtime` 的跨语言参考测试，不能把既有六项算法/控制/发现参考算作完整 adapter 对照。fixture 不运行真实 Codex/MCP 服务，不访问真实账号或技能文件。既有 MCP 测试仍使用 conda qwenpaw 的标准库 Python 子进程替身；不进入 Rust 产品。

## 可复核证据与边界

日志位于仓库忽略目录 `dist/qa-harness-pool-20260915-ttSo6T/`：逐命令 `.log`、`command-*.json`，以及来源核对脚本 `verify.mjs` 和结果 `verification.json`。最终核对时间 `2026-09-15T01:29:48.576Z`；workspace 测试于 `01:27:32.612Z` 开始、`01:29:32.964Z` 完成，退出码 0。

- 与上一阶段相比仅新增池模块及其测试，调整 Codex 错误/模块导出、Provider 及相关 fixture/tests；Cargo manifests/lock 不变，未升级依赖。当前核对 33 个 Harness/脚本/清单输入。
- 原 2940 个构建来源（去除先前 Harness 注册差异后）、61 个现有脚本、原 Python Provider/投影来源均核对一致。`console/src` 没有修改；本轮未重新运行浏览器验收。
- 原 `NvQ0h0` 九包和 source release 哈希不变，仍是尚未包含 Harness 的旧开发快照；本轮未重建包、commit 或 push。
- thread/start/resume、持久化、聊天/命令/审批/取消、完整 resolver、Qoder、七个 HTTP 路由和 Agent backend 接线仍未完成。
- 原生独立 fixture 的 dyld 启动问题根因未确认；真实 Codex、原生 Desktop、VS Code 激活和 Windows/Linux/macOS x64 运行验收仍开放。没有修改安全策略或借用日常凭据。

下一步接入 generation 感知的 thread/session 映射与恢复，再组合完整 adapter；不得将内存会话准备完成当作原聊天功能可用。整体 goal 保持进行中。
