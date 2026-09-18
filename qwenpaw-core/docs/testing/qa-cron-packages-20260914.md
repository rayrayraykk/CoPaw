# 多 Agent Cron：新一轮 QA 制品

日期：2026-09-14。沿用已批准的全部制品构建/逐项验证方案。本轮把已验收的
公开多 Agent Cron 和目录列表截断修复纳入制品；不改原前端、不迁移数据。
源码验收见 [Cron 验收](cron-public-scope-acceptance.md)，此前 `737ANs` 不含该修复。

## Checklist

- [x] 只读核对 source Core、前端和将被构建工具替换的生成暂存目录；确认
  不包含用户维护文件/链接后，在全新目录构建九类 macOS ARM64 QA 制品。
- [x] 九个文件及全部来源输入逐项校验；只读挂载/卸载 DMG，验证签名完整性
  和四份原 Console 内容一致，不启动包内 Core 或桌面应用。
- [x] 两个实际 SDK 包及依赖隔离安装，TS 26 项、Python 37 项依次验收；
  使用同批安装的混合语言客户端，Core 仅调用已验证的源码 release。
- [x] 通用/ARM64 VSIX 分别隔离安装但不激活；完整业务 manifest、编译文件、
  平台资源与载荷对照，实际安装后各 24 项关闭/管理回归。
- [x] 保留版 wheel 实际安装，CLI 单元 855 项、集成 36 项依次运行；断言
  导入安装目录、关闭 pytest 源码路径注入，核对准确计数和零跳过。
- [x] 最终复验所有来源/制品摘要、测试终态和先后顺序、原前端零变更以及
  无本批残留挂载/测试进程；更新入口并保留全部失败记录。
- [ ] 包内 Core 运行、原生 Desktop/VS Code 激活、跨平台及全功能交互。

构建沿用现有正常 QA ad-hoc 签名，不变更系统安全设置或重试包内执行。
不接触真实 key/钥匙串/日常应用数据，不删除旧 QA 包或缓存，不 commit/push。
“构建成功、签名完整、隔离安装通过”不等于原生启动及全交互验收通过。

## 当前构建记录

本批输出 `dist/qa-runtime-20260914-4v2E9d`，预检及首段外层命令日志在
`dist/qa-build-cron-20260914-S52kaA`。前端生产构建 37.05 秒，Core 暂存构建
54.43 秒，Tauri release 20.52 秒。已核对并由正常构建替换两个生成暂存目录，
没有删除用户源文件，原暂存内容可由上批制品恢复。

源码验收的 `16e722...` 经过发布脚本 `cargo build --release -p qwenpaw-cli`
重新编译后，本批实际 source Core 为
`54f91416e223443dd0572b47b3ad25e7c0d3a5e0c5a1e679f56df46fb35d7ffd`。
本轮 SDK 必须对该实际二进制重新测试，不把两种构建的摘要/结果混写。

首次 DMG 创建因“资源忙”终态退出。确认本批无挂载、无 hdiutil 进程且没有
半成品后，原参数仅重试一次成功；未改变签名或系统安全设置，根因未确认。
已从脚本的 `--after-desktop` 入口继续余下制品，不重做成功的桌面构建。

九类制品已生成；`build-manifest.json` 和 `SHA256SUMS` 记录全部文件，
`source-inputs.json` 记录 2902 个输入，来源树 SHA-256：
`98f5ec52ff3c54f91f4d79cee08728a1643d29bcd71aeaa3035e505d530b3498`。

`static-inspection.json`：九个文件/2902 输入核对通过，DMG 和桌面 ZIP 签名
完整，DMG 已卸载。四份 Console 各 1311 文件，内容树摘要均为
`931aaed10507a21a239153dd25db1c5bcf8dad1887b3a5fc1dfc81e1bce9e688`，
和上批一致。TS 21 个文件、Python 7 个模块、两份 VSIX 各 16 个编译文件一致。
Core tar/ARM64 VSIX 内 Core 与 source `54f914...` 一致；正常 QA 签名后的
桌面 Core 为 `1ca59a706b66bd27d04040910db50477f6b4b0e1b4f89bc08573e32ad766a9e4`。

## 安装态验收

`installed-sdk-source-control.json`：本批 npm/wheel 及锁定依赖离线安装，
TS **26/26** 后再执行 Python **37/37**，全部无跳过。Python `-S` 禁用 site，
实际 SDK/websockets 导入路径均位于本批安装目录；混合连接使用同批 TS 包。
完整模块/编译文件、README 与依赖字段均同源码一致。测试只调用 source
Core `54f914...`，未启动包内副本。另以已有源码构建的 Rust SDK 测试程序
对这份新 Core 执行 **3/3**，没有重编译/替换正在验收的 Core。

`isolated-client-installation.json`：VS Code 源码 **73/73**；通用和 ARM64
VSIX 各安装一次，各 **24/24** 隔离关闭/管理测试通过，未激活扩展。完整
manifest 业务字段及编译文件和包载荷一致；只单独验证安装器附加的元数据，
平台由 VSIX XML 及 Core manifest/摘要确认。没有忽略业务配置差异。

保留版 wheel 实际安装后 CLI 单元 **855/855**（17.12 秒，一条既有 audioop
弃用警告），随后集成 **36/36**（68.47 秒）；核对真实导入安装目录、禁用
pytest 源码路径注入，JUnit 验证精确计数及零失败/错误/跳过。两 SDK 和
扩展/保留版检查合计 15 条安装命令按顺序成功退出。

最终 [final-verification.json](../../../dist/qa-runtime-20260914-4v2E9d/final-verification.json)
于 2026-09-14 12:12:48 UTC 通过：重新核对九个文件、2902 条来源、15 条安装
检查的成功退出及先后顺序、source Core 摘要和前端零 diff/status；本批无残留
挂载或 source Core 进程。首次 DMG 失败及一次原参数重试记录保留。

## 制品入口

- [macOS ARM64 QA DMG](../../../dist/qa-runtime-20260914-4v2E9d/QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg)
- [桌面 ZIP](../../../dist/qa-runtime-20260914-4v2E9d/QwenPaw-Tauri-2.2.0b5-macOS.zip)
- [Core tar](../../../dist/qa-runtime-20260914-4v2E9d/qwenpaw-core-darwin-arm64-QA.tar.gz)
- [WebUI tar](../../../dist/qa-runtime-20260914-4v2E9d/webui/qwenpaw-webui-2.2.0b5-QA.tar.gz)
- [TypeScript SDK](../../../dist/qa-runtime-20260914-4v2E9d/sdk/qwenpaw-sdk-0.2.0.tgz)
- [Python SDK](../../../dist/qa-runtime-20260914-4v2E9d/sdk/qwenpaw_sdk-0.2.0-py3-none-any.whl)
- [通用 VSIX](../../../dist/qa-runtime-20260914-4v2E9d/vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix)
- [ARM64 VSIX](../../../dist/qa-runtime-20260914-4v2E9d/vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix)
- [保留版 Python wheel](../../../dist/qa-runtime-20260914-4v2E9d/legacy/qwenpaw-2.2.0b5-py3-none-any.whl)

本批是 macOS ARM64 QA，不是生产签名/公证、原生窗口及全交互通过的发布版。
WebUI/Desktop 载荷逐字节一致不替代运行态测试；VS Code 隔离安装不等于激活。
保留版 CLI 回归不等于 Rust CLI/TUI 功能全部实现。后续继续实际功能与客户端
接线缺口，只有生产变化才重建新批次，不以重复打包替代全功能目标。
