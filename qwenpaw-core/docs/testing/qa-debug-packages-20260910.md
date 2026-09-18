# Debug 日志版本：逐项制品验收

日期：2026-09-10；本机 macOS ARM64 QA 制品，不是签名公证的正式发行版。
源码功能与原页面证据见 [Debug 验收](backend-debug-logs-acceptance.md)。

目录：[qa-runtime-20260910-ZPQQT0](../../../dist/qa-runtime-20260910-ZPQQT0)。
此批包含真实 Debug 文件日志、CLI tracing 和原页面交互支持；上一批
`qa-runtime-20260910-z8rImE` 保留不动，不包含这些新增功能。

## 构建与检查

常规构建一次成功，无 DMG 重试：Console 40.75 秒，Core staging 复用已验证
release（0.49 秒），Tauri Rust 构建 20.79 秒。没有为绕过启动失败而重签或
换路径执行任何分发包。只刷新构建脚本管理的 Tauri/VS Code staging，
未进行用户尚未确认的缓存/旧安装包清理。

| 制品 | 字节数 | 当前验收层级 |
| --- | ---: | --- |
| [DMG](../../../dist/qa-runtime-20260910-ZPQQT0/QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg) | 53,279,620 | 校验、只读挂载、签名验证、包内前端/Core 一致；已卸载，未启动 |
| [桌面 ZIP](../../../dist/qa-runtime-20260910-ZPQQT0/QwenPaw-Tauri-2.2.0b5-macOS.zip) | 49,407,299 | 解包、签名与内容一致；未启动 |
| [独立 Core](../../../dist/qa-runtime-20260910-ZPQQT0/qwenpaw-core-darwin-arm64-QA.tar.gz) | 15,512,258 | 解包后与 source release SHA 相等；没有执行包内 Core |
| [WebUI](../../../dist/qa-runtime-20260910-ZPQQT0/webui/qwenpaw-webui-2.2.0b5-QA.tar.gz) | 26,285,658 | 解包后与经过原页面验收的 Console 逐字节一致 |
| [TypeScript SDK](../../../dist/qa-runtime-20260910-ZPQQT0/sdk/qwenpaw-sdk-0.2.0.tgz) | 11,786 | 离线安装真实 npm 包，4/4 对照 source Core 通过 |
| [Python SDK](../../../dist/qa-runtime-20260910-ZPQQT0/sdk/qwenpaw_sdk-0.2.0-py3-none-any.whl) | 7,584 | 离线安装真实 wheel，5/5 对照 source Core 通过 |
| [通用 VSIX](../../../dist/qa-runtime-20260910-ZPQQT0/vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix) | 28,750 | 独立 user/extensions 目录安装，15 个 JS 一致；未激活 |
| [ARM64 VSIX](../../../dist/qa-runtime-20260910-ZPQQT0/vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix) | 16,051,933 | 独立目录安装，15 个 JS 和 2 个 Core 资源一致；未激活 |
| [保留的 Python 产品](../../../dist/qa-runtime-20260910-ZPQQT0/legacy/qwenpaw-2.2.0b5-py3-none-any.whl) | 37,098,657 | 离线安装；安装后 CLI 单测 855/855、集成 36/36 通过 |

完整摘要见 [SHA256SUMS](../../../dist/qa-runtime-20260910-ZPQQT0/SHA256SUMS)；
[构建清单](../../../dist/qa-runtime-20260910-ZPQQT0/build-manifest.json) 记录当前
dirty worktree 的 2,866 个输入，树摘要为
`434ccb57ca6fe1d2854e4e1200322283cc839655281a82b82a78ad0b5de3be1d`。

[静态检查](../../../dist/qa-runtime-20260910-ZPQQT0/static-inspection.json)
明确为 `passed: true, packagedRuntimeTested: false`：9 个制品摘要全部相等；
source Core 为 `67fa91219b2f5d5957aac5efdfb43f538f5305a84cbb98d6899e62fde91d2719`，
桌面 QA 签名 Core 为 `ba59353fda586bbc061af75305bdecbca71d4380ae812669884abe26bc8c13c3`。

四份前端内容（DMG、ZIP、WebUI、Python 产品 wheel）各 1,311 个文件，
树摘要仍为 `931aaed10507a21a239153dd25db1c5bcf8dad1887b3a5fc1dfc81e1bce9e688`，
与上一批相同。SDK 的 18 个 TS 输出、5 个 Python 模块及每份 VSIX 的
15 个 JS 也与对应源码输出一致。

## 安装后验证

- [x] TypeScript 安装时使用 offline/ignore-scripts，校验实际包入口；原
  4 项编译后的 SDK 测试全部通过，0 跳过，0.083 秒。
- [x] Python 使用 conda qwenpaw、no-index/no-deps 安装，校验实际导入来自
  新 wheel；5/5，0.608 秒，无跳过。
- [x] 两个 VSIX 各用独立目录，排除已有 VS Code IPC 环境；注册表均只有
  `qwenpaw.qwenpaw-vscode@0.2.0`，通用包无 Core，平台包 Core 资源完整。
- [x] 保留的 Python 产品实际导入安装后的 wheel，CLI 单测 **855/855，
  17.23 秒**，集成 **36/36，65.68 秒**，无跳过。只存在原 `audioop`
  弃用警告。使用临时 working/secret/backup 目录和进程内 file credential
  配置，不访问系统钥匙串，不修改用户默认配置。
- [x] 安装完成后再次校验 9 个制品摘要、三份验收报告成功、DMG 已卸载
  和 `console/src` 零修改，见 [最终检查](../../../dist/qa-runtime-20260910-ZPQQT0/final-verification.json)。

安装记录：[SDK](../../../dist/qa-runtime-20260910-ZPQQT0/installed-sdk-source-control.json)、
[VSIX 与 Python 产品](../../../dist/qa-runtime-20260910-ZPQQT0/isolated-client-installation.json)。

安装 SDK 使用的可执行文件始终是 `qwenpaw-core/target/release/qwenpaw-core`，
不是刚解包的二进制。这些结果证明 SDK 安装和源码服务互通，不证明分发包
启动。VSIX 仅安装和内容验证，未激活、未执行内置 Core。

## 尚未满足的完整目标

- [ ] 包内 Core 的真实启动；此前本机安全环境限制未解决，未绕过或重试。
- [ ] 原生 Desktop/WKWebView/bridge/tray 与 VS Code 激活交互。
- [ ] Windows/Linux/macOS x64 实际构建和运行。
- [ ] 总计划中其他未实现功能与所有原交互等价。

全局目标仍执行中；不能将本批“构建/静态/隔离安装通过”表述为“所有功能完成”。
没有 commit、push、公证发布或访问真实模型服务/系统钥匙串。
