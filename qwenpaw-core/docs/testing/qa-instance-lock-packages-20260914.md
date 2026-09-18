# 启动互斥修复：整套 QA 制品

日期：2026-09-14，macOS ARM64。继续已确认的九类构建和逐客户端验收方案，
不迁移旧数据，不改原前端。源码基础见 [启动互斥验收](instance-lock-acceptance.md)。

本批 source release Core SHA-256：
`1437995c909850b86a7f434be3aed586b6036789d03f90f8d594e35e8b3b858d`。
该源码已通过 Rust 715、原前端 2453、显式浏览器/参考 27、release 集成 6、
TS 10、Python 17、VS Code 57 项检查。旧 `2qPEew` 使用 `19e454...`，
不能作为此次启动锁修复的分发证明。

## 执行清单

- [x] 在新目录依次构建 DMG、桌面 ZIP、Core tar、WebUI tar、TS SDK、
  Python SDK、通用 VSIX、ARM64 VSIX、保留版 Python wheel；不覆盖旧制品。
- [x] 逐项核对来源与文件哈希；只读 DMG 挂载和签名完整性检查，四份原
  Console 逐文件相同，Core 与本批源构建对应；不执行分发 Core。
- [x] 实际离线安装 TS/Python SDK，分别运行 10/17 项并确认实际导入路径；
  运行对照仅连接上述 source release Core。
- [x] 两份 VSIX 分别隔离安装但不激活；保留版 wheel 安装后依次运行
  855 项 CLI 单元和 36 项集成测试，不用源码路径注入代替安装态。
- [x] 最终检查来源未变化、测试执行顺序、无本批挂载或 source Core 残留，
  更新主计划和制品入口，保留失败记录。
- [ ] 分发 Core 真正启动、原生 GUI/扩展激活、跨平台、共享宿主连接与
  全功能交互独立完成，不以本批静态与源码对照关闭这些项。

## 构建记录

本批目录：产品仓库 `dist/qa-runtime-20260914-g6i9VJ`。
Console 生产构建 **43.25 秒**，Core 暂存构建 **0.55 秒**，Tauri release
构建 **22.68 秒**。桌面与扩展的生成暂存资源从 `19e454...` 更新到本批
`143799...`，旧制品输出目录不变。

首次 DMG 创建报“资源忙”，构建已经终止。核对无挂载、无相关构建进程、
无目标 DMG 半成品后，仅原参数重试一次成功（**6.07 秒**）。首次日志
`desktop-dmg.log`、挂载检查 `desktop-dmg-preflight.log`、重试日志和时间
记录 `desktop-dmg-retry.log` / `.json` 保留；随后由既有 `--after-desktop`
入口继续其余制品，没有重新签名或运行包内 Core，资源忙根因仍未确认。

九个文件及大小/哈希见 `build-manifest.json`、`SHA256SUMS`。来源清单共
**2888** 条，来源树 SHA-256：
`9a60897a83599286bb88c69cad1740b7002650223c449357638a8621e1932cae`。
清单表示构建来源，不是完整功能或包内运行验收。

## 静态核验与 SDK 安装态

`static-inspection.json` 通过：九个制品校验和、来源输入、只读 DMG 验证和
签名完整性核验完成，挂载已释放。Core tar 与 ARM64 VSIX 的 Core 字节
等于 source release；桌面正常 QA 签名后的 Core SHA-256 为
`ed19bfa77a32a4dc7161f765d3f5a4512775f8bd149de5c94562d357fad4c7bf`。
未执行任何包内 Core。

四份 Console 各 **1311** 个文件逐文件一致，载荷摘要为
`931aaed10507a21a239153dd25db1c5bcf8dad1887b3a5fc1dfc81e1bce9e688`。
原 `console/src` 没有改动；构建字节一致不等于全部原生交互已经验收。

