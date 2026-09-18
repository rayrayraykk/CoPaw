# Rust SDK 优雅关闭验收

日期：2026-09-10。对应 [生命周期计划](../architecture/stdio-host-lifecycle.md)
中的 Rust SDK 本步。仅改 Rust 客户端、测试依赖和测试；不改原前端或 Core
执行逻辑，不读取真实 key/keychain，不运行分发包 Core，不清理、commit/push。

## 实现边界

- 自有 `StdioAppServer::shutdown(self)` 关闭所有 clone 的入场，结束 stdin，
  继续读取 stdout，等待 Core 退出。若 caller 指定 stderr 为 pipe，则排空
  该 pipe；继承或文件重定向不改为其他配置。
- 通信、非零/信号退出、超时均返回错误。正常最多等待 30 秒，失败清理时
  对自有子进程请求强制终止、最多再等 5 秒；强制终止不表示持久化成功。
- 普通 `AppServerClient::shutdown` 仍是任意字节流的 transport-only 关闭，
  不要求外部 peer 退出。两个路径共用 worker，但只在自有进程的关闭路径
  排空输出。退出时仍待响应或排队中的请求明确终止，关闭后的 clone 不再发新请求。
- join 期间句柄保留在共享槽位，取消等待不会把任务句柄丢失；自有进程
  Drop/取消的应急路径请求终止并 abort readers，不宣称异步收尾或 reap 完成。

## 测试证据

- 最初两项均失败：正常退出缺少 EOF 后收尾标记；非零退出却返回 Ok。
  保留这些断言后实现修复，两项通过。
- Rust SDK 当前 **8 项**：原 3 项协议测试，加 EOF 后 2 MiB stdout 与
  2 MiB stderr、延迟写标记和 clone/pending 关闭；非零退出；真实 30 秒
  超时；普通 transport 不等待仍存活 peer；取消 drain 等待后保留 worker，
  随后 EOF 可重新等待完成。子进程夹具通过 Node 启动，Node 需在 PATH。
- CLI 的 **2 项**：原真实 Core 连接，以及实际模型请求仍在进行时调用
  Rust SDK shutdown。先直接用 `ThreadStore::load_all` 读取数据库，证明
  已保存 idle Thread / interrupted Turn，再重开 Core 比完整历史。这里的
  Store 初始化不是只读 SQLite 连接，但不会调用 Core 的启动恢复；不通过
  `Core::persistent` 读取来掩盖旧 inProgress 状态。
- 测试编译曾误用不存在的 `load_threads`，按真实存储接口修为 `load_all`。
  Clippy 首次指出 raw string hash、局部 import 和测试函数长度，均按代码
  结构修正，没有关闭 lint 或放宽测试时限。

## 首次全库失败与复验

首次 `cargo test --workspace` 的 SDK 组 **7/8**：非零退出测试在现有 5 秒
等待处超时；此前 package-only 同一测试通过。全部进程确认结束后，为
夹具增加 EOF/stdout/stderr/saved 阶段诊断，没有改生产逻辑或超时时间。
同 workspace 命令过滤该用例 **1/1，0.29 秒**通过；随后完整工作区复验
**703/703**，另外 26 项保留在显式浏览器/参考组。

本次复验 App Server 452/452（14.21 秒）、HTTP 36/36（4.03 秒）、Rust SDK
8/8（30.47 秒）、SDK/CLI 2/2（2.93 秒）、CLI stdio 4/4（1.46 秒）。
复验通过不证明首次偶发超时的根因已修复，失败记录仍保留；不能以“单独
通过”追认首次全库为通过。严格 Clippy 最终 **1.60 秒**、fmt/diff 检查通过。

## release 与客户端门禁

- [x] `CARGO_INCREMENTAL=0 cargo build --release --workspace`，**1.02 秒**。
  Rust SDK 本机库 `target/release/libqwenpaw_app_server_client.rlib` SHA-256：
  `04f71b0cd5a065f72af2462a513110681595a1443e885ff82b674bc457f16046`。
  这是本机编译产物，不是已发布或跨平台可安装的 Rust crate。
- [x] `CARGO_INCREMENTAL=0 cargo test --release -p qwenpaw-cli --test sdk_client`，
  编译 **2.47 秒**，优化版客户端对真实 source Core **2/2，1.15 秒**。
- source Core SHA-256：
  `e630c45861b71c1b97c0e222d7a5a8cdcf3a1968e3defc44b129e6d61f3c4860`。
- [x] 对接上述 source Core 顺序检查：TypeScript 编译与 **9/9，30.394 秒**；
  Python **5/5，0.606 秒**（conda `qwenpaw`，验证实际源码导入路径）；VS Code
  编译与 **57/57，0.198 秒**。全部无跳过，未激活原生扩展。
- [x] 收尾确认本轮构建、测试和源 Core 进程均结束，`console/src` 零 diff，
  `git diff --check` 通过；source Core 摘要未变，未删除旧制品或缓存。
- [ ] Rust SDK 分发、整套九类制品、原生/跨平台和全部功能验收。

## 仍然开放

Python SDK 的优雅关闭、默认 CLI/SDK Workspace 初始化、凭据优先级和后台
调度器单实例归属仍未完成。Core 最终保存失败仅 warning 的错误传播也未
改变；正常存储测试不提供所有磁盘故障下的保存确认。原始通知订阅仍是
广播 API，不提供每次关闭的持久化确认。

本轮未重新运行未变更前端的 2453 项及显式浏览器/参考 26 项，最新相应
结果见 [stdio 服务端验收](stdio-host-lifecycle-acceptance.md)。原前端源码
零 diff 不等于所有产品功能、原生窗口和各 OS 已完成。整套 `vGGX8Z` 包
不包含后续 stdio/SDK 变更；TS 单独包 `Iie6Ho` 只包含 TypeScript SDK，不携带
Rust SDK 或 Core。
