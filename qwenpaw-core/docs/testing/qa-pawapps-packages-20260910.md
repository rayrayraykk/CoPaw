# PawApps 目录版本：九类 QA 制品

日期：2026-09-10。输出目录：`qwenpaw/dist/qa-runtime-20260910-z8rImE`。
来源为已完成 [源码回归](pawapps-directory-acceptance.md) 的版本。
source Core SHA-256：
`150924c9a22e7505835615ba32f7e06689be68e6c9e558b32a756d0a502277a9`。

本批是本机 macOS ARM64 QA 制品，不是正式签名发布，也不声明全部功能等价。
未确认清理范围，旧输出保留；只刷新构建脚本原本管理的 Tauri/VS Code 资源。

## Checklist

- [x] 独立构建九类产物并生成来源、SHA256SUMS 清单。
- [x] 校验所有制品摘要、来源输入、解包资源，DMG 只读挂载和签名完整性检查。
- [x] TS/Python SDK 离线隔离安装后，使用 source Core 运行原测试，不运行包内 Core。
- [x] universal/darwin-arm64 VSIX 独立 user-data/extensions 目录安装，不激活扩展。
- [x] legacy wheel 离线隔离安装，原 CLI/集成测试使用临时工作区和文件凭据后端。
- [ ] 包内 Core 启动及真实 Desktop/VS Code 交互：仍待设备管理员核查，不重试受阻程序。
- [ ] Windows/Linux/macOS x64 当前版本构建与实机验证。

## 范围边界

普通构建允许新 QA App 的 ad-hoc 签名及其静态校验，不修改系统防护，
不重签已受阻的旧程序来规避启动限制。不执行完整 qualifier 的 runtime probe。
源码中新增的是 PawApps 目录 API；插件注册表/加载执行与动态前端 UI、默认
CLI/SDK Workspace 接线等仍未完成。安装成功、静态检查和 source Core 测试
不能代替包内启动或完整交互验收。

## 制品与来源

文件均相对于上述输出目录，逐项大小和 SHA-256 见 `build-manifest.json`
和 `SHA256SUMS`。构建工作区未提交，不能仅凭 Git commit 识别来源。

| 制品 | 文件 |
| --- | --- |
| macOS ARM64 QA DMG | `QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg` |
| Desktop ZIP | `QwenPaw-Tauri-2.2.0b5-macOS.zip` |
| Rust Core | `qwenpaw-core-darwin-arm64-QA.tar.gz` |
| 原 WebUI | `webui/qwenpaw-webui-2.2.0b5-QA.tar.gz` |
| TypeScript SDK | `sdk/qwenpaw-sdk-0.2.0.tgz` |
| Python SDK | `sdk/qwenpaw_sdk-0.2.0-py3-none-any.whl` |
| 通用 VSIX | `vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix` |
| macOS ARM64 VSIX | `vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix` |
| 保留版 Python 产品 | `legacy/qwenpaw-2.2.0b5-py3-none-any.whl` |

来源输入 **2860** 文件，树摘要：
`03330f30704566103a61cf3b4097567e4c8016aa377129beab51668a2e2911dd`。
DMG **53,745,564 字节**，SHA-256：
`214e1b1651193e433e4c7eaa118dd46318e14f21909728f5694836562a21a1c6`。
Core archive 和平台 VSIX 中的 Core 与 source Core 字节一致；两份 Desktop
签名后 Core 均为 `118667e0ac04a2de2f224f9d09822ef62efc91c152ff9ba0abb78cbe67e5b851`。
四份 Console 均为 **1311 文件**，树摘要
`931aaed10507a21a239153dd25db1c5bcf8dad1887b3a5fc1dfc81e1bce9e688`，与上一批相同。
TS 18 文件、Python 5 个 Python 文件、每份 VSIX 15 个 JS 文件均与来源一致。
`static-inspection.json` 为 `passed: true`、`packagedRuntimeTested: false`；
DMG 正常卸载，未运行其中的程序。

## 隔离安装结果

检查根目录：
`/private/var/folders/0s/4ht2q69j6sx49r64ktp8pssm0000gn/T/qwenpaw-qa-runtime-jdwtBe/inspection-LNnHH6`。

TS/Python 安装目录为 `installed-sdk-MOQNRJ`。TS 离线禁用安装脚本，原测试
保持目录关系，src 指向真实安装包并断言入口路径；**4/4，0.078 秒**，无跳过。
Python wheel 离线无依赖安装，断言真实导入路径后执行原测试
**5/5，0.605 秒**，无跳过。两组仅连接上文 source Core，报告为
`installed-sdk-source-control.json`。

两份 VSIX 已隔离安装并校验唯一扩展注册记录、15 个 JS 和平台 Core 资源；
剔除 VS Code IPC 环境变量，不连接日常 VS Code，不激活扩展。
安装根目录为 `installed-clients-hpsBUQ`。legacy wheel 离线无依赖安装后，
每组先断言导入真实安装包，按顺序通过原 CLI **855/855，17.51 秒**和集成
**36/36，67.49 秒**，无跳过。只出现原 audioop 弃用警告。工作区、secret、
backup 都指向本次临时目录，测试进程用文件凭据后端，不访问系统钥匙串。
这证明保留版 Python 产品的这些测试通过，不是 Rust CLI/TUI 全等价的证明。

各阶段非重叠开始/结束时间和路径记录在 `isolated-client-installation.json`；
未激活扩展、未运行包内 Core。安装后再次核验全部九个制品的大小与摘要通过，
无相关 DMG 挂载或 Cargo/Core/hdiutil 残留进程；`console/src` 仍为零 diff。
未 commit、push 或发布。

## 构建失败保留

首次 DMG 创建返回 `hdiutil: create failed - 资源忙`。检查确认创建进程已结束、
无相关挂载、目标 DMG 不存在后，同一份 App 仅重试一次镜像创建成功。
随后 `--after-desktop` 续建剩余制品，没有重新编译或改签以绕过失败。
`desktop-dmg.log` 与 `desktop-dmg-retry.log` 均保留。source release 检查复用
已验收二进制，Cargo **0.42 秒**；此前真实源码编译 **52.45 秒**。
