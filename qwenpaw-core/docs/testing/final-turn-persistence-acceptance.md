# 最终 Turn 保存失败验收

日期：2026-09-10，macOS ARM64。实施方案见
[最终保存边界](../architecture/final-turn-persistence.md)。本记录随顺序验收更新，
未完成项保持开放，不将上一批 `WXFYc3` 制品算作已包含新代码。

## 已实现和直接验证

- 最终 upsert 失败时，现有 `turn/completed` 中的 Turn 为 failed，包含
  明确的未保存提示；保留 items、既有模型错误和上一个成功检查点。Thread
  为 error，不尝试绕过失败再写一次，也不产生成功保存回执。
- Core 实例保留最终写入失败记录；正常后续任务仍可执行。后续成功写入、
  恢复不清除此前失败；关闭全部入场并排空服务后，stdio/HTTP/WSS 都检查
  该记录并返回错误。新实例沿用已有 interrupted 启动恢复，不承诺断电保证。
- 首个失败回归确认旧逻辑仍为 Completed 而不是 Failed（1 项失败，0.07 秒）；
  修复后原持久化组 6/6（0.22 秒）。新增最终完成/中断/原模型错误矩阵直接
  比较完整公开 Thread/Turn、未改写的磁盘 journal、旧 checkpoint、失败提示
  和恢复后的失败记录。测试使用私有 SQL 错误标记，客户端提示不包含它。
- 三宿主结果矩阵通过：使用临时 SQLite 的真实最终写入失败，再让 stdio、
  HTTP、带临时认证/TLS 材料的 WSS 分别关闭，检查返回的准确错误和服务收尾。
  此矩阵不声称三种连接都完成了真实客户端故障交互。
- Rust SDK/CLI 3/3：正常连接、正常关闭保存、真实 source Core 最终保存
  被触发器拒绝。故障时只关闭 stdin，SDK 收到 Core 退出码 1；重开前完整
  Thread/Turn 仍等于旧 inProgress 快照。随后移除触发器、重开，才检查原有
  interrupted 恢复。这区分了“退出时已保存”和“重开时修复旧 journal”。
- Console/Cron/Heartbeat 的失败钩子通过：Console 最终 SSE 结构明确 failed，
  SSE 和内存历史都保留原回复；Cron 记录 error 而不是 success；三个生产者
  都不创建错误的自动快照。原先仅搜索 stream 中 completed 字符串的断言改为
  比较最终 response，避免被中途已完成的 message 事件误导。

## 回归和仍未定位的 SDK 超时

- 第一次完整工作区停在既有 Rust SDK 非零退出用例：7/8，5 秒超时，EOF
  阶段文件尚不存在。App Server 453/453 和 HTTP 36/36 在此前已通过，不能
  用它们追认整个工作区成功。原始日志保存在本轮输出目录 `rust-workspace.log`。
- 临时加入 worker 阶段打印和子进程收到的方法标记，库复验通过；四轮各
  四个并发自有子进程均能 EOF/排空/非零退出（专项 1/1，3.64 秒）。之后
  删除临时打印，保留阶段文件和并发回归，原 5 秒、正常 30 秒、强制清理
  5 秒的时限均未放宽，未串行化生产进程创建来掩盖竞争。
- 最终完整工作区 **707/707**，另有 26 项随后显式通过；Rust SDK 9/9（31.31
  秒）、Rust SDK/CLI 3/3（3.11 秒）、Core 127/127（3.01 秒）、App Server
  453/453（14.09 秒）、HTTP 36/36（4.55 秒）。通过不证明首次超时根因已解决。
- Clippy 首次仅报新增共享测试函数 113/100 行；拆出模型夹具后通过，未关闭
  lint。最终 `cargo clippy --workspace --all-targets -- -D warnings` 4.23 秒。
- 原前端 **295 文件、2453/2453，72.42 秒**，`console/src` 零 diff；保留
  测试夹具主动触发的错误日志和 jsdom 未实现提示，没有以静默日志代替通过。
- 显式浏览器/参考 **26/26**，App Server 25 项 307.49 秒、CLI 浏览器
  1 项 15.39 秒；无跳过。这些既有用例不替代保存失败的前端浏览器专项。
- source release 构建 53.89 秒；release Rust SDK **3/3**（1.24 秒），
  TypeScript **9/9**（30.386 秒），Python **16/16**（32.883 秒），
  VS Code 编译与单测 **57/57**（0.187 秒），均无跳过。
