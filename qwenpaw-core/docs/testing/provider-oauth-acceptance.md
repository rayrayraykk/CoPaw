# 模型提供商 OAuth 本机验收

使用未经修改的 `console/src` production build、真实 Chrome、临时 Rust Core/SQLite、内存凭据存储与本机授权服务。不读取真实钥匙串，不需要用户的 API key，不运行 Python AgentScope。当前原版 provider OAuth 注册表只有 OpenRouter。

## 执行

在 `qwenpaw-core` 目录，Node 24+、Chrome 与 `console/dist` 可用时：

```sh
cargo test -p qwenpaw-app-server provider_oauth --all-features
cargo test -p qwenpaw-app-server original_provider_oauth_browser \
  --all-features -- --ignored --nocapture
```

浏览器门禁默认 ignored，必须显式运行；可用 `QWENPAW_CHROME` 指定浏览器。

## 验证内容

- 32 字节随机 state、state 绑定 callback_url、S256 challenge/verifier 和完整 start/status 响应；模拟服务实际校验 code 单次使用与 PKCE。
- 成功后凭据仅进入凭据存储；普通 JSON 不含 key；callback HTML 不含 state/key/上游错误，并带 CSP nonce、no-referrer 和 nosniff。
- 普通 API key 配置、授权重连、活动 Core 的实际 key 更新、持久化数据库重开及安全存储重新加载、清空 key 后 OAuth 状态恢复未连接。
- 缺失、错误、过期和重复 state；新授权淘汰旧会话；容量限制；不支持的提供商返回 404。
- localhost 与显式 HTTPS origin 的回调构造；拒绝恶意 Host/Origin；忽略伪造的代理和 Hub 回调头；已绑定 state 不接受另一个受信 origin 的回调。
- 拒绝授权、上游非成功/重定向/无 key/非法 key/超大响应；不暴露诊断内容，不覆盖旧配置。
- 模拟凭据写入先修改再失败，验证恢复旧 key，且不错误进入 completed。
- code 交换暂停期间手工换 key 或开始新授权，旧响应不能覆盖新状态；直接修改凭据存储但不改变 registry revision 也会被旧 key 指纹校验拒绝。
- 自动模型发现失败时登录仍成功，保持已保存的 key，与原版行为一致。
- 原 Chat 模型选择器 FREE → Connect OpenRouter → 原确认弹窗 Continue → 真实弹出页跳转 → 原两秒状态轮询 → 自动导航原模型管理 → Add Models/Filter/Add → 新文档加载仍已连接且模型仍存在 → 返回 Chat。

## 记录与边界

2026-09-09：8/8 普通 OAuth 测试及 1/1 显式原前端浏览器门禁通过，严格 Clippy 通过。浏览器测试早期选错了 Models 页入口，并错误依赖会随导航消失的成功 toast；按原源码修正测试，未改变前端或伪造配置状态。

完整回归：工作区 355/355 普通 Rust 测试与 5/5 显式浏览器门禁通过；Console 295 个文件的 2453/2453 测试和 production build（含 Monaco CSS、预压缩与 initial-bundle 校验）通过。重建 release Core 后，TypeScript SDK 3/3、Python SDK 4/4、VS Code 57/57 测试通过，真实 Core 连接未跳过。SDK 与 VS Code 编译、严格 Clippy、格式/diff、inventory 3/3 和快照校验通过；`console/src` 零改动。现有 9 个旧分发包不作为本次实现的验收证据。

本机授权服务器验证协议与应用行为，不等于 OpenRouter 真实账号验收。Hub 信任边界下的托管回调、DMG 内原生外部浏览器交接、Windows/Linux 安装态仍需后续验收。HTTP 交换有 30 秒超时和 16 KiB 响应上限；授权会话 TTL 为十分钟，最多 32 个。取消原弹窗只停止前端轮询，原版没有服务端取消接口，本次不新增不同的交互。SDK 仍通过 App Protocol 连接同一 Rust Core，未把 OAuth 或 Agent 执行搬到语言 SDK 中。
