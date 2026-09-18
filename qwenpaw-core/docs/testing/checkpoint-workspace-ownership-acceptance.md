# 检查点 Workspace 归属验收

日期：2026-09-09—2026-09-10；对应计划 §14.2.24.53 和 [设计](../architecture/checkpoint-workspace-identity.md)。这是实施中的局部证据，不是检查点全功能或安装包完成声明。

## 会话目录第一步

本轮修改 `desktop_chats::checkpoint_sessions`：移除项目路径筛选，先解析注册上下文，再由 `bound_checkpoint_sessions` 按固定的 WorkspaceDataKey 读取完整会话。内部固定上下文入口不重新按可复用的 Agent 名称绑定。已有目录条目不能被项目路径或客户端 alias 改写归属；未登记 SDK Thread 仍只归当前默认 Workspace，不归拥有其项目目录的其他 Agent。

`desktop_checkpoint_session_tests.rs` 使用隔离临时 Core、真实注册/删除/重开、真实聊天目录及 HTTP 路由，比较完整会话结构：

1. 共享项目的两个 Agent 不混入彼此聊天；同一 Agent 的另一个项目和归档聊天均保留，重开后相同。
2. 保留基础目录改名后仍拥有原会话，同名新目录拥有自己的会话；已固定的旧上下文不随当前同名注册改变。
3. SDK Thread 即使位于其他 Agent 的基础目录，也不被该 Agent 认领，默认目录可见且持久归属不反转。
4. 完整会话可见不意味着旧单目录 ZIP 已能安全支持跨项目快照；手工快照保留原有 404 边界，且不写入 `data/checkpoints`。这项是过渡期安全回归，不是目标功能验收；新版快照信封实现后必须替换为成功创建、基础文件范围与原 Thread 项目路径独立恢复的完整测试。

前三项在旧生产代码下 **0/3**，实际会话均被项目过滤成空数组（夹具的默认 `New Chat` / `desktop` 字段核对修正后再复现）。修复后定向检查点组 **13/13**。扩大列表后，第四项先复现错误 200 和无效范围快照；补回写入边界后通过。

## 会话目录第一步验证

- [x] 完整 Rust 普通测试 **559/559**，18 项需外部环境的显式用例未计入普通通过数。App Server 库 **335/335，8.67 秒**，HTTP **36/36，3.63 秒**，Core 库 **108/108，2.93 秒**。
- [x] `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`：**13.26 秒**通过。
- [x] 原页面逐项执行 **17/17** 通过，包括备份后的 24 个页面导航（含 `/checkpoints`）与各自操作。导航不是跨 Agent 检查点 CRUD 验收。
- [x] 调度参考单独在正确 conda qwenpaw 环境复验 **1/1，0.11 秒**，内部 **16 个 APScheduler 3.11.3 日程**全部匹配。本次显式组首次为 **17 通过、1 失败，283.57 秒**：调用参数把 conda 的 PATH 覆盖成外层路径，`python` 启动报 `NotFound`；没有改测试或调度实现，保留失败，不能写成同一次 18/18。
- [x] fmt 检查、diff 检查和 `console/src` 零 diff。
- [x] 当前源 Core release 构建 **49.29 秒**；随后 TypeScript SDK 编译与 **4/4，0.792 秒**，conda qwenpaw 中 Python SDK **5/5，1.009 秒**，VS Code 编译与 **57/57，2.982 秒**依次通过，均无跳过项。真实 Core 测试使用本次 release，不使用旧包替代。
- [ ] 新快照存储、请求 Agent 范围及各端分发验收。

完整普通组首次在启动 HTTP 契约失败：夹具已创建一个默认 SDK Thread，但旧断言仍要求检查点图 `sessions: []`。根据原完整 ChatManager 范围，更新为该已知 Thread 的完整会话结构（包括独立计算的 session key），没有删除用例或只断言成功码；重新执行整个工作区通过。

本轮未改 `console/src`，未改归档格式或移动历史文件，未重新生成 §52 分发文件。该批文件的安装态 SIGKILL 及管理员核查依赖继续保留在 [制品记录](qa-mail-packages-20260909.md)。后续修复不得把当前 404、旧路径哈希状态或默认 Agent 写死当作最终行为。

会话目录第一步的源 release SHA-256：`bfc25619a23bb5944aa5fa70ca0b4ec0d3f5c088f407d9d53037cde39427b519`。§52 的九类分发制品仍是原构建快照，未被覆盖；不能把这里的客户端测试替代安装态、原生窗口或 Windows/Linux 验收。

## 请求入场第二步（2026-09-10 收尾）

全部 11 个原 HTTP 调用现在解析 `X-Agent-Id`，在生命周期边界内固定 AgentContext、基础目录和配置后再取检查点锁。图、手工快照和恢复使用固定 WorkspaceDataKey 的会话目录；恢复根据该 Agent 的 running 配置区分 Memory，缺省与原运行配置页面一致，不读取其他 Agent 的全局目录配置。成功响应不变，前端源码未改。

新增 `desktop_checkpoint_workspace_tests.rs` 4 项普通回归：

1. writer 自动开关、保留设置与默认 Agent 隔离；项目配置共享/切换、重开和默认 reset 不改变 writer 设置。
2. 未知、禁用、标识失配各执行全部 11 个调用（共 33 次），分别返回完整 404/403/409 结构；默认 state.json 字节不变。
3. writer 自定义 Memory 目录及文件快照，在重开后预览、恢复、创建 safety、更新 HEAD、预览/执行 GC 和 reset；拒绝默认 Agent 访问 writer commit，默认文件和完整图不变。
4. 暂停检查点锁时，已入场写入持有生命周期锁、删除等待；释放后写入先完成。保留目录换名继承，旧名字用于新目录不继承。此测试没有覆盖删除物理目录后同路径新代际。

初始夹具宏导入问题修正后，前两项在旧生产代码下 **0/2**：未知 Agent GET status 返回 200，writer 开关改变默认状态。修复后四项通过。完整普通 **563/563**（App Server **339/339，9.05 秒**，HTTP **36/36，3.62 秒**，Core **108/108，2.94 秒**）。严格检查先报告一个测试函数 123 行，将 GC/reset 断言按阶段提取后 **9.71 秒**通过，未放宽 lint。

新增真实原页面专项 **1/1，15.42 秒**：两 Agent 的项目配置共享同一目录，但已有 Thread 仍在各自基础目录。通过原侧栏与控件切换、自动开关、创建 writer 快照、修改保留数、切回默认确认不变、重载 writer、清理及只重置 writer；HTTP 仅观察这些 UI 写入，Rust 再重开并比较默认图完整结构。未驱动 RestoreModal，也不证明外部项目 Thread 的文件快照完成。

- [x] 提取测试阶段后的专项 **4/4，0.29 秒**；正确 conda qwenpaw + Node 24 环境下，一次串行完整显式组 **19/19，296.98 秒**（18 个页面场景与 1 个调度参考），无跳过项。先前 PATH 失败仍保留，不据本次通过删除历史记录。
- [x] Core release **49.38 秒**构建通过；随后 TS SDK 编译与 **4/4，0.779 秒**、conda qwenpaw 中 Python SDK **5/5，0.588 秒**、VS Code 编译与 **57/57，2.974 秒**依次通过，均无跳过项。当前 `target/release/qwenpaw-core` SHA-256：`a295e85a42561b2f1b7577a7e90ab50d1902270c9a8e91e4c59cf7a6cc851731`。旧 §52 九类分发文件仍不包含本次修复，未覆盖或改签其失败样本。
- [x] fmt、diff 检查通过；`console/src` 的 diff 和 status 均为空。浏览器驱动位于 Core 的 scripts/，未修改原前端组件。

