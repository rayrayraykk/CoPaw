# Agent 组合发布：真实宿主接入验收

日期：2026-09-15，macOS ARM64。证据目录：`dist/qa-publication-host-20260915-ILpXIO`。对应 [恢复协议和清单](../architecture/agent-publication-journal.md)。本轮接入真实 Profile 保存与启动恢复；不表示所有功能、操作系统或分发包已经完成。

## 实现边界

Profile 保存现在以安装级 SQLite 的判定、可信摘要和非秘密文件元数据作为恢复权威，不再仅依赖宿主内存。元数据含精确路径、文件身份/权限/摘要、安装/事务/Agent ID，不含配置正文或凭据；逻辑备份不携带这些控制记录。私有凭据副本仍走专门的凭据存储接口，正常生产适配器不降级成明文文件或内存。

`staging` 预留后准备私有凭据副本，成功后持久进入 `publishing`，再发布凭据及两个文件。Channels 与 `committed` 仍在同一个 SQLite 事务中提交，没有 Channels 变更时也必须提交判定。失败恢复和正常关机共用同一入口。

prepared 恢复先完成文件和凭据逆操作，committed 保留新值；之后先持久进入 `cleaning` 再删除恢复副本。cleaning 重开只清理，不再回滚，因此允许此前已成功删除的私有凭据副本不存在。最后同事务级联删除判定、摘要和元数据；元数据删除失败时三者全部保留。

Workspace/Desktop 构造器在 `initialization_root`、环境、Agent 设置、MCP、模型和 catalog 初始化之前执行恢复。持续恢复失败则构造失败，不启动业务服务；同一存活宿主仍保留原有 HTTP/协议/备份准入与 Profile 重试规则。

## 新增验证

- Storage 4 项：元数据写入失败不留下部分控制行；状态转换/重开/陈旧 ID/过早提交和删除保护；最终级联删除失败原子保留；非法长度/阶段不是缺失记录。原孤立摘要测试同步改为业务准入也拒绝。
- Core 1 项：预留、进入 publishing 和 cleaning 均遵守既有全局恢复屏障。
- 宿主 2 项：真实 Profile 凭据写入后报错、持续逆操作失败、重开数据库后启动拒绝与故障解除恢复；私有副本准备后报错、清理已删后报错，prepared/committed 两种 cleaning 重开均不重复逆操作。测试凭据替身明确为内存实现，不宣称系统账户跨进程持久化。
- 宿主强杀矩阵 1 项及其测试子进程入口 1 项：父进程初始化隔离数据目录，子进程通过真实 HTTP Router 提交默认 Agent Profile，分别在 staging、publishing、secret-published、files-published、committed、cleaning、files-cleaned、secrets-cleaned、finished 九个标记处暂停，父进程强杀并 wait 回收。重开走真实 App Server 构造器，核对完整 Profile、文件、Channels、空控制记录及再次重开稳定性。该矩阵**没有凭据变更**，两个 secret 标记不构成凭据强杀覆盖。

共新增 9 个测试入口，其中 1 个仅供子进程调用、在普通测试中无参数直接返回。此前七个文件 rename/SQLite 组合强杀边界仍保留在普通回归内；它们与新宿主矩阵的覆盖范围不同。

首轮 `workspace-initial` 在 App Server 538 通过后发现 1 项接口退化：`agent.json` 为目录时新路径返回 500，而旧契约是 400 和 `Agent config is not a regular file`。实现补回相同的前置检查，未修改断言来接受退化。初次编译的可见性/旧内存测试引用以及 Clippy 的通配导入和分支写法也已修正；保留失败日志，不关闭 lint。

最终 `workspace-verified` **838 通过、0 失败、31 个显式项忽略**，退出 0，总耗时 115.534 秒。该命令在最后的代码修正与格式化之后运行；较早的 `workspace-final` 也为 838/838，但不替代最终源码的重跑。`clippy-final` 全工作区/全目标警告即错误通过，`fmt-check` 通过。

原页面专项顺序 **7/7**：Agents 3 项（32.24 秒）、Channels 1 项（11.77 秒）、备份 2 项（76.57 秒）、邮件抽屉 1 项（11.65 秒）。四条命令均退出 0，覆盖配置保存/重开、Chat/Cron 停止隔离、Channels 抽屉、备份活动刷新/取消/完整往返，以及邮件操作归属/刷新/重开。未重跑其余 24 个显式项，不把普通测试的 ignored 计作通过。

`verify.mjs` 检查上述终态、9 个新增入口、历史失败记录、上一阶段证据、2,906 条来源和九个旧制品哈希，输出 `verification.json`。本轮原 Console 和浏览器脚本未改，release Core 及旧制品未重建。来源记录明确区分真实无凭据 Profile 强杀、内存凭据替身故障重开、OS 凭据强杀未测以及完整恢复未完成。

## 仍然开放

- 凭据系统强杀/断电、凭据回滚中间点的真实进程中断，以及 prepared 回滚的 cleaning/最终删除宿主强杀矩阵。现有凭据测试是明确的故障注入与数据库重开，不是 OS 凭据持久性证明。
- SQLite 预留之前进程退出留下的孤立暂存，以及所有已在途业务的完整停顿、panic/poisoned 锁边界。未知目录不会自动递归清理。
- 检查与 rename/remove 之间的恶意并发竞态、跨平台目录持久性和大型 Workspace 性能。当前观察到的独立改动会保留，不宣称原子 CAS。
- 17 个外部 Channel 运行时等功能缺口、全功能发布门禁、共享宿主默认策略、原生 Desktop/VS Code 激活、Windows/Linux/macOS x64 实机测试。旧焦点与 SDK EOF 间歇故障不以本轮通过撤销。
- 没有运行包内 Core、访问真实凭据或系统 Keychain；没有 commit/push。旧九类 `eaTvv4` 制品不含本轮改动，尚未重建。