`installed-sdk-source-control.json` 通过：TS 包和 Python wheel 在全新隔离
目录离线安装，核对实际导入位置、18 个 TS 生产文件及 5 个 Python 模块。
TS README 与源码相同，Python wheel METADATA 包含当前 README；本批
已包含最终保存失败、同目录锁与尚未实现自动连接的文档说明。

顺序执行 TS **10/10**（命令耗时 **30.437 秒**）和 Python **17/17**
（**33.506 秒**），无跳过。Python 使用 qwenpaw conda 环境。这些安装后的
客户端连接 `143799...` source release，不用它们宣称分发 Core 启动通过。

## VSIX、保留版 CLI 与最终核验

`isolated-client-installation.json` 通过。通用和 ARM64 VSIX 分别使用全新
VS Code 用户目录及扩展目录安装，每份 15 个 JavaScript 文件与制品一致。
通用包不含 Core，ARM64 包的两个 Core 资源匹配；均未激活扩展。

保留版 Python wheel 离线安装后，实际导入路径位于隔离安装目录，禁用
pytest 的仓库 `pythonpath` 注入。CLI 单元 **855/855，17.15 秒**，
集成 **36/36，66.47 秒**，依次完成且无跳过；单元日志保留 1 条 warning。
这是原 Python CLI 的安装态回归，不是 Rust CLI/TUI 已全量实现的证明。

`final-verification.json` 于 **2026-09-14 08:54:00 UTC** 通过：九个文件
大小/哈希、2888 条来源及来源树摘要、SDK 文档、安装检查的顺序与退出码
重新核验；原 `console/src` 的 diff 和状态均为空，git diff 检查通过，
没有本批 DMG 挂载或 source Core 进程。首次 DMG 失败记录没有覆盖。
报告的 `packagedRuntimeTested=false`、`vscodeActivated=false` 保持不变。

## 制品入口

全部位于产品仓库 [本批输出目录](../../../dist/qa-runtime-20260914-g6i9VJ)：

- [macOS ARM64 QA DMG](../../../dist/qa-runtime-20260914-g6i9VJ/QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg)
- [桌面 ZIP](../../../dist/qa-runtime-20260914-g6i9VJ/QwenPaw-Tauri-2.2.0b5-macOS.zip)
- [Core tar](../../../dist/qa-runtime-20260914-g6i9VJ/qwenpaw-core-darwin-arm64-QA.tar.gz)
- [WebUI tar](../../../dist/qa-runtime-20260914-g6i9VJ/webui/qwenpaw-webui-2.2.0b5-QA.tar.gz)
- [TypeScript SDK](../../../dist/qa-runtime-20260914-g6i9VJ/sdk/qwenpaw-sdk-0.2.0.tgz)
- [Python SDK](../../../dist/qa-runtime-20260914-g6i9VJ/sdk/qwenpaw_sdk-0.2.0-py3-none-any.whl)
- [通用 VSIX](../../../dist/qa-runtime-20260914-g6i9VJ/vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix)
- [ARM64 VSIX](../../../dist/qa-runtime-20260914-g6i9VJ/vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix)
- [保留版 Python wheel](../../../dist/qa-runtime-20260914-g6i9VJ/legacy/qwenpaw-2.2.0b5-py3-none-any.whl)

这些是本机 QA 构建，不是已通过原生启动和公证的正式发布版。下一步继续
共享宿主/默认 Workspace 接入设计与测试，不能以反复重打包替代功能实现。

## 边界

正常 QA ad-hoc 签名不等于正式 Developer ID/公证。本批不为了绕过已知
启动失败调整签名、路径或系统安全策略；不运行会执行包内 Core 的完整
qualifier/运行 smoke，不使用真实 key/keychain 或日常 App 数据。

正常构建只替换已经核对的生成暂存资源。上轮获准删除的 debug 目录已重新
用于构建，本批不继续清理缓存、旧包、未提交改动或报告，不 commit/push/发布。
