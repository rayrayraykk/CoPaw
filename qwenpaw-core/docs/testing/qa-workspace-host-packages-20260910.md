# Workspace 宿主版本：九类 QA 制品

本机日期 2026-09-10；按 UTC 命名的输出目录为 `qwenpaw/dist/qa-runtime-20260909-VVMQCa`。这是 QA 构建与分层验证记录，不声明全功能、原生窗口或跨平台验收完成。

## 来源和源码验证

本批首次包含共用无页面 Workspace 构造器、基础目录/项目目录分离及首次项目选择持久化修复。Desktop 和 headless 不向 selected project 或重开时的 fallback 目录写模板；已有 default 绑定失效不自动修复，其他健康 Agent 可继续运行。具体失败回归见 [源码验收](checkpoint-workspace-ownership-acceptance.md)。

- [x] 最新普通 Rust 工作区 **653/653**：App Server **411/411，13.41 秒**、Core **126/126，2.98 秒**、HTTP **36/36，3.74 秒**。20 项显式测试另计，不算普通通过数。
- [x] 原页面/参考组 **20/20，277.89 秒，无跳过**；保留上轮备份往返超时，后文单独说明。
- [x] 原前端 **295 个测试文件、2453 项测试通过，63.46 秒**，生产构建通过；`console/src` 零改动。
- [x] 浏览器工具的确定性传输/退出测试 13 项及原诊断测试 3 项，合计 **16/16**；最终 fmt/diff 检查通过。Rust 严格 Clippy 仍以相同 Rust 源码上一轮 **11.00 秒**结果为证，本轮只修改浏览器脚本和文档，没有声称重跑了该命令或独立 Tauri crate 的严格检查。

Core source release SHA-256：`0dd0817f14d011bad42011de2b1c9b80cc04cb0ff5b7dc494fd580483faaa90b`。打包阶段使用该二进制，Cargo release 检查 **0.50 秒**；此前实际编译 **52.50 秒**。构建清单包含 **2,858** 个来源输入，source tree SHA-256：`190dbd08bb5f7f07d6395ae0ab094df5f2aaa60b54d25778be378163d46710c7`。当前工作区未提交，不能只用 Git commit 识别本批源码。

## 制品与静态核验

下列文件均相对于本批输出目录。大小和 SHA-256 逐项记录在 `build-manifest.json`、`SHA256SUMS`。

| 类型 | 文件 | 构建/静态核验 |
| --- | --- | --- |
| macOS ARM64 DMG | `QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg` | 通过 |
| Desktop ZIP | `QwenPaw-Tauri-2.2.0b5-macOS.zip` | 通过 |
| Rust Core | `qwenpaw-core-darwin-arm64-QA.tar.gz` | 通过 |
| 原 WebUI | `webui/qwenpaw-webui-2.2.0b5-QA.tar.gz` | 通过 |
| TypeScript SDK | `sdk/qwenpaw-sdk-0.2.0.tgz` | 通过 |
| Python SDK | `sdk/qwenpaw_sdk-0.2.0-py3-none-any.whl` | 通过 |
| 通用 VSIX | `vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix` | 通过 |
| macOS ARM64 VSIX | `vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix` | 通过 |
| 保留版 Python 产品 | `legacy/qwenpaw-2.2.0b5-py3-none-any.whl` | 通过 |

DMG **53,724,444 字节**，SHA-256 `2a6b89a508dd755af9f496fb98539704263a48a114fb4a25c3ffe250e0b975c2`。镜像验证、只读挂载、两份 App 签名完整性检查和正常卸载通过。独立 Core 与平台 VSIX Core 和 source release 字节相同；ZIP/DMG 签名后 Core 均为 `be13e71014926adbdc7464d45cdff774a4c20008f0e64996c54bbe801dd50d94`。