当前状态/归档仍是 v1 路径哈希格式；新请求校验不能证明历史状态所属代际。自动快照仍使用旧结束钩子。这两点及跨项目快照必须继续实现，不能拿已通过的手工调用替代。

自动钩子的具体未完成风险：它仍按 Thread 项目路径读取自动开关与写入归档；当共享项目就是另一个 Agent 的基础目录时，手工接口修复本身不能阻止自动钩子使用该目录的状态。这是当前源码边界，不是已通过的自动隔离验收；必须随固定运行上下文及新归属格式一起修正。

## 版本化身份第三步（2026-09-10，实施中）

状态与嵌套 ZIP 已升级为 v2，归属包含类型化 WorkspaceDataKey 和基础根目录，状态目录不再单凭路径哈希命名。Console Lease 保存入场 AgentContext；自动快照等待检查点锁后验证取消与目录 marker，不请求生命周期锁，也不按结束时的可复用公开名称或 Thread 项目重新决定归属。Backup 显式使用来源/目标 key，重写 staging 的身份、摘要、父节点及 HEAD；外部 Thread 项目不重定向。

此前“当前仍为 v1”的描述属于第二步历史边界，第三步生产代码已改变，但尚不能据此宣布完整等价。新生产代码下首轮定向 **19 通过、1 失败、1 显式浏览器项未执行，0.62 秒**：失败项仍把 Thread 外部项目路径当作非法来源。将此旧单目录断言改为检查 ZIP 信封根目录/key 不匹配，并新增外部项目成功恢复覆盖；没有取消归档路径、链接、预算、Thread 身份或 live 数据不变的安全断言。

随后定向 **26/26，0.74 秒**通过，另 1 浏览器项待显式执行：

- 同路径物理目录新代际不会继承自动开关，重开后仍隔离。
- 共享项目等于另一 Agent 基础根时，自动快照仍仅写入入场 Workspace。
- 公开名称复用后，冻结身份仍写入原保留 Workspace；持有生命周期锁时完成，不重新入场死锁。
- 等待检查点锁期间取消或替换 marker，不新增快照；错误 state key 的读取/修改/reset 均拒绝且字节不变。
- 未绑定旧路径哈希数据返回 409，旧字节不变；这不是旧格式迁移已完成。
- 手工跨项目快照经重开可预览/恢复基础文件，不改变外部项目文件与原 Thread 项目。
- Backup 在新 key 下重写归属及图；settings-only 也重绑定，错误来源 key 在改写前拒绝；已有真实 HTTP 备份、恢复和失败回滚用例仍通过。

严格 Clippy 首次报告 `rewrite_entries` 101 行，提取信封重映射步骤后 **13.32 秒**通过，未禁用 lint。完整普通 **572/572**，19 显式项不计入普通通过数；App Server **348/348，8.94 秒**、HTTP **36/36，3.62 秒**、Core **108/108，2.83 秒**。fmt、diff 检查通过，`console/src` 的 diff 与 status 均为空。

扩展原页面专项 **1/1，15.89 秒**：writer Thread 位于共享外部项目，通过原 Snapshot/RestoreModal 控件创建快照、预览、勾选单个文件、确认恢复，验证 safety 与 HEAD，再关闭详情并 GC/reset。预览未创建 safety，未勾选时确认不可用；Rust 重开后检查所选基础文件恢复、未选基础文件/外部项目文件未变，Thread 项目仍为共享外部项目，默认完整图未变。HTTP 在浏览器驱动中仅观察结果，不替代这些 UI 写入。完整 19 项显式组、release、SDK 与新分发包仍待本次复验。

### 第三步完整复验

- [x] 正确 conda qwenpaw + Node 24 环境下，完整显式组一次串行 **19/19，305.21 秒**，无跳过，包括扩展的 RestoreModal 专项和 16 个 APScheduler 日程参考。
- [x] `CARGO_INCREMENTAL=0 cargo build --locked --release -p qwenpaw-cli`：**50.48 秒**；当前源 Core SHA-256 为 `d4dda54bb5269730cf3631c3892c068cbbdb614bb6b282c69558b4632e045127`。
- [x] 指定上述 release 路径后依次执行：TS SDK 编译与 **4/4，0.802 秒**，conda qwenpaw 中 Python SDK **5/5，0.610 秒**，VS Code 编译与 **57/57，2.984 秒**，均无跳过。
- [ ] §52 九类分发文件未重建，不包含第三步修复；其安装态 SIGKILL 和管理员核查依赖仍保留。没有对失败样本改签、换路径重试或修改安全策略。没有据源码 release/客户端成功声称 DMG、VSIX 安装态或 Windows/Linux GUI 已完成。

## 运行时第四步（2026-09-10，实施与验证中）

生产代码新增 `desktop_checkpoint_runtime.rs`：按固定 Workspace/session 管理 pending 和 active，默认 1.5 秒后执行；同会话只取代 pending，不 abort 已开始的文件写入。任务执行阶段持有 Core operation guard；Agent 停用/删除、应用关闭、备份恢复会取消并排空任务。reset/关闭自动开关在检查点锁内作废旧 pending，运行入场上下文不重新按名称绑定。自动快照之后每个 Workspace 至少相隔 15 分钟才触发当前会话 GC，非独立后台定时扫描。

单个和批量聊天删除现在先固定生命周期与 Workspace 身份，在检查点锁内预检状态，然后只清理实际成功删除的 Thread 所属 refs、HEAD 和不再引用的 ZIP；其他会话/Workspace 不变。预检状态损坏不删除 Core 聊天。批量删除遇到后续失败时，先清理此前已成功删除的部分，再返回错误，不把未删除会话算作成功。

本轮已执行的失败证据与修复：

1. 新删除专项在旧代码下 **0/1，0.19 秒**：会话目录已移除，图中仍有该聊天的节点与 HEAD。接入后专项通过，扩展验证 pending 被取消及 ZIP 清理。
2. 运行时初组 **6 通过、1 失败，0.35 秒**，失败由夹具误用 `/enabled` 导致空响应解析失败；按实际原接口改为 `/toggle`，未修改原路由。
3. 完整普通首次 App Server **355/355，9.48 秒**，HTTP **35/36，3.63 秒**，在原测试“流结束即有自动快照”的断言返回 0 而非 1。根据原 1.5 秒防抖改为最多 5 秒的条件轮询，保留 query、commit、ZIP 损坏拒绝等后续校验，不将超时视为成功。
4. 新 GC 专项在修复前 **0/1，0.14 秒**，零保留清理预览错误地选中了较旧的手工快照。现在统一自动/手动 GC 选择规则：仅 auto 和 pre-restore 参与回收及响应 refs，自动配额不被手工/safety 占用；compact 忽略 auto 的数量和天数，但仍保留 HEAD，safety 继续遵守其保留天数。压缩的 live ZIP 集合来自全部剩余状态，而非仅 `kept_refs`，防止把手工归档误当成孤儿。原 Python `test_checkpoint_basic.py -k gc` 在 conda qwenpaw 中 **5/5，1.88 秒**，15 项未选中；不是整个 Python 基线复验。

阶段定向 **36/36，1.03 秒**通过（另 1 原页面显式项未在该组执行），随后新增了 compact 忽略非零自动保留规则且手工 HEAD 仍可恢复的回归。虚拟时钟覆盖同会话合并、不同会话/Workspace 不合并、关闭清空任务、取消正在等待锁的 active、恢复 barrier 拒绝后台写入和 15 分钟 GC 窗口。完整普通、严格检查、原页面、release 与客户端仍按后续实际结果记录，尚不将本段作为已全部验证。

