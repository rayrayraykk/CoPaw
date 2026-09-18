# Codex 单客户端启动生命周期验收

2026-09-15；按 [Harness 方案](../architecture/harness-runtime-parity.md)，增加 `qwenpaw-harness::codex::lifecycle`。这是单个客户端的进程所有者，不是整个 workspace adapter，也未取代原 capability fingerprint 对应的多客户端/多会话映射。

## 已实现

- [x] `LaunchConfig` 持有已解析 binary、host cwd/base environment、runtime environment 和有序 config overrides；不实现 Debug，避免直接格式化凭据。生产命令为 `app-server`、每项 `-c <override>`、`--listen stdio://`，参数不经过 shell。
- [x] 环境先清空隐式继承，再显式合并 base/runtime，保留空值和同名覆盖；cwd/binary 要求绝对路径。发现结果及环境由上层提供，不自动扫描真实账号环境。
- [x] 16 项有界操作队列由独立任务顺序执行；并发 start 返回同一 generation。普通 RPC 仍走已有独立读写循环，不经过生命周期队列。
- [x] 相同配置不重启；不同配置回收旧进程后更新，下一次 start 才启动新进程。旧请求与订阅关闭，旧 client 不会偷偷指向新进程。
- [x] 已接受的启动/配置更新/shutdown 不因调用方取消等待而半途丢失。终态 shutdown 被所有者处理后关闭队列，拒绝后续/排队请求，再回收并返回结果；它不是抢占已经排在前面的操作。
- [x] 普通 stop 后可再启动；已观察到的异常退出先确认回收再创建新 generation。普通握手失败清理成功后可重新配置恢复。
- [x] 回调在首条协议读取前安装；原传输可在初始化早期处理服务端审批，不存在“先启动读循环，之后才补回调”的窗口。
- [x] 初始化错误与清理错误不再混淆：新增 `StartupCleanup` 保存后者；停止或启动清理失败会锁存，后续 start/configure 不会静默尝试新进程。

依据是本机原 `harnesses/codex/app_server.py::start/configure_runtime/stop`、`harnesses/runtime.py::adapter` 和 `codex/adapter.py::_prepare_runtime`。没有新增 Python 产品执行路径。

## 测试范围

12 个生命周期测试验证了完整启动参数/环境、12 个并发 start 去重、配置不变/改变、挂起请求与旧订阅关闭、取消启动、取消替换、取消终态 shutdown、正常停止后重启、异常退出后重启、首条审批与回调释放、普通握手失败恢复、强制停止错误锁存及 Drop 清理。

进程测试使用私有测试命令工厂，将 argv 换成当前 Cargo Rust 测试入口；并非真实 Codex 的 `app-server` 子命令。环境/工作目录、生命周期队列、初始化、双向管道和停止逻辑使用实际实现。生产命令的完整 program/argv/cwd/显式 env 另做结构断言；不能将工厂测试称为真实 CLI 验收。

取消替换通过 EOF gate 暂停旧 fixture，取消调用方后排入新 start；测试命令工厂在真正创建新进程前检查旧 finished 记录，之后才放行。该记录证明 fixture 已处理 EOF，不单独证明 OS 已回收；回收保证来自 `CodexProcess::shutdown` 对 child/supervisor/worker 的等待以及显式返回。Drop 测试只观察异步关闭，不能当作同步回收证明。

## 验证结果

| 检查 | 结果 |
| --- | --- |
| 生命周期专项 | 12 passed、0 failed、0 ignored，串行 13.39 秒 |
| 整个组件普通组 | 45 passed、0 failed、2 ignored，包含 1 个子进程 fixture 入口 |
| 带空格目录源码构建/执行 | 同组 45/0/2，21.68 秒 |
| 既有原 Python 显式对照 | 2/2 测试；18 组发现 + 13 组控制面，4.06 秒 |
| 全 workspace | 916 passed、0 failed、44 ignored，含 2 个 doc tests |
| 静态检查 | workspace/all-targets Clippy `-D warnings`、cargo fmt 通过 |

31 组对照验证既有发现/控制面，不是对新所有者执行 Python 生命周期对照。44 个默认忽略项中两项已如上显式运行，其他此前 42 项原 UI/参考验收未在本轮重跑。没有据此宣布完整原交互一致。

首次编译中测试的 `unwrap_err` 不必要地要求 client 实现 Debug，已改为检查取消错误，没有给含状态的客户端增加 Debug。Clippy 的嵌套条件警告已修正。失败日志保留，未放宽 lint、既有驱动或产品测试超时。

[最终校验](../../../dist/qa-harness-lifecycle-20260915-L5dact/verification.json) 于 `2026-09-14T23:55:46.797Z` 通过，包含命令终态、源码与制品哈希；日志保存在同目录。

## 剩余边界

沿用既有 Rust 传输的关闭 stdin、等待、必要时强制回收并报错；原 Python stop 使用 terminate，再超时 kill。这不是停止细节完全相同的声明，真实 Codex 和上层 UI 故障恢复仍需验收。清理故障锁存后没有隐式 reset；完整 adapter 必须处理诊断、恢复与用户提示，不能绕开此门禁直接丢弃所有者并宣称已安全重启。

handler 生命周期、capability fingerprint、多 session 共享/隔离、thread 恢复及跨 generation 重新订阅仍由上层实现。审批闭包应避免强捕获其所属 adapter/owner 的循环引用，接线时需验证销毁边界。队列等待时间不算 handshake timeout；原生/跨平台仍未验证。

本轮 Cargo.lock 与所有 Cargo.toml 未改，既有发现/控制面与 56 个脚本保持哈希一致；只增加两个生命周期文件，修改传输/fixture 文件。相对 NvQ0h0 的原 2940 个来源，仍只有前几轮新增 workspace 成员对应的 Cargo.toml/Cargo.lock 不同，其余来源及原 Console 不变。

九包及 source release Core 哈希不变，未重建且不包含当前 Harness 组件；没有运行真实 Codex/Qoder、读取真实凭据、执行包内 Core、启动原生窗口、激活 VS Code、commit 或 push。复制测试二进制的历史启动问题仍未确认根因。

- [ ] 完整 provider adapter、目录/状态、会话/能力映射和七个 HTTP 路由接线。
- [ ] Codex/Qoder Agent 聊天、审批、取消、附件、命令和历史恢复。
- [ ] 原 UI 完整对照、受影响九包重建并逐个测试。
- [ ] 真实账号、原生与跨平台验收；总 goal 未完成。
