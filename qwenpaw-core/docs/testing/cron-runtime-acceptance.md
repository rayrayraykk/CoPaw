# Cron 原生调度验收（进行中）

日期：2026-09-09，对应计划 §38 与 [Cron 设计](../architecture/cron-runtime.md)。以下按切片保留历史验收；当前已接通文本与默认 Console Agent 执行、内部 Job 归属及范围备份。非默认 Job HTTP/执行、外部 Channel 和全部原控制语义仍未完成。

## 实现与验证

- 原 API 会接受非法时区/分钟值且 next_run_at 恒空。新增回归先在原实现失败，再接入时间解析和实际 next 状态后通过。
- 3/4/5 字段按原模型归一化，Cron 字段同时满足；支持范围/列表/步长、月/星期名称、月末、闰日与无解计划。chrono-tz 处理配置时区与 DST；一次性无偏移时间不再误按 UTC 解释。
- 一次性/固定按天间隔保留小数秒、次数含第一次、截止含边界，日期溢出在写入前拒绝。
- App Server 每 250 毫秒检查持久化时间槽，不依赖浏览器或 HTTP 请求；暂停无 next、替换/恢复重算，手动执行不消耗自动计划。
- 循环任务积压合并到最新槽，超出 grace 记 skipped 并推进。开始投递前持久化下一槽和 in-flight 标记；重开后未完成投递记 cancelled、不盲目重放。此机制不是第三方投递 exactly-once 保证。
- 文本按原实现 trim 后送 Console，可选 Inbox 的 trigger 为 manual/scheduled；历史上限对齐原 50 条。删除同步清除游标/状态/历史。
- 调度和完整文本投递持有 Core operation guard，与恢复互斥；shutdown 后停止检查并等待调度任务退出。旧非法计划自动禁用并记录 error，未实现的 Agent/外部投递不记成功。

四个时间引擎单测、七个运行时单测与一个非法配置回归新增通过；连同已有归一化测试，Cron 普通测试 13/13。覆盖真实后台无请求触发、重复/合并/错过、手动不吞槽、持久化重开、未完成投递恢复、暂停/替换/删除和恢复/退出互斥。

显式差分测试在 qwenpaw conda 下调用原 APScheduler **3.11.3**，16 组计划、每组最多四个连续时间槽全部一致，包括纽约/柏林 DST 回拨、纽约与 Lord Howe 前拨、上海时区、月末/闰日、星期步长和有限重复。`scripts/cron_reference.py` 仅为测试参考，不被 Rust Core 启动、调用或打入新 Desktop 运行时。原版 named weekday/month range 不应用 `/step`，前拨缺失墙钟时间会附前偏移；保留了这些实际参考行为，没有仅据通用 cron 规则猜测。

原页面 Chrome 场景通过：Create Job、原自定义目标选择、启停及 next 状态、已暂停任务手动触发、真实 History、编辑、刷新和删除。所有变更由原 UI 触发，fetch 仅检查持久化；`console/src` 零 diff。初次驱动误用无 `+` 的按钮文案、Ant Design 通用关闭选择器和 Escape，分别失败；检查原组件后改为实际 `+ Create Job` 和原 Modal 标题关闭图标，未修改前端或放宽断言。成功独立运行 13.77 秒。

## 全回归与制品

第一轮及最终全工作区普通测试均通过，最终 **414/414**。显式整组 **10/10** 通过（175.93 秒）：9 个原页面浏览器场景，外加 1 项原 APScheduler 差分测试。浏览器包含两项备份、Cron、Market、Anthropic、Gemini、Provider OAuth、OpenRouter、Responses；备份 roundtrip 同时覆盖 24 页导航。严格 App Server all-targets/all-features Clippy、fmt/diff、inventory 3/3 与快照校验通过；清单仍是 370 调用、38 个未注册路由，不代表剩余原功能已完成。

最新 release Core 构建通过，SHA-256 为 `d9a55d7fb675503ef7035669b9c7806c95401cb80774462fff6f83ce28d85250`。显式连接该文件的 TypeScript SDK 4/4、Python SDK 5/5、VS Code 57/57 通过，SDK/扩展编译通过；这些是客户端回归，不是 SDK Cron 功能验收。

此前 `dist/qa-runtime-20260909-OPX10W/` 九类 QA 制品未包含本轮 Cron 修改，不能从源码 release 或 Chrome 通过推断 DMG/VSIX 最新安装态已完成。本轮没有使用生产 key、读取日常桌面凭据或改变系统信任设置。

## Agent 前置切片：逐 Turn 运行配置

`Core::start_turn_with_runtime` 为可信宿主提供逐次模型/运行配置入口；参数在输入与 Turn 持久化前校验。普通 `start_turn` 和 `start_turn_with_model` 也在启动异步 Agent loop 前快照全局运行设置，避免调用返回后热更新改变已接收任务。没有为 App Protocol、SDK 或原页面新增关闭审批的输入字段。

