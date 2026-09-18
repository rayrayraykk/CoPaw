# SDK 配置版本：九类 QA 制品

本机日期 2026-09-10；输出目录按 UTC 命名为 `qwenpaw/dist/qa-runtime-20260909-rWHV5J`。本记录区分构建、安装和运行验收，不声明全功能或跨平台完成。

## 来源与边界

包含完成钩子、真实保存回执、SDK 受跟踪完成消费者及本次每 Agent runtime/provider/usage owner 修复；保留 SDK 显式 Thread 模型。不迁移旧 Python 数据，不修改 `console/src`。

普通工作区 **637/637**，最后测试补强后的 App Server **395/395**复验，严格 Clippy **14.83 秒**；原页面/参考串行 **20/20，320.09 秒**。source release **55.59 秒**，Core SHA-256：`7c0ac37f17bb0a9df7297939bd9b56b21ed229549bf83acf31d2cedc20c63bf1`。之后 TS **4/4**、Python **5/5**、VS Code 编译及 **57/57**顺序通过；详细范围见 [源码验收](checkpoint-workspace-ownership-acceptance.md)。

本批来源清单包含 **2,856** 个输入，source tree SHA-256：`112349b6f4593c10fe1f46db5b926f3614759b9b6d9adae28e6ee3758b4c4331`。这是 dirty 构建，不能仅凭 Git commit 识别源码。旧 `qa-runtime-20260909-EWMbMd` 等包及失败样本原字节保留。

## 制品

下列文件均相对于本批输出目录；大小和 SHA-256 逐项记录在 `build-manifest.json` 与 `SHA256SUMS`。

| 类型 | 文件 | 构建与静态核验 |
| --- | --- | --- |
| Desktop DMG | `QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg` | 通过 |
| Desktop ZIP | `QwenPaw-Tauri-2.2.0b5-macOS.zip` | 通过 |
| Core | `qwenpaw-core-darwin-arm64-QA.tar.gz` | 通过 |
| 原 WebUI | `webui/qwenpaw-webui-2.2.0b5-QA.tar.gz` | 通过 |
| TS SDK | `sdk/qwenpaw-sdk-0.2.0.tgz` | 通过 |
| Python SDK | `sdk/qwenpaw_sdk-0.2.0-py3-none-any.whl` | 通过 |
| 通用 VSIX | `vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix` | 通过 |
| macOS ARM64 VSIX | `vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix` | 通过 |
| 保留版 Python 产品 | `legacy/qwenpaw-2.2.0b5-py3-none-any.whl` | 通过 |

- [x] 九文件校验和与 2,856 个来源输入逐项核验。
- [x] DMG 镜像验证、只读挂载、ZIP/DMG 签名完整性及正常卸载。
- [x] 四份 Console 的 **1,311** 个文件相同，树摘要 `931aaed10507a21a239153dd25db1c5bcf8dad1887b3a5fc1dfc81e1bce9e688`，也与上一批相同。TS 18 文件、Python 5 个 Python 文件、两类 VSIX 各 15 个 JS 执行文件与来源相符。
- [x] 独立 tar/平台 VSIX Core 与 source release 一致；ZIP/DMG 签名后 Core 一致，摘要 `4da7119f2129729472d794d7da2b8947ab9ee9f192bead3dd61be7638394c9db`。
- [ ] 包内 Core 首次启动、原生窗口、VS Code 激活与打包 WebUI 完整运行。
- [ ] 最新 Windows/Linux 实机、Rust CLI/TUI 全功能及其他未完成原功能。

`static-inspection.json` 为 `passed: true`、`packagedRuntimeTested: false`。未启动包内 Core、重试被阻断程序或改动终端安全策略；已有 SIGKILL 仍需 [设备管理员核查](core-startup-policy-investigation.md)。镜像/签名完整性不证明系统允许执行。

## 实际安装后的顺序验证

隔离目录：`/private/var/folders/0s/4ht2q69j6sx49r64ktp8pssm0000gn/T/qwenpaw-qa-runtime-VSb1p2/inspection-oxGhUu`。SDK 显式连接本次 source release，而不是包内 Core。

- [x] TS tgz 离线安装到 `installed-ts-7G0DvK`；导入实际安装的 dist/src，原编译测试按原目录层级复制，契约文档链接回原夹具，未改断言。**4/4，1.006 秒，无跳过**。
- [x] Python wheel 在 conda qwenpaw 离线无依赖安装到 `installed-python-2zSKKS`，断言导入路径属于安装目录；原测试 **5/5，0.604 秒，无跳过**。
- [x] 两类 VSIX 在 `installed-vscode-2CWZAF` 分别隔离安装，注册表各只有 `qwenpaw.qwenpaw-vscode` 0.2.0；每份 15 个 JS 和平台包 Core 资源与解包来源逐字节相等，通用包无 Core。`vscode-installation.json` 记录非重叠时间；未激活扩展或运行 Core。仅既有 Node url.parse 弃用警告。
- [x] 保留版 wheel 离线无依赖安装到 `installed-legacy-KocqNF/package`，每组测试开始前断言实际导入路径。CLI 单测 **855/855，17.94 秒**，随后集成 **36/36，70.51 秒**，全部无跳过。临时工作区/secret/backup 目录及 file-backed 凭据隔离，不访问日常数据；仅已有 audioop 弃用警告。这证明 legacy 包被测范围，不证明 Rust CLI/TUI 等价。
- [x] 本批解包 WebUI 配合 source Core 控制组 **24/24** 页面导航通过，无失败 API/页面断言；报告 `webui-navigation-source-control.json` 显式标注 `packagedRuntimeTested: false`。复用原导航脚本，scratch 启动夹具只改变本批输出/解包路径，未修改页面或断言；临时源服务器结束后退出。导航加载不证明所有 CRUD/业务交互完整等价。

## 保留的构建失败

首次 DMG 创建退出 1：`hdiutil: create failed - 资源忙`。只读检查确认无残留创建进程、挂载或目标 DMG 后，同一输入仅重试一次创建成功；保留 `desktop-dmg.log` 与 `desktop-dmg-retry.log`。后续通过 `--after-desktop` 继续构建，未重编译、移动或改签既有失败样本。

保留版 CLI 首次测试调用误带当前 pytest 环境未提供的 `--no-cov`，测试尚未收集便退出 4；去掉多余参数后复用同一安装目录执行上述完整测试，没有安装或调整依赖、修改测试断言。

构建仅刷新 Tauri/VS Code 管理的可重建资源；未进行用户尚未确认的缓存或旧包清理，未提交、推送或发布。
