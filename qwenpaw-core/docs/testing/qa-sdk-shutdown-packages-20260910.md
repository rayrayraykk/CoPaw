# stdio 与三语言关闭改造后的 QA 制品

日期：2026-09-10，平台 macOS ARM64。批次
[`qa-runtime-20260910-WXFYc3`](../../../dist/qa-runtime-20260910-WXFYc3)。
包含 stdio 宿主退出及本轮 SDK 变更，旧 `vGGX8Z` 和单独 SDK 包均保留。
当前工作区有未提交修改，不是已发布 commit 的干净构建。

## 来源

- source release Core SHA-256：
  `e630c45861b71c1b97c0e222d7a5a8cdcf3a1968e3defc44b129e6d61f3c4860`。
- 桌面包内签名后的 Core SHA-256：
  `87120f4fe547ee1f60b232da866edd2ea01bf23797117424cf1d6d5d68a8d5e4`。
- 2882 个来源条目树摘要：
  `f02e4321ac65b85dd650bdb62c1cd70dcbf2744073f86516905eeb873401bd09`。
- Console production build 40.74 秒，Core staging 缓存构建 0.44 秒，
  Tauri release build 21.32 秒。使用 QA ad-hoc 签名，无生产签名/公证。
- 首次 DMG 创建报“资源忙”；原构建进程已结束，`hdiutil info` 无挂载、
  无残留相关进程且无半成品目标后，原参数重试成功。两次日志均保留。
  没有重试包内 Core 启动或修改系统安全策略。

## 九类文件与逐项验收

全部文件的字节数和 SHA-256 与
[`build-manifest.json`](../../../dist/qa-runtime-20260910-WXFYc3/build-manifest.json)
及 [`SHA256SUMS`](../../../dist/qa-runtime-20260910-WXFYc3/SHA256SUMS) 核对通过。

| 制品 | 字节数 | 本批已完成检查 |
| --- | ---: | --- |
| `QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg` | 53,680,004 | 镜像校验、只读挂载、App 深度签名及资源比对；已卸载 |
| `QwenPaw-Tauri-2.2.0b5-macOS.zip` | 49,311,279 | 解包、App 深度签名、与 DMG 内容比对 |
| `qwenpaw-core-darwin-arm64-QA.tar.gz` | 15,414,612 | 解包 Core 与 source release 字节一致，未执行包内 Core |
| `webui/qwenpaw-webui-2.2.0b5-QA.tar.gz` | 26,285,313 | 解包后全部前端资源与 production build 一致 |
| `sdk/qwenpaw-sdk-0.2.0.tgz` | 13,113 | 实际离线安装、导入路径和文件摘要校验，9/9 测试 |
| `sdk/qwenpaw_sdk-0.2.0-py3-none-any.whl` | 9,621 | 实际离线安装、导入路径和文件摘要校验，16/16 测试 |
| `vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix` | 28,750 | 独立 profile 安装、15 个 JS 文件一致，无 Core 资源 |
| `vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix` | 15,951,497 | 独立 profile 安装、15 个 JS 和 2 个 Core 资源一致 |
| `legacy/qwenpaw-2.2.0b5-py3-none-any.whl` | 37,098,657 | 安装后导入路径确认；CLI 单测 855/855、集成 36/36 |

TypeScript 安装后测试 30.401 秒，Python 32.650 秒，均包含真实 30 秒关闭
超时和 Core 重开前的存储检查，均无跳过；连接的是上述 **source Core**，
不是解包 Core。保留版 CLI 单测 17.32 秒、集成 67.78 秒，无跳过；保留
已有 `audioop` 弃用警告，未修改旧产品代码。VSIX 仅安装，不激活原生扩展。
安装及测试时间戳证明顺序为 TS、Python、两类 VSIX、legacy 单测、集成，
没有用并发启动代替逐项检查。

## 前端与验收边界

`console/src` 零 diff。DMG、ZIP、WebUI、legacy wheel 四份 Console 各
1311 个文件，树摘要均为
`931aaed10507a21a239153dd25db1c5bcf8dad1887b3a5fc1dfc81e1bce9e688`，
与此前批次相同。本轮没有重新运行原前端 2453 项和完整浏览器/参考 26 项；
最近的相应源码验收见 [stdio 宿主记录](stdio-host-lifecycle-acceptance.md)。
文件一致性和既有专项通过不能证明所有原产品交互已经等价。

- [x] 九类构建、来源条目、解包内容、签名完整性和校验和。
- [x] `static-inspection.json`、`installed-sdk-source-control.json`、
  `isolated-client-installation.json` 均为 passed，且明确未执行包内 Core。
- [x] `final-verification.json` 再核对九个文件、2882 条来源、三个报告及
  source Core 摘要；检查安装步骤无时间重叠、测试镜像已卸载、无 source
  Core 残留进程、前端零 diff 和空白检查通过。
- [ ] 包内 Core 首次启动、Desktop/WebKit 原生交互、VS Code 激活。
- [ ] Windows/Linux/macOS x64 实机构建与完整运行，生产签名/公证。
- [ ] 默认 CLI/SDK Workspace 初始化、后台单实例及最终保存失败传播，
  其余原产品全功能交互。不能把本批构建和控制组通过称为全功能发布验收。

未执行清理、commit/push 或发布；缓存及历史制品仍等待用户确认清理范围。
