# 模型请求选项贯通验收

对照原 `providers/provider.py::get_effective_generate_kwargs` 和 `openai_provider.py`，把原页面已有的 provider headers/生成参数、单模型生成参数接入 Rust Core 实际请求。没有修改 `console/src`、SDK 协议结构或调用 Python AgentScope。

## 实现范围

- `ModelRequestOptions` 与 URL/key 位于同一运行时锁中；提交配置前校验参数，SQLite 写入成功后才交换整个运行时状态。
- 按实际请求的模型 ID 递归合并 provider 默认参数和单模型覆盖参数，不修改保存的原始对象。`extra_body` 展开到请求体；可选 max_tokens/temperature/top_p 的 null 省略，0 等有效值保留。
- OpenAI 兼容 provider 的 GPT-5/o 系列 max_tokens 映射到 max_completion_tokens，显式后者优先；Ollama 不做此云模型映射。
- 自定义 headers 覆盖默认 headers（包括 Authorization）；拒绝非法 HTTP header、过深/超限参数及 model/messages/stream/stream_options/tools 协议字段覆盖。上游 HTTP 错误中的 key、header 值和 Bearer token 脱敏。
- 活动 provider 配置和单模型设置保存后立即生效；失败时 registry 回滚，Core 不部分应用新 URL/key/选项。
- 启动和 detached restore 使用同一 provider 配置入口。只恢复 Agent 等局部状态且端点不变时保留当前选项；改变端点不继承旧选项。恢复回滚包含完整 volatile 选项。
- SDK 直接改变 Core API base 会清空旧 provider 请求选项；只改模型或 key 不清空。Core 的 SQLite 和公开 `config/read` 不新增敏感 header 字段。

## 测试

在 `qwenpaw-core` 执行：

```sh
cargo test -p qwenpaw-core --all-features model_options
cargo test -p qwenpaw-core --all-features request_headers
cargo test -p qwenpaw-app-server --all-features ollama
```

新增两个参数单测、两个 Core 状态测试和一个真实 HTTP header/error 测试。数据库失败测试在临时 SQLite 中安装失败 trigger，验证整个 snapshot 及运行时配置未改变；并发测试交错 1000 次 provider 更新与 1000 次快照读取，验证 URL/key/header 不混用。没有真实外部 key 或钥匙串访问。

扩展原 Ollama 本机 fixture：配置后实际聊天、活动模型参数更新后聊天、启动后聊天、恢复后聊天、清空参数/header 后聊天，共五次请求；检查深层覆盖、默认参数保留、header 到达和清空后不再发送。非法协议覆盖导致配置保存失败且完整 registry 不变。

## 边界

2026-09-09：最终工作区 362/362 普通测试、现有 5/5 浏览器门禁、严格 Clippy、格式/diff 与 inventory 校验通过。release Core 重建后，TypeScript SDK 3/3、Python SDK 4/4 和 VS Code 57/57 通过（包含真实 Core 连接，未跳过）；TypeScript SDK/VS Code 编译通过。五项现有浏览器门禁覆盖 Backups、Market、OpenRouter/OAuth，不是全部 Models 页设置交互验收。

这不是全产品等价证明。Anthropic/Responses/Gemini 原生推理协议、Agent 级 provider/思考设置、各 provider 特殊参数兼容仍待完成。单独启动的 SDK 进程不会因此自动继承 Desktop 的 provider registry；跨进程设置一致性仍待贯通。现有 QA 包不自动包含源码更新，真实模型服务和正式签名分发仍需安装态验收。