- `verification.json` 核对最终九条命令的退出码与顺序时间戳、原前端零 diff、
  source Core 哈希及无遗留 source Core 进程。该阶段 source release SHA-256：
  `51ba72f69ce147781d49b66529fe11c0cb55bd6efb5e8799e345dfcdc9dfdc93`。
  该报告明确 `newPackagesBuilt=false`、`packagedRuntimeTested=false`；上一批
  `WXFYc3` 安装包不包含此修复，不能作为当前源码的安装态验证。

日志目录：`dist/qa-final-persistence-20260910-78QvFt`（产品仓库根目录）。
其中每个记录的命令都有独立 `.log` 和退出码/时间戳 `.json`，失败日志不覆盖。

## TS/Python 真实 Core 故障及安装态补齐

同日后续输出：`dist/qa-sdk-final-persistence-20260910-TJWoJ3`。本步只补
SDK 测试及其安装检查数量断言，不改 SDK 生产逻辑、Rust Core 或原前端。
使用以上同一 SHA-256 的 source release Core，未执行分发 Core。

- 两种 SDK 都等待 loopback 模型请求进入，再在临时 SQLite 创建拒绝最终
  Turn 写入的触发器。首次及重复 close 均准确报告退出码 1；重开之前以
  只读连接比较完整 Thread/Turn，含原 updatedAt，仍等于旧 inProgress。
  只有比较通过后才移除触发器并重开，检查完整 interrupted 恢复结果。
  原正常关闭用例仍单独执行，不能把失败用例的恢复当成成功关闭保存。
- 最终源码顺序复验：TypeScript 编译及 **10/10，32.508 秒**，Python
  **17/17，33.419 秒**。这取代本记录上一阶段 SDK 的 9/16 项数量；原始
  日志保留。两者均无跳过，正常 30 秒超时测试没有缩短。
- TypeScript 新打包、离线隔离安装后 **10/10，32.495 秒**；检查实际导入
  目录和完整生产载荷哈希，测试引用安装目录中的代码。产物：
  `typescript-Zo8eOI/qwenpaw-sdk-0.2.0.tgz`，SHA-256：
  `931e3f7b4b1181710bf0a72f2192d34c6e5797621a9707936692884876a7df66`。
  与上一批 SDK 包相同是因为 SDK 生产代码未改，不代表它与旧 Core 的组合
  也拥有本次修复。
- Python 新 wheel、离线隔离安装后 **17/17，33.422 秒**；实际导入路径和
  全部模块哈希与 source SDK 对齐。产物：
  `python-jLL4NU/qwenpaw_sdk-0.2.0-py3-none-any.whl`，SHA-256：
  `7455c0d2886c4d695ba724eecb1adbd799cf231c9e05636e645aaade98c17039`。
- 首次 Pylint 报共享测试 59/50 条语句，拆出重开检查后 **10/10**，未禁用
  语句数规则。局部 pre-commit 的 AST、mypy、Black、Prettier 等通过；
  Flake8/Pylint 单独运行，保留用户 f-string 规则所需的 F541/W1309 例外
  （另保留 E203），无全局配置改动，首次失败日志不覆盖。
- 本轮 `verification.json` 核对两种源码测试和两种安装测试的顺序、全部
  退出码、包哈希、四个改动测试/检查文件的哈希、Core 哈希、原前端零 diff
  和无遗留 source Core 进程。没有重建 DMG 或声称完成前端保存错误场景。

## 原页面刷新丢失错误的修复

同日后续输出：`dist/qa-browser-final-persistence-20260910-1e1gjk`。本步真正
修改 Rust 历史适配；前两轮 `51ba72...` source release 与 SDK 安装报告不能
直接充当本步源码验证，新的 release/客户端和整套制品需独立复验。

- 第一遍浏览器已看到回复和保存失败提示，但测试的“journal 只有用户输入”
  假设错误：Core 会在模型/工具步骤中保存中间 items。该失败 11.36 秒，
  日志 `browser-red.log` 保留。修正为比较完整中间 Turn/items，仍要求
  inProgress / 无最终 error；不把已保存中间回复当成最终写入确认。
- 第二遍确认产品缺口：刷新后回复仍在，保存失败提示消失，历史请求均为
  HTTP 200。`browser-history-red.log` 记录真实页面文本和诊断，46.58 秒。
  原历史接口只遍历 Turn.items，因此丢弃了 Turn.error。
