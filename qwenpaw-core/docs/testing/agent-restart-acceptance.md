# Agent 重新启用与 Cron 生命周期验收

日期：2026-09-09。承接计划 §14.2.24.43；这是完整原功能等价目标的一个切片，不是全功能交付声明。

## 实现与原版语义

原 `app/crons/manager.py` 每次构建 CronManager 时初始化空 `_states`，启动时重新注册 trigger，另存执行历史。原 Cron/IntervalTrigger 从当前时间计算下一槽；一次性 DateTrigger 保留原 run_at，包括过期时间，交给 misfire 规则判断。无效 trigger 在启动时禁用，不额外制造执行历史。

Rust 仅在 Agent false → true 时准备对应 Cron 新状态，重置最近运行字段并重建游标；其他 Agent、任务规格、公开 ID 和历史保持。无任务不创建 Cron 数据，已启用的重复请求不读取/改写 Cron。零间隔、未知类型及其他无效 schedule 在计算前校验并禁用；未完成的持久化声明返回 409，不被重置抹掉。

创建、复制、启停和删除共用 Lifecycle 锁，锁顺序为 Lifecycle → Cron → Agent。关闭/删除发布后捕获取消与完成令牌，释放 Cron/Agent 锁再排空，保留 Lifecycle 锁。复用原 HTTP 中间件独立任务及 Core operation guard，不新增重复的异步包装。

重新启用先提交 Cron，再发布 Agent 索引。SQLite 失败不发布 enabled；索引失败回滚 Cron 原始字节；回滚也失败则返回明确恢复错误，Agent 保持 disabled。该保证针对普通错误处理，不是 SQLite/文件跨存储的崩溃原子性。

## 本机验证

- 新增 8 项普通测试，最终定向 **8/8**（0.42 秒）。完整数据比较覆盖固定时间的各 trigger、无效零间隔/类型、其他 Agent 活动声明、历史、关闭后重开再启用、重复请求、空/损坏数据、未收束声明、写入及发布失败。
- 发布失败夹具采用有效的紧凑索引：每个配置和字段均在限制内，重新格式化发布时才超过总容量。SQL trigger 分别模拟首次写入和回滚失败。成功回滚必须恢复包括空白在内的原 Cron 字节，不只是等价 JSON。
- 并发测试运行实际本地模型任务，暂持 Inbox 锁让 finish 等待；关闭调用方被取消后，重新启用仍不能越过 Lifecycle 边界。放开锁后旧任务记录 cancelled，随后最近状态重置，历史和文件副作用断言通过。
- 原 Agent 页面显式场景已通过：选择 Writer、关闭、重新启用、删除 Editor、刷新；默认审批不变，Writer 旧两次取消历史保留而最近状态清空。修改动作均使用原按钮/确认框，浏览器 fetch 仅用于读取验证。最终显式整组 **15/15**（245.95 秒）通过，包含 14 项原页面场景和 APScheduler 3.11.3 的 16 案例差分。本次没有复现此前导航 fetch 间歇性错误，不据此宣称已定位其根因。
- Rust 工作区普通测试 **488/488** 通过，15 项显式环境测试另行执行；新增 helper 的最终定向回归随后再次通过。严格 App Server Clippy 通过（6.54 秒），fmt/diff、Node 驱动语法检查通过，`console/src` 零 diff。
- Console inventory **3/3** 与快照检查通过，仍有 370 项调用、38 项没有注册 Rust 路由；这不是全接口完成。
- Core release 构建通过（约 77 秒），SHA-256：`37d3bdd238cbb5f0076f69d845c6db678f8a19e8dc562efb8eaacfdedf37d461`。显式使用该二进制后，TS SDK **4/4**、conda Python SDK **5/5**（0.601 秒）、VS Code **57/57** 及编译全部通过，无跳过。

测试开发阶段修正了错误的 HTTP 方法、过早命中字段限制的故障夹具和测试 helper/lint 问题；没有放宽生产验证或删除失败断言。

## 未完成与制品

非默认 Job HTTP 仍返回 501，后台仍跳过非默认 Job。删除后重新注册同 ID/同 Workspace 或不同 Workspace 的数据归属尚未完成，不能让新 Workspace 继承旧任务，也不能以删除旧文件代替隔离。普通聊天和其他后台服务的完整 Agent 停机、渠道投递及各端原交互还需继续逐项验收。

本轮模型请求仅使用隔离本地 fixture，没有生产 key 或日常 keychain 访问；没有启动日常桌面数据目录的原生窗口。旧九类 QA DMG/VSIX 未重建，不包含本切片；源码构建不等同于 Windows/Linux 实机、生产签名/公证或安装态验收。未提交/推送。
