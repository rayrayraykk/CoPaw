# 活跃检查点版本：九类 QA 制品

本机日期：2026-09-10（UTC+8）。脚本采用 UTC 日期，输出目录为 `qwenpaw/dist/qa-runtime-20260909-EWMbMd`。这是分层验收记录，不是全功能/全平台/安装态全通过声明。

## 来源与范围

本批包含检查点第五步 Core/App Server 恢复静默期、自动 pending 保留、运行中手工快照修复，替代第三、四步包作为新的本地 QA 构建；旧包及其失败样本原字节保留。`console/src` 零 diff。源 Core SHA-256：`5ef1d8bd2e8f2bb3acc6303b30b46a4324a709f935962af1b18eba9e20fff2ae`。

源码基线：完整普通 **606/606**、严格 Clippy **8.67 秒**、完整显式组 **20/20，314.09 秒**，release **54.22 秒**；TS **4/4**、Python **5/5**、VS Code 编译与 **57/57**依次通过，连接上述源 Core，不是包内 Core。详细失败和修复记录见 [检查点验收](checkpoint-workspace-ownership-acceptance.md)。

本批 `build-manifest.json` 为 dirty 构建，Git commit 不能单独证明来源；记录 **2,849** 个构建输入，source tree SHA-256 为 `3137594a97c168f2938191142a728e1d03b3bcf77a59bfa58a1aba7bca716d6b`。随后逐文件验证，不把后来源码修改自动计入旧清单。

暂存根目录：`/var/folders/0s/4ht2q69j6sx49r64ktp8pssm0000gn/T/qwenpaw-qa-runtime-NyHuQO`。构建只更新 Tauri/VS Code 管理的生成资源，可由来源重建；没有清理用户未提交改动、旧制品或未确认的构建缓存。没有发布、真实密钥、Developer ID 公证或系统安全策略变更。

## 制品 Checklist

以下路径均相对于本批输出目录。

| 类型 | 文件 | 构建 |
| --- | --- | --- |
| Desktop DMG | `QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg` | 完成 |
| Desktop ZIP | `QwenPaw-Tauri-2.2.0b5-macOS.zip` | 完成 |
| 独立 Core | `qwenpaw-core-darwin-arm64-QA.tar.gz` | 完成 |
| 原 WebUI | `webui/qwenpaw-webui-2.2.0b5-QA.tar.gz` | 完成 |
| TypeScript SDK | `sdk/qwenpaw-sdk-0.2.0.tgz` | 完成 |
| Python SDK | `sdk/qwenpaw_sdk-0.2.0-py3-none-any.whl` | 完成 |
| 通用 VSIX | `vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix` | 完成 |
| macOS ARM64 VSIX | `vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix` | 完成 |
| 保留版 Python 产品 | `legacy/qwenpaw-2.2.0b5-py3-none-any.whl` | 完成 |

- [x] 九类文件生成，各步日志和来源清单保存在本批目录；本次 DMG 创建首次成功。
- [x] 九文件校验和、2,849 个来源输入与各分发载荷逐项验证。
- [x] DMG 镜像、只读挂载、ZIP/DMG 签名完整性检查及正常卸载。
- [x] SDK 实际安装后连接源 Core 的测试、保留版 CLI、两类 VSIX 隔离安装验证。
- [ ] 分发 Core 首次启动、实际原生窗口、VS Code 激活、完整打包 WebUI 运行。
- [ ] 最新 Windows/Linux 构建与实机交互、所有原功能矩阵。

`inspect-qa-macos.mjs` 只做静态核查，输出 `packagedRuntimeTested: false`；不运行会改签参考副本或启动包内 Core 的完整 qualifier。旧安装态 SIGKILL 的终端防护侧核查依赖仍未关闭，见 [策略核查材料](core-startup-policy-investigation.md)。静态签名完整性不等于系统允许执行，源 Core 成功也不能替代分发 Core 启动。

## 静态核验结果

本批 `static-inspection.json` 为 `passed: true`，记录检查与正常卸载完成；恢复观察时检查进程已结束，`hdiutil info` 无挂载。没有因上次输出丢失而重新执行检查。

四份 Console 分发均含相同的 **1,311** 个文件，树摘要为 `931aaed10507a21a239153dd25db1c5bcf8dad1887b3a5fc1dfc81e1bce9e688`。TS 包 18 文件、Python 包 5 个 Python 文件、两类 VSIX 各 15 个 JavaScript 执行文件均与来源一致。通用 VSIX 不包含 Core。ZIP/DMG 已签名 Core 相同，摘要为 `cc67c83c5f6a4d83b2571aeda54b0d14c23e13c82562520ee55860dcc6fb708d`；独立 tar/平台 VSIX Core 与上述源 release Core 相同，不混淆签名前后字节。

## 实际安装后的顺序复验

解包和隔离安装位于本批 scratch 的 `inspection-Lp4ncH`，不写日常应用目录。后续测试按顺序运行，SDK 明确连接源 `target/release/qwenpaw-core`，不启动包内 Core。

- [x] TS tgz 离线安装，真实模块路径位于 `installed-ts-mt8Y23/node_modules/@qwenpaw/sdk`。完整 **4/4，1.555 秒，无跳过**：协议初始化/通知、契约、真实源 Core 建会话和图片跨轮复用。将原编译测试逐字复制至隔离目录的相同层级，仅用 `src` 链接指向实际安装代码、`docs` 链接指向共享契约夹具；未改写测试断言、源码包或来源清单。
- [x] Python wheel 在 conda `qwenpaw` 离线无依赖安装，真实导入路径断言在 `installed-python-k6dOZU`，原完整测试 **5/5，0.604 秒，无跳过**。
- [x] 保留版 wheel 在 `installed-legacy-ccAXE6` 离线安装，单测和集成开始前分别断言真实导入路径。CLI 单测 **855/855，17.36 秒**，随后集成 **36/36，70.64 秒**。仅既有 `audioop` 弃用警告；该结果证明保留版 Python 产品的被测 CLI 范围，不证明 Rust CLI/TUI 已完全等价。
- [x] 两类 VSIX 在 `vscode-serial-cJRvr1` 分别隔离安装成功，CLI 集成退出后才开始。注册表各只有 `qwenpaw.qwenpaw-vscode` 0.2.0，来源为 VSIX；各 15 个执行文件与解包来源完全相同，平台包的 2 个资源文件也逐字节相同。通用包无 Core 资源，平台包清单限定 `darwin-arm64`。记录见本批 `vscode-installation.json`，未激活扩展或启动 Core。
- [x] 打包 WebUI + 源 Core 控制组：原 `/models` 页面浏览器检查通过（本批 `webui-browser.json`），随后原导航脚本 `--all` **24/24** 页面通过，均无失败 API、页面断言或浏览器错误（`webui-navigation-source-control.json`）。后者直接服务本批已逐文件核验的解包资产，仅增加 scratch 内启动夹具，未改原导航脚本或前端；自己的服务器结束后正常退出。导航加载不等于全部 CRUD/业务交互等价，也不代替分发 Core 启动。

本次 VSIX 检查首次在通用包已成功安装、执行文件已核对后，因为误断言 `TargetPlatform="universal"` 而失败。读取实际清单确认通用包没有 TargetPlatform 字段，平台包才声明 `darwin-arm64`；修正临时检查条件，复查原安装目录，不重新安装通用包、不改变清单或产物，然后才安装并核对 ARM64 包。机器报告的通用包 start/finish 是补做核验时间，不冒充首次安装时间。两次安装均有既有 Node `url.parse()` 弃用警告。
