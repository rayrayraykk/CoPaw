# 邮件 ACL 版本的 macOS QA 制品

日期：2026-09-09；承接计划 §14.2.24.52。不是全功能、完整 GUI 或其他平台完成声明。

## 构建范围与位置

新输出：`dist/qa-runtime-20260909-jxgzYX`。旧 §51 输出 `dist/qa-runtime-20260909-K8SPWI` 保留。

本机隔离暂存：`/var/folders/0s/4ht2q69j6sx49r64ktp8pssm0000gn/T/qwenpaw-qa-runtime-WVPs0l`。测试不使用日常 App 数据或真实 key，不修改系统安全策略。

目标九类文件：桌面 DMG/ZIP、Core tar、WebUI tar、TS SDK tgz、Python SDK wheel、通用/arm64 VSIX、保留的 Python 产品 wheel。前端 `console/src` 零 diff；重建的是打包暂存目录，不删除旧 QA 制品。

构建前证据：普通 Rust **555/555**，邮件恢复失败扩展单跑 **1/1**，严格 Clippy（10.07 秒），一次完整原页面/调度参考组 **18/18**（282.39 秒）；release SDK/插件记录见 [邮件 ACL 验收](mail-workspace-ownership-acceptance.md)。Core SHA-256：`c9a0b22eb277e143f9b06fcf88197100ae8e64f053323e53f7455b42eb5431e2`。

## 构建及失败记录

- 原 Console 构建、Core release 暂存和版本启动检查、Tauri App、正常本机 QA ad-hoc 签名/校验、ZIP 通过。未使用生产签名证书，也未对失败样本重新签名以绕过启动问题。
- 首次 `hdiutil create` 报“资源忙”，构建脚本非零结束；只读检查无挂载镜像、无半成品 DMG，约 57 GiB 可用。相同参数重试一次成功，失败日志 `desktop-dmg.log` 与重试记录 `desktop-dmg-retry.log` 分别保留。
- 从已成功的桌面阶段继续其余制品，不重写已有构建日志。

## 安装态验收 Checklist

- [x] 九类制品、SHA256SUMS、构建输入清单与嵌入的原 Console 内容核对。构建清单记录 dirty 工作树 2836 个输入文件；后续验收文档更新不冒充原快照的一部分。原 Console **1311 个文件字节一致**，树哈希 `74ca39444b3b61637102f3ce163bfc9987c056ba4c4386897896f656097de976`。
- [x] 校验、逐项解包、DMG 只读挂载/卸载；结束后 `hdiutil info` 无挂载镜像。
- [ ] 安装的 TS/Python SDK、Core tar、DMG/ZIP 内 Core、通用/平台 VSIX 逐项运行。
- [x] 安装的 Python legacy 版路径确认、CLI/TUI 帮助与单测 **855/855**（17.78 秒，1 条既有 audioop 弃用警告），CLI 集成 **36/36**（70.67 秒）。没有用源码导入替代安装 wheel。
- [ ] 解包的 WebUI 由该包 Core 服务并运行真实 Chrome 页面测试。
- [x] 记录首次启动失败；不以暖启动重试覆盖。首次分发组 **21/24**，下面三项失败均保留。

## 首次分发失败与对照

`qualification.json`：失败项目为 `python-sdk-packaged-core`、`package-smoke`、`webui-browser`。其余 21 个检查通过不表示 21 个功能或完整客户端通过。

- 安装的 Python SDK + 解包 tar Core **3/5，2 errors**（23.486 秒），两个真实 Core 用例在初始化握手阶段出现 `TransportClosedError`。同一已安装 SDK + 源 release 对照 **5/5**（0.962 秒）通过；不据 Python 错误本身确定退出信号。
- 四个解包 Core 的独立版本探测均为外部 `SIGKILL`、空输出、约 11–12 秒；不是超时、输出上限或探测器主动终止。时间戳确认逐项串行，无重叠。真实模型/图片/重开和使用包内 Core 的 VSIX CRUD 因启动门禁未通过而未执行，不能算成功。
- 源 Core 对照的握手/模型/图片/重开通过；打包的通用/平台 VSIX 客户端使用源 Core 对照的 Thread CRUD 通过。这只能支持客户端逻辑对照，不能替代各自分发 Core 验收。
- 解包 WebUI 的 Core 服务也没有成功报告监听地址，`webui-browser.log` 非零；没有完成该包浏览器验收。
- 只读 `codesign --verify --strict` 对原失败 tar Core、`--deep --strict` 对原失败 ZIP App 都通过。未修改这些文件；签名完整性不等于启动放行。

| 对象 | PID | UTC 起止 | 退出 | 同 PID 的本机终端防护记录 |
| --- | --- | --- | --- | --- |
| tar Core | 58768 | 14:52:25.989–14:52:37.085 | SIGKILL，11.096 秒 | 非白名单标记 |
| ZIP Core | 58779 | 14:52:37.087–14:52:49.051 | SIGKILL，11.964 秒 | 非白名单标记 |
| DMG Core | 58883 | 14:52:49.051–14:53:00.788 | SIGKILL，11.737 秒 | 非白名单标记 |
| 平台 VSIX Core | 58992 | 14:53:00.788–14:53:12.616 | SIGKILL，11.828 秒 | 非白名单标记 |
| 源 release 对照 | 59033 | 14:53:12.616–14:53:12.883 | 0，267 毫秒 | 对应 QwenPaw 记录无该标记 |

本机 UTC+8。日志查询仅针对上述 PID 和 22:52:20–22:53:20 时间段；只保留进程/时间/相关标记，不复制无关策略。记录来自 `com.alibaba.endpoint.aliedr.ne`，包含 `mac_non_white_proc` 的关联并不证明它发送了 SIGKILL。四项在串行执行下仍复现，不能继续把旧脚本并行当作充分解释。需要终端防护管理员核查实际策略/处置，不能通过重签失败文件、迁移路径、关闭服务、清除属性或不断重试绕过。

桌面正常 QA 签名后的 Core 哈希为 `f67865b73a9aede92277dd915129d6e330e0ea27865719a6798d300f86512968`；源/tar/平台 VSIX 原字节对应本节前述 `c9a0...`。完整原始探测元数据见输出目录 `native-execution.json`，所有失败结果和包均保留。

完整桌面 GUI 隔离依赖、首次分发 SIGKILL 历史、其他平台和真实邮件监听等未完成范围仍按既有记录保留。
