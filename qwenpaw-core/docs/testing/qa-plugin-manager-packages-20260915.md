# 插件管理读取：九类开发包

后续补充验证：当前源码全部 35 个显式项及原 Console 2453 项已在 [完整回归](full-ui-regression-20260915.md) 中通过，原包与源码哈希未改。下文保留构建阶段自身的实际测试范围。

延续已批准的构建与逐项测试方案，将通过回归的管理读取接口纳入新开发快照。插件执行路线仍待用户确认；本轮不引入 Python、不改运行时决策或原前端。控制日志：`dist/qa-delivery-manager-20260915-EOsFab`。

## Checklist

- [x] 核对上一轮 849 普通测试、4 项相关显式测试的源码证据及旧九类包；两处暂存移至控制目录下 `staging-before-build`，可恢复。
- [x] 全新目录构建九类包，不覆盖 `VQYCTN`。
- [x] 逐包校验内容、原 Console、来源与签名；DMG 只读检查后正常卸载。2931 条来源与四份各 1311 文件的 Console 校验通过。
- [x] 新 release App Server 原管理页/App Center/参考测试；Rust SDK 对新 source Core 集成；隔离安装 SDK、VSIX 和 legacy wheel 后逐项测试全部通过，范围如下。
- [x] 汇总新包下载、SHA-256、失败记录及实际测试范围；最终来源、新旧各九包及保留暂存复核通过。
- [ ] 全功能、插件后端路线、包内 Core/原生/跨平台等剩余验收；不能随构建勾选完成。

不执行被限制的包内 Core、不启动原生窗口或激活 VS Code、不访问系统凭据、不绕过安全机制；不 commit/push 或发布。先前测试只作为已核对基线，不冒称本轮重跑。

本批输出：`dist/qa-runtime-20260914-CE3oB2`。Rust release Core 编译通过（54.88 秒）；source Core 为 `ab663c7277c9502e853bd83cb64d76f21bab11c24f541445a5bfd05944e6151b`，DMG/ZIP 签名后 Core 为 `4d240d8cd950c4cf1bf908649870515d9e5b987a1388bdf954a37f7cae4a8a37`。这是签名/字节一致性检查，不是包内 Core 运行验收。

## 保留的构建失败

首次 DMG 创建报“资源忙”并退出 1。App、签名和 ZIP 已完成，故未重跑这些步骤。只读预检确认无部分 DMG、本批挂载或 hdiutil 进程后，原参数重试成功（13.19 秒），再用既有 `--after-desktop` 完成剩余制品。首次失败、预检、重试及续建分别记录，没有覆盖失败日志或更改安全策略。

## 本轮运行验证

- 新编译的优化版 App Server 插件专项 4/4，5.96 秒：原管理页搜索/视图/刷新/重载、其 Python 13 个响应对照；原 App Center 打开/交互/返回/重开、其 Python 10 个响应对照。使用实际 Console 与隔离凭据，不启动系统凭据后端或原生桌面窗口。
- Rust SDK 对新 source Core：3/3，0.08 秒。复用源码未改的既有 release 集成测试程序，实际启动新 Core，验证连接、关闭时保存 interruption 和最终写入失败传播；未宣称重新编译了 Rust SDK 分发包。
- 安装后 TypeScript SDK 26/26（30.45 秒）、Python SDK 37/37（进程 34.75 秒），无失败或跳过；真实连接新 source Core stdio/WS/WSS。隔离导入路径、源码文件、依赖与 README 核对通过，Python 使用 `-S` 验证没有从日常 site-packages 导入被测 SDK。
- VS Code 源码 73/73；Universal 与 ARM64 VSIX 分别安装到独立 user-data/extensions 目录，校验实际安装内容、平台声明和资源，再各执行 24/24 组件测试。不激活扩展，不执行包内 Core，也不宣称完整 VS Code UI 验收。
- 保留版 wheel 在独立目录安装后，CLI 单测 855/855（17.62 秒），集成 36/36；JUnit 确认无失败、错误或跳过。保留原 `audioop` 弃用警告，不通过屏蔽日志消除它。这些不是 Rust CLI/TUI 的功能等价证据。
- [最终证据复核](../../../dist/qa-runtime-20260914-CE3oB2/final-verification.json) 于 2026-09-14T20:10:24.896Z 通过：2931 条来源、6 条额外相关证据、新旧各九类包及保留暂存哈希均核对；原前端与既有 Console 脚本不变，首次 DMG 失败保留。

## 仍未完成

本批已包含插件管理读取修复；没有实现或假装实现 Python 插件后端执行、安装/上传、热加载/卸载。运行时路线仍等待用户确认，见 [决策文档](../architecture/plugin-runtime-decision.md)，自动续跑不代表同意引入 Python。

前轮普通 Rust 849（35 ignored）和相关四项开发态显式测试只作源码匹配的基线；本轮新增的是优化版四项及新制品安装/运行检查，没有重新跑所有 35 项显式测试或原前端 2453 项。历史 SDK EOF、全部外部渠道与剩余原功能、原生 Desktop、包内 Core 执行、VS Code 激活、Windows/Linux/macOS x64 仍开放。没有 commit/push 或发布，总 goal 未完成。

旧 `VQYCTN` 九类文件及本轮移动的暂存仍保留。本页 `CE3oB2` 是该阶段开发快照；后续最新包为包含市场搜索和产品版本修复的 [xic9ei](qa-product-version-packages-20260915.md)，仍不是全功能正式版本。

## 下载

均为本机开发制品；桌面包为 QA ad-hoc 签名，未做 Developer ID 公证或原生启动验收。

| 制品 | 文件 |
| --- | --- |
| macOS ARM64 DMG | [下载](../../../dist/qa-runtime-20260914-CE3oB2/QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg) |
| macOS Desktop ZIP | [下载](../../../dist/qa-runtime-20260914-CE3oB2/QwenPaw-Tauri-2.2.0b5-macOS.zip) |
| macOS ARM64 Core | [下载](../../../dist/qa-runtime-20260914-CE3oB2/qwenpaw-core-darwin-arm64-QA.tar.gz) |
| 原 WebUI | [下载](../../../dist/qa-runtime-20260914-CE3oB2/webui/qwenpaw-webui-2.2.0b5-QA.tar.gz) |
| TypeScript SDK | [下载](../../../dist/qa-runtime-20260914-CE3oB2/sdk/qwenpaw-sdk-0.2.0.tgz) |
| Python SDK | [下载](../../../dist/qa-runtime-20260914-CE3oB2/sdk/qwenpaw_sdk-0.2.0-py3-none-any.whl) |
| Universal VSIX | [下载](../../../dist/qa-runtime-20260914-CE3oB2/vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix) |
| macOS ARM64 VSIX | [下载](../../../dist/qa-runtime-20260914-CE3oB2/vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix) |
| 保留版 Python 产品 | [下载](../../../dist/qa-runtime-20260914-CE3oB2/legacy/qwenpaw-2.2.0b5-py3-none-any.whl) |

[SHA256SUMS](../../../dist/qa-runtime-20260914-CE3oB2/SHA256SUMS)、[构建来源](../../../dist/qa-runtime-20260914-CE3oB2/build-manifest.json)、[静态检查](../../../dist/qa-runtime-20260914-CE3oB2/static-inspection.json)。DMG SHA-256：`c7678867d7c73f154bd6bac80f167a17a3d0e0ec1eb61b8deb84dd8efcf091a6`。
