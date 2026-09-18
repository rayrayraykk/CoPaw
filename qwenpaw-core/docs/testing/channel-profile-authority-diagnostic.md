# Channels / Agent Profile 配置权威诊断

日期：2026-09-14，macOS ARM64。输出为仓库 `dist/qa-channel-authority-20260914-fGNJcc`。原始报告确认产品缺陷；后续前置校验修复单独记录，不代表统一权威已完成。

## 方法与结果

独立诊断程序链接当前 source Core/App Server，在新目录启动真实 loopback HTTP，使用返回空值/拒绝真实保存的假凭据存储。先完成初始 16 个 HTTP 请求，再关闭宿主、重新打开同一 SQLite 和注册表，执行 6 个重开/复制请求。没有模型调用、系统 keychain、外部通道或包内 Core 执行。

| 对照 | 观察 |
| --- | --- |
| Channels PUT → Agent GET | Console 返回已保存前缀；Agent 详情 `channels` 仍为 `{}` |
| Profile PUT → Channels GET | Profile 返回并持久化另一前缀；Channels 仍返回前一个值，default/editor 完整配置不变 |
| 非法 Console 配置 | Profile PUT 对数字前缀/未知字段返回 200；相同 Console 对象经专用接口返回 400 |
| 未实现外部通道 | Profile PUT 接受启用 Telegram 与假 token，专用接口返回 501；不代表通道真实启动 |
| 文件与重开 | Profile 值同时存在于 `agent.json` 和注册表，Channels v2 保存另一份；重开后两份仍不同 |
| 复制控制 | 复制后的 Profile 清空 Channels，独立 Workspace 的 Console 使用默认配置；该原行为正常 |

`fixture-only-not-real` 是测试专用字符串，不是真实 token。诊断仅证明通用写入口不执行同一校验/门禁，以及该字段进入普通配置文件；不声称观察到真实凭据泄露或外部 Telegram 连接。

`diagnostic.json` 明确记录 `diagnosticPassed=true`、`productInvariantPassed=false`；初始和重开阶段完整请求/响应及 SQLite 设置分别在 `initial.json`、`reopen.json`。命令 `diagnostic.log`/`command-diagnostic.json` 退出 0 是准确复现，不是功能通过。

第一次 Profile 请求只携带 `channels`，符合当前 Rust 部分更新接口，但原 Python 请求还要求 `id`、`name`。因此另用新 `fixture-full-profile` 和完整必填字段重复 22 个真实 HTTP 请求，结果相同；`diagnostic-full-profile.json`、`full-profile-initial.json`、`full-profile-reopen.json` 保留独立证据，未覆盖原报告。

## 原版语义与前置校验实施

新增 [Python 对照脚本](../../scripts/channel_profile_reference.py)，提取原 `update_agent` handler AST，使用真实 Pydantic 模型及 ASGI 请求验证。配置存储替换成内存完整 JSON 序列化边界；只 stub 根配置读取、磁盘发布、无邮件的 driver 同步和重载调度，不导入或启动原应用，不测试真实磁盘/后台重载。

8/8 对照通过：缺失保留、null 未配置、空对象补齐默认、部分对象替换并补齐默认、完整往返、数字前缀拒绝、缺少 id/name 拒绝、Console 数组拒绝。原类型错误返回 422，Rust 专用接口既有 400 约定暂时保持，不能据此声称错误响应完全等价。原 `get_channel` 源码还明确在未配置时返回 404，该读取语义尚待统一存储实现。

新增 Rust 前置校验测试先得到 1 通过/3 失败：非法或外部配置返回 200；组合请求触发假凭据存储的禁止写入断言。接入共用 Console 解析后发现另一个真实缺陷：Serde 接受空数组为默认结构；专用接口因此曾返回 200。现显式要求 Console 为对象。加入容器/大小边界后 5/5 通过，验证 17 个外部通道各三种非默认变更、未知通道、文件/注册表/SQLite 未变、假凭据无写入及默认/null/缺失往返。失败日志均保留，不把第一次 green 命令误记为通过。

`guard-http.json` 使用第三个新 fixture，初始 18 请求、重开 2 请求。非法字段/数组和外部通道+假邮件凭据的组合请求均在发布前拒绝，文件与注册表字节不变。但有效 Channels/Profile 前缀仍不同，因此报告仍为 `productInvariantPassed=false`，阶段 `validationGatePassed=true` 仅表示新增校验。

全量默认结构对照另外发现 Slack 的 `allow_from` 和 `require_mention` 与原 Python 不同；不能仅用 Rust 自己的默认值测试证明原配置可回写，已加入原版全结构对照测试。

修正 Slack 后，直接传递 Python JSON 的 Rust 对照通过，但 Node 真实 HTTP 的完整默认回写仍返回 501：`JSON.stringify` 将 `poll_sec: 1.0` / `call_timeout: 120.0` 变为整数，原比较错误区分了等值 JSON 数字。`guard-http-final.log` 保留该失败，新 Rust 红测试独立复现。比较已修正为等值数字，新增第 6 项测试同时保证不同数值和字符串数值继续被拒绝。后续结果使用新 fixture 与 `*-validated` 命名，不覆盖失败报告。

## 最终前置校验验收

- 最终源码完整 Rust 工作区：764 通过、0 失败、31 显式项未在普通组运行；新增 6 项包含在 764 中，不重复计数。
- 另行运行原版全结构对照、原 Agents 页面、原 Channels 页面，3/3 通过；不是全部 31 个显式项重跑。两个页面均使用原构建产物，包含编辑/保存和宿主重开后的持久化断言。
- 原 Python 语义对照 8/8；最终 Node 真实 HTTP `guard-http-validated.json` 为 21 请求，默认差异为空，完整原默认配置回写成功；无效保存与假凭据组合请求不落盘，重开后保持。有效 Profile/Channels 值仍分歧，统一权威状态明确为 false。
- 最终工作区所有 target 的 Clippy `-D warnings`、`cargo fmt --all --check`、Python 语法/79 列检查通过。Clippy 首次发现新增条件可折叠，修正后重新运行了完整工作区和上述三个显式测试。
- 原 `console/src` 无变更；本轮没有重跑 2,453 项前端单测或重新安装 SDK/VSIX，保留上一批验收边界，不把旧记录重复当作本轮执行。

最终来源与结果核对输出为本诊断目录的 `verification.json`，对应 `verify.mjs`。来源核对初次未处理清单中既有 `deleted: true` 项而失败；修正诊断读取逻辑，未删除或恢复该历史文件。九类旧制品保持，source release 未重新构建，当前源码修复不在旧 DMG 中。

源码显示 `config_for_agent` 使用注册配置快照，但本诊断没有发起实际 Agent Turn，因此运行期配置消费者仍需单独测试，不能由本次 HTTP 结果代证。

## 当前边界与下一步

- 本轮仅新增/修改 Channels 与 Profile 的前置校验及配套测试、对照脚本；Console 源码与 SDK 未修改，没有重新打包。
- source release 仍为 `7dbb3b342d361f75bfd9d2eb6c349b181f43acac0076001291eb2cc98ab3b37a`；最新 `eaTvv4` QA 批次的既有验收保留，不把诊断视作其中已修复的功能。
- 下一步按 [统一权威方案及清单](../architecture/channel-profile-authority.md) 完成验证后，继续单一配置视图和组合发布回滚；不把前置校验称为双份存储的修复。
- 总目标未完成；这不是外部通道运行、原生激活或跨平台验收。
