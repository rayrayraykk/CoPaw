# Channels Workspace 归属验收

日期：2026-09-14，macOS ARM64。方案见 [Channels 归属与架构图](../architecture/channel-workspace-scope.md)。本轮不改变原 Console 源码，不运行分发 Core，不使用真实 key、系统 keychain 或日常数据。

## 缺陷与修复

隔离真实 HTTP 诊断确认：Rust 原 Channels API 忽略已有 `X-Agent-Id`。writer 单项保存、editor 批量保存都覆盖 default/其他 Agent；未知 Agent、停用 Agent、原目录被替换的 Agent 也能保存。重开后覆盖仍在。原前端和 Python 后端均按所选 Agent 请求/保存，属于已批准的等价功能缺口。

生产改动为 Channels API、原生设置解码，以及备份过滤/恢复合并；注册层不变。内部 v2 条目使用类型化 Workspace 标识，生命周期锁串行化注册变更和配置操作，通道锁保护读改写。缺失/停用/目录代际不符分别拒绝；原生 v1 全局配置只归默认 Workspace，读取不重写，正常保存发布 v2。不读取旧 Python 文件。

保留原目录注册到另一个公开 ID 时配置接回，新目录和复制使用独立默认配置。原复制流程清空 Channels 的行为不变。范围备份过滤所选标识，恢复显式重映射来源到本机目标，保留默认和其他未选配置；只恢复全局设置不能顺带导入通道配置。

## 执行证据

输出：仓库 `dist/qa-channel-scope-20260914-ezhatT`。命令日志与 `command-*.json` 退出记录保留。

| 记录 | 结果与范围 |
| --- | --- |
| `diagnostic.json` | 两阶段真实 HTTP 准确复现缺陷；`diagnosticPassed=true`、`productInvariantPassed=false`，不是产品通过 |
| `channel-red-corrected.log` | 0 通过、2 失败；未知身份 GET 返回 200 而非 404，writer 保存改变默认完整配置 |
| `channel-workspace.log` | 完整普通 workspace 757 通过、0 失败、29 ignored；含新增八项普通测试及跨 Core HTTP 备份恢复扩展 |
| `channel-clippy.log` | workspace/all-targets，warnings 作为错误，退出 0；包含后补的全局恢复测试与原页面测试源码 |
| `channel-browser-and-scope.log` | 11/11，0 ignored；十项普通相关测试加一项真实原页面测试，含新补的全局恢复不导入未选通道配置 |
| `channel-browser-harness.log` | 浏览器诊断/关闭工具测试 16/16 |
| `channel-inventory.log` | API 清单提取测试 3/3，快照检查通过；仍有 370 个调用、34 个未匹配项，不把清单通过解释为全功能完成 |

最终逐项核对见同目录 `verification.json`：记录本轮源码 SHA-256、上一批 2,903 条输入的变化范围、原前端零 diff/status、顺序执行的退出记录和仍未更新的 release/制品边界。

首次核对误将设计文档算入打包来源清单，预期八项变化而实际七项，断言退出 1；修正为逐项匹配七个来源文件，并单独记录文档、测试新文件和浏览器脚本哈希后复验。失败日志保留，不改写上一批来源清单。

原页面测试使用原 `console/dist` 与隔离 source App Server，初始配置由测试夹具建立。保存操作全部通过原侧栏、Console 卡片与抽屉按钮完成；辅助 HTTP 只读取完整配置做隔离断言。验证 writer/editor 保存、editor 刷新、切回 writer/default，以及 Core 重开后全部字段相等。没有触发模型请求。

最初诊断把创建 Agent 的成功码写成 200，而原契约是 201；纠正后使用新的隔离目录，保留初次证据。红测试首次因宏导入歧义未编译；纠正后才取得上述真实红结果。后续两次扩展测试的夹具分别遗漏 toggle JSON 对象和创建 Agent 必填 name，修正测试请求后通过；这些不是产品失败，也不隐藏日志。诊断 Cargo.lock 的 383 个注册表依赖均逐项匹配项目锁文件；诊断是项目依赖子集，不能用整个锁文件完全相等作为条件。

## 尚未完成

- [x] 本次变更后的完整显式组 30/30、原前端 2453/2453；新 source Core `7dbb3b3...` 上 Rust SDK 3、安装后 TS 26/Python 37、VS Code 源码 73、两 VSIX 各 24、保留版 CLI 855+36 顺序通过。
- [x] 新九类 `eaTvv4` QA 制品纳入本修复并完成静态/隔离安装验收，见 [下载与制品记录](qa-channel-packages-20260914.md)。上面的 `verification.json` 是构建前源码阶段记录，新包最终记录为该批 `final-verification.json`；旧 `wADUE9` 和 `be7918...` 保留，不用于证明本修复。
- [ ] 包内 Core、原生 Desktop/VS Code 激活、Windows/Linux 实机。
- [ ] 17 个外部通道运行时、凭据、登录、健康检查与投递。原有 501 门禁保持，Console 配置隔离不等于外部通道已经实现。

生命周期锁不限制同权限外部进程修改目录；本轮不声称能消除校验后的任意文件系统替换竞态，也不扩展默认宿主生命周期或模型凭据策略。
