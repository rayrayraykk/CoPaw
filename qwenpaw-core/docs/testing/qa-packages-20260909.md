# 2026-09-09 九类 QA 制品验收

此报告记录 §14.2.24.25 的历史源码快照，不包含后续模型运行时修复。更新包见 [最新模型运行时 QA 制品](qa-runtime-packages-20260909.md)，不要将下面的“当前源码”理解为后续工作树。

## 结论

九个当前源码分发文件已构建，但**不能标记为全部验收通过**。macOS 本机编译目录中的 Core 可以运行，解包后的 Core 在本机原生启动检查中被系统终止。SDK/VSIX 客户端对照测试通过，不能替代包内 Core 的执行验收。原前端 `console/src` 零修改，旧分发包保留。

输出目录：`dist/qa-20260909-poJEFp/`（相对于产品仓库根目录）。这是独立的 QA 输出，不是正式发布，也未推送或上传。当前环境为 macOS 26.3.2 / arm64；不代表 Windows/Linux 新版安装态已经验收。

## 逐包结果

| 文件 | 已验证 | 尚未通过或尚未验证 |
| --- | --- | --- |
| `QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg` | 构建、DMG 校验、只读挂载、包内签名结构、Rust-only 资源、Console 全文件比对 | 挂载包内 Core 启动失败；完整 Tauri GUI/原生桥接及正式公证未通过 |
| `QwenPaw-Tauri-2.2.0b5-macOS.zip` | 解包、签名结构、Core 与相同签名变换的源构建比对、Console 完整比对 | 解包后 Core 启动失败；不能交付为已验证可运行桌面包 |
| `qwenpaw-core-darwin-arm64-QA.tar.gz` | 解包、可执行权限、版本对应源码、完整 SHA-256 与源 release 二进制相同 | 解包后 `--version`/stdio 启动被系统终止，SDK 握手失败 |
| `webui/qwenpaw-webui-2.2.0b5-QA.tar.gz` | 解包后的全部静态文件与原 production build 相同；由可运行的源 release Core 提供服务，真实 Chrome 原 Models 页导航通过 | 此包不含 Core；不把源 Core 对照当成分发 Core 已通过 |
| `sdk/qwenpaw-sdk-0.2.0.tgz` | 安装到临时目录；包内 SDK 连接源 release Core，完成模型 SSE Turn 和数据库重开历史恢复 | 连接分发 Core 的测试因原生启动失败而未通过 |
| `sdk/qwenpaw_sdk-0.2.0-py3-none-any.whl` | 临时安装；导入路径确认为安装目录；使用源 release Core 的 4 项测试通过 | 单独运行包内 Core 对照时出现 `TransportClosedError`，不沿用前一次短时通过结果 |
| `vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix` | 独立扩展目录真实安装；包内入口/客户端可加载；包内协议客户端连接源 Core 完成 Thread 创建、列表和归档 | universal 包需要外部 Core；本轮搭配分发 Core 的执行验收未通过 |
| `vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix` | 独立扩展目录安装；目标/版本/QA manifest、内置 Core 摘要、包内 executable resolver 和协议客户端对照通过 | 内置 Core 启动失败；未将对照的源 Core 假装为内置 Core |
| `legacy/qwenpaw-2.2.0b5-py3-none-any.whl` | 隔离源码构建与临时安装；Console 全文件相同，CLI 版本及 TUI help 正常；855 项 CLI/TUI 单测 + 36 项 CLI 集成测试通过 | 这是保留原 Python 产品的 legacy wheel，不是 Python SDK，也不是 Rust 功能等价的证明 |

四份 Console（WebUI archive、ZIP、DMG、legacy wheel）共 1311 个文件逐一比对，摘要树一致。九个发布文件的 `SHA256SUMS` 全部通过。`build-manifest.json` 标记 `dirty: true`，记录基线 commit、2646 个输入文件的摘要树与九个产物摘要；不能只用未包含当前改动的 `c5264977` 当作构建来源。

## macOS 原生执行问题

参照 [Apple 的可信执行诊断说明](https://developer.apple.com/forums/thread/706442)，分别检查签名结构、系统分发策略和实际执行，三者不互相替代：

- `codesign --verify --deep --strict` 对解包 App 通过；Core archive 与源 release 文件摘要一致。
- 重新对源 Core 的临时副本执行与桌面包相同的 ad-hoc 签名后，摘要与 ZIP/DMG 内 Core 一致；排除了只因外层签名变换造成摘要差异的误报。
- 本机源 release Core 的 `--version`、SDK Turn 和重开数据库通过。
- `/tmp`、系统临时目录和独立仓库 QA 安装目录中的解包 Core 均出现启动终止；没有修改这些产物的内容来掩盖失败。
- 系统日志包含 `Unrecoverable CT signature issue` 和 ad-hoc 签名拒绝；`syspolicy_check distribution` 报告 `Notary Ticket Missing`。这些是已观察到的签名/分发问题，不宣称仅凭该检查已解释所有启动失败。
- 没有关闭 Gatekeeper/AMFI、修改系统安全策略、清除隔离标记或读取生产签名私钥。正式 Developer ID 签名/公证及干净机器验收仍需发布配置；本轮不能声称 ad-hoc QA 包在任意 macOS 可直接运行。

## 可复跑证据

输出目录包含：

- `build-manifest.json`、`source-inputs.json`、`SHA256SUMS`：构建来源与文件完整性；
- `package-smoke.json` / `.log`：从实际解包和安装目录加载代码，原生失败与源 Core 对照分开记录；有失败时命令返回非零；
- `webui-browser.json`：解包 WebUI 的原 Models 页浏览器结果；
- `python-sdk-source-control.log`：已安装 Python SDK 对源 Core 的测试；
- `legacy-tests.log`、`legacy-integration-tests.log`：已安装 legacy wheel 的 891 项测试。

复跑脚本位于 `qwenpaw-core/scripts/release/`。`qa-package-smoke.mjs` 接收此前解包并安装 SDK 的临时根目录、产品仓库根目录和 QA 输出目录；每轮新建隔离运行目录，不使用用户日常工作区。`write-qa-manifest.mjs` 只记录来源和摘要，不把构建成功标为功能通过。

测试临时根目录为 `/tmp/qwenpaw-release-20260909-J9JDw9`。只读 DMG 挂载和临时 HTTP 服务在验收结束后释放；临时 SDK/VSIX 安装目录保留用于排查，不修改用户现有扩展和产品安装。
