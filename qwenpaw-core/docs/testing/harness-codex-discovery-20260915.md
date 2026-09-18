# Codex 只读文件发现验收

2026-09-15；按 [Harness 专项方案](../architecture/harness-runtime-parity.md) 在 `qwenpaw-harness::codex::discovery` 实现可执行文件查找。输入为调用方显式提供的绝对 host cwd/home、环境映射与可选 bundled candidate；不捕获或修改进程全局环境，也不将 Agent workspace 偷换成 host cwd。

## 本轮实现

- [x] configured → CODEX_BINARY → bundled candidate → PATH → standalone，保留完整 canonical path 和来源标签。
- [x] 显式无效选择立即返回未找到；只有原字面值 `codex` 可以继续查下一来源。显式 `codex` 成功命中 PATH 时仍标记 configured，而不是 path。
- [x] 相对路径、当前用户 tilde、带空格目录、目录/不可执行文件拒绝、符号链接规范化；Unix 使用已锁定 nix 的安全 `access(X_OK)`，不是只猜 mode 位。
- [x] 配置/PATH/standalone 拒绝 `.app` 与 `openai.chatgpt-*` 路径；检查规范化目标，符号链接不能绕过。首个 PATH 可执行文件为内嵌运行时时转查 standalone，不继续挑后一个 PATH 可执行文件。
- [x] 保留原 SDK candidate 分支允许其所属 embedded executable 的例外，但必须由 caller 显式提供，不自动扫描编辑器安装目录。
- [x] Windows standalone 默认位置、LOCALAPPDATA 空值回退、PATHEXT 后缀顺序及当前目录 opt-out 的纯逻辑测试。

Windows 当前目录规则参考本机 CPython 3.12 `shutil.which` 与 [Microsoft 文档](https://learn.microsoft.com/en-us/windows/win32/api/processenv/nf-processenv-needcurrentdirectoryforexepathw)：对裸命令看 opt-out 变量是否存在，而非其内容。本实现对调用方环境映射应用该规则，不声称调用过 Windows API。macOS 缺失 PATH 的默认列表另与本机 Python `os.confstr("CS_PATH")` 核对，为 `/usr/bin:/bin:/usr/sbin:/sbin`。

## 原实现对照

`scripts/codex_discovery_reference.py` 直接加载原 `src/qwenpaw/harnesses/codex/discovery.py`，运行未经修改的 resolver 与默认目录函数。它只在隔离 Python 子进程中设置 fixture cwd/home/env。未提供 bundled 路径时传入明确不存在的 candidate，避免原函数 import 可选 SDK；因此这里不验证 SDK 包的自动发现。

18 组案例逐项比较完整 path/source 或 null，覆盖来源优先级、显式失效、原 `codex` 回退、PATH 遮蔽、embedded 例外、目录、相对路径、tilde 和前置 `./`。另有 Python 4 项完整结构单测，包含不可执行文件。fixture 文件是不能当作真实 Codex 使用的测试文本；发现过程中不启动它们。

同轮重跑上一控制面 13 组对照，防止新依赖与模块接入破坏已有结果。两项显式 Rust 参考测试共 31 组案例通过。普通组件测试中的真实子进程仅为已编译 Rust fixture，不是真实 Codex。

## 检查结果

| 检查 | 结果 |
| --- | --- |
| 本轮普通发现测试 | 9 passed、0 failed |
| 完整组件普通组 | 33 passed、0 failed、2 ignored；包含 1 个 Rust 子进程 fixture 入口 |
| 带空格目录源码构建/执行 | 完整普通组 33/0/2，8.27 秒 |
| 显式原 Python 对照 | 2/2 测试，18 组发现 + 13 组控制面，7.64 秒 |
| Python 参考单测 | 4/4，1.014 秒，Conda qwenpaw |
| 完整 workspace | 904 passed、0 failed、44 ignored，含 2 个 doc tests |
| 静态检查 | workspace/all-targets Clippy `-D warnings`、cargo fmt 通过 |
| Python 格式 | Black py311/79 未产生修改，Flake8 通过；仅因全 f-string 约束忽略 F541 |

44 个默认忽略项包含本轮已显式运行的两项参考及此前 42 项验收；此前 42 项浏览器/其他参考未在本轮重新执行。不能将 904 或 31 组案例等同于原 UI/完整 Harness 已验收。

首次编译的默认 PATH 生命周期错误，以及随后 Clippy 报告的条件折叠、测试辅助函数多余 Option 和过长测试函数均已修正。失败日志保留，没有关闭 lint 或放宽现有产品测试超时。带空格构建复用前一 QA 的缓存目录，重新编译当前源码，未复制或重新签名测试二进制。

[最终校验](../../../dist/qa-harness-discovery-20260915-kHLjhz/verification.json) 于 `2026-09-14T23:39:56.442Z` 通过；日志、命令终态与源码哈希位于同一目录。

## 剩余边界与制品

纯 Rust 不 import `codex_cli_bin`。当前只接收分发层给出的 bundled executable 和真实来源标签：SDK 来源可保留 `python-sdk`，其他分发来源不能冒充它。自动定位/打包该 executable 尚未实现。本组件未被 App Server/Agent 调用，完整 provider installed/status、启动配置更新和运行时所有权仍待接线。

仅验证 macOS ARM64；本机 `rustup target list --installed` 也只有该 target。Windows 原生查找/权限/大小写、canonicalize 的 verbatim/UNC 表示与长路径执行仍需原实现对照及实机验收；其他用户名 tilde、特殊文件与异常环境等边界未完成全面对照。Linux 默认 PATH/执行权限实现不能从 macOS 结果推导实机通过。

lock 仅在本地 `qwenpaw-harness` block 增加已锁定的 nix；Unix 启用其 fs/user features，无外部依赖版本变化。相对 NvQ0h0 原 2940 个构建来源，除此前 workspace 成员对应的 Cargo.toml/Cargo.lock 外一致，54 个既有脚本一致；新源码/脚本单列哈希。控制面文件、原前端、九包及 source release Core 均未改变。

九包不含本轮发现组件或前两轮通信/控制面，未重建；没有执行包内 Core、启动原生 Desktop、激活 VS Code、访问真实凭据、commit 或 push。复制测试可执行文件的历史启动问题仍未确认根因。

- [ ] bundled 分发定位、启动/配置替换生命周期与完整 provider 状态。
- [ ] Qoder 协议和两类 Agent 的聊天、审批、取消、附件、命令、会话恢复。
- [ ] 原 UI 对照、全部受影响制品重建和逐包测试。
- [ ] 真实账号、原生与跨平台验收；总 goal 未完成。
