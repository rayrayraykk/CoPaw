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
  - [x] PawApps 目录接口与原 App Center 列表/卸载源码验收：见 [实现与验收 checklist](../architecture/pawapps-runtime.md)。注册表/插件执行、动态前端加载及新制品验收仍未完成，目录 API 不替代这些功能。
- [ ] Channels、Access Control、Mail Access Control、Messages、Voice 和 Browser Control 全交互等价；
- [ ] Cron、Heartbeat、Env、Security、Backup、Debug、ACP 和 Hub 全交互等价；
  - [x] Debug 实际文件日志、宿主 tracing 与原页面筛选/刷新：源代码及原 Chrome 页面通过，[设计/checklist](../architecture/backend-debug-logs.md) 和 [制品验收](../testing/qa-debug-packages-20260910.md) 已更新；包内启动、OS 剪贴板、原生和跨平台验收仍未完成。
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
- [x] Rust workspace 121 个测试、严格 Clippy 和格式检查通过；
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
- [x] macOS App/ZIP/DMG、Core archive、WebUI archive、两个 SDK 包、两个 VSIX 和 legacy wheel 完成本机构建及反向安装/解包校验；最新 Project Directory Core 已重新嵌入 App、ZIP、DMG 和 darwin-arm64 VSIX，并更新全部 SHA-256；
- [x] legacy wheel 安装后 CLI 与现有 TUI 入口可用；
- [ ] macOS DMG 安装态启动：镜像校验、挂载、包结构、深度签名及内嵌 arm64 Core 均通过；但 ad-hoc QA 包从分发目录/DMG 启动时被当前阿里企业安全 EDR 以 exit 137 终止，Gatekeeper 也按预期拒绝无 Developer ID 的包，等待签名与公证后复测；
- [ ] Windows/Linux 原生安装态构建与交互回归：必须在对应原生 runner 完成，不能用 macOS 结果替代；
- [ ] 语义等价门禁：当前仍有 211 个调用点未注册、23 个明显占位实现和 11 个静态未解析表达式；

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

#### 14.2.24.4 当前子切片：Project Directory 管理

后续多 Agent 审计发现四种写入和列表仍固定 default 根；以下是早期单 Workspace
基线，不代表跨 Agent 已完成。按 [项目归属 checklist](../architecture/project-directory-ownership.md)
修复请求 Agent 的基础目录、排队/异步身份及原页面选择流程；实际成功更新应只影响
该 Agent，不是下文旧单 Workspace 基线所称的全局活动项目。

2026-09-10 多 Agent 修复专项 14/14（包含原页面 Chromium 与两 Agent 的原 Python
20 步响应对照）通过，另覆盖 PUT/reset/worktree 标记和服务重新打开，详见
[当前验收记录](../testing/project-directory-ownership-acceptance.md)。最终普通
686/686、显式 workspace 26/26、原前端 2453/2453、release/SDK/VS Code 检查
通过；首次 Chrome 退出超时保留，不追认其根因。九类新制品
`qa-runtime-20260910-vGGX8Z` 已构建并完成静态/隔离安装检查，见
[制品记录](../testing/qa-project-packages-20260910.md)。下方早期打包勾选仍是
历史证据，WebKit、包内执行和跨平台限制不能由这次分层检查关闭。

本子切片保持原 `ProjectSelectModal` 与 API client 零改动，补齐新建、服务端本地导入、浏览器 ZIP 导入和 Git Clone 四条真实写入链路。所有成功链路都必须持久化切换全局活动项目，Core 重启后继续生效；失败链路不得留下活动项目指向或可误认为成功的响应。

- [x] 新建项目目录并执行真实 `git init`，兼容原请求/响应和目录重名行为；
- [x] 从用户 Home 下复制本地目录，排除生成物、符号链接/Windows junction 与敏感凭据文件；
- [x] 接收原前端 multipart ZIP，限制上传量、成员数和解压总量，拒绝绝对路径、目录穿越、符号链接及覆盖逃逸；
- [x] 以 SSE 输出真实 `git clone --progress` 日志、完成或错误事件，禁用交互凭据并限制输出与执行时间；
- [x] 四种成功链路均切换并持久化活动项目，列表、读取和 Core 重启恢复一致；
- [x] 补齐 HTTP 正常、非法名称、敏感导入、ZIP bomb/zip-slip/symlink、Clone 失败与状态不污染测试；
- [ ] 使用未修改的原 Project Directory 弹窗逐项完成创建、ZIP 导入和 Clone WebKit E2E，并记录精确请求与响应；
- [x] 更新 API inventory，确认 `console/src` 零改动，通过 Rust、Console 与各客户端回归后重建全部发布产物；

原弹窗所在的 Configuration 页面还依赖 `/workspace/running-config` 完整默认结构；本切片同步修复了该既有不足，否则原页面会在读取 `reme_light_memory_config.needs_reindex` 时崩溃。未修改 Console 的 Chromium E2E 已在 production Console 与 release Core 上完成创建、浏览器目录 ZIP 导入、Clone、Recent Projects、活动项目回显与 Core 重启恢复，精确请求记录保存在本机隔离 QA 目录；浏览器错误为 0。Playwright WebKit 1.61 与 1.63 均在当前 macOS 企业安全环境的 XPC 页面创建阶段挂起、尚未向 Core 发出请求，WebKit 项保持未完成并等待在原生 runner 复测，不能用 Chromium 结果冒充通过。当前 inventory 为 370 个调用点、168 条 Rust 路由、159 个已注册调用点，其中 136 个非占位、23 个占位、211 个未注册和 11 个静态未解析表达式。

#### 14.2.24.5 当前子切片：Agent 运行配置与 Voice 设置

本子切片保持原 Configuration 与 Voice Transcription 页面不变，把现有只读固定值替换成持久化、可校验的 Rust Core 设置。配置中的 API Key 不进入 SQLite 或响应日志，统一使用 Desktop 平台凭据存储；不具备实际运行能力的转写后端不得通过伪造成功状态冒充可用。

- [x] 建立有界、版本化的 Agent 运行设置存储，Core 重启后恢复且损坏数据 fail-closed；
- [x] 完成 `/workspace/running-config` GET/PUT，返回完整默认结构、校验原表单约束并安全处理嵌入与 ADBPG Key；
- [x] 完成 Agent language GET/PUT，校验原支持语言、保持原响应字段，并将原版 8 个语言模板逐文件覆盖到当前工作区；
- [x] 完成 audio mode、transcription provider type 与 provider selection 的 GET/PUT，并保持三者重启一致；
- [ ] 将可直接生效的运行参数接入 Rust agent loop/shell 执行；尚未具备的 memory/voice runtime 明确报告限制，不伪装热应用；
- [x] 补齐 HTTP 正常、非法输入、秘密不落盘、并发更新和 Core 重启恢复测试；
- [x] 使用未修改的原 Configuration 与 Voice Transcription 页面完成读取、保存、刷新回显与错误态 E2E；
- [x] 更新 inventory，确认 `console/src` 零改动并通过全量回归与全部客户端/产物重建。

当前已把 iteration/max steps、Shell timeout、Shell executable 与 STRICT/SMART/AUTO/OFF 审批模式热应用到 Rust runtime；OpenAI-compatible Whisper 转写走真实 multipart 请求，本地 Whisper 缺少依赖时明确返回 unavailable。Memory manager、LLM 限流/重试、context compact 等表单字段目前只做有界持久化，尚未全部接入对应 Rust runtime，因此运行参数总项继续保持未完成，不能把“保存成功”记作“全部热应用”。

2026-09-05 本机验收：Rust workspace 126 个测试与严格 Clippy 通过；Console 295 个测试文件、2453 个测试、production build 和 inventory 防漂移通过。未修改的原页面完成语言、IANA 时区、Shell timeout/executable、STRICT 审批、Whisper API、Auto/Native Audio 保存及刷新回显，9 次页面写请求均为 200 且浏览器错误为 0；Core 重启后状态恢复，印尼语 8 个模板与原资源逐字一致。当前 inventory 为 370 个调用点、175 条 Rust 路由、166 个已注册调用点，其中 148 个非占位、18 个占位、204 个未注册和 11 个静态未解析表达式。macOS App/ZIP/QA DMG、Core archive、WebUI archive、TypeScript/Python SDK、universal/platform VSIX 与 legacy wheel 均使用本切片源码重建并完成反向安装或解包校验，`dist/SHA256SUMS` 全部通过。

#### 14.2.24.6 当前子切片：Profile Markdown 与系统提示词

本子切片保持原 Files Workspace 的 Profile 交互不变，移除 `/workspace/files` 与 `/workspace/system-prompt-files` 的空数组占位。Fresh Start 必须安装当前语言的默认模板；保存、启用状态和排序不仅要刷新回显，还必须在下一轮 Rust Agent Turn 的 system message 中生效。

- [x] Fresh Start 按当前语言补齐缺失的 8 个 Agent Markdown 模板，不覆盖用户已有内容；
- [x] 完成 Profile Markdown 列表、读取与原子保存，保持原响应字段、排序和 `.md` 名称行为；
- [x] 完成 system prompt files GET/PUT，限制数量、名称、重复项和请求体大小并持久化重启恢复；
- [x] 按配置顺序组合实际 Markdown，处理 frontmatter 与禁用的 heartbeat/memory 段，并在新 Thread 与已有 Thread 下一轮 Turn 生效；
- [x] 补齐正常、非法路径、符号链接、超限、并发、运行时生效与重启测试；
- [x] 使用未修改的原 Files/Profile 页面完成打开、编辑、启停、排序、刷新和重启 E2E；
- [x] 更新 inventory，确认 `console/src` 零改动并完成全量回归与受影响产物重建。

2026-09-05 本机验收：Rust workspace 129 个测试与严格 Clippy 通过；Console 295 个测试文件、2453 个测试、production build 和 inventory 防漂移通过，`console/src` 保持零改动。未修改的原 Files/Profile 页面完成 Fresh Start 8 个模板、打开编辑保存、启停、从 Workspace 添加、拖拽排序、刷新与 Core 重启恢复；5 次写请求均为 200，浏览器错误为 0。当前 inventory 为 370 个调用点、178 条 Rust 路由、169 个已注册调用点，其中 153 个非占位、16 个占位、201 个未注册和 11 个静态未解析表达式。macOS App/ZIP/QA DMG、Core archive、WebUI archive、TypeScript/Python SDK、universal/platform VSIX 与 legacy wheel 均使用本切片源码重建并完成反向安装或解包校验，`dist/SHA256SUMS` 9 个产物全部通过。

#### 14.2.24.7 当前子切片：Workspace 剩余文件契约

本子切片清零原 `console/src/api/modules/workspace.ts` 中尚未注册的调用，但不以文件 CRUD 冒充尚未实现的 Memory engine。Memory 图谱、自动采集、搜索索引与 reindex 仍由后续真正的 Memory runtime 切片跟踪。

- [x] 完成 Workspace ZIP 下载，以及有大小、成员数、解压总量和路径安全限制的合并上传；
- [x] 完成 daily/digest Memory Markdown 的递归列表、读取、原子保存与 legacy 无 section 路由；
- [x] 完成 Coding Mode 递归文件列表、5 MiB 文本读取、ETag/304 与原子保存；
- [x] 拒绝绝对路径、目录穿越、符号链接、特殊文件、ZIP bomb/zip-slip 和并发不安全写入；
- [x] 补齐精确响应、错误、覆盖合并、缓存和重启后文件状态的 Rust HTTP 测试；
- [x] 使用未修改的原 Files/Coding 页面逐项完成 Memory 打开编辑与代码文件打开编辑 E2E；
- [x] 更新 inventory，确认 `console/src` 零改动并通过全量回归；
- [x] 重建并逐个反向验证 Desktop、Core、WebUI、SDK、VSIX 与 legacy 产物。

2026-09-05 本机验收：Rust workspace 130 个测试、格式检查与严格 Clippy 通过；Console 295 个测试文件、2453 个测试、production build 和 inventory 防漂移通过，`console/src` 保持零改动。未修改的原 Files/Coding 页面完成 daily、digest 与 Workspace 三类文件的展开、打开、Monaco 编辑、保存和 Core 重启恢复；3 次写请求均为 200，浏览器错误为 0。当前 inventory 为 370 个调用点、186 条 Rust 路由、180 个已注册调用点，其中 164 个非占位、16 个占位、190 个未注册和 11 个静态未解析表达式；`workspace.ts` 的 25 个调用点已全部注册且均非占位。macOS App/ZIP/QA DMG、Core archive、WebUI archive、TypeScript/Python SDK、universal/platform VSIX 与 legacy wheel 均使用本切片源码重建；9 个产物逐个完成执行、安装、解包或挂载反向验证，`dist/SHA256SUMS` 全部通过。

#### 14.2.24.8 当前子切片：Checkpoint 完整交互

本子切片替换 Checkpoints 页面现有 status/graph 假数据，并补齐该页面全部 11 个调用。实现必须保存真实 Core Thread 与选定 Workspace 内容，恢复必须先预览并创建 safety checkpoint；开启自动检查点后，成功完成的 Console Turn 必须实际产生快照。

- [x] 建立按 Workspace 隔离、持久化且有版本/数量/大小限制的 Checkpoint 元数据与快照存储；
- [x] 完成真实 status、graph、manual snapshot 与成功 Turn 后 auto snapshot；
- [x] 完成 conversation、Memory、选择性 Workspace 文件的 restore preview/apply，并在变更前创建 pre-restore safety checkpoint；
- [x] 完成 GC preview/apply、GC settings 与 reset，保持原请求和响应结构；
- [x] 拒绝跨 Workspace、错误会话、非法 commit/path、符号链接、超限与并发恢复，并保证失败不留下部分恢复状态；
- [x] 补齐 Core Thread 导出/恢复和 HTTP 正常、错误、自动快照、重启恢复测试；
- [x] 使用未修改的原 Checkpoints 页面完成开关、快照、预览恢复、GC 设置、GC 与重置 E2E；
- [x] 更新 inventory，确认 `console/src` 零改动，通过全量回归并重建、逐个验证全部发布产物。

2026-09-05 本机验收：Rust workspace 132 个测试、格式检查与严格 Clippy 通过；Console 295 个测试文件、2453 个测试、production build 和 inventory 防漂移通过，`console/src` 保持零改动。未修改的原 Checkpoints 页面在 release Core 上完成自动开关、手工快照、conversation/Memory/指定文件恢复预览与执行、刷新恢复、GC 设置/清理和重置，24 次 Checkpoint 请求全部为 200，浏览器错误为 0；HTTP 契约另覆盖成功 Turn 自动快照、Core 重启、并发快照、目录穿越、符号链接与归档篡改。当前 inventory 为 370 个调用点、195 条 Rust 路由、189 个已注册调用点，其中 175 个非占位、14 个占位、181 个未注册和 11 个静态未解析表达式；Checkpoint 的 11 个调用点已全部注册且均非占位。macOS App/ZIP/QA DMG、Core archive、WebUI archive、TypeScript/Python SDK、universal/platform VSIX 与 legacy wheel 均使用本切片源码重建；9 个产物逐个完成执行、真实 Core 连接、安装、解包、签名检查或挂载反向验证，`dist/SHA256SUMS` 全部通过。QA DMG 当前为 ad-hoc 签名，待用户提供 Apple 发布凭据后才能生成 Developer ID 签名和 notarized 正式包。

#### 14.2.24.9 当前子切片：Agent Statistics 与 Token Usage

本子切片保持原 Agent Statistics 和 Token Usage 页面不变，替换现有固定零值与空数组占位。统计事实必须来自 Rust Core 已持久化的真实 Thread、Turn、模型 usage 和工具调用；模型未返回 usage 时仍记录真实 LLM 调用次数，但不得估算或伪造 Token 数。

- [x] 在 Core Thread 快照中持久化有版本兼容默认值的 Turn 时间、LLM 调用和 Token/cache usage 元数据；
- [x] 请求 OpenAI-compatible 流式 usage，兼容缺省 usage 和 cache 明细，并把每个模型步骤准确归入对应 Turn；
- [x] 完成 `/api/token-usage` 与 `/api/token-usage/details` 的日期、模型和 Provider 筛选及原响应结构；
- [x] 完成 `/api/agent-stats` 与 `/api/agent-stats/llm-tool-trend` 的会话、消息、工具、LLM 与 Token 日聚合；
- [x] 补齐新旧快照兼容、真实 SSE usage、无 usage、筛选、日期交换、重启恢复和精确 HTTP 响应测试；
- [x] 使用未修改的原 Agent Statistics 与 Token Usage 页面验证日期切换、非空图表/表格、刷新和 Core 重启恢复；
- [x] 更新 inventory，确认 `console/src` 零改动并通过 Rust、Console 和客户端全量回归；
- [x] 重建 Desktop、Core、WebUI、SDK、VSIX 与 legacy 全部 9 个发布产物，并逐个反向验证。

2026-09-05 本机验收：Rust workspace 136 个测试、格式检查与严格 Clippy 通过；Console 295 个测试文件、2453 个测试、production build 和 inventory 防漂移通过，`console/src` 保持零改动。未修改的原 Agent Statistics 与 Token Usage 页面通过真实 OpenAI-compatible SSE usage 验证日期范围切换、非空指标/图表/表格、空区间和 Core 重启恢复，全部请求为 200 且浏览器错误为 0；HTTP 契约另覆盖模型/Provider 筛选、反向日期交换、缺省 usage、旧快照兼容、Thread 删除后的全局 Token 账本保留和 Checkpoint 不回写历史。当前 inventory 为 370 个调用点、196 条 Rust 路由、190 个已注册调用点，其中 179 个非占位、11 个占位、180 个未注册和 11 个静态未解析表达式；统计与 Token Usage 的 4 个调用点已全部注册且均非占位。macOS App/ZIP/QA DMG、Core archive、WebUI archive、TypeScript/Python SDK、universal/platform VSIX 与 legacy wheel 均使用本切片源码重建；9 个产物逐个完成执行、真实 Core 连接、安装、解包、签名检查或只读挂载反向验证，`dist/SHA256SUMS` 全部通过。QA DMG 当前为 ad-hoc 签名，待用户提供 Apple 发布凭据后才能生成 Developer ID 签名和 notarized 正式包。

#### 14.2.24.10 当前子切片：内置 Tools 管理

本子切片保持原 Tools 页面不变，只展示 Rust Core 当前真实注册的内置工具。启停必须改变下一次模型请求中的工具定义并在执行边界再次校验，不能只保存 UI 状态；Rust 尚未实现的异步执行和工具专属配置不得返回无效果成功。

- [x] 为 Rust 内置工具提供单一来源的名称、描述和定义元数据；
- [x] 在 Core SQLite 中持久化启停覆盖值，兼容新增工具默认启用并在重启后恢复；
- [x] 过滤发送给模型的内置工具定义，并拒绝模型调用已禁用工具；
- [x] 完成 `/api/tools`、toggle、async-execution、config 读取与写入的原响应/错误契约；
- [x] 覆盖并发批量启停、未知工具、禁用执行、真实模型请求过滤和重启恢复测试；
- [x] 使用未修改的原 Tools 页面验证单项/批量启停、刷新与 Core 重启恢复；
- [x] 更新 inventory，确认 `console/src` 零改动并通过 Rust、Console 和客户端全量回归；
- [x] 重建 Desktop、Core、WebUI、SDK、VSIX 与 legacy 全部 9 个发布产物，并逐个反向验证。

2026-09-05 本机验收：Rust workspace 140 个测试、格式检查与严格 Clippy 通过；Console 295 个测试文件、2453 个测试、production build 和 inventory 防漂移通过，`console/src` 保持零改动。未修改的原 Tools 页面连接真实 Rust Desktop HTTP 服务完成单项启停、刷新恢复、批量全部禁用/启用和 Core 重启恢复，共 19 个 Tools 请求全部返回 200，浏览器错误为 0；Core 契约另覆盖精确六工具目录、并发不同工具启停、未知工具、异步/配置能力的显式拒绝、SQLite 重启恢复、真实模型请求定义过滤和恶意禁用工具调用的执行边界拒绝。当前 inventory 为 370 个调用点、200 条 Rust 路由、194 个已注册调用点，其中 184 个非占位、10 个占位、176 个未注册和 11 个静态未解析表达式；Tools 的 5 个调用点已全部注册且均非占位。macOS App/ZIP/QA DMG、Core archive、WebUI archive、TypeScript/Python SDK、universal/platform VSIX 与 legacy wheel 均使用本切片源码重建；9 个产物逐个完成执行、真实 Core 连接、安装、解包、清单校验、签名检查或只读挂载反向验证，`dist/SHA256SUMS` 全部通过。QA DMG 当前为 ad-hoc 签名，待用户提供 Apple 发布凭据后才能生成 Developer ID 签名和 notarized 正式包。

#### 14.2.24.11 当前子切片：工具调用生命周期与后台执行

本子切片保持原 Chat 工具卡、后台任务面板和 Tool Offload 设置页不变。Rust 必须以模型给出的真实 `tool_call_id` 和实际执行过程为准维护状态；取消、延时和 Offload 不能只是 UI 标记。前台调用移入后台后 Agent loop 可继续，但原执行仍受动态硬截止时间约束，完成输出继续通过原结果与 SSE 契约提供。

- [x] 固化原 11 个 Console 调用点及 Python 成功、404、409、校验和 SSE 终止契约；
- [x] 在 Core 建立有界的进程内调用协调器，按 Thread 隔离真实状态、动态截止时间、最终输出和 60 秒完成缓存；
- [x] 为 Shell 等受限执行接入可动态延长的硬截止时间，单工具取消不得误中断整个 Turn；
- [x] 实现用户/超时 Offload：前台立即返回提示、实际执行后台继续，并在完成/取消后发布最终输出；
- [x] 将 `keep_foreground` / `offload` 策略持久化到 Core SQLite，并让新调用立即采用当前策略；
- [x] 完成 list、info、output、stream、offload、cancel、extend-deadline 与 offload-policy 的全部原 HTTP/SSE 契约；
- [x] 覆盖跨 Session 越权、并发状态转换、完成缓存淘汰、Core 重启策略恢复及真实 Shell 进程终止测试；
- [x] 使用未修改的原前端验证 Offload 设置、长时 Shell 倒计时、延时、手动后台、后台输出、取消和刷新恢复；
- [x] 更新 inventory，确认 `console/src` 零改动并通过 Rust、Console 和客户端全量回归；
- [x] 重建 Desktop、Core、WebUI、SDK、VSIX 与 legacy 全部 9 个发布产物，并逐个反向验证。

2026-09-05 本机验收：Rust workspace 147 个测试、格式检查与严格 Clippy 通过；Console 295 个测试文件、2453 个测试、production build 和 inventory 防漂移通过，`console/src` 保持零改动。未修改的原 Tool Offload 设置页、Chat 工具卡与后台任务面板连接 release Rust Core，完成策略保存/刷新/Core 重启恢复、真实长时 Shell 倒计时、延时、手动移入后台、后台 SSE 最终输出、单工具取消、Agent loop 继续响应和工具消息历史刷新恢复；19 个工具/策略请求均成功，浏览器错误为 0。浏览器验收同时校正了历史 ToolCall/ToolResult 的 `plugin_call` / `plugin_call_output` 精确消息结构，以及原前端空 JSON 取消请求的兼容行为。当前 inventory 为 370 个调用点、208 条 Rust 路由、204 个已注册调用点，其中 195 个非占位、9 个占位、166 个未注册和 11 个静态未解析表达式；Tool Calls 的 11 个调用点已全部注册且均非占位。TypeScript SDK 3 个、Python SDK 4 个真实 Core 测试与 VS Code 57 个测试全部通过。macOS App/ZIP/QA DMG、Core archive、WebUI archive、TypeScript/Python SDK、universal/platform VSIX 与 legacy wheel 均使用本切片源码重建；9 个产物逐个完成执行、安装、解包、清单校验、签名检查或只读挂载反向验证，`dist/SHA256SUMS` 全部通过。QA DMG 当前为 ad-hoc 签名，待用户提供 Apple 发布凭据后才能生成 Developer ID 签名和 notarized 正式包。

#### 14.2.24.12 当前子切片：Heartbeat 配置与真实调度

本子切片保持原 Heartbeat 与 Inbox 页面不变，移除 `/config/heartbeat` 的固定假数据并补齐保存与立即执行。Heartbeat 必须读取当前 Workspace 的 `HEARTBEAT.md` 并启动真实 Rust Agent Turn；配置保存成功必须热重排后台任务，Core 重启后继续生效。当前只对已实现的 Console 通道承诺 `last` 投递，尚未移植的外部 Channel runtime 仍由 Channels 总项跟踪。

- [x] 建立有界、版本化的 Heartbeat 配置存储，精确兼容原字段并在 Core 重启后恢复；
- [x] 实现配置校验、活动时段判断、执行超时、单实例并发抑制和可取消的热重排调度器；
- [x] 每次执行读取当前 Workspace 的 `HEARTBEAT.md`，启动真实 Agent Turn，并处理缺失/空文件与失败状态；
- [x] 实现 `main`、`inbox` 与当前 Console 范围内的 `last` 目标，Inbox 结果携带可读取的真实执行 trace；
- [x] 完成 GET/PUT `/config/heartbeat` 与 POST `/config/heartbeat/run` 的原响应及错误契约；
- [x] 覆盖非法配置、跨午夜活动时段、手工/定时执行、超时、并发、热重排和 Core 重启恢复测试；
- [x] 使用未修改的原 Heartbeat 与 Inbox 页面验证读取、保存、刷新、自动执行、执行详情和重启恢复；
- [x] 更新 inventory，确认 `console/src` 零改动并通过 Rust、Console 和客户端全量回归；
- [x] 重建 Desktop、Core、WebUI、SDK、VSIX 与 legacy 全部 9 个发布产物，并逐个反向验证。

2026-09-05 本机验收：Rust workspace 153 个测试、格式检查与严格 Clippy 通过；Console 295 个测试文件、2453 个测试、production build 和 inventory 防漂移通过，`console/src` 保持零改动。未修改的原 Heartbeat 与 Inbox 页面连接 release Rust Core，完成配置保存/刷新、自动调度、真实 Agent Turn、Inbox trace 展示和 Core 重启恢复；18 个 Heartbeat 请求均成功，浏览器错误为 0。HTTP 契约另覆盖 `main`、`last`、`inbox`、缺失/空 `HEARTBEAT.md`、跨午夜活动时段、热重排、并发抑制和执行超时。当前 inventory 为 370 个调用点、210 条 Rust 路由、206 个已注册调用点，其中 198 个非占位、8 个占位、164 个未注册和 11 个静态未解析表达式；Heartbeat 的 3 个调用点已全部注册且均非占位。TypeScript SDK 3 个、Python SDK 4 个真实 Core 测试与 VS Code 57 个测试全部通过。macOS App/ZIP/QA DMG、Core archive、WebUI archive、TypeScript/Python SDK、universal/platform VSIX 与 legacy wheel 均使用本切片源码重建；9 个产物逐个完成执行、安装、解包、清单校验、签名检查或只读挂载反向验证，`dist/SHA256SUMS` 全部通过。QA DMG 当前为 ad-hoc 签名，待用户提供 Apple 发布凭据后才能生成 Developer ID 签名和 notarized 正式包。

#### 14.2.24.13 当前子切片：MCP 客户端与访问策略管理

本子切片保持原 MCP 页面不变，补齐客户端详情、新建、编辑、启停、删除、工具发现、工具白名单、访问主体与访问策略接口。Desktop 管理的 `headers` / `env` 值只能进入系统凭据库，SQLite 仅保存名称和非敏感配置；API 继续返回原页面识别的 `********`，JSON 编辑原样提交遮罩值时必须保留已有密钥。更新采用每个 Turn 的 MCP 快照：新 Turn 立即使用新配置，进行中的 Turn 不被中途换表破坏。

- [x] 固化原 10 个未注册调用点、Python 成功/错误响应、默认 `ask` 和遮罩字段更新契约；
- [x] 为 Rust MCP Manager 提供有界配置快照、完整工具发现和 `allow` / `ask` / `deny` 策略求值；
- [x] 在 Core 增加 MCP 快照热替换，使新 Turn 使用最新配置且在途 Turn 保持一致；
- [x] 使用版本化 SQLite 元数据和系统凭据库存储 Desktop MCP 配置，完成失败回滚与重启恢复；
- [x] 实现详情、CRUD、toggle、tools、whitelist、access-principals 和 policy 的原 HTTP 契约；
- [x] 覆盖重复键/名称、保留遮罩密钥、路径/大小限制、断线重连、策略执行和 Core 重启测试；
- [x] 使用未修改的原 MCP 页面验证新建、编辑、启停、工具白名单、访问策略、删除和重启恢复；
- [x] 更新 inventory，确认 `console/src` 零改动并通过 Rust、Console 和客户端全量回归；
- [x] 重建 Desktop、Core、WebUI、SDK、VSIX 与 legacy 全部 9 个发布产物，并逐个反向验证。

2026-09-05 本机验收：Rust workspace 159 个测试、格式检查与严格 Clippy 通过；Console 295 个测试文件、2453 个测试、production build 和 inventory 防漂移通过，`console/src` 保持零改动。未修改的原 MCP 页面连接 release Rust Core，完成客户端新建、遮罩密钥编辑、真实 `echo` 工具发现、默认访问策略切换为 `deny`、启停、删除和 Core 重启恢复，相关请求全部返回 200 且浏览器错误为 0；HTTP 与 Core 契约另覆盖密钥不落 SQLite、工具白名单、策略优先级、审批边界、断线重连和在途 Turn 的确定性配置快照。当前 inventory 为 370 个调用点、220 条 Rust 路由、216 个已注册调用点，其中 208 个非占位、8 个占位、154 个未注册和 11 个静态未解析表达式；MCP 管理的 10 个调用点已全部注册且均非占位。TypeScript SDK 3 个、Python SDK 4 个真实 Core 测试与 VS Code 57 个测试全部通过。macOS App/ZIP/QA DMG、Core archive、WebUI archive、TypeScript/Python SDK、universal/platform VSIX 与 legacy wheel 均使用本切片源码重建；9 个产物逐个完成执行、真实 Core 静态服务、安装、解包、清单与哈希校验、签名检查或只读挂载反向验证，`dist/SHA256SUMS` 全部通过。QA DMG 当前为 ad-hoc 签名，待用户提供 Apple 发布凭据后才能生成 Developer ID 签名和 notarized 正式包。

#### 14.2.24.14 当前子切片：Security 配置与真实工具执行边界

本子切片保持原 Security 页面不变，替换 Tool Guard、Sandbox、File Guard、Skill Scanner 与免认证主机的固定响应。Tool Guard 配置必须在每个 Turn 开始时形成快照并参与真实工具决策；`denied_tools` 和命中 `auto_denied_rules` 的调用直接拒绝，其余规则命中按 Agent 审批等级处理。Sandbox 只有在当前平台后端真实可用且执行链已采用隔离时才能返回 `effective=true`，不能把 Workspace 路径校验冒充系统级 Sandbox。Skill Scanner 的模式、超时、白名单和阻断历史先形成可供后续 Skills 安装链路调用的持久化 Core 服务，本切片不伪造扫描记录。

