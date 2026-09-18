# Responses 与桌面生命周期 QA 制品（2026-09-09）

计划 §14.2.24.37。输出目录为产品仓库的 `dist/qa-runtime-20260909-OPX10W/`；旧目录保留。包含 §35 的桌面生命周期修复和 §36 的 Rust 原生 Responses；本次仅 macOS arm64 QA，不是正式签名/公证发布。

## 构建记录

九类制品全部构建完成，`SHA256SUMS` 九项通过：DMG、桌面 ZIP、Core archive、WebUI archive、TypeScript SDK、Python SDK、universal VSIX、darwin-arm64 VSIX、legacy wheel。

DMG 首次创建报告“资源忙”，目标文件未生成，`hdiutil info` 确认无挂载。同一 HFS+ 命令重试成功，之后使用构建器的 `--after-desktop` 继续剩余制品；首次 `desktop-dmg.log` 保留，不覆盖。重试不说明该系统错误的原因已修复。

`build-manifest.json` 记录 dirty 工作树、2790 个输入文件，来源树摘要为 `2a68555fef83d00c5edd89cb83f02e6a03a371f461091f191513cd4f57de7847`。这是构建来源证据，不是验收通过声明。

统一验收驱动现显式把解包 Core 传给 WebUI 浏览器检查，不默认使用源码 Core。原生执行诊断器 3/3 测试、驱动语法检查及 `git diff --check` 通过；`console/src` 零 diff。

## 安装态验收

使用隔离目录、本地模型 fixture 和假 key，不读取日常桌面数据或生产模型凭据。

- legacy wheel 临时安装后的版本、TUI help、855 项 CLI/TUI 单测和 36 项 CLI 集成测试通过。测试从安装目录导入产品，保留 Python 3.13 将移除 `audioop` 的已有警告；这不是 Rust 功能等价证明。

| 制品 | 首轮结果 |
| --- | --- |
| DMG / 桌面 ZIP | CRC/解包、只读挂载、签名结构、资源比对通过；包内 Core 版本、安装的 TS SDK 握手、图片请求、重启历史与原快照复用通过；完整 GUI 未运行 |
| Core archive | 原生版本探测 SIGKILL，未执行依赖该门禁的 TS SDK 检查；同路径后续单次版本探测通过，复测另存 |
| WebUI archive | 解包资源一致；由解包 Core 托管的 Chrome 原 Models 页通过，`webui-browser.json` 明确记录该 Core 路径 |
| TypeScript SDK | 临时安装成功；DMG/ZIP/平台 VSIX 内三个 Core 的图片/历史实际请求通过；独立 Core 路径留待复测 |
| Python SDK | 临时安装成功；连接解包 Core 的 5 项测试中 2 项启动连接关闭失败，源码 Core 对照 5/5 通过 |
| 两类 VSIX | VS Code CLI 隔离安装均成功；平台版内置 Core 选择与实际包内客户端 Thread CRUD 通过；universal 客户端对独立 Core 的检查因版本门禁失败而不执行，源码对照通过 |
| legacy wheel | 隔离安装、原 Console 资源、版本与 891 项 CLI/TUI 测试通过，不作为 Rust 等价证据 |

原 Console 1311 个文件在新 production build、WebUI、DMG、ZIP、legacy wheel 中完全一致，树摘要仍为 `74ca39444b3b61637102f3ce163bfc9987c056ba4c4386897896f656097de976`。

源码 release、解包 Core archive 与平台 VSIX Core 的 SHA-256 一致：`76c1903956bffce66c2080b7f7d3f99891ed90ed3868280528afb61cc900561d`。桌面包 Core 经过正常 ad-hoc 签名，与相同签名处理的参考副本一致：`8ad1c7a6e7f67e4d84242939e2616af385ace8cda6668e8a184f99b8be1463a4`。

## 首轮失败与同路径复测

首轮 `qualification.json` 整体未通过：Python SDK packaged-Core 和 package-smoke 非零退出。独立 archive Core 的版本探测约 11.096 秒收到 SIGKILL，`timedOut:false`、`terminatedByProbe:false`；同期另外三处包内 Core 约 15.736–15.738 秒返回正确版本并通过实际图片/历史请求，源码 Core 对照约 224 毫秒成功。源码和 VSIX 与失败的 archive Core 字节相同。

同一解包 archive 文件随后单次版本探测约 6 毫秒成功。没有重签名、改变字节/路径/权限、清除隔离标记或关闭系统保护。这只能证明前后执行结果不同，不能解释或宣称修复 SIGKILL。

完整复测单独保存在 `recheck/`，不覆盖首次日志：四处包内 Core 版本探测全部通过（5–10 毫秒），安装的 TS SDK 分别完成图片请求、重启历史与原快照复用，两类包内 VSIX 客户端 Thread CRUD 均通过；加上单独列出的源码对照共收到 10 次本地模型请求。安装的 Python SDK 对同一 archive Core 复测 5/5 通过（0.584 秒）。

`recheck/qualification.json` 的重新挂载、package-smoke、Python SDK 与卸载四项均为 0。原始 `qualification.json` 仍保留非零项；不能将复测成功改写为首次安装已通过。临时服务已退出，`hdiutil info` 无残留挂载，按本轮隔离目录匹配的测试进程为空。制品和测试目录保留，未提交、推送或上传发布。

## 边界

- 源码基线为 Rust 普通测试 402/402、桌面 debug/release 各 68/68；不是本次包内测试数量。上轮 MCP 取消与备份浏览器首次失败、独立复测通过的记录仍保留。
- 包内 SDK 图片链路检查的是 Chat Completions；Responses 的原生工具/图片/历史验收见[源码验收](responses-runtime-acceptance.md)，不能把文件摘要一致称为包内 Responses 已实跑。
- 原 Models 页的 Chrome 检查不能代替 WKWebView、桌面桥接、托盘、窗口退出或全部业务交互。
- 首次安装稳定性、Apple Developer ID 签名/公证、Windows/Linux 最新实机安装态、真实账号和其他原功能仍需完成。
