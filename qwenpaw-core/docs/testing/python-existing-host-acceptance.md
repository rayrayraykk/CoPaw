# Python SDK 显式连接已有 Core

日期：2026-09-14。对应 [接入 checklist](../architecture/sdk-existing-host-connection.md)。
本轮补齐 Python WS/WSS 连接态及 wheel，不表示全产品或原生客户端验收完成。

## 实现与兼容边界

`WebSocketConnection.connect()` 复用现有 AppServerClient 初始化、请求关联、
通知与 Thread API，只提取发送/初始化/响应等待复用点。默认 QwenPaw 仍拥有
stdio 子进程，原有 close/EOF/最终保存失败逻辑未替换。Python 不运行 Agent，
不读 Core 数据库，不启动/停止远端 Core，不写全局模型配置。

同步接口背后是独立 asyncio I/O 线程及协议 reader。唯一新增运行依赖为
`websockets==15.0.1`；显式关闭代理、压缩、重定向和库内敏感调试日志。
WS 只接受 loopback IP；WSS 校验 CA 和 hostname，可传 CA PEM，不能跳过验证。
token 仅放 Authorization，拒绝 URL userinfo/query/fragment；错误不反射凭据。
建连 15 秒、初始化默认 15 秒，cancel Event 仅取消建连/初始化；输入上限
1 MiB，传输写缓冲预算 2 MiB，不声称限制任意并发调用方的总内存。

disconnect 拒绝新请求、唤醒 pending、执行关闭握手，预算五秒，I/O 紧急清理
最多另加一秒。关闭回调独立派发，在剩余预算内等待；回调/reader 阻塞无法
强制杀 Python 线程，须报告清理未确认，重复 disconnect 保留失败。dispose
先停止本地网络，不做关闭握手，关闭回调最多等一秒；不等待已经执行中的
reader 回调。回调重入不等待自己。两者均不取消已接收 WS Turn，Console 原有
SSE 断连取消语义不变，也不把断连当作后台持久化确认。

## 验证与交付

输出目录：[qa-python-ws-sdk-20260914-RIX33b](../../../dist/qa-python-ws-sdk-20260914-RIX33b)。
命令终态、来源和制品摘要见 [verification.json](../../../dist/qa-python-ws-sdk-20260914-RIX33b/verification.json)。

| 项目 | 已确认结果 |
| --- | --- |
| Python 源码全套 | 37/37，0 skipped；原 stdio 17 项、WS fixture 17 项、真实 Core/跨语言 3 项 |
| wheel 隔离安装全套 | 37/37，0 skipped；`python -S` 禁用 site，SDK 和 websockets 均从独立 target 导入 |
| 实际包载荷 | 七个 SDK `.py` 与源码字节相同，包内 README 与源码相同；唯一 Requires-Dist 为 websockets==15.0.1 |
| Python 检查 | 新文件 Ruff check/format 通过；六个相关 Python 文件 79 列、f-string（docstring 除外）、Python 3.10 AST 语法检查通过 |
| Rust SDK 文档更新 | doctest 2/2、严格 Clippy 与 release library 重建通过；没有改 Rust 生产逻辑 |
| 原 Console | `console/src` 零 diff/零 status；本轮未重跑前端，上一 TS 切片 2453/2453 是历史基线 |
| source Core | 沿用上一切片 `fafbeadf3275494099e3f342b65dcd901c919bb4425fe2795d1a2df0cd4f5cf1`，未替换或运行包内 Core |

最终 wheel：[qwenpaw_sdk-0.2.0-py3-none-any.whl](../../../dist/qa-python-ws-sdk-20260914-RIX33b/final/qwenpaw_sdk-0.2.0-py3-none-any.whl)。
17,032 bytes，SHA-256 `940bdcc6ecc9148d990f1f641379a8b96cce8a8fd9ee77e833572bb0639384ff`。
首次候选 wheel 保留在上一层，不含最后的关闭回调修复，不作为最终交付。
依赖另行离线安装，wheel 不包含 Rust Core 或整个依赖树。用于此次安装的
websockets wheel 是 macOS ARM64 / CPython 3.12，不是跨平台安装验证。

真实 Core 用独立临时数据、本地模型、假 key 和临时 TLS 证书。两个 Python
连接共享 Thread；模型暂停时关闭第一个，第二个仍读到完整相同 inProgress；
释放后 completed，重连读取完整终态，配置保持不变。Thread facade 另跑一轮。
WSS 不可信 CA、hostname 错误、缺失/错误 token 均拒绝；两个正确连接互不影响。
混合测试由 Python 建 Thread，已安装 TypeScript SDK 读取并新建另一 Thread，
Python 继续读取完整相同状态；客户端退出不停止宿主。最后仅 fixture owner
终止/等待自己启动的 Core，不把 SIGTERM 当作保存证明。

## 保留的失败与未覆盖项

- 首轮异常帧/关闭竞态暴露原始 CancelledError；映射为不泄露负载的连接错误。
- 首次全套 34 项中写阻塞与 disconnect 竞态误报新的传输失败；修复本地主动
  关闭取消的判定，后续 dispose 竞态同样覆盖，未放宽错误帧或正常关闭断言。
- 关闭回调阻塞的红灯两个子例均超时；独立派发并有界等待后通过，增加回调
  重入测试。首次修复中 dispose 的 reader 取消误记为新错误，日志保留。
- 初版 lint/格式及早期候选 wheel 留存，不覆盖失败证据。

超时和写背压专项注入测试私有短时限/阻塞协程/缓冲测量值，不冒充各 OS TCP
缓冲实测。执行平台仅当前 macOS ARM64、conda qwenpaw / CPython 3.12、Node 24；
Python 3.10 仅语法检查，未运行其解释器。未使用实际 key、系统凭据或日常数据。

三语言完整能力一致、自动安全发现/默认 Workspace、宿主退出策略、原生桌面
与 VS Code 激活、Windows/Linux/macOS x64 实机及全部旧功能仍待验收。DMG/ZIP/
VSIX/Core tarball 未在本轮重打；TS README 更新尚未进入此前 npm 包。旧包不
自动代表最新源码。未 commit/push/发布，未绕过此前包内 Core 的系统终止问题。
