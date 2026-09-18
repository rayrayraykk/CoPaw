# 桌面 Core 生命周期验收

日期：2026-09-09。对应计划 §14.2.24.35。范围仅为原 Tauri 壳的进程管理和测试编译条件，不改变 Console、App Protocol、SDK 或 Core 推理代码。

## 复现与修复

旧实现新增 `closed_process_clears_the_ready_port` 后，执行：

```sh
cargo test --locked --lib backend::tests -- --nocapture
```

结果 1 通过、1 失败：进程结束后的 `port()` 实际返回 `Some(54321)`，预期 `None`。修复后相关测试通过。

代码检查还发现，原 `restart_backend` 的 stop/start 没有覆盖整个操作的互斥锁。并发调用可共同等待旧进程后各自启动新进程，后一次赋值覆盖前一个 child。现在使用异步生命周期锁串行化 stop/start；退出先设置终止标志，已等待旧进程和排队中的重启都不再启动替代进程。

代次与状态更新放在同一状态锁内，消除旧事件检查代次后、新代次已开始、旧事件再修改状态的窗口。异常事件立即清空 ready port。事件流无终止确认地关闭时保留进程管理状态；两次终止等待均失败也不再清空 receiver，因此重复点击 Retry 不会绕过失败启动另一个进程。

七个新增测试覆盖：

- 异常退出清理旧端口并保留错误；
- 旧代次的 ready/error/close 不影响新代次；
- 事件流关闭但终止未知时保留管理状态；
- 两个重启依次等待各自前一个进程，不重复占用替代进程槽位；
- 退出阻止正在等待和排队的重启，退出后也拒绝重启；
- 终止通道异常关闭后连续两次重启都失败，零替代启动；
- spawn 错误向调用方返回，后续正常重试不被永久禁止。

并发测试显式轮询 future 和发送 watch 信号，不依靠 sleep 或随机调度命中竞态；这些是状态机单元测试，不冒充真实进程或窗口测试。

## 验证记录

- 修复后 `cargo test --locked --lib backend:: -- --nocapture`：13/13 通过。
- `cargo test --locked --all-targets`：桌面 library 28/28，helper 40/40，共 68/68 通过。
- 首次 `cargo check --locked --release --all-targets`：失败，现有 helper 测试引用的解析/截断函数及其常量、类型在 macOS release 被 cfg 排除。修复仅增加 `test` 分支，不改变非测试的正式 helper 路径。
- `cargo test --locked --release --all-targets`：修复 cfg 后桌面 library 28/28、helper 40/40，共 68/68 通过。
- `cargo build --locked --release --bins`：桌面主程序和原生 helper 正式二进制构建通过；没有运行主程序或把此结果当作 bundle 验收。
- `cargo clippy --locked --lib -- -D warnings`：未通过，四处现有 lint：`backend_download.rs` 两处 redundant closure，`computer_use_runtime.rs` 和 `updates/version.rs` 各一处 type complexity。未关闭 lint，也没有为此重构无关业务代码。
- 已修改 backend 模块格式与 `git diff --check` 通过，`git diff --numstat -- console/src` 输出为空。
- helper 测试还报告已有的 unused `Cursor` import；依赖 `block 0.1.6` 报告 future incompatibility，未把这些警告描述为零警告通过。

## 尚未覆盖

本轮没有启动完整 Tauri 窗口：桌面壳强制默认 app data，Core 默认系统凭据服务也不随外部临时目录隔离。尚需确定不接触日常数据和凭据的安装态测试方式；没有改变用户 HOME、凭据存储、安全设置或读取真实 key。

当前环境未提供可直接控制原生窗口的测试工具。Chrome 的原页面测试不能证明 WKWebView、原生 bridge、托盘、窗口关闭确认或完整退出流程等价。状态测试与 release 编译也不替代这些门禁。

此前 `dist/qa-runtime-20260908-fRyKH7/` 九类 QA 制品未在本轮重建，因而不含本轮桌面生命周期修复。其首次启动 SIGKILL 与原路径后续复测通过仍按原记录保留，不能由本次状态修复推断冷启动问题已解决。完整 GUI、新版安装包验收、生产签名/公证和剩余原功能仍未完成。

后续 §37 已将生命周期修复打入 `dist/qa-runtime-20260909-OPX10W/` 的新 DMG/ZIP，见[制品验收](qa-responses-packages-20260909.md)。包内 Core 实际执行通过，但没有启动完整原生窗口；这不替代本节尚未覆盖的桌面桥接、托盘与退出验收。