- 修复仅在历史适配追加原 Console 已支持的 error 消息：稳定的
  `turn_id + "_error"` id、assistant role、failed status、空 content、原
  error.message 和既有 timestamp。原消息顺序、内容与图片输入重建保留；
  不改变协议版本，不写数据库，不改 `console/src`。
- JSON 矩阵 1/1 比较正常完成、中断、带回复/无回复失败的完整输出及稳定 id。
  原 Console 保存故障测试也增加真实 HTTP history 错误消息断言。
- 浏览器修复后 **1/1，14.08 秒**：原输入框发送 → 回复/错误同时可见 →
  测试管道暂停并只读核查 journal → 同一宿主刷新后回复/错误仍在 → journal
  完全不变 → 移除临时触发器 → 原输入框再次发送并刷新。模型四次请求和
  两次实际用户文本均准确，避免将旧草稿拼接误判为新消息成功。
- 最终完整磁盘 Thread/Turn 与内存一致，两轮分别为 failed/completed；
  只有成功轮次产生自动检查点。HTTP 宿主收尾仍准确报告早先的保存失败，
  后续写入不追认此前失败。此处刷新不是 Core 重启，不能代替启动恢复测试。
- 严格 Clippy **14.06 秒**通过，未增加 lint 例外。完整顺序回归继续记录，
  浏览器专项不代替全产品交互、分发运行或原生端验收。
- 完整普通工作区 **708/708**，27 项随后显式执行；其中 App Server
  **454/454，13.91 秒**、HTTP **36/36，8.53 秒**、Rust SDK **9/9，30.07
  秒**、真实 Core SDK 集成 **3/3，2.23 秒**。首次通过不证明此前 SDK
  偶发超时根因已解决。原前端 **295 文件、2453/2453，64.18 秒**，源码
  目录的 diff 和状态均为空；保留主动失败夹具的 stderr 与 jsdom 提示。
- 后续显式浏览器/参考 **27/27**：App Server 26 项 **319.64 秒**，CLI
  浏览器 1 项 **15.39 秒**，无失败或跳过。新增错误刷新场景也包含在整组中。
- 新 source release 构建 **52.98 秒**；SHA-256 为
  `19e4548ad6401da74e13490d25a3995c3332da3a1e2542cbf193a1dde5b11092`。
  优化版 Rust SDK **3/3，1.25 秒**、TypeScript 编译及 **10/10，30.390 秒**、
  Python **17/17，33.387 秒**、VS Code 编译及 **57/57，2.983 秒**顺序通过，
  全部连接此 source Core，无跳过，不是 VS Code 原生激活测试。
- 本轮 `verification.json` 验证最终九条命令的顺序/退出状态、各组数量、
  两次失败日志、六个源码/测试文件哈希、新 Core 哈希、前端目录无变更和
  无遗留 source Core 进程。`newPackagesBuilt`、`packagedRuntimeTested`、
  `nativeActivationTested` 均为 false；没有用源码通过来追认旧安装包。

## 后续整套制品验收

历史刷新修复后的 source Core `19e454...` 已进入独立九类 macOS ARM64 QA
批次 `qa-runtime-20260910-2qPEew`。静态校验、2886 条来源和四份各 1311 个
原 Console 文件比对通过；安装后 TS 10/10、Python 17/17、保留版 CLI
855/855 + 36/36 顺序通过。两种 VSIX 隔离安装但未激活。详情及最终报告
见 [整套制品验收](qa-final-persistence-packages-20260910.md)。以上是后续
批次的新证据，不修改前述源码阶段报告的 `newPackagesBuilt=false`。

## 后续门禁

- [x] 完整显式浏览器/参考 26 项，以及新 source release 和语言客户端复验。
- [x] TypeScript、Python 实际 Core 保存失败专项，以及两种 SDK 安装态全套。
- [x] 前端保存错误显示、刷新与下一轮恢复的真实浏览器专项。
- [x] 本次历史适配修复后的全库、原前端、显式浏览器、release 和客户端顺序复验。
- [ ] SDK 5 秒偶发超时的确定根因与修复。
- [x] 最新九类 QA 制品、静态来源校验及上述 SDK/保留版 CLI 安装态复验。
- [ ] 分发 Core 启动、原生 GUI/扩展激活和跨平台。
- [ ] 默认 CLI/SDK Workspace 初始化、后台任务归属及其余原产品功能等价。

本步不使用真实 key/keychain、不操作日常数据、不运行分发包 Core，不清理
缓存或旧包，不 commit/push/发布。
