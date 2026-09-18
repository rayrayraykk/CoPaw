# 三语言连接态：整套 QA 制品

日期：2026-09-14，macOS ARM64。继续已确认的九类构建及逐项验收；本批纳入
WS Close 回复修复、VS Code owned shutdown，以及 TS/Python 显式 WS/WSS SDK。
不迁移旧数据、不修改原 Console 源码、不切换默认宿主生命周期。

## 执行 checklist

- [x] 检查现有生成暂存资源和源 Core，在全新目录依次构建 DMG、桌面 ZIP、
  Core tar、WebUI tar、TS SDK、Python SDK、通用/ARM64 VSIX、保留版 wheel。
- [x] 核对九个制品、来源输入和签名完整性；只读挂载 DMG，四份原 Console
  逐文件一致；不运行任何包内 Core。
- [x] 从本批实际包隔离安装 TS/Python SDK 及声明依赖；顺序执行完整 26/37
  项（无跳过），Python 禁用 site 并包含与本批已安装 TS 的混合连接用例。
  测试仅调用 source Core；不使用旧 checker 中已过时的 10/17 项计数。
- [x] 两种 VSIX 分别隔离安装但不激活，运行可隔离的扩展单元回归；保留版
  wheel 安装后顺序跑 CLI 单元/集成，禁止源码 pythonpath 注入。
- [x] 最终重新核对来源、制品、测试终态、原前端零 diff，以及本批挂载/
  测试进程清理，更新制品入口和主计划；失败日志原样保留。
- [ ] 分发 Core 真正启动、原生 GUI/扩展激活、跨平台和全交互仍单独验收。

构建仅更新已核对的生成暂存资源，正常 QA ad-hoc 签名不等于正式签名/公证。
本批不因历史包内 Core 被系统终止而改变签名、路径或安全策略以绕过问题。
不接触真实 key、系统凭据、日常应用数据，不清理旧包/缓存，不 commit/push。

## 本批来源与静态证据

输出：[qa-runtime-20260914-737ANs](../../../dist/qa-runtime-20260914-737ANs)。
Console 生产构建 44.33 秒，Core 暂存构建 0.49 秒，Tauri release 21.22 秒。
本批 source Core 为 `fafbeadf3275494099e3f342b65dcd901c919bb4425fe2795d1a2df0cd4f5cf1`；
桌面正常 QA 签名后的 Core 为 `cd61283e54811cb69a4adae998a3cfdd36517cfca1fd2a8b69eb9ad4112f3798`。
Core tar/ARM64 VSIX 与 source Core 字节一致；签名检查不等于可运行或公证。

`build-manifest.json` / `SHA256SUMS` 记录九个文件，`source-inputs.json` 记录
2901 条来源，来源树 SHA-256 为
`447d03d248e9239bbbb88b1e0a6d707ffa61920be5962c2f5427765ca30cc1b9`。
`static-inspection.json` 核对四份 Console 各 1311 文件、TS 21 个文件、Python
七个模块、两份 VSIX 各 16 个编译文件。四份 Console 摘要均为
`931aaed10507a21a239153dd25db1c5bcf8dad1887b3a5fc1dfc81e1bce9e688`，与上一批相同。

## 逐客户端安装态

`installed-sdk-source-control.json`：本批 TS/npm 与 Python wheel 在独立目录
离线安装。TS 26/26 后再跑 Python 37/37，无跳过；Python 使用 conda qwenpaw
解释器加 `-S`，SDK 和 websockets 均从安装目录导入。Python/TS 混合测试使用
本批两份实际安装包，不借用上一轮 SDK；所有 Core 执行只指向 source binary。
两种 SDK 的实际包载荷与 README 均和源码一致，依赖分别为 ws 8.21.3、
websockets 15.0.1。Python 的依赖 wheel 使用上一轮下载的本地缓存，未内嵌 Core。

`isolated-client-installation-complete.json`：VS Code 源码 73/73，两份实际
安装 VSIX 各 24/24 shutdown/manager 专项（VS Code API stub，未激活扩展）。
全部业务 manifest 字段、16 个编译文件及平台 Core 资源逐项核对。通用包
不含 Core，ARM64 包匹配本批 source Core；测试显式指向 source，不执行包内副本。

保留版 wheel 实际安装后，CLI 单元 855/855（18.02 秒，一条既有 audioop
弃用 warning）及集成 36/36 顺序通过。测试开始前核对 `qwenpaw.__file__`
位于安装目录，禁用 pytest 的源码 pythonpath 注入，JUnit 核对完整计数及
零失败/错误/跳过。这是保留版 CLI/TUI 的回归，不能替代 Rust CLI/TUI 全量实现。

最终 [final-verification.json](../../../dist/qa-runtime-20260914-737ANs/final-verification.json)
于 2026-09-14 11:13:45 UTC 通过：九个制品、2901 条来源、十五条安装检查的
成功退出与先后顺序、source Core 摘要及原 Console 零 diff/status 均重新核对。
本批没有残留挂载或 source Core 进程；此前检查器失败报告保留，未据此重建包。

## 保留的构建/检查失败

- 首次 DMG 创建“资源忙”后构建终态退出。确认本批没有挂载、DMG 进程及
  半成品，原参数仅重试一次成功，原失败/检查/重试日志保留；根因未确认。
- 静态检查和卸载成功后，外层命令记录器与检查器争用 `static-inspection.json`，
  外层报 EEXIST/退出 1。原检查报告未覆盖；`static-result-audit` 独立核对完整
  报告及无残留挂载。后续命令记录增加 `command-` 前缀，不重挂载或重签名。
- VS Code 安装器重排 `package.json` 并增加 `__metadata`；初次字节比较误报。
  独立诊断证明仅增加安装元数据，全部业务字段一致。第二次错误假设本地 VSIX
  安装元数据会带 darwin-arm64，实际两份均为字符串 `undefined`；平台改由
  VSIX XML 的 TargetPlatform、Core manifest 和二进制摘要确认，未忽略业务
  字段或编译载荷差异。失败报告保留，已完成的安装/测试不重复执行。

## 制品入口

- [macOS ARM64 QA DMG](../../../dist/qa-runtime-20260914-737ANs/QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg)
- [桌面 ZIP](../../../dist/qa-runtime-20260914-737ANs/QwenPaw-Tauri-2.2.0b5-macOS.zip)
- [Core tar](../../../dist/qa-runtime-20260914-737ANs/qwenpaw-core-darwin-arm64-QA.tar.gz)
- [WebUI tar](../../../dist/qa-runtime-20260914-737ANs/webui/qwenpaw-webui-2.2.0b5-QA.tar.gz)
- [TypeScript SDK](../../../dist/qa-runtime-20260914-737ANs/sdk/qwenpaw-sdk-0.2.0.tgz)
- [Python SDK](../../../dist/qa-runtime-20260914-737ANs/sdk/qwenpaw_sdk-0.2.0-py3-none-any.whl)
- [通用 VSIX](../../../dist/qa-runtime-20260914-737ANs/vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix)
- [ARM64 VSIX](../../../dist/qa-runtime-20260914-737ANs/vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix)
- [保留版 Python wheel](../../../dist/qa-runtime-20260914-737ANs/legacy/qwenpaw-2.2.0b5-py3-none-any.whl)

这是本机 QA 批次，不是正式签名、公证、原生启动及全交互通过的发布版。
原 Console 源码不改、构建内容一致不代表所有原生交互已测。未执行包内 Core，
未启动桌面窗口或激活扩展；跨平台、默认 Workspace/自动共享及所有旧功能
继续按主计划验收。后续应继续功能接线与缺口验证，不以反复打包替代实现。
