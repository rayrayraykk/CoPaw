# 默认 Core 启动互斥验收

日期：2026-09-14。方案与清单见
[默认 Workspace 宿主](../architecture/default-workspace-host.md)。输出目录为
产品仓库的 `dist/qa-instance-lock-20260914-LvXBVd`。

## 清理与实施范围

用户明确允许后，仅删除 `qwenpaw-core/target/debug`（此前约 236 GiB）。
删除完成后可用空间约 237 GiB；源码、未提交改动、release、九类安装包和
历史报告保留。该目录不可直接恢复，但可由源码重建。本轮 Cargo 禁用增量
编译及 dev/test 调试符号；未修改仓库 profile 或测试时限。

CLI 在数据库打开及系统凭据读取之前，通过 fs2 对数据目录中的
`.core-instance.lock` 取得非阻塞独占锁；File 保留到服务结束，锁文件不
删除、不截断、不写 PID。冲突返回退出码 1 和明确错误，不输出 ready。
所有 CLI 传输共用该入口。当前没有自动连接已有宿主，也没有改变轻量
嵌入构造器，因此不能声称旧二进制或直接嵌入 Core 已受保护。

## 失败与复验记录

- `red.log`：修复前真实活跃轮次回归 **0/1**，第二个 Core 退出码实际 0，
  期望 1；编译 53.24 秒，测试 1.97 秒。两个自有进程先收尾再断言。
- `green-initial.log`：加入锁后同一回归 **1/1，2.63 秒**，完整内存及
  磁盘快照不变，不仅检查第二个进程退出码。
- `green-expanded.log`：新增测试误将返回 `()` 的通用 client.shutdown
  当作 Result，编译失败；仅修正测试中的 `.unwrap()`。
- `green-expanded-fixed.log`：扩展真实进程测试 **3/3，5.01 秒**。
- `clippy.log`：全 workspace/all-targets 严格 Clippy **23.21 秒**通过，
  无新增 lint 例外；fmt/diff 检查通过。
- `rust-workspace.log`：首轮全库在既有 Node 假子进程 SDK 测试失败，
  4 项启动报 `NotFound`。当次 PATH 未包含已安装的 Node 24；不是锁冲突
  或此前偶发 EOF 超时。补齐运行 PATH 后完整重跑，不修改原断言或时限。

## 本步验证矩阵

- [x] 第一个真实 Core 的活跃轮次；默认/显式 stdio、HTTP、Desktop、
  remote WSS 五种冲突启动均被拒绝，且没有 ready/协议输出。
- [x] 独立数据目录可以同时启动、返回空列表并正常关闭，原快照不变。
- [x] 正常关闭后同目录重开，完整 Thread/Turn 与关闭后的磁盘值相同。
- [x] 真实强制终止自有测试进程，重开前旧磁盘仍 inProgress；之后新
  Core 才恢复为 interrupted，核对完整快照而不是仅检查一个状态字段。
- [x] 模型初始化失败之后，下一个同目录进程可以启动。
- [x] 锁文件字节保留、重复获取、不同目录、目录别名及无效目录单测。
- [x] 最终普通 Rust **715/715**；其中 App Server **454/454**、Rust SDK
  **9/9**、CLI 普通单元 **6/6**、新增真实进程 **3/3**。27 项浏览器/参考
  用例需另行显式执行；没有以普通通过代替它们。
- [x] 原前端 **2453/2453，78.98 秒**（295 文件）；源码目录仍无变更。
- [x] 显式浏览器/原 Python 参考 **27/27**：App Server 26 项 **321.76 秒**，
  真实 CLI Debug 页面 1 项 **15.41 秒**。它们不替代全部原生和跨平台交互。
- [x] source release **12.26 秒**；优化版锁回归 **3/3，2.22 秒**和 Rust
  SDK 真实 Core 回归 **3/3，0.28 秒**，包含真正的最终保存故障传播。
- [x] TypeScript **10/10，30.389 秒**、Python **17/17，33.335 秒**、
  VS Code 编译及 **57/57，0.204 秒**顺序通过；Python 使用 qwenpaw conda
  环境并核对实际源码导入路径，各真实 Core 用例连接本步 source release。
  无跳过；VS Code 原生激活并不包含在这些客户端测试中。
- [ ] 新分发包、原生交互、跨平台与共享宿主自动连接。

本步源码验收时，原 `2qPEew` 九类 QA 制品未重建，不包含此次锁修复；它们的静态通过不能
作为新源码安装态证明。只使用临时目录与本地模型，不调用真实密钥或日常
App 数据，不执行分发 Core、不修改系统安全策略、不 commit/push/发布。

本步新 source release SHA-256：
`1437995c909850b86a7f434be3aed586b6036789d03f90f8d594e35e8b3b858d`。
旧 `2qPEew` 九个制品的大小与 SHA-256 在清理之后再次逐一验证，均未变化；
它们仍是修复前的 `19e454...` 批次，不用旧通过结果证明新入口行为。

`verification.json` 通过：最终八条命令的开始/结束顺序、退出码和信号，
各组测试数量，三份首次失败记录，九个本步代码/依赖/SDK 文档摘要，以及
新 source Core 哈希均已核对。原前端 Git 状态为空，无遗留 source Core
进程。报告明确 `artifactsRebuilt=false`、`packagedRuntimeTested=false`、
`nativeActivationTested=false`；清理范围只包含已获准的 debug 构建目录。

后续整套九类 QA 批次 `g6i9VJ` 已包含 `143799...`。静态来源/签名核验、
四份原 Console 一致性、安装后 TS 10/Python 17、保留版 CLI 855+36 通过，
两种 VSIX 隔离安装未激活。见 [后续制品验收](qa-instance-lock-packages-20260914.md)。
本页源码报告的历史 `artifactsRebuilt=false` 不改写；包内运行、原生端、
跨平台和共享宿主仍未完成。