四份 Console 各 **1,311** 文件，树摘要 `931aaed10507a21a239153dd25db1c5bcf8dad1887b3a5fc1dfc81e1bce9e688`，与前一批相同。TS 18 文件、Python 5 个 Python 文件、每份 VSIX 15 个 JS 文件与来源一致；通用 VSIX 不带平台 Core。`static-inspection.json` 为 `passed: true`、`packagedRuntimeTested: false`，不证明包内程序能启动。

## 顺序隔离安装检查

隔离检查目录：`/private/var/folders/0s/4ht2q69j6sx49r64ktp8pssm0000gn/T/qwenpaw-qa-runtime-5znSwE/inspection-JkDi6T`。

- [x] TS tgz 在 `installed-sdk-WBBX9T/ts-install` 离线安装，禁用安装脚本。原编译测试按原层级复制，src 链接指向真实安装包，契约 fixture 链接指向原文档；断言真实包入口路径，未改测试断言。使用 source Core **4/4，0.074 秒，无跳过**。
- [x] Python wheel 在 `installed-sdk-WBBX9T/python-install` 离线无依赖安装，先断言导入路径，再以 source Core 执行原测试 **5/5，0.598 秒，无跳过**。报告 `installed-sdk-source-control.json` 明确不测试 packaged Core。
- [x] 两种 VSIX 在 `installed-clients-copGhv` 各自独立 user-data/extensions 目录安装，注册表仅有 `qwenpaw.qwenpaw-vscode` 0.2.0；每份 15 个 JS 与解包内容逐字节相同，平台 Core 两个资源文件也相同。仅安装和读取验证，**未激活扩展、未启动包内 Core**。
- [x] 保留版 wheel 离线无依赖安装到 `installed-clients-copGhv/legacy-install`；每组先断言实际包导入路径。conda qwenpaw 下依次通过 CLI **855/855，17.30 秒**和集成 **36/36，67.72 秒**，无跳过。工作区、secret、backup 均隔离，凭据使用测试进程的文件后端，不访问系统钥匙串。仅已有 audioop 弃用警告；这不是 Rust CLI/TUI 等价的证明。

安装阶段的开始/结束时间、独立目录及非重叠顺序记录在 `installed-sdk-source-control.json` 和 `isolated-client-installation.json`；详细原始输出在对应隔离目录中。

## 失败和未完成门禁

第一次 DMG 创建退出 1，原日志为 `hdiutil: create failed - 资源忙`。只读检查确认无残留创建进程、挂载或目标 DMG 后，同一 App 输入仅重试一次镜像创建成功。通过 `--after-desktop` 续建其余制品；没有重新编译或改签此 App 来绕过失败，`desktop-dmg.log` 和 `desktop-dmg-retry.log` 均保留。

上轮完整页面组 **19/20，433.45 秒**中的备份往返 180 秒超时没有足够阶段证据，至今不能确认原因。本轮先原样提取 DevToolsClient，确定性测试 **1 通过、4 失败**证明断连后请求悬挂和同步 send 失败残留；修复后拒绝/排空 pending，Browser.close 还须观察实际正常进程退出。它是单独已证明的验收工具缺陷，不能据此次 **20/20**将其追认为上次超时根因。

- [ ] 包内 Core 首次启动、Desktop 原生窗口/桥接/托盘与 VS Code 激活；仍需要 [设备管理员核查](core-startup-policy-investigation.md)，未重试被阻断程序或修改系统防护。
- [ ] 当前版本 Windows/Linux/macOS x64 实机及全部原生交互。
- [ ] 默认 CLI/SDK Workspace 接线、凭据优先级、正常退出时后台任务与检查点收尾。
- [ ] 38 个尚未注册 Rust 路由的前端调用点，以及已注册路由背后的未完整功能；前端源码不变不等于全部交互已经等价。
- [ ] 独立 Tauri crate 的既有严格 Clippy 问题及其他总计划门禁。

未清理用户尚未确认的缓存/旧包，未提交、推送或发布。构建仅刷新原脚本管理的 Tauri/VS Code 生成资源；旧候选和失败证据保留。