新增 8 个 Core 单测和 1 个 MCP 集成测试通过，覆盖：

- 普通入口及模型选择入口接收后的配置热更新不影响该 Turn；
- Off 后台 Turn 实际执行 Shell、模型读取结果并完成，另一聊天仍停在审批且可独立拒绝；
- 单次一步上限和 1 秒 Shell 超时实际生效，全局上限/超时不变；
- 单次安全任务等待审批时取消，旧审批失效、恢复 lease 可取得、线程可继续下一轮；
- 已禁用内置工具和 MCP Deny 不被 Off 重新启用或绕过；
- 10 组非法边界与控制字符配置均拒绝，全量备份结构和全局配置不变，公共配置写入与逐 Turn 校验结果一致；
- wire 输入额外的 `runtime`、`runtimeConfig`、`approvalLevel` 按已有反序列化规则忽略，不能关闭当前 Strict 审批。

原 Python `security/tool_guard/execution_level.py` 明确将 Off 定义为关闭整个 Tool Guard；当前 Rust 与此一致。本切片保留该语义，未误将 Tool Guard 自身的 denied_tools 说成 Off 下仍生效；独立的 MCP Deny/内置启用开关另有真实测试。

首次测试编译遇到测试宏导入歧义与 `Turn.error` 类型不匹配，修正测试后通过；首次严格 Clippy 指出可用 let-else，按建议修正，未添加 lint 豁免。第一次基线命令过滤名错误实际执行 0 个测试，没有将其计作通过；随后不加过滤重新验证了改动前 99 个 Core 单测和 5 个 MCP 集成测试。

最新全工作区 **423/423** 普通测试通过，Core/App Server all-targets/all-features 严格 Clippy、fmt/diff、inventory 3/3 与快照检查通过；`console/src` 仍零 diff。release Core 构建通过（61 秒），SHA-256 为 `e544b48c837268daa2cef3e7c24c436247a1b9600acf6d554c82a2d951028daf`。本项只完成 Agent 执行前置隔离，不改变 Cron 手动 Agent 仍为 501 的事实。

显式连接这一新 release 文件的 TypeScript SDK **4/4**、qwenpaw conda Python SDK **5/5**、VS Code **57/57** 通过且无跳过，TypeScript SDK build 与扩展 compile 通过。未重建九类 QA 制品，旧 DMG 的状态不因此改变。

显式整组浏览器/参考回归 **10/10** 通过（174.71 秒）：9 个原页面场景及 1 项 APScheduler 差分，覆盖备份两项、Cron、Market 和五项 Provider 页面。浏览器原 Cron 场景仍只验收文本执行，不算 Agent 任务验收；本次通过也不关闭此前间歇性故障调查。

## 尚未完成

- 多 Agent Cron 的 Provider/Workspace、审批、推送和生命周期隔离：内部归属和范围备份已接入，非默认 Job HTTP 请求仍明确返回 501；候选目标 GET 已按 Agent 范围开放。这是临时限制，不是原功能等价。
- 所有外部 Channel、Console CLI 终端流式输出、归档共享会话、运行中更改并发配置、高级 request_context、Agent Copy jobs 与剩余控制语义；默认 Console 原页面通过不证明这些门禁完成。
- 外部 Channel 投递、调度的多 Agent 隔离、CLI/TUI/SDK Cron 完整操作与剩余原接口语义。
- 全原生桌面交互、生产签名、公证、首次安装稳定性及 Windows/Linux 最新实机验收。

此前备份浏览器和 MCP 取消的间歇性失败、包内 Core 首次 SIGKILL 继续保留；单轮复测成功不会关闭其根因调查。

以上前置切片的测试与构建进程已结束，浏览器测试中的 App Server 使用隔离目录并在断言前停止。未提交、推送或上传发布，整体目标保持进行中；后续执行切片另记如下。

## 默认 Console Agent 执行切片

手动 Agent 请求立即返回 `started`，后台使用默认 Agent 的 Provider/Workspace 和上一切片的可信逐 Turn 配置入口；不再将 Agent 任务固定返回 501。SQLite 保存唯一执行 run ID；实时任务登记与崩溃孤立声明分开处理，下一次 tick 不会将存活任务记为中断。并发/排队总数有 256 个上限，每个 Job 的 semaphore 保留至 trace/Inbox/状态发布完成，手动排队、到期超限 skipped。

共享聊天按 Agent/channel/user/session 从持久化目录匹配，保留已有名称和分组；独立聊天沿用 `{session}:cron:{job_id}`，重启后继续同一线程。并发共享会话等待已有 Turn 完成，不直接返回 ThreadBusy。超时、退出、删除和 Core 恢复中断并排空 Turn，清理审批和声明，删除后的任务不会重建 Job 状态或通知。

