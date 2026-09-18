# 默认 Console Agent Cron QA 制品（2026-09-09）

计划 §14.2.24.38，输出目录 `dist/qa-runtime-20260909-WmuXFt/`（相对产品仓库）。九类 macOS arm64 QA 制品已构建；这是默认 Console Cron 执行切片，不是全部原功能完成或正式签名/公证发布。旧包与测试日志保留。

## 构建与来源

DMG、桌面 ZIP、Core archive、WebUI archive、TS SDK、Python SDK、universal VSIX、darwin-arm64 VSIX、legacy wheel 均已生成，九项 SHA-256 校验通过。完整路径/文件摘要见输出目录的 `build-manifest.json` 与 `SHA256SUMS`。

来源工作树 dirty，HEAD `c526497784d9dec58151cb7aa1f2db02f8fcebe0`，记录 2798 个输入文件，树摘要 `6fa0effd490c7f9493a8ff38b00928adf1565dfc98170833393f64bd0f14b860`。验收后追加的文档不是构建输入；构建来源清单不能当作测试通过证明。

首次 DMG 创建报资源忙，确认无挂载且目标文件未生成后，同一命令重试成功；失败 `desktop-dmg.log` 和成功 `desktop-dmg-retry.log` 分开保留。随后 `--after-desktop` 继续其余构建，不重跑已完成的 Console/桌面编译。此前 generated sidecar 目录已保留到本机临时备份，不删除旧资源或用户数据。

源码 release、解包 Core 和平台 VSIX Core 摘要一致：`586d41084b04d352bce99dec6c11ce900ec56279d7da87aee060a2126d2a6ebe`。桌面包 Core 经 ad-hoc 签名，与同样签名处理的参考副本一致：`6ea3767f7b354d72b6a4817421b260c437a60eca03f435eb1fe8ecb135692c59`。

原 Console 1311 个文件在 production build、WebUI、桌面 ZIP/DMG 与 legacy wheel 中完全一致，树摘要仍为 `74ca39444b3b61637102f3ce163bfc9987c056ba4c4386897896f656097de976`；`console/src` 零 diff。

## 逐包验收

全部使用临时目录和本地模型 fixture；未使用生产模型 key，未启动读取日常 app data 的完整桌面窗口。

| 制品 | 首轮结果 | 同路径复测 |
| --- | --- | --- |
| DMG / ZIP | 只读挂载、CRC/解包、签名结构、资源比对通过；包内 Core 版本及安装 TS SDK 的图片/模型请求、重启历史通过 | 两处 Core 实际请求均通过 |
| Core archive | 版本探测约 11.092 秒后 SIGKILL，非主动超时；其 TS SDK 检查因前置门禁失败未执行 | 原文件、原路径版本及 TS SDK 图片/历史通过 |
| WebUI | 解包 Core 托管的 Chrome 原 Models 页通过，报告明确记录实际 Core 路径 | 无需重复该已通过场景 |
| TS SDK | 隔离安装通过；桌面两处与平台 VSIX Core 的模型/图片/历史通过 | 四处包内 Core 均通过 |
| Python SDK | 隔离安装通过；5 项中 2 项 App Server 连接关闭失败，源码 Core 对照 5/5 通过 | 同一安装 SDK 对原解包 Core 5/5 通过 |
| 两类 VSIX | VS Code CLI 隔离安装均通过；平台版包内客户端 Thread CRUD 通过；universal 对 archive Core 检查因原生门禁失败未执行 | 两类包内客户端 Thread CRUD 均通过 |
| legacy wheel | 隔离安装、版本、TUI help、855 项 CLI/TUI 单测及 36 项 CLI 集成通过 | 不以 legacy 通过推断 Rust 等价 |

首轮 `qualification.json` 仍为失败：`python-sdk-packaged-core` 与 `package-smoke` 非零。后续完整复测另存 `recheck/`，四处包内 Core、两种 SDK 与两类 VSIX 客户端通过，包含单独列出的源码对照共 10 次模型请求。复测的挂载、package smoke、Python SDK 与卸载四项均为 0；没有改变失败二进制字节、路径、权限、签名、隔离标记或系统保护设置。

## 未解决的问题与验收边界

- 独立 Core 冷启动 SIGKILL 重现，复测成功不等于修复。只读系统日志可见该路径的 AMFI `has no CMS blob?` / `Unrecoverable CT signature issue`；成功执行的平台 VSIX 路径也出现同类日志，因而不能只据这些日志判断失败的唯一根因或保证重签名可修复。未清理隔离标记、关闭系统保护或要求生产凭据。
- 源码普通 Rust 438/438、严格 App Server Clippy、TS SDK 4/4、VS Code 57/57 通过；不是本次包内测试计数。原 Agent Cron UI 的真实工具往返在隔离 Rust 测试 fixture 中完成；包内 SDK smoke 验证图片/模型/历史，不冒称包内 Cron 浏览器全场景已实跑。
- 显式浏览器/调度参考首轮 10/11，OAuth 页面重载停在 Loading Console。独立复测 1/1、随后整组 11/11（188.46 秒）通过；首次失败与 Console 资源重建时间重叠，相关性尚不足以确定原因。
- 多 Agent Cron 归属、外部 Channel、完整 CLI/TUI/remote 等价、其余原功能门禁仍未完成。默认 Console 成功不覆盖临时非默认 Agent 501 限制。
- Chrome 不代替 Tauri WKWebView、桌面桥接、窗口/托盘生命周期或完整 GUI；生产签名/公证、首次安装稳定性与 Windows/Linux 最新实机安装态仍待验收。

临时测试服务与 DMG 挂载结束后检查无残留。制品及日志保留；本切片未提交、推送或上传发布，整体目标继续进行。