### 第四步完整普通复验

最终普通工作区 **583/583**（新增 11 项普通回归），19 显式项另行执行；App Server **359/359，9.38 秒**，HTTP **36/36，3.79 秒**，Core **108/108，2.96 秒**。最新严格 Clippy **13.50 秒**、fmt 和 diff 检查通过，`console/src` 的 diff/status 均为空。完整显式组、release、客户端与新的九类 macOS QA 制品继续按实际结果记录。

- [x] 完整显式组一次串行 **19/19，299.91 秒**，包括原 RestoreModal 和 16 个调度参考日程，无跳过。
- [x] release **50.11 秒**构建通过；源 Core SHA-256：`f930cc2103980ec0c6e6876c031ea3670e1bf4f8c856004d56ad24916bad719f`。
- [x] 指定本次源 release 后顺序执行 TS SDK 编译与 **4/4，0.777 秒**、conda qwenpaw 中 Python SDK **5/5，0.603 秒**、VS Code 编译与 **57/57，3.135 秒**，均无跳过。
- [x] 新九类 QA 制品已在 `dist/qa-runtime-20260909-uzKETn` 生成（目录日期按脚本 UTC）；校验和 **9/9**，2,842 个构建输入复查相符。四类分发中的 1,311 个原 Console 文件一致，DMG 校验与 ZIP/DMG 签名完整性通过。已安装 Python SDK **5/5**、TypeScript 初始化/建会话 **1/1**（均连接源 Core），旧 wheel CLI **855/855 + 36/36**，两类 VSIX 隔离安装与执行文件比对通过。失败记录、安装时序和验收边界见 [本批制品验收](qa-checkpoint-packages-20260910.md)。
- [ ] 新包 Core 运行、实际原生窗口及跨平台验收仍待完成；本轮没有重新执行被拦截的包内 Core。旧失败样本、终端防护核查依赖保留，系统策略不变。

## 后续必须完成

- 运行中的检查点恢复静默期及完整失败恢复；当前 Console 默认防抖、触发式 GC 与聊天删除清理已有实现和回归，但所有生产入口的自动钩子覆盖、原可配置防抖策略仍需核对，不能据当前范围宣布整个运行时完成。
- 有依据的旧原生归档接回仍未实现；不迁移 Python 数据，也不依据同名/同路径猜测历史所有者。
- 扩展真实跨 Core HTTP 的外部项目历史及并发/回滚组合，现有重映射单测与 idle HTTP 成功不覆盖全部组合。
- 各端最新分发与安装态、完整原生 GUI 和 Windows/Linux 实机验证，及总计划中其他未完成功能。

## 第五步：原生 Turn 排空基础（2026-09-10）

原 `restore.py::_run_restore` 对所有非 dry-run 操作执行静默期，包括只恢复会话；先前只提文件/Memory 的范围已在设计中修正。Core 的 `finish_turn` 在更新内存 idle 状态之后仍有最终存储写入和完成事件发送，不能只轮询 Thread status 或把客户端断开视作生产结束。

新增每 Thread 的执行租约：统一 Turn 入场获取，后台任务持有至 `run_turn` 完全返回。`quiesce_threads` 先验证完整 ID 集合，再去重、稳定排序并等待写租约；没有中断已有 Turn。Guard 同时持有全局 operation 租约，Backup 无法在其存续期替换整个 Core 状态。普通检查点替换保留当前 Thread 的同一执行锁，持久化加载/全量 Backup 安装才创建新锁。

新增 6 项原生回归 **6/6，0.19 秒**：

1. 仅选中 Thread 被阻止新运行，另一 Thread 可正常完成；检查点替换后仍被阻止，释放后可运行，重复 ID 不造成死锁。
2. 审批中的 Turn 保持完整历史和审批可用，获准正常完成后才获得排空 Guard。
3. 超时和取消分别释放已取得的部分锁，不改变活跃 Turn，原审批仍可完成。
4. 未知 ID 明确拒绝；持有 Guard 时全局 Backup 等待超时，释放后可进入 Backup，Backup 独占期间拒绝排空入场。
5. 丢弃事件消费者后，原后台 Turn 仍被跟踪，审批正常完成前排空不会成功。
6. 两个重叠集合以相反调用顺序请求，依赖统一锁顺序正常完成，释放后两个 Thread 都可继续运行。

首次编译因测试层 glob 宏导入冲突失败，显式导入 `pretty_assertions::assert_eq` 后通过。首次严格检查报告新字段使 `from_store` 达到 101 行、两处单元素 slice 多余 clone；提取加载/全量恢复共用的 ThreadRecord 初始化步骤，使用 `from_ref`，不禁用 lint。随后严格 Clippy **18.41 秒**通过，完整普通与原页面回归继续记录。

这是 Core 执行原语，不是已接通的 HTTP 恢复：App Server 的 Workspace 入场冻结、完整生产者排空、调度恢复与非目标 Workspace 并行仍未实现。§53 第四步 QA 包和源 release `f930…` 不含本次代码，旧制品及其验收证据不改写。

### 第五步源码复验

- [x] 完整普通工作区 **589/589**，19 个显式项不计入普通通过数；App Server **359/359，9.49 秒**，HTTP **36/36，3.72 秒**，Core **114/114，2.93 秒**，其他协议/SDK/MCP/存储/工具回归均通过。
- [x] 严格 Clippy **18.41 秒**；fmt/diff 与 `console/src` 零 diff。
- [x] 本次完整显式组 **19/19，299.35 秒**，一次串行执行，无跳过；包括原 RestoreModal 和 16 个 APScheduler 日程参考。
- [x] 本次 release **53.11 秒**构建通过，SHA-256：`5c2c1f39cd884b23a2eb28b3eb31c2ee0d6e84288c257cfee8e34023fcdbd7d5`。随后显式指定此源 Core，依次执行 TypeScript 编译与 **4/4，0.798 秒**、conda qwenpaw 中 Python SDK **5/5，0.607 秒**、VS Code 编译与 **57/57，0.186 秒**；均无跳过，过滤继承的密钥/产品配置，使用本地测试夹具。
- [ ] Workspace HTTP 协调接入及其专项、最新分发/安装态，仍属于后续实施而非本次原生基础完成范围。

## 第五步：App Server 恢复协调（2026-09-10，接入复验中）

真实恢复已固定 WorkspaceDataKey，在生命周期/Cron/checkpoint 三个锁内登记暂停与现有生产者，再释放三个锁排空 Console、Cron、Heartbeat、自动任务及完整原生 Thread 集合。30 秒共享超时返回原 400/detail；预览不静默、不创建 safety。SDK 新 Thread/Turn、聊天创建/删除、Agent 删除/停用会等待目标范围恢复结束，其他 Workspace 仍可入场。Cron 暂停不改写持久化开关或时间游标。

原 `src/qwenpaw/app/crons/manager.py::run_job` 使用后台任务并立即返回。接入初稿曾在恢复期间返回 409，已按原行为改为立即 `{"started":true}`、后台释放 Cron 锁等待、重新验证归属后执行。新 Cron 专项验证暂停期间完整存储结构不变、先前禁用任务仍禁用，以及恢复后的 manual/scheduled 两条成功历史，不能以只检查 HTTP 200 替代执行结果。

