# stdio 宿主退出源码验收

日期：2026-09-10。对应 [设计、架构图和 checklist](../architecture/stdio-host-lifecycle.md)。
使用临时 Workspace、回环模型夹具与无秘密凭据存储；不读取真实 keychain，
不运行分发包中的 Core，不操作日常数据。原 `console/src` 零 diff。

## 修复和测试范围

- 可注入 I/O 与 `run_stdio` 使用同一生产实现，不另外实现测试协议。
  最初 EOF、输入失败、输出失败、显式停止四项均失败，修复后纳入全库通过。
- 六项传输测试：无 Desktop 的 EOF 关闭入场；闲置输入时显式停止；输入仍
  开着时观察写失败；保留输入错误；EOF 排空准确的 initialize JSON；输出
  不被读取时在服务收尾之后等待五秒，明确返回输出排空超时。最后一项使用
  Tokio 虚拟时钟，正常输出测试实际读取小容量 duplex 管道。
- 四项 Workspace stdio 测试：等待工具审批时 EOF，拒绝副作用并保存
  `Interrupted`；已完成 Turn 的自动快照仍在等待时 EOF，清空 tracker 且
  重开后完整历史不变；已入场 Heartbeat 租约未释放时退出不得返回，关闭后
  拒绝新入场；真实 Heartbeat 回环模型请求被中断，退出后 Turn 状态为
  `Interrupted`，重新打开存储得到完全相同的 Thread 快照。
- 新增真实 CLI 集成测试仅关闭 stdin，不发送 terminate/kill，收到两条
  完整 initialize/thread-list 响应并获得成功退出码。失败路径的测试进程
  清理不作为正常退出的证据。
- Heartbeat 事件流原先在取消/超时后被丢弃；中断 API 的返回只代表请求。
  现在保留该事件流直到最终事件，再释放完成租约。首次编译发现保留借用
  后返回 `turn_id` 的 move 冲突，修正后运行完整测试。没有弱化原超时断言。
- 原有真实 WebSocket 断连/宿主停止区别测试、Heartbeat 定时/超时/重叠抑制、
  Console/Protocol/Backup 收尾测试均随工作区通过。普通 WS 断连仍让已接收
  的 Protocol 任务完成，不把单个远程客户端退出解释成宿主停止。

## 本轮门禁

- [x] `CARGO_INCREMENTAL=0 cargo test --workspace`：**697/697**；另有 26 项
  显式浏览器/参考测试。App Server 452/452（14.28 秒）、HTTP 36/36
  （4.44 秒）、真实 CLI stdio 4/4（1.46 秒）、Rust SDK 1/1（2.96 秒）。
- [x] `CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets -- -D warnings`：
  **34.07 秒**，没有新增 lint 豁免。
- [x] 原前端 `node --max-old-space-size=4096 ./node_modules/vitest/vitest.mjs run`：
  **295 文件、2453/2453，74.88 秒**。使用 Node 24.18.1。
- [x] `cargo fmt --all -- --check`（同样 `CARGO_INCREMENTAL=0`）、
  `git diff --check` 和 `console/src` 零 diff；API inventory 检查保持当前
  **370 调用、34 未注册路由**，不将“清单无漂移”解释为全功能完成。
- [x] `CARGO_INCREMENTAL=0 cargo test --workspace -- --ignored --test-threads=1`：
  **26/26**，无跳过；App Server 25 项 **307.39 秒**、真实 CLI 日志页 1 项
  **15.38 秒**。未与 release 构建重叠，浏览器退出时限及断言保持原样。
- [x] `CARGO_INCREMENTAL=0 cargo build --release -p qwenpaw-cli`：**52.51 秒**。
  源 `target/release/qwenpaw-core` SHA-256：
  `e630c45861b71c1b97c0e222d7a5a8cdcf3a1968e3defc44b129e6d61f3c4860`。
- [x] 对接该 source release 依次检查：TypeScript SDK 编译和 **4/4，0.815 秒**；
  Python SDK **5/5，0.604 秒**（conda `qwenpaw`，验证实际源码导入位置）；
  VS Code 编译和 **57/57，0.180 秒**。全部无跳过，未激活原生扩展。
- [x] 收尾核对测试/构建和源 Core 进程均已结束，`console/src` 零 diff，
  diff 空白检查通过；未 commit/push，未清理旧包或构建缓存。
- [ ] 新九类分发制品与各端完整运行。

## 不能从这些结果推出的结论

默认 CLI/SDK 仍调用轻量构造器，三个 SDK 的主动关闭仍会立即终止子进程。
它们需要后续真实进程回归及优雅关闭实现；共享服务初始化还需要确定多进程
调度器归属、凭据优先级和数据目录，不能在每个 SDK 子进程启动重复定时任务。

本轮的待执行快照测试证明退出时取消延迟任务并保留已保存 Turn，不证明
每个取消的快照都已写入。输出测试不涵盖无限输入加满队列时的全部读取时序。
现有后台服务各自的退出策略不等于强杀、断电或文件系统级崩溃原子性。
另外，`Core::finish_turn` 的存储 upsert 失败目前只记录 warning，随后仍发送
`TurnCompleted`。因此本轮等待证明正常存储下退出不抢在保存尝试前返回，
并不提供磁盘写失败时的持久化成功确认或向 SDK 传播保存错误；该失败语义
仍需专门的故障注入和实现，不可把终态事件解释成无条件保存成功。

九类 QA 批次 `qa-runtime-20260910-vGGX8Z` 包含此前项目目录修复，**不包含本次
stdio 后续源码变更**。源二进制验证不能替代包内启动、原生 Desktop/WKWebView、
VS Code 激活，以及 Windows/Linux/macOS x64 的实机构建与功能验收。
