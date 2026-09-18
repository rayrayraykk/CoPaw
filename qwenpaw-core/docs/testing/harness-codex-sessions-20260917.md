# Codex 会话持久化与恢复验收

测试于 2026-09-15 完成；2026-09-17 核验当前源码、命令终态和保留安装包。不是两天持续后台测试。

Provider 可显式打开单 workspace 的会话 owner，连接能力客户端池、`thread/resume` 和 `thread/start`，持久化原 `codex_sessions.json`。状态探测不创建会话目录；reset 发布删除，stop 保留映射。loaded 标记绑定进程 generation。已确认的远端 ID 在保存失败后暂留内存，重试不重复创建；进程崩溃或关闭后的未发布 ID 不保证恢复。

- 新增 12 项普通测试（session/store 11、Provider 1）：并发、重开、配置切换、进程退出、reset、写失败重试、取消等待、Drop、原默认/自定义参数与 Unix 权限。
- 原 `_thread_for_session` / `reset_session` 方法体六组完整请求、结果与最终映射对照；Python 参考程序 2/2。参考使用内存存储与 RPC 替身，不证明真实文件/进程行为，后者由 Rust 测试分别验证。
- 组件及带空格目录各 108 passed / 0 failed / 7 ignored；七项显式参考全部通过。整个 workspace 979 passed / 0 failed / 49 ignored，包含 2 个 doctest。
- Rust fmt、workspace 严格 Clippy、Python Black/Flake8 通过。初始 Clippy 参数拷贝/函数长度、Python W503/E501/格式检查失败已修正，失败日志保留；未放宽 lint。

证据：仓库忽略目录 `dist/qa-harness-sessions-20260915-yhCGFI/` 的逐命令日志、`command-*.json`、`verify.mjs` 和 `verification.json`。最终来源核验为 `2026-09-17T07:43:40.181Z`；workspace 命令于 `2026-09-15T01:46:20.079Z` 退出 0。

原 2940 构建来源、61 个既有脚本、原 Python Provider/投影来源、原前端、旧九包与 source release 均核对不变。新增会话模块/测试与两个参考脚本；tempfile 从 Harness dev dependency 移至运行依赖，Cargo.lock 和根 manifest 不变、无依赖升级。本轮没有重建安装包、commit 或 push。

边界：调用方须保证每个状态目录只有一个 writer，尚未实现多进程文件锁或目录替换竞态防护；原子替换不证明断电持久性。状态文件符号链接/权限错误拒绝，不沿用原吞掉所有 I/O 错误的行为；非规范容器型 thread ID 不模拟 Python 任意字符串化。resume 仅在远端协议拒绝后新建，断连/超时直接报错。原聊天/审批/命令/历史恢复、完整 resolver、Qoder、HTTP 和新包仍未完成。
