# 最新恢复改动：九类开发快照验收

日期：2026-09-15（Asia/Shanghai）。构建目录使用 UTC 日期：`dist/qa-runtime-20260914-6BiWlu`。本批把 [宿主恢复集成](agent-publication-host-acceptance.md) 和 [凭据强杀验证及测试竞态修复](agent-publication-interrupt-acceptance.md) 纳入重新构建的 Core；不是全功能正式发布，不关闭原生、跨平台或全部旧功能等价门禁。没有修改原 Console 源码，没有 commit、push 或发布。

## 下载清单

九个文件均已构建，大小、SHA-256、来源及包内内容核对通过。DMG/App 使用既有 ad-hoc QA 签名，**尚未验收原生启动，也没有 Developer ID 签名与公证**。

| 制品 | 文件 |
| --- | --- |
| macOS ARM64 DMG | [下载](../../../dist/qa-runtime-20260914-6BiWlu/QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg) |
| macOS Desktop ZIP | [下载](../../../dist/qa-runtime-20260914-6BiWlu/QwenPaw-Tauri-2.2.0b5-macOS.zip) |
| macOS ARM64 Core | [下载](../../../dist/qa-runtime-20260914-6BiWlu/qwenpaw-core-darwin-arm64-QA.tar.gz) |
| 原 WebUI | [下载](../../../dist/qa-runtime-20260914-6BiWlu/webui/qwenpaw-webui-2.2.0b5-QA.tar.gz) |
| TypeScript SDK | [下载](../../../dist/qa-runtime-20260914-6BiWlu/sdk/qwenpaw-sdk-0.2.0.tgz) |
| Python SDK | [下载](../../../dist/qa-runtime-20260914-6BiWlu/sdk/qwenpaw_sdk-0.2.0-py3-none-any.whl) |
| Universal VSIX | [下载](../../../dist/qa-runtime-20260914-6BiWlu/vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix) |
| macOS ARM64 VSIX | [下载](../../../dist/qa-runtime-20260914-6BiWlu/vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix) |
| 保留版 Python 产品 | [下载](../../../dist/qa-runtime-20260914-6BiWlu/legacy/qwenpaw-2.2.0b5-py3-none-any.whl) |

[SHA256SUMS](../../../dist/qa-runtime-20260914-6BiWlu/SHA256SUMS)、[构建清单](../../../dist/qa-runtime-20260914-6BiWlu/build-manifest.json)、[静态检查](../../../dist/qa-runtime-20260914-6BiWlu/static-inspection.json)。DMG SHA-256：`8941c02761a41da440fb5f8bbdf799f42aff84d811e749d9b9b42b6006e78fef`。

## 来源与恢复性

- 本轮构建前复核上轮 50 个证据文件，仅计划文档有预期更新；842 项普通 Rust 测试的上轮最终通过记录仍有效，本轮不将它们计为重新执行。
- 新清单含 2927 条来源记录，相对旧 `eaTvv4` 增加 21 个文件、变更 17 个文件，均属于已实现的后端和测试工作；来源树摘要为 `28a615bb05141f5aef68db0efcd554b708f0862f688b122b31941f7be12307d1`。
- release Core 正常构建耗时 57.31 秒，版本检查通过。源码二进制 SHA-256 为 `6bd1bd3872a24222a9986b648279502b5e4d715ccb7961be47611213dc9488b4`；Core archive 和 ARM64 VSIX 内 Core 与之相同。
- DMG 与 Desktop ZIP 的签名后 Core 摘要相同：`280a0c4cc23e4d41286df9b06ff5031077e53f12640d0be9be4df334a7408945`。DMG 完成校验、只读挂载、App 深度签名完整性检查，并正常卸载；未执行包内二进制。
- 四份 Console（WebUI、DMG、Desktop ZIP、保留版 wheel）各 1311 个文件，与原生产构建逐文件相同；内容树摘要仍为 `931aaed10507a21a239153dd25db1c5bcf8dad1887b3a5fc1dfc81e1bce9e688`。前端归档字节摘要可因归档元数据变化而不同，不代表资源改动。
- 两处旧暂存目录已验证无跟踪文件/链接，Core 和 Console 来源匹配；移动至 `dist/qa-delivery-20260915-3RUKd0/staging-before-build/tauri-core` 与 `vscode-core`，可恢复。旧九个制品和用户数据保留。

## 逐项测试

| 项目 | 结果与边界 |
| --- | --- |
| 完整原页面/参考显式组 | 31/31：App Server 30 项、CLI Debug 1 项；不是原生 Desktop 或 VS Code 激活 |
| 原前端单测 | 295 个文件，2453/2453，66.78 秒；原有预期错误注入和 jsdom 提示保留 |
| Rust SDK | 3/3；复用未改测试源码对应的已编译 release 测试程序，调用本批新源码 Core；未重编测试程序或执行分发副本 |
| 安装后 TypeScript SDK | 26/26，无跳过；新 tgz 离线安装，连接 source Core stdio/WS/WSS |
| 安装后 Python SDK | 37/37，无跳过；新 wheel、`python -S`、websockets 15.0.1，含同批 TS/Python 共享连接测试 |
| VS Code 源码 / 安装后两种 VSIX | 73/73；隔离安装后各 24/24，模块和包资源一致；未激活扩展或运行包内 Core |
| 保留版 wheel CLI | 855/855 单测和 36/36 集成，JUnit 确认无失败/错误/跳过，真实导入新安装包；不代表 Rust CLI/TUI 全功能等价 |

执行日志和命令退出元数据在 `dist/qa-delivery-20260915-3RUKd0`；安装测试日志在制品目录 `installed-sdk`、`installed-clients`。SDK/客户端 15 条安装、测试和结果检查命令顺序执行，全部退出 0。最终 [证据复核](../../../dist/qa-runtime-20260914-6BiWlu/final-verification.json) 于 `2026-09-14T18:11:07.947Z` 通过：2927 条来源、前端源码/既有脚本来源未变、新旧九类制品、保留暂存、安装测试结果均重新核对。

## 已保留的失败和未完成项

首次 DMG 创建报“资源忙”，构建退出 1。只读确认无部分 DMG、无本批挂载、无 hdiutil 进程后，按原参数重试一次成功，再用既有 `--after-desktop` 续建；未重做成功的 Desktop、未改安全检查或尝试重启被拒绝的程序。失败、预检、重试、续建记录全部保留。

- [x] 九类开发制品构建、来源/内容核对，旧包和两处暂存可恢复。
- [x] 完整原页面与原前端回归；安装后 SDK 和两种 VSIX 指定范围测试。
- [x] 保留版 CLI 安装测试及本批最终证据复核。
- [ ] 包内 Core 运行、原生 Desktop 窗口、VS Code 激活；不绕过既有安全限制。
- [ ] Windows/Linux/macOS x64 原生构建与实机验证。
- [ ] 系统凭据服务/断电持久性、注册前孤立暂存、全部在途业务静止、并发路径替换边界及恢复协议完备性。
- [ ] 17 个外部 Channel 和记忆、索引、图谱、插件、ACP、语音、浏览器、邮件、审批等剩余功能差距；默认共享宿主/凭据策略仍待确定。

本轮通过不表示历史浏览器焦点或 SDK EOF 偶发失败已找到根因，也不表示全部前端交互已被穷尽验证。总 goal 保持开放。
