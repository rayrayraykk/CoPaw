# SDK EOF 诊断与并发准备阶段验收

2026-09-15，macOS ARM64，conda qwenpaw / Node 24。对应 [方案与 checklist](../architecture/sdk-eof-race.md)。日志及摘要位于 `dist/qa-sdk-eof-20260915-bwshAx`。

## 实际改动

仅修改 SDK 测试，不修改 SDK/Core 生产逻辑。正常退出用例仍保留原 5 秒关闭超时，失败时增加子进程最后阶段、closing、worker/pending 状态。状态在 timeout 返回后读取，此时 owned shutdown future 的 Drop 可能已请求终止/abort；不能把它们当作超时前的精确现场快照。

新增一个普通测试：四个独立 current-thread runtime 各重建 16 次，总计 64 个独立临时目录和真实 Node 子进程；正常与非零退出各 32 次。每次实际握手、收到待响应请求通知、调用 shutdown，检查 TransportClosed、saved 阶段及完整 methods JSON。保留 2 秒通知和 5 秒关闭边界，没有永久串行化测试配置。

该压力测试单项首先通过（命令名 `runtime-churn-red` 不表示出现过红灯）。加入完整默认并发组后，既有四进程用例在准备屏障的 5 秒等待处失败，尚未进入 shutdown，见 `workspace-final.log` 的 `stdio_shutdown_tests.rs:148:22`。不能把这个失败描述为新捕获的 EOF 故障。

修正准备顺序：仍并发初始化四个子进程，但先收集所有初始化结果，再建立四方屏障同时发起关闭。保留四轮、每轮四进程、原初始化默认超时、屏障 5 秒和关闭 5 秒及原断言。准备和关闭计时不再混在一起；没有改生产调度或降低并发。

## 验证结果

- 最初仅加诊断：SDK 包内 21/21；workspace 特性组合下关闭组 6/6（第七个取消等待用例名不含该过滤串，未计入）；完整默认并发 workspace 846/846，33 ignored。这些在新增压力测试之前执行，不冒称最终源码结果。
- 独立运行时压力单项：1/1，13.22 秒，实际 64 个子进程；首次没有复现原 EOF 超时。
- `workspace-final`：App Server/HTTP 通过，SDK 21/22，准备屏障超时失败，记录保留。
- 修正夹具后的 `workspace-ready`：默认并发全 workspace **847 通过、0 失败、33 ignored**，命令 87.97 秒；SDK **22/22，32.21 秒**，包括原关闭用例和新增压力测试。真实 Core SDK 集成 **3/3**，验证初始化、关闭保存 interruption 及最终写入失败传播；stdio **4/4**。
- SDK 严格 Clippy（全目标、全特性、警告即错误）和 workspace fmt 检查通过。
- `verification.json` 于 2026-09-14T19:19:51.138Z 核对既有 2927 个输入及上一轮插件源码摘要，确认本轮没有生产源码变化。九个旧制品和 source release SHA-256 不变；记录两个 SDK 测试文件的最终 SHA-256。

## 尚未关闭的门禁

上一轮正常 EOF 5 秒超时的根因仍未证明。本轮修正的准备屏障问题发生在 shutdown 之前，不能追认它解释历史 EOF 失败；默认并发一次通过也不能宣称竞态彻底解决。源码诊断和 64 子进程覆盖为下次失败提供更明确的阶段边界。

本轮未重跑原前端 2453 项或 33 个显式页面/参考项：它们的最近结果在 [插件验收](frontend-plugin-loading-acceptance.md)，且相关生产源码、前端、脚本摘要未变。本轮未重建 release/九类包，未运行包内 Core、原生 Desktop、VS Code 激活或跨平台实机；旧 `6BiWlu` 包仍不含此前插件加载与工具校验的生产修复。总目标保持开放，后续仍需把已验证的生产改动纳入新开发快照，以及完成全部功能和客户端门禁。
