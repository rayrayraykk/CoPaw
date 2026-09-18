# 最新模型运行时 QA 制品（2026-09-09）

本组来自 §14.2.24.26–29 后的当前工作树，包含 Ollama 地址转换、请求选项、Anthropic 原生 Messages、Agent 模型路由和 turn 配置隔离。原前端源码未改，重新运行原 production build 后打包。源 Core 基线为 375 项普通 Rust 测试、6 项原页面浏览器门禁与 SDK/VS Code 回归通过；这些源码测试不替代分发包验收。

输出目录：`dist/qa-runtime-20260908-yv7Wee/`（产品仓库根目录下）。目录日期按 UTC，实际本地构建日期为 9 月 9 日。保留上一组 `dist/qa-20260909-poJEFp/`，不重标记旧包。本组仅 macOS arm64 QA，不是正式签名发布，也不代表 Windows/Linux 新版安装态通过。

## 构建与证据

九个文件已生成，`SHA256SUMS` 全部通过。`build-manifest.json` 标记 dirty 工作树并记录 2772 个来源文件；来源包括当前代码、打包脚本、Console 资源、文档资源和图标，不只依赖未包含本机改动的 Git HEAD。

| 制品 | 本轮检查状态 |
| --- | --- |
| `QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg` | 创建、CRC、只读挂载、App 签名结构和资源比对通过；包内 Core 握手失败，完整 GUI/原生桥接未通过 |
| `QwenPaw-Tauri-2.2.0b5-macOS.zip` | 解包、签名结构、原前端/Core 内容比对通过；包内 Core 握手失败，不能标记可运行 |
| `qwenpaw-core-darwin-arm64-QA.tar.gz` | 解包后与最新源 Core 摘要相同；stdio 握手失败，独立 `--version` 被 SIGKILL |
| `webui/qwenpaw-webui-2.2.0b5-QA.tar.gz` | 解包资源完全一致，源 Core 托管下真实 Chrome Models 页通过；该包不含 Core，不替代分发 Core 验收 |
| `sdk/qwenpaw-sdk-0.2.0.tgz` | 临时安装后对源 Core 的 SSE Turn、重开数据库恢复通过；对分发 Core 握手失败 |
| `sdk/qwenpaw_sdk-0.2.0-py3-none-any.whl` | 临时安装后对源 Core 4/4 测试通过；对解包 Core 的真实握手测试失败（其余 3 项通过） |
| `vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix` | 独立目录真实安装，包内协议客户端对源 Core 的 Thread CRUD 通过；搭配分发 Core 握手失败 |
| `vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix` | 独立安装、QA manifest、内置 Core 摘要及 executable resolver 通过；客户端源 Core 对照通过，但内置 Core 启动失败 |
| `legacy/qwenpaw-2.2.0b5-py3-none-any.whl` | 隔离构建、临时安装、版本/TUI help、855 项 CLI/TUI 单测和 36 项 CLI 集成测试通过；这是保留的 Python 产品，不证明 Rust 全功能等价 |

首次 DMG 创建报告“资源忙”；无遗留挂载，磁盘空间足够。显式使用 HFS+ 重新创建后成功，已通过镜像校验。没有清除隔离标记、关闭 Gatekeeper/AMFI 或读取生产签名私钥。

ZIP、DMG、WebUI 和安装后 legacy wheel 的原 Console 共 1311 个文件逐一相同；摘要树也与上一组 QA 完全一致（`74ca3944…de976`），不是仅主页截图相似。源 Core SHA-256 为 `139ea2be505c54aa2ec2d542365d5b41f3eed565e55258383246576fa9a59350`；桌面包经同样 ad-hoc 签名变换后的 Core 与签名参考副本相同（`e62798e7…4e452`）。这证明本轮更新进入制品，不证明系统允许其执行。

本轮 10 秒限时的独立 `--version` 探测曾超时，由探测器发 SIGTERM；延长 Core archive 探测上限至 30 秒后，观察到系统 SIGKILL，stdout/stderr 均为空。源码目录中的同一 Core 可执行；不把探测器超时误报为系统拒绝，也不把源码对照替代包内失败。沿用此前未关闭的 macOS 原生分发门禁，未新宣称仅缺公证就能解释全部现象。

WebUI 首次验收因脚本假 shutdown token 少于后端要求的 16 字节而提前退出。这是测试脚本错误，已修正并独立重跑通过，结果在 `webui-browser.json`；原 `qualification.json` 保留先前测试结果，不覆盖失败历史。临时服务器已停止，DMG 已卸载。源码 Core 对照使用隔离配置和假 key，仅访问本地服务。

## 复跑

在 `qwenpaw` conda 环境中，将 Node 24+ 加入 PATH，从 `qwenpaw-core` 运行：

```sh
node scripts/release/build-qa-macos.mjs
node scripts/release/qualify-qa-macos.mjs <新建的 QA 输出目录>
```

构建脚本每次新建输出和临时目录，只有桌面构建确已完成时才可用 `--after-desktop <目录>` 继续其余包。构建不会调用清空公共 dist 的 legacy wheel 脚本，不上传发布，显式使用 ad-hoc QA 签名。

验收脚本从真实解包与安装目录运行测试，记录 `qualification.json` 和逐项日志；`package-smoke.json` 将包内 Core 和源 Core 对照结果分开。失败时返回非零，DMG 最后卸载。临时目录见输出中的 `locations.json`，不使用日常工作区或已有扩展目录。完整桌面 GUI、系统签名/公证、真实账号和全功能等价仍需独立证据，不能从文件摘要或版本检查推断完成。
