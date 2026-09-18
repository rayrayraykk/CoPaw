# Channels 隔离修复：九类 QA 制品验收

日期：2026-09-14，macOS ARM64。批次 `dist/qa-runtime-20260914-eaTvv4`，包含 [Channels 配置隔离修复](channel-workspace-scope-acceptance.md)。本轮未改生产代码或原 Console 源码，完成上轮修复后的完整原页面回归、重新构建和逐项隔离安装测试；没有提交或发布。

## 下载清单

九个文件均已构建并核对大小、SHA-256 和包内来源。**DMG/App 是 ad-hoc QA 包，原生启动仍未验收，不是正式签名发布。**

| 制品 | 文件 |
| --- | --- |
| macOS ARM64 DMG | [下载](../../../dist/qa-runtime-20260914-eaTvv4/QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg) |
| macOS Desktop ZIP | [下载](../../../dist/qa-runtime-20260914-eaTvv4/QwenPaw-Tauri-2.2.0b5-macOS.zip) |
| macOS ARM64 Core | [下载](../../../dist/qa-runtime-20260914-eaTvv4/qwenpaw-core-darwin-arm64-QA.tar.gz) |
| 原 WebUI | [下载](../../../dist/qa-runtime-20260914-eaTvv4/webui/qwenpaw-webui-2.2.0b5-QA.tar.gz) |
| TypeScript SDK | [下载](../../../dist/qa-runtime-20260914-eaTvv4/sdk/qwenpaw-sdk-0.2.0.tgz) |
| Python SDK | [下载](../../../dist/qa-runtime-20260914-eaTvv4/sdk/qwenpaw_sdk-0.2.0-py3-none-any.whl) |
| Universal VSIX | [下载](../../../dist/qa-runtime-20260914-eaTvv4/vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix) |
| macOS ARM64 VSIX | [下载](../../../dist/qa-runtime-20260914-eaTvv4/vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix) |
| 保留版 Python 产品 | [下载](../../../dist/qa-runtime-20260914-eaTvv4/legacy/qwenpaw-2.2.0b5-py3-none-any.whl) |

摘要见 [SHA256SUMS](../../../dist/qa-runtime-20260914-eaTvv4/SHA256SUMS)、[构建清单](../../../dist/qa-runtime-20260914-eaTvv4/build-manifest.json)，最终核对见 [验收结果](../../../dist/qa-runtime-20260914-eaTvv4/final-verification.json)。
DMG SHA-256：`8c863e8c3d3a8f5fe40ab1d8b70c9659cd7931e4692ff431e1b3b3b088b3c230`。

## 构建与内容核对

- 构建前逐项核对上一轮 15 个来源文件。两处暂存目录无跟踪文件和符号链接，旧 Core/1,311 个 Console 文件与已知来源一致。仅替换可重建的 `console/src-tauri/binaries/qwenpaw-core` 与 `extensions/vscode/resources/core` 内容，可从旧 QA 包恢复；旧包和用户数据保留。
- 标准 stage 重新编译 Core，53.43 秒；本批 source Core SHA-256 为 `7dbb3b342d361f75bfd9d2eb6c349b181f43acac0076001291eb2cc98ab3b37a`。新 SDK/客户端全部对这个来源二进制测试。
- 2,906 条来源记录逐项核对，来源树摘要 `68560a1e27b15708dd702ee6daba508f57afc5842e4849641ad4c35d90ee09df`。
- DMG 校验、只读挂载、App 深度签名完整性检查通过并正常卸载。Desktop ZIP/DMG 内的签名后 Core 摘要相同，为 `88339c2244c419e62c9a6f9f3464eed44dcba03796939f4a7d29c396d6747b46`；Core archive 和 ARM64 VSIX 内 Core 与 source Core 相同。未执行分发二进制。
- WebUI、DMG、Desktop ZIP、保留版 wheel 的 Console 各 1,311 个文件完全一致，内容树摘要仍为 `931aaed10507a21a239153dd25db1c5bcf8dad1887b3a5fc1dfc81e1bce9e688`。TS、Python 和 VSIX 模块均与对应源码输出相同。

## 逐项验证

| 项目 | 结果与边界 |
| --- | --- |
| 原页面/参考显式组 | 30/30：App Server 29 项与 CLI Debug 页面 1 项；含 Channels 新专项。不是原生 Desktop/VS Code 激活 |
| 原前端 | 295 个文件、2453/2453；65.38 秒，未修改源码；预期错误注入/jsdom 提示保留 |
| Rust SDK 来源对照 | 3/3；直接运行未变的已编译测试程序，其真实 CLI 路径指向本批 source Core，不重新编译替换已打包二进制 |
| 安装后 TypeScript SDK | 26/26，无跳过；新 tgz 离线安装，真实 source Core stdio/WS/WSS |
| 安装后 Python SDK | 37/37，无跳过；新 wheel、`python -S`、websockets 15.0.1，含同批 TS/Python 共享连接测试 |
| VS Code 源码 | 73/73；并非扩展原生激活 |
| Universal / ARM64 VSIX | 隔离安装，各 24/24；安装后模块与包内资源一致，不执行包内 Core |
| 保留版 wheel CLI | 855/855 + 36/36；实际导入新安装包，JUnit 确认无失败/错误/跳过，不代表 Rust CLI/TUI 全功能完成 |

SDK 与客户端共 15 条安装/测试/结果检查命令顺序执行，全部退出 0，见本批 `installed-sdk-source-control.json`、`isolated-client-installation.json`。完整显式/前端日志在 `dist/qa-channel-scope-20260914-ezhatT/channel-explicit-all.log` 和 `channel-frontend-all.log`。本轮没有重跑已验证且源码不变的普通 Rust 757 项，不将其计为新的执行记录。

## 保留的失败与开放项

首次 DMG 创建因“资源忙”退出 1。只读预检确认无部分 DMG、无本批挂载和 hdiutil 进程后，按原参数重试一次成功；随后 `--after-desktop` 续建其余制品，没有重做成功的 Desktop 或改签名/安全策略。失败日志、预检、重试及续建元数据均保留。

- [x] 九类文件构建、来源/包内容核对与逐项隔离安装测试。
- [x] 原 `console/src` 零 diff/status，旧制品和用户数据保留。
- [ ] 包内 Core、原生 Desktop 启动与 VS Code 激活。
- [ ] Windows/Linux/macOS x64 原生构建与实机验证。
- [ ] 默认共享宿主/凭据策略、17 个外部通道和其他全功能差距；goal 继续，不能用本批验收证明功能全等价。
