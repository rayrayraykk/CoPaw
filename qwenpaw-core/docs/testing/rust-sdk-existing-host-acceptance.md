# Rust SDK 显式 WS/WSS 接入验收

日期：2026-09-14。对应 [方案与 checklist](../architecture/sdk-existing-host-connection.md)。
这是共享 Core 接入的一个已实现切片，不是全功能或三语言验收完成。

## 本次实现

`WebSocketConnection::connect(endpoint, options)` 连接已经运行的 Core，
复用 `AppServerClient` 的类型、请求关联、initialize/version 校验及通知。
SDK 正常依赖没有 Agent/Core/工具/存储实现，不复制运行时。

`disconnect(self)` 只断开本客户端，拒绝所有 clone 的新请求并唤醒 pending；
五秒仍不能结束则报错，由 owner Drop 中止本地 worker。Drop 或取消关闭等待
也只清理本地连接，不杀进程，不发送宿主 shutdown，不改全局模型配置。
已有 WS 任务继续，显式 turn/interrupt 与原 Console SSE 取消行为保持原样。

明文 WS 限 literal loopback IP；远程 WSS 校验证书和 hostname，可显式指定
受信 CA，但不能关闭验证。token 只进敏感 Authorization header；拒绝 URL
userinfo/query/fragment 与非法 token。握手限 15 秒，消息限 1 MiB，不跟随
重定向、不降级 TLS、不自动重连或重放。握手/传输错误不带响应正文、header
或 token；业务响应仍是调用方应当保护的应用数据。

## 实测范围

本轮输出目录为
[qa-rust-ws-sdk-20260914-TNXscR](../../../dist/qa-rust-ws-sdk-20260914-TNXscR)。
最终 [verification.json](../../../dist/qa-rust-ws-sdk-20260914-TNXscR/verification.json)
于 10:01:17 UTC 核验八条成功命令、九个源码输入与 SDK release library 摘要，
确认 release Core 和原前端未改。正常依赖树只依赖协议层，不含 Core 实现。
使用 conda `qwenpaw`、Node 24.18.1、当前 macOS ARM64 的源码构建。
Cargo 全部设置 `CARGO_INCREMENTAL=0`，dev/test debug 信息均为 0。
所有模型、token、证书和数据库均为本地临时 fixture；不使用用户 key 或系统凭据库。

| 门禁 | 结果与范围 |
| --- | --- |
| Rust SDK 全套 | 21/21；原有 9 项 + 新 WS 12 项，包含真实 30 秒 stdio 关闭超时 |
| 普通 workspace | 729 通过、0 失败、27 ignored；不把未执行的浏览器/参考用例算作通过 |
| 最终真实 App Server 专项 | 2/2；在补充 hostname/错误 token 断言及测试辅助函数整理后重跑 |
| README 示例 | 2/2 doctest；编译并运行示例测试框架，不实际调用示例内的连接函数 |
| 严格检查 | workspace/all-targets Clippy `-D warnings` 与 fmt check |
| release | 单独构建 Rust SDK library；不是新桌面/Core 分发包 |

普通 workspace 执行后，仅新增了 WSS 测试断言、整理测试 helper，以及纳入
README doctest；最终专项、文档测试和严格检查覆盖这些后续改动。没有在运行
过旧测试后把未验证的生产行为计入结果。

WS 用例包含请求反序响应与通知、pretty JSON、Ping/Pong、错误版本不发送
initialized、二进制/非法 JSON/超大入站消息、超大出站消息不发送、Drop 唤醒
pending、握手超时/取消、302 不跟随且不泄露响应，以及关闭超时/取消收尾。
背压测试运行真实 WebSocket sink，但底层是 64 字节有界内存流；它证明 SDK
对阻塞写入的收尾，不证明各操作系统 TCP 缓冲实现或原生窗口行为。

真实服务端两组检查：

1. 自签证书默认不受信；指定 CA 后仍拒绝 hostname 不符；缺失/错误 token
   返回 401；正确连接分别 initialize，一条连接的初始化不授权另一条连接。
2. 两个 SDK 连接同一持久化 Core；模型请求被本地 fixture 暂停后关闭第一条
   连接，第二条仍读到完整相同 inProgress 状态。释放模型后正常 completed，
   协议 Thread 与 Core 完整结构一致，配置不变。两条都断开仍不停止宿主；
   测试最后显式关闭服务端并等待结束。

第二组是实际 AppServer/Core 对照，不等于默认 Workspace/Cron 接线已完成。

## 保留的失败与修复记录

- 首次测试编译失败：断言 `unwrap_err` 隐含要求连接对象实现 Debug。
  修改断言，不为含敏感连接状态的对象增加 Debug。
- 首轮超时 fixture 挂起：在真实网络建立前暂停 Tokio 时钟，自动推进可能
  先触发握手截止而让 accept 等待。确认自有测试 PID 后终止该进程，保留
  `unit-fixed` 的 SIGTERM/101；改为读完握手再暂停时间，后续 8 项及扩展测试通过。
- 严格检查曾发现不必要的 Value 按值传递、测试函数过长；改为借用和测试
  helper。纳入 README 后又发现标题缺 backticks，修正文档，不降低 lint。
- 首次汇总校验命令的 shell 引号错误导致 Node 语法失败、未生成验收 JSON；
  后续使用已有参数化 runner 传参，不把该失败当作成功报告。
- 汇总校验还曾把独立诊断项目的 383 个依赖错误当作整个 workspace 的依赖集；
  首次断言失败保留。改为逐项核对该诊断子集的 name/version/source/checksum，
  383 项全匹配；当前 workspace 共 435 项，其余 52 项不据此宣称做过前后比对。

## 交付边界与未完成项

- Rust 客户端源码和 README 是本次交付；原 `console/src` 零 diff、零 status。
- 现有 release Core `143799...` 未替换。App Server 对 SDK 的新增依赖仅用于
  测试，CLI 默认行为未改；本轮没有重打 DMG/ZIP/WebUI/VSIX/TS/Python 包。
- TS/Python 显式连接尚未实现；自动发现、默认 Workspace、模型凭据优先级和
  共享宿主退出规则继续开放，不能据此宣称多端自动共享已经完成。
- 没有执行包内 Core、启动原生桌面/扩展、实测 Windows/Linux/macOS x64，
  没有提交、推送或发布，也没有清理旧包/应用数据。
- 分发 Core 被系统终止的既有问题没有在本轮重试或绕过；父 goal 保持开放。
