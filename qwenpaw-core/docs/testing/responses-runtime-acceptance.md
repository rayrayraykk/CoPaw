# Responses 原生运行时验收

日期：2026-09-09。对应计划 §14.2.24.36。原 Console 源码未改动；Rust Core 负责模型调用、工具执行和持久化，SDK 不承载 Agent loop。

## 差异与实现

原 `OpenAIResponseModel` 可配置和探测，但 `desktop_models::provider_runtime` 没有 Responses 分支，会按 Chat Completions 请求。现已增加 `ModelProtocol::OpenAIResponses`，保留 Base URL 代理前缀，使用 `/responses` 与 Bearer/custom headers，并沿用 Provider 和单模型参数覆盖。

请求使用原生 input、function_call/function_call_output 和 call_id；文本/图片依照原顺序编码。工具声明扁平化并保留 schema，不把可选字段自动变成必填；缺省明确 `strict:false`。`max_tokens`/`max_completion_tokens` 映射到 `max_output_tokens`，显式输出上限优先；原 provider 的 disable_thinking 模型白名单保持不变，不猜测其他模型能力。

Rust 持有历史，固定 `store:false`，请求 `reasoning.encrypted_content` 并保存/回传原生 output items。配置不允许覆盖 input/instructions 或通过 previous_response_id/conversation/store/background/truncation 替换本地状态边界。原生工具历史校验 ID、名称、参数与 Core 调用记录一致；公开 Thread 不携带加密推理内容。

实现依据 OpenAI Docs 的 [函数调用](https://developers.openai.com/api/docs/guides/function-calling) 与 [Responses 迁移说明](https://developers.openai.com/api/docs/guides/migrate-to-responses)。使用的是本地 fixture 模型名与假 key，没有请求真实推理服务。

## 流与失败边界

- 复用 SSE 分帧、UTF-8、单事件 256 KiB 和空闲超时；整个 Responses 流累计最多 4 MiB，最多 4096 个 output items、128 个函数调用。
- 文本和 refusal 实时显示，completed 输出补齐尚未收到的尾部；完整输出必须匹配已显示文本及 output_item.done。
- 工具只从有效 completed 的完整 output 中产生。未完成、失败、取消、连接截断、ID 改变、重复 call ID、非法参数或 usage 失败均不释放工具调用。
- input_tokens、output_tokens、cached_tokens 校验后计量，不重复叠加 output_tokens_details.reasoning_tokens；未返回 usage 不猜测。
- reasoning/output items 存入已有 provider_content，跨工具步骤与数据库重开继续使用；其他协议不会把这些私有字段发进 Chat Completions。

## 验证

- 五个 Core 测试通过：完整原生历史/图片/工具结果形状、生成参数和本地状态保护、完成时调用/usage、畸形/错误边界、分片中文/拒绝/截断/超时。
- 一个 App Server 集成测试通过：原 Provider 配置和选择、真实 loopback `/responses` 请求、Rust `read_file` 工具往返、图片输入、数据库重开、原生历史复用、错误不执行工具及取消完成；使用内存凭据库和临时工作区。
- 原页面 Responses 浏览器场景通过：配置、Add Model、Agent 选模型、原文件上传、回复/工具卡片、刷新历史/图片。测试保留原冻结 URL 行为，仅在隔离后端预置 loopback 地址；断言两次真实 Responses 请求均携带所上传图片。
- 浏览器首轮发现假 key 不满足原 `sk-` 校验，以及新测试 flag 未从导航路径剔除；已修复测试数据和驱动。随后发现原保存动作会访问模型目录，fixture 缺少 `/models`，已补齐。以上没有修改或绕过原前端。
- 首轮全工作区回归在 MCP 取消测试的 3 秒窗口超时，已单独复测通过（0.58 秒），无该测试 helper 残留；没有修改该测试或放宽超时。完整回归正在继续，首轮失败不抹除，也不据此断定根因。
- 全工作区完整复测 402/402 普通测试通过；四个主要 crate 的 all-targets/all-features 严格 Clippy、fmt、diff、inventory 3/3 和快照通过。
- 八个显式浏览器门禁首轮 7 通过、1 失败。Responses、Anthropic、Gemini、OAuth、OpenRouter、Market 和备份 active/reload/cancel 通过；原备份 roundtrip 在创建后未出现表格行，仍停留在创建弹窗。同一实现独立复测通过（66.27 秒），包含 24 页导航和完整创建/导出/导入冲突/外来信任/恢复/删除。未修改备份实现、驱动或超时，原因尚未确认；不能把复测通过称为间歇性问题已修复。
- 新 release Core 构建通过，SHA-256：`76c1903956bffce66c2080b7f7d3f99891ed90ed3868280528afb61cc900561d`。
- 显式连接上述 release Core 的 TypeScript SDK 4/4、qwenpaw conda Python SDK 5/5、VS Code 57/57 回归通过；SDK/扩展编译通过。它们验证既有 App Protocol/图片等回归，不声称独立 SDK 已新增 Desktop Provider 配置接口。

## 仍未完成

这不是全部 Responses/原产品功能等价：原推理展示、托管工具、音频/视频/文档及远程媒体、其他高级参数、独立 SDK 与 Desktop 配置同步、真实账号和第三方兼容服务仍需验收。

旧九类 QA 包尚未包含本轮 Responses 和上一轮桌面生命周期修复。完整 Tauri/WKWebView、安装态、冷启动稳定性、生产签名/公证、Windows/Linux 实机以及其他原功能仍未关闭。此文不把配置可保存、测试路由存在或构建成功视为全功能完成。

后续 §37 已在 `dist/qa-runtime-20260909-OPX10W/` 重建九类包并执行安装态检查，见[制品验收](qa-responses-packages-20260909.md)。首轮 archive Core/Python SDK 启动失败、同路径复测通过分别保留；完整 GUI、首次安装稳定性和其余全功能门禁未关闭。
