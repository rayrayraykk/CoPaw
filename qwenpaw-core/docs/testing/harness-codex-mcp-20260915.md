# Codex MCP 独立 CLI 发现验收

2026-09-15；按 [Harness 方案](../architecture/harness-runtime-parity.md) 接入 Provider 只读 MCP 发现及短命进程所有权。没有接入七个 HTTP 路由、放开 Agent backend 或修改原前端。

## 实现

- [x] 实际产品命令为解析后的 Codex binary 加 `mcp list --json`；program/argv 分开传递，cwd 使用请求 workspace，环境来自 Provider 显式 host snapshot，不拼接 shell。无需先启动 app-server；缺失安装保持原 adapter 的空数组行为。
- [x] 完整只读 DTO 保留 name/provider_id/transport/enabled/auth_status/read_only/scope；保留顺序和重名项，过滤配置、地址、环境变量等私有字段。缺失 enabled 为 false，与 Skills 默认值不同。
- [x] 非零退出保留原 stderr UTF-8 replacement/trim 错误文案，无效 JSON 保留原提示；合法非数组 JSON 返回原空列表，结构映射错误不返回部分结果。
- [x] 独立有界所有者队列容量 16，最多 8 个并发直接子进程；与 app-server RPC 分开，不因挂起 MCP 发现阻塞账号调用。
- [x] 同时读取 stdout/stderr 并等待 child；分别限制 8 MiB/1 MiB，超限明确报错。stdin 为 null，不让只读发现消费调用方终端输入。
- [x] 请求取消、超时、输出错误时关闭读管道并 kill/wait；显式回复前完成直接子进程回收。回收失败返回 `McpCleanup` 并锁存，所有者不再启动新进程；worker panic 也锁存错误。
- [x] Provider stop/shutdown 同时处理 app-server 与 MCP 所有者，并等待两者。stop 可重新使用；shutdown 处理后关闭所有克隆入口。最后句柄 Drop 请求异步清理，不宣称 Drop 同步回收。

生命周期 shutdown 仍非抢占已经接受的队列操作；文件发现和排队时间不计入 MCP 运行 timeout。清理最多另等待已有的 5 秒停止期限。这里管理直接子进程，不宣称覆盖外部 CLI 任意派生进程树、OS 无法回收或运行时整体被强制销毁的情况。

## 原生 fixture 启动问题：未解决

最初新增的标准库 Rust fixture 在临时目录现场编译，但进程未在期限内写入 main 的启动标记；8 项进程测试因此失败。随后在 QA 目录独立编译并直接启动，仍未进入 main。`codesign --verify --strict` 静态校验通过，采样显示主线程停在 `_dyld_start`、physical footprint 96 KiB；这些证据不能确定是签名、系统策略、加载器还是其他原因。

没有复制测试二进制、修改签名/xattr、调整系统安全策略或增加测试超时来掩盖失败。受控诊断进程到期后被终止并等到退出。原 Rust fixture 源码、编译产物、失败日志和采样输出保存在 `dist/qa-harness-mcp-20260915-h8FAer`，其中 `native-fixture-source.rs`、`mcp-fixture`、`fixture-direct.log`、`fixture-diagnostic.log` 可复核。历史复制二进制启动问题与本次是否同因也未知。

当前 MCP 进程测试替身改为 `tests/support/mcp_fixture.py`：用 conda qwenpaw 的 Python 加 `-I` 启动独立标准库脚本，环境清空后只传测试 mode/必要 Windows 系统项，不导入 QwenPaw/AgentScope/SDK，不读取真实配置或凭据。测试命令工厂替换真实 Codex executable，但 Rust 的进程启动、双管道、限长、超时、取消和 kill/wait 全部实际执行。

这使普通组件进程测试需要 qwenpaw Python 测试环境；Python 仅存在于 `#[cfg(test)]` 路径及参考程序，不是 Rust 产品 fallback。其他已有 app-server fixture 仍通过当前 Rust 测试二进制运行。MCP 替身通过不证明新原生 CLI 能在本机启动，更不证明真实 Codex 已通过。