- [x] 修正手动 Cron 后 8 项 HTTP/原生生产者竞争回归 **8/8，0.62 秒**：跨 Workspace Console/审批、三种恢复范围超时零修改、预览、Cron、Heartbeat、未登记 SDK Thread、准备失败解锁、Agent 删除排队。
- [x] 首次严格检查报告 dispatch 109 行与测试 142 行，提取方法/断言辅助函数后 **13.66 秒**通过，未放宽 lint。
- [x] 新增 HTTP 等待方断开专项及完整普通工作区 **598/598** 通过：App Server **368/368，9.69 秒**、HTTP **36/36，3.66 秒**、Core **114/114，2.98 秒**；19 个显式项另行执行。首次新增专项因夹具审批误用 `root` 而非 `default-root` 返回 403（原 8 项仍通过）；修正请求归属后通过，不修改权限实现。新增测试后的严格 Clippy **12.60 秒**、fmt/diff 与 `console/src` 零 diff 通过。
- [x] 完整显式组一次串行 **19/19，298.92 秒**：原页面交互及调度参考均通过，无跳过；使用 conda qwenpaw 与 Node 24.18.1，不用模拟 HTTP 调用替代原页面控件操作。
- [x] release **52.67 秒**；本次源 Core SHA-256 为 `575a63ad419c7f96568db4b4d2ca0c0e8b0bc72feebac943e3a55b9e1caf5990`。构建退出后依次验证 TypeScript 编译及 **4/4，0.823 秒**、conda qwenpaw 的 Python SDK **5/5，0.602 秒**、VS Code 编译及 **57/57，0.181 秒**，均无跳过；环境过滤继承密钥与产品配置，并显式连接本次源 Core，未启动包内 Core。
- [ ] 最新分发与安装态验收；旧九类 QA 文件未覆盖或重建，仍是第三、四步版本。

恢复等待方断开不等于后台事务取消：既有 `restore_operation` 中间件保留事务及 Core operation 租约，避免 blocking 文件写入尚未结束就放开入场。新增用例已验证原审批/活跃 Turn 保持、恢复继续持有暂停、完成后文件与 safety 一致且可以再次入场。真实网络断连、运行中 RestoreModal、完整 Backup/文件提交回滚交叉矩阵及原 pending 策略的等价性尚未全验收，父项保持未完成。第四步 QA 包仍不含本次 App Server 代码，没有再次运行被拦截的包内 Core。

### 已确认的下一项交互差异（不以当前测试结果掩盖）

原 `restore.py::_run_restore` 关闭 query gate 并持有 service lock，但没有调用 `Debouncer.cancel_pending`；`runtime.py::schedule_auto_snapshot` 的计时器仍可到期，自动快照等待 service lock 后继续。当前 Rust `freeze_workspace` 取消 pending，完成钩子也在暂停时直接返回，因此会丢失原版本应保留的自动快照。这是已确认的行为差异，不只是缺少测试；当前“失败恢复取消 pending”的断言描述初稿，不能成为最终等价契约。

- [x] 将自动任务的入场与已执行写入区分：保留防抖计时器/完成事件，自动任务在恢复暂停时释放 checkpoint 锁等待；持锁写入已在 freeze 获取同一锁之前完成，不能再等待那些正等待恢复门禁的任务而形成死锁。修正与结果见下节。
- [x] 先复现恢复成功/失败/超时后的 pending 保留、暂停中到期不写入及完成钩子不丢失，再更新上述初稿测试；Agent 删除/关闭、禁用/reset 与全量 Backup 的取消入口仍保持原范围，回归结果见下节。
- [ ] 再扩展运行中原 RestoreModal 与最新全套回归/制品，不将本次接入基线当作这项差异已经修复。

### 自动快照保留修正与运行中原弹窗（实施中）

- [x] 先运行失败回归：恢复专项 **8 通过、3 失败，0.74 秒**；预检失败、成功恢复等待中、超时等待中均因 pending 已被取消而失败，原 8 项仍通过。
- [x] 移除 Workspace restore 对 pending 的取消和暂停期完成钩子的丢弃。自动写入使用同一个 checkpoint 锁检查暂停，暂停时释放锁等待，同时监听取消/应用关闭；不请求生命周期锁。restore 取得 checkpoint 锁时，已有快照写入已经完成，不能再等待那些等待 restore 的自动任务。全量 Backup、Agent 关闭/删除、reset/禁用的独立取消逻辑保持。
- [x] 修正后检查点定向组 **41/41，2.54 秒**（另 1 个原页面显式项未在该组执行）。验证 pending 到期仍被跟踪、暂停中状态字节与文件不变、超时/失败后继续快照，成功后 pending 与原 Console 完成钩子各产生一条 auto；图的完整 summary 包含正确 safety/HEAD 数量。
- [ ] 新增运行中原 RestoreModal 专项：复用原页面控件，浏览器观察真实 loading、返回按钮禁用及选择保留后通过测试进程 stderr 发出固定握手，Rust 夹具再验证暂停/未写文件并放开本地模型。没有添加生产接口、修改前端或用固定 sleep 猜测 UI 已进入等待；模型完成及恢复后重开继续检查文件、会话和默认 Workspace 不变。
- [ ] 完整普通/显式组、严格检查、release/SDK/VS Code 与分发结果另行记录。严格检查初次发现测试 helper 114 行及 doc Markdown 问题，提取审批完成后剩 101 行，继续提取复用的虚拟时钟推进步骤；未放宽 lint。

本段更新上方接入初稿的 pending 行为；历史通过数和旧包内容不随修复改写。仍未宣布所有生产入口自动钩子、可配置策略、完整恢复/Backup 竞争或跨平台交互已完成。

运行中弹窗首轮 **0/1，45.46 秒**：夹具在原页面创建前置手工快照之前启动了同一 Thread，`/snapshot` 返回 409，尚未到达恢复按钮。现将夹具启动握手移到原页面完成手工快照之后，随后仍必须观察实际 loading 才释放模型。未放宽 API 错误断言，也没有更改原 UI。另记录待处理的产品差异：原路由/`make_snapshot_result` 不按活跃 Thread 拒绝手工快照，而 Rust `export_thread_checkpoint` 当前会返回 busy；调整夹具时序不是这项差异的修复，活跃会话手工快照及其可恢复的一致性需独立实施验收。

- [x] 修正夹具时序后运行中原弹窗 **1/1，15.98 秒**，全部原 CRUD 仍经控件完成，且重开验证所选基础文件恢复、未选基础/共享项目文件未改、目标会话恢复到基线、默认完整图不变，夹具模型实际执行并完成。
- [x] 最终普通工作区 **601/601**（App Server **371/371，10.25 秒**；HTTP **36/36，4.99 秒**；Core **114/114，2.96 秒**），20 个显式项另行执行。新增取消专项分别调用关闭与 Backup 使用的自动任务排空入口，证明等待恢复门禁的自动任务可被取消，不使排空死锁；这不是完整 Backup HTTP 交叉矩阵。
- [x] 严格 Clippy **10.81 秒**通过，fmt/diff 与 `console/src` 零 diff 通过；没有删除文件或修改系统策略。
- [x] 本次完整显式组一次串行 **20/20，320.11 秒**，无跳过，含保留的 idle 检查点场景、新运行中 RestoreModal 场景及原调度参考。
- [x] release **52.76 秒**，本次源 Core SHA-256：`05306401f4b98c8ee90ebd6509ada8b3092c058b41fc451dc8a46c948ca7cc07`。随后明确连接此 Core，TypeScript 编译及 **4/4，0.818 秒**、conda qwenpaw 中 Python SDK **5/5，0.596 秒**依次通过，无跳过。
- [x] Python SDK 退出后继续 VS Code 编译及 **57/57，0.177 秒**，同样指定本次源 Core，无跳过；本轮客户端按 TS → Python → VS Code 顺序完成。
- [ ] 最新分发及跨平台完整验收；旧 QA 包不含本次修复，本轮未启动包内 Core、未改签或改变系统安全策略。最终 fmt/diff 与前端源码零 diff 保持。