- [x] 固化原 Security 页面 18 个调用点、Python 成功/错误响应、默认值和保存顺序；
- [x] 在 Rust Core 建立有界、版本化、可热更新的 Security 配置与每 Turn 快照；
- [x] 嵌入原 21 条 Tool Guard 内置规则，实现工具别名、参数规则、禁用规则和自动拒绝求值；
- [x] 将 Tool Guard 决策接入真实内置工具与 MCP 执行边界，并覆盖 `STRICT` / `SMART` / `AUTO` / `OFF`；
- [x] 实现 Sandbox 能力探测与诚实状态，确保未隔离时永不报告 `effective=true`；
- [x] 实现 File Guard、Skill Scanner 配置/白名单/阻断历史及 allow-no-auth hosts 的原 HTTP 契约与持久化；
- [x] 覆盖非法输入、规则匹配、拒绝/审批、Turn 快照、并发更新、历史索引和 Core 重启测试；
- [x] 使用未修改的原 Security 页面验证各 Tab 的读取、编辑、保存、刷新和重启恢复；
- [x] 更新 inventory，确认 `console/src` 零改动并通过 Rust、Console 和客户端全量回归；
- [x] 重建 Desktop、Core、WebUI、SDK、VSIX 与 legacy 全部 9 个发布产物，并逐个反向验证。

2026-09-05 本机验收：Rust workspace 168 个测试、格式检查与严格 Clippy 通过；macOS 真实 `sandbox-exec` 测试确认 Workspace 内写入成功且 Workspace 外写入被拒绝。Console 295 个测试文件、2453 个测试、production build 和 inventory 防漂移通过，`console/src` 保持零改动。未修改的原 Security 页面连接 release Rust Core，完成 21 条内置规则渲染、Sandbox 开启并真实生效、Hidden Newlines、File Guard 路径、Skill Scanner Block 模式和 allow-no-auth host 的保存、刷新及 Core 重启恢复，相关请求全部返回 200 且浏览器错误为 0。当前 inventory 为 370 个调用点、234 条 Rust 路由、232 个已注册调用点，其中 227 个非占位、5 个占位、138 个未注册和 11 个静态未解析表达式；Security 的 18 个调用点已全部注册且均非占位。TypeScript SDK 3 个、Python SDK 4 个真实 Core 测试与 VS Code 57 个测试全部通过。macOS App/ZIP/QA DMG、Core archive、WebUI archive、TypeScript/Python SDK、universal/platform VSIX 与 legacy wheel 均使用本切片源码重建；9 个产物逐个完成执行、真实 Core 静态服务、临时安装、解包、清单与哈希校验、签名检查或只读挂载反向验证，`dist/SHA256SUMS` 全部通过。QA DMG 当前为 ad-hoc 签名，待用户提供 Apple 发布凭据后才能生成 Developer ID 签名和 notarized 正式包。

#### 14.2.24.15 当前子切片：Skills 工作区、Skill Pool 与安装链路

本子切片保持原 Skills 与 Skill Pool 页面以及 `console/src/api/modules/skill.ts` 不变。Skill 内容与 manifest 使用 Rust 新版本独立数据目录；创建、编辑、重命名、启停、传输和安装必须操作真实文件并原子更新 manifest。所有进入工作区或 Pool 的内容都经过上一切片完成的 Skill Scanner；ZIP/Hub 输入必须有下载、大小、条目数、解压总量、路径和符号链接边界。Hub 或 AI Provider 不可用时返回原页面可处理的真实错误，不使用固定成功结果。

- [x] 固化原 Skills API、Python 路由、manifest/目录格式和原页面调用顺序；
- [x] 实现工作区 Skill 的 reconcile、列表、详情、创建、保存/重命名、启停、批量操作、删除、channels、tags 与 config 契约；
- [x] 实现 Skill Pool 的 reconcile、列表、详情、创建、保存/重命名、删除、tags、automation 与工作区双向传输；
- [x] 实现有界且防 zip-slip 的工作区/Pool ZIP 导入，并保持冲突预览、重命名和 409 结构；
- [x] 嵌入并列出当前双语 builtin Skills，实现选择性导入、更新通知、单项更新和版本/语言状态；
- [ ] 实现 Skills Hub 搜索、Pool 同步导入、工作区异步安装任务、状态轮询和取消；
- [x] 将 create、save、enable、ZIP、Hub、Pool 上传/下载和 builtin 导入全部接入真实 Skill Scanner，并记录结构化 422 阻断；
- [x] 实现 AI optimize SSE，复用当前 Rust 模型配置并保持增量、done 和 error 事件；
- [ ] 覆盖正常、冲突、非法名称/路径、ZIP bomb、扫描阻断、并发 mutation、任务取消和 Core 重启测试；
- [ ] 使用未修改的原 Skills 与 Skill Pool 页面逐项验证 CRUD、启停、编辑、标签、传输、Builtin、ZIP 与 Hub 交互；
- [x] 更新 inventory，确认 `console/src` 零改动并通过 Rust、Console 与各客户端全量回归；
- [x] 重建 Desktop、Core、WebUI、SDK、VSIX 与 legacy 全部 9 个发布产物，并逐个反向验证。

2026-09-05 本机阶段验收：Rust workspace 172 个测试、格式检查与严格 Clippy 通过；真实 HTTP 契约覆盖工作区 CRUD、保留附属文件的编辑保存、channels、tags、启停、Skill Scanner 结构化阻断与持久历史、工作区到 Pool 传输、冲突响应、16 个双语 builtin 的列出和导入。Console 295 个测试文件、2453 个测试、production build 和 inventory 防漂移通过，`console/src` 保持零改动。未修改的原 Skills 页面连接 release Rust Core 完成创建、打开、编辑、保存、禁用和上传 Pool；原 Skill Pool 页面完成条目渲染、打开编辑器、详情读取和 16 个 builtin 读取，两页请求均无 4xx/5xx 且浏览器错误为 0；另外 28 个既有 Console 导航页全量 smoke 均通过。当前 inventory 为 370 个调用点、278 条 Rust 路由、266 个已注册调用点，其中 261 个非占位、5 个占位、104 个未注册和 11 个静态未解析表达式；`skill.ts` 的 35 个显式调用点均已有 Rust 路由，动态 ZIP helper 由两条真实 multipart 路由承接。TypeScript SDK 3 个、Python SDK 4 个真实 Core 测试与 VS Code 57 个测试全部通过。macOS App/ZIP/QA DMG、Core archive、WebUI archive、TypeScript/Python SDK、universal/platform VSIX 与 legacy wheel 均使用本切片源码重建；9 个产物逐个完成执行、解包、sidecar 边界、签名和 DMG 校验，`dist/SHA256SUMS` 全部通过。Hub 多来源页面 URL 的精确解析、任务取消/并发/重启边界以及 ZIP/Hub/Builtin 的完整浏览器点击矩阵仍保留为下一轮检查项；QA DMG 当前为 ad-hoc 签名，待用户提供 Apple 发布凭据后才能生成 Developer ID 签名和 notarized 正式包。

#### 14.2.24.16 当前子切片：多 Agent 配置、工作区隔离与运行路由

本子切片保持原 Agents、Agent Selector、Chat、Skills 与 Files 页面不变。`X-Agent-Id` 必须决定请求实际访问的 Agent 与 Workspace，不能继续只改变前端选中态或把所有请求落到 default Workspace。新 Rust 版本建立自己的 Agent catalog，不读取或迁移 Python 旧配置；default Agent 始终存在且不可删除、禁用或取消置顶。Agent 创建、复制和删除要使用有界文件操作与原子 catalog/`agent.json` 写入，删除默认只注销配置，不递归删除用户自定义 Workspace。

- [x] 固化原 `agent.ts` / `agents.ts` 23 个未注册调用点、Python 成功/错误结构、`X-Agent-Id` 传播及原页面操作顺序；
- [x] 建立版本化、原子持久化的 Rust Agent catalog，并从新版本 default Workspace 独立启动；
- [x] 实现 Agent 详情、创建、复制、更新、model/backend settings、删除、启停、置顶和排序契约；
- [x] 初始化新 Agent 的模板、sessions、memory、skills、jobs、chats 与 `agent.json`，按复制选项只复制白名单文件；
- [x] 将 Chat/会话、Skills/Skill Pool 传输、Files/Workspace 与相关设置按 `X-Agent-Id` 路由到真实 Agent Workspace；
- [x] 对 default 的不可删除/禁用/取消置顶、ID/路径逃逸、重复 Workspace、并发 mutation 与失败回滚做强制边界；
- [x] 实现 `/agent/`、health/admin status/shutdown 的原本地语义；ReMe 未移植写操作明确返回 unavailable，embedding test 连接真实兼容服务并校验向量，不伪造成功；
- [x] 覆盖 CRUD、copy 选项、排序/置顶约束、跨 Agent 文件与 Skill 隔离、真实 Chat Turn/Thread Workspace、Core 重启和并发测试；
- [x] 使用未修改的原 Agents 与 Agent Selector 页面验证创建、编辑、复制、置顶、启停、切换和删除，并验证 Files Workspace 隔离；
- [ ] 使用原页面补做拖拽排序及配置测试模型后的 Chat/Skills 完整点击链；当前无用户模型 key，Rust HTTP 契约已用本地 mock 模型覆盖真实 SSE Turn 与 Skill 隔离，不能冒充外部模型验收；
- [x] 更新 inventory，确认 `console/src` 零改动并通过 Rust、Console 与各客户端全量回归；
- [x] 重建 Desktop、Core、WebUI、SDK、VSIX 与 legacy 全部 9 个发布产物，并逐个反向验证。

2026-09-05 本机阶段验收：Rust workspace 174 个测试、格式检查与严格 Clippy 通过；多 Agent 契约覆盖版本化原子 catalog、凭据脱敏、默认/自定义 Workspace、Profile 与 Skill 隔离、真实本地模型 SSE Turn、Thread 所属 Workspace、跨 Agent 404、模型/embedding 配置、复制白名单、置顶/排序、启停、删除和 Core 重启。Console 295 个测试文件、2453 个测试、production build 与 inventory 防漂移通过，`console/src` 保持零改动；未修改的原 Agents 页面和侧栏选择器完成创建、编辑、复制、置顶、禁用/启用、切换和删除，所有请求无 4xx/5xx 且浏览器错误为 0，另外 24 个内置导航页全量 smoke 均通过。当前 inventory 为 370 个调用点、297 条 Rust 路由、285 个已注册调用点，其中 280 个非占位、5 个占位、85 个未注册和 11 个静态未解析表达式。TypeScript SDK 3 个、Python SDK 4 个真实 Core 测试与 VS Code 57 个测试全部通过。macOS App/ZIP/QA DMG、Core archive、WebUI archive、TypeScript/Python SDK、universal/platform VSIX 与 legacy wheel 均用本切片源码重建；9 个产物逐个完成执行、临时安装、解包、只读挂载、manifest/sidecar/签名与哈希校验，`dist/SHA256SUMS` 全部通过。QA DMG 当前为 ad-hoc 签名，待用户提供 Apple 发布凭据后才能生成 Developer ID 签名和 notarized 正式包。

#### 14.2.24.17 当前子切片：ACP 配置、Node Runtime 与控制命令识别

本子切片保持原 ACP 页面、Chat 输入行为以及 `console/src/api/modules/acp.ts`、`commands.ts` 不变。Agent ACP 配置写入该 Agent 的新 Rust catalog/`agent.json`，继续由 `X-Agent-Id` 隔离；全局 Node runtime 单独使用版本化原子文件。Node runtime 状态必须真实检查 `node` 与 `npx` 可执行文件及版本，非法自定义路径不得被保存。控制命令只识别原内置命令的完整首 token，不能把 `/stopx` 等前缀误判为控制命令。

- [x] 固化原 ACP/Node runtime 数据结构、默认 Agent、校验规则和原页面调用顺序；
- [x] 删除固定空对象占位，实现 Agent 级 ACP 完整读取、整体更新和单 Agent 更新；
- [x] 实现全局 Node runtime 自动发现、自定义路径校验、版本探测和原子持久化；
- [x] 实现 `/commands/check`，覆盖空白、大小写、参数、未知命令和相似前缀；
- [x] 覆盖多 Agent 隔离、默认值合并、错误输入、并发更新、Core 重启和 Node 探测测试；
- [x] 使用未修改的原 ACP 页面逐项验证读取、编辑、保存、刷新；以原 `commandsApi` 契约测试和真实 Rust HTTP 测试验证命令判定；
- [x] 更新 inventory，确认相关调用均为非占位且 `console/src` 零改动；
- [x] 通过 Rust、Console 与各客户端回归，重建并逐个反向验证全部发布产物；
- [x] 提交并推送本子切片。

2026-09-05 本机阶段验收：Rust workspace 178 个测试、格式检查和严格 Clippy 通过；ACP 契约覆盖四个内置 Agent、整体/单项写入、Agent 隔离、默认值合并、并发更新、非法模式、非法 Node 路径不落盘、真实 `node`/`npx` 版本探测、Core 重启恢复和控制命令完整首 token 判定。Console 295 个测试文件、2453 个测试、production build 和 inventory 防漂移通过，`console/src` 保持零改动；未修改的原 ACP 页面完成内置项展示、自定义项创建/编辑/删除、跨 Agent 隔离和 Node runtime 自动探测/保存，所有请求无 4xx/5xx 且浏览器错误为 0，另外 24 个内置导航页全量 smoke 均通过。当前 inventory 为 370 个调用点、303 条 Rust 路由、291 个已注册调用点，其中 287 个非占位、4 个占位、79 个未注册和 11 个静态未解析表达式。TypeScript SDK 3 个、Python SDK 4 个真实 Core 测试与 VS Code 57 个测试全部通过。macOS App/ZIP/QA DMG、Core archive、WebUI archive、TypeScript/Python SDK、universal/platform VSIX 与 legacy wheel 共 9 个产物均以本切片源码重建，并分别完成签名、只读挂载、解包、运行、临时安装、manifest、sidecar 和 SHA-256 反向校验。QA DMG 当前为 ad-hoc 签名，待用户提供 Apple 发布凭据后才能生成 Developer ID 签名和 notarized 正式包。

#### 14.2.24.18 当前子切片：Provider、Model 配置与本地模型设置

本子切片保持原 Models 页面、Chat Model Selector 与全部 `console/src` 业务代码不变。把当前仅返回一个固定 Provider 的兼容实现替换为新 Rust 版本化、原子持久化的 Provider registry；API key 继续只进入系统凭据存储，registry 仅记录是否已配置。先完整闭环不依赖外部 key 的 Provider/Model CRUD、本地模型配置，以及原“添加模型”流程硬依赖的单模型实时连接测试；Provider 级连接测试、模型发现、OAuth、multimodal probe 和本地推理进程作为紧邻子切片继续完成，在这些调用全部验收前不把 Models 页面标为全交互等价。

- [x] 固化原 ProviderInfo/ModelInfo、创建/配置/删除、模型增删/可见性/参数配置、本地模型设置的请求、响应和原页面操作顺序；
- [x] 建立版本化、原子持久化的 Rust Provider registry，并保持新版本空数据启动和内置 Provider 默认值；
- [x] 将默认及自定义 Provider API key 放入系统凭据存储，读取接口只返回掩码，不把明文写入 JSON、SQLite、日志或响应；
- [x] 实现自定义 Provider 创建/配置/删除及重复 ID、内置 ID、协议、URL、header 和内容边界校验；
- [x] 实现内置/自定义 Provider 的模型新增、删除、隐藏/恢复和逐模型 generation/thinking 配置；
- [x] 实现本地模型 max context、固定/自动端口和 provider generation kwargs 的读取、更新与重启恢复；
- [x] 修正 active model 的 Provider/Model 存在性校验与 global/agent scope 响应，不再把任意模型写成固定 Provider；
- [x] 实现原“添加模型”操作前置的单模型实时连接测试，并用本机 OpenAI-compatible/Anthropic mock 验证成功与结构化失败，不伪造在线结果；
- [x] 覆盖凭据脱敏、失败回滚、并发 mutation、多 Agent scope、非法输入和 Core 重启测试；
- [x] 使用未修改的原 Models 页面验证 Provider 创建、真实模型探测、模型新增/配置/删除和 Provider 删除；
- [ ] 使用未修改的 Chat Model Selector 与本地模型弹窗验证隐藏/恢复和本地设置（依赖紧邻切片的模型发现与本地推理进程接口）；
- [x] 更新 inventory，确认本子切片调用均为非占位且 `console/src` 零改动；
- [x] 通过 Rust、Console 与各客户端回归，重建并逐个反向验证全部 9 个发布产物；
- [x] 提交并推送本子切片。

2026-09-05 本机阶段验收：Rust workspace 182 个测试、格式检查和严格 Clippy 通过；Console 295 个测试文件、2453 个测试、production build 和 inventory 防漂移通过，`console/src` 保持零改动。未修改的原 Models 页面连接 release Rust Core，完成自定义 Provider 创建、使用本地 OpenAI-compatible mock 的真实模型连接测试、模型新增、generation 参数保存、模型删除和 Provider 删除，所有页面请求无 4xx/5xx 且浏览器错误为 0；另外 24 个内置导航页全量 smoke 均通过。当前 inventory 为 370 个调用点、311 条 Rust 路由、299 个已注册调用点，其中 295 个非占位、4 个占位、71 个未注册和 11 个静态未解析表达式。TypeScript SDK 3 个、Python SDK 4 个真实 Core 测试、VS Code 57 个测试与 legacy CLI/TUI 891 个测试全部通过。macOS App/ZIP/QA DMG、Core archive、WebUI archive、TypeScript/Python SDK、universal/platform VSIX 与 legacy wheel 共 9 个发布文件均以本切片源码重建，并分别完成启动、真实 Core 静态服务、临时安装、解包、只读挂载、manifest、sidecar、签名和 SHA-256 反向校验，`dist/SHA256SUMS` 全部通过。QA DMG 当前为 ad-hoc 签名，待用户提供 Apple 发布凭据后才能生成 Developer ID 签名和 notarized 正式包。

#### 14.2.24.19 当前子切片：远程模型发现、连接与能力探测

本子切片继续保持原 Models 页面、Chat Model Selector 和 `console/src` 全部业务代码不变。自定义 OpenAI Chat/Responses 与 Anthropic Provider 按协议访问真实模型目录；Provider 级连接测试必须发送独立的轻量请求，不能借固定成功或已有模型状态冒充。多模态探测使用最小图片和视频语义探针，并将结果原子写回对应模型。没有用户外部 key 时，全部成功路径用本机严格 mock 验证请求方法、路径、鉴权、请求体和响应解析。

- [x] 固化 Provider 连接、模型发现、多模态探测、保存/不保存和 Chat Selector 隐藏/恢复的原请求、响应及页面顺序；
- [x] 为自定义协议启用匹配的 discovery/connection capability，并拒绝不支持或配置不完整的请求；
- [x] 实现 OpenAI-compatible、Responses 与 Anthropic Provider 级真实连接测试和结构化失败/凭据脱敏；
- [x] 实现模型目录发现、分页/去重/数量与响应体边界、`save=true/false`、同步时间和失败状态；
- [x] 实现图片/视频语义探测，区分明确不支持与网络失败，并持久化 capability/probe source；
- [x] 将原 Python 35 个 built-in Provider 和 200 个内置模型固化为 Rust 只读 catalog，保留原展示顺序、分组、模型能力与配置元数据；
- [x] 覆盖三种协议、鉴权模式、自定义 header、异常响应、重定向、超限、并发 mutation 与 Core 重启；
- [x] 使用未修改的原 Remote Model Manage 页面验证刷新模型、候选添加、能力探测与状态回显；
- [x] 使用未修改的 Chat Model Selector 验证已配置模型切换及 Agent scope；候选添加由原 Remote Model Manage 页面验证，隐藏/恢复由原组件单测与 Rust HTTP 契约覆盖；
- [x] 更新 inventory，确认相关调用均为非占位且 `console/src` 零改动；
- [x] 通过 Rust、Console 与各客户端回归，重建并逐个反向验证全部 9 个发布产物；
- [x] 提交并推送本子切片。

2026-09-05 本机阶段验收：Rust workspace 186 个测试、格式检查和严格 Clippy 通过；Console 295 个测试文件、2453 个测试、production build 和 inventory 防漂移通过，`console/src` 保持零改动。未修改的原 Models/Remote Model Manage 页面连接 release Rust Core，完成自定义 Provider 创建、真实连接测试、自动发现、候选添加、图片/视频能力探测、配置回显和删除；原 Chat Model Selector 完成 Agent scope 下的模型切换，页面请求无 4xx/5xx 且浏览器错误为 0。当前 inventory 为 370 个调用点、322 条 Rust 路由、301 个已注册调用点，其中 297 个非占位、4 个占位、69 个未注册和 11 个静态未解析表达式。TypeScript SDK 3 个、Python SDK 4 个真实 Core 测试、VS Code 57 个测试与 legacy CLI/TUI 855 个测试全部通过。额外执行的 legacy Python 全量单测为 10207 个通过、21 个跳过、7 个失败；失败均位于未修改的 legacy AgentScope/媒体兼容层，当前 Conda 环境明确存在 `reme-ai 0.4.1.5` 与项目要求 `0.4.1.10` 及 AgentScope API 版本不一致，未作为本 Rust 子切片的通过项。macOS App/ZIP/QA DMG、Core archive、WebUI archive、TypeScript/Python SDK、universal/platform VSIX 与 legacy wheel 共 9 个发布文件均以本切片源码重建，并分别完成真实启动、静态服务、临时安装、解包、只读挂载、manifest、Rust-only sidecar、签名和 SHA-256 反向校验。QA DMG 当前为 ad-hoc 签名，待用户提供 Apple 发布凭据后才能生成 Developer ID 签名和 notarized 正式包。

#### 14.2.24.20 后续子切片：本地模型下载与 llama.cpp 运行时

本切片实现原 Local Model Manage 弹窗使用的 12 条运行、下载和状态接口。模型与 llama.cpp 必须下载到新 Rust 版本独立数据目录，采用 staging、大小/条目/路径边界和取消语义；启动必须执行真实 `llama-server`、等待 `/health`、注册 `qwenpaw-local` Provider 并切换模型，停止或进程退出后清理运行状态。测试使用本地 HTTP 归档和可执行 fixture，不下载数 GB 公网模型，也不伪造进程在线。

原前端契约与执行边界如下，`console/src/api/modules/localModel.ts`、`LocalModelManageModal.tsx` 及其子组件保持不变：

| 方法 | 路径 | 原页面语义 |
| --- | --- | --- |
| `GET/POST/DELETE` | `/api/local-models/server` | 读取 installable/installed/available/port/model 状态，启动指定已下载模型，停止当前进程 |
| `GET` | `/api/local-models/server/update` | 比较已安装 runtime 与锁定 release，未安装或平台不支持时必须返回 `has_update=false` |
| `GET/POST/DELETE` | `/api/local-models/server/download` | 读取、启动/更新及取消 llama.cpp 下载，状态严格使用 idle/pending/downloading/canceling/completed/failed/cancelled |
| `GET` | `/api/local-models/models` | 按物理内存返回原两项推荐模型，并合并磁盘中额外 GGUF 仓库，不重复 ID |
| `GET/POST/DELETE` | `/api/local-models/models/download` | 读取、启动及取消单个 HF/ModelScope 仓库下载；同一时间最多一个模型任务 |
| `DELETE` | `/api/local-models/models/{model_id}` | 安全删除已下载仓库；运行中、路径非法或不存在时保持原 409/400/404 边界 |
| `GET/PUT` | `/api/local-models/config` | 已完成；继续提供 max context、固定/自动端口和 provider generation kwargs 的原子持久化 |

实现约束：

- runtime 固定使用原版本 `b8744` 和原 QwenPaw 镜像命名规则；macOS/Linux 使用 tar.gz，Windows 使用 ZIP，按 OS/arch 选择，下载地址只来自内置 HTTPS origin，测试构造器才能注入 loopback fixture；
- runtime 和模型分别只允许一个在途任务，但两类任务可并行；取消进入 `canceling` 后必须等待 worker 退出并清理 staging，再成为 `cancelled`，不能提前报告完成；
- runtime 归档限制响应体、条目数、单文件/总解压量和路径长度，拒绝绝对路径、`..`、符号链接、硬链接、设备文件与嵌套目录逃逸；安装只在完整校验后原子替换，失败不得破坏上一版 runtime；
- 模型 ID 只接受规范的 `owner/repository`，HF 与 ModelScope adapter 先读取真实仓库目录，只下载常规 `.gguf` 文件；限制文件数、单文件/总大小和目标路径，所有文件写入任务 staging，至少一个非 `mmproj` GGUF 后才原子发布；
- `llama-server` 仅以参数数组启动，不经 shell；固定端口必须先检查占用，自动端口由 loopback socket 分配；命令包含 host、port、model、alias、log-file、gpu-layers、ctx-size 和可选 mmproj；readiness 只接受同一子进程仍存活且 `/health` 非 5xx；
- 启动成功后原子写入 `qwenpaw-local` 的 loopback `/v1` URL、唯一模型及能力，并切换全局 active model；停止、更新、异常退出或恢复失败时清空 local Provider 和对应 active 状态；Core 启动时仅恢复自己新数据目录内仍完整的 runtime/model；
- Core shutdown 必须先取消两类下载并终止自己创建的 llama-server；测试覆盖进程组清理，不能误杀外部 Ollama、LM Studio、Python legacy 或其他 QwenPaw 实例；
- HTTP 错误不得包含用户目录、临时路径、远端 token、响应体或命令环境；日志只记录有界的模型 ID、阶段和子进程退出状态。

- [x] 固化原本地运行时状态、下载进度、推荐模型、启动/停止/删除和错误响应；
- [x] 实现跨 macOS/Windows/Linux 的 llama.cpp 包选择、安全下载、解压、更新检查与取消；
- [x] 实现 Hugging Face/ModelScope GGUF 下载、staging、进度、取消、完成恢复和安全删除；
- [x] 实现真实 `llama-server` 进程生命周期、端口占用、health readiness、日志与 Core 退出清理；
- [x] 将运行模型写入 `qwenpaw-local` Provider、active model 与实际 Rust Turn 路由，停止后原子清理；
- [x] 以 loopback runtime/ModelScope mock 覆盖 12 条 HTTP 接口、安全归档、下载安装、启动、停止、删除和 Core 重启自动恢复；
- [x] 使用未修改的 Local Model Manage 弹窗验证已安装模型渲染、启动、运行态刷新、停止和高级设置保存；原 Console 单测保持全量通过；
- [x] 补强在途下载取消、同类任务互斥/两类下载并行、双模型切换、子进程异常退出及 Provider 回退回归；
- [x] 通过全量 Rust/Console/SDK/VS Code 回归，重建并逐个反向验证 9 个发布产物后提交推送。

2026-09-05 本机阶段验收：Rust workspace 191 个测试、格式检查和严格 all-features/all-targets Clippy 通过；Console 295 个测试文件、2453 个测试、production build 和 inventory 防漂移通过，`console/src` 保持零改动。真实 HTTP 契约使用 loopback llama.cpp 归档、可执行 fixture 和 ModelScope mock 覆盖全部 12 条本地模型路由、runtime/model 安装、在途取消与 staging 清理、同类任务互斥、runtime/model 并行、health readiness、双模型切换、Provider 激活、子进程异常退出回退、正常关闭、Core 重启自动恢复、停止及删除；未修改的原 Local Model Manage 弹窗连接 release Rust Core 完成两个已安装模型渲染、启动、切换确认、运行态刷新、停止、逐个删除和 max context 保存，全部请求返回 200 且浏览器错误为 0，另外 24 个内置导航页全量 smoke 均通过。当前 inventory 为 370 个调用点、334 条 Rust 路由、313 个已注册调用点，其中 309 个非占位、4 个占位、57 个未注册和 11 个静态未解析表达式；本地模型调用均已注册且非占位。TypeScript SDK 3 个、Python SDK 4 个真实 Core 测试与 VS Code 57 个测试通过。macOS App/ZIP/QA DMG、Core archive、WebUI archive、TypeScript/Python SDK、universal/platform VSIX 与 legacy wheel 共 9 个发布文件均以本切片源码重建，并分别完成运行、临时安装、解包、只读挂载、manifest、内嵌 Core、签名和 SHA-256 反向校验；legacy wheel 的 CLI/TUI 入口也从安装后成品验证通过。QA DMG 当前为 ad-hoc 签名，待用户提供 Apple 发布凭据后才能生成 Developer ID 签名和 notarized 正式包；Windows/Linux 原生安装态与进程树回收仍由对应平台发布门禁验证。

#### 14.2.24.21 当前子切片：Backup 创建、恢复与可移植归档

