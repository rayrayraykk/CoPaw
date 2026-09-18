# Codex 会话驱动的 turn 协议验收

对应 [架构与 checklist](../architecture/harness-runtime-parity.md)，2026-09-17。这里验证原始 Codex turn 协议，不是原 Chat 页面完整验收。

`CodexSessions.start_turn` 先完成持久会话准备，再建立 `CodexTurn` 所有者。保持原 prompt、localImage、文件文本输入以及模型、推理、summary、审批和 sandbox 参数。通知订阅发生在 turn/start 之前，回复到达前的事件保留在队列中；按 thread/turn ID 过滤，完整保留工具与推理通知供下一层转换。

取消或 Drop 请求中断；取消发生在 start 回复前时，所有者仍等待回复中的 ID，再请求 turn/interrupt。`finish` 才能确认 worker 结果；`next` 返回 EOF 不代表成功。正常远端终态保留完整通知（含 failed 状态），interrupt 的结果仅为应答已收到，不宣称执行状态已完全落盘。订阅落后先尝试 interrupt，再报告通知丢失；若中断本身失败则返回该错误。

## 当前证据

- 新增 10 项普通测试，组件 118 passed / 0 failed / 7 ignored；带空格目录同组通过。
- 七项既有显式 Python 参考全部通过；本轮没有新增执行原 `run_turn` 的跨语言参考。原参数/过滤行为依据本地原 adapter 源码做断言，不能泛化成原 adapter 全部行为等价。
- 新测试覆盖提前到达的事件、跨 thread/turn 过滤、完整参数、取消前后时序、Drop、中断失败、EOF、缺失 ID、保留远端 failed 状态、队列满时取消与订阅丢失。后两项使用内存协议 peer/通知注入；其他流式测试使用当前 Rust 测试二进制子进程，不运行真实 Codex。
- 严格 workspace Clippy 与格式检查通过，未新增 lint suppression。
- 首次完整 workspace **失败**：SDK 的 `independent_current_thread_runtimes_drain_owned_children_while_peers_restart`，worker 1 / round 0 / mode 7 在 5 秒等待后仍为 `fixture/wait` 阶段。SDK 普通组当次 21 通过、1 失败，后续 workspace 项未全部执行。
- 单独构建的定点复测通过（13.43 秒）；对首次失败的同一个 `qwenpaw_app_server_client-65c76dd62db30736` 测试二进制定点复测也通过（13.53 秒）。未更改 SDK 源码、断言或超时；根因尚未确认，不作为已修复。
- 完整工作区按原参数复跑通过：989 passed / 0 failed / 49 ignored（包含 2 个 doctest）。`workspace-tests-rerun` 于 `07:55:32.453Z` 开始、`07:57:31.862Z` 退出 0；不是仅重跑成功子集。首次失败保留，根因仍开放。

日志保留在仓库忽略目录 `dist/qa-harness-turn-20260917-jr7sQx/`。`workspace-tests.log` 保留首次失败，`sdk-runtime-focused.log` 和 `sdk-runtime-original-image.log` 分别记录两次定点检查；不得覆盖失败或只保留成功子集。

最终来源核验为 `2026-09-17T07:57:49.618Z`，`verification.json` 同时记录首次失败与完整复跑。核对 2940 个原构建来源、63 个既有脚本、40 个当前 Harness/脚本/清单输入，以及原 Python 源码、原前端、旧九包与 source release 哈希；只有本轮 Codex 模块导出/错误、session 的 turn 入口、测试 fixture 与新 turn 模块发生预期变化。Cargo manifests/lock 不变，SDK 生产源码和关闭断言不变。

## 未关闭的门禁

原 HarnessEvent/Chat 响应转换、工具展示、审批上下文、同会话并发 turn/reset/provider 替换互斥、HTTP/Agent backend 与 Qoder 均未接入。附件文件访问权限由未来入口验证，此层只投影路径，不读取附件。start 超时或缺失 ID 时远端状态可能不确定，未实现无 ID turn 的完整恢复。未证明任意非规范 Python 容器字符串化、真实账号、跨平台、原生 Desktop/VS Code 激活或新包运行。

这轮没有改原前端、升级依赖、重建安装包、commit 或 push。旧九包仍不包含新 Harness；整体 goal 未完成。
