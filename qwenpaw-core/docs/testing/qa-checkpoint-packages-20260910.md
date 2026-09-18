# 检查点运行时版本：九类 QA 制品验收

本机日期：2026-09-10（UTC+8）。承接计划 §14.2.24.53；生成脚本使用 UTC 日期，因此本批目录仍含 `20260909`。本记录不是全功能或安装态全通过声明。

版本边界：本批是检查点第三、四步版本。之后新增的第五步 Core Thread 排空 API 及 App Server 恢复协调接入均不在这些制品内；下述构建输入相符结果是本批验收时的证据，不表示后续工作树仍与该清单相同。新代码的源码回归单独记在检查点实施记录，不覆盖本批原文件或摘要。

## 范围与清单

保持原 `console/src`，沿用已确认的 Rust Core / App Server / SDK 架构。本轮只完成当前版本的构建和分层验收，不删除旧失败样本、不发布、不接入真实密钥、不修改终端防护策略。

- [x] 第三、四步生产修复进入新的九类 macOS ARM64 QA 制品。
- [x] 九个文件校验和、解包内容及来源核对。
- [x] DMG 镜像校验、只读挂载、ZIP/DMG App 签名完整性检查，检查后正常卸载。
- [x] 已安装 Python SDK 的完整 5 项测试，以及已安装 TypeScript SDK 的初始化/创建会话专项；明确连接源码 Core 控制组。
- [x] 已安装旧版 wheel 的 CLI 单测 855/855。
- [x] 已安装旧版 wheel 的 CLI 集成测试 36/36；两类 VSIX 隔离安装及安装后执行文件比对。
- [ ] 分发 Core 首次启动、完整原生窗口、实际 VS Code 激活和打包 WebUI 运行验收。
- [ ] 最新 Windows/Linux 构建与实机交互、完整原功能矩阵。

## 制品与来源

输出：`qwenpaw/dist/qa-runtime-20260909-uzKETn`。
暂存：`/var/folders/0s/4ht2q69j6sx49r64ktp8pssm0000gn/T/qwenpaw-qa-runtime-bq3QS8`；本次解包/安装位于其 `inspection-Kt8tXI` 子目录，不使用日常应用的数据目录。

| 类型 | 输出目录内的文件 |
| --- | --- |
| Desktop DMG | `QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg` |
| Desktop ZIP | `QwenPaw-Tauri-2.2.0b5-macOS.zip` |
| 独立 Core | `qwenpaw-core-darwin-arm64-QA.tar.gz` |
| 原 WebUI | `webui/qwenpaw-webui-2.2.0b5-QA.tar.gz` |
| TypeScript SDK | `sdk/qwenpaw-sdk-0.2.0.tgz` |
| Python SDK | `sdk/qwenpaw_sdk-0.2.0-py3-none-any.whl` |
| 通用 VSIX | `vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix` |
| macOS ARM64 VSIX | `vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix` |
| 保留的 Python 产品 | `legacy/qwenpaw-2.2.0b5-py3-none-any.whl` |

`build-manifest.json` 与 `SHA256SUMS` 保存九个文件的大小和完整摘要；`shasum -a 256 -c SHA256SUMS` 为 **9/9 OK**。记录的 2,842 个构建输入随后逐文件重验全部相符，未重写清单：源树摘要 `2f744025f0fdb4147ffae2349cb1f51760c8639c70f5ce18ef2375f319824a25`。Git 为 dirty，不能仅用提交号代表这些产物。

源 release Core SHA-256：`f930cc2103980ec0c6e6876c031ea3670e1bf4f8c856004d56ad24916bad719f`。独立 tar 与平台 VSIX 中的 Core 与其字节相同。标准 QA ad-hoc 签名后的 ZIP/DMG Core 彼此相同，摘要为 `ee02a93bde74b7b0b941a30517c56ce2695505bb4bc26410857cbb1f8aa90c7f`；不把已签名副本误称为与源文件逐字节相同。

## 静态检查与前端不变

WebUI tar、ZIP App、DMG App 和旧版 wheel 内 **1,311** 个 Console 文件均与本次 `console/dist` 逐文件相同。实际文件树摘要为 `74ca39444b3b61637102f3ce163bfc9987c056ba4c4386897896f656097de976`，机器结果见输出目录 `static-inspection.json`。