每次执行创建独立 trace，完成后写真实用户/模型/工具记录；silent 不产生额外 Console 推送气泡，Inbox 关闭也保留 trace。独立 trace 在 5000 个总量上限内按时间淘汰未引用的已完成记录，不删除正在运行或关联通知的 trace。明确标记 Cron/Agent 归属的独立 trace 也纳入范围备份和恢复；其他未归属、错误归属或跨 Agent 引用仍不得导出。

新增 **14 个 Agent 测试 + 1 个 trace 容量测试**。Cron 27/27 普通测试通过；完整工作区最终两次检查均通过，最后一次为 **438/438** 普通测试。严格 App Server all-targets/all-features Clippy、fmt/diff、inventory 3/3 与快照检查通过。`console/src` 零 diff。

原 Agent Cron 页面独立验收通过（15.47 秒）：原 JSON 输入、独立会话、final/silent/Inbox 开关、创建/启停/立即执行/历史/编辑/刷新/删除；实际 Rust 工具生成隔离 Workspace 文件，模型收到工具结果后完成，真实 Inbox trace 包含 4 条记录。原前端没有修改。首次浏览器失败是驱动错把 Inbox 的 `events` 读成 `items`，修正驱动后通过；没有改 API 回包或放宽断言。

开发中曾出现空 `active_runs` 序列化导致旧完整结构断言失败，改为跳过空内部字段保留旧数据形状；测试模块私有可见性、显式导入、静态 fixture index 和 Vec 借用类型错误已修正；严格 Clippy 的 wildcard imports、clone、条件合并和分号/contains_key 提示均修正，无新增 lint 豁免。这些失败不算成功验收。

新 QA 九类制品已在独立输出目录构建并完成逐包检查，首次安装态失败与同路径复测另记于 [Cron QA 制品](qa-cron-packages-20260909.md)；不得将旧 DMG 或上个 release 的 SDK 结果记作本切片包内验证。

显式整组首轮 **10/11**（251.12 秒），Provider OAuth 的 Models 重载未完成，页面仍显示 `LOADING CONSOLE`；记录到的 API 全部 200/201，没有失败 API。该轮与 Console production build 时间重叠，但尚无充分证据将失败归因于资源重建。一次独立复测的终端输出在上下文交接时丢失，未计作通过；确认进程已结束后重新独立验收 **1/1** 通过（14.90 秒）。随后在 Console 构建结束后重跑整组 **11/11** 通过（188.46 秒）：10 项原页面浏览器与 1 项 APScheduler 差分。不能把复测成功改写成首次整组全绿或故障已修复。

新 release SHA-256 为 `586d41084b04d352bce99dec6c11ce900ec56279d7da87aee060a2126d2a6ebe`，显式连接该文件的源码 TS SDK **4/4**、VS Code **57/57** 通过，扩展编译通过。输出目录为 `dist/qa-runtime-20260909-WmuXFt/`。首次 DMG 创建资源忙，确认无挂载/目标文件后原命令重试成功；原失败日志保留。首次重试启动未找到 PATH 中的 Node，未执行 hdiutil，随后使用明确 Node 24 路径执行成功。安装态另记，不将源码客户端结果混入包内验收。

安装的 Python SDK 对本切片源码 Core 对照 **5/5** 通过；同 SDK 首次连接解包 Core 两项失败，原生版本探测记录 SIGKILL（11.092 秒，非主动超时）。原文件原路径完整复测的四处包内 Core、两种 SDK 与两类 VSIX 客户端均通过；原始失败不覆盖。原 Console 1311 文件与之前完全一致，legacy 安装态 855+36 项通过。测试服务与只读 DMG 挂载已释放，`git diff --check` 与原前端零 diff 检查通过；未提交/推送或使用生产 key。全功能门禁与首次启动根因继续未完成。

## 持久化目标与归属前置切片

此节是上述 QA 制品之后的源码变化，不将旧包记作包含此功能。原非默认 Job HTTP 仍为 501，后台仍跳过非默认 Job；只完成解开该门禁所需的持久化与隔离基础。

