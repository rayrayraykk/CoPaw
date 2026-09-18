# 产品版本与插件市场：开发包构建验收

后续完整回归提醒：同源码、同包的 [39 项首次专项回归](full-ui-product-version-20260915.md) 有 1 项浏览器退出超时；随后增加可选诊断、保留原超时/关闭判定，[整组 39 项重跑通过](browser-shutdown-diagnostics-20260915.md)，原超时根因仍未确认。原 Console 2453 单测通过。下文构建阶段记录保留，不代表全功能或原生验收完成。

延续已批准的构建与原交互等价方案，将市场搜索及产品版本身份修复纳入九类新开发包，保留旧 `CE3oB2`。本轮不修改原前端或插件后端执行路线，不发布、不 commit/push。

控制日志：`dist/qa-delivery-version-20260915-DGV6U0`。

## Checklist

- [x] 核对版本修复 855 普通测试、新专项 3 项及市场回归 2 项的来源；旧九类包哈希一致，Tauri/VS Code 两处暂存已移动至控制目录的 `staging-before-build`，可恢复。
- [x] 在新目录构建 DMG、Desktop ZIP、Core、WebUI、TS/Python SDK、Universal/ARM64 VSIX、保留版 Python wheel。
- [x] 九包哈希、2934 条来源、签名及包内容检查通过；四份 Console 各 1311 文件一致，DMG 只读挂载检查后正常卸载。
- [x] 优化版版本 3/3、市场 2/2 及 Rust SDK 对新 source Core 3/3；安装 TS 26/Python 37、VSIX 各 24、legacy CLI 855+36 全部通过。
- [x] 新旧各九包、2934 条来源与保留暂存最终复核通过，下载与测试范围见下文。
- [ ] 完整原功能、插件运行时、包内 Core/原生 Desktop/VS Code 激活、跨平台及历史偶发问题继续开放。

构建通过不代表原生运行或全功能验收通过。不执行包内 Core、不绕过安全策略、不启动原生窗口或激活扩展，不读取真实账号/系统凭据。仅 source release/test-harness Core 与隔离 headless Chrome/安装测试可执行。此前测试只作为来源核对的基线，不冒称本轮已重跑。

## 构建过程

新目录为 `dist/qa-runtime-20260914-xic9ei`。首次 DMG 创建“资源忙”退出 1，失败日志保留。预检确认目标 DMG 不存在、本批未挂载、无 hdiutil 进程后，使用原参数重试成功；随后通过既有 `--after-desktop` 续建，不重跑 App/签名/ZIP，也不改变安全设置。两个仅用于读取 SDK 测试程序哈希的 shell 内联诊断曾因 JavaScript/引号语法错误失败；修正诊断后完成来源校验，再执行测试，未更改被测代码。

release Core 编译 56.40 秒。source Core SHA-256 为 `f3881250843b0a516c6f721932ed98f300d55b7c746b793f9dd67f023547cf66`；桌面签名后 Core 为 `e09e9bce5e76fcf077e4f29e4b3fe153d232bd000a9d26825e36982291fecb05`。二者分别用于源码运行测试与包内容/签名核验，不混为包内运行证据。

## 本轮运行验证与边界

- 新编译优化版 App Server 版本专项 3/3（3.40 秒）：HTTP 产品 `2.2.0b5`、SDK Core `0.2.0` 与协议 `3` 独立，原版本文本对照，原页面标签/悬停/警告/取消/重载。原市场页面及 Python 17 组参数/完整响应对照 2/2（4.59 秒）。
- Rust SDK 集成 3/3（0.13 秒），复用哈希和源码均已核对的既有 release 测试程序，实际启动本轮新 source Core；不是执行归档包内 Core。
- 隔离安装 TS SDK 26/26（30.46 秒）、Python SDK 37/37（命令 34.86 秒），无跳过，真实连接新 source Core stdio/WS/WSS。核对安装内容、README、依赖及 Python `-S` 导入来源。
- VS Code 源码组件 73/73；Universal/ARM64 VSIX 分别安装到独立目录，内容、平台声明和资源核验通过，各 24/24 安装后组件测试。不激活扩展、不代表完整 VS Code 界面验收。
- 保留版 wheel 隔离安装后，CLI 单测 855/855（17.82 秒）、集成 36/36（72.22 秒）；JUnit 无失败、错误或跳过。保留原 `audioop` 弃用警告。这不是 Rust CLI/TUI 的完整等价证明。
- [最终复核](../../../dist/qa-runtime-20260914-xic9ei/final-verification.json) 于 `2026-09-14T21:04:40.874Z` 通过：2934 条构建来源、11 条额外源码证据、新旧各九包及旧暂存均匹配，原前端与旧浏览器驱动未改。四份 Console 与此前原版构建的文件树一致。

此前普通 Rust 855（39 ignored）仅作为源码匹配基线，本轮没有重跑完整 workspace、全部显式项或原 Console 2453 单测。当前包已包含市场搜索与版本修复，但安装/上传/卸载、官方目录、Python 插件后端路线及其他剩余原功能继续开放。原生 Desktop、包内 Core、VS Code 激活、Windows/Linux/macOS x64 与历史 SDK EOF 偶发问题仍未验收；没有 commit/push 或正式发布，总 goal 未完成。

## 下载

均为 macOS ARM64 本地开发快照或对应的通用语言/前端包。桌面为 ad-hoc 签名，未做 Developer ID 公证或原生启动验收；不是 Windows/Linux/macOS x64 制品。

| 制品 | 文件 |
| --- | --- |
| macOS ARM64 DMG | [下载](../../../dist/qa-runtime-20260914-xic9ei/QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg) |
| Desktop ZIP | [下载](../../../dist/qa-runtime-20260914-xic9ei/QwenPaw-Tauri-2.2.0b5-macOS.zip) |
| macOS ARM64 Core | [下载](../../../dist/qa-runtime-20260914-xic9ei/qwenpaw-core-darwin-arm64-QA.tar.gz) |
| 原 WebUI | [下载](../../../dist/qa-runtime-20260914-xic9ei/webui/qwenpaw-webui-2.2.0b5-QA.tar.gz) |
| TypeScript SDK | [下载](../../../dist/qa-runtime-20260914-xic9ei/sdk/qwenpaw-sdk-0.2.0.tgz) |
| Python SDK | [下载](../../../dist/qa-runtime-20260914-xic9ei/sdk/qwenpaw_sdk-0.2.0-py3-none-any.whl) |
| Universal VSIX | [下载](../../../dist/qa-runtime-20260914-xic9ei/vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix) |
| macOS ARM64 VSIX | [下载](../../../dist/qa-runtime-20260914-xic9ei/vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix) |
| 保留版 Python 产品 | [下载](../../../dist/qa-runtime-20260914-xic9ei/legacy/qwenpaw-2.2.0b5-py3-none-any.whl) |

[SHA256SUMS](../../../dist/qa-runtime-20260914-xic9ei/SHA256SUMS)、[构建来源](../../../dist/qa-runtime-20260914-xic9ei/build-manifest.json)、[静态检查](../../../dist/qa-runtime-20260914-xic9ei/static-inspection.json)。DMG SHA-256：`9c072d4f6070c7d0186aa6db93a57b625060e845ed313525ac4edc9efd50f6b9`。
