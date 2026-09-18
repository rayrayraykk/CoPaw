# Ollama 地址与模型检查验收

对照原 `src/qwenpaw/providers/ollama_provider.py`，Rust Desktop provider adapter 保存服务根地址，实际 API 使用根地址加 `/v1`。无须 AgentScope Rust SDK，也不调用 Python AgentScope。`console/src` 不改动。

## 本机执行

在 `qwenpaw-core` 运行：

```sh
cargo test -p qwenpaw-app-server --all-features ollama
```

新增测试使用随机本机端口、临时 workspace 和内存凭据实现；不会下载模型、访问生产 key 或读取真实钥匙串。

- 根地址、尾斜杠、已有 `/v1`、反向代理前缀的转换，以及其他 provider 地址不变。
- 原 HTTP 路由请求的临时连接测试、保存配置、发现并保存模型、全局选择模型；fixture 只开放 `/proxy/v1/models` 与 `/proxy/v1/chat/completions`。
- 模型检查只 GET 模型目录；存在、缺失、HTTP 503 的结果以及 `provider_only` 标记；检查过程不产生推理请求。原内置 Ollama 的目录获取失败返回空目录，因此模型检查仍报告 `model_not_found`，不擅自改变页面状态；独立的 provider 连接检查保留真实 503 与可重试标记。
- 选择模型后，Core 实际启动 Turn、接收流式文本和成功完成事件；重新初始化与 detached restore 后再次实际聊天，共三次成功推理请求。
- 已保存带 `/v1/` 的 registry 在启动和恢复时规范化，运行时仍使用正确的 `/v1` API。
- 备份能够正确匹配根地址和运行时地址对应的凭据；不匹配的地址仍拒绝归属敏感 key；不写入伪造的 Ollama 用户 key。

## 边界

2026-09-09：工作区 357/357 普通 Rust 测试、现有 5/5 原前端浏览器门禁通过；最终错误语义对齐后新增 Ollama 2/2 再次通过。四模块 all-targets/all-features 严格 Clippy、格式/diff、inventory 3/3 与快照校验通过。上述五项浏览器测试覆盖已实现的 Backups、Market、OpenRouter/OAuth，不是 Ollama 专项页面全交互测试。

这是地址、模型检查和基础流式推理链路的测试，不是实际 Ollama 安装/模型性能验收，也不是新 DMG 的验收。现有 QA 包不自动包含本次源码变更。

尚待完成：Ollama 原版全部生成参数/自定义请求头/环境默认值、Agent 级模型设置贯通，其他 provider 原生推理协议，以及所有客户端安装态测试。原前端代码不变不等于所有交互已经验证相同。