TypeScript 包 18 个构建文件、Python 包 5 个 `.py` 文件，以及两类 VSIX 各 15 个执行文件与来源一致。通用 VSIX 不包含 Core 可执行文件。VSIX 沿用 `.vscodeignore` 的 `out/**/*.map` 排除规则，未改变打包策略。

这些结果证明前端内容未被打包过程替换，不证明每个未实现的后端行为已等价。源码实际原页面测试的覆盖与缺口见 [检查点实施验收](checkpoint-workspace-ownership-acceptance.md)。

## 已安装客户端测试

Python 使用 conda `qwenpaw`，Node 使用 24.18.1。wheel 通过 `pip --no-index --no-deps --target` 安装至独立目录；TypeScript tgz 通过 `npm --offline --ignore-scripts --prefix` 安装。真实导入路径已断言位于本次安装目录，未误用 editable 源码包。运行环境过滤密钥、代理和产品配置，Core 数据使用临时目录。

- Python SDK **5/5，0.606 秒**：模拟协议/断流、协议契约、真实源码 App Server 建会话、图片快照及后续复用；无跳过。真实模型请求仅使用本地 fixture。
- TypeScript SDK：实际安装包初始化和 `thread/start` **1/1**，比较 idle/archived 完整选定结构，关闭客户端。未宣称这等于源码 4 项测试全部在安装包上复验。
- 旧版 wheel CLI 单测 **855/855，17.25 秒**；仅一个既有 `audioop` Python 3.13 弃用警告。该结果证明旧版 Python CLI 保留，不证明 Rust CLI/TUI 已完全替代。
- 旧版 wheel CLI 集成 **36/36，70.24 秒**；同样先断言导入路径来自新安装目录。
- 通用与 macOS ARM64 VSIX 分别安装成功。两个独立 `extensions.json` 均只有 `qwenpaw.qwenpaw-vscode` 0.2.0，来源为 VSIX；安装后的执行文件、平台包资源与解包来源逐字节一致。安装器有既有 Node `url.parse()` 弃用警告，未启动扩展窗口或 Core。

首次通用 VSIX 安装开始时 CLI 集成检查尚未结束，两者有短暂重叠，不能把首批报告成完全串行验收。随后已补做：等待所有检查终止后，旧 wheel CLI 集成 **36/36，66.94 秒**；确认其退出后，在新建 `inspection-Kt8tXI/vscode-serial-TPD20o` 下先安装通用 VSIX，确认退出后再安装 ARM64 VSIX，均成功。两者注册表和实际安装执行文件/平台资源再次核对一致。串行补验不启动包内 Core，也不抹去首次时序记录。

上述 SDK 均明确连接 `target/release/qwenpaw-core`，不是安装包 Core 运行通过。

## 保留的失败与边界

1. 首次 DMG 创建返回 `hdiutil: create failed - 资源忙`；只读检查无挂载后，同一输入的镜像创建仅重试一次成功。保留 `desktop-dmg.log` 和 `desktop-dmg-retry.log`，随后从 `--after-desktop` 继续构建；未重新编译/改签来掩盖此次镜像错误。
2. 首次 payload 检查误将源码目录的 `.js.map` 当作 VSIX 必须内容而失败。读取既有排除规则后修正检查期望，复查执行代码通过；未放宽 Console/SDK 逐文件比较，未修改扩展源码或打包规则。
3. 本轮没有启动新旧包中的 Rust Core，也未运行会重新签署参考副本并调用原生启动探针的完整 qualifier。旧串行 SIGKILL 与终端防护非白名单标签关联仍未取得处置信号来源的直接证据，见 [策略核查材料](core-startup-policy-investigation.md)。静态签名验证不等于系统允许执行，源码控制组通过也不关闭此依赖。
4. 安装后注册表检查首次假定 `metadata.targetPlatform` 存在而失败；实际两个注册表都不保存此字段。改为核对 VSIX manifest 的显式平台属性、注册表身份/版本/来源以及实际安装文件，均通过；不修改安装元数据来迎合断言。

当前源码普通测试 583/583、显式组 19/19、严格 Clippy、release 与 TS/Python/VS Code 源码客户端结果见检查点实施记录；不重复计算成安装态通过。恢复静默期、所有自动快照生产入口、可配置策略、其他未完成 API 及跨平台范围仍保留在总计划，目标未完成。
