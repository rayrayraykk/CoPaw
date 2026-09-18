# SDK 显式连接已有 Core

日期：2026-09-14。沿用三语言 SDK 服务于同一 Rust App Server 的架构目标，
先补显式连接，不启动后台进程、不改变现有 stdio 默认行为。此步骤不依赖
尚待确认的“最后一个客户端退出后宿主是否继续运行”决策。

## 接入契约

Rust 先增加拥有**连接**的 `WebSocketConnection`，复用 AppServerClient 的
请求类型、关联和通知；不复制 Agent/工具/存储逻辑。连接后仍执行原协议
initialize/version 校验。TS/Python 已按同一连接契约接入；各语言测试通过
不代表全部能力对齐或所有原生客户端已消费新接口。

只接受显式端点：WSS 正常校验 TLS，可显式提供受信 CA；明文 WS 仅允许
loopback IP 字面量。token 仅放 Authorization，不放 URL、日志或 Debug
输出；拒绝 URL 用户信息/query/fragment 和非法 token。握手有固定超时，
不自动发现、不跟随端点变更、不降级 TLS、不重放业务请求。

`disconnect(self)` 只关闭此连接，不能发送宿主 shutdown、停止 Cron 或
终止其他客户端。已接收的 WS 任务按现有服务端契约继续；显式 turn/interrupt
仍可取消任务。断开不证明所有后台任务已持久化。关闭须有界、拒绝新请求并
唤醒 pending 请求；Drop/等待取消清理本地通信任务，不操作远端进程。

保留现有 `StdioAppServer::spawn/shutdown`、原 Console SSE 取消行为以及
用户显式模型配置。新连接不传入客户端模型 env/key，不写全局配置。

## 实施与验收清单

- [x] 核对现有 Rust 客户端与服务端 WS/WSS 行为及可复用依赖。
- [x] Rust SDK 显式连接/独立初始化、请求和通知、关闭/Drop 与超时实现。
- [x] 非 loopback 明文、带凭据 URL、非法 token、握手拒绝/挂起的失败测试；
  错误中不得泄露 token，默认 TLS 拒绝不受信证书，显式 CA 正常连接。
- [x] 两个真实连接共享同一个 Core；关闭一个不影响另一个，活跃任务按
  既有 WS 断连契约继续，配置/Thread 状态与调用前后完整核对。
- [x] 输出/输入异常、二进制消息、pending 请求、版本不符、握手取消与
  关闭背压收尾有回归，不让本地通信 worker 泄漏。
  背压使用真实 WebSocket sink + 有界内存字节流，不冒充不同 OS 的 TCP 缓冲实测。
- [x] Rust SDK 全套、严格 Clippy/fmt、真实服务端对照及源码依赖构建；更新
  README 和主计划，明确该结果不覆盖三语言/原生/跨平台/分发运行。
  Rust SDK 21/21、普通 workspace 729 通过/27 ignored、最终真实服务端 2/2、
  README doctest 2/2、release library 与严格检查通过，见
  [验收记录](../testing/rust-sdk-existing-host-acceptance.md)。
- [x] TypeScript 与 Python 显式连接态。
  - [x] TypeScript 增加 `WebSocketConnection.connect()`，复用现有
    AppServerClient/Thread API 的流适配；不修改 owned `QwenPaw.start()`。
    使用支持 Authorization/CA 的 Node WS 库，不能用不支持这些选项的浏览器
    WebSocket 代替远程安全契约。初始化失败、AbortSignal、异常帧/写背压有界
    清理；`disconnect()` 可重复等待，`dispose()` 仅终止本连接。
  - [x] TS 本地 WS/WSS/真实 source Core、原 SDK 回归；构建 npm 包并隔离安装
    验证实际导出/依赖，不将 mock 测试计作原生 VS Code 激活验收。
  - [x] 真实 TS 客户端发现 App Server 收到 Close 后立即 abort writer，未发出
    关闭回复；补原始 WS Close/Ping 回归并修复服务端控制帧 flush，关闭等待
    有界，不修改活跃任务/SSE/宿主生命周期；重建 source Core 后复验。
    TS 源码/安装态各 26/26、Rust 733 通过（含 2 doctest）/27 ignored、原前端
    2453/2453、原 Python 17/17、VS Code 73/73；见
    [TS 验收与新 npm 包](../testing/typescript-existing-host-acceptance.md)。
  - [x] Python 显式连接与 wheel 隔离安装验证。
    - [x] Python `WebSocketConnection.connect()` 复用 AppServerClient 的初始化、
      请求关联/通知和 Thread；只抽出发送消息、初始化与响应等待复用点，
      保留默认 Event.wait 和 stdio close。
      WS I/O 由独立 asyncio 线程执行，同步外观保持不变；不继承进程所有权。
    - [x] 使用现有环境的 websockets 15.0.1 并声明依赖，显式禁用代理、压缩、
      重定向和库内敏感调试日志；TLS 不允许跳过验证。可传 CA PEM 和建连
      cancel Event，不把进程 env/model/key 注入已有 Core。
    - [x] 覆盖握手/初始化失败与取消、消息限制、异常帧、并发请求与通知、
      重复/回调关闭及写阻塞清理；超时/线程清理未确认必须报错，不假报成功。
      单独复现用户 close handler 阻塞，验证 disconnect/dispose 均不无限等待；
      无法强制终止用户回调时报告清理未确认，重复关闭不得抹掉失败。
    - [x] 原 Python SDK 与真实 source Core WS/WSS 测试，79 列/英文注释/
      f-string 检查；构建 wheel 并隔离安装复验，更新三语言架构与未覆盖范围。
      源码/安装态各 37/37、0 skipped；含原 stdio 17 项、WS fixture 17 项及
      真实 Core/跨语言 3 项。Python 与已安装 TS SDK 共享一个 source Core；
      见 [Python 验收和最终 wheel](../testing/python-existing-host-acceptance.md)。
- [ ] 三语言完整能力一致、各现有客户端实际消费新连接 API。
- [ ] 桌面/VS Code/CLI/TUI 自动共享接入、安全发现和默认 Workspace；宿主
  生命周期选择确认后另行实施。

所有实际运行仅使用本地临时模型、测试证书/凭据和 source 构建，不触及
日常应用数据或系统证书库，不执行包内 Core，不 commit/push/发布。
