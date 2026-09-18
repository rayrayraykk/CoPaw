# 插件加载与工具校验：九类开发快照

后续更新：管理读取修复已进入 [CE3oB2 新开发快照](qa-plugin-manager-packages-20260915.md)。下文保留本批 `VQYCTN` 的历史构建与验收记录，不代表最新包。

继续用户已批准的“所有产物构建并逐个测试、原前端交互保持”目标。本次将前端插件真实加载与工具列表 Agent 校验修复纳入新的 macOS ARM64 Core 和九类 QA 制品。不是全功能正式发布；原生 Desktop、VS Code 激活、包内 Core 执行以及跨平台门禁仍未通过，不绕过安全限制。

## Checklist

- [x] 复核最新默认并发 Rust 847 项、原页面/参考 33 项、原前端 2453 项的来源；校验旧九类制品与暂存，两处暂存移至控制目录下 `staging-before-build`，可恢复。这些测试是前两轮证据，不计为本轮重跑。
- [x] 使用当前源码重新构建原 Console、release Core、DMG/Desktop ZIP/Core archive/WebUI/TypeScript SDK/Python SDK/两种 VSIX/保留版 Python wheel；全新输出 `dist/qa-runtime-20260914-VQYCTN`，使用既有 QA ad-hoc 签名和离线 Rust 依赖，未发布。
- [x] 九类文件、2930 条来源及签名完整性核对通过；四份原 Console 各 1311 文件逐文件一致；DMG 已只读检查并正常卸载，未执行包内 Core。
- [x] 新 source Core 的 Rust SDK 集成 3/3，优化版 App Server 的原插件页面/对照 2/2；隔离安装新 SDK/两种 VSIX/保留版 wheel 并完成下列测试，没有启动原生窗口或激活扩展。
- [x] 当前结果、SHA-256 与下载路径已汇总；最终来源/新旧制品/保留暂存复核通过，旧包可恢复，首次 DMG 失败与未完成项保留。
- [ ] 全功能、历史 SDK EOF/浏览器时序、原生和跨平台最终验收；不可随本批开发快照勾选。

控制日志目录：`dist/qa-delivery-plugin-20260915-EoFvZk`。新制品目录由现有构建脚本创建并写入 `locations.json`。本轮不修改产品源码、默认凭据策略或原前端；仅产生构建及 QA 证据。

## 已保留的构建失败

首次 DMG 创建报“资源忙”，流水线退出 1；Desktop App、QA 签名和 ZIP 此时已成功。只读确认没有部分 DMG、本批挂载或 hdiutil 进程后，以原参数重试一次成功，再使用既有 `--after-desktop` 续建。没有重建已完成的 Desktop、没有执行被拒绝的包内程序、没有更改安全策略；失败、预检、重试与续建记录均保留。

## 构建来源

release Core 构建 55.53 秒，source Core SHA-256 为 `70bd954627d5413ab9d6390995e552ff64d763588fc286b5a04fd425b4e5814e`；Core archive 与 ARM64 VSIX 内字节相同。DMG/Desktop ZIP 内签名后的 Core SHA-256 均为 `a6fe24923d821ab3db68c20ca064e6e3ccb71629374ef4f1965f2aa6385cc3cc`，这是签名与内容检查，不是原生运行通过。

来源树 SHA-256：`12c34bdfcdd2373703620b23d85479129eb7ac140b45078a1a8bad30c09de487`，含 2930 条记录。四份 Console 内容树仍为 `931aaed10507a21a239153dd25db1c5bcf8dad1887b3a5fc1dfc81e1bce9e688`，源码与既有 Console 脚本未改。

## 下载清单

均为本机 QA 开发制品。DMG/App 未做 Developer ID 签名或公证，也未完成原生启动验收。

