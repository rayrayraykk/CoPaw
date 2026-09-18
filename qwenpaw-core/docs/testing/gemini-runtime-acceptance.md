# Gemini 原生运行时验收

日期：2026-09-09。对应计划 §14.2.24.31；仅使用隔离目录、内存凭据和 loopback HTTP fixture，不使用真实 Google key。

## 已实现与专项证据

- Core 原生 GenerateContent 请求和 SSE：systemInstruction、contents、functionDeclarations、functionCall/functionResponse，模型资源名、代理前缀与 `/v1`、`/v1beta` 地址。
- API key 放在 `x-goog-api-key`，不放 URL query；provider identity、headers、默认参数和单模型参数进入现有 turn 快照。
- 保留原 provider 的 token/thinking 参数映射，处理 tool choice、原生配置 snake_case/camelCase 及工具 schema 引用、nullable、const。JSON Schema 的业务属性名保持原样；递归/展开有界。
- 原生 parts 与 thoughtSignature 原样持久化、回传；模型没有给 ID 时生成本地 ID，但不将其写入下一轮原生 parts/response。并行工具调用和响应保持各自分组与顺序。
- SSE 处理分片 UTF-8、隐藏 thought 文本、空文本签名、累计 token/cache 使用量；等待 EOF 读取尾部 usage。缺少完成原因、安全拒绝、流错误、重复调用、过大数据明确失败；待执行工具不因已有部分响应而提前执行。
- 原模型 API 选择后的实际文件读取、重开 SQLite、备份恢复、单模型参数覆盖、错误和取消通过本地 HTTP fixture 验证。
- 原模型连接检查及发现使用 Gemini REST；分页 token 编码、重复模型去重和游标循环错误有专项验证。

## 原页面浏览器

`original_gemini_browser_configures_selects_chats_and_reloads` 已专项通过。
原 Console `dist` 加隔离 Chrome；后端 fixture 仅预置本地 Gemini 地址，保留原 `freeze_url=true`。配置 key、添加模型、Chat 搜索选择、发送、展开原 Read File 输出和刷新历史，均由原 UI 控件完成。fetch 只用于读取持久化断言，未用 API 写操作代替 UI。

首轮在 Chat 直接等待测试模型失败：模型不在当前推荐显示范围。读取原 ModelSelector 后改用现有搜索框搜索显示名，再点击同一原模型按钮，专项通过；没有改 Console 组件或推荐逻辑。
另一个初始 fixture 错误是在 Add Model 请求夹带单模型参数，该接口不保存此字段；改用原单模型配置接口设置覆盖参数后，实际请求与恢复检查通过。

## 完整回归与制品

最终检查结果：

| 检查 | 结果 |
| --- | --- |
| `cargo test --workspace --all-features --quiet`（qwenpaw conda 环境） | 384/384 普通测试通过，7 个显式浏览器测试另跑 |
| `cargo test -p qwenpaw-app-server --all-features -- --ignored --test-threads=1` | 7/7 通过，包括两项备份、Market、Anthropic、Gemini、OAuth、OpenRouter |
| Core/App Server/Storage/MCP 全目标全特性严格 Clippy | 通过 |
| `cargo fmt --all -- --check` / `git diff --check` | 通过 |
| Console API inventory 单测与快照 | 3/3 通过，快照一致；370 个调用点、38 个未注册路径仍须按清单处理 |
| release Core `cargo build --release --locked -p qwenpaw-cli` | 通过 |
| TypeScript SDK 构建 / 对新 Core 回归 | 通过 / 3/3 |
| Python SDK 对新 Core 回归 | 4/4 |
| VS Code 扩展编译 / 对新 Core 回归 | 通过 / 57/57 |
| `git diff --numstat -- console/src`（仓库根） | 零改动 |

本切片新增 7 个 Core 普通测试、2 个 App Server 普通测试和 1 个浏览器测试。先完成的工作区运行是 383 项，补充 schema 展开上限与不匹配原生历史测试后，重新执行最终工作区，结果为 384 项；未用旧计数代替最终状态。流错误 fixture 先发出工具调用再返回 error，明确断言该 turn 没有工具执行结果。

源码 release Core SHA-256：`c13b7c571b99fab5f96468d5b4f0741fcf163bd065e15db36381ce9e8b6de9e7`。SDK/VS Code 既有模型回归仍使用 OpenAI 兼容 fixture，证明公共协议未回退，不证明独立 SDK 进程已共享 Desktop 的 Gemini 配置。

此前九类 QA 包位于 `dist/qa-runtime-20260908-yv7Wee/`，不含本次 Gemini 实现；本轮没有将源码 Core 回归标记为安装态通过，详见 [逐包报告](qa-runtime-packages-20260909.md)。所有本轮临时测试服务已结束。

## 未完成范围

图片、文档、视频等聊天输入输出，reasoning 的原页面展示，高级 provider/Agent 设置、真实账号、跨进程 provider 配置同步仍需逐项对齐。后续 §14.2.24.32 已补充 Gemini 模型页多模态探测，见 [探测验收](gemini-multimodal-probe-acceptance.md)，不能据此关闭实际聊天媒体输入。macOS 分发 Core 启动失败、完整安装态 GUI/桥接、正式签名/公证、全部产品功能门禁均未因这些切片通过而关闭。

实现参照原 `src/qwenpaw/providers/gemini_provider.py`、本机 AgentScope Gemini model/formatter，以及官方 [GenerateContent API](https://ai.google.dev/api/generate-content)、[thought signatures](https://ai.google.dev/gemini-api/docs/generate-content/thought-signatures)、[Models API](https://ai.google.dev/api/models)。没有引入 Python 运行时或将 Agent 循环放入 SDK。
