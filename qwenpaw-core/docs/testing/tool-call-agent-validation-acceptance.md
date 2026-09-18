# 工具调用列表：Agent 校验回归

日期：2026-09-15。目标是保持原前端工具控制交互，同时避免无效 Agent 上下文回退到 default 的运行状态。实现只修改 `desktop_tool_calls::list_calls`，测试扩展既有真实 HTTP/Shell 集成用例；不修改原 Console 或 SDK，不引入新的默认策略。

## 证据与改动

代码核查发现两个错误吞并：Agent 标识解析失败时 `unwrap_or_else` 回退 default；Workspace/会话解析失败时经 `.ok().flatten()` 被转换为空列表。现在列表直接传播既有 Agent 和 Workspace 校验错误，只有有效上下文内查不到会话时返回原 `{"items":[],"total":0}`。

- 红灯：真实 Shell 正在运行时，`X-Agent-Id: ../default` 得到 200 而非 400；既有 default 查询确认当时有一项运行中的调用。测试于状态断言处失败，日志保留。
- 绿灯：相同用例通过无效字符 400、非法编码 400、未知 Agent 404 的完整错误结构；错误文案取自既有 Agent 校验器，不新造第二套规范。
- 兼容：省略、空串、空白、default 和两端空白 default 都继续定位原运行调用。对重复查询仅规范化会推进的 elapsed/剩余时限三个字段，然后比较完整列表结构；有效但不存在的 session 仍返回完整空列表。
- 真实操作：原用例继续验证后台切换、时限控制、取消、SSE 完成与最终输出，不把只有参数校验通过当成工具运行成功。

此次调查最初怀疑运行中的历史输出没有补发；原 Python `tool_calls/_stream.py` 明确是无历史存储的实时广播，故没有据此修改订阅语义或添加回放。此调查结论不证明 Rust 已实现所有工具增量输出功能。

## 本机验证

- [x] 红测试 0/1，修复后专项 1/1，补充非法编码和 default 变体后专项 1/1；都是扩展同一个用例，没有增加测试入口计数。
- [x] `cargo fmt --all --check` 通过。
- [x] 完整普通 Rust 工作区 842 通过、0 失败、31 ignored，120.11 秒；严格 App Server all-targets/all-features Clippy 通过。
- [x] [最终复核](../../../dist/qa-tool-call-scope-20260915-QZnln3/verification.json) 于 `2026-09-14T18:20:07.846Z` 通过：2927 条来源只有指定的生产/测试两文件变化，旧九个包和 release Core 字节未变；原前端及既有脚本来源保持。

日志和命令元数据：`dist/qa-tool-call-scope-20260915-QZnln3`。所有 Cargo 构建禁用 incremental/debug info；测试和检查 offline/locked，运行在 qwenpaw conda 环境。只使用临时 Workspace、内存凭据和本机模型 fixture。

## 交付边界

本轮不重跑 31 项显式原页面和 2453 项前端测试；其上一批结果见 [开发快照验收](qa-publication-packages-20260915.md)，不算作本轮执行。`6BiWlu` 九类包与 release Core 保留原字节，**不含本轮列表校验修复**。未启动包内 Core、原生窗口或 VS Code 激活，未 commit/push。

这是一个运行状态隔离修复，不是全功能交付。外部 Channels、插件/浏览器等缺口、恢复完整性、默认共享策略、原生与跨平台测试仍开放。
