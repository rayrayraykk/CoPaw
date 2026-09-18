# 官方目录版：九类开发包 — 2026-09-15

本批纳入官方插件目录、版本保序比较，以及全量回归发现并修复的 Git 排除规则后台写入完成时序。原 Console 源码未改。源码修复过程见 [版本与 Git 验收](catalog-versions-git-write-20260915.md)，未完成项见 [官方目录方案](../architecture/official-plugin-catalog.md)。

构建后补充：保持本批源码、脚本及九包不变，已完成 [原前端全组回归](full-ui-official-catalog-20260915.md)：显式项 42/42（31 个浏览器、10 个 Python 对照、1 个源码文本检查）、原 Console 2453/2453、DevTools 16/16；来源和九包哈希再次核对通过。下文构建阶段的测试范围保留原记录，不能将这次补充验收扩大为原生或全功能通过。

控制记录：`dist/qa-delivery-catalog-20260915-1IstBp/`；制品：`dist/qa-runtime-20260914-NvQ0h0/`。目录日期按 UTC，文档日期按本机时区。本批为 macOS ARM64 本地开发快照及相应通用语言/前端包，不是 Windows/Linux/macOS x64 二进制发行版。

## 下载

桌面仅为 ad-hoc 签名，未做 Developer ID 公证；没有启动原生窗口、执行包内 Core 或激活 VS Code 扩展。请勿将构建/静态检查通过理解为全功能或原生运行验收通过。

| 制品 | 文件 |
| --- | --- |
| macOS ARM64 DMG | [下载](../../../dist/qa-runtime-20260914-NvQ0h0/QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg) |
| Desktop ZIP | [下载](../../../dist/qa-runtime-20260914-NvQ0h0/QwenPaw-Tauri-2.2.0b5-macOS.zip) |
| macOS ARM64 Core | [下载](../../../dist/qa-runtime-20260914-NvQ0h0/qwenpaw-core-darwin-arm64-QA.tar.gz) |
| 原 WebUI | [下载](../../../dist/qa-runtime-20260914-NvQ0h0/webui/qwenpaw-webui-2.2.0b5-QA.tar.gz) |
| TypeScript SDK | [下载](../../../dist/qa-runtime-20260914-NvQ0h0/sdk/qwenpaw-sdk-0.2.0.tgz) |
| Python SDK | [下载](../../../dist/qa-runtime-20260914-NvQ0h0/sdk/qwenpaw_sdk-0.2.0-py3-none-any.whl) |
| Universal VSIX | [下载](../../../dist/qa-runtime-20260914-NvQ0h0/vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix) |
| macOS ARM64 VSIX | [下载](../../../dist/qa-runtime-20260914-NvQ0h0/vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix) |
| 保留版 Python 产品 | [下载](../../../dist/qa-runtime-20260914-NvQ0h0/legacy/qwenpaw-2.2.0b5-py3-none-any.whl) |

[SHA256SUMS](../../../dist/qa-runtime-20260914-NvQ0h0/SHA256SUMS)、[构建来源](../../../dist/qa-runtime-20260914-NvQ0h0/build-manifest.json)、[静态检查](../../../dist/qa-runtime-20260914-NvQ0h0/static-inspection.json)。DMG 为 53,604,127 字节，SHA-256：`9302ed9c0041efec726fa32509b042ba49b5b5c5f022861cd9521b10821f1c41`。

## 验收 checklist

- [x] 构建前最终 workspace 871 passed、0 failed、42 ignored，含 2 个 doc tests；忽略项不计为自动通过。Clippy workspace/all-targets `-D warnings`、格式检查通过。
- [x] 目录与原 Python/浏览器相关筛选 18/18，含 17 项目录及 1 项既有 ACS3 签名向量。
- [x] 全部九包构建成功，本批 DMG 首次创建成功；source release Core 编译 55.20 秒。
- [x] 九包哈希、2940 个构建来源和签名/内容检查通过；DMG 只读挂载后正常卸载。四份 Console 各 1311 文件与上一批原版构建一致。
- [x] 新优化版 App Server harness 目录 18/18、Git 完成时序/错误及原排除规则 3/3；产品身份及原兼容页面 2 项、HTTP/SDK 身份 1 项分两条命令通过。
- [x] 既有、源码及二进制哈希均核对的 Rust SDK harness 对本批新 source Core 3/3；不执行归档/VSIX/桌面内的 Core。
- [x] 隔离安装 TS SDK 26/26、Python SDK 37/37；VS Code 源码组件 73/73，两类 VSIX 各 24/24 安装后组件测试；保留版 CLI 单测 855/855、集成 36/36，均无跳过。
- [x] 新旧九包、保留 staging、全部来源与测试证据最终复核于 `2026-09-14T22:23:39.353Z` 通过，见 [最终记录](../../../dist/qa-runtime-20260914-NvQ0h0/final-verification.json)。
- [ ] 完整原功能、插件后端、异常元数据、原生运行、跨平台及其余历史偶发问题继续开放。

source release Core SHA-256 为 `31459d3ba9123ecf3ea1098231434a1f04a6ace8e7ce53d7a1ec28f5ed9ca833`；桌面签名后 Core 为 `10ce598382c967afb0144cc0462889bfcc05805320c23afcc8b79491d21f713f`。二者的不同属于签名状态，不能混淆为包内运行证据。

旧 `xic9ei` 九包未覆盖。原 Tauri/VS Code staging 和旧 source Core 保存在控制目录 `staging-before-build/`，可恢复。本轮没有 commit/push、正式发布、真实账号/系统凭据或安全策略绕过。没有重跑全部原 Console 2453 单测或整组浏览器测试；本批不能用于证明其他未测页面/客户端已经全部等价。

保留版 Python CLI 测试不等于 Rust CLI/TUI 全面迁移完成；官方目录读取不等于插件安装/上传/卸载或 Python 插件后端可运行。总 goal 保持未完成。