- 候选目标从真实 Agent 聊天目录读取，精确保留 channel/user/session、去重、顺序、keyword/channel/limit 和 console 兜底。重开后结果一致，归档聊天行为对齐原目录查询；内部 alias 不导出。Agent 不存在/禁用/非法 ID 分别拒绝；limit 越界、负数或非数字统一 422。原失败先复现硬编码 admin、NUL alias 和非默认候选固定 501。
- 内部 owners 与 Job JSON 的业务 meta 分离，无归属原生记录仅属 default。含 owners 使用持久化 v2，拒绝 v1 冒带 owners、非法/悬空归属、重复 Job ID、未知版本和超限；旧仅接受 v1 的读取逻辑不能静默把非默认 Job 当作默认。版本测试是格式门禁单测，不是旧二进制降级实机验收。
- 默认列表过滤其他归属，已知其他 Job ID 的 CRUD/run/state/history 一律 404，定时 tick 不执行其他归属。恢复孤立 run claim 只中断相同 Agent 的 Cron trace，不因 run ID 相同修改其他 Agent trace。
- 实际备份管线按选中 Agent 筛选 Job、owners、状态、历史、时间游标和执行声明，剔除无 Job 的关联记录；恢复只替换所选归属，拒绝跨范围 Job/run ID 冲突及合并后超限。现有 ZIP 范围测试和真实 Agent archive 联合回滚测试增强为完整 Cron 结构比较，未用纯 helper 测试替代实际备份。
- 原 Agent Cron 浏览器改用持久化已知 user/session 选项，同时断言其他用户和其他 Agent 的候选不显示；仍验证实际模型/工具往返、History 和无额外 Console 文本气泡。独立 **1/1** 通过（15.26 秒），两次后续整组中该项也通过。

新增四项归属/候选运行测试与六项备份格式测试。最终 conda qwenpaw 全工作区普通测试 **448/448** 通过，严格 App Server Clippy 通过。初次未使用 conda 的工作区运行在本地模型备份 fixture 启动请求超时；同一测试独立复测通过（3.30 秒），后续 conda 整组通过，但没有证明环境或时序是唯一原因，也未增加超时。初次 lint 中两个过长函数通过拆分职责修复，没有新增放宽 lint。fmt/diff、Node 驱动语法检查、API inventory 3/3 与快照检查通过；`console/src` 零 diff。

最终功能 release 构建通过（50.05 秒），SHA-256：`28576bbda41cab95ae51c28bcfe8f7015136fd2a298022e5d125252853aa4373`。明确设置该 `QWENPAW_CORE_BIN` 后，源码 TypeScript SDK **4/4**、conda Python SDK **5/5**（0.590 秒）、VS Code **57/57** 通过，SDK/扩展编译通过，未跳过真实 Core 测试。此后仅新增浏览器诊断与测试报告字段、修正注释，没有修改该二进制对应的生产功能。**本切片未重建九类 QA 包**，旧 DMG/VSIX 的安装态结果不能移作本切片结果。

### 浏览器失败与诊断，不关闭问题

第一轮显式整组 **10/11**（202.34 秒），备份 roundtrip 在创建后未找到表格行；Modal 仍打开、Confirm 可用、没有失败 API。检查原 Modal 可见 afterOpenChange 会重置默认名称及 job runner，旧驱动只等待 input 出现就输入，存在初始化时序缺口。但当时报告没有输入值，不能据此断言这次失败唯一由该缺口引起。

测试驱动现先等待原 Modal 初始化名称，再输入并等待名称与 Confirm 可用；不修改 Console、不增加超时、不模拟成功 API。独立两项备份浏览器 **2/2** 通过（80.23 秒），覆盖实际创建、刷新、导入导出、信任/冲突、恢复、删除，以及活动任务刷新/SSE/取消/后续创建。

第二轮显式整组仍 **10/11**（203.72 秒），这次所有备份操作通过，失败发生在 `/inbox` 导航：浏览器打印 `Failed to load agents: TypeError: Failed to fetch`，失败 HTTP 状态列表为空。它与第一次创建失败不同；硬导航取消旧页面请求是待验证假设，不是已确认根因。

新增仅用于报告的诊断：API 请求起始页面、document/loader、取消/失败、导航前未结束请求，以及报错 JavaScript context 的创建页面。只记路径/标识，不记 query、请求头或正文，保留原错误判定。独立诊断单测 **3/3** 通过，包含旧页面取消、上下文归属、既有错误监听继续收到事件和敏感字段不进入报告。待原场景捕获足够证据再判定根因，不能因诊断存在或后续一次通过而关闭问题。

接入诊断后，同一原备份 roundtrip 独立 **1/1** 通过（66.43 秒）：24 页导航全部通过、导出 ZIP 21378 字节、导入状态依次为 409/200/400/200，创建/刷新/信任/冲突/直接恢复/恢复前备份/删除完整断言通过。此轮没有复现 fetch 错误，因此未取得其根因证据。测试报告字段变更后严格 Clippy 再次通过（6.74 秒）；该次独立通过不改写此前两次整组 10/11 的记录。

剩余功能仍包括：非默认 HTTP 归属赋值及模型/Workspace 真正执行、审批与 Console 推送归属、Agent 关闭/删除的排队任务、Agent Copy 的 SQLite Job 复制、外部渠道和全部原控制语义；完整桌面 GUI、生产签名/公证、跨平台实机及首次安装稳定性也未完成。无需生产 key 即可继续上述本地 fixture 开发；本轮未提交/推送或访问日常凭据。
