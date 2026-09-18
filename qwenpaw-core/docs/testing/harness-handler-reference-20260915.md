# Harness 原 handler 契约基线

2026-09-15；对应 [专项方案](../architecture/harness-runtime-parity.md)。确认当前 Rust 缺少原 Codex/Qoder backend 执行链后，新增原 HTTP handler 参考程序和测试，为后续 Rust 接线提供完整响应及调用参数基线。本阶段没有产品运行时代码变更，没有将 Harness 标记为已实现。

## 验收 checklist

- [x] 参考程序直接加载原 `routers/harnesses.py`、registry、events 和 base 定义，不启动原产品。
- [x] 原七个接口共 29 次 HTTP 请求，7/7 单测通过、无跳过；最终执行 2.120 秒。
- [x] 断言完整 HTTP 状态、JSON 响应、依赖调用及顺序，不只检查响应存在或数组长度。
- [x] Black 显式 Python 3.11 target / 79 列及 Flake8 检查通过；执行环境为 Conda qwenpaw 的 Python 3.12。
- [x] 2940 个原构建来源、既有 50 个脚本、source release Core 与九包全部保持原哈希；新增的两个参考/测试脚本单独记录 SHA-256。
- [ ] Rust Harness 子进程协议、完整运行时、Agent 接线、原页面浏览器及分发验收仍待完成。

## 实际覆盖

| 测试组 | 请求数 | 已证明的原 handler 行为 |
| --- | --- | --- |
| 未知与计划中 provider | 12 | 六个 provider 入口先返回 404/409，不解析 workspace |
| Models | 3 | 完整默认字段、当前 backend 的已保存配置、其他 backend 空配置、选择另一测试 workspace |
| 未安装与能力不支持 | 4 | 三种空集合含 message；Qoder MCP 不创建 adapter、也不解析 workspace |
| MCP/Skills | 2 | 完整只读字段及传给 adapter 的已解析 workspace 路径 |
| 登录/注销 | 4 | 未保存配置、设备码默认/显式值、原登录响应、成功注销、结构化不支持错误 |
| Status | 2 | 未保存配置/默认空配置、保留实时状态字段、registry 覆盖 adapter capabilities 的完整响应 |
| List | 2 | 只将当前 backend 的已保存 settings 传入 runtime.providers |

程序使用 httpx ASGI 传输，不开网络 socket；原 runtime、外部 adapter 和 workspace resolver 均为显式替身。`x-test-agent` 只属于参考程序，不是新增产品协议。这些测试证明 handler 参数选择和序列化，不证明真实 workspace 鉴权/路由、provider 目录实时状态、账号、子进程、模型执行或完整多 Agent 隔离。后续 Rust 必须分别验证这些链路，不能复制替身成为生产实现。

记录路径通过 `Path.relative_to(...).as_posix()` 归一化隔离临时目录，避免把 POSIX `/` 当作 Windows 路径分隔符；本机执行结果仍不是 Windows/Linux 运行证据。

## 命令与证据

在 `qwenpaw-core/`、Conda `qwenpaw` 环境执行：

```sh
python -m unittest discover -s scripts/tests -p test_harness_reference.py -v
python -m black --check --target-version=py311 --line-length=79 scripts/harness_reference.py scripts/tests/test_harness_reference.py
python -m flake8 --extend-ignore=F541 scripts/harness_reference.py scripts/tests/test_harness_reference.py
```

用户要求字符串仅使用 f-string，因此 Flake8 命令仅排除与该要求冲突的 F541，未更改仓库规则或忽略其他错误。首次 lint 的 F541、随后格式化产生的两条 E501 及修正记录均保留；两条超长字符串已拆分。Black 最初自动目标版本高于当前解释器，最终显式 `py311`，不使用 `--fast` 跳过安全检查。

完整日志及终态位于 `dist/qa-harness-contract-20260915-j4wsFj/`；[最终校验](../../../dist/qa-harness-contract-20260915-j4wsFj/verification.json) 于 `2026-09-14T22:44:48.434Z` 通过。当前九包未重建、未覆盖；原前端未改，未使用真实账号/Keychain、未执行外部 Harness CLI 或包内 Core、未启动原生窗口、未 commit/push。总 goal 未完成。
