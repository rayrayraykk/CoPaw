# Python SDK 优雅关闭验收

日期：2026-09-10。对应 [生命周期计划](../architecture/stdio-host-lifecycle.md)
的 Python 本步。使用 conda `qwenpaw`、Python 3.12、macOS ARM64；不读取真实
key/keychain，不运行分发包 Core，不修改前端、日常数据或系统安全设置。

## 实现与语义

`close()` 关闭请求入场、唤醒待响应请求，在写锁下结束 stdin，继续读取
stdout 并等待自有进程退出。正常期限 30 秒；失败后只终止自有进程，最多
再等 5 秒。非零退出、超时或未确认的清理均抛出 `ShutdownError`，不把
强制终止当作保存成功。stderr 仍继承父进程。

普通线程的并发和重复 close 共享结果；reader 回调主动关闭时把输出转交
临时 drainer，避免等待自己。reader 回调遇到其他关闭者时不互等，此次
回调重入不是独立的持久化确认。关闭结果发布后才执行延迟的 close 通知，
允许通知启动另一个关闭线程并 join。任意用户回调可能永久阻塞，SDK 不
强制杀 Python 线程，也不跨线程关闭正在读取的 TextIO；未完成则报告失败。

启动或上下文正文已有异常时保留原异常，清理失败仍记录在客户端。关闭后
重连须创建新实例：旧实现也因保留关闭错误而不能成功复用；现在在再次
启动子进程前明确拒绝，避免产生无效子进程。

## 测试与失败记录

- 最初 EOF 标记和非零退出两项均失败；保留断言后实现关闭逻辑。
- 当前 **16 项**全部通过：原有 5 项，新增关闭专项 10 项，以及真实 Core
  持久化 1 项。包括 2 MiB stdout、EOF 后延迟标记、待响应请求、通知拒绝、
  4 个并发调用、close/reader 回调重入、非零退出、异常保留、重复关闭。
- 超时用例实际等待生产的 30 秒期限，并检查子进程退出、reader 结束和
  缺少成功标记。另两项 reader/写锁故障注入使用测试内的 0.1/0.2 秒期限，
  不改变生产默认值或替代真实 30 秒超时用例；验证失败和句柄保留。
- 真实 Core 连接临时目录及 loopback 模型夹具。在模型请求进行中关闭，
  **重开 Core 前**通过只读 SQLite 直接比较完整 Thread/Turn 状态；只允许
  updatedAt 推进及 idle/interrupted 终态差异，再重开验证完整历史一致。
- 最终源码 **16/16，33.847 秒**；wheel 实际离线安装后 **16/16，33.717 秒**，
  均无跳过。安装验证检查实际导入路径，逐个比较 SDK Python 文件 SHA-256。
- 2026-09-10 收尾依次复验：Rust release SDK/CLI **2/2，1.26 秒**、TypeScript
  编译及 **9/9，30.396 秒**、VS Code 编译及 **57/57，0.191 秒**，均无跳过。
  未激活原生 VS Code 扩展；这不是本轮重新运行整个 Rust 工作区。

## 风格与类型检查

首次 pre-commit 修改编码声明、尾逗号和 Black 格式，并发现 IO 类型、列表
类型与测试结果列表类型问题，均修正。最终 AST、编码、私钥检测、空白、
尾逗号、mypy 和 Black 通过，Ruff 通过。根目录 pre-commit 的 Flake8/Pylint
两项没有按原配置全绿；它们单独对本次六个 Python 文件检查：

- Flake8 保留原 E203 例外，额外忽略 F541，以遵守用户只用 f-string 的规则。
- Pylint 保留仓库既有禁用项，额外忽略对应的 W1309；白盒关闭测试单独
  忽略 W0212，其余五个文件单独忽略 R1732（跨方法持有 Popen 及有期限的
  acquire/finally 不能换成无期限 with）。两个最终检查均为 10/10。
- 没有修改仓库全局 lint 配置。最终 pre-commit 仅跳过上述单独检查的两项。
  复验后包内 Python 文件与源码仍一致，`console/src` 零 diff，diff 检查通过。

## 产物

目录：`dist/qa-python-close-20260910-I6Dn7J`（产品仓库根目录下）。

- wheel：`qwenpaw_sdk-0.2.0-py3-none-any.whl`
- SHA-256：`f16b2ec9e3332bf394e7408b49765c0e9d3132ae75b31af5716b813ce343c8be`
- 对照 source Core：`target/release/qwenpaw-core`
- Core SHA-256：`e630c45861b71c1b97c0e222d7a5a8cdcf3a1968e3defc44b129e6d61f3c4860`

构建、离线安装和安装态测试日志及 `verification.json` 均保留。检查脚本为
`scripts/release/check-python-package.mjs`，只在新空输出目录工作，不执行
包内 Core，不覆盖旧报告。wheel 不包含 Core，也未发布到包仓库。

## 未完成门禁

三语言自有进程关闭基础已分别验收，但默认 CLI/SDK Workspace 初始化、
凭据优先级、后台调度器跨进程单实例归属尚未完成。Core 最终 upsert 失败
仍仅 warning；正常存储测试不等于所有磁盘故障下的持久化成功确认。

整套九类新制品、包内 Core 启动、原生窗口/扩展激活、Windows/Linux/macOS
x64 和全部原功能交互仍独立开放。`vGGX8Z` 九类包不包含后续 stdio/SDK
变更，本 wheel 不能代替整套包验收。旧缓存和产物未删除，清理范围待确认。

后续同日已完成 `WXFYc3` 九类 macOS QA 构建/静态检查及 SDK/VSIX/legacy
隔离安装复验，见 [新批次记录](qa-sdk-shutdown-packages-20260910.md)。
这里单独 wheel 的摘要仍属于 `I6Dn7J`，不能当作新批次 wheel 的字节摘要；
包内 Core 启动、原生交互、跨平台和全功能父项没有因此关闭。