## 测试证据

新增 10 个 MCP 模块普通测试、2 个 Provider 普通测试；另扩展既有未安装测试。覆盖完整命令、DTO、双管道、错误、输出限长、超时、仅关闭 reply 的取消、最大并发容量、stop、终态 shutdown、Drop、spawn 失败及故障锁存。故障锁存使用注入的清理错误，不是假装制造了真实 OS kill/wait 失败。

Provider 测试证明 MCP 发现不隐式启动 app-server；另在 MCP 挂起期间查询账号，再 shutdown 同时回收两类进程。命令工厂测试与完整生产命令结构断言分开，不能统称实际 Codex CLI 验收。

扩展原控制参考程序提取未改动的 `discover_mcp` 方法，使用显式 subprocess double；七组完整对照包含无安装、空数组、null、对象、混合完整字段及重复项、非法 JSON、非零退出/无效 UTF-8 stderr。比较整个调用 program/args/cwd/stdout/stderr 和返回值/错误，而非仅 DTO 字段子集。参考程序本身新增 4 个 Python 单测，连同既有控制/Skills 共 9 项。

| 检查 | 结果 |
| --- | --- |
| 新 MCP / Provider 普通测试 | 12 passed、0 failed |
| 全组件及带空格源码构建目录 | 各 73 passed、0 failed、5 ignored；含 1 个旧 Rust fixture 入口 |
| 显式原方法参考 | 5/5；含 MCP 7 组及此前 Skills/控制/发现/完整 Provider 对照 |
| Python 控制/MCP 参考单测 | 9/9 |
| 全 workspace | 944 passed、0 failed、47 ignored，含 2 个 doc tests |
| 静态检查 | workspace/all-targets Clippy `-D warnings`、cargo fmt、Black、79 列 Flake8 |

47 个默认忽略项中五项参考另行通过；此前其余 42 项 UI/参考测试本轮未重跑。全 workspace 通过后，仅为满足 Black/Flake8 将两处测试脚本的等价字节拼接改成 join，随后重跑组件普通组、带空格目录及 Python 单测/格式检查；Rust 产品代码没有再改。

早期构造字段遗漏、Clippy 类型复杂度/测试函数提示和参考程序将 `OsStr` 直接序列化为平台枚举的问题已修正，失败日志保留。未放宽 lint；既有能力 DTO expectation 与 Python F541 例外保持，不新增抑制。

最终命令、源码及九包校验于 `2026-09-15T00:52:05.553Z` 通过，见 [verification.json](../../../dist/qa-harness-mcp-20260915-h8FAer/verification.json)。新 Rust fixture 启动验证仍明确为 false，不能从 `verified` 推导完整产品通过。

## 剩余门禁

Cargo.lock/manifest 不变；原 2940 个构建来源相对 NvQ0h0 仍仅有前轮加入 Harness 成员对应的 manifest/lock 差异，其余旧来源、原 Console 和九包/source release Core 不变。本轮未重建安装包、执行包内 Core、启动原生桌面、激活 VS Code、使用真实账号、commit 或 push。

还未完整覆盖原 Python 任意数值/容器字符串转换、UTF-16 JSON 自动探测和特殊 Unicode trim；Windows/Linux 实机及特殊路径也未验证。超时/容量错误暂沿用共享错误类型，完整 adapter/HTTP 错误映射仍须对齐原页面，不能直接把内部错误文本当最终 UI 契约。

- [ ] 新原生 executable 启动根因、真实 Codex/Qoder 及各平台验证。
- [ ] MCP/Skills capability 投影、fingerprint/session 所有权与会话恢复。
- [ ] Codex/Qoder 聊天、命令、附件、审批、取消及原 Agent 生命周期接线。
- [ ] 七个 HTTP 路由、原 UI 全流程、新九包构建及逐一验收。

总 goal 未完成；本轮仅关闭有明确测试证据的 MCP Provider 发现与直接子进程管理切片。
