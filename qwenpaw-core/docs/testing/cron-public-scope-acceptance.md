# Cron 公开多 Agent 验收

日期：2026-09-14。方案：[公开接入及架构图](../architecture/cron-public-agent-scope.md)。
证据目录：仓库根 `dist/qa-cron-public-scope-20260914-1caPG3/`。

## 行为变化

原 `app/crons/api.py` 按当前 Workspace 选 CronManager，POST 生成 ID，PUT 是
create-or-replace。Rust 公开入口现按请求 Agent 的有效注册绑定解析
WorkspaceDataKey + public ID；读写、状态、历史、取消仍用内部键。请求正文的
metadata 不能转移归属。相同公开任务 ID 可同时属于不同 Workspace。

后台校验各任务实际归属，不再跳过所有非默认 Agent；无效/停用/已删除的
归属不派发，不影响健康副本。手动请求在检查点恢复期间仍立即返回 started，
恢复后必须验证固定工作区身份及原内部键，不能转到后来重建的同名任务。
审批保持原全局 Inbox 的 request ID/root session 行为，不按当前选中 Agent
错误屏蔽其他 Agent 的合法审批。

## 回归与失败记录

- `public-scope-red-compiled`：新增首批 4 项均真实失败（501 或未调度）；
  `public-scope-green-initial` 四项全绿。首次红测编译的断言宏歧义单独保留。
- `cron-expanded`：214 通过、1 失败、13 忽略。唯一失败是旧测试要求 foreign
  ID 的 PUT 返回 404；原 API 实际允许本范围新建。仅移除错误的拒绝预期，
  新独立测试完整比较其他范围的原任务不变；其他跨范围 404 断言全部保留。
- 7 项公开测试覆盖 CRUD/启停/run/state/history、伪造 meta、同名 PUT、并行
  模型/文件/会话、30 项无效 scope 请求、健康复制任务调度、删除后名称复用与
  保留目录重新注册。恢复测试新增 writer 范围，原 default 用例仍保留。
- `full-workspace` 首次全量 741 通过、28 忽略；随后扩充的最终结果见下节。
- `browser-writer` 原页面专项 1/1（12.92 秒）：所有修改来自原 UI，fetch 仅
  验证结果；切换 writer、候选目标隔离、创建/启停/真实工具执行/trace/历史/
  编辑/刷新/删除，完整默认 Cron 数据及默认工作区文件不受影响。
- 严格检查修正了新增函数长度和 if-let 风格；提取 helper 时漏导入类型的
  编译失败保留。`clippy-final` 已通过，最终测试补充后再次检查。
- `workspace-verified`：466 通过、1 失败、27 忽略。新增同名重建边界的状态/
  历史断言通过，但 fixture 请求数量错写为 1；该 Agent 会先工具调用再模型
  收尾，实际是 2。改为准确比较最后消息角色 `[user, tool]`，不删除任务状态、
  历史和无文本投递断言。最终重跑保留独立日志。
- `browser-suite`：**26/27**（362.23 秒），Cron/审批/复制/Agent 生命周期
  均通过。唯一失败是原项目目录浏览找不到临时 fixture；请求 200、无网络
  错误。Rust 列表排序前 `.take(500)`，原 Python 没有此截断，原页面也没有
  分页入口。新增 512 目录（另含隐藏目录/普通文件）的完整 HTTP 回归，
  不清理系统临时目录、不换 fixture 路径绕过。修复及重新验收继续记录。
- `directory-red` 编译后 0/1，完整数组证明目录遗漏；仅移除该入口 `.take(500)`
  后 `directory-green` **1/1**（0.42 秒），两 Agent × 隐藏开关共 4 个完整响应。
  没有调高截断值、删除数据或改前端来让测试通过。

## 最终验收

- [x] 原前端 295 文件、2453 项测试通过（94.29 秒），源码未改。
- [x] API inventory 3/3 与快照通过：370 调用、34 未注册；不据此宣称功能等价。
- [x] 浏览器诊断/关闭驱动 16/16。
- [x] source release workspace 首次构建为 `a7826546...`（1m03s）。目录修复后
  `release-directory` 再构建通过（58.00 秒），最终 Core SHA-256：
  `16e722bde0ea4d40010666cec376c338f99e36628ac993906d9ec5dc592f7053`。
- [x] 目录修复前 `workspace-roundtrip` 为 742；最终 `workspace-directory`
  完整 Rust **743 通过、28 忽略**，其中 App Server 468 通过、27 忽略；
  `clippy-directory` 全工作区/all-targets 严格检查及最终 fmt/diff 通过。
- [x] `explicit-directory` 全工作区全部 **28/28** 显式契约/原页面通过：
  App Server 27 项（338.17 秒），CLI Debug 1 项（15.55 秒）。原项目目录
  逐级浏览、隐藏过滤、创建/克隆/导入/刷新恢复全部通过。没有缩短场景、
  放宽断言或换路径；旧的其他浏览器间歇性问题不据此统称已修复。
- [x] Cron Core 的 TypeScript **26/26**、Python **37/37**（含 TS/Python
  共用宿主）、VS Code 编译及源码 **73/73**依次通过；release Rust SDK **3/3**。
  Python 实际导入本仓源码，conda qwenpaw 的 websockets 为锁定的 15.0.1；
  `python-imports` 保留准确路径。所有客户端测试无跳过，没有原生插件激活。
- [x] 目录修复后的最终 source Core 再次依序通过 TypeScript **26/26**、Python
  **37/37**（36.231 秒）、VS Code **73/73**及编译；release Rust SDK **3/3**。
  日志分别为 `typescript-directory`、`python-directory`、`vscode-directory`、
  `rust-release-sdk-directory`，不与之前二进制的结果混写。

只用临时数据、本地模型和假凭据；没有真实 key、钥匙串、包内 Core 执行、
原生 Desktop/VS Code 激活、commit/push。本切片源码验收时的九类 `737ANs`
**不含本轮 Cron 改动**；随后 [新批次 `4v2E9d`](qa-cron-packages-20260914.md)
已纳入改动并通过构建、静态及隔离安装测试。外部渠道、全部旧控制语义、
默认共享宿主、包内运行/原生/跨平台验收仍未完成。旧浏览器间歇性失败不因
本輪通过被统称为已修复。
