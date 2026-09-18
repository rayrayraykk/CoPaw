# 图片运行时九类 QA 制品（2026-09-09）

输出目录：`dist/qa-runtime-20260908-fRyKH7/`，相对于产品仓库根目录。保留旧包，不重标记或覆盖旧发布。全部九个文件已构建、解包并检查；本次仅 macOS arm64 QA，不是正式签名/公证发布。

包含 Gemini 原生 GenerateContent、模型页图片/视频探测、实际聊天图片快照及 SDK 协议更新（计划 §31–33）。源码基线：396 项 Rust 普通测试、7 项原浏览器门禁通过；原 Console 生产构建本轮重新执行。源码测试不替代包内执行，下表是实际分发文件的结果。

## 制品与实际检查

| 制品 | 结果与边界 |
| --- | --- |
| `QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg` | CRC、只读挂载、签名结构、原资源比对通过；挂载内 Core 的版本、SDK 握手、图片推理请求、重启与不可变历史通过；完整 Tauri GUI/桥接未验收 |
| `QwenPaw-Tauri-2.2.0b5-macOS.zip` | 解包、签名结构、资源比对通过；包内 Core 完成相同图片/重启验收；完整 GUI 未验收 |
| `qwenpaw-core-darwin-arm64-QA.tar.gz` | 首次执行失败；同一解包文件未修改的复测通过版本、握手、图片请求、重启历史。首次启动稳定性仍未关闭 |
| `webui/qwenpaw-webui-2.2.0b5-QA.tar.gz` | 解包资源一致；首次由源码 Core 提供对照，复测实际由解包 Core 托管并通过 Chrome 原 Models 页 |
| `sdk/qwenpaw-sdk-0.2.0.tgz` | 临时安装；针对四处包内 Core 执行图片请求、重启、修改原文件后继续发送原快照，复测全部通过 |
| `sdk/qwenpaw_sdk-0.2.0-py3-none-any.whl` | 临时安装；首次对 archive Core 的两个启动测试失败，源码对照 5/5 通过；同路径 archive Core 复测 5/5 通过，包含图片链路 |
| `vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix` | 独立扩展目录真实安装；包内协议客户端与解包 Core 的 Thread CRUD 复测通过；不含自带 Core，不是全插件 UI 验收 |
| `vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix` | 独立安装、QA manifest 和内置 Core 选择/摘要通过；内置 Core 的图片/历史及实际包内客户端 Thread CRUD 通过 |
| `legacy/qwenpaw-2.2.0b5-py3-none-any.whl` | 隔离安装；版本/TUI help、855 项 CLI/TUI 单测及 36 项 CLI 集成测试通过；仍是独立 Python 产品，不证明 Rust 功能全部等价 |

以上模型请求只访问本地 fixture 并使用假 key，不作为真实云账号或真实视觉模型质量的证据。

## 字节与来源证据

`SHA256SUMS` 九项通过。构建 manifest 记录 dirty 工作树、2786 个输入文件及构建时摘要，不只依赖尚未包含本机变化的 Git HEAD。原前端在新 production build、DMG、ZIP、WebUI 和安装的 legacy wheel 中共有 1311 个相同文件；与上一组 QA 的树摘要一致：

`74ca39444b3b61637102f3ce163bfc9987c056ba4c4386897896f656097de976`

源码 release、解包 Core archive、平台 VSIX Core 的 SHA-256 均为：

`af4cc2d9a3a6e7f88e16076a0a12bd75f745fbe2e42e90f484c0ca06ba50d392`

桌面包的 Core 经正常 ad-hoc 签名后与同样签名的参考副本一致：

`6abce8bb52778ee01094a003daf998f43146fc01e96b4323eb77ea064de1a473`

DMG 首次创建报告“资源忙”，确认无挂载、空间足够且目标文件未生成后，同一 HFS+ 命令重试成功。首次 `desktop-dmg.log` 保留，不覆盖失败。

## 首次执行与同路径复测必须分开看

原始 `qualification.json`、`native-execution.json`、`package-smoke.json` 和 Python 日志保留首次失败。新诊断器分别记录 exit code、signal、是否由测试主动结束、输出限额及持续时间；三项测试覆盖成功/错误版本、非零退出、自然 SIGKILL、测试超时、文件缺失和输出溢出。

首次新 archive Core 约 10.951 秒后收到 SIGKILL，45 秒测试超时没有触发。同期另外三处包内 Core 在约 15.75 秒返回版本并完成图片/历史测试。archive 与成功的 VSIX Core 字节相同。

随后对 archive 的同一路径执行 `--version`，未更改文件、签名、权限或系统配置，约 219 毫秒正常返回。再运行完整包内检查，四处 Core 的版本探测均通过（约 7–14 毫秒），四组 SDK 图片请求/重启、两类实际 VSIX 客户端、Python SDK 5/5 和解包 WebUI+解包 Core 均通过。

复测结果另存在 `recheck/native-execution.json`、`recheck/package-smoke.json`、`recheck/webui-browser.json`；后者明确记录实际 Core 路径。Python 复测命令与输出为本轮工具执行记录的 5/5。WebUI 验收脚本在构建后增加了显式 Core 路径参数，仅影响测试驱动，不修改制品。

该现象说明执行结果有首次/后续差异，但不足以确定系统缓存、签名评估或终端防护的具体因果。部分相似系统签名警告也出现在成功的源码测试中；不把它们单独当作根因。没有清除隔离/来源标记、关闭安全组件、换名隐藏程序或使用生产签名私钥。

## 未关闭门禁

- 首次安装/执行稳定性：首次失败未被解释或修复，不能把复测成功写成全新机器必定可运行。
- 完整 Tauri GUI 与原生桥接：本轮没有启动日常应用数据目录下的桌面实例，包内 Core 验收不等于窗口内全部交互验收。
- Apple Developer ID 签名/公证、Windows/Linux 最新安装态、真实账号和计划中剩余原功能。

临时测试服务已退出，DMG 已卸载。制品和隔离验收目录保留。未上传发布、未提交推送，总体目标保持进行中。