| 制品 | 文件 |
| --- | --- |
| macOS ARM64 DMG | [下载](../../../dist/qa-runtime-20260914-VQYCTN/QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg) |
| macOS Desktop ZIP | [下载](../../../dist/qa-runtime-20260914-VQYCTN/QwenPaw-Tauri-2.2.0b5-macOS.zip) |
| macOS ARM64 Core | [下载](../../../dist/qa-runtime-20260914-VQYCTN/qwenpaw-core-darwin-arm64-QA.tar.gz) |
| 原 WebUI | [下载](../../../dist/qa-runtime-20260914-VQYCTN/webui/qwenpaw-webui-2.2.0b5-QA.tar.gz) |
| TypeScript SDK | [下载](../../../dist/qa-runtime-20260914-VQYCTN/sdk/qwenpaw-sdk-0.2.0.tgz) |
| Python SDK | [下载](../../../dist/qa-runtime-20260914-VQYCTN/sdk/qwenpaw_sdk-0.2.0-py3-none-any.whl) |
| Universal VSIX | [下载](../../../dist/qa-runtime-20260914-VQYCTN/vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix) |
| macOS ARM64 VSIX | [下载](../../../dist/qa-runtime-20260914-VQYCTN/vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix) |
| 保留版 Python 产品 | [下载](../../../dist/qa-runtime-20260914-VQYCTN/legacy/qwenpaw-2.2.0b5-py3-none-any.whl) |

[SHA256SUMS](../../../dist/qa-runtime-20260914-VQYCTN/SHA256SUMS)、[构建清单](../../../dist/qa-runtime-20260914-VQYCTN/build-manifest.json)、[静态检查](../../../dist/qa-runtime-20260914-VQYCTN/static-inspection.json)。DMG SHA-256：`b21fba73bca5d852060db163fe37f226fa79cf7328a2797400a1d7dc4ad1040a`。

## 逐项运行证据

- 优化版 App Server 插件专项：2/2，3.95 秒；新编译的 release 测试程序使用隔离凭据存储，真实原 App Center 完成打开/交互/刷新/返回/重开，并对照原 Python 的十个响应。不启动带系统凭据存储的 CLI Desktop，也不代表 DMG 运行验收。
- Rust SDK 对新 source Core：3/3，0.13 秒；复用对应未改测试源码的既有 release 集成测试程序，实际启动本批新 Core，验证连接、关闭时保存 interruption、最终写入失败传播。未把它描述为重新编译的 Rust SDK 分发包。
- 隔离安装 TypeScript tgz：26/26，无失败/跳过/取消，33.18 秒；连接新 source Core stdio/WS/WSS。
- 隔离安装 Python wheel：37/37，无跳过，37.72 秒；使用 `python -S` 确认从新安装目录导入 SDK 和 websockets 15.0.1，并运行同批 TS/Python 共享连接测试。安装内容、依赖与 README 均匹配源码。

SDK 日志位于新制品目录 `installed-sdk`。客户端记录位于 `installed-clients`：

- VS Code 源码 73/73；Universal 与 ARM64 VSIX 分别安装到独立 user-data/extensions 目录，安装内容、平台声明、资源摘要均核对，两者安装后指定组件测试各 24/24。未激活扩展、未启动包内 Core，也不将组件测试称为完整 VS Code UI 验收。
- 新保留版 wheel 隔离安装后，CLI 单测 855/855，集成 36/36；JUnit 确认无失败、错误或跳过，实际导入路径在新安装目录。使用独立测试数据目录，不代表 Rust CLI/TUI 已实现全部旧功能。
- [最终证据复核](../../../dist/qa-runtime-20260914-VQYCTN/final-verification.json) 于 2026-09-14T19:35:42.170Z 通过：2930 条构建来源、13 条额外相关测试/代码证据、新旧各九类文件、原前端/既有脚本、保留暂存及安装测试结果重新核对。首次 DMG 失败没有被覆盖。

## 未完成边界

本轮不把上一轮普通 Rust 847、完整显式组 33、原前端 2453 宣称为重新执行；它们的来源仍匹配，本轮新增的是优化版插件与新制品的相关验收。历史 SDK EOF 超时/浏览器时序、包内 Core 运行、原生 Desktop、VS Code 激活、Windows/Linux/macOS x64、真实凭据及外部 Channel、插件后端/安装管理等剩余功能继续开放，不能由本批构建或静态检查代替。

旧 `6BiWlu` 九类文件和本轮移动的暂存均保留。当前推荐核对和下载的是 `VQYCTN` 这批开发快照；没有 commit、push 或对外发布。总 goal 未完成。