## 活跃会话手工快照（实施中）

- [x] 原实现隔离实验：conda qwenpaw、临时工作区、实际 TaskTracker 的未完成生产任务及 CheckpointService。运行中创建手工快照成功，Git blob 与任务开始前已保存的会话字节完整相等，任务仍为 running；任务正常结束写入新的会话文件后，原快照 blob 仍是旧字节。没有改动原代码或取消任务。
- [x] Rust 原生失败回归 **0/2，0.06 秒**，两项均复现 ThreadBusy。初轮第二项曾因模型夹具的全局请求计数导致下一轮直接结束，改为每轮独立本地模型夹具后，两项均到达实际审批等待并复现 busy；没有放宽审批要求。
- [x] HTTP 运行中快照/恢复失败回归及原生实现：每个活跃 Turn 在修改会话前保存完整上一完成边界，导出时不混入进行中的 Turns、messages、metadata 和 system prompt；恢复入场仍拒绝活跃任务。缓存仅存在于活跃任务，结束释放，不更改持久化格式。
- [x] 首轮/后续轮、取消/失败、恢复后新轮、图片与输入失败、完整结构一致性和原 UI 手工快照验证；最新普通/显式/release/源码客户端结果见下文，分发逐项记录，不据此宣称完整安装态通过。

HTTP 失败专项 **0/1，0.12 秒**同样复现真实 `/snapshot` 409，任务仍在审批中。实现后已通过原生专项 **3/3，0.11 秒**：首轮与后续轮分别导出完整旧结构，system prompt 刷新不污染快照；成功/中断后的下一轮采用更新边界；恢复后不会重新导出被丢弃历史；图片内容在源文件变化后仍完整，坏图片输入不替换边界。另新增模型失败后的边界回归，完整工作区及真实 HTTP 正向结果待记录。

缓存放在非持久化 ActiveTurn 内，每个活跃 Turn 增加一份会话快照的内存占用，Turn 完成时释放，不给所有 idle Thread 永久缓存副本，也不迁移存储格式。导出复制这份完整快照，而非只过滤进行中 Turn 或截断 messages；活跃恢复仍返回 busy。原页面测试现在重新在点击手工快照之前启动本地模型任务，先验证活跃快照成功，再观察恢复 loading 后放开模型，不再保留上轮临时避开活跃快照的启动时序。

- [x] 完整普通工作区 **606/606**：App Server **372/372，10.70 秒**、HTTP **36/36，3.80 秒**、Core **118/118，2.95 秒**；20 个显式项另行执行。真实 HTTP 活跃快照成功，完整活跃会话/审批不变，任务正常完成后恢复该快照，Turns/messages/metadata 回到一致的旧边界；图有一条手工、一条 safety、一个 HEAD。
- [x] 模型失败后下一轮也导出完整失败边界，未伪装成成功或丢弃错误；该新增原生回归已计入完整工作区，不叠加计算。
- [x] 最新严格 Clippy **8.67 秒**、fmt/diff 与 `console/src` 零 diff 通过；先前严格检查 **26.39 秒**属于新增模型失败测试前基线。
- [x] 完整 20 项原页面/参考、release 与源码客户端顺序复验完成，数值见下文；最新分发的安装/运行边界另见制品记录。

- [x] 本次完整显式组一次串行 **20/20，314.09 秒**，无跳过。运行中页面专项现在在点击 Snapshot 之前已有活跃原生 Turn，原按钮成功创建手工快照，然后 RestoreModal 等待该 Turn 正常完成再恢复；空闲场景及其余页面保留。
- [ ] 本次源码 release/客户端顺序复验后，计划在新目录生成九类 macOS ARM64 QA 制品并逐项检查。只替换构建脚本管理的 Tauri/VS Code 生成资源，旧 QA/失败样本不改动；约 49 GiB 可用空间，不执行未确认的缓存清理。安装态启动仍受既有终端防护核查依赖约束，不运行会改签参考副本或探测包内 Core 的完整 qualifier。

- [x] 本次 release **54.22 秒**，源 Core SHA-256：`5ef1d8bd2e8f2bb3acc6303b30b46a4324a709f935962af1b18eba9e20fff2ae`。随后明确连接此源 Core，依次完成 TypeScript 编译及 **4/4，0.796 秒**、conda qwenpaw 的 Python SDK **5/5，0.602 秒**、VS Code 编译及 **57/57，0.177 秒**，全部无跳过。
- [x] 新九类制品构建和静态逐项验收完成：`dist/qa-runtime-20260909-EWMbMd`，2,849 个来源输入、九文件校验和、分发载荷均相符，DMG 首次生成成功且静态检查后正常卸载。新增 `scripts/release/inspect-qa-macos.mjs` 仅进行来源/校验和/载荷与签名完整性检查，明确输出 static-only 与 packagedRuntimeTested=false，不调用包内 Core 或签名参考副本。后续实际安装的顺序验证记录在 [本批制品验收](qa-active-checkpoint-packages-20260910.md)，未关闭完整分发运行与跨平台要求。

## 第六步：自动完成钩子（2026-09-10，实施中）

本步按架构文档新 checklist 接入 Console/Cron/Heartbeat，SDK/App Protocol 独立跟踪，前端源码不变。旧 `qa-runtime-20260909-EWMbMd` 制品不含本步新生产代码，不能把旧包验收当作新钩子分发通过。

- 原 Python `test_checkpoint_hooks.py` **6/6，0.08 秒**，conda qwenpaw、隔离上下文；源码确认 Workspace 通用 POST_RESPONSE 顺序为成功保存 session 后安排快照，取消路径仅保存 session、不执行快照钩子。
- 新 Cron/Heartbeat 真实入口失败回归 **0/2，0.21 秒**：两个原生模型/工具任务均成功，自动开关已开启，但图的 total/auto/heads 均为 0，而非预期的 1。首次编译发现新测试的 assert_eq glob 导入歧义，显式导入修复后才取得该失败证据；编译失败不算产品失败回归。
- Console/Cron/Heartbeat 共用完成状态和 slash 判断。Cron 入场捕获 AgentContext，完成钩子在运行租约内且在最终 Cron/Inbox 写入之前登记，不依赖 Inbox 开关、不重新获取生命周期锁或按 Agent 名重新绑定；Heartbeat 同样保留启动时上下文和已有租约。
- 第一批 **6/6，7.45 秒**通过：不投递 Inbox 的 Cron/Heartbeat 成功快照，完成/中断/失败/运行中与空白/slash 判定，实际三入口 slash 过滤、Cron 取消/超时及 Heartbeat 超时无快照，共享项目下非默认 Cron 只写自己基础 Workspace。非默认用例经过原有内部 scoped executor，不将其称为非默认 HTTP 安全门禁已移除；取消用例调用现有 native cancel_job，不虚构不存在的 job stop API。
- Console slash 专项初次 **5 通过、1 失败，7.50 秒**，原因是夹具传入未创建的会话别名导致 404；改为先创建真实聊天再使用其 ID。生产模块移除未使用 TurnStatus 导入后旧测试暴露其隐式依赖，改为测试文件显式导入，未改变取消断言。
- 继续核查原 debounce 闭包发现 query_text 在登记时捕获。新专项 **0/1，1.68 秒**复现：正常长查询后、旧 pending 写入前完成 `/help`，图中的 query 错变为 `/help`。修复将完整触发查询随 pending 保存；实际会话/文件仍在写入时快照，None 保持旧查询 fallback，手工/safety 不强行覆盖查询。
- [x] 最新钩子专项、完整普通/显式组、严格检查及源码 release/客户端顺序回归；失败和结果见本节，不计为最新分发启动通过。
- [ ] SDK 完成观察、断连与生命周期追踪、所有生产入口和可配置策略、完整 Backup/恢复交叉矩阵及最新分发继续保留。

