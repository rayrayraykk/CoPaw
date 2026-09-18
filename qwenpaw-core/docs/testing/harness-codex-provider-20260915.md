# Codex Provider 控制入口验收

2026-09-15；依据 [Harness 架构与 checklist](../architecture/harness-runtime-parity.md)，组合已有文件发现、进程所有者及控制面。原 Console 未修改；本轮不接入 HTTP 或放开 Agent backend。

## 已实现

- [x] 原 Codex、Claude Code、Qoder 目录顺序、完整 capabilities、命令和审批 presets。Claude 保留 coming-soon；目录声明不等同于 Rust 已能执行对应能力。
- [x] `CodexProvider` 提供完整状态、模型、浏览器/设备码登录、注销、可重启 stop 和终态 shutdown。状态包含实际探测路径/来源、账号白名单及原错误提示。
- [x] 缺失文件不启动进程；无账号仍保留 installed；远端协议失败仅暴露原 message，不包含 code/data 中的私密信息。启动 I/O 错误向上传递，不伪装成未安装。
- [x] 主机 cwd/home 必须绝对路径，环境和 bundled candidate 显式传入；构造前验证，不读取日常账号。含环境的 settings 不实现 Debug。
- [x] 文件发现使用阻塞任务；每次状态重新探测，需要新进程时再次解析 binary。已有活进程不因路径探测变化自动换绑；正常 stop 后下次 start 采用新结果。

`LaunchConfig.binary` 改为可未解析状态；真正启动前发现失败返回 `NotInstalled`，没有填造占位路径。settings 改变后的 workspace 锁、整个 Provider 替换和 capability fingerprint 会话管理仍由后续完整 adapter 实现。

## 测试及参考边界

9 个 Provider 普通测试覆盖目录身份、非法主机上下文、缺失状态、完整控制流程、无账号/协议错误、动态 PATH 切换、候选删除及恢复、启动 I/O 错误和独立所有者隔离。

控制链路实际使用 Rust 子进程、生命周期队列、初始化和双向管道，但测试命令工厂把启动参数替换成当前 Rust fixture 入口。动态路径测试的候选是不会被执行的合成可执行文件：工厂记录解析路径后启动 fixture。因此这证明选择与生命周期集成，不证明对应真实 CLI 已执行。

Python 参考程序加载原 `events.py`，从原 registry 提取未改动的目录定义，从原 Codex adapter 提取未改动的 status 方法，再按原 router 覆盖 capabilities。安装发现和账号客户端为显式替身，不导入产品初始化器或运行真实 Codex。比较整个三项目录，以及缺失、已登录、无账号、远端错误四份完整状态结构；不是只比较字段子集。原模型/账号控制面 13 组及文件发现 18 组对照也重新执行。

## 验证结果

| 检查 | 结果 |
| --- | --- |
| Provider 普通测试 | 9 passed、0 failed |
| 整个组件普通组 | 54 passed、0 failed、3 ignored，含 1 个 fixture 入口 |
| 带空格目录源码构建/执行 | 同组 54/0/3，23.68 秒 |
| 显式原实现参考 | 3/3 测试，10.48 秒；包含完整三项目录、四类状态及此前 31 组发现/控制案例 |
| 新 Python 参考程序单测 | 4/4，1.031 秒；Black、79 列 Flake8 通过 |
| 全 workspace | 925 passed、0 failed、45 ignored，含 2 个 doc tests |
| Rust 静态检查 | cargo fmt、workspace/all-targets Clippy `-D warnings` 通过 |

45 个默认忽略项中的三项参考已另行执行；其他此前 42 项原 UI/参考测试未在本轮重跑。没有把忽略项算成通过，也没有据此宣布完整原交互一致。

首次编译的测试数组所有权错误已修正，首次 Clippy 和 Python 行长问题的失败日志保留。`HarnessCapabilities` 的 19 个独立布尔字段必须映射原公开 wire contract，故仅在该 DTO 上添加带理由的 `clippy::struct_excessive_bools` expectation；`ProviderStatus` 使用 flatten 复用目录声明，未新增第二处抑制，也没有全局放宽 lint。Python Flake8 仅忽略统一 f-string 规则导致的 F541。

[最终验证记录](../../../dist/qa-harness-provider-20260915-LNSNSO/verification.json) 于 `2026-09-15T00:16:34.547Z` 通过，命令与失败/成功日志位于同目录。

## 剩余门禁

文件系统/NSS 探测没有总超时；放入阻塞任务不等于所有启动步骤都有截止时间。沿用前轮清理失败锁存及 generation 绑定规则，上层恢复、重订阅和 UI 故障提示仍待实现。

缺失提示保留原 `qwenpaw[codex]` 文案仅为响应等价，不引入 Python 产品运行时；自动 bundled candidate 分发/发现仍开放。Provider models 缺失时返回 `NotInstalled`，未来 HTTP router 必须先做原 capability precheck，返回原空 models 和 message，不能把此组件直接称为已完成原路由。

本轮 Cargo.lock/Cargo.toml 未变，原 56 个脚本未变；相对 NvQ0h0 的 2940 个构建来源仍只有前几轮新增 Harness workspace 成员所需的 manifest/lock 差异，其余来源及原 Console 不变。九包和 source release Core 哈希不变，未重建，仍不包含本组件。

- [ ] 完整 Codex adapter：MCP/Skills、能力 fingerprint、会话/线程映射、恢复、聊天/命令/附件/审批/取消。
- [ ] Qoder 协议与 adapter；七个 HTTP 路由及 Agent 生命周期/执行接线。
- [ ] 原 UI 全流程对照、重建全部受影响包并逐一测试。
- [ ] 真实账号、原生桌面/VS Code 激活、包内 Core 和跨平台验收。

没有运行真实 Codex/Qoder、访问系统凭据、执行包内 Core、绕过安全策略、commit 或 push。复制测试二进制的历史启动问题根因仍未确认；总 goal 保持未完成。
