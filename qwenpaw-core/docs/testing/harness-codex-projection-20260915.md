# Codex 能力配置投影与指纹验收

2026-09-15；依据 [Harness 方案](../architecture/harness-runtime-parity.md) 实现已解析有效能力的内存模型、配置投影和值摘要。这不是完整 workspace resolver，也尚未用于实际创建/复用会话客户端。

## 已实现

- [x] `RuntimeCapabilities`、`SkillDefinition`、`McpServerDefinition` 保留原能力信息、工具策略及 credential/runtime revision；含值的整体对象不实现 Debug 或 Serialize。路径使用原生 PathBuf，不做字符串路径拼接。
- [x] stdio 生成 command/args/cwd/env_vars；HTTP/SSE 生成 url/env_http_headers。解析后的 env/header 值单独放入进程环境映射，不在 overrides 中输出这些值的明文映射。
- [x] 原工具白名单保序且保留重复项；deny 过滤、缺失 tools 与空数组差异、默认审批模式及按名称排序的单工具策略全部保留。含点号/Unicode 等不符合原配置键规则的工具不生成单工具 approval override。
- [x] 配置键规范化/散列、HTTP header 环境变量名、截断长度和 SHA-256 大小写保持原规则；不同原名即便规范化后相同也使用各自散列。
- [x] 共享 stdio 环境变量相同值可以转发，冲突值返回原错误且不包含任一值。env 保留输入顺序以报告原第一个冲突变量，env_vars/指纹键列表另行排序。
- [x] 指纹采用原排序、紧凑、ASCII JSON 编码及 SHA-256；列表顺序保留，中文/非 BMP 字符、DEL/换行/引号等按原转义。显示名、技能描述及直接 env/header 值不进入原指纹字段。
- [x] `refresh_runtime_revision` 实现原 resolver 对已解析 env/header 值的摘要。值变化但未刷新 revision 时，指纹按原规则不变；刷新后能区分不同值。resolver 接线时必须调用，不能把摘要计算当成自动凭据监听。

这层是纯内存转换，不读真实凭据、技能文件或配置，也不启动外部程序。没有对原 command/args/url 中可能内嵌的敏感数据做新脱敏承诺；它们仍按原规则参与配置与摘要。未提供整体 Debug/Serialize 也不是内存清零或所有调用方日志安全的证明。

## 对照与测试

新增 5 个能力模型测试和 5 个配置投影测试，验证完整 overrides/environment/roots、摘要变化规则、共享值冲突及顺序、原策略边界、键名区别、重复技能根与非 Unicode 路径拒绝。

参考程序加载原 `capabilities/models.py`，执行原 `codex/projection.py` 函数及原 resolver 的 `_runtime_revision` 方法；只替换导入/凭据输入边界，不改函数体或加载产品启动器。`env_entries` 是测试传输字段，用于在 JSON 对象排序之后恢复原 env 插入顺序，再传入原 Pydantic 模型，不是新增产品 API 字段。

12 组能力输入分别使用原给定 revision 和重新计算的 revision，共 24 组完整对照。包含 stdio/HTTP/SSE、ASCII/中文/非 BMP/空/长配置键、空工具集、共享值及两个冲突场景；其中 20 组比较整个投影/指纹/revisions，4 组比较完整冲突错误。对照不执行 Codex，也不证明生成配置已被真实 Codex CLI 接受。

| 检查 | 结果 |
| --- | --- |
| 新增能力/投影普通测试 | 10 passed、0 failed |
| 全组件普通组 | 83 passed、0 failed、6 ignored，含 1 个旧 Rust fixture 入口 |
| 带空格源码构建目录 | 同组 83/0/6；27.60 秒 |
| 全部显式原实现参考 | 6/6 测试；包含新增 24 组和此前控制/发现/Skills/MCP/Provider 对照；9.75 秒 |
| 新 Python 参考程序单测 | 4/4，完整输出断言 |
| 全 workspace | 954 passed、0 failed、48 ignored，含 2 个 doc tests |
| 静态检查 | workspace/all-targets Clippy `-D warnings`、cargo fmt、Black、79 列 Flake8 通过 |

48 个默认忽略项中的六项已另行执行；其余此前 42 项 UI/参考测试本轮未重跑，不计入本轮通过。早期 Python 测试缺少原模型必填 transport、格式/行长检查，以及 Clippy 排序/长测试函数问题均已修正，保留失败日志，未新增 lint 抑制。最终只调整 Python 单测的预期字符串常量以满足行长，再复跑该单测与格式检查；Rust 产品和原参考程序没有再改。

## 依赖与交付边界

新增直接依赖均已存在于锁文件：sha2 0.10.9 和 indexmap 2.14.1（后者用于保留 env 顺序）。Cargo.lock 仅给 Harness 增加两条依赖边，Harness Cargo.toml 增加对应声明；不升级包版本。不能将本轮说成“所有 manifest/lock 均未变”。

最终源码、命令和旧九包校验于 `2026-09-15T01:13:23.172Z` 通过，见 [verification.json](../../../dist/qa-harness-projection-20260915-7aztdb/verification.json)。原 2940 个旧构建来源及 59 个既有脚本按最新已验证基线复核；manifest/lock 中 Harness 新成员和本轮依赖边单列。原前端、九包及 source release Core 未变，未重建安装包、启动原生桌面、激活 VS Code、使用真实账号、commit 或 push。

既有组件回归仍使用前轮的 Python 标准库 MCP 子进程替身；Python 不进入本轮 Rust 产品逻辑。新独立 Rust fixture 停在 dyld 入口的根因未解，未重试或改变系统安全策略，也不把已有 Rust 单测通过当作这一问题解决。

## 未关闭的门禁

- [ ] 完整 workspace resolver：策略求值、凭据获取/版本、skill 文件 revision 和路径规范化。当前模型接收已解析输入，不模拟 Pydantic 任意原始输入转换；特殊/空/相对路径的上游规范化及 Windows/Linux 实机另验。
- [ ] 将投影用于受控 LaunchConfig，接入 `skills/extraRoots/set`、fingerprint 客户端池、session/线程恢复与错误/清理处理。
- [ ] Codex/Qoder 聊天、命令、附件、审批、取消、七个 HTTP 路由和原 Agent 生命周期。
- [ ] 原 UI 全流程对照、新九包构建及逐一测试，真实账号/原生/跨平台验收。

总 goal 未完成；配置转换与摘要等价不代表客户端池或完整原功能已可用。
