# TypeScript SDK 显式连接已有 Core

日期：2026-09-14。对应 [接入方案与 checklist](../architecture/sdk-existing-host-connection.md)。
本轮交付 Node SDK 连接态及服务端 WS Close 修复；不是所有客户端/分发运行完成。

## 实现与边界

- 新 `WebSocketConnection.connect()` 复用现有 AppServerClient 和 Thread API。
  使用流适配处理 WS 完整消息/pretty JSON，不复制请求关联或 Agent Runtime。
  `QwenPaw.start()`、VS Code 默认 stdio 和 Python 接入未切换。
- WS 仅 loopback IP，WSS 正常校验 CA/hostname。token 只在 Authorization，
  不进 URL/错误或连接对象 inspect。拒绝 URL query/fragment/userinfo；支持
  显式 CA，不支持跳过证书校验、重定向、自动发现、自动重连或重放。
- 建连总时长 15 秒，initialize 使用请求超时，消息上限 1 MiB，WS 写缓冲预算
  2 MiB。AbortSignal 仅取消建连和初始化，不控制后续宿主或任务。
- disconnect 立即关闭请求入口并唤醒 pending，重复调用共享结果；五秒超时报错
  并只终止此连接。dispose 立即断开，不停止 Core，不承诺后台保存完成。
- `ws@8.21.3` 是实际运行依赖，`@types/ws@8.18.1` 仅开发使用，均已锁定。
  安装关闭 scripts；不依赖可选 native accelerator，不增加 Python/Agent 依赖。

## 真实客户端揭示的服务端缺陷

第一轮 TS 全套 **24 通过/2 失败**。两项真实 Core 连接在 disconnect 时报告
`Core WebSocket closed abnormally`。服务端收到 Close 后直接 abort writer，
丢失 tungstenite 已排队的 Close 回复。

新增两个原始控制帧回归：idle Ping 原本通过；Close 原本失败，明确返回
`ResetWithoutClosingHandshake`。未将客户端改成忽略异常关闭。修复由 reader
通知 writer 停止业务输出并 flush 已排队回复，最多等待一秒，再按原逻辑 abort。
普通异常断连和宿主关闭仍走原分支，不取消已接收 Turn，不更改 Console SSE。
修复后四项服务端 WS/Rust SDK 专项全部通过，随后重建 source Core。

## 证据与交付

输出目录：[qa-ts-ws-sdk-20260914-YlVavq](../../../dist/qa-ts-ws-sdk-20260914-YlVavq)。
最终 [verification.json](../../../dist/qa-ts-ws-sdk-20260914-YlVavq/verification.json)
于 10:24:34 UTC 核验十五条成功命令、十四项来源摘要、实际安装载荷及
source Core/包摘要；所有本轮进程已终态，无遗留自有监听 Core。

| 项目 | 已确认结果 |
| --- | --- |
| TypeScript 源码全套 | 26/26，0 fail/skip/cancel；其中原有 10 项 + WS fixture 14 项 + 真实 Core 2 项 |
| TypeScript 隔离安装态 | 26/26；测试通过 symlink 导入实际安装包，21 个编译文件及 README 与源输出逐项一致 |
| Rust 服务端控制帧/共享连接专项 | 4/4；Close 红灯先复现，修复后通过 |
| Rust 全套 | 733 通过（含 2 doctest）、0 失败、27 ignored；不把浏览器/参考专项算作执行过 |
| 严格检查 | workspace/all-targets Clippy `-D warnings`、fmt check；Rust SDK 文档更新后追加 doctest/Clippy/library 重建 |
| 原前端 | 295 文件、2453/2453，72.55 秒；console/src 零 diff/零 status |
| 保持现有接入的 Python SDK | 17/17，33.216 秒，指向新 source Core；未实现 Python WS |
| 保持 stdio 的 VS Code | 编译及 73/73，未激活扩展/原生 UI |
| Core source release | 58.71 秒，SHA-256 `fafbeadf3275494099e3f342b65dcd901c919bb4425fe2795d1a2df0cd4f5cf1` |
| npm 包 | 18,454 bytes，SHA-256 `610c43e99af2fa9b8367e62456ab276ddbe94c53fdbf8905f270a421d2b198ec` |

新包：[qwenpaw-sdk-0.2.0.tgz](../../../dist/qa-ts-ws-sdk-20260914-YlVavq/qwenpaw-sdk-0.2.0.tgz)。
包声明运行时依赖 ws；隔离安装使用 npm 本地缓存 offline 解析依赖，并非包内
包含整个依赖树。没有发布到 registry，也没有覆盖旧批次的同名包。

真实 Core 测试使用独立临时目录、本地模型和假 key。WS 测试暂停实际模型，
关闭第一条连接后第二条仍读取完整相同 inProgress 状态，释放模型后 completed；
第三条新连接读取完整相同终态，模型配置不变。WSS 测试由 OpenSSL 生成临时
证书，默认不信任、自定义 CA 下 hostname 错误、缺失/错误 token 均拒绝；
正确连接关闭一条不影响另一条。测试最后由 fixture owner 终止/等待自有监听
进程，未把 SIGTERM 当作持久化确认，也未使用桌面模式或系统凭据库。

固定超时使用 Node mock timers；缓冲预算用实际 WS 对象 getter 注入满值，
关闭超时使用暂停读取的真实 WS peer。这不是 Windows/Linux TCP 缓冲实测。
所有执行在当前 macOS ARM64、Node 24.18.1 和 conda qwenpaw 上完成。

## 保留的失败记录

- 初次编译失败：readonly CA 数组的类型缩窄；修正复制分支，不取消 readonly 契约。
- 测试使用 `toReversed` 不符合 SDK ES2022 target；改为副本 reverse，不抬高
  SDK 最低 Node 版本。该次编译仍产生了 JS，早期 JS 测试不作为最终编译验收。
- 最初 raw HTTP upgrade fixture 未结束服务器侧半关闭 socket，after 清理挂起；
  确认自有 Node 测试 PID 后 SIGTERM 终止。fixture 改为等待客户端 EOF 后结束
  服务器半边，并在失败清理中跟踪/销毁自有 socket；重跑 12/12，再扩展测试。
- 真实 Core 两项异常 Close 及 Rust 红灯保留；修复服务端后才重新构建/测试。

## 仍未完成

Python 显式连接、自动共享发现/默认 Workspace、共享宿主退出选择、原生窗口
与扩展激活、各平台实机和全部旧功能继续开放。原前端源码没有改动；单元回归
不代替原界面端到端全交互验收。旧 DMG/ZIP/VSIX/Core tarball 尚不含本轮新
服务端，须后续重打并单独验收；未执行任何包内 Core 或绕过既有系统终止问题。
