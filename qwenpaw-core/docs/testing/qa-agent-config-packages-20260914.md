# Agent 设置绑定修复：九类 QA 制品验收

日期：2026-09-14，macOS ARM64。批次 `dist/qa-runtime-20260914-wADUE9`。
本批包含 [Agent 设置保存修复](agent-config-binding-acceptance.md)，不修改
原 Console，不迁移旧数据，不提交或发布。

## 下载清单

下列九个文件均已构建并逐项核对大小、SHA-256 和包内来源。
**DMG/App 为 ad-hoc QA 包，尚未完成原生启动验收，不是正式签名发布。**

| 制品 | 文件 |
| --- | --- |
| macOS ARM64 DMG | [下载](../../../dist/qa-runtime-20260914-wADUE9/QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg) |
| macOS Desktop ZIP | [下载](../../../dist/qa-runtime-20260914-wADUE9/QwenPaw-Tauri-2.2.0b5-macOS.zip) |
| macOS ARM64 Core | [下载](../../../dist/qa-runtime-20260914-wADUE9/qwenpaw-core-darwin-arm64-QA.tar.gz) |
| 原 WebUI | [下载](../../../dist/qa-runtime-20260914-wADUE9/webui/qwenpaw-webui-2.2.0b5-QA.tar.gz) |
| TypeScript SDK | [下载](../../../dist/qa-runtime-20260914-wADUE9/sdk/qwenpaw-sdk-0.2.0.tgz) |
| Python SDK | [下载](../../../dist/qa-runtime-20260914-wADUE9/sdk/qwenpaw_sdk-0.2.0-py3-none-any.whl) |
| Universal VSIX | [下载](../../../dist/qa-runtime-20260914-wADUE9/vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix) |
| macOS ARM64 VSIX | [下载](../../../dist/qa-runtime-20260914-wADUE9/vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix) |
| 保留版 Python 产品 | [下载](../../../dist/qa-runtime-20260914-wADUE9/legacy/qwenpaw-2.2.0b5-py3-none-any.whl) |

完整摘要见 [SHA256SUMS](../../../dist/qa-runtime-20260914-wADUE9/SHA256SUMS)
和 [构建清单](../../../dist/qa-runtime-20260914-wADUE9/build-manifest.json)。
DMG SHA-256：`968ff5fcbab3f6c591d450183ddf73c870474092861852f44749adcca91b3408`。

## 构建与包内容核对

- 构建前核对上轮六个验收来源文件；两处可重建暂存目录没有跟踪文件或
  符号链接，暂存 Console 1,311 个文件与当前构建完全一致。构建仅替换
  `console/src-tauri/binaries/qwenpaw-core` 和扩展 `resources/core` 的生成
  暂存内容；可重新构建或从旧 QA 包恢复，旧包、源码和报告均保留。
- 标准 stage 脚本再次编译 Core（54.29 秒），本批来源二进制 SHA-256 为
  `be7918b23a994c4d3a7b640879d3fb4bc74aef4ebc2320800d771ed20d1d5b76`。
  它与此前源码验收的 `8dad780...` 字节不同，故客户端全部对本批来源 Core
  再验证；不直接沿用旧二进制测试结果，也不猜测字节差异的具体原因。
- 2,903 条源码记录逐项核对；来源树摘要为
  `50e3600ee3c06211f25c13b9cc18ac1b685446895c0684ba37dd7ce864fd8fd7`。
- DMG 校验、只读挂载、App 深度签名完整性检查通过，之后正常卸载。
  Desktop ZIP/DMG 的签名后 Core 一致，摘要为 `3ee8ff117bdca6c5fdde04c1d55aa7248e60c8735df54cc226028098ca1ea9eb`。
  Core archive 和 ARM64 VSIX 内的未另签名 Core 与本批来源二进制相同。
- WebUI、Desktop ZIP、DMG 和保留版 wheel 的原 Console 各 1,311 个文件
  完全相同，内容树摘要仍为 `931aaed10507a21a239153dd25db1c5bcf8dad1887b3a5fc1dfc81e1bce9e688`。
  TS 21 个编译文件、Python 7 个模块及两份 VSIX 各 16 个编译文件与源码输出一致。

## 逐项测试

所有凭据都是测试假值。新 SDK 包离线安装到本批隔离目录，测试调用本批
**source Core**，不执行分发目录中的 Core。

| 验收 | 结果 | 边界 |
| --- | --- | --- |
| Rust SDK 来源对照 | 3/3 | 直接运行已编译测试程序，其真实 CLI 路径指向本批 source Core，不重新编译替换该二进制 |
| 已安装 TS SDK | 26/26，无跳过 | 模块从新 tgz 安装；stdio、WS/WSS、关闭及实际轮次 |
| 已安装 Python SDK | 37/37，无跳过 | conda qwenpaw、`python -S`，确认导入新 wheel 与 websockets 15.0.1，含同批 TS/Python 共享宿主 |
| VS Code 源码对照 | 73/73 | 同一个 source Core；并非原生扩展激活 |
| Universal / ARM64 VSIX | 隔离安装，各 24/24 | 安装后的模块与资源逐项相同；只排除已验证的安装器元数据，不执行包内 Core |
| 保留版 wheel CLI | 855/855 + 36/36 | 新 wheel 安装后运行，JUnit 确认无失败/错误/跳过；不是 Rust CLI/TUI 全功能证明 |

SDK 和客户端共 15 条安装/测试/结果检查命令顺序执行，全部正常退出。
证据为本批 `installed-sdk-source-control.json`、`isolated-client-installation.json`
及其安装目录中的日志。此前源码层的 Rust 749、显式组 29 和原前端 2453
通过记录仍见源码验收，不把它们称为本批原生安装态测试。

## 保留的失败与限制

首次 DMG 创建因“资源忙”退出 1。只读预检脚本最初有正则转义语法错误，
没有执行检查或重试；修正后确认无部分 DMG、无本批挂载、无 hdiutil 进程，
才按原参数重试一次并成功。未改变签名、安全策略或 Core 路径。
构建失败在 `dist/qa-build-config-binding-20260914-9D1WC9`；预检失败、修正
检查、DMG 重试及 `--after-desktop` 续建记录均在本批目录中保留。

- [x] 九类文件构建、来源/包内容核对及逐项隔离安装测试。
- [x] 原 `console/src` 保持零 diff/status；未删除旧制品或用户缓存。
- [ ] 包内 Core、原生 Desktop 启动和 VS Code 激活，仍未验收。
- [ ] Windows/Linux/macOS x64 原生构建与实机验证，不能用 ARM64 结果替代。
- [ ] 默认共享宿主/凭据策略、外部通道及其他功能差距继续开放；goal 未完成。

初版最终审计复用了旧 inspection 目录后缀，其制品和安装测试核对记录保留。
随后按本批 `static-inspection.json` 的实际目录补充一致性/目录存在校验，
重新检查全部来源、制品、顺序和挂载收尾；没有重新构建或修改包。
最终以本批 `final-verification-corrected.json` 为准。
