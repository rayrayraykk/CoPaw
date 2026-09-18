# Codex 已连接控制面验收

2026-09-15；延续 [Harness 方案](../architecture/harness-runtime-parity.md)。在已有 Rust `CodexClient` 上补充账号状态、两种登录、注销及模型分页，没有引入第二套 transport 或 Python 产品运行时。此组件仍未接入 App Server/Agent，不能据此创建 Codex backend。

## 本轮完成

- [x] `account/read` 使用 `refreshToken: false`；仅保留原 `type`、`email`、`planType` 字段，区分缺失、显式 null、空对象与非空对象。`requiresOpenaiAuth: false` 不被当成已登录。
- [x] 浏览器登录保留 `type: chatgpt`、`useHostedLoginSuccessPage: true`、`appBrand: codex`；设备码使用 `chatgptDeviceCode`。注销等待远端回复，不直接读删凭据文件。
- [x] `model/list` 使用 `includeHidden: false`，从 null cursor 取完全部页面，保留顺序、重复项、原字段默认值与 reasoning efforts；不额外本地过滤 hidden。
- [x] 第二页失败返回完整远端错误；重复 cursor、非法数据显式失败；总分页 deadline 超时清除等待请求，不返回已取得的部分列表。
- [x] 真实 Rust 测试子进程通过 stdin/stdout 执行全部控制方法，核对结果和调用顺序；完整 RPC 参数在同一客户端的双向管道测试中断言。

OpenAI Docs 技能用于核对 [官方 App Server 文档](https://learn.chatgpt.com/docs/app-server) 的账号、登录与模型分页 wire 字段；产品默认值以原 `src/qwenpaw/harnesses/codex/adapter.py` 和 `harnesses/events.py` 为准。没有使用真实 Codex、账号或 API key。

## 对照方式与边界

新 `scripts/codex_control_reference.py` 从原 adapter 提取未经修改的四个异步方法 AST，使用原 `HarnessProvider` / `HarnessModel` 类型。测试替身仅提供已安装、已连接客户端和固定 RPC 回复，记录完整 method/params；不执行 adapter 构造、发现、会话文件、真实启动或凭据操作。

Rust 显式测试逐项比较 13 组输入的完整公开结果和请求数组：6 组账号、3 组模型、2 组浏览器登录、1 组设备码、1 组注销。状态仅投影 authenticated/account，不把完整 provider 元数据、installed、启动错误或 runtime 路径当成已验证。请求 ID 去除后比较，因为 ID 关联由通信测试单独验证。

覆盖有效协议字段及原布尔/整数转换；任意浮点数、容器的 Python `str()` 行为不保证等价，非法账号/分页结构返回 `InvalidFrame`。重复 cursor 和整体 deadline 是有界失败保护，不表示原异常输入行为完全一致。

## 验证结果

| 检查 | 本轮结果 |
| --- | --- |
| Rust 组件普通组 | 24 passed、0 failed、1 ignored；包含 1 个子进程 fixture 入口 |
| 带空格目录源码编译/执行 | 同组 24/0/1，8.26 秒 |
| 原 Python 方法显式对照 | 1/1 测试，13 组案例，2.05 秒 |
| Python 参考程序单测 | 4/4，1.059 秒；Conda qwenpaw |
| 完整 workspace | 895 passed、0 failed、43 ignored，含 2 个 doc tests |
| Rust 静态检查 | workspace/all-targets Clippy `-D warnings`、cargo fmt 通过 |
| Python 静态检查 | Black py311/79、Flake8 通过；只为全 f-string 约束忽略 F541 |

43 个默认忽略项包含本轮已单独运行的控制面对照及此前 42 个显式验收项；后者未在本轮重新执行，不能计为本轮已通过。Python 对照在最终格式修改完成后重新运行。带空格测试复用上一通信验收的 target 目录重新编译当前源码，运行 Cargo 生成的可执行文件，不复制二进制，也不宣称上一轮复制路径启动问题已定位。

日志与命令终态在 `dist/qa-harness-control-20260915-VofSWR/`。[最终校验](../../../dist/qa-harness-control-20260915-VofSWR/verification.json) 于 `2026-09-14T23:24:16.621Z` 通过，包含源码与制品哈希。

## 制品与剩余门禁

原 2940 个构建来源除已新增 workspace 成员对应的 Cargo.toml/Cargo.lock 外保持一致，52 个既有脚本及原 Console 不变。新 crate 六个文件和本轮两个参考脚本单列哈希。本轮 lock 仅在本地 `qwenpaw-harness` 依赖项加入已锁定 `serde`；移除这一行即还原上一通信验收 lock 哈希，无外部依赖升级。

NvQ0h0 九包和 source release Core 哈希不变；它们不包含本轮控制面或上一轮通信组件。没有重建包、执行包内 Core、启动原生 Desktop、激活 VS Code、读真实凭据、commit 或 push。

- [ ] 安装发现、纯 Rust 分发 candidate、完整 provider 状态与 workspace 配置替换。
- [ ] Qoder 协议、Agent 生命周期、聊天/审批/取消/附件/命令与历史恢复。
- [ ] 原 UI 完整 Harness 对照及受影响制品重建、逐包验收。
- [ ] 真实账号、原生与 Windows/Linux/macOS 跨平台验收；总 goal 仍未完成。