查询修复首轮 **6 通过、1 失败，9.14 秒**：旧 pending 已保留长查询，但 Core 格式化后的 UserMessage 丢掉末尾空格。原 `AgentRequest → _request_input_to_msgs → _last_user_text` 隔离实验确认首尾空白保留，因此没有修改测试期望去适配裁剪。改为从原请求最后一条消息提取文本块，不展开附件；Console 在入场时持有查询，Cron 从已捕获的 job 请求取值，Heartbeat 使用其原始查询。None 保留现有 fallback，手工/safety 不覆盖。最终新增专项 **8/8，9.15 秒**；严格 Clippy **14.50 秒**、fmt/diff 与前端源码零 diff 通过，完整回归另行记录。

完整普通工作区 **614/614**通过：App Server **380/380，11.29 秒**、HTTP **36/36，4.58 秒**、Core **118/118，2.92 秒**。20 个显式项仍单独执行，不计入普通通过数。源码核查还确认 `Core::finish_turn` 对最终 upsert 失败只记录警告，仍发送 Completed；因此不能用这批正常存储下的结果证明全部“保存成功后自动快照”语义。存储失败的内部回执及最终写入与下一 Turn 的顺序已经列入第六步 checklist，需独立失败回归和实现，不擅自将存储错误伪装为任务成功持久化。

- [x] 本次完整显式组一次串行 **20/20，320.41 秒，无跳过**，包含原页面活跃快照/恢复、Console/审批、Cron/Agent 停用、邮件、备份、市场、模型以及原调度参考；未修改前端组件或测试断言去忽略失败 API。
- [x] 新源码 release **52.18 秒**；Core SHA-256：`3f807a7426dbf637100ab11df03b1b8cebc816fafca91580d4005403896bb63b`。随后依次连接此源 Core：TS 编译与 **4/4，0.815 秒**，conda qwenpaw 下 Python SDK **5/5，0.590 秒**（显式断言源码导入路径），VS Code 编译与 **57/57，0.176 秒**，全部无跳过。
- [ ] 本步最新分发构建/安装态、Windows/Linux 与全部原功能仍未完成；旧九类包保留原字节，其源 Core 是先前 `5ef1d8bd...`，不把新 `3f807a74...` 的结果写成旧包通过。未进行分发 Core 重试/改签、系统安全策略变更或未确认清理。

### 第六步补充：真实保存失败边界（2026-09-10）

此前测试进程句柄 14187 已不存在，进程表也没有仍运行的 Cargo 测试，因此重新运行当前两项故障回归，而不是凭丢失输出推断结果。隔离 SQLite 使用 `BEFORE INSERT` 触发器按最终 Turn 状态拒绝写入，不移动用户数据库，不依赖真实模型或密钥。

- 初始原生失败 **0/2，0.10 秒**：入场失败后完整 ThreadReadResponse 从 Idle/空 Turns 错变 Active/新增进行中 Turn；完成保存失败后 export 错误包含未保存的 Turns/messages/metadata。初稿测试访问私有 `core.inner` 曾编译失败，改用公开读接口和后续重试证明入场恢复，不将编译错误算产品失败。
- 入场先保存候选快照再发布内存，最终保存与释放 active 同锁序列化；检查点的完整旧边界从仅 ActiveTurn 持有改为活跃或最终保存失败期间保留。已显示的回复继续保留，后续成功完整保存释放 fallback。
- `turn_was_persisted` 只查询当前运行时观察到的最终成功写入 ID；不推断运行中/失败/重开/恢复导入的成功回执，也不因下一轮保存成功为旧失败 Turn 补发回执。成功恢复才清空运行时回执，协议字段不变。
- 原生故障/恢复/重开专项 **5/5，0.28 秒**：模型选择和完整入场状态不泄漏、失败后再次入场仍保留旧检查点和可见回复、后续成功写入只认可新 Turn、恢复失败保持旧回执/成功恢复清空，以及已有进行中 journal 重开仍恢复为 Interrupted。重开测试不宣称丢失的最终回复已写入磁盘。
- 上层失败回归 **0/1，1.66 秒**：三入口循环先在 Console 复现，虽然最终 SQLite 保存被拒绝，仍新增一条 auto/HEAD；不能把首次失败称为三个分支都已执行。接入统一回执门禁后钩子组 **9/9，9.45 秒**，包含三入口真实完成但保存失败无快照、正常入口/slash/取消/超时及旧查询保留。
- [x] 最终写入竞争：直接隔离 finalization 阶段，外部 SQLite 写事务阻塞实际 upsert；完成事件和下一轮入场均等待写入结束，读接口不暴露提前 Idle。写入成功后比较完整备份与检查点。
- [x] 新失败查询保留旧 pending：首轮成功后持有 checkpoint 锁，第二轮完成快照被触发器拒绝；生产租约正常退出，不等待安排新快照，不替换旧 query；RAM 两轮回复保留，但完整导出仍为第一轮已保存边界，原 pending 最终只生成原查询的一条快照。
- [x] 完整普通工作区 **622/622**，其中 Core **124/124，2.97 秒**、App Server **382/382，11.17 秒**、HTTP **36/36，4.81 秒**；严格 Clippy **17.11 秒**通过。原生新增六项、上层新增两项，不将普通组忽略的 20 项计为通过。
- [x] 显式组 **20/20，322.24 秒，无跳过**：包含原页面运行中 Snapshot/RestoreModal、其余页面及原 Python 调度参考；`console/src` 未改动。
- [x] 本次源码 release **53.89 秒**，Core SHA-256：`ee6d7a4d2fa14febaa4fe6524b4199d4d38cdb457075ec7db4b9f546a84ce4fb`。显式连接此源 Core 后顺序完成 TypeScript 编译与 **4/4，0.819 秒**、conda qwenpaw 的 Python **5/5，0.600 秒**（导入路径断言为源码 SDK）、VS Code 编译与 **57/57，0.183 秒**，均无跳过；不是 VS Code 原生激活或安装态 Core 测试。
- [ ] 新分发和完整功能仍未完成；本节改动尚未进入旧九类 QA 包。不执行未确认清理，也不重试被终端防护阻断的分发 Core。

### 第六步补充：SDK/App Protocol 运行归属（2026-09-10）

按架构清单，SDK 完成观察在 `turn/start` 入场内创建，而不是依赖响应或通知写到客户端。可选 Workspace 上下文固定基础目录、DataKey 和查询，普通 stdio 未初始化 Workspace 服务时只提供运行追踪，不假装检查点功能已经可用。

