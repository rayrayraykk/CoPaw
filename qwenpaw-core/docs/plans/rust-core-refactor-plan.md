# QwenPaw Rust Core 重构计划

> 状态：Approved / 执行中
>
> 创建日期：2026-09-01
>
> Core 工作区：`/Users/qbc/Desktop/repos/as/qwenpaw/qwenpaw-core`
>
> 产品仓库：`/Users/qbc/Desktop/repos/as/qwenpaw`
> 参考项目：[`rayrayraykk/CoPaw`](https://github.com/rayrayraykk/CoPaw)、[`openai/codex`](https://github.com/openai/codex)

## 1. 背景

当前 QwenPaw 主要由 Python 后端、React WebUI 和 Tauri 桌面封装组成，后端同时承担：

- Agent Loop 与上下文管理；
- 模型 Provider；
- 工具调用、MCP、Skills 与插件；
- Workspace、文件、Checkpoint 与 Sandbox；
- 审批、权限和治理；
- 会话、记忆和持久化；
- Cron 与后台任务；
- 钉钉、飞书、Telegram、Discord 等消息渠道；
- Web API、TUI 和桌面应用的运行时支撑。

本次重构希望将这些能力逐步迁移到 Rust Core，并让同一个 Core 服务于：

1. VS Code 插件；
2. Tauri 桌面应用；
3. 现有 WebUI；
4. 现有 CLI、TUI、远程访问和消息渠道客户端。

现有前端的页面、交互和视觉不在本次重构范围内。重构过程中应优先保持现有 WebUI 的 API 契约和用户行为。

## 2. 调研结论

### 2.1 QwenPaw 现状

初步只读调研显示：

- 用户指定的 `rayrayraykk/CoPaw` 是 `agentscope-ai/QwenPaw` 的 fork；
- Python 包位于 `src/qwenpaw`；
- WebUI 位于 `console`，技术栈为 React、TypeScript、Vite；
- 桌面端使用 Tauri；
- 当前代码已经包含 Agent Runtime、App、Channels、Drivers、Governance、Plugins、Providers、Sandbox、Services 等多个边界；
- 当前 Python 依赖包含 AgentScope、WebSocket、Uvicorn、APScheduler、MCP、多个消息渠道 SDK、浏览器与本地模型相关依赖；
- 当前项目已经存在测试分级和 contract / integration / E2E 的概念，可以作为迁移验证基础。

因此，不建议直接进行逐文件、逐类的 Rust 翻译，也不建议一次性删除 Python 后端。

### 2.2 Codex 可借鉴的部分

Codex 的 `app-server` 已经采用“无 UI Rust Core + 多客户端协议”的结构，适合借鉴：

- 使用 Thread、Turn、Item 表达会话和一次 Agent 执行；
- 使用双向请求、响应、通知承载流式事件；
- 将审批建模为 Server 向 Client 发出的请求；
- 支持初始化握手与客户端能力声明；
- 支持 stdio、socket、WebSocket 等不同传输；
- 从 Rust 类型生成 TypeScript 和 JSON Schema；
- 使用有界队列、背压、取消和明确的过载错误；
- UI 只消费协议，不直接依赖 Agent 内部实现。

QwenPaw 不应直接依赖或 Fork Codex Core。两者业务目标不同，本项目只借鉴其架构边界、协议生命周期和工程实践。

## 3. 重构目标

### 3.1 最终目标

- Core Runtime 使用 Rust 实现；
- WebUI、Desktop、VS Code 使用同一套领域模型和协议；
- 现有 WebUI 业务代码原则上保持不变；
- Core 可以作为独立进程运行；
- 支持 macOS、Linux 和 Windows；
- 本地场景默认不依赖 Python 环境；
- Agent、工具、会话、审批和配置具有明确、可测试的边界；
- 网络暴露默认安全，远程访问必须显式启用并配置认证；
- 迁移过程中始终存在可运行、可验证的版本。

### 3.2 第一阶段 MVP

第一阶段只建立一个完整的 Rust 垂直闭环：

- 启动 Rust Core；
- 托管现有 WebUI 构建产物；
- 配置 OpenAI-compatible / Qwen 模型；
- 创建、恢复和持久化会话；
- 流式返回 Agent 消息；
- 支持文件、Shell 和 MCP 工具；
- 支持 allow / deny / ask 审批；
- 支持中断正在执行的 Turn；
- 提供 VS Code 最小连接示例；
- Tauri 能够启动 Rust Core sidecar。

### 3.3 非目标

以下内容不纳入第一阶段 MVP：

- 重新设计或重写 WebUI；
- 一次性实现所有消息渠道；
- 一次性复刻全部 Browser / Computer Use；
- 一次性迁移所有 ReMe 和知识库能力；
- 一开始就实现远程集群调度和多租户；
- 直接兼容 Codex app-server 的全部 API；
- 为尚未出现的业务场景预先设计抽象。

## 4. 核心原则

### 4.1 兼容优先

先冻结现有前端依赖的 API 契约，再实现 Rust 兼容层。不能依靠肉眼判断“前端应该可以工作”。

### 4.2 垂直迁移

每一阶段都应产生完整、可运行的用户路径。避免先重写所有数据结构，再等待数月才得到可运行产品。

### 4.3 协议与实现分离

客户端只能依赖协议 crate 和生成的 SDK，不能依赖 Agent Runtime、数据库或工具实现。

### 4.4 单一领域模型

Web 兼容 API 可以保留旧的 HTTP payload，但进入 Core 后必须转换为统一的 Session / Thread / Turn / Item 模型。

### 4.5 安全默认值

- 默认只监听 loopback、stdio 或本机 IPC；
- WebSocket 远程监听需要显式配置；
- 远程连接必须使用认证；
- Shell、文件写入和敏感工具必须经过策略与审批；
- 路径必须规范化并限制在允许的 Workspace Root 中。

### 4.6 跨平台优先

- 路径使用 `Path` / `PathBuf`；
- 不在领域层拼接 `/` 或依赖 Unix 路径；
- IPC 为 Unix socket 和 Windows named pipe 提供平台适配；
- 进程、信号、权限与 Sandbox 通过 trait 和平台模块隔离。

## 5. 建议的仓库结构

采用同一产品仓库内的可提取 Core 边界。原 QwenPaw 仓库继续作为主产品、客户端和发行仓库，保留其社区入口、历史与 Star；`qwenpaw-core/` 是逻辑独立且可在未来抽出的 Rust workspace：

```text
qwenpaw/qwenpaw-core/
├── docs/
│   ├── plans/
│   ├── architecture/
│   ├── migration/
│   └── api-contract/
├── references/
│   └── codex/                   # openai/codex，只读参考
├── Cargo.toml
├── crates/
│   ├── qwenpaw-protocol/          # App Protocol 的 Rust 类型源
│   ├── qwenpaw-app-server/        # stdio / WS / WSS 与 Web 兼容边缘
│   ├── qwenpaw-app-server-client/ # Rust 客户端，不包含 Agent 业务逻辑
│   ├── qwenpaw-core/              # Thread / Turn / Agent Runtime
│   ├── qwenpaw-tools/
│   ├── qwenpaw-mcp/
│   ├── qwenpaw-storage/
│   └── qwenpaw-cli/               # `qwenpaw-core app-server`
├── sdk/
│   ├── typescript/                # Node/VS Code SDK
│   └── python/                    # Python SDK
└── scripts/

qwenpaw/
├── console/                     # 现有 WebUI
├── console/src-tauri/           # 现有 Desktop
├── extensions/vscode/           # VS Code 插件
├── src/qwenpaw/                 # 迁移期 Python legacy service
├── packaging/                   # Core 版本锁定与产品打包
└── tests/                       # 产品兼容与 E2E 测试
```

说明：

- `qwenpaw/console` 是现有前端源码，重构期间不做 UI 改造；
- 开发模式可以直接构建该目录；
- 发布时将 `console/dist` 作为静态资源打包给 Rust Server；
- 不使用跨仓库符号链接，避免 Windows 和打包环境兼容问题；
- `references/codex` 不参与 QwenPaw 编译；
- 产品仓库通过版本清单和校验值锁定发布 Core binary，不以 Git submodule 作为用户安装方式；
- 本地开发直接使用仓库内 `qwenpaw-core/target`，目录边界仍保持独立依赖和质量门禁。

## 6. 目标架构

```text
┌────────────────────────────── Client / Product Layer ──────────────────────────────┐
│                                                                                    │
│  VS Code Extension       Python application    Existing CLI / TUI*   Remote client │
│          │                       │                      │                   │         │
│          ▼                       ▼                      ▼                   ▼         │
│  TypeScript SDK           Python SDK        Rust app-server-client   language SDK  │
│          └───────────────────────┴──────────────────────┴───────────────────┘         │
│                                          │                                         │
│                              QwenPaw App Protocol v3                               │
│                              stdio / WS / authenticated WSS                        │
│                                          │                                         │
│  Existing React WebUI ── unchanged HTTP/SSE ── Web Compatibility Adapter           │
│                                          │                                         │
│  Tauri Desktop ───────────── process ownership / lifecycle ────────────────┐        │
└────────────────────────────────────────────────────────────────────────────┼────────┘
                                                                             ▼
┌────────────────────────────── App Server Host ─────────────────────────────────────┐
│ qwenpaw-core app-server                                                            │
│ initialize · request routing · notifications · approval · cancellation · transport │
└──────────────────────────────────────────┬─────────────────────────────────────────┘
                                           ▼
┌────────────────────────────── Rust Core Runtime ───────────────────────────────────┐
│ Thread / Turn / Item state machine · Agent loop · Model · Tools · MCP · Storage     │
│ Workspace boundary · credential boundary · bounded execution                        │
└────────────────────────────────────────────────────────────────────────────────────┘
```

`*` CLI、TUI、消息渠道和 Hub/远程能力已经存在。图中的 SDK 是逐入口迁移边界；
当前 Python 实现继续保留，只有在对应能力清单和回归测试达到等价后才允许切换。

该分层对齐 Codex 的核心思路：SDK 不实现 Agent Loop，也不直连数据库；SDK
负责启动或连接 App Server、完成 initialize、关联请求/响应、消费通知并暴露
语言友好的 Thread/Turn API。App Server 是稳定宿主边界，`qwenpaw-core` crate
才是领域运行时。现有 WebUI 为保持业务源码不变，继续通过 App Server 内的
HTTP/SSE 兼容适配器进入同一个 Rust Core。

SDK 首批只实现有真实消费者的 Rust、TypeScript 和 Python。Go、Java 等语言
必须等到出现调用方后再增加，避免复制尚未稳定的高层 API。

这里的 SDK 改造是接入层重构，不是产品功能裁剪。现有 CLI、TUI、远程访问、
消息渠道及旧 Python 版本在对应 Rust 接入完成前必须继续可用；任何入口切换
都必须先有等价能力清单和回归测试，禁止以“后续客户端”为由移除既有功能。

### 6.1 App Protocol

内部协议参考 Codex app-server 的资源化命名，但只实现 QwenPaw 需要的部分。
`qwenpaw-protocol` 是唯一协议类型源；各语言 SDK 只能消费生成物和固定的
方法表，不得各自发明 wire payload。

初始资源：

- `initialize` / `initialized`；
- `thread/start`；
- `thread/read`；
- `thread/list`；
- `thread/resume`；
- `thread/archive`；
- `turn/start`；
- `turn/interrupt`；
- `model/list`；
- `config/read`；
- `config/write`；
- `workspace/read`；
- `workspace/list`。

初始通知：

- `thread/started`；
- `turn/started`；
- `turn/completed`；
- `item/started`；
- `item/updated`；
- `item/completed`；
- `item/agentMessage/delta`；
- `thread/tokenUsage/updated`。

Server 发起的初始请求：

- `tool/approval/request`；
- `credential/request`；
- `userInput/request`。

### 6.2 核心领域对象

#### Thread

表示一段可恢复的长期会话，包含配置快照、工作区、创建来源和多个 Turn。

#### Turn

表示用户的一次输入以及 Core 为完成该输入所进行的一轮执行。一个 Thread 同一时间默认最多运行一个 Turn。

#### Item

表示 Turn 中可以持久化和流式更新的原子事件，例如：

- UserMessage；
- AgentMessage；
- ReasoningSummary；
- ToolCall；
- ToolResult；
- ApprovalRequest；
- FileChange；
- CommandExecution；
- Error。

#### Approval

审批必须是协议中的一等对象，而不是某个 UI 的弹窗实现。客户端掉线、超时、拒绝时必须有明确状态。

### 6.3 传输层

| 场景 | 默认传输 | 说明 |
|---|---|---|
| VS Code 本地插件 | stdio | 插件启动 Core，部署简单 |
| Desktop | loopback HTTP/SSE | Tauri 管理 Core sidecar，现有 WebUI 通过兼容适配器接入 |
| WebUI | HTTP + WebSocket/SSE | 兼容现有前端 |
| 本地调试 | stdio / WebSocket | 可使用协议检查客户端 |
| 远程连接 | WSS | 必须启用认证与 TLS |

协议层与传输层必须解耦，同一请求处理器不能包含 stdio 或 WebSocket 专用逻辑。

### 6.4 Web Compatibility API

该层负责：

- 保持现有 HTTP path、method 和 payload；
- 将旧 API 请求转换为 Core command；
- 将 Core event 转换为现有 SSE / WebSocket 事件；
- 保持现有错误码、空值和分页行为；
- 在迁移期间将尚未实现的路由代理到 Python 后端。

兼容层不能包含新的 Agent 业务逻辑。

### 6.5 Agent Runtime

初始模块边界：

- `AgentRunner`：驱动一次 Turn；
- `ModelProvider`：统一模型请求和流式响应；
- `ContextBuilder`：构建有硬上限的上下文；
- `ToolRegistry`：注册和发现工具；
- `ToolExecutor`：执行工具并产生事件；
- `ApprovalService`：执行策略并等待客户端审批；
- `ThreadStore`：会话与事件持久化；
- `EventSink`：向一个或多个客户端发布事件；
- `CancellationToken`：中断 Turn 和工具执行。

不为单一实现提前建立复杂插件框架；trait 只放在确实存在多个实现或测试替身的边界。

### 6.6 Storage

MVP 建议：

- SQLite 保存 Thread、Turn、Item、审批、任务和索引数据；
- 文件系统保存 Workspace 资源、大对象和可读配置；
- 凭据进入操作系统 Keychain / Credential Manager，或经过本机密钥加密；
- schema migration 随 Rust binary 一起发布；
- 持久化事件应能够恢复最终状态，不依赖前端缓存。

Rust 新版本使用全新的 SQLite，不兼容或导入现有 Python 数据库内部表结构。旧数据目录由旧版本继续拥有，新 Core 不读取也不修改。

## 7. 分阶段实施计划

### 阶段 0：Clone、基线与契约冻结

工作内容：

1. Clone `rayrayraykk/CoPaw` 到同级产品仓库 `qwenpaw`；
2. Clone `openai/codex` 到 Core 仓库的 `references/codex`；
3. 记录 remote、branch 和 commit；
4. 阅读两个仓库内适用的 `AGENTS.md` 和贡献规范；
5. 使用 `conda qwenpaw` 环境安装和运行原项目；
6. 运行现有 Python、前端和 E2E 测试；
7. 枚举 WebUI 实际使用的 REST、SSE 和 WebSocket 接口；
8. 捕获成功、失败、取消、审批和断线场景 fixtures；
9. 建立 Python 模块到 Rust crate 的迁移矩阵；
10. 输出详细架构决策记录。

验收标准：

- 原项目在本机可启动；
- 测试基线和已知失败均有记录；
- 每个 WebUI API 都有负责人、payload 和迁移状态；
- 能明确列出第一条 Rust 垂直链路需要替换的 Python 模块；
- 没有开始业务代码迁移。

### 阶段 1：Rust Workspace 与协议骨架

工作内容：

1. 建立 Cargo workspace；
2. 定义领域对象和稳定 ID；
3. 实现 initialize handshake；
4. 实现请求、响应、通知和 Server Request；
5. 实现 stdio 传输；
6. 实现有界队列、背压、超时和取消；
7. 生成 JSON Schema 和 TypeScript 协议类型；
8. 提供协议测试客户端；
9. 建立 tracing 和结构化日志。

验收标准：

- 客户端能够 initialize；
- 能够创建 Thread 和启动模拟 Turn；
- 能收到增量 Item 事件；
- 能够 interrupt Turn；
- Rust 类型、JSON Schema 和 TypeScript 类型一致；
- 协议测试 100% 通过。

### 阶段 2：WebUI 兼容层

工作内容：

1. 使用 Axum 提供 HTTP 服务；
2. 托管原 `console/dist`；
3. 实现健康检查、静态资源和前端路由 fallback；
4. 建立现有 API route inventory；
5. 为未迁移 route 实现受控的 Python proxy；
6. 将聊天流映射到 App Protocol event；
7. 建立 Python / Rust differential contract tests。

验收标准：

- 不修改前端业务代码即可加载现有 WebUI；
- 已迁移 API 与 Python fixtures 一致；
- 未迁移 API 可以通过受控 proxy 工作；
- 前端测试继续通过；
- 错误、取消和断线行为有自动化覆盖。

### 阶段 3：首个纯 Rust Agent 闭环

工作内容：

1. 实现 OpenAI-compatible 模型 Provider；
2. 实现基础 Agent Loop；
3. 实现上下文构建和 token 上限；
4. 实现 Thread / Turn / Item 持久化；
5. 实现流式 AgentMessage；
6. 实现文件读取、文件写入和 Shell 工具；
7. 实现 MCP client；
8. 实现 allow / deny / ask 策略；
9. 实现审批超时、客户端断线和 Turn 中断；
10. 实现 Workspace Root 和路径安全检查。

验收标准：

- 只启动 Rust binary 即可完成一次对话；
- 对话可以调用工具并由 WebUI 审批；
- 重启后能够恢复 Thread；
- Turn 可以可靠中断；
- 工具不能越过允许的 Workspace Root；
- macOS、Linux、Windows CI 全部通过。

### 阶段 4：分模块迁移

建议顺序：

1. 配置、模型 Provider 和凭据；
2. Agent Modes、Loop、Context 和 Hooks；
3. Workspace、文件、Checkpoint 和备份；
4. MCP、Skills、Plugins 和 Driver；
5. Memory / ReMe 兼容；
6. Cron、Heartbeat 和后台任务；
7. 消息渠道；
8. Browser / Computer Use；
9. Marketplace、Hub 和本地模型。

每个模块必须遵循：

1. 冻结当前契约；
2. 添加迁移前测试；
3. 实现 Rust 版本；
4. 运行 differential tests；
5. 切换 route / service；
6. 观察稳定性；
7. 删除该模块的 Python proxy。

### 阶段 5：客户端接入

#### WebUI

- 继续使用兼容 API；
- 不进行页面和视觉改造；
- 只允许必要的构建、启动地址或类型生成调整，并在修改前单独确认。

#### Desktop

- Tauri 启动 Rust sidecar；
- 使用本机 IPC；
- 管理 Core 启动、退出、崩溃恢复和版本匹配；
- 移除 Python 环境下载和启动流程。

#### VS Code

- 使用生成的 TypeScript SDK；
- 默认通过 stdio 启动 Core；
- 支持 Thread 列表、聊天、流式 Item、工具状态和审批；
- VS Code 不直接访问 SQLite 或 Workspace 内部存储。

验收标准：

- 三个客户端使用同一 Core；
- 同一 Thread 能够被支持的客户端恢复；
- 协议版本不匹配时给出明确错误；
- 客户端不复制 Agent Runtime 逻辑。

### 阶段 6：去 Python、加固与发布

工作内容：

1. 删除最后的 Python proxy；
2. 删除运行时 Python 环境依赖；
3. 验证新版本使用独立数据目录且不会读取或修改旧 Python 数据；
4. 完成权限、目录穿越、命令注入和凭据审计；
5. 完成远程连接认证和 TLS 指南；
6. 生成 macOS、Linux、Windows 发布物；
7. 完成安装、升级、降级和卸载测试；
8. 完成性能和资源基线。

验收标准：

- 正常用户路径不需要 Python；
- 新版本明确从空数据启动，旧版本数据保持原样且不会被新版本修改；
- 三个平台可以安装、启动、升级和卸载；
- 发布包包含 SBOM、license notice 和校验信息；
- 安全与回归测试全部通过。

## 8. 测试策略

### 8.1 测试层级

| 层级 | 目标 |
|---|---|
| Unit | 领域状态机、策略、序列化、路径规则 |
| Protocol | 请求、响应、通知、版本与 schema |
| Contract | Rust API 与冻结的 Python / WebUI 契约一致 |
| Integration | Model、Storage、MCP、工具和审批闭环 |
| E2E | WebUI、Desktop、VS Code 的关键用户路径 |
| Cross-platform | Windows、Linux、macOS 行为一致 |

### 8.2 必须覆盖的失败路径

- 模型超时、限流和流中断；
- 客户端在审批期间断线；
- Tool 运行期间取消 Turn；
- 进程输出过大和事件消费者过慢；
- SQLite 锁、迁移失败和磁盘空间不足；
- 非法路径、符号链接逃逸和 Windows 路径边界；
- Core 与客户端协议版本不兼容；
- Core 崩溃后恢复未完成 Turn。

### 8.3 质量门禁

- 所有新增 Rust 代码通过 `cargo fmt --check`；
- 所有新增 Rust 代码通过 `cargo clippy --all-targets --all-features`；
- 单元、协议和契约测试通过率 100%；
- 关键用户路径必须有 integration 或 E2E 测试；
- API 变更必须更新 schema、fixtures 和文档；
- 非机械性变更尽量控制在可独立评审的小批次内。

## 9. 兼容与迁移策略

### 9.1 Strangler 模式

迁移期间 Rust Server 是唯一对客户端暴露的入口：

```text
Client -> Rust Compatibility API -> Rust implementation
                              \--> Python legacy service
```

Python legacy service 只绑定本机随机端口或本机 IPC，不直接暴露给用户。代理表必须显式列出，禁止隐式 fallback。

### 9.2 API 兼容级别

每个旧 API 标记以下状态之一：

- `captured`：已记录契约；
- `proxied`：Rust 入口代理到 Python；
- `shadowed`：Rust 与 Python 同时执行并比较，但只返回 Python 结果；
- `native`：由 Rust 实现；
- `removed`：经确认后移除。

### 9.3 数据边界（Fresh Start）

- Rust 新版本不迁移 Python 配置、会话、记忆、Workspace 状态或凭据；
- Rust Core 使用独立的新数据目录和 SQLite，不扫描旧 Python 数据目录；
- 旧版本及其数据保持原样，可通过启动旧版本继续访问；
- 新旧运行时禁止同时写入同一个逻辑会话或数据库；
- 用户需要在新版本重新配置模型凭据和创建会话；
- 后续如需人工导出能力，必须作为新的独立需求评审，不能隐式恢复自动迁移。

## 10. 安全设计重点

- Core 默认只允许本机连接；
- WebSocket 检查 Origin，远程模式要求 WSS 和 bearer token；
- token 不允许出现在命令行参数和普通日志中；
- Workspace 文件访问使用 canonical path 和明确 root；
- Shell 参数和展示信息分离，不使用字符串拼接模拟 shell；
- 工具权限在 Core 校验，不能信任客户端隐藏按钮；
- 审批决策绑定 thread、turn、item 和 request ID；
- 所有队列有上限，所有大对象有大小限制；
- 日志默认脱敏，不记录消息正文、凭据和完整环境变量；
- Plugin / Skill / MCP 配置在加载前进行来源与权限检查。

## 11. 可观测性与运维

MVP 至少提供：

- `/healthz`：进程是否存活；
- `/readyz`：是否可以接收请求；
- JSON 结构化日志；
- thread、turn、request、tool call correlation ID；
- 队列长度、Turn 耗时、模型耗时、工具耗时；
- token usage；
- 不包含敏感内容的错误分类。

暂不在 MVP 中引入复杂的分布式 tracing 后端，但内部 span 结构应允许后续接入 OpenTelemetry。

## 12. 主要风险与应对

| 风险 | 影响 | 应对 |
|---|---|---|
| QwenPaw 功能面过大 | 重写周期失控 | 先做垂直 MVP，按模块迁移 |
| 前端 API 未文档化 | WebUI 隐性回归 | 从源码和运行流量双向冻结契约 |
| Python 与 Rust 行为细节不同 | 数据或用户体验不一致 | differential contract tests |
| 多客户端同时操作同一 Thread | 状态冲突 | MVP 明确单活动 Turn，操作串行化 |
| Windows 行为滞后 | 后期返工 | 从阶段 1 开始启用 Windows CI |
| Sandbox 跨平台差异 | 安全能力不一致 | 平台适配层和明确 capability |
| 插件生态依赖 Python | 无法快速去 Python | 临时 sidecar，逐类定义兼容边界 |
| Codex 架构被过度照搬 | QwenPaw 需求被扭曲 | 只借鉴模式，所有 API 由实际用例驱动 |
| 上游持续变化 | 迁移目标漂移 | 固定基线 commit，按周期选择性同步 |

## 13. 评审决策点

开始编码前需要确认以下事项。用户已于 2026-09-01 确认启动 Rust Core + VS Code 第一阶段；未明确的实现细节继续通过基线调研收敛：

- [x] D1：确认 Codex 指 `https://github.com/openai/codex`；
- [x] D2：目标保持可拆分的 `qwenpaw-core` 边界；首个版本按用户决定暂存于 CoPaw 的 `qwenpaw-core/`，Codex 仅作为本地忽略的参考仓库；
- [x] D3：确认新 Desktop 正常运行时完全去 Python；旧 Python 版本仅作为独立 legacy 产品保留，新版不提供 sidecar fallback；
- [x] D4：确认现有 WebUI 业务代码不改，必要修改保持最小；
- [x] D5：确认第一阶段目标为 Rust Core + VS Code，不包含全部消息渠道、Browser Use 和完整 Memory；
- [x] D6：MVP 首个模型接口采用 OpenAI-compatible / Qwen；
- [x] D7：MVP 会话存储采用 SQLite，Workspace 文件继续使用文件系统；
- [x] D8：App Protocol 是 QwenPaw 自有协议，不承诺完整兼容 Codex；
- [x] D9：迁移期间 Rust Server 是新客户端唯一 Core 入口；
- [x] D10：确认后续实现按阶段开发、构建和测试，不进行一次性大改。
- [x] D11：确认 Rust 新版本从空数据启动，不实现 Python 数据迁移或自动导入。
- [x] D12：用户于 2026-09-04 将最终目标扩大为现有 QwenPaw 前端全部交互等价；此前“页面可导航、空响应或禁用状态”不再视为完成，实现状态必须由真实行为和原前端 E2E 共同证明。

## 14. 执行 Checklist

### 14.1 方案与基线

- [x] 用户完成第一阶段方向评审；
- [x] 评审决策点全部确认；
- [x] Clone QwenPaw；
- [x] Clone Codex；
- [x] 记录仓库基线；
- [x] 阅读适用的仓库开发规范；
- [x] 建立测试基线（Rust workspace、VS Code 扩展、原 WebUI production build）；
- [x] 建立 API inventory；
- [x] 建立 contract fixtures；
- [x] 完成详细架构设计；
- [x] 完成迁移矩阵。

### 14.2 Rust MVP

- [x] 创建 Cargo workspace；
- [x] 完成 domain 边界评审，当前类型由 protocol/runtime 所有，不创建空 domain crate；
- [x] 创建 protocol crate；
- [x] 创建 server crate；
- [x] 创建 runtime crate；
- [x] 完成 model 边界评审，单一 OpenAI-compatible adapter 保留在 runtime，不创建空 models crate；
- [x] 创建 tools crate；
- [x] 创建 storage crate；
- [x] 完成 governance 边界评审，当前审批策略保留在 Core 状态机；
- [x] 完成 platform 边界评审，平台代码保留在各自所有组件；
- [x] 创建 CLI；
- [x] 生成 JSON Schema；
- [x] 生成 TypeScript 协议类型；
- [x] 实现 stdio；
- [x] 实现 loopback HTTP health / WebSocket App Protocol；
- [x] 实现 Desktop/WebUI 当前范围的 HTTP/SSE 兼容层，未实现产品域显式返回空/禁用或 404；
- [x] 实现模型流式响应；
- [x] 实现基础 Agent Loop；
- [x] 实现基础文件与 Shell 工具；
- [x] 实现 MCP stdio、Streamable HTTP 与 legacy SSE 客户端；
- [x] 实现 stdio 客户端审批闭环；
- [x] 实现持久化；
- [x] 完成首个模型流式端到端闭环。

### 14.2.1 当前开发切片：Coding Agent 工具闭环

- [x] Thread 绑定规范化的 Workspace Root；
- [x] 模型流支持 OpenAI-compatible `tool_calls` 增量；
- [x] 实现有最大步数限制的 Agent Loop；
- [x] 模型请求使用有界上下文并保留完整的最新工具调用链；
- [x] 实现 Workspace 内只读文件工具；
- [x] 实现 Workspace 内文件写入工具且默认必须审批；
- [x] 实现无需审批的 Workspace 文件枚举；
- [x] 实现无需审批的 Workspace 文本搜索；
- [x] 实现需要审批且具有唯一匹配保护的局部文本替换；
- [x] 为浏览、搜索、局部编辑补充路径安全和 Agent Loop 测试；
- [x] 实现 Shell 工具且默认必须审批；
- [x] 实现审批请求、响应、拒绝、取消和超时；
- [x] Turn 中断能够取消已启动的 Shell 子进程；
- [x] Shell 支持有界 `timeoutMs` 并在超时后终止子进程；
- [x] Turn 中断能够取消等待中的模型 HTTP 请求；
- [x] VS Code 使用原生对话框处理审批，不新增 Webview；
- [x] VS Code 使用 SecretStorage 管理模型 API Key，不写入普通 settings；
- [x] VS Code 在持久化 Thread 丢失时自动创建新 Thread 并重试一次；
- [x] 添加模型工具调用、路径越界、审批和真实 stdio 集成测试；
- [x] Rust fmt、test、clippy 全部通过；
- [x] VS Code compile、test、VSIX package 全部通过；
- [x] 确认 `console/` 源码零改动且原 WebUI build 继续通过。

### 14.2.2 当前开发切片：VS Code Core 分发闭环

- [x] 显式 `qwenpaw.core.path` 保持最高优先级；
- [x] 插件按 OS 与 CPU 架构发现内置 Core binary；
- [x] 无内置 binary 时兼容从 `PATH` 启动 Core；
- [x] 产品仓库锁定 Core 与 App Protocol 版本；
- [x] 打包时生成 Core SHA-256 清单；
- [x] 启动内置 Core 前校验目标平台、版本和 SHA-256；
- [x] 为显式路径、PATH fallback、内置 binary 和篡改场景补充单测；
- [ ] 使用 Developer ID 签名并 notarize Core 后验证 macOS arm64 平台 VSIX；
- [x] macOS bundled 打包拒绝 ad-hoc 或未通过 Gatekeeper 的 Core；
- [x] 在 CI 原生 runner 从同一源码 SHA 生成 Linux、macOS arm64/x64 和 Windows 平台 QA VSIX；

#### 跨平台 VSIX 发布门禁

- [x] Core tag workflow 构建四个目标平台的原生归档；
- [x] macOS Core workflow 强制 Developer ID 签名、notarization 与 Gatekeeper 校验；
- [x] 产品仓库通过 `core-release.json` 锁定 Core repository、tag 和 asset；
- [x] VSIX 打包前执行 Core `--version` 并拒绝版本漂移；
- [x] 产品 workflow 在四个原生 runner 生成 target-specific VSIX；
- [x] 手动 QA VSIX 使用独立 artifact 名、3 天保留期与 `packageKind: qa`，不伪装生产产物；
- [ ] 配置 Apple secrets 并实际跑通四个平台 workflow；

### 14.2.3 当前开发切片：MCP stdio 工具闭环

- [x] 提取原 QwenPaw `mcp.clients` 配置子集及 legacy wrapper；
- [x] 使用官方 Rust MCP SDK 实现 stdio initialize 与工具发现；
- [x] 支持 `command`、`args`、`env`、`cwd` 和工具白名单；
- [x] 使用稳定命名空间避免不同 MCP server 工具重名；
- [x] MCP 工具调用复用 App Protocol 的逐次审批；
- [x] Turn 中断取消 MCP 调用并触发有界子进程清理；
- [x] 限制 client、工具、Schema、结果和各阶段超时；
- [x] 补充真实 stdio MCP、Agent Loop、审批与中断测试；
- [x] VS Code 支持配置 MCP JSON 或 legacy `agent.json` 路径；
- [x] 实现 Streamable HTTP 与有界 legacy SSE transport；
- [x] 支持敏感 HTTP headers、Bearer access token 与已有授权的 refresh-token 更新；
- [x] 网络 MCP 继续复用逐次审批、Turn 中断和工具结果上限；
- [x] 补充真实 Streamable HTTP、legacy SSE、OAuth refresh 与 Agent Loop 测试；
- [x] 按 MCP 2026-07-28 实现 RFC 9728 资源元数据、RFC 8414/OIDC 授权服务器发现；
- [x] 实现 PKCE S256、一次性 state、10 分钟超时和 loopback 浏览器回调；
- [x] 在 authorization/token 请求中带上受保护 MCP `resource`，并按 RFC 9207 校验授权响应 `iss`；
- [x] 预注册 client ID 优先，仅将带 `application_type=native` 的 DCR 作为旧服务兼容 fallback；
- [x] access/refresh token 及授权元数据只存入系统 Keychain / Credential Manager / Secret Service，不进 SQLite 或日志；
- [x] refresh 后原子更新访问凭据，revoke 立即断开已缓存 MCP 连接；
- [x] 保持 Console 业务源码不变，实现 `/api/mcp/oauth/start|status|revoke` 契约；
- [x] 通过 App Protocol 暴露同一 OAuth 状态机，VS Code 仅负责用系统浏览器打开授权 URL；
- [x] 使用真实 loopback OAuth/MCP fixture 覆盖成功、state/issuer 不匹配、超时、refresh 和 revoke；

### 14.2.4 当前开发切片：App Protocol 契约闭环

- [x] 单一 Rust 类型源覆盖全部 MVP request、response 和 notification payload；
- [x] 生成版本化 App Protocol JSON Schema（当前 v3）；
- [x] 生成包含全部稳定消息的 typed contract fixtures；
- [x] 生成可审阅的方法与通知 inventory；
- [x] 生成 TypeScript 协议类型并由 VS Code 替换手写 payload 接口；
- [x] Rust 测试校验生成物与类型源无漂移；
- [x] VS Code 使用协议版本和 SHA-256 锁定 SDK 快照；
- [x] `Item` variant 字段统一为 camelCase 并补充序列化测试；

### 14.2.5 当前开发切片：HTTP / WebSocket 传输闭环

- [x] 同一 App Protocol handler 同时服务 stdio 与 WebSocket；
- [x] 提供 `/healthz`、`/readyz` 和 `/app-protocol`；
- [x] 每个 WebSocket 连接独立维护 initialize 生命周期；
- [x] WebSocket 输入限制为 1 MiB text frame；
- [x] HTTP listener 强制 loopback，拒绝公网和 unspecified 地址；
- [x] 默认校验 loopback same-origin，并支持显式开发 Origin allowlist；
- [x] 增加真实 TCP health、WebSocket 握手、协议和连接隔离测试；
- [x] 实现远程 WSS、认证、TLS 和可轮换 token；

### 14.2.6 当前开发切片：VS Code Thread 与模型交互

- [x] 审计 Chat Participant 与 App Protocol 的 MVP 能力差距；
- [x] 使用原生 QuickPick 选择持久化 Thread，不新增 Webview；
- [x] 支持显式创建新 Thread，并让选择仅覆盖下一次请求；
- [x] 使用 Core `model/list` 选择模型，并支持输入兼容端点模型 ID；
- [x] 模型变更写入 VS Code 工作区配置并通过 Core 热更新；
- [x] 补充 Thread 选择优先级单测和真实 Core list/read 集成测试；
- [x] Rust、VS Code、VSIX 和原 WebUI 全量构建门禁通过；

### 14.2.7 当前开发切片：Thread 生命周期

- [x] 实现 `thread/archive` 并拒绝归档活跃 Turn；
- [x] 默认列表隐藏归档 Thread，支持 `includeArchived` 查询；
- [x] 实现 `thread/resume` 并持久化解除归档状态；
- [x] 归档 Thread 禁止 `turn/start`，但保留 `thread/read`；
- [x] 旧 snapshot 缺少 `archived` 字段时兼容为未归档；
- [x] VS Code 使用原生命令归档，并可选择恢复归档 Thread；
- [x] Chat 历史引用已归档 Thread 时自动新建并重试一次；
- [x] Rust、VS Code、VSIX 和原 WebUI 全量构建门禁通过；

### 14.2.8 当前开发切片：配置与 Workspace 资源

- [x] 实现脱敏 `config/read`，仅返回 API key 是否已配置；
- [x] 实现 `config/write` 的 URL/模型校验与运行时热更新；
- [x] 使用 SQLite transaction 持久化非敏感 Core 配置；
- [x] 重启时恢复持久化配置，且永不持久化 API key；
- [x] 实现基于已登记 Thread 的 `workspace/list` 聚合；
- [x] 实现仅允许精确登记根目录的 `workspace/read`；
- [x] VS Code 启动及设置变化时同步模型配置，不重启 Core；
- [x] VS Code 使用原生命令展示脱敏配置和 Workspace；
- [x] 补充持久化、校验、Workspace 与真实 stdio 协议测试；
- [x] Rust、VS Code、VSIX 和原 WebUI 全量构建门禁通过；

### 14.2.9 当前开发切片：模型传输可靠性

- [x] 模型 HTTP client 禁止 redirect 并设置连接/响应头超时；
- [x] 模型 SSE 设置可配置且有上下界的 stream idle timeout；
- [x] 成功响应强制 `text/event-stream` Content-Type；
- [x] 使用自有有界 SSE decoder 替换事后检查的第三方聚合；
- [x] 单个 SSE event 上限 262,144 bytes，错误体上限 65,536 bytes；
- [x] 支持 LF、CRLF、CR、comment、分片与多行 data；
- [x] 缺少 `[DONE]` 的 EOF 按失败处理，不接受部分回答；
- [x] 429、超时、畸形/过大事件和断流均形成可观察失败；
- [x] 失败 Turn 状态和错误持久化并通过 App Protocol 返回；
- [x] Rust、VS Code、VSIX 和原 WebUI 全量构建门禁通过；

### 14.2.10 当前开发切片：VS Code Turn 可观察性与生命周期

- [x] 使用原生 Chat progress 展示工具开始、成功与失败；
- [x] 使用原生 Chat progress 展示工具审批允许与拒绝；
- [x] 不向 Chat progress 泄漏工具输出或完整调用参数；
- [x] 明确区分 Turn 完成、失败、中断与非法终态；
- [x] Chat 取消继续通过 `turn/interrupt` 传递到 Core；
- [x] Core 初始化失败时关闭 RPC 并回收已启动的子进程；
- [x] Core 连接中断时立即结束活跃 Chat Turn，禁止无限等待；
- [x] Manager 能安全释放或重启一个已经启动失败的 Core Promise；
- [x] 补充纯 TypeScript 展示状态测试与真实 Core 中断测试；
- [x] Rust、VS Code、VSIX 和原 WebUI 全量构建门禁通过；

### 14.2.11 当前开发切片：VS Code Core 崩溃恢复

- [x] 同一时刻只允许一个 Core 启动 Promise，避免并发重复进程；
- [x] Core 启动失败后清除缓存，使下一次请求可以重试；
- [x] Core 连接关闭后只失效对应实例，不影响更新一代进程；
- [x] 下一次 Chat 请求按需启动新的 Core，不进行后台崩溃循环；
- [x] Restart 能跳过已失败实例并启动新的 Core；
- [x] Extension dispose 能安全回收 pending 或 active Core；
- [x] 补充并发、失败、崩溃、重启与释放的纯 TypeScript 单测；
- [x] Rust、VS Code、VSIX 和原 WebUI 全量构建门禁通过；

### 14.2.12 当前开发切片：VS Code Thread 分页与多根 Workspace

- [x] `thread/list` 持续读取 cursor，避免只显示前 200 条 Thread；
- [x] 分页设置总量上限并拒绝重复 cursor，禁止无限循环；
- [x] 新 Thread 默认绑定 active editor 所属 Workspace folder；
- [x] 提供原生 `QwenPaw: Select Workspace` 命令，不新增 Webview；
- [x] Workspace 选择只覆盖下一次新 Thread，并覆盖历史 Thread；
- [x] 已移除或失效的 Workspace 选择回退到当前默认根目录；
- [x] 补充分页、选择优先级与真实 Core cursor 集成测试；
- [x] Rust、VS Code、VSIX 和原 WebUI 全量构建门禁通过；

### 14.2.13 当前开发切片：VS Code Chat 文件引用

- [x] App Protocol v2 增加结构化 `fileReference` 输入；
- [x] 引用数量、路径长度和行区间均由 Core 有界校验；
- [x] Core 规范化文件并拒绝 Workspace 外部、目录和不存在路径；
- [x] 模型只收到相对路径/行号提示，引用阶段不读取文件内容；
- [x] VS Code 仅接收 file `Uri` / `Location`，忽略未知引用类型；
- [x] VS Code 对引用去重、限量并转换为 1-based inclusive 行区间；
- [x] 协议版本、Schema、fixtures、TypeScript SDK 与发布锁同步到 v2；
- [x] 补充协议、路径安全、输入映射和真实 stdio Core 测试；
- [x] Rust、VS Code、VSIX 和原 WebUI 全量构建门禁通过；

### 14.2.14 当前开发切片：架构与迁移基线收口

- [x] 编写 Rust Core、VS Code、现有 Desktop/WebUI 的系统边界总览；
- [x] 建立原 QwenPaw Web API inventory，并标注第一阶段迁移归属；
- [x] 建立 Python 到 Rust 的能力迁移矩阵与退出条件；
- [x] 记录当前 crate 拆分，避免为概念分层创建空 crate；
- [x] 回填 14.1 基线 Checklist 并校验文档内本地链接；

### 14.2.15 首版暂存到 CoPaw 单仓

- [x] Core 以独立 `qwenpaw-core/` Rust workspace 纳入 CoPaw；
- [x] Core CI 与 release workflow 改为单仓工作目录和路径触发；
- [x] Core release 使用 `qwenpaw-core-v*`，避免与产品 tag 冲突；
- [x] VS Code 协议同步、真实 Core 测试路径和 release 锁切换到单仓；
- [x] 从 CoPaw 根目录重新验证 Rust、VS Code、VSIX 与 workflow；

### 14.2.16 Desktop/WebUI Rust sidecar 基础链路

- [x] Core Desktop HTTP 模式绑定随机 loopback 端口并输出兼容 ready marker；
- [x] Core 托管现有 Console 静态目录与 SPA fallback，不修改 React 业务源码；
- [x] 提供 `/api/version`、`/api/healthz` 和 token 保护的 Desktop shutdown；
- [x] Tauri 完成 Rust Core 本地/打包 sidecar 启动链路；该阶段的临时 Python fallback 已在 Rust-only 收口中删除；
- [x] 增加真实 HTTP、静态资源、鉴权 shutdown、进程退出和 Tauri 路径测试；
- [x] 更新 Desktop 打包资源边界并通过 Rust Core、Tauri、Console 本地质量门禁；

### 14.2.17 新版本数据边界

- [x] 取消 Python 数据迁移，Rust 新版本从空数据启动；
- [x] Desktop/WebUI 切换时为 Rust Core 使用独立的 `rust-core-v1` 数据目录；
- [x] 增加测试证明 Rust 启动不会扫描或修改旧 Python 数据；
- [x] 在升级说明中明确旧会话不会出现在新版本，模型凭据需要重新配置；

### 14.2.18 当前开发切片：WebUI 首条 Rust 对话链路

- [x] 在 App Server 传输边缘实现 Console 兼容 adapter，不向 Core 领域层引入旧 payload；
- [x] 保持 `console/` React 业务源码零改动；
- [x] 实现本地 auth、language、upload-limit、agent、model 和 coding-mode 启动读取契约；
- [x] 将 Chat 列表、历史、归档和恢复映射到 Rust Thread，并兼容前端本地 session ID；
- [x] 将文本 Chat 请求和 Core 增量映射为现有 Console SSE，并支持按本地 session ID 停止；
- [x] 将 Core 一次性工具审批映射为 Console 轮询与 approve/deny 接口；
- [x] 对 `similar` 泛化审批返回明确不支持，避免伪造安全策略兼容；
- [x] 补充真实模型 SSE、历史持久化、中断和拒绝 shell 的 HTTP 集成测试；
- [x] 支持有界 Console 附件上传/预览，并复制到当前 Workspace 作为 Core 文件引用；
- [x] 实现单一 OpenAI-compatible Desktop 模型配置写入和系统安全凭据存储；
- [x] 实现持久化默认 project-directory、目录浏览/创建与单 Workspace Thread 绑定；
- [x] 通过 Core workspace 全量测试、严格 Clippy、Tauri 测试/release check 与 Console 生产构建；
- [x] 支持单 Workspace 文件树、元数据、UTF-8 分块读取、ETag 保存、流式下载和冲突感知上传；
- [x] 使用跨平台原生文件事件实现 Workspace recursive watch SSE，并补充真实变更集成测试；
- [x] 将 Coding Mode 开关持久化到 Rust SQLite，并实现现有 Console 的 11 个 Workspace Git 读写契约；
- [x] 使用真实临时 Git 仓库覆盖 init、status、diff、stage/unstage、commit、branch、discard、revert 与注入拒绝；
- [x] 初始化 Git 时不自动 stage/commit 用户内容，并验证嵌套 Workspace 不会误操作父仓库；
- [x] 将全局 UI language GET/PUT 持久化到 Rust SQLite，验证七种现有 Console 语言、非法输入和 Desktop 重启恢复；
- [x] 使用全新 Chrome profile 验证持久化语言驱动现有 Console 本地化，且启动 API 与浏览器错误均为零；
- [x] 补齐已观察 Chat 启动调用图，并用真实 headless Chrome 验证页面渲染、0 个 API 404 和 0 个浏览器错误；
- [ ] 支持多 project-directory、memory/profile 与剩余 Coding 文件契约；
- [x] 完成非 Chat 导航页的调用图、空/禁用状态响应和 24 页真实浏览器 E2E 契约；
- [x] 默认切换 Desktop 到 Rust Core；下一切片已删除 `QWENPAW_DESKTOP_RUST_CORE=0` 与 legacy backend 回退；

### 14.2.19 当前开发切片：Rust-only Desktop 收口

- [x] Desktop 开发和发布模式只启动 Rust Core，不再读取 Python backend 切换环境变量；
- [x] Tauri 安装包不再包含 PyInstaller backend、Python runtime 或仅供 Python backend 使用的 Node runtime；
- [x] Computer Use 原生 helper 使用独立资源目录，不依赖 legacy backend 目录；
- [x] Windows 安装器的 CLI PATH 和进程清理只指向 Desktop、Rust Core 与原生 helper；
- [x] macOS/Windows 打包脚本只构建 Console、Rust Core、Tauri 和原生 helper；
- [x] 更新 Fresh Start、系统架构和迁移矩阵，明确新 Desktop 不提供 Python fallback；
- [x] Rust Core Desktop 默认在版本化新数据目录发布端口，并支持原生安装验证传入隔离的端口文件路径；
- [x] 通过 Rust Core、Tauri、Console 本地门禁及 macOS/Windows 原生 runner 打包验证（[Desktop Build #33536039382](https://github.com/rayrayraykk/CoPaw/actions/runs/33536039382)）；

### 14.2.20 当前开发切片：OAuth/WSS 安全审计

- [x] 将 RustSec 依赖漏洞扫描固化进 Core CI；
- [x] 远程 WSS 在 Unix 上拒绝组或其他用户可读的 TLS 私钥；
- [x] 修正 App Protocol 中已经过时的交互式 OAuth 能力说明；
- [x] 通过 OAuth/WSS 回归测试、Rust workspace 门禁和 workflow lint；

### 14.2.21 当前开发切片：macOS 生产发布 fail-closed

- [x] 可复用 Desktop workflow 区分手动 QA 与 production signing；
- [x] 统一正式发布在构建前校验 Apple 与 Tauri updater secrets；
- [x] production macOS 构建由 Tauri 完成 Developer ID 签名与公证，不再在公证后重签；
- [x] production macOS 构建强制通过 codesign、stapler 与 Gatekeeper 验收；
- [x] 补充脚本单测并通过 workflow、Shell、Tauri 单测与本地 macOS QA 打包门禁；
- [x] macOS/Windows 原生 runner 完成 Rust-only 安装、启动、WebView 与真实聊天 QA（[Desktop Build #33545103126](https://github.com/rayrayraykk/CoPaw/actions/runs/33545103126)）；
- [ ] 配置 Apple/Tauri 生产凭据并在 GitHub 原生 runner 完成一次真实签名与公证；

### 14.2.22 后续目标完成度审计

- [x] Desktop/WebUI：新 Desktop 只启动 Rust Core，现有 React 业务源码不变，原生包安装/启动/页面矩阵通过；
- [x] 数据边界：按 Fresh Start 决策不迁移、不扫描、不修改 Python 数据，使用版本化新库；
- [x] 交互式 OAuth：MCP discovery、PKCE loopback callback、系统浏览器、安全凭据存储、refresh/revoke 及 VS Code/Console 契约通过；
- [x] 远程 WSS：TLS、bearer token file、Origin allowlist、token rotation、私钥权限与原生平台回归通过；
- [x] 跨平台 QA：Core CI 通过 Linux/macOS/Windows，Desktop 通过 macOS/Windows 原生 runner；
- [x] 原生发布 QA：四平台 Core 归档通过 [Core Release #33548216424](https://github.com/rayrayraykk/CoPaw/actions/runs/33548216424)，四平台 VSIX 通过 [VSIX #33549602153](https://github.com/rayrayraykk/CoPaw/actions/runs/33549602153)；
- [x] 生产发布脚本在缺失 Apple/Tauri 凭据时 fail-closed，不会发布 ad-hoc macOS 产物；
- [ ] 外部阻塞：仓库配置 Apple/Tauri 凭据后，完成 Core、平台 VSIX 与 Desktop 的真实 Developer ID 签名/公证发布验收；

### 14.2.23 当前开发切片：Codex 式 App Server SDK 分层

- [x] 用户确认 App Server 作为统一客户端宿主，SDK 不承载 Agent Runtime；
- [x] 更新目标架构图，明确 Core、App Server、SDK、产品壳和 Web 兼容层；
- [x] 新增 Rust `qwenpaw-app-server-client`，提供有类型的请求、通知和 stdio 生命周期；
- [x] 将 `sdk/typescript` 从协议类型快照扩展为可独立构建和测试的客户端 SDK；
- [x] VS Code 复用 TypeScript SDK 的 RPC/initialize 层，不再维护独立 wire client；
- [x] 新增 Python SDK，通过 stdio 启动或连接 `qwenpaw-core app-server`；
- [x] Rust、TypeScript、Python SDK 共用 App Protocol v3 fixtures 做一致性校验；
- [x] 建立现有 CLI、TUI、远程访问和消息渠道入口的非退化清单与回归门禁；
- [x] 更新 SDK 使用说明、兼容策略和本机验证命令；
- [x] 通过 Rust workspace、SDK、VS Code、Tauri 与 Console 本机回归；

### 14.2.24 当前开发切片：原前端全交互等价

完成标准不是页面能打开，也不是接口不返回 404。每个原前端可触发的读取、写入、流式响应、取消、错误和重启恢复行为都必须由 Rust Core 提供，并以原前端实际操作验证。占位空数组、固定对象和无效果成功响应均按未实现处理。

- [x] 从 `console/src` 生产调用点生成 HTTP、SSE、WebSocket 和下载/上传调用清单，并建立防漂移检查；
- [ ] 为每个调用标记 Rust 实现、真实行为测试、原前端 E2E 和跨平台状态，且完成门禁要求不存在占位或未知项；
- [ ] Chat、会话、分组、附件、审批、工具调用和 Inbox 全交互等价；
- [ ] Workspace、Memory、Profile、系统提示词、Coding 文件、Git 和 Checkpoint 全交互等价；
- [ ] 模型、Provider、OAuth、本地模型、Agent、多 Agent、统计和 Token Usage 全交互等价；
- [ ] MCP、Skills、Tools、Plugins、PawApps、Market 和 Harnesses 全交互等价；
- [ ] Channels、Access Control、Mail Access Control、Messages、Voice 和 Browser Control 全交互等价；
- [ ] Cron、Heartbeat、Env、Security、Backup、Debug、ACP 和 Hub 全交互等价；
- [x] Env 的原前端 CRUD、安全凭据持久化、重启恢复和 Rust Agent Shell 继承已实现；其他运行时适配器继承仍由上一项跟踪；
- [ ] Cron 已完成原前端任务 CRUD、启停、Console 文本立即执行、可选 Inbox 结果、状态/历史、投递目标和重启恢复；后台调度、Agent 任务、其他 Channel 和 Heartbeat 仍由上一项跟踪；
- [ ] Access Control 已完成原前端白名单/黑名单读取、新增、删除、元数据修改、审批动作契约和重启恢复；非 Console Channel 接入、入站消息拦截及运行时生成 pending 记录仍由 Channels 项跟踪；
- [ ] Mail Access Control 已完成原前端 13 条路由、地址与域通配符校验、批量白黑名单、pending 备注/批准/拒绝/忽略、隐藏批准重放状态、Inbox 已读联动和重启恢复；邮件监听、入站 pending 生成及批准邮件实际重放仍由 Mail Runtime 跟踪；
- [ ] Channels 已恢复与旧版一致的 18 个内置通道目录、默认配置、单 Agent 冲突检查和 Console 配置保存/重启恢复；17 个外部通道 runtime、凭据安全存储、健康检查、重启及 QR 登录仍待逐个移植，当前启用请求明确失败而不会伪装成功；
- [ ] 原 Console 业务源码保持不变，全部原交互用例在 Rust Core 上通过；
- [ ] Desktop、WebUI、VS Code、Rust/TypeScript/Python SDK、CLI 和 TUI 逐个构建及回归；
- [ ] macOS、Windows 和 Linux 原生制品完成安装态回归；

#### 14.2.24.1 2026-09-04 本机验收记录

- [x] 生成并锁定 370 个 Console 生产调用点，CI 校验调用清单与 Rust 路由漂移；
- [x] Rust workspace 120 个测试、严格 Clippy 和格式检查通过；
- [x] Console 295 个测试文件、2453 个测试及 production build 通过；
- [x] 使用原 Console 的 Environment 页面完成新增、保存、刷新回显和删除 WebKit E2E；
- [x] 累计使用未修改的 Console 与对应 release Rust Core 完成 Environment、Cron、Access Control、Mail Access Control、Channels、Inbox Messages 与 Chat Catalog 七组 WebKit E2E；
- [x] 使用原 Console 的 Cron 页面完成创建、启停、立即执行、历史、编辑和删除 WebKit E2E，并验证 Core 重启后任务仍可读取；
- [x] 使用原 Console 的 Access Control 抽屉完成白名单新增、刷新回显、删除和黑名单新增/删除 WebKit E2E，并验证 Core 重启持久化契约；
- [x] 使用原 Inbox 的 Mail Access Control 抽屉完成 pending 备注、批准、拉黑、忽略，以及白名单/黑名单新增、刷新回显和删除 WebKit E2E，并验证 Core 重启持久化契约；
- [x] 使用未修改的原 Inbox 页面完成来源筛选、分页、单条/全部已读、trace 查看、单条/批量删除、空态和刷新恢复 WebKit E2E；
- [x] 使用未修改的原 Chat 抽屉和 Sessions 页面完成分组新建/重命名/置顶/删除回迁，以及会话重命名/置顶/移动、单条归档、批量归档/恢复和批量物理删除 WebKit E2E；
- [x] 使用原 Console 的 Channels 页面验证 18 个内置通道目录，并完成 Console Bot Prefix 保存、刷新回显和清空 WebKit E2E；
- [x] TypeScript SDK、Python SDK、VS Code extension 均通过真实 Rust Core 测试；
- [x] legacy CLI/TUI 专项 893 个测试通过；初次运行因 `qwenpaw` conda 环境漏装已声明的 `pytest-asyncio` 产生 73 个收集/执行失败，补齐开发依赖后全量重跑通过；
- [x] macOS App/ZIP/DMG、Core archive、WebUI archive、两个 SDK 包、两个 VSIX 和 legacy wheel 完成本机构建及包结构校验；最新 Chat Catalog Core 已重新嵌入 App、ZIP、DMG 和 darwin-arm64 VSIX，原 Chat 抽屉与 Sessions 管理页已在 release Core 上通过 WebKit E2E 和重启恢复验证，并更新全部 SHA-256；
- [x] legacy wheel 安装后 CLI 与现有 TUI 入口可用；
- [ ] macOS DMG 安装态启动：镜像校验、挂载、包结构、深度签名及内嵌 arm64 Core 均通过；但 ad-hoc QA 包从分发目录/DMG 启动时被当前阿里企业安全 EDR 以 exit 137 终止，Gatekeeper 也按预期拒绝无 Developer ID 的包，等待签名与公证后复测；
- [ ] Windows/Linux 原生安装态构建与交互回归：必须在对应原生 runner 完成，不能用 macOS 结果替代；
- [ ] 语义等价门禁：当前仍有 215 个调用点未注册、23 个明显占位实现和 11 个静态未解析表达式；

#### 14.2.24.2 当前子切片：Inbox Messages

`/api/messages/send` 是外部客户端的主动通道投递接口，原 Console 不调用它；原前端的 Messages 交互实际由 `/api/console/inbox/*` 提供。本子切片先完成用户可见的 Inbox，再在 Channels runtime 切片实现主动外部投递。

- [x] 用 SQLite 持久化有界 Inbox event 与 trace 数据，重启后恢复；
- [x] 完成事件分页、来源/状态/Agent/未读筛选，并返回精确 total 与 unread_count；
- [x] 完成单条/全部已读、删除、共享 trace 引用和最后引用删除语义；
- [x] 将 Cron 保存到 Inbox 的行为接入同一事件存储；
- [x] 补齐 HTTP 契约、非法输入、容量限制和重启恢复测试；
- [x] 使用未修改的原 Inbox 页面验证查看、单条已读、全部已读、删除、筛选和 trace 交互；
- [x] 更新 API inventory，确认原 Console 业务源码零改动并通过完整回归；

Inbox 读取、状态和持久化契约已完成；真实 Agent Cron、Heartbeat、Memory 与 Mail Monitor 产生 trace/event 的运行时仍分别由对应未完成切片跟踪，不能用测试种子替代。

#### 14.2.24.3 当前子切片：Chat Catalog 与分组管理

本子切片只迁移原 Console 已使用的会话目录管理行为。消息与 turn 继续以 Core Thread 为唯一事实来源；名称、置顶、来源、分组和展示时间等目录元数据使用独立的有界 SQLite setting 持久化，避免把旧 Python `ChatSpec` 数据结构侵入 App Protocol。

- [x] 为已有及新建 Core Thread 提供持久化 ChatSpec 元数据，并保持列表筛选、排序、状态和重启恢复；
- [x] 完成会话创建、重命名、置顶、移动分组、单删和批删，删除后同步清理 Core Thread 与本地 session alias；
- [x] 完成自定义分组创建、重命名、置顶、顺序调整和删除回迁，严格保护 Cron/Subagents 固定分组；
- [x] 完成单条及批量归档/恢复、运行中冲突、部分失败和幂等语义；
- [x] 为元数据容量、非法输入、未知对象、并发锁顺序和重启恢复补齐 Rust HTTP 测试；
- [x] 使用未修改的原 Chat 抽屉与 Sessions 页面完成新建分组、重命名、置顶、移动分组、归档、恢复和删除 WebKit E2E；
- [x] 更新 API inventory，确认 `console/src` 零改动并通过全量回归；

### 14.3 客户端与发布

- [x] WebUI 启动与导航契约测试通过；
- [x] Desktop 默认切换到 Rust sidecar；
- [x] VS Code Chat Participant MVP 完成；
- [x] macOS 本地测试通过；
- [x] Linux 测试通过；
- [x] Windows 测试通过；
- [x] 确认不迁移旧数据，新版本从空数据启动；
- [x] 新 Desktop 删除 Python proxy，保留的 Python 源码仅属于可独立运行的 legacy 产品；
- [x] 新 Desktop 安装包删除运行时 Python 依赖；
- [x] 完成当前范围的 OAuth/WSS、路径、凭据、审批与发布门禁安全审计；
- [x] 完成 macOS/Windows 未签名 QA 发布验收；
- [ ] 完成 Apple Developer ID 签名/公证的生产发布验收。

## 15. 阶段 0 预期交付物

本计划通过评审后，下一阶段只交付调研和设计，不立即开始大规模业务重写：

1. `docs/architecture/system-overview.md`；
2. `docs/architecture/app-protocol.md`；
3. `docs/api-contract/web-api-inventory.md`；
4. `docs/migration/python-to-rust-matrix.md`；
5. 原系统测试基线报告；
6. 第一条 Rust 垂直链路的详细任务拆分；
7. 更新后的执行 Checklist。

完成上述交付物并再次评审后，再进入 Rust 代码实现。
