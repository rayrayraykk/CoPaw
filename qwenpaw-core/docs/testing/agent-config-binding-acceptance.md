# Agent 设置保存的 Workspace 绑定验收

日期：2026-09-14，macOS ARM64。目标是补齐已批准的 Workspace 隔离要求，
不修改原 Console，不改共享宿主默认行为或模型凭据优先级。
实施清单见 [身份方案](../architecture/workspace-data-identity.md#设置保存入口的绑定校验补漏2026-09-14)。

## 已复现与修复

将 writer 的真实目录重命名保留，在相同路径放入没有原标识的新目录及
`agent.json` 哨兵。原 `replace_config_field` 和公开 Agent 设置更新只验证
目录存在，随后写入新目录并报告成功。两项失败回归均准确复现，不是编译
或环境失败；首次 HTTP 测试在普通 PUT 失败处停止，尚未执行后续模型 PATCH。

生产改动限于 `desktop_agents.rs`：两个保存入口在原注册锁内调用现有
`context_from_catalog`，以返回的目录执行原来的配置发布和回滚。校验先于
凭据读取和文件写入。字段替换继续拒绝停用 Agent；普通/模型管理设置仍
允许编辑绑定有效的停用 Agent。未增加存储格式、迁移、锁或新 API。

## 回归与证据

输出：产品仓库 `dist/qa-config-binding-20260914-w4Fves`。日志和退出元数据
均保留；所有测试使用独立临时目录、假凭据和 loopback 模型，不调用系统
keychain，也不运行分发 Core。

| 记录 | 结果 | 范围 |
| --- | --- | --- |
| `red.log/json` | 0 通过、2 失败，退出 101 | 修复前真实成功写入与预期拒绝不符 |
| `green.log/json` | 5/5，退出 0 | 替换目录、缺失/错误/过大/错配标识、公开普通和模型设置、Agent 模型选择、正常保存/重开和 Unix 重定向 |
| `workspace.log/json` | 749 通过、28 ignored，退出 0 | 完整普通 workspace；含新增第六项注册锁排队后重新校验。随后新增的显式原 Agents 页面测试由下述浏览器组单独执行 |
| `clippy.log/json` | 退出 0，11.48 秒 | workspace/all-targets，warnings 为错误；包含新增浏览器测试源码 |
| `explicit.log/json` | 29/29，退出 0 | App Server 显式 28 项（339.37 秒）与 CLI Debug 页面 1 项；含新增 Agents CRUD 与重开 |
| `frontend.log/json` | 2453/2453，退出 0 | 原 Console 295 个测试文件，66.83 秒；保留预期错误注入及 jsdom 提示 |
| `fmt.log/json`、`inventory.log/json` | 全部退出 0 | 格式检查；3 项扫描器测试与 API 快照防漂移。34 个静态未匹配调用仍在，不把快照一致当作全功能完成 |
| `release-rust-sdk.log/json` | 构建成功、3/3 | release 构建 53.67 秒；Rust SDK 真实 Core、排空保存与保存失败传播 |
| `typescript.log/json` | 构建成功、26/26 | 新 source Core 的 stdio、WS/WSS、关闭与真实轮次 |
| `python.log/json` | 37/37 | conda qwenpaw；同一 source Core，包含与 TS 连接同一个宿主的测试 |
| `vscode.log/json` | 编译成功、73/73 | 同一 source Core 的扩展源码测试，不是原生 VS Code 激活 |

新增普通六项核对完整注册表、配置内容、目录标识和错误响应；Unix 重定向
还核对目标没有新增 `agent.json`。排队测试明确让注册锁阻塞保存 future，
在等待期间替换目录，释放锁后必须拒绝，不靠 sleep 猜测是否开始执行。

`source-delta.json` 对照上一批 `4v2E9d/source-inputs.json` 的 2,902 条记录，
确认仅一个已有生产源文件变化；测试模块登记另有变化，新增测试文件单列。
第一次来源核对脚本没有处理清单中的 `deleted:true` 记录，遇到此前已删除
的 `desktop_navigation.rs` 后退出 1；修正为检查该路径仍不存在后完成逐项
核验。保留 `input-delta` 失败与 `input-delta-corrected` 成功日志，不重新生成
旧清单来掩盖差异。原 `console/src` 的 diff/status 均为空。

新增浏览器测试使用既有 `--agents-crud` 驱动和未修改的 Console build，
核对原模态框创建/编辑/复制、表格置顶/启停/删除、侧栏选择及 Workspace
文件隔离；重开后完整注册表一致，默认 Agent 未改变，没有发起模型请求。
文件隔离断言沿用驱动的辅助 HTTP 操作，不把它描述成浏览器内手工编辑文件。

本轮源码验收构建的 Core SHA-256：
`8dad78051d73ef2c040a28f205f540026ce4245ba411b8a56ae141441a221b6c`。
release/Rust SDK → TS → Python → VS Code 顺序执行，全部使用同一源码二进制；
最终来源、结果和前端零改动核对见 `verification-final.json`。

## 尚未完成

- [x] 显式原页面整组、前端完整回归与最新 source release/客户端验证。
- [x] 新九类 `wADUE9` QA 制品已纳入修复并完成静态/隔离安装验收，见
  [下载清单与制品记录](qa-agent-config-packages-20260914.md)。标准打包脚本
  重新构建 source Core 为 `be7918...`，所有客户端再次对其验证；旧 `4v2E9d`
  文件保留且不含本修复。原生激活与包内 Core 运行仍未完成。
- [ ] 包内 Core、原生 Desktop/VS Code 激活及 Windows/Linux 实机验收。
- [ ] 共享宿主生命周期/凭据策略和其余全功能项，均不由本次保存校验代替。

这只是保存入场时的目录代际校验，不声称能阻止同权限外部进程在校验后
再次替换文件系统。也没有借此修复尚未接入的 Channel 配置/外部运行时。