- 真实 `process_line` 红测 **0/2，0.21 秒**：正常 Completed 和响应队列已占满时请求方断连，均没有应有的 auto/HEAD。隔离模型/工具和 SQLite，不使用真实密钥。
- 新 `protocol_runs` 持有任务注册和 Core operation lease，64 项有界转发队列保留正常背压；客户端断开继续排空并完成，显式 Agent/应用取消打断输出等待并中断 Core。状态保存、钩子和审批清理完成后释放状态租约，最后通知的传输不再占用恢复屏障。
- 接入实际 Agent 停用/删除、Workspace 恢复和 HTTP/WSS 关闭；WebSocket 读/响应等待能够被 writer 断开或宿主 shutdown 唤醒，关闭时终止仅负责网络写出的 task，不 abort Core 生产任务。stdio EOF 保留原来排空输出的行为，完整 headless 初始化/退出策略仍待验收。
- 第一批 **2/2，3.31 秒**；增加长流和 Agent 控制时测试误用 `unwrap` 要求 `DispatchError: Debug`，改成测试中打印其 message，不修改生产错误类型来迎合测试。随后 **5/5，7.12 秒**、**7/7，9.02 秒**，最终八项 **8/8，10.81 秒**。编译错误不算产品回归。
- 八项覆盖：正常成功快照；响应前断连继续完成；256 个 delta 完整且响应在前；64 项输出队列满时真实停用/删除 writer 而 default 的完整运行状态不变；真实 WS 断连 Completed 与宿主关闭 Interrupted、完整图重开保持；自动关闭/slash/最终 SQLite 写入失败无快照；完成钩子等待锁时 Core 恢复屏障保持；真实 HTTP 恢复暂停 SDK、拦住新入场、恢复完整会话/选择文件并保留暂停期自动快照。
- [x] 严格 Clippy **13.48 秒**通过，前端源码未改。
- [x] 完整普通工作区 **630/630**：App Server **390/390，12.36 秒**、Core **124/124，3.21 秒**、HTTP **36/36，8.01 秒**；20 项显式页面/原 Python 参考单独串行 **20/20，318.65 秒，无跳过**。fmt/diff 与 `console/src` 零改动检查通过。
- [x] source release **52.80 秒**，Core SHA-256：`f74012f11aa844c454eb679648666c191d9ce713478ebfc41b80fa495bc8209f`。之后 TS 编译及 **4/4，0.801 秒**、conda qwenpaw Python **5/5，0.599 秒**依次显式使用该 source Core，Python 另断言源码 SDK 导入路径；随后 VS Code 编译及 **57/57，0.179 秒**通过。全部无跳过；VS Code 单测不是原生扩展激活/安装态 Core 验收。
- [ ] 纯 headless Workspace 服务初始化、SDK 每 Agent 配置/usage owner 一致性、完整 Backup/失败交叉矩阵及全部分发运行仍未完成。本次只固定完成消费者归属，未将原 `Core::start_turn` 的全局 runtime/model 路径改成各 Agent 配置，不能据共用测试模型宣称全 SDK 配置行为一致。

这次未重打九类包：旧 `qa-runtime-20260909-EWMbMd` 保持原来源和静态/安装后控制组范围，不包含保存失败及 SDK 消费者的新代码。未重试包内 Core、改签失败样本或修改终端安全策略；未清理用户未指定的缓存/制品。当前检查未发现本轮 Cargo 或 Core app-server 进程遗留。

### SDK 已绑定 Agent 的配置（2026-09-10）

- 初始两项红测 **0/2，0.22 秒**：writer 的一步限制被忽略，实际 Completed；用量归到 default/无 DataKey。Core 全局设置和显式 Thread 模型不能为修复这两项而改写。
- Core 新受信宿主入口共用校验与 provider/options 原子快照，在实际入场锁内保留 Thread.model。原 Console/Cron/Heartbeat 的默认模型选择入口保持旧语义，不增加可由 wire 指定的 runtime/owner 权限。
- App Protocol 使用入场时固定的 AgentContext，解析运行配置/provider 并传入该 Workspace 的用量归属；plain AppServer 没有 Workspace 服务时保持旧路径。这不是默认 stdio Workspace 初始化的完成声明。
- 原生专项 **2/2，0.14 秒**：显式与全局 fallback 两种 provider 快照均不受入场后同步热更影响，模型级参数按 Thread 模型解析；无效 runtime/owner/headers 不改变完整备份或检查点。上层五项初次完整 **5/5，0.80 秒**，整理辅助代码后 **5/5，0.33 秒**。
- 上层覆盖非默认步数限制、固定用量及 Thread 模型、两个真实本地 provider 并发运行及热更、完整 ledger 重开、无效配置拒绝且不写入，以及严格审批不能被 wire 字段覆盖。进一步启用假模型的 usage，精确断言伪造 usageOwner 后两次 writer 调用仍归 writer/DataKey。
- 测试整理失败保留：严格 Clippy 先报告 142/104 行，提取辅助函数过程中有同名 history 变量遮蔽函数与多余 mut，修复后 provider 用例仍为 122、102 行；继续抽取 provider 配置和启用 usage，不放宽 lint 或原断言。最终严格 Clippy **14.83 秒**通过。
- [x] 普通工作区 **637/637**：App Server **395/395，12.53 秒**、Core **126/126，3.02 秒**、HTTP **36/36，4.47 秒**。最后仅测试辅助函数及伪造 owner 断言改变后，App Server 普通库全组重新 **395/395，10.23 秒**；20 个显式项未计入普通通过数。
- [x] 原页面/参考 **20/20，320.09 秒**串行通过，无跳过。source release **55.59 秒**，SHA-256：`7c0ac37f17bb0a9df7297939bd9b56b21ed229549bf83acf31d2cedc20c63bf1`；随后显式连接此源 Core，TS 编译与 **4/4，0.810 秒**、conda qwenpaw 的 Python **5/5，0.606 秒**（断言源码导入路径）、VS Code 编译与 **57/57，0.178 秒**顺序通过，无跳过。不是包内 Core 或原生扩展激活验收。
- [x] 新九类 macOS ARM64 QA 制品生成到 `dist/qa-runtime-20260909-rWHV5J`，来源 2,856 个输入与载荷逐项静态通过，DMG 正常卸载；生成资源仅由原构建脚本更新，旧包及失败样本保留。安装后 SDK/VSIX/保留版 CLI 结果及首次镜像资源忙记录见 [本批制品验收](qa-sdk-config-packages-20260910.md)。未执行未确认清理。
- [ ] 包内 Core 启动、原生交互、Windows/Linux 与完整功能不由静态检查或 source Core 控制组代替；终端防护核查依赖仍保留。

后续 headless 初始化需分离 Workspace 服务与静态页面/桌面凭据。目前 CLI 不带 --desktop 仍走 AppServer::new；stdio 只排空 protocol_runs，而 HTTP 额外启动 Cron/Heartbeat 并收尾其他生产者。必须独立验证无静态页面、无系统钥匙串的默认 SDK 工作流及 EOF/写端断开的后台任务策略，不能仅删除 CLI 参数冲突或重用 Desktop 构造器宣称完成。

### 无页面 Workspace 宿主：共用初始化（2026-09-10）

第一步仅分离初始化：新增 `AppServer::new_workspace_with_stores`，显式注入数据目录、基础 Workspace、凭据 store；Desktop 校验页面和关闭令牌之后才使用同一 `workspace_inner`。不改变 plain `AppServer::new`、CLI/SDK 默认启动、凭据选择、调度器或 stdio EOF 策略。没有伪造页面、使用真实钥匙串或初始化日常工作区。

