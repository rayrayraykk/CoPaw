# Agent 组合发布：凭据与逆操作进程中断验收

日期：2026-09-15，macOS ARM64。证据目录：`dist/qa-publication-interrupt-20260915-BwzT3m`。承接 [真实宿主接入](agent-publication-host-acceptance.md) 与 [恢复协议](../architecture/agent-publication-journal.md)。本轮扩大实际宿主恢复验证，不宣称全功能或安装包验收完成。

## 隔离方式

凭据服务仅存在于单元测试：父测试进程在 `127.0.0.1` 随机端口启动服务，内存持有合成 live 凭据、安装/事务/Agent 绑定的私有恢复账户，以及写入记录。被测 App Server 子进程通过显式传入的 `DesktopCredentialStore` 适配器调用服务；没有系统账户、明文持久化降级或生产网络服务。子进程被杀后服务继续存在，重启宿主仍读取同一份凭据状态。

适配器使用有长度限制和超时的测试帧，服务拒绝错误 live key、错误恢复身份、占用账户和不匹配的清理记录。子进程只运行当前单元测试可执行文件，通过真实 HTTP Router 保存 Profile，恢复走真实 App Server 构造器。不是运行包内 Core，也不是模拟一个未发生的宿主崩溃。

生产路径只新增 `cfg(test)` 暂停标记；原业务逻辑、前端、浏览器脚本和公开协议未改变。旧无凭据矩阵的构造/请求/进程回收辅助函数供新矩阵复用。

## 强杀矩阵

原凭据分别为缺失、空字符串、非空字符串，每个场景都使用新的隔离目录和凭据服务。

| 分支 | 检查点 | 强杀案例数 |
| --- | --- | ---: |
| 正向发布 | staging、私有副本已写但接口未返回、secret-prepared、publishing、live 已写但接口未返回、secret-published、files-published、committed、cleaning、files-cleaned、私有副本已删但接口未返回、secrets-cleaned、finished | 13 × 3 = 39 |
| prepared 逆操作 | rollback-start、files-rolled-back、旧凭据已写但接口未返回、secret-rolled-back、cleaning、files-cleaned、私有副本已删但接口未返回、secrets-cleaned、finished | 9 × 3 = 27 |

逆操作通过 SQLite `BEFORE UPDATE OF state` 触发器拒绝真实提交，进入正常错误恢复路径，未手工替换回滚算法。每次检查到达指定标记后，父进程强杀并 wait 回收；自动资源回收也负责失败时终止遗留子进程。

强杀后先检查外部服务的实际 live 值和恢复账户，再重开宿主：prepared 恢复精确原文件/Channels/凭据，committed 保留新值。比较完整 Profile、配置与 catalog 字节、Channels 值、恢复账户为空、SQLite 无待恢复记录，以及完整凭据写入序列。再次重开后全部状态不变，不新增逆操作写入。递归检查隔离目录的普通文件（含 SQLite/WAL、暂存与诊断日志），不跟随链接，未发现两条非空合成凭据正文。

## 失败记录与最终检查

首次 `credential-targeted` 编译失败于嵌套通配导入造成的 `assert_eq` 宏歧义，已显式导入。首轮 `credential-matrix` 为 1 通过、2 失败：正向测试的预期遗漏了提交后去掉密钥但保留的 `mail.credential` 对象，已依据实际请求和既有脱敏实现修正完整预期；逆操作测试在 `inverse-write-before-return` 前子进程退出，当时 stderr 被丢弃，无法从现存证据确定原因。

后续为子进程保存诊断 stderr 并在提前退出时读取，不扩大超时，不跳过检查点，也不把恢复断言缩小为部分字段。`credential-diagnostics` 的两个矩阵及子进程入口均通过，覆盖 66 次实际强杀，耗时 12.75 秒。该通过不能倒推首轮退出已经定位或修复。

初次严格 Clippy 指出测试监听错误匹配和无须按值传参，均已修正，没有关闭 lint。

完整并发回归继续复现凭据夹具失败，新增分类诊断表明服务端读请求立即得到 `WouldBlock`，而非两秒超时。`readiness-red` 用“连接已被接受但延迟 20ms 发送请求”稳定复现：返回 `fixture-frame-rejected`。修正方案是对 accepted socket 显式 `set_nonblocking(false)`；保留原来的两秒读写超时、帧长度限制和所有矩阵断言。

同轮还暴露既有 `protocol_hook_disconnect_before_response_still_snapshots_saved_completion` 的同步缺口：Core Turn 已完成不代表协议 producer 已消费完成事件并排入自动快照；旧 settle 在队列仍为零时会提前通过。按现有生产完成顺序，测试应在中断 transport 之前取得对应 Workspace 的 protocol completion token，释放模型响应后等待该 token，再等待快照任务排空并断言完整 graph。此改动只补测试等待条件，不延长定时器、不改变生产事件或快照逻辑。

上述两处测试修正后，`readiness-green` **20/20** 通过，包括延迟发送和完整 66 次强杀矩阵；最终 `workspace-verified` **842 通过、0 失败、31 显式项忽略**，并包含此前失败的协议完成快照测试。`clippy-verified` 全工作区/全目标警告即错误与 `fmt-verified` 均退出 0。共新增 4 个入口：两个矩阵、一个子进程入口、一个确定性的 socket 就绪回归；子进程入口无参数时直接返回，不按单独功能覆盖计数。

`verify.mjs` 检查最终整组、红/绿证据、四个新增入口和旧协议断言通过，链式核对上一阶段、2,906 条来源和九个旧制品哈希，保存当前文件哈希。历史失败日志原样保留，其中失败断言曾输出合成凭据字符串；它们不是真实密钥，不能把成功运行日志/隔离目录的扫描解释为全部历史日志无正文。

## 尚未证明的范围

- 外部内存服务跨宿主死亡存活，不等于系统 Keychain/Secret Service/Windows Credential Manager 或整机断电持久性；服务进程自身崩溃也不在本矩阵内。
- 暂存完成到 SQLite 预留之前退出留下的孤立数据、所有在途业务的停顿、恶意文件检查/操作竞态、panic/poisoned 锁和跨平台运行仍开放。
- 未重跑原页面显式组；上一阶段 7/7 与本轮普通回归分别记录，不能合并声称本轮全显式通过。既有焦点/SDK EOF 间歇记录保留；本轮非阻塞读取竞态有确定性红/绿证据，但不倒推最初缺 stderr 的退出已逐个取得诊断。
- 原 Console、浏览器脚本、release Core 和九个旧制品保持原样。未 commit/push、未访问真实凭据、未运行原生 Desktop 或激活 VS Code；旧制品仍不包含此前宿主接入改动。
