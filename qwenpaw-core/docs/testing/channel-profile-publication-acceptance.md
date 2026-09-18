# Channels / Profile：权威读写与普通失败回滚

日期：2026-09-14，macOS ARM64。证据目录 `dist/qa-channel-publication-20260914-gP6wR1`。承接 [前置校验诊断](channel-profile-authority-diagnostic.md) 与 [实施清单](../architecture/channel-profile-authority.md)，不是全功能或发布验收。

## 本轮实现

- Channels v2 的 Workspace 记录是通道配置的唯一运行权威。Agent GET、普通/模型配置响应、运行与 Cron Workspace 上下文、邮件上下文使用相同投影；原文件/注册表不再新增独立 Channels 值。
- Profile 提交字段缺失保留现值；null 表示未配置；空/部分对象补齐原版默认配置。未配置时单项返回 404、列表使用原版最小禁用项；Console 单项保存可重新建立完整配置。
- 保留旧非空影子数据，冲突写入返回 409，不自动迁移、删掉或启用外部通道。复制仍得到独立的默认通道配置。
- Agent 锁后取得 Channels 锁，先校验/暂存，再发布凭据和文件，最后提交 SQLite。复用 `RestoreFiles` 保留精确原文件；失败能恢复原文件、注册表、凭据及“原设置不存在”状态。没有异步中断点穿过发布段。
- 新文件保持私有暂存权限；拒绝把目录或链接当作 Agent 配置文件覆盖；凭据发布后重新检查 Workspace 身份和文件类型。宿主公共初始化入口创建并规范化数据目录，包含环境形式的相对 `QWENPAW_HOME`，不改变用户选择的数据位置。

## 验证证据

新增 7 项发布回归、1 项 nullable 范围备份/恢复回归和 1 项相对数据目录初始化回归；配合此前 6 项前置校验，验证双向 API、完整运行配置快照、其他 Agent 不变、重开、禁用 Agent 管理、并发编辑、旧影子冲突、SQLite 插入失败、缺失配置文件、目录保护，以及凭据先变更再报错时的逆操作。断言涵盖完整配置与原始文件字节。

独立 source Core/App Server 宿主执行初始 17、重开 18 个真实 HTTP 请求，包含完整 Profile 经 Node JSON 往返、复制、未配置/重新启用、并发保存与文件无影子。`http-authority-validated.json` 的 `productInvariantPassed=true` 仅表示这些已声明的一致性断言通过，不代表下文恢复边界已完成。

额外相对路径对照区分两个构造入口：显式相对目录已由原构造函数规范化，35 个请求直接通过；环境变量入口曾对合法 Profile PUT 返回 500（`environment-relative-red.log`），原因是事务暂存要求绝对路径。首次仅修正事务目标后，保存通过但复制仍返回 500（`environment-relative-green.log`，该命令名不表示通过）。最终撤去局部修补，在公共宿主初始化入口规范化数据目录；新 fixture 的 `environment-canonical` 35 请求全部通过，并增加不修改基础 Workspace 的相对路径单测。环境参数仅传入隔离子进程，没有修改进程外环境或日常数据。

完整显式组 31/31 通过：App Server 30 项、CLI Debug 原页面 1 项，分别耗时 361.98 / 15.52 秒，含 Python 原版默认/日程/项目/Debug 对照及原 Agents、Channels、Chat、Cron、备份、检查点、模型等页面。该组运行于最后的相对路径修正前；修正后的验收另行记录，不把旧运行说成新源码的再次执行。

### 最终源码复验

最终源码的普通工作区 `workspace-complete` 为 **773 通过、0 失败、31 显式项忽略**，包含下面曾超时的 SDK EOF 用例；`clippy-complete`（全工作区/全目标、警告即错误）及 `fmt-complete` 均通过。最终环境入口 HTTP 证据是 `http-env-canonical.json`，35 请求通过。显式页面组的最终源码复跑另有 Anthropic 失败，不能沿用前一轮 31/31 作为本次全绿结论。

`explicit-complete` 的 App Server 组为 **29 通过、1 失败**，393.79 秒，退出 101，后续 CLI 未执行。Anthropic 用例配置/模型选择 API 均成功；事件日志记录聊天框获得焦点后约 1 毫秒失焦，文字和 Enter 实际进入其他 INPUT，聊天框长度仍为零。没有失败 API 或已发出的聊天请求，说明本次失败发生在输入阶段，但尚不能判定焦点变化的最终原因。保持生产代码、浏览器脚本和超时阈值不变，`anthropic-isolated` 单独复测 **1/1** 通过，12.37 秒；这不能消除原始整组失败，也不算修复。随后 `cli-explicit-complete` 补跑原 Debug 页面 **1/1** 通过，14.86 秒；不把分次结果描述为整组 31/31。

最终证据汇总脚本 `verify.mjs` 同时核查成功与失败退出码、383 项锁定依赖、上一批 2,906 条源码来源和九个原制品哈希，并单独记录本次修改文件；`verification.json` 的 `verified` 仅表示记录自洽，`explicitFullRunPassed`、`irrecoverableRollbackComplete`、`crashRecoveryComplete` 均保持 false。

### 保留的失败

- 首轮完整普通组两项失败，因为旧测试把公开运行视图等同于原始注册表。现分别断言完整视图和完整持久化内容，原目录、身份、禁用编辑及不影响其他 Agent 的断言保留。
- Clippy 首次要求简化 Option 组合，按建议修正；未禁用 lint。
- 同时运行页面/编译和普通测试时，未改动的 Rust SDK EOF 测试在原 5 秒边界超时。退出已确认后，保持源码与阈值不变，单独复测 1/1 通过（0.28 秒）。根因尚未证明；保留失败记录，不把“负载导致”写成定论。
- QA 对照 Cargo.lock 首次读取预算不足，停止而未写入截断文件；随后完整读取并保留锁定依赖。没有更改产品依赖。

## 恢复与发布仍未完成

可成功完成的逆操作已有测试，但持续失败的凭据逆操作尚未由宿主长期保留/重试，进程强杀后的组合事务恢复也未实现。文件恢复不安全时保留的目录不能代证凭据恢复所有权、后续请求准入或重启恢复。这些仍是下一阶段必做项，当前阶段不能标成恢复完备。

本轮未改 `console/src`、SDK 或平台壳，没有重新构建九类分发制品，也没有执行包内 Core、启动原生 Desktop 或激活 VS Code。上一批 `eaTvv4` 的 DMG/其他制品不包含本轮实现；原生和 Windows/Linux/macOS x64 项继续开放。