2026-09-08 范围确认：用户要求初始测试后继续完成全部已有功能。前端 API 覆盖、打包成功与 AgentScope 运行行为等价分别验收；记忆/检索、上下文总结、执行 hooks、多 Agent 协作和失败恢复不得用页面成功或配置可保存代替运行测试。当前 Rust loop 是独立重写，不是完整 AgentScope Rust 移植。见 [AgentScope replacement boundary](../architecture/system-overview.md#agentscope-replacement-boundary-2026-09-08)。

本切片保持原 Backups 页面、API 模块和交互顺序不变，替换 list/active 两个占位并补齐其余 11 个未注册调用。归档只读取 Rust 新版本的数据目录和已注册 Workspace，不迁移或覆盖 legacy Python 数据。创建任务归 Core 所有，SSE 断开仅取消订阅；只有显式 cancel 或 Core shutdown 才取消任务并清理 staging。

原前端契约与执行边界：

| 方法 | 路径 | 原页面语义 |
| --- | --- | --- |
| `GET` | `/api/backups`、`/api/backups/{id}` | 按时间倒序列出归档；详情提供每个 Agent 的文件数、字节数和名称 |
| `POST` | `/api/backups/jobs`、`/api/backups/stream` | 启动唯一的创建操作；前者返回 202 snapshot，后者兼容 legacy progress SSE |
| `GET/POST` | `/api/backups/jobs/active`、`/{jobId}`、`/{jobId}/events`、`/{jobId}/cancel` | 断线恢复、最新状态 SSE、显式幂等取消和终态保留 |
| `POST` | `/api/backups/{id}/restore` | 按 full/custom 与 scope 恢复，返回 `ok` 和实际保留的本地保护项 |
| `POST` | `/api/backups/delete` | 逐 ID 返回 deleted/failed，不因单项不存在而中断批次 |
| `GET` | `/api/backups/{id}/export` | 下载原始 ZIP，使用安全 attachment filename |
| `POST multipart` | `/api/backups/import` | 上传、完整性/信任校验、409 冲突 token、确认覆盖和临时文件回收 |

归档格式使用 `qwenpaw-rust-backup-v1`：`meta.json` 记录原页面字段及格式/产品版本，`manifest.json` 记录每个归档文件的 SHA-256 与大小；内容位于 `data/workspaces/{agent_id}/`、`data/config/`、`data/secrets/` 和 `data/skill_pool/`。本机创建的归档使用系统凭据库中的随机 HMAC key 签名；导入或恢复未签名 legacy 与异机签名归档必须分别显式选择 `legacy` / `foreign`，接受后用本机 key 重新签名并记录 `accepted_via_trust=true`。`include_secrets` 导出的是有版本、有界的逻辑凭据快照，导入时仅经 `DesktopCredentialStore` 恢复，不生成普通磁盘明文密钥。

SQLite 会话、usage 和非敏感设置通过同一个读事务导出至 `data/core-state.json`，不直接复制运行中的数据库/WAL。`legacy` 信任选项表示本归档格式的未签名文件，不授权导入旧 Python 产品的数据格式。勾选 `include_secrets` 的 ZIP 内含可恢复的凭据内容，HMAC 只提供完整性和本机信任标记，不提供加密；系统签名 key 本身不进入归档。恢复仍需补齐凭据库写入、回滚及全部运行态重载后方能通过本切片验收。

安全与一致性约束：

- 单归档、单文件、条目数、路径长度和总解压量均有硬上限；拒绝绝对路径、`..`、重复条目、符号链接、硬链接、设备文件、压缩炸弹和 manifest 外文件；
- Agent Workspace 只按已注册的 canonical 路径读取；归档内 Agent ID 和恢复目的地必须重新校验，两个 Agent 或未选择 Agent 不能解析到同一物理路径；
- 创建写入同目录 staging 并在签名/manifest 完整后原子发布；取消、失败和 shutdown 不留下可见 ZIP；
- 恢复先完成格式、签名、manifest、scope、空间与目的地预检，再将选中目录解压到同父目录 staging；全部 staging 成功后才逐项原子交换，失败回滚已交换项并保留原数据；
- full 恢复全局配置时替换备份中的完整 Agent 注册表，备份之后新增的注册项从注册表移除；custom 保留未选择 Agent 的本地注册/配置，只覆盖所选 Agent 和 scope。两种模式均不删除未选择或归档缺失的工作目录；恢复期间与创建互斥，并暂停受影响的 Heartbeat/本地模型或在途 Turn，完成后恢复可恢复的运行态；
- pending token 是随机、单次、限时的服务端映射，不直接接受客户端路径；导入临时文件固定在 Rust backups staging 目录，成功、拒绝、过期和 shutdown 均回收；
- HTTP 错误不泄露绝对路径、凭据、签名 key、归档内容或内部数据库细节；list/detail 永不返回签名值。

- [x] 固化原 13 条 Backup 路由、Python 成功/错误/信任响应和原页面调用顺序；
- [x] 初始实现并测试无订阅创建完成、幂等取消/任务互斥、ZIP 导出与再导入、异机信任、冲突 token 单次确认/过期清理、实际凭据键和嵌套 Agent Workspace；
- [x] SQLite 同事务快照与整库事务替换测试通过；在 settings/threads 写入后故意触发 usage 插入失败，验证三张表全部回滚并且重开数据库后不变；
- [x] 修正 API 清单对 `const url = getApiUrl(...); fetch(url, {method: ...})` 的方法识别，覆盖同名局部变量、作用域遮蔽和多个消费者，避免 POST 导入/stream 被 GET 动态详情路由误判为已接通；
- [ ] 实现版本化 archive/manifest、逻辑数据快照、本机签名和有界安全 ZIP 读写；
- [x] 补齐恢复所需归档数据：独立的所选 Agent 注册/配置快照、空 Workspace 标识、Workspace 检查点，以及显式 secrets scope 下的逻辑 OAuth 凭据；
- [x] 修复真实检查点回归发现的控制目录泄漏：快照/预览/恢复排除工作区内实际 Rust 数据目录（含自定义路径），禁止数据库/WAL/归档被递归复制或恢复覆盖；
- [x] OAuth 凭据恢复在写入前校验全部 client/resource/URL/大小，逐键保存原值；失败逆序回滚并报告回滚失败，未勾选 secrets 时不访问 OAuth 凭据库；
- [x] 为 Desktop 模型 key、环境变量、Agent/provider secret 和 MCP headers/env 实现凭据原值捕获与逆序回滚基础；覆盖先写后报错、回滚重试、空变更零访问，并在真实 ZIP 测试中联合回滚文件、所选 Agent 凭据与 Core；完整 scope/保护项授权和生产事务接入仍单独验收；
- [x] 对齐独立 secrets scope 的校验和规划：不受 Workspace Agent 选择限制；创建/导入入口校验逻辑凭据版本、字段、键类别、大小及 OAuth 自身结构；缺失 payload 不授权清空，已知本地业务 key 的删除必须有完整 secrets scope；目标配置匹配、保护项判定与生产恢复接入仍单独验收；
- [x] 按 Agent scope 保留 SQLite 中的聊天名称/分组、Inbox 事件/关联 trace、邮件 ACL 与默认 Agent 的 Cron/Heartbeat 状态；不因关闭全局配置而丢失所选 Agent 数据，也不导出未选择 Agent 的记录；
- [x] HTTP 处理在客户端断开后仍持有停写租约，直到已启动的写入处理结束；补充恢复等待与断开请求的回归；
- [ ] 实现应用所有的创建 job、active/status/SSE/legacy stream、取消、互斥、终态保留与 shutdown 清理；
- [ ] 实现 list/detail/delete/export 和 multipart import、信任确认、409 pending token 覆盖与过期回收；
- [ ] 实现 full/custom restore 的预检、staging、原子交换/回滚、Agent 目录规划和凭据恢复；
- [x] 将 Agent 规划接入真实目录 staging 与 catalog 文件事务基础；保护工作区内实际 Core 数据子树和已有 recovery 目录，验证空工作区、新建父目录、多 Agent 与中途失败回滚；生产 Backup HTTP 协调器接通仍单独验收；
- [ ] 对齐原 full/custom 注册表语义：全局配置包含独立完整 Agent 注册表快照，所选 Agent 的工作区/聊天数据仍按 scope 隔离；full 替换注册项而非删除无关目录，custom 合并所选项；
- [ ] 合并恢复所选 Agent 的 SQLite 会话、聊天分组、Inbox/trace、邮件 ACL 与默认 Agent 的 Cron/Heartbeat；验证 ID 冲突时拒绝且当前快照不变，未选数据完整保留；
- [x] 将所选 Thread/聊天项目路径映射到本机，重写嵌套检查点的 Thread 根路径、ZIP 摘要和图引用，并将检查点目录加入文件事务基础；通过实际 HTTP 预览/恢复验证，未选择的 Agent 和历史文本不变；完整 Backup HTTP 协调器仍单独验收；
- [x] 实现只读 Agent 目录/注册表规划和类型化 SQLite 范围合并基础；以真实创建 ZIP 验证合并后的 Core/SQLite 应用、重开读取和回滚，完整 Backup HTTP 事务接入仍由后续项验收；
- [x] 提取同文件系统 staged 文件/目录交换事务，保留旧内容直到外层文件/凭据/Core 操作全部成功；先接入现有检查点恢复验证中途失败与会话恢复拒绝，回滚失败必须保留恢复数据，不能由临时目录析构删除；
- [x] 恢复停写门禁覆盖 Core 修改入口、Turn 与后台工具完整生命周期、HTTP/App Protocol 处理及 Heartbeat 后置写入；独占恢复时拒绝新修改，等待失败则不替换原数据；
- [x] 在独立内存 Core 中预验证备份及配置，独占期间事务替换 SQLite 和活动 Thread/模型/安全/工具状态；补充验证失败、旧任务回写、取消恢复和重开数据库回归；
- [x] OAuth 异步流程停写：MCP 重配共享回调/凭据任务跟踪，取消旧授权 listener 后等待已启动的阻塞凭据写入，覆盖请求被取消后的晚到写入和恢复后重新授权；
- [ ] 将 Core 恢复基础接入完整 Backup restore：补齐模型/Skill 任务取消及本地服务停启、App Server 缓存重载、文件/凭据联合回滚（含 OAuth），并验证客户端断开不打断交换；
- [x] 接入生产 restore 协调器及首批真实 HTTP 回归：私有归档副本预检、请求 scope、应用所有 worker、全局/Skill staging、独占期最新状态规划、候选加载、文件/凭据/Core 联合提交与失败恢复；HTTP/App Protocol/Heartbeat 在恢复及本地服务重载完成前不接收新工作；
- [x] 验证 restore 客户端在凭据写入期间断开仍完成提交、独立 custom scope 不覆盖未选域、恢复后的默认目录确实存在，以及全局/Skill 目录联合回滚和控制/recovery 路径保护；
- [x] 源模型凭据导出覆盖默认/自定义 Provider、有效易失 key 与显式清空、global/secrets 四种组合；未匹配运行时 URL 时拒绝发布，防止错绑凭据；secrets-only 恢复同步派生的 key 配置标记并验证重启；
- [x] 用实际 HTTP 和本机模型进程 fixture 验证成功恢复启动备份模型、凭据失败回滚后重启原模型、备份模型文件缺失时回退远程 Provider，以及 shutdown 终止进程；该 fixture 只模拟健康检查，不宣称真实模型推理或 Windows 原生进程验收；
- [x] 直接恢复异机归档时持久化显式信任接受并重签名，验证未确认零修改、确认后不再重复提示及默认保护项保留；
- [x] shutdown 等待应用所有的恢复 worker 后，在阻塞线程中对未完成逆向操作重试一次；不重开应用门禁、不启动本地模型，仍失败时保留恢复对象和停写租约并警告；验证故障 fixture 下退出完成、成功恢复原状态及失败后的进程内再次清理；
- [ ] 完成完整多 Agent/OAuth/保护项和原页面回归；SDK config/write 导致运行时 URL 与 Desktop Provider 注册表不一致时目前只做到安全拒绝导出，尚未完成注册表一致性。凭据逆向失败只在进程内保留恢复对象，不宣称跨重启崩溃恢复；
- [x] 将模型候选运行态加载从启动文件写入中分离，严格处理凭据读取失败；独占租约内可按最新状态重建候选 Core，候选凭据使用只读覆盖层；实际模型 HTTP 请求验证恢复后的模型名和 key，完整 Backup 事务仍单独验收；
- [ ] 补齐 Agent/MCP/环境完整候选 hydration，以及启动配置中尚未落入 Desktop 设置的 MCP 配置、内联/易失凭据的归档与保留；不能在关闭 secrets scope 时因凭据库无对应条目而错误清空有效运行值；
- [x] MCP 候选加载按显式恢复值/本机有效值/已有凭据选择敏感字段，返回待提交凭据而不在预检写钥匙串；需要保留的内联值在联合事务中转存、显式清空用空敏感对象覆盖，验证重启不会丢失或复活旧凭据；不访问未选 OAuth 凭据域；完整 HTTP 协调器仍单独验收；
- [x] 环境候选加载按独立 secrets scope 选择完整恢复值或本机有效值，同步候选键目录并返回待提交凭据；有效易失环境导出、缺失 payload 不清空、显式空快照清空、重启、凭据/Core 联合回滚及实际 Agent Shell 执行回归通过；完整 HTTP 协调器与 MCP 环境展开仍单独验收；
- [x] MCP 使用不可变的应用环境上下文展开 HTTP/SSE、stdio 环境和 OAuth 字段，不修改宿主进程环境；环境改变后重建连接缓存，Core 配置/环境更新互斥，OAuth 不使用资源不匹配的旧凭据；候选应用/回滚及真实连接收到对应值已验证，完整 HTTP 协调器仍单独验收；
- [x] Agent 候选加载先校验全部 profile（含 disabled），再按默认 Agent 覆盖全局运行配置；不创建模板或工作目录，验证真实 ZIP 文件/凭据/Core 联合恢复与运行参数回滚；
- [x] 创建归档时捕获有效 MCP bootstrap 配置及显式 secrets 范围的内联字段；配置和凭据在同一 MCP 锁内捕获，空客户端列表明确导出，四种 global/secrets 组合与跨 Core 应用/回滚通过；目标本机易失凭据保留及完整协调器仍单独验收；
- [ ] 覆盖篡改、Zip Slip/链接/炸弹、非法 ID、断线续传、并发、取消、冲突 token、失败回滚与 Core 重启测试；
- [ ] 使用未修改的原 Backups 页面验证创建、进度、刷新续传、取消、详情、导出、导入冲突/信任、恢复和删除；
- [x] 原 Backups 浏览器往返验证创建、刷新保留、导出真实 ZIP、同 ID 导入冲突覆盖、恢复前自动备份、实际文件恢复、搜索/删除及异机导入信任/直接恢复默认保护项；
- [x] 新增显式原页面浏览器门禁：真实在途备份刷新后恢复进度弹窗，SSE 接回同一 job、不重复创建；原取消按钮使任务终止且不发布残留归档，随后可再次创建；使用 fixture 的检查点锁确定性延长任务，不 mock 前端或添加生产控制接口；
- [ ] 更新 inventory，确认 `console/src` 零改动并通过 Rust、Console、SDK、VS Code 与 legacy 回归；
- [ ] 重建 Desktop、Core、WebUI、SDK、VSIX 与 legacy 全部 9 个发布产物，并逐个反向验证后提交推送。

2026-09-08 初始测试：`cargo test --workspace --all-features` 200 个测试通过；涉及模块的严格 all-targets/all-features Clippy 通过；API 方法识别新增 3 个 Node 测试通过。`console/src` 保持零改动。当前清单为 370 个调用点、343 条路由、323 个已注册调用点、2 个占位及 47 个未注册；这些数字只用于定位接口缺口，不能作为完整功能通过的证据。Backup restore 路由尚未注册，当前仅完成 SQLite 事务回滚基础；完整恢复仍需解决运行态停写/重载、Agent catalog 与绝对路径重映射、文件交换回滚、所有凭据类型（包括 OAuth）的范围和原页面验收。本轮未将未完成的 Backup 切片发布为新的 9 个成品包，已有体验包仍对应上一轮已验证源码。

2026-09-08 运行态恢复基础后续进展：新增 8 个回归后，工作区 208 个测试通过。`CoreOperationGuard` 覆盖多步修改、整个 Turn、工具实际执行与后台结果发布；HTTP/App Protocol、Heartbeat、聊天后置检查点、Skill 安装、模型下载及阻塞文件任务保留相应租约。`begin_restore` 取消在途 Turn/工具并有界等待，不排队独占写锁，避免旧操作嵌套获取租约时死锁；持续新流量或不能退出的操作会导致超时，不应用备份，但已请求的任务取消不会撤销。

`prepare_restore` 在独立内存 SQLite/Core 中预验证；取得独占权后 `capture_rollback` 再抓取最新状态与易失的模型 key、环境变量和 Agent 运行参数，避免丢失停写等待期间的最后一次写入。`CoreRestoreGuard::apply` 在所有必要锁就绪后事务替换数据库、同步替换 Thread/usage/模型/MCP/安全/工具策略并清空旧工具缓存；提交后不再 await，也没有可返回失败的步骤。回归覆盖真实延迟模型取消、Turn 已结束的后台 Shell 退出、旧线程不回写、候选配置错误/忙碌/超限、超时/取消释放门禁、内存与数据库一致、保留独占权回滚及重开数据库。完整应用恢复仍未接通，不能把这些 Core 基础测试等同于原 Backups 页恢复验收；OAuth、路径重映射、文件/凭据联合回滚和完整发布包仍按上述未完成项继续推进。

OAuth 停写后续实现：回调 listener 和系统凭据的阻塞 save/delete 使用共享 `TaskTracker`；重新配置 MCP 仍保留对旧流程的跟踪。Core 取得独占租约后取消回调并等待所有凭据任务，候选 Core 应用前也执行同样的协调。阻塞凭据写入不会因请求取消而消失，超时的恢复不得绕过它开始交换。新增 MCP 回归验证旧配置 listener 关闭、重新授权仍可完成、请求被 abort 后阻塞写入仍受跟踪；Core 入口回归验证两次获取/释放恢复租约均能关闭回调且不保存 token。OAuth 凭据的归档导出、恢复和联合回滚仍未完成。

本阶段最终本机验证：`cargo test --workspace --all-features --quiet` 211/211 通过；Core、App Server、Storage、MCP 四个修改模块的 all-targets/all-features 严格 Clippy 通过；`cargo fmt --all -- --check`、`git diff --check`、API inventory 和 3 个 Node 测试通过。`console/src` 零改动。此记录不是新源码的 Windows/Linux 打包验收，也不是完整功能验收；尚未重建或推送本切片的成品安装包。

归档补全后续验证：工作区 218/218 测试及四个修改模块严格 Clippy 通过。`data/agents.json` 独立记录所选 Agent 顺序、配置和源 Workspace，空目录仍可在详情中显示；`data/checkpoints/{agent_id}/` 只包含经过摘要/结构/控制路径校验的已引用快照和状态，不包含孤儿或 staging 文件。真实嵌套 ZIP 回归覆盖默认/自定义 Core 数据目录及 macOS 大小写路径别名：快照不得捕获控制数据，旧式危险快照的预览、恢复和再备份均拒绝，Core 数据保持不变。OAuth 快照包含显式未授权的 null 项，只处理已配置 OAuth client；支持全部预校验、部分写入失败逆序回滚以及回滚失败后的逐键重试。上述凭据基础尚未等同于文件、SQLite、运行态和凭据联合恢复验收。

Agent 范围与断线保护后续验证：工作区 223/223 测试、四模块严格 Clippy、格式及 diff 检查通过。真实 HTTP 创建两个 Agent 的置顶聊天和自定义分组，再逐项读取 ZIP，比较完整聊天/Inbox/邮件 ACL 状态与 Thread 内容；覆盖 writer-only（有/无全局配置）、default-only、global-only。Agent 注册表不再作为全局配置整表复制，而由所选 `data/agents.json` 表达。聊天归属由 catalog 确定，不能由项目路径推断：两个 Agent 可以使用同一项目，所选 Agent 也可以使用自身 Workspace 之外的项目。App Protocol 没有 Agent selector，未登记到聊天 catalog 的 SDK/CLI 会话按默认运行时归属备份，外部项目会话不会因路径不同遗漏。默认聊天内置分组为 schema 要求，可生成静态默认值，但不得夹带未选择 Agent 的重命名/自定义分组。Inbox 只导出选中事件引用且没有跨 Agent 歧义的 trace；损坏的已选数据、跨 Agent 的邮件待办归属返回失败，不发布不完整归档。

HTTP handler 改为应用所有的任务：请求 future 被取消时，handler 仍保留 Core 租约，直到所等待的阻塞写入结束。新增测试主动 abort HTTP 请求，验证此时恢复超时而不交换数据；释放阻塞写入后，原写入完整落库且恢复重新取得独占权。SSE producer 仍按原有独立租约管理。恢复请求本身仍须在完整 restore 实现中持有文件/凭据联合事务，不能仅依赖普通 HTTP middleware。本次未重建或推送安装包；全部既有功能、原 Backups 页完整恢复、Windows/Linux 新源码与 9 个发布产物仍未整体验收。

清单校验同时修复 Rust 测试文件中的路由被误计为正式接口的问题，排除 `_tests.rs`、`tests.rs` 与 `tests/`。重新生成并验证后仍为 370 个调用点、343 条 Rust 路由、47 个未注册调用点；新增断线测试的 `/write` 不进入发布清单。3 个 Node 回归通过，`console/src` 零改动。

文件交换事务后续进展：`desktop_restore_files` 在目标父目录创建独立 staging/recovery，用同文件系统 rename 替换文件或目录；旧内容保留到外层明确 commit。任一步交换失败时，包含“旧文件已经移走，但新文件未安装”的当前步骤在内逆序回滚，单步逆操作失败不阻断其他条目，重试只处理尚未完成的逆操作。未提交事务析构会回滚；失败的原件保留在 `.qwenpaw-restore-*`，不会随临时目录自动删除。父目录被重定向时拒绝交换/清理；只清理事务新建的空父目录，不递归删除新增文件。恢复残留目录排除在后续 Backup 与检查点采集之外，防止反复嵌套归档。

该事务已实际接入原检查点恢复，而不是只保留测试用实现。恢复文件后仍保留原件，直到检查点头和 Core 会话成功；会话恢复拒绝时逆序恢复新增、修改、删除文件及新建空目录，并恢复 safety head。Core 持久化成功后同步标记文件事务提交，再异步清理原件，避免取消清理任务导致只回滚文件而不回滚数据库。7 个交换/回滚单测及新增真实 HTTP 会话拒绝回归通过，原真实检查点恢复/预览/GC 测试也通过。本阶段全量测试达到 231/231，四模块严格 Clippy 与 API 清单校验通过；完整 Backup restore 的范围合并、路径重映射、凭据联合事务和进程/缓存协调仍未接通。这不是跨进程崩溃原子性或 Windows/Linux 新源码验证的证明，尚未重建发布包。

注册表语义校正（2026-09-08）：上文 223 测试阶段“注册表只由所选 `data/agents.json` 表达”的方案不能满足原 full 恢复语义，已调整。全局 scope 单独导出 `data/config/agent-registry.json`，仅含全部 Agent 的 id、Workspace 引用与 enabled/pinned，不包含未选择的 Agent 运行配置、文件或聊天。所选 Agent 的完整配置仍由 `data/agents.json` 表达；两份快照的共同引用必须一致。full + globals 替换全部注册项，custom 只合并所选项。只读恢复规划优先使用现有本机目录，其余映射至本机 fallback/id，不将备份中的外机绝对路径用作写入目标；注销某个 Agent 不授权删除或覆盖它的目录。该规划还未接入完整恢复事务，清单继续保持未完成。

范围恢复基础后续验证：9 个 Agent 规划测试覆盖 full/custom 注册差异、global-only 引用恢复、本地缺失/新建 Workspace 映射、未选及注销目录保护、Core 控制目录、符号链接别名、父路径穿越、引用不一致与合并后数量上限。7 个类型化状态合并测试比较完整结构，覆盖所选聊天/分组、Inbox/trace、邮件 ACL、usage、默认 Agent 的 SDK/CLI 会话和 Cron/Heartbeat；global-only 不替换 Agent 状态，security/MCP 仅报告实际覆盖保留的本地键。Thread、Inbox event/run/trace 和 usage ID 冲突会在写入前拒绝；未选择 Agent 与无明确归属的孤立 trace 不被局部恢复删除。usage 导出同时修正为始终按 Agent scope 隔离，全局配置不再附带未选 Agent 的 ledger。

真实集成回归从 HTTP 创建的 Agent/聊天数据生成 writer-only ZIP，修改本地聊天后，将 ZIP 的逻辑快照与当前状态合并，经 detached Core 预验证、独占租约、实际 Core/SQLite 应用和回滚，分别比较完整快照并重新打开 SQLite 验证持久化结果；默认 Agent 的本地修改不被覆盖。另有归档回归证明 manifest/HMAC 合法也不能绕过全局引用与所选配置的一致性校验，完整运行配置不得混入 reference-only 注册表。工作区 249/249 测试、四模块严格 Clippy、格式/diff 和 API inventory 校验通过；清单仍有 47 个未注册调用，`console/src` 零改动。

这仍不是完整 Backup 页面恢复验收：新的规划和合并函数暂由集成测试调用，尚未接入生产恢复协调器，不能据此勾选 full/custom HTTP restore。下一步直接接入目录 staging/路径重映射（含嵌套检查点摘要）、候选运行配置/凭据加载、应用所有的联合交换与回滚、进程及缓存重载；接通时移除明确标记的临时 dead-code allowance，再执行原页面交互和全部发布产物回归。本阶段不推送未完成的 Backup 切片，也不重标记旧体验包为新源码。

工作区交换后续进展：`desktop_backup_restore_workspaces` 先检查全部所选目标、manifest 路径冲突、文件数/大小与受保护路径，再向同文件系统 recovery 目录解压，并在每个文件落盘时重新验证大小与 SHA-256。普通 Workspace 整目录交换；包含实际 Core 数据或旧 `.qwenpaw-restore-*` 的 Workspace 只递归交换其他条目。空 Workspace 由显式 Agent 元数据授权，不能凭缺失的 scope 擦除目录；新建父目录由事务跟踪，回滚只删除本次创建的空目录。已有恢复残留不会被整目录替换后的清理间接删除。

Workspace 恢复对本地符号链接/Windows reparse entry 使用显式条目交换策略：只 rename 链接本身，不跟随外部目标；原普通文件/检查点事务仍默认拒绝链接目标。原目录权限在目录替换时保留，避免将私有目录扩大为普通默认权限。所选 `agent.json` 仅重映射已知的 Workspace/project 路径字段，不替换描述、提示或其他任意文本；仍先核对归档原字节摘要，非法配置对象在 staging 阶段拒绝，未选文件和空目录不会因此被生成新配置。

11 个目录 staging 定向测试覆盖多 Agent/default 内嵌 Core、嵌套 recovery 保留、空 Workspace、新建父目录 commit/rollback、控制路径/父文件冲突、后续摘要失败、最后 catalog 交换失败、Unix 链接目标保护，以及 Agent 配置路径重映射/非法对象；另有私有目录权限回归。实际 ZIP 的原状态合并测试已扩展为 Workspace 文件、catalog 与 Core/SQLite 联合应用和回滚，保留独占租约并验证未选默认 Agent 的本地文件修改和重新打开的数据库结果。这些调用仍由集成测试驱动，不代表生产 Backup restore 已注册；检查点/Thread 路径重映射、global/Skill scopes、凭据联合恢复、进程与缓存协调仍需继续接通，尚未重建发布产物。

本阶段本机验证：Rust 工作区 261/261 测试、Core/App Server/Storage/MCP 四模块 all-targets/all-features 严格 Clippy、格式及 diff 检查通过；Console API inventory 与 3 个 Node 回归通过，`console/src` 零改动。当前仍有 47 个未注册调用；新增目录事务的 Windows junction 原生执行、完整 Backups 页面恢复与全部成品包均未在本阶段验收，不以本机 Unix 链接用例替代跨平台证明。

检查点跨目录恢复后续进展：所选 Thread 的 Workspace 根路径及聊天 `runtime_context` 中已知项目路径按 Agent 归属映射到本机，不按共享项目路径猜测归属，不修改历史消息、标签或任意文本。外部项目仍保留原引用，不能授权复制未选项目。嵌套检查点 ZIP 在独立 staging 中先校验原摘要、Thread/Turn、路径、链接、声明大小与累计解压预算，再重写根路径和所含 `agent.json` 的已知路径；内容实际改变才生成新摘要，并同步更新父节点与 HEAD。相同本机路径保留原 ZIP 字节和 commit。新检查点目录与 Workspace/catalog 共用文件事务；包含旧 recovery 的检查点目标拒绝替换。

真实跨 Workspace/不同 Core 数据目录集成回归已通过：从 HTTP 创建的 ZIP 规划并应用候选 Core 与文件后，经实际检查点 graph/preview/restore 路由恢复旧版文件，源目录和历史内容不变。该测试直接调用 Backup 恢复组件，不是尚未注册的 Backup restore 路由。生产导出同时修复共享项目的 Agent 隔离：检查点状态、HEAD 和嵌套 ZIP 只包含所选 Agent 的 Thread，归属取自与逻辑会话相同的 SQLite 快照，源检查点状态不被过滤修改。逻辑元数据条目也计入 20,000 条归档上限；嵌套 ZIP 的声明大小现在在实际检查点读取中有界校验。

Desktop 凭据事务基础新增 6 个单测：任何写入前先读完原值，空变更不访问凭据库，未改变的值不重写；覆盖每个域写入前失败/写入后报错、逆序回滚继续处理其他键、失败逆操作重试以及析构不隐式访问凭据库。错误不包含底层凭据诊断，类型中不提供备份签名 key 的写入能力。真实 Agent-only ZIP 集成回归已加入所选邮件凭据，联合验证文件/catalog/Core/SQLite 应用与回滚，完整比较未选凭据及签名值保持不变。此基础只接收调用方已授权的变更；全部凭据 payload 校验、scope/本地保护项筛选、候选运行配置 hydration 与 OAuth 联合协调仍未完成，不能把空变更测试当成完整 HTTP secrets-scope 验收。

本阶段最终本机验证：Rust 工作区 277/277 测试、四模块 all-targets/all-features 严格 Clippy、格式/diff 检查、API inventory 与 3 个 Node 回归通过；`console/src` 零改动。当前仍有 47 个未注册调用，完整 Backup restore 尚未注册；本阶段未重建或推送 9 个发布产物，也未将 macOS 本机测试作为 Windows/Linux 新源码验收。

凭据范围语义校正：对照原 Python `add_secrets` / `_stage_secrets`，secrets 是独立的完整范围，不受 `include_agents` 或所选 Workspace ID 限制。上阶段“只接收调用方已授权键”的事务基础不是实际产品范围规则。当前恢复规划已按全部已登记业务凭据键与归档键计算替换/删除；未勾选 secrets 或 payload 缺失返回空计划，不清空任何 key。备份签名 key 和无关 OS 凭据不属于业务目录，永不进入该计划。MCP 本地保护项实际被保留时，计划排除 MCP headers/env 和 OAuth；OAuth 仍须在最终目标配置中匹配 client/resource 后才能读写。

生产创建与导入现已接入逻辑凭据校验，而非只在测试中调用。校验要求版本、必需域（含显式 nullable API key）、严格字段、合法 Agent/provider 命名空间与域大小限制；复用现有 Agent/模型环境规则，拒绝默认 API key 重复别名、伪造 signing key 类别、嵌套 MCP headers 注入和非法 OAuth 结构。OAuth 的无副作用格式校验与恢复时目标资源校验分离，异机凭据可先合法导入而不要求本机已经有同名 client。模型 registry 的类型化读取共用于凭据键目录，读取有界且使用同一打开句柄，避免重复实现源配置解析。

真实 HTTP 回归验证：摘要/本机测试签名合法也不能绕过凭据校验；显式 foreign trust 不授权非法 payload，失败不发布 ZIP、不创建 pending token、不修改本机业务 key，响应不包含密钥。有效异机凭据归档可导入但不会立即应用。实际 ZIP 联合回滚测试已改用完整 secrets 规划：writer 工作区回到归档版本、默认工作区保留本地编辑，而两个 Agent 的凭据都按独立 secrets scope 恢复；回滚后完整凭据、文件、catalog 与 SQLite 均回到恢复前状态。此前“未选凭据不变”的测试只验证事务接受的子集，并不能代表勾选完整 secrets 的产品语义，本次已纠正。完整 Backup restore HTTP 协调器、候选运行配置 hydration、OAuth 联合事务和发布产物仍未完成。

本阶段本机验证：Rust 工作区 287/287 测试通过；Core/App Server/Storage/MCP 四模块严格 all-targets/all-features Clippy、格式/diff 检查及 Console inventory 的 3 个 Node 测试通过。`console/src` 零改动，清单仍有 47 个未注册调用。没有将本阶段声明为原页面完整恢复、Windows/Linux 新源码或 9 个发布产物验收；本轮未提交推送未完成的 Backup 切片或重建体验包。

模型候选加载后续进展：模型 registry 的选择规则抽取后与现有启动路径共用，恢复路径单独读取已验证的内存字节并返回规范化后的 staging 字节，不写原 registry、Workspace 模板或系统凭据。凭据读取失败和非法 key 返回经过脱敏的错误，不能沿用原启动路径“警告后按无 key 启动”的行为。只读候选凭据层中，显式 null 覆盖本地值，未出现在计划中的键回退到本地凭据库；所有写入和签名 key 访问均被拒绝。这个覆盖层不代表内联/易失凭据的保留已经完成，MCP bootstrap 配置与有效运行值仍须在完整 hydration 中显式处理。

`CoreRestoreGuard::prepare_restore` 支持独占停写后，用同一租约下最新合并的快照重建候选 Core；不会再调用普通 `Core::prepare_restore` 触发读/写租约冲突。回归覆盖在途操作完成前的最后一次修改、普通入口在恢复中被拒绝、租约入口成功构建、非法候选不改动原状态。实际本机模型服务测试将模型候选应用到 live Core 后运行一轮 Turn，断言收到恢复后的 URL 对应请求、Authorization、模型名、stream 和用户输入，并收到正常结束；该测试未调用尚未注册的 Backup restore HTTP 路由，不能替代完整页面恢复验收。

本阶段最终本机验证：Rust 工作区 294/294 测试通过；四模块 all-targets/all-features 严格 Clippy、格式/diff 检查、Console inventory 与 3 个 Node 测试通过。`console/src` 零改动，仍有 47 个未注册调用。模型加载及租约重建基础已验证，不代表完整 Backup 页面、全部既有功能或跨平台发布已通过；本轮未重建或推送 9 个发布产物。

Agent 候选加载后续进展：`hydrate_restore` 复用现有 Agent 设置和运行参数校验，先验证全局设置与全部计划中的 profile，再一次替换候选的默认运行参数；默认 Agent 的 `running` 按启动语义覆盖全局值，缺失该字段才使用全局值，缺失默认 Agent 或损坏的 disabled profile 都拒绝。4 个测试比较完整运行参数，覆盖独占期应用/回滚、无模板/目录生成、默认覆盖与全局回退、非法 profile 不部分修改候选。真实 ZIP 联合文件/凭据/Core 回归也已加入该候选加载与运行参数回滚断言；仍不是生产 Backup restore HTTP 入口验收。

本轮修复实际导出缺口：未写入 `desktop_mcp_data` 的 bootstrap MCP manager 现在也进入全局逻辑快照；导出的是有效配置的脱敏版本，明确包含空客户端列表，避免全局恢复意外继承目标机器的客户端。MCP 配置与敏感字段在同一次 MCP 锁内捕获，随后释放锁再复制工作区。显式 secrets scope 使用有效 manager 的 headers/env/旧式 OAuth 配置字段，不要求它们已经存入平台凭据库；独立 OAuth 凭据仍按原逻辑导出。global/secrets 四组合的真实归档测试完整比较配置和敏感域，验证未选 secrets 不读 OAuth、来源 Core 不被写入配置、另一 Core 中配置及内联凭据可加载/应用/回滚。关闭 secrets 时保留目标已有 bootstrap/易失值、保护项匹配以及模型易失 key 绑定仍待完整运行态协调处理，不把本次源归档修复宣称为全部凭据恢复已完成。

本阶段最终本机验证：Rust 工作区 299/299 测试、Core/App Server/Storage/MCP 四模块严格 all-targets/all-features Clippy、格式/diff 检查、Console inventory 与 3 个 Node 测试通过；`console/src` 零改动。清单仍有 47 个未注册调用，完整 Backup restore HTTP 与 9 个发布产物仍未验收，本轮没有重建或推送体验包。

2026-09-09 MCP 目标侧恢复基础：候选加载区分显式替换/清空、本机有效内联值、已有凭据库值；全局元数据夹带 headers/env/旧式 OAuth token 时拒绝。预检只返回需要随联合事务提交的敏感对象，不写系统凭据，不访问独立 OAuth 域。保留启动内联值时将同一有效值转存到凭据库，显式清空用空对象覆盖而不是单纯删 key，防止重启加载 bootstrap 时复活旧值。3 个 MCP 回归覆盖实际持久化 Core 重开、联合回滚、新客户端现有凭据加载、后续字段错误不部分修改候选。保护项判定同时修正为有效安全默认值和 bootstrap MCP 都算本机配置，不再仅检查 SQLite 是否已有键；四种 global/protect 组合验证无 live 修改。本轮起始工作区 303/303 测试通过。

环境候选及导出后续进展：独立 secrets scope 决定完整环境替换，关闭 scope 或缺失 payload 保留本机有效运行值；显式空对象清空环境。候选加载先验证变量名目录和全部值，再同步候选目录与运行态，返回需加入外层凭据事务的值；全局配置中的外机变量名不授权读取或恢复其敏感值。显式 secrets 导出在 Desktop 环境锁内读取已注册凭据并叠加应用注入的有效运行值，后者覆盖同名旧值；不枚举操作系统进程环境。已知业务凭据目录纳入易失环境键，使完整 secrets 替换可清理其旧凭据，不误删无关 OS key。

4 个新增回归使用真实创建 ZIP 的 global/secrets 四组合，比较完整环境、SQLite 和凭据结构，验证新值物化后重开数据库与正常 Desktop 初始化仍生效、联合回滚恢复原值，以及非法目录/变量/大小在候选写入前拒绝。另经本机模型 HTTP fixture 发起真实 Agent Shell 工具调用，分别在 Core 应用和回滚后读到对应环境值，测试进程环境不被修改。仅本机 fixture 与内存凭据参与测试。完整 Backup restore HTTP 仍未注册，MCP `${ENV}` 仍需候选专属环境上下文，模型易失 key 绑定、global/Skill staging 和进程/cache 联合协调也仍未完成；这些回归不是原页面全恢复或最终发布验收。

本阶段最终本机验证：Rust 工作区 307/307 测试通过；Core/App Server/Storage/MCP 四模块 all-targets/all-features 严格 Clippy、格式/diff 检查、Console inventory 与 3 个 Node 测试通过。`console/src` 保持零改动，清单仍有 47 个未注册调用。本轮未提交推送未完成的 Backup 切片，未重建 9 个发布产物，也未将旧体验包重标记为当前源码或跨平台全功能验收包。

MCP 环境上下文后续进展：应用环境随 manager 不可变保存；HTTP/SSE 的 URL/header、stdio 子进程继承和 client env 覆盖、旧式 OAuth access/refresh/token endpoint，以及交互式 OAuth resource/client/scope/endpoint 元数据均使用同一上下文。未提供的变量仍按原行为回退到宿主环境，显式空字符串覆盖宿主值，过程中不修改进程环境。相同上下文保留连接缓存，不同上下文新建连接和工具路由；旧 Turn 持有的 manager 快照仍使用旧上下文，OAuth 活动跟踪器继续共享。

Core 的环境更新和 MCP 重配置共用 MCP 写锁，环境更新同时获取两个状态锁后再修改，避免重配置覆盖刚生效的环境。恢复加载顺序必须先环境、后 MCP binding；候选在写元数据之前校验启用客户端展开后的 URL、header、OAuth 字段及 stdio env，不启动网络或读写 OAuth 凭据。禁用配置保持未激活状态。未闭合变量表达式的错误也不再回显完整配置值。

OAuth 业务账号保持原配置 URL 的键身份，避免变更导致已有钥匙串条目不可见；但取出的凭据资源必须等于当前展开后的资源。不匹配时呈现未授权，不刷新或发送旧 token，仍允许显式重新授权；Backup prepare 继续拒绝资源不匹配的归档。测试验证新旧环境对应的 OAuth 授权状态、预检零写入和凭据应用/回滚，均不访问真实系统凭据。

本轮新增 7 个回归覆盖上下文保留/替换、预检拒绝非法展开值、stdio 实际子进程继承及覆盖、交互式 OAuth 完整本机回调、OAuth 资源隔离、App 候选错误不部分修改，以及 Core 应用/回滚后真实 HTTP MCP 连接的路径与 Authorization。原 HTTP OAuth 刷新和 legacy SSE 回归也改为由应用环境提供配置，验证实际端点收到展开值。此进展尚不代表整包 Backup restore 已接通；模型易失 key 绑定、global/Skill staging、进程/cache 协调和生产联合事务仍需完成。

本阶段最终本机验证：Rust 工作区 314/314 测试、四模块 all-targets/all-features 严格 Clippy、格式/diff 检查及 Console inventory 的 3 个 Node 测试通过。`console/src` 零改动，仍有 47 个未注册调用。本轮没有将恢复组件的通过声明为原 Backups 页完整恢复、Windows/Linux 原生执行或 9 个发布产物验收；未重建或推送本切片安装包。

2026-09-09 生产恢复入口进展：`POST /api/backups/{id}/restore` 已注册，私有 ZIP 副本经完整校验后按 full/custom 和独立 scope 规划。协调器持有应用所有的 worker，在取消 Skill/模型工作和取得 Core 独占租约后再次读取最新本地状态；候选完成 Agent/环境/MCP/模型加载，文件交换和 Desktop/OAuth 凭据先提交，Core/SQLite 最后应用，随后同步提交文件事务并清理会话别名、审批、推送及工作目录缓存。本地模型恢复沿用普通启动逻辑，应用门禁保留到该流程完成。有效目标模型 key 仅在 Provider URL 匹配后保留，显式恢复值优先。

本阶段新增 7 个真实 HTTP 回归，覆盖完整恢复后数据库重开、凭据先写后报错的联合回滚、逆向失败后保留停写并由下一次明确请求先恢复旧状态、客户端断开仍提交、shutdown 等待断线后的提交，以及 independent custom scope 和默认目录映射。另有 3 个全局/Skill 文件事务回归，验证联合逆向恢复、不修改控制/recovery 目录、缺失 payload 不清空本地文件。修正选中子目录被恢复树移除后仍被缓存选中的问题；未选择全局 scope 时不改当前默认目录和全局设置。Rust 工作区 324/324 测试、四模块严格 all-targets/all-features Clippy、格式/diff 检查通过，inventory 的 3 个 Node 测试通过；当前为 370 个调用点、344 条路由、46 个未注册调用。

原 Console 全量命令首次运行发现新增 `node:test` 文件被 Vitest 重复收集：原 295 个文件/2453 个用例通过，但总命令因该重复收集失败。现已在 Vite 测试配置中仅隔离这个 Node 文件，仍由 `verify:api-inventory` 显式运行，未删测试且 `console/src` 零改动。修正后完整重跑 295/295 文件、2453/2453 测试通过，独立 Node 测试 3/3 和 inventory 通过；production build、Monaco CSS 检查、369 个静态资源预压缩及 initial bundle 检查通过。这是原 Console 构建和组件回归，不是全部交互的浏览器端到端验收。

剩余边界不因此关闭：原 Backups 页面完整浏览器流程、活跃本地模型恢复/回退、全范围跨 Agent/保护项/OAuth HTTP 组合、源模型易失 key 导出和 Provider 绑定仍需完成；直接恢复未经 import 重签名的异机归档尚未持久化该次信任接受标记。凭据回滚原值只在当前进程中保留，不能宣称崩溃/重启可恢复。9 个发布产物尚未按本切片重建，旧体验包没有重标记为当前实现；完整已有功能仍未验收。

原页面浏览器验收后续进展：新增显式浏览器门禁，使用真实 Chrome、未修改的 Console production build、临时工作目录及两套内存凭据；从原 UI 控件完成本机备份往返，再导入另一套签名密钥生成的归档并恢复实际文件。导入响应顺序独立检查为 409/200（冲突及覆盖）、400/200（未信任及显式确认），不能用同路径最后一次成功响应遮住其他失败。用例说明和复现命令见 [Backup browser acceptance](../testing/backup-browser-acceptance.md)。

浏览器回归发现并修复了实际产品问题：异机归档默认保留本机安全配置时，合并器此前把裸 `SecuritySettings` 写入需要版本封装的持久化字段，导致候选 Core 加载返回 400。现复用 Core 的版本化安全配置编码，包含尚未落库的默认值；原保护项测试增加真正加载候选 Core 的断言，而不是只比较错误结构。新增生产 HTTP 回归同时验证自定义本机安全规则、有效 bootstrap MCP 保留以及数据库/桌面重开后仍生效。

本阶段最终验证：默认 Rust 工作区 325 个测试通过；另行显式执行的 1 个浏览器门禁通过，包含全部 24 个导航页及本机/异机 Backups 原页面往返，浏览器异常和非预期 API 失败均为 0。四个修改模块严格 all-targets/all-features Clippy、格式/diff 检查、Node inventory 3 个测试与清单校验通过。`cargo build -p qwenpaw-cli --release --all-features` 成功，`target/release/qwenpaw-core` 的版本和 CLI 帮助验证通过；TypeScript SDK 构建及 3 个测试、qwenpaw conda 环境下 Python SDK 4 个测试通过，两个 SDK 均显式连接该次 release 二进制；VS Code 编译及 57 个测试通过。`console/src` 零改动，原 Console 的 2453 个测试和生产构建沿用上一轮已通过的同一前端源码。仍有 46 个未注册调用，未将导航渲染通过等同于这些功能已实现。DMG/ZIP/Core archive/WebUI archive/SDK packages/VSIX/legacy wheel 共 9 个发布文件尚未以本切片源码重新封装验收，本轮未提交推送或重标记旧体验包。

补充 release 客户端验证：VS Code 再次显式设置 `QWENPAW_CORE_BIN=target/release/qwenpaw-core`，57/57 测试通过且无跳过，包含真实 App Protocol 连接、文件引用、Thread 分页、Shell 工具审批和在途模型请求取消。

2026-09-09 模型凭据和本地服务恢复后续进展：归档创建在同一 Desktop 模型锁内捕获已校验的注册表及所选凭据，注册表不再随后从文件系统重新读取；有效 Core key 优先于已过期的存储值，有效空值也作为明确清空，非活动 Provider 凭据保留。16 种默认/自定义 Provider、有效 key/清空、global/secrets 组合的 ZIP 检查通过，未选择 secrets 的归档没有测试凭据内容。运行时 URL 不匹配时拒绝导出且不发布部分归档；这不等于 SDK 和 Desktop 配置目录已完全一致，也不宣称整个文件系统获得同一时刻的原子快照。

恢复候选根据将要应用的全部 Provider 凭据重建 `api_key_configured` 标记；secrets-only 也通过现有文件事务持久化这些派生标记，不替换其他 Provider 设置。有效 key/清空两种恢复、数据库及 Desktop 重开均通过。实际 HTTP 本地模型测试以临时 Python 健康服务 fixture 覆盖模型 A 恢复、失败后模型 B 重启、A 文件缺失后的远程回退三条路径；每条均检查 Core URL/模型/key、模型资产未被误删及 shutdown 后监听端口关闭。Python fixture 运行于 qwenpaw conda 环境，不是新产品的 Python 内核依赖。

直接信任恢复使用已校验的私有 ZIP 副本重签名并原子持久化接受标记，不重新读取可变的公开归档；明确接受信任是独立决定，后续恢复失败不撤销该记录，与导入确认的语义一致。

本阶段 Rust 全量回归最初暴露跨秒时间假设：失败 Turn 的更新时间被要求等于 Thread 创建时间。核对完成逻辑先写存储、后发事件后，将测试改为精确匹配完成元数据时间，新增 SQLite 重开后的完整 Thread/Turn/元数据比较；未改动生产完成顺序。修正后工作区 330 个普通测试通过；原 24 页及 Backups 往返浏览器门禁通过。release Core 构建、TypeScript SDK 3 个、Python SDK 4 个、VS Code 57 个显式 release 客户端测试通过。严格 Clippy、格式和 inventory 验证通过；新增在途刷新/取消浏览器门禁仍在本机验收中。9 个分发产物尚未重新封装，未提交推送，不能将旧包视作上述源码。

在途任务及退出清理验收后续：新增第二个显式浏览器门禁通过，覆盖刷新到新 Document 后接回原 SSE/job、取消不发布归档及后续创建，详见浏览器验收文档。退出清理现等待恢复 worker 结束，再在阻塞线程对保留的逆向操作重试一次；成功释放 Core 独占租约，失败保留恢复对象并告警，两者都不重新开放应用门禁或重启模型服务。新增生产 HTTP 测试注入持续多次凭据写入失败，验证退出重试成功与再次失败两种情况，后者仍可在同一进程再次清理；最终完整凭据、文件、Core 快照及重开 SQLite 与恢复前一致。不会自动重试无限次，也不宣称阻塞系统钥匙串调用具备硬超时或恢复原值可跨进程保留。

本阶段最终验证更新：Rust 工作区 331/331 普通测试通过；两个 ignored 浏览器门禁均单独显式通过，包括 24 页导航、备份本机/异机往返及在途刷新/取消，浏览器异常和非预期 API 失败为 0。Core/App Server/Storage/MCP 严格 all-targets/all-features Clippy、格式/diff、Node inventory 3/3 及清单校验通过。退出清理改动后重新构建 release Core，TypeScript SDK 构建及 3/3、qwenpaw conda Python SDK 4/4、VS Code 编译及 57/57 均再次通过并显式连接最新 release 二进制。`console/src` 零改动；完整 Console 2453 测试及生产构建沿用本切片前面记录的相同前端源码。仍有 46 个未注册调用及完整多 Agent/OAuth、Provider 一致性等未验收功能；未重建 9 个分发产物，未提交推送，未将本阶段进展标为全部功能完成。

#### 14.2.24.22 OpenRouter 原模型管理交互

在已批准的原前端全交互等价范围内，补齐原弹窗依赖的三个 OpenRouter 专用接口，不修改 `console/src`。以当前 Python 路由、Provider 归一化及原 React 消费逻辑为契约：模型按 ID 首次出现去重，系列排序，斜杠 ID 使用最后一段为名称；输入/输出模态分别使用“任一匹配”，系列忽略大小写，`is_free=false` 不排除免费模型，价格上限直接比较原 per-token 字段。扩展发现中的非空 key 沿用现有安全 Provider 更新流程；发现和筛选不自动添加模型，原 Add 动作仍负责持久化。

- [x] 复用有界模型目录传输，保留扩展价格/模态字段和分页、错误、重定向安全边界；
- [x] 实现 series、discover-extended、models/filter 三条原 HTTP 契约，以及匹配旧版的归一化/筛选测试；
- [x] 真实本机 HTTP 测试验证凭据与自定义请求头、失败不泄密、不隐式添加模型、原 Add 持久化及 Core 重开后加载；
- [x] 对齐原 OpenRouter 多模态探测：读取模型目录元数据，不调用付费聊天探测；目录缺失/失败不能覆盖已知能力，保留现有版本冲突检查；
- [x] 使用未修改的原模型管理弹窗完成系列/模态/免费筛选和添加、刷新及多模态探测，检查真实请求与持久化结果；
- [x] 更新 inventory，通过 Rust 全量、release Core 构建、SDK/VS Code 和三个显式原前端浏览器门禁；
- [ ] 纳入后续统一制品重建与逐件安装态验收，不把当前 release 二进制等同于已重打包的全部发布文件。

2026-09-09 实现进展：系列与筛选每次读取真实 Provider 目录，无静态伪造或隐式添加。请求使用当前安全凭据、原 OpenRouter 默认标识头及大小写不敏感的自定义头覆盖；扩展发现的非空 key 显式写入现有凭据存储，注册表只记录配置标记。六个普通测试覆盖原完整响应字段、首次去重、系列排序、模态/价格/免费组合、未知/极小非零价格、两套发现接口的三页游标、HTTP 失败/无效目录/重定向、不泄露凭据、添加与持久化，以及多模态探测目录缺失时保持原能力。

三页测试先复现并修复共享传输的问题：后续页此前不断追加 `after`，第三页请求包含两个游标；现在每页从初始端点重新构造分页参数。普通发现与 OpenRouter 扩展发现均验证请求顺序恰为初始页、after=one、after=two。原浏览器筛选/添加/刷新已通过，新增元数据探测点击及最终全量回归继续验收。清单新增三条实际路由后未注册调用从 46 降至 43，不能据此认定其他功能或发布产物完成。

本切片最终本机验证：Rust 工作区 337/337 普通测试通过；另外三个浏览器门禁显式通过，覆盖原 24 页导航、本机/异机备份往返、在途刷新/取消及 OpenRouter 模型筛选/添加/刷新/目录能力探测，均无浏览器异常或非预期 API 失败。四模块严格 all-targets/all-features Clippy、格式/diff、Node inventory 3/3 通过。重新构建的 release Core 被 TypeScript SDK 3/3、qwenpaw conda Python SDK 4/4、VS Code 编译及 57/57 显式连接并验证。`console/src` 零改动；Console 2453 测试及 production build 沿用前面记录的同一前端源码。当前 inventory 为 370 调用、347 路由、43 未注册调用、2 占位；Provider OAuth、其他剩余功能及 Backup 完整边界继续由前面的未完成项跟踪。本切片未重建 9 个分发包、未提交推送，不宣称全功能等价或跨平台安装态验收完成。

#### 14.2.24.23 原技能市场与安装链路

继续已批准的原前端等价改造：以原 `market` 四个 Provider、分类注册表及 `useMarketSearch/useMarketInstall` 为契约，提供来源状态、语言化分类、按来源独立分页与部分失败结果。QwenPaw/ModelScope 使用公开目录；ClawHub 分别实现关键字 overfetch 与游标浏览；Aliyun 使用 Rust ACS3-HMAC-SHA256，读取有效应用环境中的 AK/SK（缺失时报告不可用），不引入 Python SDK/内核。网络请求有超时和响应大小边界，签名不跨重定向发送。

检查发现现有 Rust Hub 安装仅直接下载 URL，尚不能把市场详情页 URL 解析为包；所以“列表能显示”不算完成。市场搜索与四类 source_url 安装解析一并跟踪，复用既有扫描、staging、任务取消和文件事务，禁止直接执行下载内容。

- [x] 接通 providers/categories/search 三条契约，保留来源顺序、分类映射、分页与单来源失败隔离；
- [x] 实现四来源的实际网络请求和响应归一化，Aliyun 签名通过官方已知向量及本机请求校验；
- [x] 补齐四来源详情 URL → 可验证包/文件的安装解析，验证工作区任务队列与 Skill Pool 同步导入、失败及工作区取消；
- [x] 以临时数据、本机目录/包服务和假凭据完成 HTTP 全结构测试及原市场页浏览器搜索/筛选/分页/安装；
- [x] 用任务状态锁保护最终磁盘提交和完成状态发布，回归并发取消不会把已提交技能报告为 cancelled；
- [x] 更新清单、运行全量及显式 release 客户端回归；
- [ ] 将变化纳入统一制品重建及安装态验收；缺少真实云凭据不冒充生产账号验收。

实现已接通 QwenPaw UUID 下载与 owner/name archive、ModelScope 版本 archive、ClawHub detail → version → 有界多文件读取、Aliyun 签名 GetSkillContent。原 source_url 与 installed_from 写入现有结果/manifest，不把详情 HTML 当包。签名请求禁止重定向；单文件、包大小和任务总时长受限。ClawHub 缺失引用文件时整次安装失败，不把不完整技能标成成功；拒绝跨平台路径穿越和重复文件。取消覆盖等待响应头/读取文件期间，并在等待安装锁后再次确认取消，复用原扫描和文件提交。

已通过 10 个本机普通测试和显式原市场页 Chrome 门禁：分类浏览、输入搜索自动清空分类、第二页追加、详情 Save → 安装队列 Done → 原 Skills 页持久化展示；四来源工作区/池导入、版本、冲突、缺文件、危险路径和卡住下载时取消/重试另由 HTTP 测试验证。前端源码不变，测试先纠正了错误的“搜索保留分类”假设。验收步骤见 [技能市场验收](../testing/market-browser-acceptance.md)。清单更新为 370 调用、350 路由、40 未注册调用、2 占位；这些计数不是全功能完成度。真实云端可用性、其他 Hub 来源（如 GitHub/skills.sh）、生产凭据链与全部制品安装态仍需后续清单验收。

最终提交边界补充：新增确定性回归先检测到安装没有等待任务状态锁，修复后在持有状态锁期间确认磁盘尚未提交；释放后，排在提交后的取消请求必须返回 completed 与完整结果。工作区任务的实际文件/manifest 提交及完成状态发布现在持有同一任务锁，下载期间不持有该锁。成功提交不再被恢复协调器随后触发的 token 取消改写为 cancelled；元数据在私有 staging 中读取，避免复制成功后读取失败留下半完成目录。Aliyun 的 Authorization/STS 头标记 sensitive，单测确认诊断格式不包含测试凭据。

本切片最终本机验证：Rust 工作区 347/347 普通测试通过（App Server 171 个，另有 4 个默认 ignored 浏览器门禁全部显式通过）；原 24 页导航、两个 Backup 场景、OpenRouter 与市场分页安装均通过。四模块严格 all-targets/all-features Clippy、fmt/diff 与 inventory 校验、inventory 单测 3/3 通过。最新 release Core 重新构建，并被 TypeScript SDK 3/3、qwenpaw conda Python SDK 4/4、VS Code 57/57 显式连接验证，客户端编译也通过。`console/src` 零改动；此前 2453 个 Console 测试和 production build 的结果沿用同一源码，本轮未重跑该前端单测套件。9 个分发包仍是旧构建，本轮未重建、未提交推送；不宣称全部旧功能或跨平台安装态已完成。

#### 14.2.24.24 原模型提供商 OAuth 登录

核对原 `provider_oauth.py`、OAuth flow/session store 和 `OAuthConfirmModal`：现有提供商 OAuth 注册表只有 OpenRouter，不把其他提供商的 API key/CLI 登录虚构成同一能力。本组保留 start/status、回调页、原弹窗继续/等待/取消及授权后模型发现，打通原 Settings/Chat 共用入口。

按 [OpenRouter 官方 PKCE 协议](https://openrouter.ai/docs/guides/overview/auth/oauth)，Rust 后端生成随机会话和 S256 verifier/challenge，把 state 放入 callback_url，并进行单次有界 code 交换。拒绝无 state 时选“最新 pending”的旧回退，防止错绑会话；不变更前端代码或用户操作。回调地址只使用经过验证的本机 Host 或服务显式允许的 HTTPS origin，不信任任意代理头。Hub 托管回调转发仍需整体 Hub 信任边界验收，不能只凭一个请求头宣称已支持。

会话有 TTL、容量和终态；开始新一轮会使旧的尚未提交会话失效。网络阶段不持配置锁；提交前校验提供商配置版本和旧凭据指纹，防止登录期间的手工配置、清空 key 或恢复被旧回调覆盖。复用凭据/注册表写入路径与模型发现，秘密不写进普通 JSON、status 或 callback HTML。配置失败不能被标为 OAuth 完成；发现失败不撤销已成功保存的凭据。

- [x] 接通 OpenRouter start/status/callback 和真实 supports_oauth/oauth_connected 展示；
- [x] 实现 PKCE、会话绑定、重复回调、超时/容量、旧会话及配置变更保护；
- [x] 安全保存凭据、同步活动 Core 配置、自动发现模型及重启恢复；
- [x] 本机假授权服务验证成功、拒绝/异常、并发与持久化完整契约；
- [x] 用原 Chat FREE 入口、确认弹窗、外部授权跳转和轮询打开原模型管理，添加模型后新文档重载并返回 Chat；普通 key 替换/清空与重连均通过；
- [x] 更新架构/清单，运行全量与 release SDK/VS Code 回归；
- [ ] 纳入统一制品重建，验收真实账号、打包浏览器交接及 Hub 回调。

实现及验收记录见 [Provider OAuth 本机验收](../testing/provider-oauth-acceptance.md)。原 Models 页未配置的 OpenRouter 位于 Available Providers，只显示 Configure；不能通过伪造 key 或更改前端来制造不存在的连接入口。原 Chat 授权成功会导航到 `/models?provider=openrouter&manageModels=true`，测试依据该真实操作结果，不依赖会随页面卸载消失的 toast。

本切片回归：Rust 工作区 355/355 普通测试、5/5 显式原前端浏览器门禁通过；Console 295 个测试文件、2453/2453 测试通过。新编译的 release Core 被 TypeScript SDK 3/3、qwenpaw conda Python SDK 4/4 与 VS Code 57/57 测试实际连接，TypeScript SDK/VS Code 编译通过。严格 Clippy、格式/diff、inventory 3/3 与快照校验通过，`console/src` 零改动。清单为 370 调用、353 路由、38 未注册调用（含 11 个动态未解析调用）和 2 个占位；不是 38 个已证明相互独立的功能缺口，也不是全功能等价证明。9 个分发包仍需统一重建，不能把旧包标记成已包含本次实现。

#### 14.2.24.25 当前源码九类 QA 制品重建与逐包反向验证

沿用已批准的全客户端构建与原交互验收范围，本轮将最新本机验证源码打入实际分发包，不等待其他功能完成才更新体验包。使用独立的日期 QA 输出目录保留旧包；不修改原前端、不读取生产签名凭据、不上传发布。此阶段仍不是全部功能等价或正式签名发布。

- [x] 记录源码基线、dirty 状态、Core/Console 内容摘要，确认所有包来自当前工作树；
- [x] 构建 macOS arm64 Tauri App ZIP 与 QA DMG，验证 Rust-only 资源、签名结构、只读挂载及包内 Core/Console 内容；
- [ ] 通过解包后的原生 Core 执行、完整 Tauri GUI/桥接和签名/公证验收；
- [x] 构建 Core archive 与 WebUI archive；解包 WebUI 由源 release Core 提供服务，原 Models 页 Chrome 导航通过；
- [ ] 通过 Core archive 的实际执行和与各客户端组合的验收，不以源 Core 替代；
- [x] 构建两种 SDK 并临时安装，对可运行的源 release Core 验证；
- [ ] 两种 SDK 对包内 Core 的原生执行/握手回归通过；
- [x] 构建两种 VSIX，校验 manifest、入口、Core 摘要，独立扩展目录安装；包内协议客户端对源 Core 的 Thread CRUD 对照通过；
- [ ] VSIX 对本轮分发 Core 的实际启动验收通过；
- [x] 在隔离源码 staging 构建 legacy wheel，核对 CLI/TUI 入口、原 Console 资源，临时安装并通过 855 个 CLI/TUI 单测与 36 个 CLI 集成测试；
- [x] 对九个发布文件生成 SHA-256 与逐包验收记录，区分已验证、本机环境限制、待补齐功能，不重标记旧包。

结果见 [九类 QA 制品验收](../testing/qa-packages-20260909.md)。新输出目录为 `dist/qa-20260909-poJEFp/`，九个文件完整性通过，原前端 1311 个文件完全一致。原生执行门禁失败：解包 Core 被本机 macOS 签名校验终止，正式分发检查缺少公证票据。源码目录中的 Core 可执行，安装的 SDK 和 VSIX 客户端对源 Core 的对照通过，但这些不能覆盖包内 Core 失败。未降低系统安全策略，也未将本组或整体目标标记完成。

#### 14.2.24.26 Ollama 接口地址与模型检查语义

沿用已批准的原功能等价范围，对照原 `ollama_provider.py` 修复服务根地址与 OpenAI 兼容 `/v1` 地址之间的转换。仅改变 Rust 后端，原 Console 不动；不需要用户 key，也不启动或下载真实模型。这里不是所有 provider 协议或 Ollama 所有参数的完成声明。

- [x] 统一 Ollama 配置保存、临时连接/发现请求、模型选择、启动与恢复使用的地址规则；支持根地址、尾斜杠、已有 `/v1` 和反向代理前缀，其他 provider 不受影响；
- [x] 模型检查采用原版目录成员检查，返回 `provider_only`，不偷偷触发推理；覆盖模型缺失、远端错误；
- [x] 用隔离本地 HTTP fixture 验证原 API 配置/发现/选择后 Core 实际流式聊天、启动及恢复，不访问生产凭据；
- [x] Rust 工作区测试、严格 Clippy、格式和前端零差异检查通过，记录验证边界；
- [ ] 后续独立补齐 Anthropic/Responses/Gemini 等实际推理协议、运行时自定义 headers/生成参数及各 Agent 模型配置贯通；不得将设置接口通过当成运行时已支持。

本切片工作区 357/357 普通测试通过，现有 5/5 原前端浏览器门禁通过；最终对照原版目录错误处理后，新增 Ollama 2/2 定向测试再次通过。严格 Clippy、格式/diff、inventory 3/3 及快照校验通过，`console/src` 零改动。目录异常在模型检查中按原版折叠为 `model_not_found`；独立 provider 连接测试仍报告真实 HTTP 错误。完整边界见 [Ollama 地址验收](../testing/ollama-endpoint-acceptance.md)。未重新打包本切片，既有 QA 包不包含这次修复，原生分发签名门禁仍未关闭。

#### 14.2.24.27 模型请求参数贯通与原子配置切换

沿用已批准的原页面交互等价范围，补齐已有 provider headers、生成参数和单模型覆盖参数进入 Rust 推理层的链路，不改 Console、不把执行逻辑放入 SDK。按原 `Provider.get_effective_generate_kwargs` 递归合并；本切片面向现有 OpenAI 兼容推理，其他 native 协议及 Agent 级 provider 切换仍是独立未完成项。

- [x] Core 将地址/key 与请求选项作为同一快照读取/提交；校验失败或持久化失败不部分更新；
- [x] provider 默认参数与当前请求模型的参数递归合并，支持自定义 headers 和 `extra_body`，不允许参数覆盖 Core 的 model/messages/stream/tools 协议控制字段；
- [x] 原 provider/单模型配置、模型选择、启动、备份恢复均加载相同选项；SDK 直接改服务地址时清除旧服务选项，避免敏感 header 跨服务泄漏；
- [x] 本机 HTTP fixture 验证实际请求、更新/清空/切换、启动/恢复、无效配置与敏感信息边界；
- [x] 工作区测试、严格 Clippy 和现有原前端门禁通过；记录安装包与仍缺少协议的边界。

本切片最终工作区 362/362 普通测试、现有 5/5 原前端浏览器门禁通过。新增 5 个测试并扩展 Ollama 真实 HTTP 测试；严格 Clippy、格式/diff、inventory 3/3 与快照通过。release Core 重建后，TypeScript SDK 3/3、Python SDK 4/4、VS Code 57/57 测试实际连接新 Core，通过；TypeScript SDK 和扩展编译通过，`console/src` 零改动。详见 [请求选项验收](../testing/model-request-options-acceptance.md)。未重新构建九个分发包；原生包签名门禁、native 模型协议、Agent 级设置、跨进程 provider 配置一致性及模型删除后旧会话的设置语义仍需继续验收，不将整体目标标记完成。

#### 14.2.24.28 Anthropic 原生 Messages 推理

沿用已批准的模型运行时等价范围。按原 Anthropic provider 与官方 Messages/streaming 文档实现原生 transport，不用 OpenAI 格式模拟 Anthropic，不改 Console/SDK 协议。`chat_model` 和 `auth_mode` 进入上一切片的原子运行时配置。

- [x] 请求转换：system、user/assistant、tool_use/tool_result、工具 schema、max_tokens/thinking 参数；根地址与 `/v1` 地址及代理前缀；API key / auth_token 两种认证；
- [x] 流解析：文本、分片 JSON 工具参数、累计用量/缓存 token、message_stop、ping/未知事件、远端 error、断流、取消及有界资源；
- [x] 原生 content blocks（含 thinking/signature）持久化并用于后续工具步骤/重开会话；OpenAI 请求不泄漏 provider 私有字段；
- [x] 原模型 API 选择后，Core 实际完成本地 fixture 的工具调用往返，覆盖启动/恢复/认证模式及错误路径；
- [x] 工作区、Clippy、原前端门禁及 release Core/SDK 回归通过；真实账号、reasoning 展示和其他协议继续单独验收。

参考：[Anthropic streaming](https://platform.claude.com/docs/en/build-with-claude/streaming)、[Messages API](https://platform.claude.com/docs/en/api/messages/create)、[工具定义](https://platform.claude.com/docs/en/agents-and-tools/tool-use/define-tools)。原生 signed 内容块不可静默丢弃或截断；不能容纳时明确失败。此切片不代表所有 Claude 功能、所有原前端交互或所有分发包已完成。

本切片新增 9 个测试，工作区 371/371 普通测试和现有 5/5 原前端浏览器门禁通过；最后把 Anthropic HTTP fixture 的 thinking budget/max_tokens 调整为 1024/2048 后，专项测试再次通过。严格 Clippy、格式/diff、inventory 3/3 与快照校验通过。release Core 重建后，TypeScript SDK 3/3、Python SDK 4/4、VS Code 57/57 回归实际连接新 Core，通过；这些 SDK 回归仍采用原 OpenAI 兼容 fixture，不作为跨进程 Anthropic 配置已贯通的证据。`console/src` 零改动。详见 [Anthropic 原生验收](../testing/anthropic-runtime-acceptance.md)；本轮未重建九类 QA 分发包或完成 Anthropic 专项浏览器验收，整体目标保持进行中。

#### 14.2.24.29 Anthropic 原页面配置与聊天验收

沿用已批准的本机测试和原交互等价范围，在未修改的 Console 上操作原配置、添加模型、Chat 选择及发送入口。使用隔离 Chrome、本地原生 Messages fixture 和临时凭据，不需要真实 key。特别检查 Chat 的 Agent 级模型选择是否真正影响推理；若仅保存设置，修复后端请求级模型路由，不通过修改全局模型或前端绕过。

- [x] 添加真实浏览器门禁：原页面配置、连接检查、添加模型、Agent 级选择、发送、工具往返和刷新历史；
- [x] 根据实际失败修复必要后端链路，并验证 Agent 选择不污染全局或其他请求的 provider/key；
- [x] 修复专项测试发现的新 Agent 直接聊天缺少内置分组而返回 422；验收不依赖先打开分组页面；
- [x] 运行专项、工作区及现有浏览器回归，确认 `console/src` 零差异；
- [x] 更新验收记录与构建状态；真实账号、未覆盖交互及分发签名门禁继续保持未完成。

本切片新增 4 个普通测试和 1 个浏览器门禁，最终工作区 375/375 普通测试、6/6 显式原前端浏览器门禁通过。浏览器实际展开 Read File 输出并刷新历史，不以折叠摘要或单纯接口成功代替工具交互验收。并发 Agent 与全局回退、删除模型后的旧会话回退、运行中修改全局配置、无效 turn 参数不改历史均通过；全局回退直接采用 Core 当前快照，避免从旧 registry 恢复过时 URL/key。严格 Clippy、格式/diff、inventory 3/3 和快照校验通过。最新 release Core 重建后，TypeScript SDK 3/3、Python SDK 4/4、VS Code 57/57 实际连接回归通过，TypeScript SDK/扩展编译通过，`console/src` 零改动。详见 [Anthropic 原页面验收](../testing/anthropic-runtime-acceptance.md)。未重新构建九类 QA 包；本切片不代表其他 Agent 设置、跨进程配置同步、全部 provider 协议、真实账号或安装态已完成，整体目标保持进行中。

#### 14.2.24.30 最新模型运行时进入九类 QA 制品

沿用已批准的全部客户端构建与逐包测试要求，在新的隔离输出目录重建 §26–29 后的源码，包括 Ollama 地址、请求选项、Anthropic native 和 Agent 模型路由。保留旧 QA 包，不读取生产签名凭据或上传发布。只使用明确的 ad-hoc QA 签名，不降低本机安全策略。

- [x] 记录本次源码/前端基线，重建桌面 App/ZIP/DMG、Core、WebUI、两种 SDK、两种 VSIX 和 legacy wheel；
- [x] 解包并逐项比对原前端、Core 内容/签名、manifest 和 SHA-256；
- [x] 从实际解包/安装目录执行 Core、SDK、VSIX 客户端、WebUI 和 legacy CLI/TUI 检查；保留失败，不以源码对照替代分发包结果；
- [ ] 分发 Core 原生执行、完整桌面 GUI/桥接、正式签名/公证及全功能门禁通过；
- [x] 更新逐包结果和路径，区分“已构建”“检查已执行”和“通过”，不将未通过包标记可运行。

结果见 [最新模型运行时 QA 制品](../testing/qa-runtime-packages-20260909.md)。新目录 `dist/qa-runtime-20260908-yv7Wee/` 包含九个发布文件，全部 SHA-256 通过；四份 Console 的 1311 个文件与原 build 及上一组 QA 完全一致，`console/src` 零改动。已安装 legacy wheel 的 891 项 CLI/TUI 测试通过；安装的 SDK、VSIX 客户端与解包 WebUI 对源 Core 的对照通过。包内 Core 握手失败，Core archive 独立执行被系统 SIGKILL；此问题不因签名结构通过而关闭。完整 GUI/桥接未通过，没有使用生产私钥或降低系统安全。所有临时服务和 DMG 挂载已释放，整体目标仍在进行中。

#### 14.2.24.31 Gemini 原生 GenerateContent 推理

沿用已批准的原功能等价范围，对齐原 `GeminiProvider` 使用的 GenerateContent 协议，不迁移到另一个 Google API，也不改 Console 或把 Agent 执行放入 SDK。通过本地 HTTP fixture 验证，暂不需要真实 key。

- [x] 原生地址、API key header、contents/systemInstruction、工具声明与 functionCall/functionResponse 映射，保留代理前缀并正确处理模型资源名；
- [x] provider/单模型生成参数转换、工具 schema 兼容处理、thinking 参数与 thoughtSignature 原样保存/回传；
- [x] 有界 SSE 解析、用量累计、完成/截断/拒绝/错误/取消语义，不让未完成的工具参数触发执行；
- [x] 原 API/Agent 选择后实际工具往返、恢复及原页面聊天验收；工作区和原有协议回归通过；
- [x] 更新架构和验收记录，明确多模态/高级能力、真实账号和新版分发包尚需继续验收的范围。

参考 [GenerateContent API](https://ai.google.dev/api/generate-content) 与 [thought signatures](https://ai.google.dev/gemini-api/docs/generate-content/thought-signatures)。原版还具有图片/文档等功能，不能用本切片文本/工具链路通过代替其等价完成。

本切片新增 9 个普通测试和 1 个原页面浏览器门禁。最终工作区 384/384 普通测试、7/7 显式浏览器门禁通过，严格 Clippy、格式/diff、inventory 3/3 与快照通过。release Core 重建后，TypeScript SDK 3/3、Python SDK 4/4、VS Code 57/57 回归通过，SDK/扩展编译通过，`console/src` 零改动。原 Gemini 地址框仍保持锁定，测试仅在隔离后端预置 loopback 地址；Chat 测试使用原搜索框定位推荐范围外的模型。详见 [Gemini 原生验收](../testing/gemini-runtime-acceptance.md)。本轮未重建九类 QA 包；多模态/推理展示、Responses、其他 Agent/业务功能、真实账号、分发 Core 原生执行与安装态门禁仍未完成，整体目标保持进行中。

#### 14.2.24.32 Gemini 原多模态探测交互

沿用已批准的全部原功能范围，先关闭模型页 Gemini 探测的错误协议与行为差异。原 provider 图片使用红色 PNG inlineData，视频使用已有公共样例 fileData URL，独立执行两次 generateContent；不套用 OpenAI 的视频颜色探测或图片失败即跳过视频逻辑。测试远端只监听 loopback，不请求真实服务或下载公共视频。

- [x] Gemini 原生图片/视频请求、认证、回答判定与独立失败语义；有界响应和敏感信息保护；
- [x] 本地 HTTP 验证真实 payload、图片失败仍测视频、错误/无答案/拒绝、结果持久化与重复探测更新；
- [x] 原页面点击 Test Multimodal，检查原能力标签与刷新后的结果；不改 Console 源码；
- [x] 工作区、严格 Clippy、原浏览器门禁及必要 release/客户端回归；更新验收与制品边界。

聊天附件差距已定位：`desktop_files::console_user_input` 将 image/video/audio/file 块转换为 `UserInput::FileReference`，`compose_user_input` 只生成路径文本。目前上传/预览可用不代表原生多模态推理可用；后续必须补齐有界媒体读取、跨 provider 编码、历史和客户端契约，保持此功能未完成，不能通过显示能力标签关闭聊天验收。

本切片新增 2 个普通测试并扩展原 Gemini 浏览器场景，最终工作区 386/386 普通测试、7/7 显式原页面浏览器门禁通过。严格 Clippy、格式/diff、inventory 3/3 与快照通过；源码 release Core 重建后，TypeScript SDK 3/3、Python SDK 4/4、VS Code 57/57 回归通过，`console/src` 零改动。详见 [Gemini 原多模态探测验收](../testing/gemini-multimodal-probe-acceptance.md)。旧 QA 制品没有刷新，实际聊天媒体输入、其他未完成功能、原生分发执行和完整安装态仍保持未完成，整体目标继续进行。

#### 14.2.24.33 实际聊天图片输入与持久化

沿用已批准的原功能等价范围。新增显式 `UserInput.image { path }`，仅由 Core 读取工作区内的图片，普通 `fileReference` 仍只传路径。保持 App Protocol v3 的既有请求和纯文本历史形状；图片消息增加可选 `input` 元数据，不在事件/SDK Thread 响应塞入 base64。原 Console 上传入口不变，后端将 image 转为图片输入，历史接口从 Core 已保存的快照还原原 image 块。

小图片在发送时有界读取并与消息一同持久化，后续工具步骤、重启、checkpoint 使用同一快照。保留原默认每图片 2 MiB 内联上限；超大图片只向模型发送大小说明，Console 保留上传图片引用。最多 32 张/轮、快照总原始字节最多 16 MiB/轮；上下文保留独立 32 MiB 编码媒体预算，既有文本预算仍生效，不能截断 base64 或把图片当普通文字压缩。超过总预算明确拒绝，不静默少发图。可配置 provider 上限、音频/视频/文档、远程 URL 与其他协议仍单列未完成。

文件读取采用跨平台 capability 文件句柄及逐组件 no-follow，拒绝工作区外路径、符号链接、非普通文件和 File Guard 保护路径，不引入 Python 或 SDK 内核。使用 PNG/JPEG/GIF/WebP 魔数识别而非信任扩展名；不承诺完整图片解码验证。仅本地 fixture 网络验收，不使用真实 key。

- [x] 协议/生成 SDK 与兼容历史，存储有序文本/图片快照；
- [x] Core 安全有界读取、原子输入验证和上下文预算；
- [x] OpenAI Chat Completions、Anthropic Messages、Gemini GenerateContent 原生图片编码，工具往返保留顺序；
- [x] 原 Console 上传、真实模型 payload、历史重载、数据库重开和失败路径验收，前端源码零差异；
- [x] 工作区/严格 Clippy/浏览器及 release 客户端回归，更新架构和未完成边界；
- [ ] 纳入最新制品统一重建与安装态验收。

格式依据 [OpenAI Images and vision](https://developers.openai.com/api/docs/guides/images-vision)、[Anthropic Vision](https://platform.claude.com/docs/en/build-with-claude/vision)、[Gemini image understanding](https://ai.google.dev/gemini-api/docs/image-understanding)。

本切片新增 10 个 Rust 普通测试，最终工作区 396/396 普通测试、7/7 原页面浏览器门禁通过；Gemini 浏览器场景新增实际文件上传及刷新后图片解码。严格 Clippy、fmt/diff、inventory 3/3 和快照校验通过。最新 release Core 重建后，TypeScript SDK 4/4、qwenpaw conda Python SDK 5/5、VS Code 57/57 显式连接回归通过；两个新增 SDK 测试确实发送图片路径，并校验下一轮仍发送原快照，SDK/扩展编译通过。`console/src` 零改动。详见 [聊天图片输入验收](../testing/chat-image-input-acceptance.md)。九类 QA 包未重建；未覆盖的媒体/参数、其他业务功能、跨平台实机与原生分发门禁继续保持未完成，整体目标仍在进行中。

#### 14.2.24.34 最新图片运行时制品与执行门禁

沿用已批准的全部客户端构建/逐项验收范围，把 §31–33 的 Gemini 原生协议、探测和聊天图片快照打入新一组九类 QA 制品，保留所有旧包。当前 source release Core、SDK 和浏览器验收通过不替代包内运行。

- [x] 复测旧包原生失败与源 Core 对照；记录是否由测试主动终止，不从签名结构或单条系统日志推断根因；
- [x] 为解包后的各 Core 增加有界版本执行诊断，保存 exit code、signal、超时来源和持续时间，缺失/错误版本不得通过；
- [x] 新目录重建九类 QA 制品，保留原 Console；验证包内 SDK 的实际图片输入和快照复用，而非只检查类型文件；
- [x] 逐包解压、安装、摘要/签名与执行检查，失败与源码对照分开记录；
- [ ] 完整桌面 GUI/桥接、分发原生执行和正式签名/公证通过；此项不能因本机源码运行而勾选。

本轮旧 Core archive `--version` 复测 11.439 秒后 SIGKILL，测试的 45 秒主动超时未触发；源 Core 同时以 0 退出（222 毫秒）。两者 `codesign --verify --strict` 均通过。相关日志可见系统签名/来源检查及终端保护组件标签，但目前没有足够证据把 SIGKILL 归因为某个组件；不关闭保护、不清除来源/隔离标记，也不通过改变路径或名称隐藏程序。

最新目录 `dist/qa-runtime-20260908-fRyKH7/` 的九类制品已全部生成，SHA-256 校验通过。原 Console 1311 个文件与上一组完全相同。首次检查中 DMG/ZIP/平台 VSIX 内的 Core 已通过实际图片请求与重启历史，独立 Core archive 仍被 SIGKILL；同一文件、同一路径、没有重新签名或修改字节的后续复测通过。完整复测确认四处包内 Core、安装的两种 SDK、两类 VSIX 客户端均可执行，WebUI 也由解包 Core 而非源码 Core 托管并通过 Chrome 原 Models 页。legacy 安装态 855+36 项 CLI/TUI 测试通过。新增原生探测器 3/3 测试通过。首次失败与复测分开保存，不能把后者称作首次启动问题已修复。详见 [图片运行时 QA 制品](../testing/qa-image-packages-20260909.md)。完整 Tauri GUI/桥接、生产签名/公证、首次安装稳定性及其他原功能门禁仍未完成。

#### 14.2.24.35 桌面生命周期与原生验收前置检查

沿用已批准的全部功能与逐客户端验收范围。桌面启动会强制使用默认 app data，Core 的系统凭据服务也不随外部临时目录隔离；当前不直接启动完整窗口，避免访问日常数据/凭据，不以 Chrome 结果替代 WKWebView 验收，也不为测试关闭系统保护或把凭据改成明文。

只读检查发现：重启调用间没有互斥，两个 stop/start 可覆盖 child；退出与重启缺少终止优先约束；事件代次检查与状态修改分离，旧事件可能污染新进程；事件流关闭未清空旧端口。先在原桌面壳修复这些已确认的生命周期缺口，保持前端和正常启动参数不变。

- [x] 增加状态/事件回归测试，先确认旧实现的失效端口问题；
- [x] 串行化重启与退出，退出后拒绝新重启；代次与状态在同一锁内更新，终止事件清理端口；
- [x] 无法确认子进程终止时保持失败状态，不允许新启动掩盖旧进程；
- [x] 运行独立 Tauri crate 的测试、格式与编译检查；原 Console 零 diff；
- [x] 修复验收发现的 macOS release 测试 cfg 缺口，仅把测试所需 helper 解析/截断函数纳入 test 编译，不改变正式版本的 helper 实现；
- [x] 记录测试证据与未覆盖项；完整 GUI、安装态重建和冷启动稳定性继续保持未完成。

新增七个状态机回归测试，debug 与 release 全 targets 各 68/68 通过。旧端口问题已先复现失败；release 测试 cfg 缺口已修复。严格 Tauri Clippy 仍有四处现有 lint 未通过，未关闭或隐藏。`console/src` 零改动，旧 QA DMG 未包含本轮修改；本轮不把状态机测试当作完整窗口验收，未访问日常数据/凭据。详见 [桌面生命周期验收](../testing/desktop-lifecycle-acceptance.md)。

#### 14.2.24.36 Responses 原生运行时

沿用已批准的原功能等价范围。当前 `OpenAIResponseModel` 已能在原模型页面配置/探测，但 `provider_runtime` 仍落入 Chat Completions 默认分支；补齐 Rust 原生 Responses，而不是修改前端或把 SDK 变成 Agent 执行器。本轮使用 OpenAI Docs 核对 [函数调用](https://developers.openai.com/api/docs/guides/function-calling) 和 [Responses 协议](https://developers.openai.com/api/docs/guides/migrate-to-responses)，只请求本地模型 fixture，不使用真实 key。

- [x] 增加独立协议分支、原生 input/工具声明/结果、文本与图片编码、生成参数映射及原 Provider/Agent 选择贯通；
- [x] 有界 SSE 文本输出、完整工具调用、usage/cache、拒绝/错误/截断/取消；只有有效 completed 才提交工具执行；
- [x] 本地保存并回传原生 reasoning/output items，`store:false`，不使用远端 conversation/previous_response_id 替代 Rust 状态；不公开加密推理内容；
- [x] 本地 HTTP 实际工具往返、重开历史、失败不执行工具和原页面聊天验收；
- [x] 工作区/格式/严格 Clippy、Responses 原浏览器及 release/客户端回归，更新验收范围；
- [ ] 整组浏览器稳定性：首轮 7/8，备份 roundtrip 同一实现独立复测通过，仍需定位间歇性失败；不得将其标记已修复；
- [x] 将本轮与桌面生命周期修改纳入新版制品并执行安装态验收，首轮失败与复测分开记录（§37）；完整全功能、生产账号和跨平台门禁仍须完成。

新增六个普通测试与一个原页面浏览器场景。最终全工作区 402/402 普通测试通过；严格 Clippy、fmt/diff、inventory 3/3 和快照通过。release Core 已重建，TypeScript SDK 4/4、Python SDK 5/5、VS Code 57/57 显式连接回归及编译通过，`console/src` 零改动。首轮 MCP 取消超时及备份浏览器失败保留，二者独立复测通过，但未声称根因已修复。详见 [Responses 原生验收](../testing/responses-runtime-acceptance.md)。本轮未重建九类 QA 制品，完整桌面/跨平台安装态、推理展示和其他原业务功能继续保持未完成。

#### 14.2.24.37 Responses 与桌面生命周期制品更新

沿用已批准的全部制品构建/逐个验收范围，将 §35–36 纳入新目录的九类 macOS arm64 QA 制品。保留旧包；不使用真实模型 key、生产签名凭据或日常桌面数据。普通源码测试不替代包内执行，也不关闭此前的首次启动与间歇性测试问题。

- [x] 更新统一验收驱动，让 WebUI 明确由解包 Core 托管，不默认使用源码 Core；失败不回退成通过；
- [x] 新目录重建 DMG/ZIP、Core archive、WebUI、TS/Python SDK、两类 VSIX 和 legacy wheel，生成来源/摘要记录；
- [x] 逐包解压/安装、原前端与 Core 摘要比对、原生版本/SDK 图片与历史/VSIX 客户端/WebUI 实际运行、legacy CLI/TUI 回归；首轮失败和复测通过不混记；
- [x] 更新制品入口和失败/复测证据，释放临时服务及挂载；
- [ ] 完整原生窗口、冷启动稳定性、生产签名公证和各平台全功能安装态验收通过；本轮包结构检查不能代替这些门禁。

新目录 `dist/qa-runtime-20260909-OPX10W/` 九类 QA 制品已生成，摘要校验通过；原 Console 1311 文件及树摘要保持一致。首次 DMG 创建资源忙，同一命令复测成功并保留原日志。首轮安装态中 archive Core 版本探测 11.096 秒后 SIGKILL（非测试超时），Python SDK 两项启动失败；另外三处包内 Core 的图片/历史与平台客户端通过，WebUI 已由解包 Core 托管并通过原 Models 页。未修改字节/路径/信任设置的同路径完整复测，四处包内 Core、安装的 TS/Python SDK、两类包内 VSIX 客户端全部通过；legacy 安装态 855+36 测试通过。首次失败仍未解释或修复，完整原生 GUI 和全功能门禁继续未完成。临时服务及挂载已释放。详见 [Responses 与桌面生命周期 QA 制品](../testing/qa-responses-packages-20260909.md)。

#### 14.2.24.38 Cron 原生调度与执行等价

沿用已批准的原功能完整重写，不改 Console。首轮只读核对发现 Cron 仅持久化配置、手动 Console 文本投递；当时没有后台调度，Agent 运行返回 501。文本调度已在本节首个切片完成，Agent 仍须接通。具体设计和分步验收见 [Cron 原生运行时](../architecture/cron-runtime.md)。必须补全时间语义与真正执行，不能以 CRUD 或全绿页面导航替代。

- [x] 对齐原 3/4/5 字段、数字星期转名称、字段 AND、时区、一次性/按天重复与结束条件，非法计划在写入前拒绝；
- [x] 接入 App Server 生命周期，持久化下一时间槽、错过执行/合并、暂停/恢复/修改/删除与重启处理；当前 Console 文本调度恢复互斥与退出测试通过；
- [x] 手动及到期 Console 文本执行、Inbox、状态/历史正确，时间计算与真实后台执行测试通过；
- [ ] Agent 任务使用原 Provider/Agent/Workspace 与 Rust Turn、会话共享/独立、逐次工具安全策略、超时/取消、trace/Inbox 和静默投递；
  - [x] 先完成可信宿主逐 Turn 运行配置与安全隔离回归；不改变 SDK wire protocol，不通过临时全局 Off 实现后台任务；
  - [x] 默认 Console Agent 的持久化会话、异步执行声明/并发、超时退出与 trace；非默认 Agent/外部 Channel 和剩余控制语义仍由父项跟踪；
- [x] 原 Cron 页面默认 Console 文本与 Agent 的创建/编辑/启停/手动执行/状态历史/删除，含真正模型/工具往返，前端零改动；不代表其他 Agent/Channel 等价；
- [ ] 全工作区与 release/客户端回归，重建制品后安装态验收；所有外部 Channel 和多 Agent 调度隔离也必须覆盖，不能以默认 Console 替代全功能。

首个调度切片新增 12 个普通测试，Cron 13/13 普通测试通过；非法时区/字段已先复现原实现失败。最终全工作区 414/414 普通测试、9/9 原页面浏览器场景及一项原 APScheduler 3.11.3 差分（16 组时间规则）通过。Cron 原 UI 的创建、启停、手动运行、历史、编辑、刷新、删除通过，首次驱动文案/关闭图标错误与修正分别保留；没有修改 Console。严格 Clippy、fmt/diff、inventory 3/3 与快照通过。release Core 已重建，TS SDK 4/4、Python SDK 5/5、VS Code 57/57 及编译通过。九类 QA 制品未重建；Agent Cron 仍返回 501/到期 error，外部 Channel、多 Agent 隔离及全功能安装态继续未完成。详见 [Cron 运行时验收](../testing/cron-runtime-acceptance.md)。上轮间歇性问题不因本轮整组通过而关闭。

Agent 前置隔离切片：新增可信宿主 `start_turn_with_runtime`，逐 Turn 配置先校验再快照，普通启动入口也不再于异步执行时读取可变全局设置。新增 8 个 Core 单测与 1 个 MCP 集成测试，验证并行审批隔离、真实 Shell 超时、单次步数限制、取消清理、MCP Deny/禁用工具、非法参数原子拒绝及 wire 字段无法关闭审批。最终全工作区 423/423 普通测试、9/9 原页面浏览器场景及原调度差分通过（显式整组 174.71 秒）；严格 Clippy、fmt/diff、inventory 通过。新 release Core 与 TS SDK 4/4、Python SDK 5/5、VS Code 57/57 验证通过；前端零 diff，九类 QA 制品未重建。Agent Cron 仍未调用此入口，持久化会话/异步并发/trace 与原页面 Agent 验收继续未勾选。

默认 Console Agent 执行切片已接通上述可信入口，替代前两段历史记录中默认 Agent 固定 501 的状态；非默认 Agent 因尚缺持久化 Job 归属暂时返回 501，避免错误使用默认凭据/Workspace，不能将这一限制视为功能完成。新增 14 项 Agent 回归及 1 项 trace 容量测试，全工作区 438/438 普通测试与严格 App Server Clippy 通过；原 Agent Cron 页面独立验收通过（15.47 秒），`console/src` 零 diff。Inbox 关闭时明确归属的独立 Cron trace 也可范围备份/恢复。

本切片 release SHA-256 为 `586d41084b04d352bce99dec6c11ce900ec56279d7da87aee060a2126d2a6ebe`。整组显式测试首轮 10/11，OAuth 页面重载失败；独立复测 1/1（14.90 秒）、随后整组 11/11（188.46 秒）通过，不据此关闭根因调查。首次 DMG 创建资源忙，原命令重试成功并保留失败日志。

- [x] 默认 Console Agent 切片打入新九类 QA 制品 `dist/qa-runtime-20260909-WmuXFt/`，摘要/资源比对与逐包检查完成；首轮失败和复测分别保留；
- [x] 同路径复测四处包内 Core、安装的两种 SDK、两种包内 VSIX 客户端通过；解包 Core 托管原 Models 页面通过；legacy 安装态 855+36 项通过；
- [ ] 首次安装稳定性：独立 archive Core 仍发生非测试超时的 SIGKILL，Python SDK 两项启动失败；同文件同路径复测成功不是修复证明；
- [ ] 移除非默认 Agent 临时 501，完成 Job 归属、执行/生命周期隔离、持久化投递目标和范围备份；具体清单见 Cron 设计的下一切片。

原 Console 1311 文件在各包中完全一致；完整桌面 GUI/桥接、正式签名/公证、其他平台最新实机和其他原功能仍未完成。详见 [默认 Console Agent Cron QA 制品](../testing/qa-cron-packages-20260909.md)。不能将包内图片 smoke 冒称包内 Cron 全交互已运行，源码 Cron 原页面验收另行记录。

Cron 归属前置切片：以下是后续源码进展，**不包含在上述 WmuXFt QA 包内**。

- [x] 候选目标读取按 Agent 隔离的持久化聊天目录，保留完整 channel/user/session 元组、去重、顺序、过滤和 limit；重开不依赖 alias，不泄露内部 NUL 限定键。
- [x] 内部 owners 与持久化 v2 版本门禁；业务 meta 不授予归属；默认请求不可读取/修改/执行恢复得到的其他 Agent Job。
- [x] 实际 ZIP 备份与恢复按选中 Agent 筛选/合并 Job、状态、历史、游标和执行声明，冲突预检，未选中数据保留；孤立执行恢复不修改其他 Agent trace。
- [x] 448/448 工作区普通测试、严格 App Server Clippy；最新功能 release 与 TS SDK 4/4、Python SDK 5/5、VS Code 57/57 及编译通过。
- [ ] 原浏览器整组稳定性：两次均 10/11，第一次备份创建失败，第二次备份操作通过但 Inbox 导航出现 Agent 列表 fetch 错误；独立通过不等于根因修复。
- [ ] 非默认 Job HTTP 赋予归属与真实执行、审批/推送、关闭/删除生命周期、Agent Copy 的原生任务复制、外部 Channel 和新制品安装态仍待完成。

新增 10 项普通测试，增强原 Agent Cron 页面已知目标选择以及现有实际 ZIP 范围/回滚测试；没有修改 `console/src`。备份驱动改为等待原 Modal 默认名称初始化后再填入测试名称，独立两项备份浏览器通过（80.23 秒），第二次整组仍有不同失败。补充仅用于证据的请求/loader/JavaScript 上下文诊断，不调整错误判定；诊断单测 3/3 通过，接入后独立原备份 roundtrip 1/1 通过（66.43 秒），此次未复现故障，根因继续未定位。详见 [Cron 运行时验收](../testing/cron-runtime-acceptance.md) 的归属前置切片。本轮不提交/推送、不使用生产 key，不将已有包当作当前新源码安装态。

#### 14.2.24.39 Console 审批持久归属与原 Inbox 交互

沿用已批准的功能完整重写。核对原 Python 与 Console 后修正审批范围设计：原 Inbox 全局汇总、跨 Agent 操作不切换 Agent，不能按当前 Agent header 拦截；实际请求归属由持久化聊天和审批 ID/根会话决定，不改前端调用。

- [x] 先复现 default/Writer 同名会话重开后的错误 Agent/session/root、跨 Agent 直接线程访问、目录损坏仍发布审批的问题；
- [x] 使用持久化身份，错误根会话不消费请求，准确响应一个 ID；未知/损坏目录拒绝工具，不冒充默认 Agent；
- [x] Console 已有线程在修改 Workspace/执行前检查归属，包括 alias 路径；无目录 SDK 线程仅属 default；
- [x] `/api/approval/list` 原形状及根会话过滤，全局 push 维持原语义；
- [x] 原 Inbox 重载与不切换 Agent 的批准/拒绝，真实 write_file 副作用和 SSE 结束验收；452/452 普通测试、严格 Clippy、release/SDK/VS Code 回归通过；
- [x] 整组显式回归 12/12 通过（203.21 秒），含 11 项原页面场景与原时间差分；保留此前间歇性失败，不能据本次通过关闭根因；
- [ ] 非默认 Cron 执行/生命周期、任务复制、审批持久规则和子 Agent 委派继续补齐，更新制品后逐客户端安装态验收。

新增四项普通测试及一项原 Inbox 浏览器，定向联合 5/5 通过（12.27 秒）。新 Core release、TS SDK 4/4、Python SDK 5/5、VS Code 57/57 及编译通过。`console/src` 零改动；当前九类旧 QA 制品不包含本切片。初始测试编译问题、旧实现实际失败、lint/fmt 修正均分别记录，不缩减原功能目标。详见 [Console 审批验收](../testing/console-approval-acceptance.md)。本轮未提交/推送、未使用生产 key 或日常凭据。

#### 14.2.24.40 Cron 公开任务 ID 与内部键分离

沿用完整原功能等价目标。原 Agent Copy 原样复制 jobs.json，保留任务 ID/规格/启用状态且不复制 jobs_history；因此不能用“全局唯一公开 ID”限制替代原每 Agent 命名空间。本切片落实原生复制前提，保持已有 Job API 形状和原前端。

- [x] v3 public_ids 映射与逐 Agent 唯一性，v1/v2 原生数据无损可读、旧 Core 版本门禁，Job 请求不可伪造内部映射；
- [x] 默认 HTTP 边界按公开 ID 查找，状态/历史/并发/删除沿用唯一内部键；trace/Inbox/独立会话保持公开 ID；
- [x] 范围备份保留映射；恢复重分配冲突内部键及关联记录，不改未选中数据和公开身份，run ID 冲突仍失败；
- [x] 六项新增普通测试、实际 ZIP 范围与联合回滚增强、原映射 Cron 页面独立控制/真实工具/刷新/删除通过；458/458 工作区普通测试与严格 Clippy 通过；
- [x] 新 Core release、TS SDK 4/4、Python SDK 5/5、VS Code 57/57 及编译通过；前端源码零 diff；
- [x] 最终整组显式回归 13/13 通过（215.27 秒，12 项原页面场景与时间差分），不据本次通过关闭此前间歇性失败；
- [ ] Agent Copy 接入原生任务与联合失败回滚；非默认 HTTP、执行/生命周期、外部渠道和新制品安装态仍待完成。

四项存储负例先在旧实现真实失败；随后实现，新增原页面映射任务场景独立 1/1 通过（13.91 秒）。原“跨 Agent Job 键一律冲突”测试改为完整结构重键验证，保留 run ID 冲突拒绝；这不是改公开 ID 或复制历史来绕过原功能。实际注册 Writer 的命名空间 HTTP 复测通过。详见 [Cron 命名空间验收](../testing/cron-namespace-acceptance.md)。旧 QA DMG 不包含本切片，未提交/推送或使用生产 key。

#### 14.2.24.41 Agent Copy 接入原生任务

延续原功能等价方案，复制原生 SQLite 中的任务规格，不读旧 jobs.json。原复制弹窗、默认选项和响应字段不变，非默认运行/生命周期仍须后续接通。

- [x] 旧入口真实失败回归；原生复制保留公开任务 ID、完整规格和 enabled，生成新内部键，不复制运行状态、游标、历史或声明；
- [x] 复制前验证、Cron → Agent 锁顺序、普通失败联合回滚；真实 SQLite/索引/凭据故障注入验证，回滚失败保留恢复资源并明确报错；
- [x] 连续复制 ID 改取 UUID 随机尾部，避免时间戳前缀碰撞造成循环；连续三次复制回归；
- [x] 十项新增后端测试和原复制弹窗独立验收通过：默认不勾选、勾选任务、刷新和 Core 重开，活动任务不重复/中断，前端源码未改；
- [x] 最终工作区 468/468、整组显式 14/14、严格 Clippy、release、TS SDK 4/4、Python SDK 5/5、VS Code 57/57 与编译通过，验收记录已更新；
- [ ] 非默认 Job HTTP、执行/生命周期、外部渠道及最新九类制品安装态验收。

普通故障回滚不等于 SQLite、文件与凭据跨存储崩溃原子性。复制保存 enabled 并不表示尚受门禁限制的非默认任务会运行，仍不能交付为完整等价版。详见 [Agent Copy 验收](../testing/agent-copy-acceptance.md)。

#### 14.2.24.42 Cron 实际 Agent 执行与关闭排空

继续完整原功能等价目标，不把内部执行验证等同于非默认公开调度已开放。删除保留 Workspace 文件，重新注册身份与启用游标仍需接通，当前 501 和后台门禁保持。

- [x] 服务端 Job owners 固定 live lease 的 Agent，模型/运行限制/Workspace/会话/trace/Inbox 归属一致；同名任务和会话并行真实工具及全局审批隔离；
- [x] 原关闭/删除入口捕获、取消并排空对应 Agent 的运行/排队任务，其他 Agent 不受影响，不在持 Cron/Agent 锁时等待 finish；完成令牌隔离随后启动的新 run；
- [x] 原 Agent 页面禁用 Writer、删除 Editor、当前选择返回 default、刷新与默认审批保留；没有改 console/src；
- [x] 两项旧实现真实失败：Writer 错用默认模型、缺失 Provider 静默落到默认模型。模型从一次 Agent 配置快照解析；
- [x] 核对原 model_factory、Anthropic/OpenAI Provider 后纠正旧 Rust 测试：目录删除不清除显式模型；保留原 Provider/model/凭据和显示。空 slot 回到全局模型，Provider 缺失报错；
- [x] 最终普通 480/480、显式整组 15/15（249.83 秒）、严格 Clippy、release、TS SDK 4/4、Python SDK 5/5、VS Code 57/57 与编译通过，验收记录已更新；
- [ ] 重新启用的游标、关闭/删除联合失败与重注册同 ID/不同 Workspace 隔离、非默认 Job HTTP/后台正式开放、外部投递及九类新制品。

详见 [Cron 执行归属验收](../testing/cron-executor-scope-acceptance.md)。前置整组暴露的模型回退错误按原源码纠正，不能只改测试为通过；同样不能把还未完成的生命周期称为全部功能完成。

#### 14.2.24.43 Agent 重新启用与生命周期串行化

沿用全部原功能等价目标。原 CronManager 重建时清空最近状态并重新注册 trigger，但历史另存不清空；一次性过期时间不移到未来，继续由 misfire 规则处理。实现方案及锁顺序见 [Cron 架构图](../architecture/cron-runtime.md)。

- [x] 仅 false → true 重建实际 Agent 的游标与最近状态；规格、公开 ID、历史和其他 Agent 数据保持，重复启用及无任务不写 Cron；
- [x] 固定时间覆盖 Cron、按天重复、过期一次性、停用和无效任务；非法零间隔/未知类型不能造成 panic，启动禁用不伪造执行历史；
- [x] 未收束声明拒绝重新启用；Lifecycle → Cron → Agent 锁顺序，排空仅保留 Lifecycle，创建/复制/启停/删除串行；
- [x] 复用原 HTTP 中间件独立处理和 Core operation guard，真实任务暂缓 finish 时取消调用方，验证重新启用仍等待旧任务结束；
- [x] SQLite 写入失败不改索引；索引发布失败还原 Cron 原字节；回滚失败保留 disabled 并明确要求恢复；
- [x] 原 Agent 页面关闭、重新启用、删除另一 Agent、刷新及默认待审批保留，console/src 零修改；
- [x] 普通 488/488、显式整组 15/15（245.95 秒）、严格 Clippy、release、TS SDK 4/4、Python SDK 5/5、VS Code 57/57 及编译通过，详见 [重新启用验收](../testing/agent-restart-acceptance.md)；旧 QA 包未重建；
- [ ] 删除后同 ID/同 Workspace 与同 ID/其他 Workspace 的归属语义、非默认 Job HTTP/后台开放和完整各端制品仍继续，不能据此宣布全部功能完成。

#### 14.2.24.44 保留 Workspace 的清理安全与本机 QA 重建

重新注册只读审查发现自动路径的布尔值表示“默认位置”，却被当成“本次新建”用于递归清理。创建失败因此可能删除上次 Agent 留下的目录或已有链接目标。这是重新注册数据保留的前置修复，不代表 Workspace/Agent 数据身份已经分离。

- [x] 实际 HTTP 失败注入复现两项数据消失；自动目录以独占 create_dir 是否成功授予清理所有权，不使用 exists 后 mkdir 的竞态判断；
- [x] 新自动目录失败仍清理，原自动目录、自定义目录、已有链接目标失败保留；4/4 定向通过；链接回归在 Unix 运行，未声称 Windows 实机覆盖；
- [x] 核对原工作区聊天文件与全局 Inbox 历史的不同范围，继续完善统一绑定，而不是重写所有历史 Agent ID；
- [x] 普通 492/492、显式整组 15/15（245.08 秒）、严格 Clippy、release、TS/Python SDK 和 VS Code 编译测试通过，见 [Workspace 保留验收](../testing/workspace-retention-acceptance.md)；
- [ ] 使用新的独立 dist/qa-runtime 目录重建本机九类 QA 制品，保留旧包；逐项解包/校验/隔离运行、SDK 安装态与 VSIX 安装、WebUI 原页面检查；
  - [x] 首批九种包构建完成，DMG 资源忙后单次恢复；保留首次失败日志。
  - [ ] 首轮解包 Core 的 Python SDK 两项错误，独立 --version 探针收到非探针触发的 SIGKILL；源码对照、桌面 ZIP/DMG Core、平台 VSIX Core 与后续 WebUI 检查成功，不能据后续通过关闭首次启动失败。
  - [x] 新目录 raw/显式临时签名的首次 SDK 与 --version 四项均失败；仓库内路径和不携带扩展属性的新副本也失败。保留所有失败，不依据签名/复制属性的猜测修改 staging；源码二进制、原候选和系统安全策略不改。
  - [ ] 继续关联具体子进程 PID/终止事件，定位首次启动 SIGKILL 后修复并重建独立候选；当前 24 个验收阶段仅 22 个成功，不能把本批九类包称作全部通过。
    - [x] 原生探针补 PID/起止时间并通过 3/3 单测；实际 tar 进程 PID 30275 与字节副本 PID 29742 被阿里终端防护标记非白名单，正常源码控制组 PID 30133 无该标记，核心字节相同。详见 [策略核查材料](../testing/core-startup-policy-investigation.md)。
    - [ ] 请终端防护管理员确认处置原因并按策略授权 QA 测试；尚无直接发送 SIGKILL 的处置记录，不把关联当作根因。系统防护不改，原候选失败记录保留，收到确认后重新验收首次启动与 SDK。
- [ ] Windows/Linux 原生与完整桌面窗口验收、完整功能等价及生产发布仍按总目标继续；QA 包不是上述门禁全部通过。

#### 14.2.24.45 Workspace 数据身份基础

在安装态等待终端防护策略核查期间，继续不依赖该授权的源码工作。设计和架构图见 [Workspace 数据身份](../architecture/workspace-data-identity.md)。本节是注册层基础，不是任务/聊天重新绑定完成。

- [x] 原生注册表 v2 增加类型化 Workspace 标识和保留目录索引；对外 Agent ID 和原 API 响应不改，旧 Core 拒绝新版本注册表。
- [x] 原生 v1 活跃注册项确定性补足历史命名空间，删除后新目录不因复用 Agent ID 获得旧标识；不导入 Python 数据。
- [x] 创建/复制/恢复规划接入本机标识，目录注册入口以实际独占创建结果区分新目录；保留自动目录的失败清理修复。
- [x] 目录代际校验在 §46 继续补齐；运行消费者接通仍独立验收，不能据此宣布全部数据归属完成。
- [x] 当前源码普通 **497/497**、显式浏览器/参考组 **15/15**（248.49 秒）、严格 Clippy/fmt/diff、release、TS/Python SDK **4/4、5/5** 和 VS Code **57/57** 与编译通过，见 [注册层基础验收](../testing/workspace-identity-foundation-acceptance.md)；制品未因注册层基础变更重建。
- [ ] Cron、聊天/分组/审批、运行结束与范围备份消费者仍需接通统一身份；非默认 Cron 门禁保持，不宣称已解决端到端串用或全部功能完成。

#### 14.2.24.46 Workspace 目录代际与隐藏元数据

沿用已确认的原交互目标，实施细节见 [目录代际方案和架构图](../architecture/workspace-data-identity.md)。

- [x] 注册表 v3；原生 v1/v2 启动建立一次目录标识基线，后续不因标识丢失自动补足。创建发布之前写入新目录代际，失败保留的新自定义目录重试不接回旧索引。
- [x] 无原标识的目录替换、失败重试、编辑/重开、新旧格式、损坏/超限/链接、复制与原响应结构回归；标识不是同权限文件系统攻击的授权边界。
- [x] 恢复保持原整目录/受保护子项交换算法，将本机标识作为内部暂存内容写入同一文件事务；保留未选中标识的原字节，归档标识拒绝覆盖本机绑定。
- [x] 标识不进入原文件列表、下载、监视通知、检查点和 Workspace 备份；直接文件路径/上传不能改写它。Git 保留原 info/exclude 规则并排除标识，真实暂存全部/清理回归通过。
- [x] 最终普通 **507/507**、严格检查、release、TS/Python SDK **4/4、5/5** 和 VS Code **57/57** 与编译通过，见 [目录代际验收与失败记录](../testing/workspace-generation-acceptance.md)。九类包没有因本次改动重建。
- [ ] 原页面曾整组 **14/15**：Anthropic 输入未发出；追加诊断后后续独立完整组 **15/15**（246.43 秒）不关闭偶发失败。继续核对焦点、状态同步和 IME 事件，不改原前端或自动重发绕过。
  - [x] 成功/失败均保留有界输入事件，成功样本 Enter 时字数已同步、无 composition 事件。短暂并列发送实验未复现，已恢复原顺序输入；仍只发一次 Enter，不以猜测加等待。缺少失败现场事件，根因继续待查。
- [ ] 同轮曾有运行时/ACP、Chrome、Core initialize 启动延迟，已保留失败与前后进程快照/独立探针；未改超时/系统策略的普通和客户端复验通过，也不关闭启动可靠性或旧候选首次 SIGKILL。
- [ ] 下一步继续将 Cron、聊天/分组/审批/alias、运行完成与范围备份接到数据标识；非默认 Cron 门禁保持。安装态首次启动仍等待终端防护侧核查。

#### 14.2.24.47 运行入口的 Workspace 绑定快照

落实已批准的统一消费者顺序第一步：在 Agent 注册锁内一次解析当前公开 ID、数据标识、基础 Workspace 和配置，并核对规范路径索引与目录标识。已有 Workspace/config/model/project 查询共用该入口；项目目录不要求 Core 标识，也不能代替基础 Workspace 的身份。注册管理入口仍可列出/删除失效注册，避免无法恢复。

- [x] 两项实际旧实现失败：已注册目录替换/标识缺失时文件接口仍返回 200。修复后不自动修复或重绑，保留目录和注册表原字节。
- [x] 实现内部快照及绑定校验，保持公开成功响应和原前端不变；项目选择从同一份配置读取。
- [x] 新增 5 项覆盖正常读取、目录换回恢复、其他 Agent 不受影响、项目目录、禁用/缺失 Agent、非法标识和符号链接替换；普通 **512/512**、严格 Clippy/fmt/diff 通过，见 [运行入口验收](../testing/workspace-runtime-binding-acceptance.md)。
- [x] 最新原页面完整组 **15/15**（245.37 秒）、release、TS/Python SDK **4/4、5/5** 和 VS Code 编译与 **57/57** 通过；前端源码零 diff。
- [ ] Cron/聊天持久化归属及范围备份消费者继续接入；快照不等于全链路完成。九类包未重建，旧制品首次启动与偶发输入失败仍按各自记录待查。

#### 14.2.24.48 Cron 持久化 Workspace 归属

延续 §47 和统一消费者方案，不改变原 Job 响应字段或前端。原始 Agent 字符串只保留为历史来源，Job 内部键对应的类型化 Workspace 标识成为数据归属依据。

- [x] Cron v4 记录每项任务的 Workspace 标识，公开任务 ID 在该标识内唯一；旧格式只接回可证明的历史归属，不凭当前同名 UUID 注册猜测孤立任务。
- [x] 创建、列表/查询、复制、重新启用、默认调度选择和运行完成统一使用数据标识；运行声明/LiveLease 保留数据标识与发生时 Agent ID，既有 Inbox 历史不重写。
- [x] Agent 归档快照 v2 携带来源绑定，全局公共引用仍为 v1；恢复映射本机目标，未选择任务/状态/历史保持，重键和回滚覆盖新增归属字段。保护未选中 Inbox run/trace ID，防止恢复声明重开后改到其他记录。
- [x] 新增 8 项覆盖原目录换 ID、同 ID 换目录、旧原生格式依据、重复公开任务 ID、复制/重开、实际运行和跨 Core HTTP 范围恢复；普通 **520/520** 通过，见 [Cron 数据归属验收](../testing/cron-workspace-ownership-acceptance.md)。
- [x] 最新原页面完整组 **15/15**（242.93 秒）、严格 Clippy/fmt/diff、release、TS/Python SDK **4/4、5/5** 和 VS Code 编译与 **57/57** 通过；前端源码零 diff。九类安装包没有因本次变更重建。
- [ ] 聊天/分组/alias/检查点及相关恢复消费者继续接入，非默认公开 Cron 门禁不提前解除；安装态与完整各端原功能仍须验收。

#### 14.2.24.49 聊天持久化归属与统一访问

延续已批准的 Workspace 消费者方案，保持原 Chat/Group 成功响应、原全局 Inbox 审批交互和前端源码不变。

- [x] ChatCatalog v2 为聊天和分组记录类型化 Workspace 归属；旧原生默认与历史命名空间有明确兼容依据，不能凭当前同名注册或项目路径认领数据。
- [x] 聊天/分组 CRUD、列表、直接 Thread ID、会话别名、停止、文件项目/工具控制和 Cron 会话共用归属校验；持久会话在重启与原目录换 ID 后可恢复，旧名称的新目录无法访问。新 Thread 与目录发布持有同一聊天锁，失败先回滚 Thread，不发布别名。
- [x] 审批解析实际当前注册但保留已发布历史身份；检查点会话筛选与范围备份/恢复接通来源/目标绑定，未选择数据保持。
  - [x] 当前审批解析、检查点会话筛选、聊天/分组/Thread 范围导出和来源/目标恢复映射已接通；真实跨 Core 恢复保留源与未选择数据。普通 Console 运行跨关闭/删除的固定身份与取消已补充 §50 本机验证，不代替检查点完整存储归属或全端安装态。
- [x] 新增实际生命周期、别名重开、跨 Agent 停止/访问、旧格式、分组及范围恢复回归；完整工作区、原页面、严格检查、release 与客户端依次验证。
  - [x] 新增 8 项，普通 **528/528**、原页面 **16/16**、严格 Clippy/fmt/diff、release、TS **4/4**、Python **5/5**、VS Code **57/57** 通过，见 [聊天归属验收](../testing/chat-workspace-ownership-acceptance.md)；普通运行跨关闭/删除的生命周期另行完成，历史浏览器偶发问题仍未关闭。
- [ ] 使用量、检查点目录代际及其余消费者/非默认 Cron 公开调度和所有端安装态仍须独立完成；本节不提前宣布全功能完成。

#### 14.2.24.50 普通 Console 运行生命周期

延续已批准的 Workspace 消费者与生命周期方案，不改变原聊天 SSE、审批控件或前端源码。关闭/删除的成功响应必须晚于已取消运行退出；不将这一点扩大为整个产品的完成声明。

- [x] 在 Agent 生命周期锁内接收聊天并登记运行，固定经过校验的 Agent/Workspace、模型与运行配置；禁止关闭/删除发布后再接收旧身份运行。代码已接入，完整验收仍以下一项为准。
- [x] 普通运行登记取消和完成信号；关闭/删除取消该 Agent 的运行，释放完成所需的状态锁后等待退出。不因 SSE 客户端断开或发送背压而阻塞取消/排空。
- [x] 审批使用本次运行固定身份，完成及中断清理待审批；保留原 SSE 终态和默认/其他 Agent 运行。
- [x] 新增真实待审批与模型等待期间关闭/删除、隔离与重建回归，验证终态、pending 清理、后续运行拒绝与另一 Agent 不变；严格检查、完整工作区、原页面、release/客户端逐项复验。
  - [x] 前 6 项新增与已有定向测试 **13/13**（0.38 秒）通过。
  - [x] 新增缺少运行配置回退与第 17 项显式场景后，完整编译曾遇到磁盘空间不足；用户确认清理后已仅删除 `target/debug/incremental`，可用空间恢复约 65 GiB。普通 **535/535**、严格检查、16 项原页面、调度参考补跑 **1/1**、release、TS **4/4**、Python **5/5**、VS Code **57/57** 通过，见 [普通 Console 生命周期验收](../testing/console-lifecycle-acceptance.md)。首次显式组因缺少 conda Python 为 16/17 的记录保留，不改写为一次整组成功。
  - [ ] 使用量/ACL、检查点存储代际、其余原产品功能和各端最新安装态继续完成；本节本机验证不解除非默认 Cron 门禁，也不代表整体目标完成。

#### 14.2.24.51 使用量归属与统计范围

延续已批准的 Workspace 消费者方案。原 `token_usage.py` 的两项用量接口和 `agent_stats.py` 的趋势接口为全局；Agent 统计汇总以请求 Workspace 选择聊天，但 Token overlay 仍全局。保持这些范围、字段与原页面不变，不把全局可见历史误当作跨 Workspace 授权漏洞。

- [x] 将现有类型化 Workspace 标识下沉为存储/Core 共用内部类型，保持既有标识 JSON 与注册校验规则。为使用量账本记录明确归属；新存储行和逻辑备份有可拒绝旧读取器的格式版本，旧原生无归属记录保留历史命名空间，不猜测同名 UUID。
- [x] Core 提供仅可信宿主可设置的 Turn 用量身份，在启动时固定到活跃 Turn，模型调用与账本同一事务写入；Console、Cron 与默认 Heartbeat 从验证过的注册快照传入。原 App Protocol/SDK wire 不新增内部授权字段，普通 Core 调用保持默认身份。
- [x] 保持全局 Token 汇总/详情与全局趋势；Agent 汇总只使用当前 Workspace 聊天，保留原全局 Token overlay。删除 Thread 不删除账本，删除/同名新目录/原目录换 ID 不改写事件发生时 Agent 标签。
- [x] 范围导出/恢复按固定数据标识筛选并显式重映射来源到目标；保留未选择账本完整结构，拒绝格式混用、无效绑定和未选择记录 ID 冲突。
- [x] 使用本机模型协议模拟 usage 验证并发归属、后续调用与重开、删除聊天、重新注册与原统计范围；补充存储版本/失败回滚及跨 Core 范围恢复测试，再顺序执行普通、严格检查、原页面、release 与客户端验证。没有使用真实服务 key，不声称生产服务已验收。
  - [x] 本机模型协议模拟、新增 9 项普通测试及扩展跨 Core 恢复断言通过，普通 **544/544**、严格 Clippy/fmt/diff 通过；见 [验收与失败记录](../testing/usage-workspace-ownership-acceptance.md)。页面/release/客户端仍以下续记录为准。
  - [x] 原页面与调度参考整组 **17/17**（270.66 秒）、release、TS **4/4**、Python **5/5**、VS Code 编译与 **57/57** 依次通过；原前端源码零 diff。
  - [ ] 用最新源码重建九类 macOS QA 制品，并逐项做解包/安装态验证；保留旧包与失败记录，不将 macOS 本机结果当作 Windows/Linux 或完整 GUI 等价证据。
    - [x] 九类 macOS QA 文件生成、校验和/原前端 1311 个文件一致；首次分发组 **22/24**，通过数不能覆盖独立 tar 的 SIGKILL。旧脚本的底层启动探测已改为逐项串行，并验证时间戳无重叠；同一批字节/路径和超时下的串行复验及 Python SDK **5/5** 通过，不改签名/隔离属性、不覆盖首次启动故障。详见 [最新制品与边界](../testing/qa-usage-packages-20260909.md)。
- [ ] ACL、检查点目录代际、其余原产品功能和全部客户端安装态独立完成，非默认公开 Cron 门禁不提前解除。

#### 14.2.24.52 邮件 ACL 原权限命名空间

实际原代码与临时存储实验确认：邮件 ACL 在 Workspace 内还按 Agent ID 分组，不能照搬聊天的换名继承语义。沿用原交互与权限边界，实施方案及 Checklist 见 [邮件访问控制身份](../architecture/mail-access-control-identity.md)。

- [x] 原路由/存储检查与三项完整结构参考实验：原目录原 ID 保留，原目录新 ID 和新目录原 ID 均为空。
  - [x] 原邮件路由与存储单测 **39/39**（0.86 秒）通过，作为下一步 Rust 等价验收的参考，前端源码未改。
- [x] 完成邮件 v2、注册/开关/广播/直接修改、范围备份及回滚和本节列出的回归；真实监听/replay 消费及全产品安装态仍未完成，不提前宣布邮件全功能完成。
  - [x] v2、原可见/直接修改契约、命名空间和完整 Workspace 恢复回归通过；新增 9 项普通测试与真实跨 Core HTTP 断言，完整普通 **553/553**、严格 Clippy/fmt/diff 通过，前端源码未改。见 [验收与失败记录](../testing/mail-workspace-ownership-acceptance.md)；真实邮件处理、页面专项、并发/失败回滚补充与新制品仍未完成。
  - [x] 现有原页面/调度参考完整组 **17/17**（269.21 秒）、最新 release、TS **4/4**、Python **5/5** 和 VS Code 编译与 **57/57** 依次通过。该页面组没有邮件抽屉专项 CRUD；§51 九类制品仍不含当前修改。
  - [x] 后续补齐邮件抽屉真实操作、目录标识替换批量预检、写入/删除竞争和双主体邮件恢复失败回滚：普通 **555/555**，新增邮件页面单跑 **1/1**，最终显式完整组 **18/18**（282.39 秒），严格检查通过。源码仍零前端组件修改；正在新目录重建九类 QA 制品，不能沿用 §51 包宣称含本次修复。
  - [ ] 九类新 QA 文件已生成并通过校验/原前端 1311 文件一致性，但首次安装态 **21/24**：tar/ZIP/DMG/平台 VSIX Core 在串行探测中均外部 SIGKILL，安装 SDK 及 WebUI 相应门禁未通过。源 release 对照正常；四个失败 PID 有终端防护非白名单关联，发送方尚未证实，需要管理员核查。原文件/失败记录保留，不改签名或安全策略，见 [本轮制品与启动证据](../testing/qa-mail-packages-20260909.md)。
- [ ] 桌面完整 GUI 需要独立账号/虚拟机和原生窗口测试手段；当前壳强制系统 App 数据目录，不能用外部 `QWENPAW_HOME` 隔离。此次未启动日常 App、读取真实 key 或修改安全策略。
- [ ] 桌面独立 crate 严格 Clippy 再次确认仍有 §35 的四处现有 lint；Core 工作区严格检查通过不代表独立桌面 crate 已通过。

#### 14.2.24.53 检查点基础 Workspace 与数据代际

承接统一消费者计划。原检查点按请求 Agent 的基础 Workspace 存储，不随 Files 项目选择改变；当前 Rust 的全局选中路径、default 会话解析和路径哈希归属需要协调修正。设计及原证据见 [检查点 Workspace 身份](../architecture/checkpoint-workspace-identity.md)。

- [x] 原实现临时目录/模拟请求完整 status 比较：共享项目隔离、项目切换稳定、基础目录换 Agent 名称及缓存重开保留、新目录不继承。
- [x] 原检查点路由与存储/恢复/运行时测试基线 **91 通过、1 个 Windows junction 专项跳过**（13.28 秒），不是跨平台全通过。
- [x] 现有 Rust 检查点定向 **10/10**（库 9 项 0.54 秒，HTTP 1 项 0.29 秒）；当前测试没有覆盖本节新发现的基础/项目分离和请求 Agent 范围，不能作为它们已正确的证明。
- [ ] 按设计添加 Rust 失败回归，接入请求及固定运行上下文、版本化状态/ZIP、所有操作/会话目录和范围恢复，保留旧原生数据且不猜测授权。
  - [x] 会话目录第一步：按固定 WorkspaceDataKey 选择完整会话，保留共享/不同项目、归档、重开及目录换名归属，SDK 仍只归默认 Workspace；新增 4 个回归。旧 ZIP 尚未支持基础/项目分离，保留原手工写入边界防止创建无法恢复的归档，不能据此缩减目标功能。完整普通 **559/559**、严格 Clippy 通过，见 [验收与失败记录](../testing/checkpoint-workspace-ownership-acceptance.md)。
  - [x] 原页面逐项 **17/17**；显式组的 Python PATH 错误保留，调度参考在正确 conda 环境单独 **1/1**（16 个日程）通过。当前 release、TS **4/4**、Python **5/5**、VS Code 编译与 **57/57**依次通过，前端源码零 diff；未覆盖旧分发文件，也未完成跨 Agent 检查点 CRUD。
  - [x] 手工请求第二步：全部 11 个调用的 Agent 入场/基础目录/生命周期边界、固定会话归属及该 Agent Memory 配置；新增 4 个普通回归，含 33 次拒绝矩阵、非默认恢复/GC/重置和写入/删除竞争。完整普通 **563/563**、严格 Clippy 通过。新原 Checkpoints 页面专项首次 **1/1**，验证侧栏切换与开关/快照/保留设置/刷新/GC/reset；没有覆盖外部项目 Thread、RestoreModal 或目录代际，完整显式组与最新 release 继续按验收记录。
  - [x] 第二步收尾（2026-09-10）：完整显式组一次串行 **19/19，296.98 秒**，最新 release 与 TS **4/4**、Python **5/5**、VS Code 编译与 **57/57**依次通过，前端源码零 diff。旧 QA 包未覆盖；自动钩子仍有共享项目路径范围风险，必须随新归属格式修正，不能据手工接口与当前页面通过宣布检查点全功能完成。
  - [x] 第三步版本化身份（2026-09-10）：state/ZIP v2、固定 Console 自动上下文、取消/marker 竞争、基础文件与 Thread 项目分离、Backup 来源/目标身份及图重写。新增 9 项普通回归，完整普通 **572/572**、严格 Clippy 通过；原页面 RestoreModal 扩展专项 **1/1，15.89 秒**，预览/选择性恢复和外部项目保持已验证。旧原生有依据接回、原防抖/周期 GC/删除会话/恢复静默期尚未完成；完整显式组与 release/客户端复验见验收记录，不提前勾选父项。
  - [x] 第三步收尾：完整显式 **19/19，305.21 秒**；release **50.48 秒**与 TS **4/4**、Python **5/5**、VS Code 编译及 **57/57**依次通过，前端源码零 diff。源 release SHA-256 见验收记录；旧 QA 包未覆盖本次变更，安装态阻碍未解除，不缩减全产品目标。
  - [x] 第四步运行时：Console 默认 1.5 秒防抖、pending/active 生命周期及 Core restore barrier，关闭/停用/reset 取消，单个/批量聊天 refs/HEAD/ZIP 清理，以及原 15 分钟自动快照后 GC。另复现并修复手动 GC 错删手工快照及 compact 语义；新增 11 项普通回归，原 Python GC 专项 **5/5**，完整普通 **583/583**、严格 Clippy 通过。所有生产入口钩子、原可配置策略、恢复静默期仍未全验收，详见实施记录。
  - [x] 第四步源码收尾：显式组 **19/19，299.91 秒**，release 与 TS **4/4**、Python **5/5**、VS Code **57/57** 通过。九类新 QA 已生成，校验和、2,842 个来源输入及四类分发的 1,311 个 Console 文件核对通过；已安装 SDK、旧 wheel CLI、VSIX 安装结果和时序见 [制品记录](../testing/qa-checkpoint-packages-20260910.md)。
  - [ ] 最新分发 Core 运行、原生窗口和 Windows/Linux 完整验收；保留旧安装态失败证据及终端防护管理员核查依赖，不把源码通过或重打包当作全端安装态通过。
  - [x] 第五步 Core 基础：新增被动 Thread 执行排空与 Guard，覆盖整个生产任务及最终写入，不靠客户端连接或 idle 状态判断完成。6 项专项、完整普通 **589/589**、显式组 **19/19**、严格 Clippy 通过；release 与 TS **4/4**、Python **5/5**、VS Code **57/57** 依次通过。超时/取消、无关 Thread、审批、断开消费者、重叠集合及 Backup 排斥均有回归，详细结果见检查点实施记录。
  - [ ] 第五步 App Server 协调：固定 Workspace 成员集合并冻结新入场，等待 Console/Cron/自动任务和原生 Turn，所有真实恢复后释放。不能直接持有现有全局生命周期/checkpoint 锁调用新 API；仅会话恢复也必须覆盖。上一版九类 QA 包不含第五步代码。
    - [x] 接入 Console/Cron/Heartbeat/SDK、会话和 Agent 生命周期门禁；三类恢复共享 30 秒排空，等待时释放全局锁。手动 Cron 保留立即成功响应并在后台等待。新增 9 项竞争/断开回归，完整普通 **598/598**、严格 Clippy **12.60 秒**、fmt/diff 与前端源码零 diff 通过；这是接入基线，不是完整等价验收。
    - [x] 本次接入源码复验：完整显式 **19/19，298.92 秒**；release **52.67 秒**，TS **4/4**、Python **5/5**、VS Code 编译及 **57/57**依次通过。新源 Core 摘要与失败记录见检查点验收文档，旧 QA 包未重建或启动。
    - [x] 已复现并修正初稿自动 pending 取消与暂停期丢弃完成钩子的差异：保留计时器及完成事件，自动写入在恢复门禁外等待；先前已写入快照由同一 checkpoint 锁保证完成，不等待会反向等待恢复的任务。成功/超时/预检失败及完成钩子回归通过，完整复验见检查点验收文档。
    - [x] 运行中原 RestoreModal 新专项 **1/1，15.98 秒**：真实控件 loading、禁用返回与选择保留观察后才放开原生生产任务，恢复及重开文件/会话/默认范围检查通过；旧 idle 专项保留，前端源码零 diff。
    - [x] 自动任务保留修正收尾：完整普通 **601/601**、严格 Clippy **10.81 秒**、完整显式 **20/20，320.11 秒**、release **52.76 秒**，TS **4/4**、Python **5/5**、VS Code 编译及 **57/57**顺序通过，无跳过；没有重建/启动旧 QA 包，来源摘要及失败记录见验收文档。
    - [x] 活跃会话手工快照原生边界已修正：先验证原实现运行中快照保存上一份 session 字节，再复现并修复 Core/HTTP busy。ActiveTurn 保存运行前完整历史，完成释放、不改存储格式；完整普通 **606/606**、严格 Clippy **8.67 秒**与前端源码零 diff 通过。图片、取消/失败、恢复后新轮和 HTTP 完整结构回归见验收记录。
    - [x] 活跃快照原页面与完整显式 **20/20**、最新 release/源码客户端顺序复验通过；最新九类 macOS ARM64 QA 构建、来源/载荷/签名完整性核验完成。TS/Python 实际安装复验连接源 Core，详情见 [本批制品验收](../testing/qa-active-checkpoint-packages-20260910.md)。
    - [ ] 分发 Core 首次启动和实际原生交互、完整 Backup/提交失败交叉矩阵、跨平台与其余原功能继续保留，不能以本次修复和静态核验关闭全产品目标。
    - [x] 第六步首批源码：接入 Cron/Heartbeat 自动完成钩子，Console 共用 slash 判断，固定身份并保留原查询文本；原参考 **6/6**、新增专项 **8/8**、完整普通 **614/614**和严格 Clippy 通过，前端零改动。当前 QA 包不含本步源码，详细失败与修复见检查点验收记录。
    - [x] Core 保存失败边界：入场先写候选快照，最终写入完成后才释放运行状态；保存失败保留旧检查点但不隐藏已显示回复。Console/Cron/Heartbeat 检查实际成功回执，失败不替换旧 pending；新增六项原生与两项上层回归，普通 **622/622**、严格 Clippy 通过。失败、重开语义和版本范围见检查点验收记录。
    - [ ] SDK 完成观察/断连追踪、恢复暂停中各生产者、完整失败交叉矩阵及最新分发，仍按第六步架构 checklist 完成；不能以 Completed 事件代替 session 保存成功证明，也不把本次源码通过计入旧 QA 包。
      - [x] 现有 Workspace 宿主的 App Protocol 完成观察已接入：响应前登记、固定绑定、正常背压/断连继续完成、显式停用或关闭排空、恢复暂停和失败保存过滤；八项专项、普通 **630/630**、严格 Clippy 通过，前端/协议字段不变。
      - [ ] 默认 stdio 的 headless Workspace 服务初始化、每 Agent runtime/model/usage owner 一致性、stdio EOF 后后台任务收尾仍需独立完成。保留 SDK 显式 Thread 模型选择的既有契约，不以统一配置为由直接覆盖它；只连接 Desktop 宿主的成功不证明默认 SDK 功能已齐全。
        - [x] 已绑定 Workspace 的 SDK 配置：受信 Core 入口保留 Thread.model，按捕获的 AgentContext 选择 runtime/provider 和固定 usage owner；显式/fallback 原子快照、并发热更、输入前拒绝与伪造 wire 字段回归通过。普通工作区 **637/637**，最后测试补强后 App Server **395/395**复验，严格 Clippy **14.83 秒**；原页面/release/客户端与新制品按验收记录继续，headless 与 EOF 父项未关闭。
        - [x] 本步原页面/参考 **20/20**、release/源码客户端顺序通过；新九类 macOS ARM64 QA 构建与静态核验、安装后 SDK/VSIX/保留版 CLI 和打包 WebUI/源 Core 导航控制组完成。新输出 `qa-runtime-20260909-rWHV5J`，细项及首次 DMG 失败见 [本批制品验收](../testing/qa-sdk-config-packages-20260910.md)。包内 Core 首次启动、原生 GUI/扩展激活、跨平台和全功能未关闭。
        - [x] 无页面宿主第一步：显式 stores/Workspace 构造器与 Desktop 共用初始化，保持页面/token 前置校验，不暴露桌面 HTTP API。七项宿主专项纳入最新 App Server **402/402**；先前完整工作区 **643/643**、最后新增审批专项后的全库复验与严格 Clippy **12.79 秒**通过。图、真实入口与损坏身份的正确拒绝边界见检查点架构/验收文档。
        - [ ] 默认 CLI/SDK 尚未调用新 Workspace 构造器；preferred project 模板写入、凭据来源与各客户端 close/服务端 EOF 收尾仍需实现和验证。构造器的嵌入测试不关闭默认客户端的缺口，旧九类 QA 包也不自动包含本次源码。
          - [x] 默认接线前多进程诊断：真实 TS SDK 启动两个同目录 source Core，第二个初始化即把第一个活跃 Turn 写成 interrupted，而第一个协议仍返回原 inProgress；完整内存/磁盘快照和独立目录对照已核对，三个自有进程正常关闭。输出 `qa-shared-core-open-20260910-gwu8id` 的 `productInvariantPassed=false`，退出 0 仅表示复现成功，不表示产品通过。缺陷未修复，不能仅加调度锁，见 [进程归属设计与 checklist](../architecture/default-workspace-host.md)；共享宿主/独立 stdio 的生命周期选择待确认，生产入口未改。
          - [x] 后续先修复默认 CLI 启动互斥：在凭据读取/数据库恢复之前取得 OS 独占锁并持有到服务排空；冲突明确非零退出，不改目录或静默创建独立会话。新增 4 项单元与 3 项真实进程回归，五种启动参数/独立目录/正常退出/真实异常终止/初始化失败覆盖，最终普通 **715/715**、严格 Clippy **23.21 秒**通过。用户允许后仅清理 debug 构建产物，空间从不足 1 GiB 恢复至约 237 GiB；首次 Node PATH 失败保留。见 [验收记录](../testing/instance-lock-acceptance.md)。旧包和直接嵌入 Core 不受本次入口锁保护，共享连接、默认 Workspace 与原生/全功能父项仍开放。
            - [x] 启动锁后续顺序门禁：原前端 **2453/2453**、浏览器/参考 **27/27**、release **12.26 秒**、优化版锁及 Rust SDK 集成 **6/6**、TS **10/10**、Python **17/17**、VS Code **57/57**全部通过。新 source Core `143799...`，最终八条命令、改动输入摘要及前端零变更已核验。三语言 SDK 文档补齐真实保存失败及同目录独占语义；旧九类制品字节仍完整但未重建，原生/跨平台及共享宿主父项不关闭。
            - [x] 后续九类 macOS ARM64 QA 批次 `g6i9VJ` 已包含启动锁 `143799...`：2888 条来源、九个文件及签名完整性核验通过，四份原 Console 各 1311 文件一致；安装后 TS **10/10**、Python **17/17**、保留版 CLI **855/855 + 36/36** 顺序通过，两种 VSIX 隔离安装但未激活。SDK 包内文档与源码一致，最终核验通过，首次 DMG 资源忙及一次原参数重试记录保留，见 [本批制品验收](../testing/qa-instance-lock-packages-20260914.md)。未执行包内 Core，原生/跨平台、默认 Workspace/共享宿主和全功能父项继续开放，不以保留版 CLI 通过代表 Rust CLI/TUI 完成。
            - [x] 共享接入前客户端审计：确认 VS Code 握手后自动写全局配置、桌面/扩展退出拥有终止进程语义；原 Console SSE 断开取消而协议 WS 断开继续，不能统一改成后台继续。既有实测基线 **5/5** 顺序通过，见 [共享接入方案/架构图/清单](../architecture/shared-host-client-attachment.md)。推荐共享连接关闭仅 detach、显式退出 Core 才排空停止；宿主退出规则仍待用户确认，未改生产入口或安装后台服务。
            - [x] Workspace 模型接入诊断：完整配置/凭据来源五组、十次本地模型请求复现首次正常而重开丢失调用方 key（空存储/读取失败 → 无认证 401）。以仓库锁为起点的 383 个依赖逐项核对后复验相同；首次独立解析差异和两轮报告保留，`productInvariantPassed=false`，不是功能通过。见 [模型优先级诊断与修复约束](../testing/workspace-model-precedence-diagnostic.md)。未改生产入口或全局 key 优先级，默认 SDK 尚未接入 Workspace；需明确 provider/endpoint 关联凭据，不可简单 fallback 到任意调用方 key。
            - [x] 不依赖共享生命周期选择的 VS Code 自有 Core 收尾：重启/异常重连等待旧资源释放，deactivate 返回 EOF 排空及进程退出结果；非零/信号/超时和重复关闭不冒充保存成功。新增红灯两项先复现，完整扩展 **73/73**，两种新 VSIX 隔离安装后各 **24/24** 专项通过，真实 source Core 保存故障在重开前核对。打包外层报告命名碰撞失败保留，现有制品由后续独立校验确认，未重新打包，见 [验收和新 VSIX](../testing/vscode-owned-core-shutdown-acceptance.md)。原前端未改，其余七类制品未重打；原生激活、包内 Core、跨平台及共享宿主父项继续开放。
            - [x] Rust SDK 显式连接已有 Core：增加 WS/WSS 连接 owner，复用协议客户端；独立 initialize、TLS/hostname/auth 校验、关闭/Drop/取消/背压有界收尾，不拥有远端进程。Rust SDK **21/21**、普通 workspace **729 通过/27 ignored**、最终真实服务端 **2/2**、README doctest **2/2**、SDK release library 与严格 Clippy/fmt 通过；原 Console 源码零改动、release Core 未替换，见 [接入方案](../architecture/sdk-existing-host-connection.md) 和 [验收记录](../testing/rust-sdk-existing-host-acceptance.md)。TS/Python 显式连接、自动共享/默认 Workspace、原生与分发运行仍未完成，本次未重打九类包。
            - [x] TypeScript 显式 WS/WSS 连接及 npm 包：复用现有协议/Thread 层，独立握手、验证证书/hostname/token、AbortSignal 与有界 disconnect，不切换 owned stdio 默认值。真实 Node 客户端发现服务端丢弃 Close 回复，原始帧红灯复现后修复 writer flush；新 source Core `fafbead...` 构建通过。TS 源码/隔离安装态各 **26/26**，Rust **733 通过（含 2 doctest）/27 ignored**，原前端 **2453/2453**、Python **17/17**、VS Code **73/73**，严格检查通过，见 [TS 验收和新 npm 包](../testing/typescript-existing-host-acceptance.md)。旧 DMG/ZIP/VSIX/Core 包未重打，不包含此次 Close 修复；Python 显式连接、自动共享和原生/全功能父项仍开放。
            - [x] Python 显式 WS/WSS 连接及最终 wheel：复用原协议/Thread 层，网络 I/O 独立线程；认证/TLS/取消/异常帧/写背压、关闭回调有界清理，owned stdio 默认不变。源码与 `python -S` 隔离安装态各 **37/37、0 skipped**，含已安装 Python/TS SDK 共同访问真实 source Core；七个 SDK 文件及包内 README 与源码一致。架构图已补 Python 网络连接，见 [Python 验收与 wheel](../testing/python-existing-host-acceptance.md)。Core 沿用 `fafbead...`，原前端源码零改动；三语言完整能力、默认 Workspace/自动共享、九类新包、原生与全功能仍开放。
            - [x] 三语言连接态后的整套 macOS ARM64 QA 批次 `737ANs`：九类构建、2901 条来源及静态签名/载荷核验，四份原 Console 各 1311 文件一致；已纳入 `fafbead...` WS Close 修复、VS Code owned shutdown 和 TS/Python 显式连接。实际安装后 TS **26/26**、Python **37/37**（含本批混合语言），VS Code 源码 **73/73**、两份 VSIX 安装态各 **24/24**，保留版 CLI **855/855 + 36/36**顺序通过。DMG 首次资源忙、记录器命名冲突、VSIX 安装元数据断言修正的失败证据保留，见 [整套制品入口与验收](../testing/qa-connected-sdk-packages-20260914.md)。未执行包内 Core/原生激活，自动共享/默认 Workspace、跨平台及全功能父项继续开放；不以重复打包替代后续功能实现。
          - 当前从 [stdio 宿主退出 checklist](../architecture/stdio-host-lifecycle.md)
            开始：先补 EOF/传输错误时共享服务排空，保留 WS 普通断连行为；
            默认入口、SDK 关闭和多进程自动任务归属仍需后续实证，不能直接启用
            每个 SDK 子进程的调度器。项目目录批次 `vGGX8Z` 不包含此后续切片。
          - [x] stdio 宿主退出源码收尾：EOF/输入失败/输出失败/显式停止共享 HTTP/WSS 服务退出；Heartbeat 保留事件流等待终态，关闭后拒绝新入场。真实 CLI 只关 stdin 即成功退出；普通 WS 断连行为保持。完整 Rust **697/697**、显式浏览器/参考 **26/26**、原前端 **2453/2453**、严格 Clippy、source release 和 TS **4/4** / Python **5/5** / VS Code **57/57**顺序通过。图与源摘要见 [stdio 验收](../testing/stdio-host-lifecycle-acceptance.md)，未重打分发包，父项未关闭。
          - [ ] Core 最终 upsert 失败完整验收，见 [实施方案](../architecture/final-turn-persistence.md)：已接入 failed/保留回复和旧检查点、实例失败记录及三宿主收尾检查，Rust SDK 真实 Core 故障退出有重开前存储验证。前一阶段完整 Rust **707/707**、Clippy、原前端 **2453/2453**、显式浏览器/参考 **26/26**、source release、Rust release SDK **3/3**及 VS Code **57/57**顺序通过；首次既有 SDK 5 秒超时及后续诊断记录保留，不能宣称根因已修复。历史适配修复的后续整组回归，以及最新九类制品静态和 SDK/保留版 CLI 安装态验收已单独完成；分发运行与原生端仍未通过，见 [验收记录](../testing/final-turn-persistence-acceptance.md)。
            - [x] TS/Python 真实 Core 最终保存失败专项补齐：首次/重复 close 准确报退出码 1，重开前完整磁盘状态仍为原 inProgress，移除触发器后才验证恢复。源码及重新打包、离线安装后 TS **10/10**、Python **17/17**顺序通过，包载荷/实际导入路径/source Core 哈希一致。输出 `qa-sdk-final-persistence-20260910-TJWoJ3`；SDK 生产代码和前端未改，旧 `WXFYc3` DMG 不含此 Core 修复，父项未关闭。
            - [x] 原页面刷新错误丢失已复现并在 Rust 历史适配修复：原回复及失败提示刷新后保留，管道暂停期间 journal 完全不变，解除临时触发器后同一输入框可继续发送，后续轮次真实保存且只创建成功检查点。专项 **1/1**、JSON 矩阵、完整普通 Rust **708/708**、严格 Clippy **14.06 秒**和原前端 **2453/2453**通过；首次错误的中间 journal 假设和真正刷新失败日志均保留。输出 `qa-browser-final-persistence-20260910-1e1gjk`，不改前端、不执行分发 Core。
            - [x] 本次历史修复后续显式 **27/27**、source release **52.98 秒**、release SDK **3/3**、TS **10/10**、Python **17/17**、VS Code **57/57**顺序通过。新 Core 为 `19e454...`，最终九条命令、源摘要和原前端零变更均已核验；前两阶段 `51ba72...` 报告保持历史边界。整套新制品、原生交互、跨平台、默认 Workspace 和其余全功能父项继续开放。
            - [x] 随后九类 macOS ARM64 QA 批次 `2qPEew` 已包含 `19e454...`：2886 条来源与九个文件哈希/静态签名核验、四份各 1311 个原前端文件一致；安装后 TS **10/10**、Python **17/17**、保留版 CLI **855/855 + 36/36**顺序通过，两类 VSIX 安装但未激活。最终报告通过且保留首次 DMG 资源忙及一次原参数重试记录，见 [本批制品验收](../testing/qa-final-persistence-packages-20260910.md)。未执行分发 Core，原生/跨平台、默认 Workspace 与其余原功能父项仍开放；保留版 CLI 通过不等于 Rust CLI/TUI 全量实现。
          - [x] TypeScript SDK `close()` 接入 EOF/管道排空/退出等待，重复与回调重入共用结果；异常及超时强杀报错。源码 **9/9**、实际 SDK 包离线安装后 **9/9**（重开前直接读取 SQLite），随后 VS Code 编译和 **57/57**通过。新 SDK 包 `qa-typescript-close-20260910-Iie6Ho`，见 [验收记录](../testing/typescript-close-acceptance.md)。Python/Rust、默认宿主、整套九类包与跨平台父项仍未关闭。
          - [x] Rust SDK 自有进程 shutdown 接入 EOF/输出排空/退出等待，通用客户端保留 transport-only 语义；clone 关闭入场、取消等待保留 worker、可选 stderr 排空及失败回归通过。完整 workspace **703/703**、严格 Clippy、release workspace 和优化版真实 SDK/Core **2/2**通过；首次非零退出用例 5 秒超时根因未证明，原断言/时限保留。见 [Rust SDK 验收](../testing/rust-sdk-close-acceptance.md)。Python、默认宿主、分发及跨平台继续开放。
          - [x] Python SDK 同步 close 接入 EOF/输出排空/等待和明确失败；并发、reader/close 回调重入、失败句柄及原异常保留有专项。源码 **16/16**、wheel 离线安装后 **16/16**，重开 Core 前只读 SQLite 比较完整状态。随后 Rust release 集成 **2/2**、TypeScript **9/9**、VS Code 编译与 **57/57**顺序通过；见 [Python SDK 验收与局部 lint 例外](../testing/python-sdk-close-acceptance.md)。默认宿主、保存失败传播、整套九类制品和全功能交互仍未关闭。
          - [x] stdio/SDK 新批次 `WXFYc3`：九类 macOS ARM64 QA 构建、2882 条来源/校验和/签名静态核验，四份各 1311 个原前端文件一致。实际安装后 TS **9/9**、Python **16/16**，两类 VSIX 安装但未激活，legacy CLI **855/855 + 36/36**顺序通过。首次 DMG 资源忙与原参数重试日志保留，见 [九类制品验收](../testing/qa-sdk-shutdown-packages-20260910.md)。未执行分发 Core，默认宿主/最终保存错误传播、原生交互、跨平台和全功能父项继续开放。
        - [x] 共享初始化源码收尾：完整显式 **20/20，319.04 秒**、source release **52.62 秒**，TS **4/4**、Python **5/5**、VS Code 编译及 **57/57**顺序通过，前端源码零 diff。新源摘要与测试范围见检查点验收文档，未重新打包或启动分发 Core。
- [ ] 原页面跨 Agent/项目完整交互、并发关闭与恢复、严格检查及各端顺序验收；本节已开始生产代码修改，但不把 §8 的单 Workspace 基线当作该范围已完成。

#### 14.2.24.53.1 模型凭据接入：具体修复决策

- [x] 继续核对 SQLite 覆盖启动地址、动态改地址保留 key，以及
  Agent/Console/Cron 的二次凭据加载，写入
  [修复策略、架构图和 Checklist](../architecture/workspace-model-credential-policy.md)。
- [ ] 确认统一优先级：provider 存储优先，严格身份绑定的启动 key 仅补明确
  缺失；读取错误阻止相关模型请求但保留原设置页修复入口。
- [ ] 按方案先写失败回归，再实现并覆盖真实多入口请求、清除/切换、重启和
  回滚。当前仅文档更新，原重启缺陷尚未修复，不重复构建已有九类制品。

#### 14.2.24.54 Cron 公开多 Agent HTTP 与后台执行

承接 §38–53 已批准的原功能等价目标，具体方案及架构图见
[公开 Cron 接入](../architecture/cron-public-agent-scope.md)。本节取代旧切片中
“非默认 Job HTTP 501、后台跳过全部非默认任务”的当前状态，不改原 Console。

- [x] 原 API 对照及 4 项真实失败回归：公开 CRUD、同名 PUT、并行运行和后台调度。
- [x] Job HTTP 在 Cron 锁内验证注册/启用/根标记，按 WorkspaceDataKey + public ID
  查找内部键；伪造 meta 不转移归属。PUT 保持原 create-or-replace 语义。
- [x] 后台逐任务核验归属；健康复制任务能调度，停用/无效归属跳过且无默认回退。
- [x] 原 writer Cron 页面切换、候选目标、CRUD、真实工具/trace、历史和刷新通过，
  默认任务及文件不变。恢复暂停期间手动运行沿用固定工作区身份。
- [x] 最终公开审批/Agent 生命周期、删除后同名重建排队边界、完整 Rust/原页面整组
  与客户端回归收口，记录见 [本轮验收](../testing/cron-public-scope-acceptance.md)。
  整组暴露原目录列表 500 项截断，512 目录 HTTP 先红后绿，仅移除截断；
  最终 Rust **743/743**、显式 **28/28**、前端 **2453/2453**、TS **26/26**、
  Python **37/37**、VS Code **73/73**和 release Rust SDK **3/3**通过。
  新 source Core 为 `16e722...`；失败日志保留，前端源码零变更。
- [x] 新九类 macOS ARM64 QA 批次 `4v2E9d` 纳入本节生产改动：2902 条来源，
  九个制品和四份原 Console 静态核验；source Core `54f914...` 由发布脚本
  重新构建，安装后 TS **26/26**、Python **37/37**，VS Code 源码 **73/73**、
  两 VSIX 各 **24/24**，保留版 CLI **855/855 + 36/36**依次通过。入口和
  保留的 DMG 资源忙/一次原参数重试记录见 [制品验收](../testing/qa-cron-packages-20260914.md)。
- [ ] 包内 Core/原生激活、跨平台、外部 Channel、自动共享/default Workspace
  及其余功能仍独立开放，不以静态/隔离安装代替运行验收。

#### 14.2.24.55 Agent 设置保存的 Workspace 绑定

沿用 §47 的隔离要求，不依赖待确认的默认共享/凭据策略。
方案和清单见 [身份方案](../architecture/workspace-data-identity.md#设置保存入口的绑定校验补漏2026-09-14)，
执行记录见 [保存校验验收](../testing/agent-config-binding-acceptance.md)。

- [x] 两项失败回归证明旧注册向被替换目录保存配置并返回成功。
- [x] 字段保存与公开 Agent 设置更新在注册锁内复用已有身份解析；保存
  目标来自验证后的目录，正常成功响应和停用 Agent 管理编辑保持。
- [x] 六项普通专项进入完整 Rust **749 通过**；严格 Clippy 通过。
- [x] 原页面/参考显式 **29/29**、原前端 **2453/2453**；source release
  `8dad780...` 构建后 Rust SDK **3/3**、TS **26/26**、Python **37/37**、
  VS Code 源码 **73/73**顺序通过，原前端源码零改动。
- [x] 新九类 macOS ARM64 QA 批次 `wADUE9` 包含本节修复；2,903 条来源、
  四份原 Console 静态核对，Rust SDK 3、安装后 TS 26/Python 37、VS Code
  源码 73/两 VSIX 各 24、保留版 CLI 855+36 顺序通过。实际包来源 Core
  `be7918...` 已复验；下载及保留的 DMG 失败/重试见
  [制品验收](../testing/qa-agent-config-packages-20260914.md)。原生运行门禁未解除。
- [ ] 默认共享、凭据策略、外部通道及原生/跨平台等父项继续开放。

#### 14.2.24.56 Channels 配置的 Workspace 隔离

继续已批准的原交互等价改造，不涉及默认共享宿主或凭据优先级选择。
方案、架构图和清单见 [Channels 归属](../architecture/channel-workspace-scope.md)，
执行证据见 [专项验收](../testing/channel-workspace-scope-acceptance.md)。

- [x] 真实 HTTP 复现跨 Agent 保存覆盖和无效身份仍可读写；两条预期正确行为的红测试确认失败。
- [x] Channels v2 使用 WorkspaceDataKey；单项/批量保存和读取校验所选注册，旧原生 v1 只关联默认 Workspace；未改原页面或接入 Python 数据。
- [x] 范围备份/恢复接入；重新注册、复制、损坏数据、全局恢复不覆盖未选通道配置通过。跨 Core 真实 HTTP 恢复保留默认配置。
- [x] 完整 Rust 普通回归 757 通过、严格 Clippy 通过；后补的全局恢复与原 Channels 页面专项共 11/11。页面实际操作侧栏/抽屉并验证刷新及 Core 重开。
- [x] 全部原页面/参考显式组 30/30、前端 2453/2453；新 source Core `7dbb3b3...` 和九类 `eaTvv4` QA 制品完成来源核对、Rust SDK 3、安装后 TS 26/Python 37、VS Code 源码 73/两 VSIX 各 24、保留版 CLI 855+36 顺序验收。见 [下载清单与限制](../testing/qa-channel-packages-20260914.md)；旧 `wADUE9` 保留，不含本修复。
- [ ] 17 个外部 Channel 运行时、共享宿主、原生激活和跨平台等父项仍独立开放。

#### 14.2.24.57 Channels 与 Agent Profile 的统一配置权威

§56 关闭专用 Channels API 的跨 Workspace 覆盖，但并未统一通用 Profile 的读写入口。
方案见 [统一权威与发布清单](../architecture/channel-profile-authority.md)，证据见 [真实 HTTP 诊断](../testing/channel-profile-authority-diagnostic.md)。

- [x] 新隔离宿主初始/重开共 22 个真实 HTTP 请求，确认双向配置分歧、通用写入口绕过 Console 校验及外部通道门禁；复制清空行为正常。
- [x] 另以包含 id/name 的完整请求重复 22 个真实 HTTP 对照；原 Python handler/模型/ASGI 8 项语义对照通过，不把内存持久化替身当作真实磁盘验收。
- [x] 前置共用校验已落地：无效 Console/容器/超限、17 个外部通道变更、未知通道、凭据发布前拒绝及有效默认/null/缺失往返；原版全结构对照修正 Slack 默认值，并补齐浏览器整数/小数往返的第 6 项回归。
- [x] 最终普通工作区 764/764；原 Agents/Channels 页面及原版完整默认对照 3/3；Node 真实 HTTP 21 请求；Clippy/fmt 通过，原 Console 零变更。31 个显式项仅重跑上述 3 项，不宣称全部重跑。
- [x] 统一配置视图、未配置状态与普通失败回滚；双向 API 和运行上下文使用 SQLite 权威，新增 7 项发布回归、1 项 null 范围备份和 1 项相对数据目录初始化回归。最终普通工作区 773/773、Clippy/fmt 通过，证据见 [权威发布验收](../testing/channel-profile-publication-acceptance.md)。
- [x] 旧影子冲突保留、并发编辑、SQLite/凭据可恢复错误、重开与 35 个真实 HTTP 请求；原版页面/参考显式组 31/31 通过，具体源码边界见验收记录。
- [ ] 最终相对目录修正后的完整显式组稳定通过：本次 App Server 为 29 通过、Anthropic 输入焦点 1 失败；该项不改代码单独复测通过，但不以重试覆盖原失败。
- [ ] 不可恢复逆操作的宿主恢复所有权、失败后准入与进程强杀/重启恢复；这些不能用普通回滚通过代证。
- [x] 同一存活宿主保留失败的文件/凭据逆操作，新 Agent/HTTP/协议/备份请求拒绝，普通配置保存和正常关机可重试；持续故障/原值缺失先红后绿，见 [宿主恢复验收](../testing/agent-publication-recovery-acceptance.md)。强杀持久恢复与所有在途业务边界仍属于上项未完成范围。
- [x] 四项新增恢复回归纳入完整 Rust 777/777；严格 Clippy/fmt 通过。文件不可达时保留待恢复对象，不重复已完成的凭据逆操作；不把该测试当作任意外部文件替换保护。
- [x] 当前恢复实现的原页面/参考完整显式组 31/31 顺序通过，Console 源码零改动；不撤销前次输入焦点/SDK EOF 失败，也不把新请求门禁当作强杀恢复或全功能完成。
- [x] 共享文件恢复的可观察独立变化保护：四项真实红测试确认数据损失；暂存原目标/替换树、父目录/恢复目录身份，回滚与清理发现变化则保留。十项新增回归纳入最终 Rust 787/787、严格 Clippy/fmt 通过，见 [独立数据保护验收](../testing/restore-independent-data-acceptance.md)。检查与操作间的恶意并发竞态、强杀及跨平台仍独立开放。
- [x] 上述文件保护最终源码的原页面/参考完整显式组 31/31 顺序通过，原前端与浏览器脚本未改；旧九类 `eaTvv4` 制品未重建，不包含本节后续修复。
- [ ] 完成恢复故障专项后更新九类制品；原生与跨平台验收保持开放。
- [x] 持久提交判定基础：安装级 SQLite UUID/状态与 Channels 同事务提交，逻辑备份不携带记录，本地恢复前拒绝业务表替换；Core 接口遵守恢复屏障。新增 10 项测试纳入完整 Rust 797/797、严格 Clippy/fmt 通过，见 [协议与后续清单](../architecture/agent-publication-journal.md) 和 [验收](../testing/agent-publication-decision-acceptance.md)。尚未接入真实 Profile 发布、文件/凭据日志或启动恢复，不以数据库重开测试代证强杀恢复。
- [x] 提交判定源码的原 Agents/Channels/备份页面专项顺序 6/6，原 Console 与浏览器脚本未改；本轮未重跑其余 25 个显式项，旧九类制品仍未重建。
- [x] 恢复凭据接口及逆操作：安装/事务独立账户，严格绑定与缺失/空值区分，预留/清理失败保留，恢复前比较现值。新增 15 项进入 Rust 812/812，严格 Clippy/fmt 通过，见 [凭据接口验收](../testing/agent-publication-credentials-acceptance.md)。系统凭据库未访问；安装身份、文件日志和真实 Profile/启动接入仍未完成。
- [x] 凭据接口源码的原 Agents/邮件页面专项顺序 4/4，原 Console 与浏览器脚本未改；本轮未重跑其余 27 个显式项，旧九类制品仍未重建。
- [x] 两文件持久日志与 SQLite 控制元数据组合：原生路径、身份/权限/内容摘要、实际位置回滚和分步清理；稳定安装 UUID、可信摘要与 prepared 同事务保存、不随逻辑备份转移。隔离库子进程七边界强杀后重开验证，新 17 个测试入口（含一个子进程入口）进入 Rust 829/829、严格 Clippy/fmt 通过，见 [文件日志验收](../testing/agent-publication-files-acceptance.md)。真实宿主接入、凭据强杀、最终清理协议和跨平台仍开放。
- [x] 文件日志源码的原 Agents/Channels/备份页面专项顺序 6/6，原 Console 与浏览器脚本未改；本轮未重跑其余 25 个显式项，旧九类制品仍未重建。
- [x] 真实 Profile、失败重试、正常关机及初始化前恢复接通：SQLite 同事务保存非秘密文件元数据/判定/摘要，cleaning 后才删副本，最终同事务清除控制记录。新增 9 个测试入口（含一个子进程入口）纳入最终 Rust 838/838、严格 Clippy/fmt 通过；无凭据变更的真实 Profile 九标记强杀和启动重开通过，见 [宿主接入验收](../testing/agent-publication-host-acceptance.md)。凭据强杀/断电、prepared 回滚收尾强杀、全部在途业务与跨平台仍开放。
- [x] 真实宿主接入源码的原 Agents/Channels/备份/邮件页面专项顺序 7/7，原 Console 与浏览器脚本未改；本轮未重跑其余 24 个显式项，旧九类制品仍未重建。
- [x] 带凭据的宿主强杀：父进程内存凭据服务跨子进程死亡存活，13 个正向和 9 个 prepared 逆向检查点乘三种原值，共 66 次真实强杀及重开通过。新增 4 个测试入口（含子进程入口）纳入完整 Rust 842/842、严格 Clippy/fmt；修复测试服务 accepted socket 的非阻塞读取竞态，旧协议快照测试改为等待 producer 的实际完成信号。见 [凭据中断验收](../testing/agent-publication-interrupt-acceptance.md)。仅测试代码/标记变更，未重跑显式页面、未重建制品；系统凭据持久性、孤立暂存、全部在途业务和跨平台仍开放。
- [ ] `eaTvv4` 仍是诊断发现时的包，不包含本节修复；外部通道与原生/跨平台父项继续开放。

#### 14.2.24.58 最新源码开发快照构建与逐项验证

继续用户已明确要求的“各个 build 好、逐个测试”，把 §57 已实现并通过本机回归的改动纳入新的隔离开发快照。它不是正式发布，也不关闭恢复完备、全部旧功能等价、原生或跨平台门禁；旧 `eaTvv4` 和用户数据保留。原有“完整门禁后更新制品”的条目仍指可宣称完整通过的交付，本次仅生成明确标注未完成验收的开发快照，不能用构建成功代替最终目标。

- [x] 核对 §57 最终 842 项源码证据（本轮不重复计数）；完整原页面/参考显式组 31/31、原前端 295 文件 2453/2453。证据在 `dist/qa-delivery-20260915-3RUKd0`。
- [x] 两处暂存目录无跟踪文件/链接，Core 与旧来源、1311 个 Console 文件逐项匹配；已移动到本批 `staging-before-build/tauri-core`、`vscode-core`，可恢复，未递归清理其他内容。
- [x] 既有正常构建/版本检查/QA 签名流程生成九类制品，批次 `dist/qa-runtime-20260914-6BiWlu`。DMG 首次资源忙；确认无半成品/挂载/进程后原参数一次重试成功，再按既有 `--after-desktop` 续建。全部记录保留；未改安全检查或访问正式签名凭据。
- [x] 九类包静态核验、2927 条来源、四份各 1311 个原 Console 文件核对通过；Rust SDK 3、安装后 TS 26/Python 37、VS Code 源码 73/两种 VSIX 各 24、保留版 CLI 855+36 均通过。只运行源码 Core，未执行包内 Core、Desktop 窗口或 VS Code 激活。
- [x] [下载清单与验收边界](../testing/qa-publication-packages-20260915.md) 已更新，最终证据复核通过；旧九个制品、可恢复暂存和用户数据保留。Windows/Linux/macOS x64、系统凭据持久性、孤立暂存/全部在途业务、17 个外部 Channel 和其他功能差距仍开放；未 commit/push 或发布，总 goal 未完成。

#### 14.2.24.59 工具调用列表的 Agent 错误隔离

继续已批准的原交互等价工作。核查原 `ToolStream` 发现其只广播实时块，不保存历史，不能凭 Rust 未补发运行中历史推断回归。本次不改变流协议；修复已确认的列表入口：`requested_agent_id` 错误被回退为 default，而 Workspace 解析错误被当作空列表，导致无效上下文获得错误工作区数据或伪成功。

- [x] 真实运行中 Shell/HTTP 红灯确认无效 Agent 列表返回 200；扩展既有用例覆盖无效/未知/非法编码 Agent、有效空 session 与正常工具控制，专项通过。
- [x] 列表传播既有 Agent/Workspace 校验错误，只有已验证上下文中不存在的 session 才返回空列表；正常省略/空白/default 语义保持，前端与其他工具动作未改。
- [x] 完整普通 Rust 842 通过/31 ignored、严格 App Server Clippy/fmt 和来源复核通过；没有新增测试入口，没有重跑显式页面。见 [验收记录](../testing/tool-call-agent-validation-acceptance.md)。旧九类 `6BiWlu` 包未变且不含本修复；全功能、原生和跨平台父项继续开放。

#### 14.2.24.60 原前端插件与 PawApp 打开

当前固定空列表与缺失文件路由阻断原 `loadPawApp`。继续已批准的功能等价方案，按 [前端插件运行链路](../architecture/frontend-plugin-runtime.md) 的 checklist 接通真实目录发现、JS/CSS 文件与原 App Center 加载交互；不添加不存在的 iframe，不将目录读取标记为已加载后端插件，不关闭安装管理/原生/跨平台父项。

- [x] 原磁盘发现和文件路由已接入 Rust；四项普通红绿测试、原 App Center 打开/交互/刷新/返回重开、原 Python 十个 HTTP 响应对照通过，前端源码不改；总架构图同步补充插件链路。
- [x] 最终串行 Rust 846/846、显式组 33/33、原前端 2453/2453，严格 App Server Clippy/fmt、API 清单和来源哈希核对通过。见 [验收记录](../testing/frontend-plugin-loading-acceptance.md)。九类 `6BiWlu` 开发快照保持原样，不含本轮及上一轮工具校验修复。
- [ ] 默认并发 SDK EOF 关闭超时仍待定位修复：本轮两次失败均保留，单项/串行通过不关闭。后端插件执行、安装管理、原生/跨平台和全功能父项仍开放。

#### 14.2.24.61 Rust SDK EOF 关闭超时

按 [关闭竞态排查方案](../architecture/sdk-eof-race.md) 保留原时限和断言，先取得失败阶段，再用受控红绿测试定位修复；默认并发验收仍必须通过，不能以串行绿灯代替。原前端与旧制品不变。

- [x] 补全正常关闭失败诊断，新增四个独立 runtime / 64 子进程回归；真实默认并发组捕获准备屏障超时，分离并发初始化与同步关闭阶段后通过，原关闭时限/断言不变。
- [x] 最终默认并发 Rust 847/847（33 ignored）、SDK 22/22、真实 Core SDK 集成 3/3、严格 Clippy/fmt 与来源核对通过；本轮仅测试改动，没有新生产实现或制品。见 [验收记录](../testing/sdk-eof-race-acceptance.md)。
- [ ] 历史 EOF 超时根因仍开放，准备屏障修正不替代它；九类新快照、原生/跨平台及全功能父项未完成。

#### 14.2.24.62 插件与工具校验的新九类开发快照

依照 [构建与逐项验收 checklist](../testing/qa-plugin-packages-20260915.md)，将 §59–61 当前源码纳入新开发快照。使用独立输出并保留旧包和暂存，实际测试 source Core 与隔离安装客户端；原生、包内执行、跨平台及全功能父项保持开放。

- [x] 新 `VQYCTN` 九类开发制品完成，含插件加载和工具校验生产修复；source Core `70bd9546…`，2930 条来源，四份原 Console 各 1311 文件一致。首次 DMG 资源忙失败保留，只读预检后原参数重试成功。
- [x] 优化版原插件页面/参考 2/2、Rust SDK 对新 source Core 3/3、安装 TS 26/Python 37、VS Code 源码 73/两种 VSIX 各 24、保留版 CLI 855+36 全部通过；最终来源/新旧包/暂存复核通过。未重复计入前轮完整回归。
- [ ] 原生、包内 Core 执行、跨平台、历史 SDK EOF 和全功能父项仍未完成；开发快照不是正式发布。

#### 14.2.24.63 原插件管理页读取

按 [读取入口方案](../architecture/plugin-management-reads.md) 接通原 PluginManager 的列表、状态和文件 API，验证原页面搜索/视图/刷新/重载；共享磁盘读取，不虚构后端加载或安装成功。安装执行和全功能父项仍开放。

- [x] 新增两项普通测试先红后绿；原管理页浏览器流程与 Python 13 个完整响应对照通过，既有 App Center 浏览器与 Python 10 个响应对照保持通过。
- [x] 默认并发完整 Rust 849 通过、0 失败、35 ignored；四项相关显式测试全部通过，严格 Clippy/fmt/API 清单及来源核对通过。见 [验收记录](../testing/plugin-management-reads-acceptance.md)。
- [x] 原前端与既有浏览器脚本不变；`VQYCTN` 九类包及 source release 哈希未改，本轮新管理读取接口尚未进入该开发快照。
- [ ] 安装/上传/市场、加载/卸载、后端运行时、完整认证及全功能/原生/跨平台门禁仍开放；历史 SDK EOF 问题不因本轮通过而关闭。

#### 14.2.24.64 插件后端运行时决策审计

按 [运行时决策](../architecture/plugin-runtime-decision.md) 继续核对安装、上传、卸载的真实执行契约。仓库 14 个插件全部声明存在的 Python 后端；只读审计及入口哈希已记录。D3 的纯 Rust/无 Python fallback 与“任意旧 Python 插件原包运行”是不同要求，不能通过新增成功响应掩盖差异。

- [x] 审计原 loader/PluginApi/安装后处理及 14 个入口，不执行插件、不安装依赖、不改产品代码或制品。
- [ ] 确认维持纯 Rust 并重写插件后端，或批准可选 Python 插件兼容进程；确认前不更改 D3。
- [ ] 依选定方案完成真实安装/执行/重装/卸载和全部原插件功能，再进入分发验收；总 goal 不变。

#### 14.2.24.65 插件管理读取开发包

插件后端路线尚待确认，不把自动 goal 续跑作为更改 D3 的授权。继续不依赖该选择的已批准构建：按 [分发 checklist](../testing/qa-plugin-manager-packages-20260915.md) 将管理读取修复纳入新的九类开发包，并逐项验证。旧 `VQYCTN` 保留，前轮 849/4 测试证据先核对，不冒称本轮全部重跑。

- [x] `CE3oB2` 九类开发制品构建与静态检查通过，source Core `ab663c72…`，2931 条来源；原 Console 四份各 1311 文件一致。DMG 首次资源忙保留，预检后原参数重试成功。
- [x] 优化版插件 4/4、Rust SDK 对新 Core 3/3、安装 TS 26/Python 37、VS Code 源码 73/两种安装 VSIX 各 24、保留版 CLI 855+36 全部通过；最终来源/新旧包/暂存复核通过，下载见上方文档。
- [ ] 插件后端路线、完整原功能、原生/包内 Core/跨平台与历史不稳定性仍开放；未更改 D3 或宣称 goal 完成。

#### 14.2.24.66 当前开发包的完整前端专项

继续 [完整回归 checklist](../testing/full-ui-regression-20260915.md)：核对 `CE3oB2` 的 2931 条来源、42 个额外脚本、九类包哈希以及全部 35 个显式项后，逐项执行 App Server/CLI 的原页面与 Python 参考测试，并重跑原 Console 全量单测。插件运行时选择仍未获新答复；不更改 D3，不将本轮验证替代全功能或原生/跨平台门禁。

- [x] 优化版 App Server 显式组 34/34（354.97 秒）及 CLI Debug 1/1（15.34 秒）全部通过，合计 28 个浏览器场景与 7 个原 Python 对照项，无失败/忽略。
- [x] 原 Console 全量 295 文件/2453 测试通过（74.68 秒）；2931 来源、42 脚本、source Core 与九类包最终哈希复核通过。未修改产品源码、前端或既有测试驱动。
- [ ] 插件路线、剩余旧功能、历史偶发问题、原生/包内 Core/跨平台门禁继续开放；没有因显式测试全部通过而标记总目标完成。

#### 14.2.24.67 原插件市场搜索

按 [市场搜索方案](../architecture/plugin-market-search.md) 接通原 Market tab 的真实分页/条件查询及失败处理。它不依赖待确认的插件后端执行路线；官方 CDN 目录与版本过滤、安装/上传/卸载继续单独开放。共用既有固定上游和网络边界，先红绿测试，再原 Python/浏览器对照，不修改前端以适配缺失行为。

- [x] 市场查询原参数/完整 JSON、422 与真实超时/重定向等五项普通测试通过；原 Python 17 组与实际 Market 页面全流程两项显式测试通过。首次选择器前缀错误保留并仅修正新驱动。
- [x] 完整 Rust 854 通过、37 ignored；严格 Clippy/fmt、API inventory 及来源/旧九包复核通过。见 [验收记录](../testing/plugin-market-search-acceptance.md)，本轮不重建旧包。
- [x] 后续 §68 修复 HTTP 产品版本与 Core 版本混用导致的原插件兼容标签误判；保持 Core/SDK/App Protocol 版本独立，原 UI 红绿测试通过，不通过改标签或前端绕过检查。
- [ ] 官方目录/安装/后端执行、完整认证及全功能/原生/跨平台门禁继续开放。

#### 14.2.24.68 产品版本身份修复

按 [版本身份方案](../architecture/product-version-identity.md) 修复已验证的 `/api/version` 混用：原 UI 使用产品版本，Core/SDK 与协议版本独立。增加 HTTP/握手完整响应、源文件防漂移以及原页面兼容/不兼容/取消/重载红绿测试；不修改前端、不执行插件安装、不引入 Python 运行时。checklist 和验收结果在该方案维护。

- [x] HTTP 产品 `2.2.0b5`、SDK Core `0.2.0`、协议 `3` 独立；新专项 3/3 和原市场显式回归 2/2 通过。保留红测试与新驱动悬停定位失败证据，原 UI/旧驱动未改。
- [x] 完整 Rust 855 通过、0 失败、39 ignored；严格检查及 2931 来源/44 既有脚本/旧九类包复核通过。见 [验收记录](../testing/product-version-identity-acceptance.md)。
- [x] 新源码已于后续 §69 纳入 `xic9ei` 九类开发包。
- [ ] 插件运行时选择、剩余原功能与原生/跨平台门禁继续开放，总 goal 未完成。

#### 14.2.24.69 产品版本与市场搜索开发包

按 [构建验收 checklist](../testing/qa-product-version-packages-20260915.md) 将 §67–68 的已验证修复纳入新的九类开发包，保留 `CE3oB2`，逐包检查并用新 source Core 验证 SDK 与原页面。不修改原前端或 D3，不以本地构建替代全功能、包内 Core/原生/跨平台验收。

- [x] `xic9ei` 九类包构建、2934 条来源与四份原 Console 一致性、签名/DMG 只读检查通过；首次 DMG 资源忙证据保留，预检后原参数重试成功。
- [x] 优化版版本 3/3、市场 2/2、Rust SDK 对新 Core 3/3；安装 TS 26/Python 37、VS Code 源码 73/两种安装 VSIX 各 24、legacy CLI 855+36 全部通过。新旧各九包与保留暂存最终复核通过。
- [ ] 全部原功能、插件后端路线、包内 Core/原生/跨平台及历史偶发问题仍开放；未宣称 goal 完成。

#### 14.2.24.70 xic9ei 完整前端专项

按 [完整回归 checklist](../testing/full-ui-product-version-20260915.md) 核对新包来源后，执行当前全部 39 个显式项（30 浏览器、8 Python 对照、1 版本文本校验）及原 Console 全量单测。不改原前端、旧驱动或产品代码，不以该专项替代剩余原功能与原生/跨平台门禁。

- [x] 核对 2934 来源、45 脚本和九包，原 Console 295 文件/2453 项通过；CLI Debug 1/1、DevTools 13/13 通过。
- [ ] App Server 整组零失败：实际 37/38 通过，版本页面在浏览器关闭 2 秒期限处失败；页面前置断言已执行完但不算整项通过。整组 gate 保持失败，原驱动/超时/产品代码未改。
- [x] 失败审计与源码/九包一致性已记录；下一步定位关闭时序，之后重跑整组，不用成功子集关闭门禁。

#### 14.2.24.71 浏览器关闭时序诊断

按 [诊断方案](../architecture/browser-shutdown-diagnostics.md) 为新增版本页面测试补充可选有界时序记录，保留现有 2 秒退出要求和原错误。先验证观测无控制副作用，再做有限复现；只有证据支持时才修复，之后重跑整组。原前端、Rust 产品代码、共享关闭判定及已交付包不改。

- [x] 观察器 3 项和共享 DevTools 13 项单测通过，有限复现 5/5；正常退出耗时 72–86 ms，无强制清理。
- [x] 当前整组 App Server 38/38、CLI Debug 1/1 全部通过；原 2934 构建来源/前端、九包/source Core 哈希一致，记录见 [验收](../testing/browser-shutdown-diagnostics-20260915.md)。本轮仅新增版本驱动诊断，不重建包。
- [ ] 原关闭超时未复现、根因未确认，未作生产修复。历史偶发问题、插件后端选择、完整原功能及原生/跨平台继续开放。

#### 14.2.24.72 原官方插件目录

按 [官方目录方案](../architecture/official-plugin-catalog.md) 补齐固定 CDN 索引、本地安装/升级标记及原版本筛选，保持原前端不变。使用纯 Rust PEP 440 依赖和原 Python 对照，先红绿再浏览器/网络边界测试；不改变插件执行路线或虚构安装成功。

- [x] 原 HTTP 200/error、gzip 双重 8 MiB 边界、30 秒超时、固定来源和本地 manifest 读取；初始 2 个红测试转绿。
- [x] 原 Python 113 条目录输入完整响应对照，原 Official Plugins 页面加载/标记/筛选/视图/刷新/故障恢复/重载通过；不触发安装操作。
- [x] workspace 866 项、专项 13 项、严格检查/格式、API 清单与来源核对通过；保留失败过程，见 [验收](../testing/official-plugin-catalog-20260915.md)。
- [x] 后续版本保序与 Git 完成时序修复已纳入新 `NvQ0h0` 九包；旧九包未覆盖，见 [构建验收](../testing/qa-official-catalog-packages-20260915.md)。
- [ ] 极端 Python 整数转换阈值、特殊 Unicode case folding 和异常元数据输入等价仍开放。
- [ ] 安装/上传/卸载和插件后端运行不属于只读目录完成范围；完整原功能与原生/跨平台门禁继续开放。

### 14.3 客户端与发布

#### 原有 Harness 功能接线

最新全组回归通过后，继续核对真正未实现的原功能。Codex/Qoder 原版 backend 不是未来扩展；当前 Rust `validate_backend` 仍以 409 拒绝创建，七个 Harness 路由未接线。按 [专项架构与 checklist](../architecture/harness-runtime-parity.md) 先固化原 handler 全响应和调用参数，再实现纯 Rust 外部进程协议、workspace 生命周期、聊天/审批/恢复及原页面对照。不能用假状态或只放开创建替代真实执行；本阶段先写参考程序，不改变原 UI 或已交付包。

- [x] 核对源文件与调用链，记录真实缺口及完整目标架构。
- [x] 原 handler 参考程序及完整结构断言通过：7/7 测试覆盖七个接口的 29 次请求；参考中 workspace/runtime/adapter 为替身，未执行外部进程，见 [验收基线](../testing/harness-handler-reference-20260915.md)。原来源与九包哈希未变。
- [x] 新增原生 `qwenpaw-harness` Codex 通信组件；本机普通/带空格构建路径各 17/17、workspace 888/0/42、严格检查通过，见 [通信验收](../testing/harness-codex-transport-20260915.md)。组件尚未被 App Server 使用，九包仍是旧快照，不含新组件；复制测试二进制启动问题保留为未解诊断。
- [x] 在已有通信组件上实现账号过滤、浏览器/设备码登录、注销及完整模型分页。原 adapter 方法体的 13 组已连接控制面对照、Python 4/4、Rust 24 项及带空格路径通过；workspace 895 passed / 0 failed / 43 ignored、严格检查通过，见 [控制面验收](../testing/harness-codex-control-20260915.md)。未实现安装发现/完整 provider 状态，未接入 Agent、未重建九包，不把本地替身算作真实账号验收。
- [x] Codex 只读文件发现保留原优先级、显式无效配置停止、PATH 遮蔽/内嵌运行时拒绝及路径规则；原发现 18 组与控制面 13 组对照通过，组件 33/0/2、workspace 904/0/44、严格检查通过，见 [发现验收](../testing/harness-codex-discovery-20260915.md)。需 caller 提供主机上下文与 bundled candidate；自动分发定位、启动配置生命周期、Windows 实机和 Agent 接线仍未完成，原前端与九包不变。
- [x] 单个 Codex 客户端的有界启动所有者完成原 argv/env 构造、并发启动去重、配置替换、取消等待后的继续清理与终态关闭；清理失败锁存，不静默重试。生命周期 12/12、组件 45/0/2、两项原参考 31 组、workspace 916/0/44 及严格检查通过，见 [生命周期验收](../testing/harness-codex-lifecycle-20260915.md)。未接入 capability fingerprint/session 映射与完整 Agent，原前端/九包不变。
- [x] Codex Provider 组合发现、生命周期与控制面，保留完整原三项目录及四类状态；动态发现、错误和独立所有者等 9 项测试通过。组件及带空格路径 54/0/3、三项显式原参考、Python 4/4、workspace 925/0/45、严格检查通过，见 [Provider 验收](../testing/harness-codex-provider-20260915.md)。能力 DTO 仅有具名布尔字段 lint expectation；目录声明不代表能力执行完成，HTTP/Agent/Qoder 和九包重建仍开放。
- [x] Codex Skills 只读发现经 Provider 复用现有进程，保留原完整 DTO、请求 cwd、按名称/来源保序去重及默认值。新增 7 项普通测试、4 组 Skills 原方法对照，组件及带空格路径 61/0/4、workspace 932/0/46、Python 5/5、四项显式参考和严格检查通过，见 [Skills 验收](../testing/harness-codex-skills-20260915.md)。MCP 的独立 CLI 路径、HTTP/Agent 和新包尚未完成。
- [x] Codex MCP 独立 CLI 发现接入 Provider；有界并发、双管道限长、取消/超时/stop/shutdown 回收与错误锁存完成本地测试。新增 12 项普通测试、7 组原方法对照，组件及带空格目录 73/0/5、workspace 944/0/47、Python 9/9、五项显式参考及严格检查通过，见 [MCP 验收](../testing/harness-codex-mcp-20260915.md)。新编译原生 fixture 停在 dyld 入口的根因未解；MCP 测试改用独立 Python 标准库脚本，不进入 Rust 产品。原前端/九包不变，实际 Codex/Qoder、完整 adapter/Agent/HTTP 与新包门禁未关闭。
- [x] 有效能力内存模型、Codex 配置投影、值摘要与 fingerprint 完成原方法对照；保留环境冲突顺序、工具策略及 ASCII 编码。新增 10 项普通测试、24 组参考、Python 4/4，组件及带空格目录 83/0/6、六项显式参考、workspace 954/0/48、严格检查通过，见 [投影验收](../testing/harness-codex-projection-20260915.md)。仅新增已有锁定 indexmap/sha2 的 Harness 依赖边；resolver/客户端池/会话接线尚未完成，原前端/九包不变。
- [x] Codex Provider 接入按能力指纹隔离的客户端池：消费 overrides/env、成功设置 roots 后绑定会话，配置切换保留其他会话进程，新 generation 恢复 roots，统一停止回收并锁存错误。新增 13 项普通测试，组件/带空格路径 96/0/6、既有六项参考、workspace 967/0/48、严格检查通过，见 [客户端池验收](../testing/harness-codex-pool-20260915.md)。原前端/旧九包与 manifests/lock 不变；thread 持久化、完整 resolver/Agent/HTTP、Qoder 和新包仍开放。
- [x] 会话 owner 连接客户端池与 thread/start/resume，按 generation 恢复并持久化原映射；reset/stop、取消等待、失败重试和 Unix 文件规则经本地测试。新增 12 项普通测试、六组原会话参考和 Python 2/2，组件 108/0/7、workspace 979/0/49、严格检查通过；9 月 17 日复核来源，见 [会话验收](../testing/harness-codex-sessions-20260917.md)。未完成聊天流/审批、整体 workspace 所有权与发布包。
- [x] 持久会话接入原 turn/start、完整原始通知与 interrupt，覆盖 start 回复前取消、过滤、Drop、背压和订阅落后；新增 10 项普通测试，组件/带空格目录 118/0/7、七项参考/严格检查通过。首次 workspace SDK 偶发关闭超时失败，同一失败二进制定点复测及原参数整组复跑后 989/0/49 通过，根因未修复，见 [turn 协议验收](../testing/harness-codex-turn-20260917.md)。原聊天事件转换、审批上下文、Qoder、HTTP/Agent 和新包仍未完成。
- [ ] Rust 外部进程与 Agent 全执行链、原 UI 和九包验收；子步骤见专项方案。

#### NvQ0h0 原前端全组回归

对本批 2940 个构建来源和九包，按原交互等价方案执行全部显式项与原 Console 单测；源码中的 42 个显式项含浏览器、Python 对照和产品版本文本检查，不统称 42 个浏览器测试。不改原 UI、既有驱动、超时或已交付包，失败先保留证据并诊断，不以成功子集替代整组结果。

- [x] 核对当前 2940 个构建来源、50 个额外脚本、42 个显式项和九包哈希，测试后再次复核一致。
- [x] 优化版 App Server 全部 41 项及 CLI Debug 原页面 1 项串行通过，分别 366.08 秒、15.27 秒；合计 31 个浏览器场景、10 个 Python 对照、1 个源码文本检查。
- [x] 原 Console 295 文件/2453 项全量单测、共享 DevTools/关闭诊断 16 项通过，无失败或跳过。
- [x] 逐项结果和来源复核写入 [验收记录](../testing/full-ui-official-catalog-20260915.md)；QA 汇总器 reporter 前缀解析修正及首次失败日志保留，产品与驱动不变。
- [ ] 完整原功能、插件执行、历史偶发问题、原生/跨平台继续开放。

#### 当前构建前门禁：目录版本与 Git 排除规则

继续原版本等价和九包构建方案：目录版本新增保序数字适配，保留原字符串；当前 96 版本的 9216 次升级、288 条兼容约束对照已通过。全量回归暴露 `exclude_core_identity` 在 Tokio 文件后台写入完成前返回，导致立即读取/后续 Git 命令可能看不到排除规则。先提取仅用于该写入的完成函数，借助单线程文件线程池阻塞确定性复现，再补 `flush`；不重写用户规则，不将 `flush` 宣称为掉电持久化。

- [x] 目录版本红绿、原 Python 交叉对照与原官方页面专项通过。
- [x] Git 排除规则确定性红绿、延迟写入错误、原 Git 测试和完整 workspace 871 项通过，严格检查无降级；见 [源码验收](../testing/catalog-versions-git-write-20260915.md)。
- [x] 新目录构建九类开发包，旧九包、两处 staging 和旧 source Core 均保留可恢复；逐包静态检查、TS 26/Python 37、两 VSIX 各 24、legacy CLI 855+36 通过。未执行包内 Core 或启动原生界面，见 [新包验收](../testing/qa-official-catalog-packages-20260915.md)。
- [ ] 完整原功能、异常元数据、原生/跨平台和其余原有未完成门禁继续开放。

以下已勾选项是此前阶段的基线记录，不表示 §14.2.24 后续变更已完成最新 Windows/Linux 实机或完整原生窗口验收；当前源码与旧制品的边界以 §38–44 及各自验收记录为准，不能把早期客户端勾选项视为最新全功能分发通过。

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
