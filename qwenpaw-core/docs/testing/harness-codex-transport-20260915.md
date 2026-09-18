# Codex Harness 原生进程通信验收

2026-09-15；实现 [Harness 方案](../architecture/harness-runtime-parity.md) 的第一个 Rust 通信组件。新 `crates/qwenpaw-harness` 已加入 workspace，尚未被 App Server/Agent 执行链使用；不表示用户可以创建或运行 Codex/Qoder backend。

## 已实现与检查

- [x] 按原 QwenPaw clientInfo 完成 initialize → initialized；不套用 QwenPaw SDK 的 protocolVersion 校验。
- [x] 独立读写任务、请求 ID 关联与完整远端错误；Codex 整数 ID 范围、反向字符串/零/负数 ID 不与本地请求混淆。
- [x] 反向请求回调异步执行；同 ID 的审批可发起嵌套请求，读取循环仍能处理响应；无回调、错误及可捕获 panic 默认拒绝。
- [x] 多订阅者通知；慢消费者明确收到 lag，EOF 关闭订阅与等待请求；超时或取消清除本地等待项，不复用 ID。
- [x] 8 MiB 帧上限、256 个本地等待请求、64 个并行反向回调；不保留/打印子进程 stderr 内容。
- [x] 进程所有权与停止：关闭 stdin 后等待 5 秒，必要时终止并回收；强制停止返回错误，不冒称成功保存。Drop 通知持有 child 的 supervisor 清理；只有显式 shutdown 的返回才能用于证明回收完成。
- [x] 停止清理回调捕获的 client 引用，避免 self-cycle 保留旧连接。
- [x] 组件完整测试 17/17（16 个断言测试及 1 个供子进程执行的测试入口），0 失败/忽略，串行 8.33 秒。
- [x] 在 `build with spaces` 重新编译并直接执行生成的测试二进制，完整 17/17 通过，8.26 秒；涵盖带空格的可执行文件目录和 workspace，不依赖复制后的二进制。
- [x] 整个 workspace 888 passed、0 failed、42 ignored，含 2 个 doc tests；workspace/all-targets Clippy `-D warnings` 和格式检查通过。忽略的原浏览器/Python 对照不计为本轮通过。
- [ ] 真实 Codex CLI、Qoder 协议、发现/配置替换/登录、Agent 接线、turn 取消与历史恢复、原 UI、原生与跨平台仍未完成。

## 依据和范围

OpenAI Docs 技能用于核对 [官方 App Server 协议](https://learn.chatgpt.com/docs/app-server) 的双向 JSONL、初始化与反向审批。另读取本机参考仓库 `references/codex`（HEAD `633ab199cfd724aa78013c006b27a2b3d049fc3b`）的 `app-server/README.md` 和 `app-server-protocol/src/rpc.rs`，以及原 `src/qwenpaw/harnesses/codex/app_server.py`；未执行本机 Codex 或获取账号凭据。

测试子进程是本次 Cargo 编译的 Rust 测试入口，不是 Codex、包内 Core 或 Python sidecar。它记录隔离目录内的假协议消息，核对真实 stdin/stdout、初始化顺序、请求参数、停止及异常退出。命令由 caller 提供，Rust 不拼接 shell；可执行文件发现、环境/账号所有权、provider 业务方法仍需后续 adapter 实现。

本地 request future 的取消不等于远端 turn 已取消，后续必须执行并验证 `turn/interrupt`。有界通知的 lag 需要上层显式处理，不能在 Agent 接线时静默忽略。非 JSON 行按原实现容忍，但非法 JSON 对象形状/超限等防护不能被解释为任意版本完整协议兼容。仅证明本机 macOS ARM64，不推导 Windows/Linux 行为。

## 保留的失败与定位

首次 11 项整组为 10 通过/1 失败；随后两次串行整组出现 4、5 项子进程握手失败。原测试将运行中的测试可执行文件复制到新路径再启动。最小、不经过新传输层的直接管道测试也能复现超时；串行控制中原可执行文件通过，复制文件在超时点没有 started/received 记录或 stdout。另有一次传输诊断的 received 记录是在停止等待后读取，不能用它证明 deadline 前已收到 initialize。

目前只能确定该复制/启动路径的测试不稳定，不能断言 macOS 安全检查、代码签名或某个具体系统机制是根因；`copiedTestExecutableStartupRootCauseResolved` 仍为 false。没有绕过安全检查、重新签名或以成功子集覆盖失败。最终 fixture 使用 Cargo 原测试二进制；另在带空格路径从源码构建并重跑完整组验证路径，保留原失败日志及两组对照。没有修改产品安全策略或既有浏览器测试驱动/超时。

首次握手错误测试使用 100 ms，无法可靠区分新进程启动与协议错误；本轮新测试改用统一 3 秒，并核对 fixture 确实收到唯一 initialize。进程终止仍使用组件的 5 秒限制，强制终止测试必须得到 `StopTimeout`。这不是调整既有产品/浏览器测试超时。

Clippy 的文档、所有权及测试写法错误已修正，未关闭 lint。新增 crate 首次生成 lock 使用 `--offline`；此后 test/clippy 均 `--offline --locked`。移除唯一新增的本地 `qwenpaw-harness` package block 后，lock SHA-256 仍为先前的 `23202a34c2636b0b2d482191c4c9c17816d1df9f81b0645722d387ea5d0db5f2`，无其他依赖升级/降级。

## 制品边界

日志及命令终态：`dist/qa-harness-transport-20260915-fxcqtJ/`；[最终校验](../../../dist/qa-harness-transport-20260915-fxcqtJ/verification.json) 于 `2026-09-14T23:06:03.832Z` 通过。

原 2940 个构建来源中仅 workspace Cargo.toml / Cargo.lock 按新增成员发生变化，其余来源及 52 个既有脚本一致；新增四个 crate 文件单列哈希。原前端无改动。NvQ0h0 九包及 source release Core 保持原哈希，但它们不包含本切片，不能再把当前整个 workspace 宣称为完全对应旧 manifest 的来源。

没有重建九包、运行真实 Codex/Qoder、读真实账号或 Keychain、执行包内 Core、启动原生窗口、激活 VS Code 或 commit/push。下一步是 adapter/runtime 接线及对应完整验收，不是以这个底层组件代替整个 Harness 功能。总 goal 未完成。
