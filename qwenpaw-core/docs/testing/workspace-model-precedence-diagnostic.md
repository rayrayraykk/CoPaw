# Workspace 模型启动优先级诊断

日期：2026-09-14，macOS ARM64。本次是默认 SDK 接入 Workspace 前的诊断，
不是已经修改 SDK 入口，也不是共享宿主验收通过。

## 已复现问题

直接把默认 CLI/SDK 切换到现有 `new_workspace_with_stores` 会引入密钥语义
变化：首次创建 provider registry 时，运行时保留调用方传入的 key；重开
已有 registry 后，使用凭据存储读取的 key 替换它。空存储或读取失败时，
即使本次启动传入有效的假 key，最终仍变成无 key。

测试并非仅检查 `api_key_configured`：本地 HTTP 模型要求认证，对每次实际
请求核对完整的模型名和假 key 来源标签，再检查轮次 completed/failed。
空存储及读取失败两组均表现为首次完成，重开后无 Authorization、模型
返回 401、轮次失败。诊断程序正常退出表示复现准确，不表示产品行为正确。

## 五组来源矩阵

每组独立目录，先验证空数据库的 Core 配置，再保存非秘密 base URL/model，
创建 Workspace、执行一轮，关闭全部对象后重新打开数据库及 Workspace。
重开时故意传入不同的 base URL/model 和新的假 key；完整 `ModelConfig`
在 Core 重开后及 Workspace 初始化后分别断言，而不是只检查某一个字段。

| 假凭据存储 | 首次 Workspace key | 重开 Core / Workspace key | 两次请求状态 |
| --- | --- | --- | --- |
| 空 | 首次调用方 | 新调用方 / 无 | completed → failed |
| 有默认 provider key | 首次调用方，首次不读存储 | 新调用方 / 存储 | completed → completed |
| 读取报错 | 首次调用方 | 新调用方 / 无，构造器仍返回成功 | completed → failed |
| registry 切换到自定义 provider | 首次调用方 | 新调用方 / 自定义 provider 存储 | completed → completed，自定义模型实际生效 |
| 调用方和存储均无 key | 无 | 无 / 无 | failed → failed，对照组 |

五组首次模型凭据读取次数均为 0，重开后均为 1；自定义 provider 读取其
专用 account，未错误读取默认 provider account。没有保存模型凭据。
已有 SQLite 的 base URL/model 覆盖重开时提供的值，但 key 来自本次调用方；
Workspace 随后又按 registry 选择 provider/model 和存储 key。因此不能简单
用“若存储为空则保留当前 key”修复，当前 key 未必属于最终选中的 endpoint。

## 证据与依赖对照

产品仓库输出：`dist/qa-model-precedence-20260914-YmkIW4`。

- `src/main.rs`、`Cargo.toml`：独立诊断源码，只调用当前公开 Core/Workspace
  构造器和 Core 轮次 API，使用内存假凭据；没有系统 keychain 调用。
- 首次 `run.log` / `run.json` / `diagnostic.json`：正常退出，编译 **25.53 秒**。
  独立 offline 解析中的部分 ICU 版本与仓库锁不同，首次锁文件另存为
  `initial-resolved-Cargo.lock`，不把该次结果单独当作锁定依赖证明。
- `locked-control/run.log` / `run.json` / `diagnostic.json`：复制当前仓库
  锁内容作为诊断项目的解析起点后，在新的数据目录完整复验；编译
  **22.11 秒**，**09:10:19–09:10:44 UTC**，退出 0，无信号。
  诊断实际使用的 **383** 个 registry 包，其 name/version/source/checksum
  全部在当前仓库锁中逐项匹配；仓库锁本身未改。
- 两次各五组、十次真实本地模型请求得到相同的来源/状态矩阵，仅随机端口
  和生成的会话标识不同。报告明确 `diagnosticPassed=true`、
  `productInvariantPassed=false`、`reproduced=true`。
- 五个关键生产源文件在执行前后摘要相同。本批不是 source release 二进制
  执行；不以诊断程序替代真实 SDK/协议构造器、原生端或包内 Core 验收。

既有 Workspace 重开测试使用的本地模型并不要求 Authorization，且假凭据
返回空；这些测试覆盖状态/检查点/归属，不能证明需要认证的 SDK 启动配置
在接入后仍然有效。

## 接入修复必须满足的约束

- [x] 完整配置和模型凭据 account/读取次数矩阵，含空、存在、错误和不同 provider。
- [x] 本地模型真实请求对照，保留无 key 失败对照，不使用生产凭据。
- [x] 仓库锁依赖对照复验，保留首次解析差异与全部诊断记录。
- [ ] 先选定 provider/endpoint，再解析与之关联的凭据；禁止把另一个 endpoint
  的启动 key 自动作为 fallback，不通过全局环境变量覆盖其他 Agent。
- [ ] 自有 SDK 的调用方配置需有明确、可重启的作用域；客户端连接共享宿主
  不注入自己的 key、不覆盖宿主模型。不能将两种模式混为同一优先级。
- [ ] 错误和缺失凭据分别定义；不要把读取失败静默变成成功初始化后请求 401。
  同时保留无需 key 的本地模型及原设置页重新配置能力。
- [ ] 将明确的优先级策略接入真实 CLI/三语言 SDK 后，复验首次/重启、显式
  Thread 模型、Agent provider、原页面配置、密钥不写入非秘密状态及失败回滚。

生产代码、原前端、默认退出规则、日常数据和已有九类 QA 包均未改。
宿主生命周期的用户选择仍见 [共享接入方案](../architecture/shared-host-client-attachment.md)。

后续补充的 [具体修复策略与实施清单](../architecture/workspace-model-credential-policy.md)
覆盖 SQLite 地址覆盖、动态改地址及 Agent/Console/Cron 二次取凭据，明确
推荐存储优先、身份绑定的启动凭据仅补缺失。该策略待确认，尚未实现；
本页历史失败结论不因增加方案而变为通过。
