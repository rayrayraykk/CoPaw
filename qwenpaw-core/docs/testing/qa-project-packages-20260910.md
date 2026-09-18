# Project Directory 修复版 QA 制品

日期：2026-09-10，平台 macOS ARM64；批次
[`qa-runtime-20260910-vGGX8Z`](../../../dist/qa-runtime-20260910-vGGX8Z)。
它包含 [项目目录多 Agent 修复](project-directory-ownership-acceptance.md)，
没有覆盖上一批 `qa-runtime-20260910-ZPQQT0`，也没有清理历史制品。

## 来源和边界

- 原前端源码 `console/src` 零 diff；业务界面没有重写。
- 源 release Core：`8dae6f31ae31c4da50f9d028e3253013d832346e919fc9e4ea930a05a119100f`。
- 桌面包内签名后的 Core：`3383bb6d3c4ecdad08955a6df850b3b5ff9b5a2abc9d3f077cda6397c69b3c01`。
- 2869 个来源条目的树摘要：
  `8fb5aa17e3c1df403cb1cbb9112b1afbfb97f38c6f0daa62f2572b350a08dd9d`。
  当前工作区含未提交改动；不是某个已发布 Git commit 的干净构建。
- Console production build 38.07 秒，Core staging 缓存构建 0.49 秒，Tauri
  release build 21.30 秒；使用 QA ad-hoc 签名，没有发行签名/公证凭据。
- 第一次 DMG 创建报“资源忙”；确认命令结束、无挂载、无半成品目标后，
  原参数重试成功。保留 `desktop-dmg.log` 和 `desktop-dmg-retry.log`。
  没有变更签名策略、重试分发 Core 启动或调整系统安全配置。

## 九类制品

全部文件的字节数和 SHA-256 已经与
[`build-manifest.json`](../../../dist/qa-runtime-20260910-vGGX8Z/build-manifest.json)
及 [`SHA256SUMS`](../../../dist/qa-runtime-20260910-vGGX8Z/SHA256SUMS) 对照通过。

| 制品 | 字节数 | 已完成检查 |
| --- | ---: | --- |
| `QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg` | 53,761,121 | 镜像校验、只读挂载、App 深度签名、Core/Console 内容；已卸载 |
| `QwenPaw-Tauri-2.2.0b5-macOS.zip` | 49,410,984 | 解包、App 深度签名、与 DMG 内容对照 |
| `qwenpaw-core-darwin-arm64-QA.tar.gz` | 15,515,997 | 解包后 Core 与当前 source release SHA 完全相同 |
| `webui/qwenpaw-webui-2.2.0b5-QA.tar.gz` | 26,285,767 | 解包后前端资源与当前 production build 完全相同 |
| `sdk/qwenpaw-sdk-0.2.0.tgz` | 11,786 | 实际离线安装、导入路径校验、4/4 SDK 测试 |
| `sdk/qwenpaw_sdk-0.2.0-py3-none-any.whl` | 7,584 | 无依赖离线安装、导入路径校验、5/5 SDK 测试 |
| `vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix` | 28,750 | 独立 profile 安装，15 个 JS 文件一致，不携带 Core |
| `vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix` | 16,053,564 | 独立 profile 安装，15 个 JS 和 2 个 Core 资源一致 |
| `legacy/qwenpaw-2.2.0b5-py3-none-any.whl` | 37,098,657 | 隔离安装及导入路径校验；CLI 单测 855/855、集成 36/36 |

DMG、ZIP、WebUI 和 legacy wheel 的四份 Console 均为 1311 文件，完整树摘要
`931aaed10507a21a239153dd25db1c5bcf8dad1887b3a5fc1dfc81e1bce9e688`，
与上一批相同。SDK 测试实际使用安装后的包，但连接的是上面核验过摘要的
**源码 release Core**，不是解包的 Core。

## 验收状态

- [x] 九类制品构建和静态检查；`static-inspection.json` 为 passed，
  `packagedRuntimeTested: false`。
- [x] TypeScript/Python SDK 隔离安装与测试；报告
  `installed-sdk-source-control.json` 为 passed，4/4（0.076 秒）及 5/5
  （0.599 秒），测试无跳过。
- [x] 两个 VSIX 和 legacy 隔离安装报告 `isolated-client-installation.json`
  为 passed；legacy CLI 855/855（17.24 秒）、集成 36/36（67.12 秒），无跳过。
  只有已有 `audioop` 弃用警告，没有修改旧产品源码来消除该提示。
- [x] `final-verification.json` 重新核对九个文件和三个报告均成功，测试 DMG
  已卸载，源 Core 测试进程无残留，前端源码零改动，diff 空白检查通过。
- [ ] 分发包 Core 启动、Desktop/WebKit 原生交互、VS Code 激活。
- [ ] Windows、Linux、macOS x64 实机构建和完整运行。

不能把构建、签名校验、源 Core 控制组或扩展安装当作包内运行成功。
此批 QA 不是全功能发布验收，完整目标继续保留。
