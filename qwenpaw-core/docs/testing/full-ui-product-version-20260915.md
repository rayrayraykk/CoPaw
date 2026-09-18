# xic9ei 原前端完整专项回归

后续：在不改原 UI、关闭判定或超时的前提下，新增可选关闭诊断并 [重跑全部 39 项通过](browser-shutdown-diagnostics-20260915.md)。下文保留本阶段真实失败，旧超时根因仍未确认。

延续已批准的原交互等价和逐项测试目标，补齐最新开发包的回归证据。当前源码有 39 个显式项：30 个浏览器场景、8 个原 Python 对照、1 个原产品版本文本校验；不能统称 39 个浏览器测试。此前包构建只执行版本/市场相关专项，本轮串行执行全部显式项，并重跑原 Console 全量单测。

## Checklist

- [x] 核对 xic9ei 的 2934 条构建来源、45 个额外脚本、39 个显式项清单、九类包和 source Core 哈希。
- [ ] 优化版 App Server 全部 38 个显式项通过。实际整组为 37 通过、1 失败，370.55 秒，退出 101；失败见下文，未重跑子集替代结果。
- [x] 优化版 CLI Debug 原页面 1/1 通过，14.71 秒，退出 0。
- [x] 原 Console 295 文件/2453 项单测全部通过，119.94 秒，退出 0；仅限制 worker 并发，不改前端用例或超时。
- [x] 汇总源码/脚本/包哈希与真实测试范围，保留全部日志和失败证据；通过门禁仍失败，没有改成绿色。
- [ ] 全部原功能、插件执行路线、历史偶发问题、包内 Core/原生/跨平台门禁继续开放。

证据目录：`dist/qa-full-ui-version-20260915-ZYKOaR`。使用 Conda qwenpaw、Node 24、离线锁定 Rust 依赖、本地协议替身及隔离 headless Chrome。凭据使用测试存储；不访问日常数据、真实账号/Keychain，不执行包内 Core、启动原生 Desktop 或激活 VS Code。此阶段不改变产品代码、原前端、现有驱动或已交付包。发生失败先诊断，不用仅重跑成功子集替代整体结果。

## 整组结果与失败定位

全部 39 个显式项实际执行完毕，合计 **38 通过、1 失败、0 ignored**。唯一失败项为 `desktop_market::tests::product_version::product_identity_preserves_original_plugin_compatibility_interaction`，错误为 `Browser did not exit after close request`，来自 `scripts/console_devtools.mjs::closeBrowser` 的 2000 ms 进程退出期限。

失败发生在新版本浏览器驱动调用 `closeBrowser` 时。按驱动顺序，此前标签、真实悬停/点击、警告/取消/重载、无安装请求和无页面异常等断言均已执行完；但浏览器未按期限完成干净退出，故整个测试仍失败，不能写成 UI 专项已通过。暂不能区分 Chrome 正常关闭耗时过长、测试资源状态或其他原因；不能把同期编译负载当作已证实根因。

只读检查及现有 DevTools 单测 13/13 通过，包含正常/异常退出、断连、协议错误及进程未退出时严格超时。没有延长超时、吞掉错误、强杀后报成功或改原 UI。测试结束后进程检查未发现对应版本浏览器驱动/隔离 profile 的残留进程。原 Console 2453 单测通过，其 jsdom 限制和预期错误边界堆栈完整保留。

原通过校验 `verify.mjs` 依旧要求整组零失败，并实际以退出 1 拒绝本轮结果。另用 [audit.json](../../../dist/qa-full-ui-version-20260915-ZYKOaR/audit.json) 记录失败事实、命令终态及来源一致性：`verified:false`、`evidenceVerified:true`、`testGatePassed:false`。源码、45 个额外脚本、九类包及 source Core 均未改变；没有用审计通过替代测试通过。

下一步优先补齐浏览器关闭的时序观测，区分协议确认、socket 关闭、主进程正常退出和故障清理；定位后再执行原整组门禁。关闭根因、历史 SDK EOF/焦点问题、插件后端路线、完整原功能及原生/跨平台仍开放。本轮不重建包、不 commit/push，总 goal 未完成。
