# 最终保存失败与历史刷新修复：整套 QA 制品

日期：2026-09-10，macOS ARM64。承接已确认的各端构建/逐项验收方案，
源码门禁见 [最终保存验收](final-turn-persistence-acceptance.md)。上一轮已
通过 Rust 708、原前端 2453、浏览器/参考 27 及 release/源码客户端检查。

本批以 source release Core SHA-256
`19e4548ad6401da74e13490d25a3995c3332da3a1e2542cbf193a1dde5b11092`
为基线。旧 `WXFYc3` 使用 `e630c4...`，不包含最终保存失败及历史刷新修复；
SDK 专项 `TJWoJ3` 连接 `51ba72...`，不代替本批组合验证。

## 执行清单

- [x] 新目录顺序构建九类：DMG、桌面 ZIP、Core tar、WebUI tar、TS SDK、
  Python SDK、通用 VSIX、ARM64 VSIX、保留版 Python 产品 wheel。
- [x] 校验九个文件及来源清单；只读挂载 DMG、核对签名完整性，并比较
  四份原 Console、SDK/扩展载荷与本批 source Core；不执行分发 Core。
- [x] 实际离线安装 TS/Python SDK，分别运行当前 10/17 项全套；确认
  真实导入目录和载荷一致，连接本批 source Core，无跳过。
- [x] 两种 VSIX 安装到各自隔离配置目录，不激活扩展；保留版产品 wheel
  离线安装后顺序验证既有 CLI 用例，避免调用日常环境中的源码副本。
- [x] 最终复核各文件哈希、来源未变化、执行顺序、无遗留 source Core 进程或本批挂载，
  并同步主计划、架构与验收结果。
- [ ] 分发 Core 真正启动、桌面原生交互、VS Code 激活及跨平台仍需独立
  验收，不用本批构建/静态检查追认这些项。

## 构建与静态核对

本批目录：`dist/qa-runtime-20260910-2qPEew`（产品仓库根目录）。
前端生产构建 39.36 秒、Core 暂存构建 0.49 秒、Tauri release 构建 21.17 秒。
源码 Core 仍为上述 `19e454...`，桌面/扩展的生成暂存资源已从 `e630c4...`
更新，旧输出目录保留。

DMG 首次 `hdiutil create` 报“资源忙”，构建已终止；确认无挂载、无本批
构建进程、无目标 DMG 半成品后，仅以相同参数重试一次成功。首次日志
`desktop-dmg.log`、检查记录 `desktop-dmg-preflight.log` 和重试日志/时间戳
`desktop-dmg-retry.log` / `.json` 均保留；随后使用既有 `--after-desktop`
继续其余制品，没有重跑或改签名绕过 Core 启动，资源忙根因未确认。

- 九个文件的大小/SHA-256 与 `build-manifest.json`、`SHA256SUMS` 一致。
- 2886 条来源记录均匹配，来源树 SHA-256：
  `38c6e64b91371ec0b59d55ab37181f93890e5aea364780c3c68c950151e25fb5`。
- Core tar / ARM64 VSIX 内 Core 与 source Core 一致；DMG 与 ZIP 中签名后
  Core 一致，SHA-256：
  `9a89b9b20e7d4815a86f39d96e71f3b7a5f5b07178e66fb3977adc4887e5538d`。
  这是签名完整性与构建来源核验，不是运行证明。
- WebUI、DMG、ZIP、保留版 wheel 四份 Console 均为 1311 个相同文件；
  载荷 SHA-256：
  `931aaed10507a21a239153dd25db1c5bcf8dad1887b3a5fc1dfc81e1bce9e688`，
  与上一批一致。TS SDK 18 个生产文件、Python 5 个模块、两份 VSIX 各
  15 个生产 JS 文件均匹配源载荷，通用 VSIX 没有夹带平台 Core。
- `static-inspection.json` 为 passed，记录只读 DMG 校验/挂载/签名检查及
  成功卸载；`packagedRuntimeTested=false`。

## 安装态顺序验收

实际离线安装后的结果如下；没有跳过用例。SDK 测试连接上述 source release
Core，不执行 tar、DMG 或 VSIX 中的 Core。

| 制品 | 实际检查 | 结果 |
| --- | --- | --- |
| TS SDK tgz | 安装目录导入、18 个生产文件比对、全套测试 | 10/10，30.395 秒 |
| Python SDK wheel | conda qwenpaw 中隔离安装、实际导入位置及 5 个模块比对 | 17/17，33.356 秒 |
| 通用 VSIX | 独立 user-data / extensions 目录安装，15 个 JS 文件相同 | 成功；不含 Core，未激活 |
| ARM64 VSIX | 另一独立目录安装，15 个 JS 文件及 2 个 Core 资源比对 | 成功；未激活 |
| 保留版 Python wheel | 离线安装后运行原 CLI 单元与集成测试，禁用源码路径注入 | 855/855（17.37 秒）＋36/36（68.27 秒） |

SDK 安装检查记录在 `installed-sdk-source-control.json`，扩展及保留版安装
检查在 `isolated-client-installation.json`。CLI 结果证明保留版 Python 产品
的安装态，不表示原 CLI/TUI 的所有功能已经迁入 Rust。

`final-verification.json` 于 2026-09-10 06:08:48 UTC 通过：九个制品的
大小/哈希、2886 条来源、source Core 哈希以及安装命令顺序和退出状态均匹配；
本批无遗留 DMG 挂载、无 source Core 进程，原前端源码无变更。
`packagedRuntimeTested=false`、`vscodeActivated=false`、`cleanup=false`。
首次 DMG 失败及一次原参数重试记录均保留，不将重试通过解释为根因已修复。

## 制品入口

九个文件均位于产品仓库的 `dist/qa-runtime-20260910-2qPEew`，完整大小和
SHA-256 见该目录 `build-manifest.json` 与 `SHA256SUMS`。

- 桌面：`QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg`、`QwenPaw-Tauri-2.2.0b5-macOS.zip`。
- Core：`qwenpaw-core-darwin-arm64-QA.tar.gz`。
- WebUI：`webui/qwenpaw-webui-2.2.0b5-QA.tar.gz`。
- SDK：`sdk/qwenpaw-sdk-0.2.0.tgz`、`sdk/qwenpaw_sdk-0.2.0-py3-none-any.whl`。
- VS Code：`vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix`、`vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix`。
- 保留版：`legacy/qwenpaw-2.2.0b5-py3-none-any.whl`。

## 边界

后续 source Core 诊断发现同目录多宿主缺陷：另一个 Core 启动时会把仍在
运行的 Turn 写成 interrupted。见 [复现与待确认的进程归属方案](../architecture/default-workspace-host.md)。
本批隔离安装测试没有覆盖这个场景，原静态报告仍有效，但不能据此推荐
多个 Core 进程同时打开同一数据目录；该缺陷尚未修复。

沿用正常 QA ad-hoc 构建流程，不为绕过既有启动失败重新签名或修改系统策略；
不运行完整 qualifier 或包内运行 smoke。仅源码 release Core 可作为 SDK
运行对照。正式 Developer ID/公证仍未完成。

构建会替换已核对的生成暂存资源，不删除旧输出、源码、未提交改动或测试
报告；用户提出的“清理”范围仍待确认。本批不使用真实 key/keychain、不
操作日常 App 数据、不 commit/push/发布，不启用 subagent。
