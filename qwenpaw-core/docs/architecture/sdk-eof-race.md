# Rust SDK EOF 关闭竞态排查

延续用户已批准的全功能/逐客户端本机验收，以及总计划 §14.2.24.60 的 SDK 失败门禁。原前端、安装包、真实凭据与默认宿主策略不在本次变更范围内。

上一轮默认并发 workspace 两次在正常退出的 5 秒等待超时；单项及串行 workspace 通过。不能据此判定负载或 SDK 正确。现有子进程写入 `initialize / initialized / fixture/wait / eof / stdout-drained / stderr-drained / saved` 阶段，但失败用例没有输出该诊断。

## Checklist

- [x] 保持原超时和断言，补全失败阶段诊断；默认 SDK/workspace 与新增 64 个独立运行时子进程测试已执行。捕获的是准备屏障失败，不是历史 EOF 的受控重现。
- [ ] 用证据区分请求队列、stdin EOF、stdout/stderr 排空与进程退出；取得根因后仅修改相关实现或错误测试夹具，不通过永久串行化、加时或重跑掩盖失败。
- [x] 准备屏障失败后分离初始化阶段，原四进程断言转绿；最终默认并发 workspace 847/847、SDK 22/22、真实 Core SDK 集成 3/3、严格检查通过。原 EOF 根因仍属于上一未完成项，见 [验收记录](../testing/sdk-eof-race-acceptance.md)。
- [x] 本次生产源码未改，不重建 release/SDK。九类旧包不包含此前插件/工具校验修复；这些生产变更的分发与总目标继续开放。

日志在 `dist/qa-sdk-eof-20260915-bwshAx`，使用 conda qwenpaw、Node 24、离线锁定 Rust 依赖与独立临时目录。无需访问日常数据、系统 Keychain 或运行包内 Core。

## 准备阶段与关闭阶段分离

新增独立运行时 64 次进程周转后，全 workspace 捕获到既有四进程测试的准备屏障 5 秒超时（`stdio_shutdown_tests.rs:148`），尚未进入 `shutdown()`。这与上一轮关闭时超时不是同一证据。修正夹具为并发初始化四个进程、收集全部初始化结果后，再建立屏障并同步调用 shutdown。保留四轮、每轮四进程、屏障 5 秒和关闭 5 秒；不改变 SDK 初始化默认超时、不减少并发或跳过断言。原 EOF 超时根因继续开放。