- 首次测试代码误用未实现的 `ModelConfig::default`，编译失败；改成显式 localhost:1/无密钥的隔离配置，不改生产类型或读取实际环境。随后五项 **4 通过、1 失败，1.91 秒**。
- 失败源于测试错误要求“任一 marker 损坏时整个构造器失败”。现有 `read_catalog/validate_catalog` 校验注册结构，`context_from_catalog` 在实际请求入场验证 marker。保持该范围：让损坏 writer 的真实 turn/start 失败，比较完整 Core 备份及 marker/catalog 原字节，再确认正常 default 可以完成任务；没有把单个 Workspace 损坏扩大为整个应用不可用。
- 六项专项 **6/6，1.93 秒**：无页面/令牌初始化与隐藏桌面路由；App Protocol 成功后快照、会话/完整用量/图在两种宿主之间反复重开；非默认模型/步数/用量；无效目录保持原文件和空目录；损坏身份入场拒绝不修复；Desktop 页面/令牌先于可能 panic 的测试凭据读取，且完整备份、Workspace/data 目录不受初始化影响。
- 又补一项严格审批：真正 App Protocol 请求等待 write_file 审批，deny 返回 accepted，文件在审批前后都不存在；终态含错误 ToolResult，正常完成历史与自动快照重开保持。最后全部 App Server 普通库 **402/402，10.56 秒**，包含七项新宿主测试。
- [x] 先前完整工作区 **643/643**（App Server **401/401，13.19 秒**；Core **126/126，3.03 秒**；HTTP **36/36，4.49 秒**）。最后只新增上述审批测试后，App Server 全库重新 **402/402**；不把两个重叠运行相加，也不声称首个命令已运行后来新增的测试。严格 Clippy 最新 **12.79 秒**通过，fmt/diff 与前端源码零 diff 保持。
- [x] 原页面/参考完整显式组 **20/20，319.04 秒，无跳过**。source release **52.62 秒**，SHA-256：`59271327038544a6c040cabc9bc7f03db319772e43803209e2589444a4e1f513`。随后 TS 编译及 **4/4，0.822 秒**、conda qwenpaw Python **5/5，0.600 秒**（断言源码导入路径）、VS Code 编译及 **57/57，0.179 秒**依次显式使用该源 Core，通过且无跳过；不是默认 SDK Workspace 初始化或包内 Core 运行验收。
- [ ] 默认 CLI/SDK 接线、退出时后台任务完整收尾、全部功能与新分发仍需后续实施。上一批 `qa-runtime-20260909-rWHV5J` 九类包不包含本次共享构造器代码，其来源和通过范围保持原记录，不改写旧清单。

架构图及后续清单已更新，尤其保留两个不能直接接线的边界：Desktop 旧初始化可能按 preferred project 复制模板；checkpoint shutdown 会取消 pending，而正常 stdio EOF 应单独验证成功任务的快照收尾。必须用真实默认客户端证明行为，不能只凭此处嵌入宿主的成功关闭父项。

### 基础目录与项目选择分离（2026-09-10）

- 原 Python 的 project_directory 服务、Agent 初始化及 Workspace service factories 区分内部基础目录与项目目录；模板属于基础目录。无页面、Desktop 两种宿主的三项初始测试各 **0/3**，复现误注册或向项目/fallback 复制模板。
- 共用初始化先解析已注册 default 的有效绑定；首次无注册时使用显式基础目录。无效 default 根/marker 不修复、不改写 fallback，继续由真实请求入场拒绝；其他 Agent 可运行。损坏 catalog 在配置/凭据初始化前拒绝。
- 九项新目录用例与七项既有宿主用例初次 **16/16，2.12 秒**；严格 Clippy 发现 single_match_else，按建议改为 if let，不禁用 lint，随后 **13.37 秒**通过。
- 随后普通工作区未通过：App Server 库 **411/411**，HTTP **35/36**，Git 状态错误读取了基础目录。追查发现仅保留内存 selected 不够：首次 default 注册还须独立保存 project_dir，否则现有 Git/Files 配置解析器回退到基础目录。新增两宿主实际 project() 断言 **0/2，0.08 秒**再次复现。修复只在首次注册、项目不同于基础目录时设置现有字段；不修改已有注册、Git 路由、请求优先级或前端，不放宽原 Git 断言。补完整 AgentContext 重开相等断言。
- [ ] 修复后全工作区、严格检查、原页面显式组、release 和各客户端逐项复验。
- [ ] 新制品构建与静态核验；包内 Core 启动、原生 GUI 与跨平台仍按原门禁，不以源代码控制组替代。

后续证据：项目字段修复后 App Server **411/411，10.70 秒**、HTTP **36/36，3.63 秒**；最新完整工作区 **653/653**（App Server **411/411，13.85 秒**、Core **126/126，2.98 秒**、HTTP **36/36，4.42 秒**），严格 Clippy **13.65 秒**通过。随后显式组 **19/20，433.45 秒**，备份 roundtrip 触发原有 180 秒超时，release/客户端/打包流水线按非零退出停止，没有继续标记候选为通过。

补充测试诊断而非改产品或放宽断言：备份浏览器 stdout/stderr 写在夹具内部存储和 Workspace 之外的临时目录，超时后终止并回收其直接 Node 子进程，再保留输出到失败报告；页面脚本追加无凭据的导航和浏览器收尾阶段日志。原期限不变。带诊断的独立同用例 **1/1，67.03 秒**，24 页无失败 API，备份创建/重载/ZIP 导出/冲突导入/恢复前备份/恢复文件/删除/外部信任/保护本地配置通过；这次没有重现超时，不能据此解释或关闭第一次失败。普通工作区计数不包含后来增加的诊断代码；后续 source release/客户端结果单独登记，不把源码控制组当作全页面或安装态通过。

诊断收尾的 fmt、严格 Clippy **11.00 秒**和诊断 Node 单测 **3/3**通过。API inventory 校验及其网络提取器单测 **3/3**通过：370 个调用点，仍有 **38 个未注册 Rust 路由**，不等于全功能完成。source release **52.50 秒**，SHA-256 `0dd0817f14d011bad42011de2b1c9b80cc04cb0ff5b7dc494fd580483faaa90b`；随后依次显式使用该源 Core：TS 编译及 **4/4，1.027 秒**、conda qwenpaw Python **5/5，0.603 秒**（断言源码 SDK 导入路径）、VS Code 编译及 **57/57，0.178 秒**通过。不是包内 Core、默认 SDK Workspace 或原生扩展激活验证。

本轮未重打九类 QA 包，`qa-runtime-20260909-rWHV5J` 仍是先前配置修复版本，不包含本节基础目录修复和共享构造器。前端 `console/src` 零改动，最终 diff 检查通过；未发现本轮 Cargo、Core app-server、浏览器脚本或隔离 Chrome 进程遗留。未删除待确认的缓存/旧包、未改安全策略、未提交或推送。备份偶发超时、分发启动、默认 SDK 接线、未实现 API 和完整跨平台/原交互验收继续保留。

### 浏览器传输与新制品复验（2026-09-10，后续批次）

确定性复现并修复 DevTools 断连后 pending 悬挂、同步发送失败残留及浏览器关闭只等待协议回复的问题。测试工具 **16/16**，原页面完整组 **20/20，277.89 秒**通过；这不证明上一批 180 秒超时的具体根因。随后最新完整工作区 **653/653**、原前端 **295 文件/2453 测试，63.46 秒**通过，前端业务源码未改。

新输出 `qa-runtime-20260909-VVMQCa` 的九类制品已经包含共享宿主及基础目录/项目分离代码，Core 仍为 `0dd0817f14d011bad42011de2b1c9b80cc04cb0ff5b7dc494fd580483faaa90b`。构建、静态检查、实际安装 SDK/VSIX 与保留版 CLI 的顺序结果和 DMG 首次资源忙记录见 [本批完整记录](qa-workspace-host-packages-20260910.md)。旧批次来源未改写，包内 Core 仍未执行，默认 CLI/SDK 接线和全部原功能门禁未关闭。
