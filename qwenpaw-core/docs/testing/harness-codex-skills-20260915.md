# Codex Provider Skills 发现验收

2026-09-15；按 [Harness 方案](../architecture/harness-runtime-parity.md) 实现原 Codex `discover_skills` 控制链路，复用已有 Provider、生命周期和双向协议组件。没有修改原前端或新增 Python 产品依赖。

## 实现与验证范围

- [x] 发出原 `skills/list` 请求，参数完整保持 `cwds: [cwd]`、`forceReload: false`；请求工作目录与启动进程的 host cwd 分离。
- [x] 输出完整原 `HarnessDiscoveredSkill` 字段：name、description、provider_id、source、enabled、read_only、scope。固定只读/provider 标记，不暴露上游 path 或其他私有字段。
- [x] 按 `(name, scope)` 保序去重，保留首项描述及 enabled；同名不同来源仍分别保留。忽略非对象项目、空名称及原方法不读取的附加字段；enabled 缺失为 true，null 为 false，非空字符串遵循原 truthiness。
- [x] 协议错误不转空列表；超时和调用方取消清理 pending waiter，不宣称取消远端执行。数据解析失败不返回此前积累的部分列表。
- [x] Provider 使用现有所有者启动/请求，缺失安装返回错误；stop 后能重新发现，shutdown 后拒绝调用。

新增 6 个客户端普通测试（含 1 个 Unix 非 Unicode 路径测试）、1 个 Provider 测试，并扩展既有缺失安装测试。Provider 测试调用实际 Rust 子进程与管道；命令工厂仍替换为测试入口，返回合成 skill 数据。它验证进程控制及请求工作目录，不证明真实 Codex 扫描磁盘上的技能，也不证明完整多 Agent workspace 隔离。

原 `scripts/codex_control_reference.py` 增加提取未改动的 Python `discover_skills` 方法。四组案例覆盖 null/空响应、混合项目与跨来源重复、布尔/整数及默认字段；完整比较请求和返回结构，原模型/账号的 13 组对照仍保留。参考程序仅使用显式 RPC 替身，不导入产品启动器、不查找真实安装或凭据；Python 单测新增完整 Skills 结构断言。

## 结果

| 检查 | 结果 |
| --- | --- |
| 新 Skills 普通测试 | 7/7，通过既有缺失安装扩展 |
| 组件普通组 | 61 passed、0 failed、4 ignored，含 1 个 Rust fixture 入口；8.04 秒 |
| 带空格目录源码构建/运行 | 同组 61/0/4；23.64 秒 |
| 显式原实现对照 | 4/4 测试；新增 Skills 四组，既有控制面 13 组、发现 18 组、三项目录/四类完整状态均通过；14.55 秒 |
| Python 控制参考单测 | 5/5；1.138 秒 |
| 全 workspace | 932 passed、0 failed、46 ignored，含 2 个 doc tests |
| 静态检查 | workspace/all-targets Clippy `-D warnings`、cargo fmt、Black、79 列 Flake8 通过 |

默认忽略的 46 项中四项已另行显式执行；其他此前 42 项原 UI/参考测试本轮未重跑，不算作本轮通过。未新增 lint 抑制；既有能力 DTO 的具名 expectation 保留，Python 仍仅忽略 f-string 要求导致的 F541。

[最终校验记录](../../../dist/qa-harness-skills-20260915-QInPw8/verification.json) 于 `2026-09-15T00:29:53.478Z` 通过；命令终态、日志和当前源码哈希位于同目录。Cargo.lock/所有 manifest 未变；原 2940 个构建来源仍只包含前轮新增 Harness 成员对应的 manifest/lock 差异，其余来源未变。本轮有意扩展控制参考程序及其单测，其他既有参考文件保持不变。

九包、source release Core 哈希与原 Console 状态均复核未变。未重建包、未执行包内 Core、未运行真实 Codex/Qoder、未访问系统凭据、未激活 VS Code 或启动原生桌面，也没有 commit/push。

## 未完成项与差异

本切片是 Provider 只读发现，不是技能投影/执行或 Skills HTTP 接口。上层仍需完成当前 workspace 选择、backend 配置绑定、缺失 runtime 时的原空列表/提示，以及原页面端到端测试。

对非规范上游输入不宣称任意 Python 行为等价：例如浮点数/容器字段的 Python `str()`，非数组 data/skills 的可迭代行为未完整实现；Rust 返回类型错误。非 Unicode 路径明确拒绝而不有损转换；Windows/Linux 实机、特殊路径及调用方路径规范化仍待验收。启动发现/队列等待不计入本次 skills RPC timeout，沿用已有所有者边界。

- [ ] MCP 原实现另起 `codex mcp list --json`，需补短命进程的输出、错误和取消/超时回收，再映射完整 MCP DTO。
- [ ] capability fingerprint、会话/线程恢复及 Codex/Qoder Agent 执行、审批、取消、命令和附件。
- [ ] 七个 HTTP 接口、原页面全流程等价、新九包重建及逐包验证。
- [ ] 真实账号、原生桌面/VS Code、包内 Core 与跨平台门禁。

总 goal 仍未完成，不从组件或静态目录通过推导完整产品可用。
