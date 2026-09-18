# Project Directory 多 Agent 验收

2026-09-14 补充：Cron 全量页面回归暴露目录列表的静默 500 项截断，原 Python
没有此限制，原目录页面也没有分页。512 目录完整 HTTP 回归先失败，移除
Rust `browse_directories` 的 `.take(500)` 后两 Agent/隐藏开关 4 组完整响应
通过；没有清理系统临时目录或修改原页面。最终回归和新 source Core 边界见
[本轮验收](cron-public-scope-acceptance.md)，下文保留 2026-09-10 历史记录。

日期：2026-09-10。对应 [设计与 checklist](../architecture/project-directory-ownership.md)。
保持 `console/src` 零改动；使用临时数据、无秘密凭据存储、回环服务器和独立
Chrome profile。Git 来源为自己创建的本地裸仓库，未测试远程凭据或真实模型。
未执行任何分发包中的 Core，未清理缓存或旧包。

## 缺陷与修复证据

- 首批 5 项测试全部复现默认 Workspace 串目录：四种写入和列表未使用请求
  Agent。修改为单次捕获的基础 Workspace；不能以当前选中项目作为项目根。
- Agent 身份在获取项目锁后再次验证。异步 clone 捕获数据身份，在注册锁内
  发布项目选择；旧 ID 被删除/复用时拒绝，不解释为同名新 Agent。
- 配置/注册发布失败执行明确回滚，保留默认 `agent.json` 原先不存在的状态；
  Core 全局 restore 期间拒绝新写操作。普通回滚不代表跨文件崩溃原子性。
- 两项新响应测试及原 Python 对照最初全部失败：PUT 多返回 `exists` 和
  `workspace_dir`，显式选择基础 Workspace 的 `is_workspace_default` 应为 false；
  list 应将 `.git` 文件也认作 Git 项目。现已按原处理器修复。
- 原 HTTP 集成断言从错误的 PUT `exists` 检查改为原契约三字段完整 JSON，
  GET 五字段保持不变，没有用删断言来获得通过。

## 项目专项

命令：`CARGO_INCREMENTAL=0 cargo test -p qwenpaw-app-server --lib desktop_project_ownership_tests -- --include-ignored --test-threads=1`，
通过 **14/14，17.95 秒**，无跳过：

- 普通 12 项：同名项目隔离、独立列表、本地导入、multipart ZIP、真实 Git
  clone SSE；排队期间删除/重建 Agent；旧身份完成发布拒绝；全局 restore
  期间配置/注册/选择不变；Unix 项目存储 symlink 拒绝；PUT/reset 完整响应；
  `.git` 文件识别；失败切换与服务重新打开保持两个 Agent 的选择和项目根。
- Python 参考 1 项：从原路由抽取不改写的 `SetProjectRequest`、GET、PUT、list
  和 `_projects_base`，执行两 Agent 共 20 步，逐项比完整状态码和 JSON。
  仅 Agent 解析、当前配置读取与保存替换成内存夹具，不加载旧产品初始化；
  此参考不证明原 Python 持久化层或所有非法路径输入的等价。
- 原页面 Chromium 1 项：通过实际侧边栏选择 Writer，创建与 Default 同名项目，
  Recent 切换、默认目录重置；“打开目录”实际逐级导航到临时夹具，切换隐藏
  文件夹、进入子目录、返回上级、刷新并确认；真实本地 clone；通过原隐藏
  webkitdirectory input 选择临时目录，经原 JSZip/multipart 上传；刷新后恢复，
  最后切回 Default 核对原选择完全不变。没有替换前端 API 或直接修改 React 状态。
  浏览器初始 Home/根目录仅列目录名；所有新增目录和文件均属于测试夹具。
- ZIP 保持原顶层文件夹布局，`node_modules` 由原页面排除；实际文件内容、
  `.git` 目录、导入源未变和 Default 没有 Writer 项目均由 Rust 断言验证。

## 检查与后续门禁

- [x] 严格 workspace Clippy：15.46 秒，无规则豁免。浏览器测试拆分文件断言
  helper，未屏蔽 `too_many_lines`。
- [x] 浏览器工具/诊断和 API 分析单测 19/19，无跳过；新脚本语法检查通过。
- [x] Python 参考脚本 pre-commit 的 AST、格式、类型、私钥与空白检查通过。
  用户要求所有字符串使用 f-string，与 F541/W1309 冲突：这两个 hook 另行执行，
  Flake8 仅豁免 F541，Pylint 豁免 W1309 和该 AST 测试已有风格例外
  W0122/C0116/R0914；检查通过（Pylint 10/10）。没有修改全仓库规则。
- [x] 完整 Rust workspace **686/686**，另外 26 项进入显式组；App Server
  442/442（18.69 秒）、HTTP 36/36（8.07 秒）、Core 126/126（2.98 秒）、
  Rust SDK 1/1（3.06 秒）、CLI 集成 3/3（1.46 秒）。
- [x] 原前端 **295 文件、2453/2453，104.34 秒**；`console/src` 零 diff，
  `git diff --check` 通过。API inventory 防漂移检查通过：370 调用、34 未匹配，
  未据此宣称所有功能已完成。
- [x] 全部显式浏览器/参考复验、源码 release 与各客户端回归。
- 源码 release 已构建成功（57.31 秒），SHA-256
  `8dae6f31ae31c4da50f9d028e3253013d832346e919fc9e4ea930a05a119100f`。
  对接这份源码二进制：TypeScript SDK 4/4（0.739 秒）、Python SDK 5/5
  （0.581 秒，校验实际导入路径且无跳过）、VS Code 编译及 57/57（0.186 秒）。
  这不是包内运行或 VS Code 原生激活验收。
- 首次显式 App Server 组 **24/25，323.57 秒**：
  `original_agents_page_stops_scoped_cron_without_touching_default_approval`
  的页面报告 `ok: true`、全部生命周期操作标记 true、无 API 错误，但 Chrome
  收到关闭请求后未在现有 2 秒内退出，进程成功断言失败；不能把页面通过
  当作整个用例通过。此轮与源码 release 编译有重叠，但尚未证明因果。
  已核对原测试及所属 Chrome 进程结束；保持退出断言和时限，进入独立复测。
  Cargo 在 App Server 失败后未执行后续 CLI Debug 显式用例，需单独补跑。
- 同源 `-p qwenpaw-app-server` 独立 Cron 场景 **1/1，10.44 秒**通过，CLI
  Debug 原页面 **1/1，16.04 秒**通过。前者 package-only 的 feature-unified
  测试二进制与 workspace 不同，不能追认首次失败的根因；正在用完全相同的
  workspace 命令重跑全部显式组，不与构建并行，退出检查仍为原来的 2 秒。
- 最终同命令 workspace 显式组 **26/26** 全部通过：App Server 25 项
  **313.09 秒**，CLI Debug 1 项 **15.58 秒**，无跳过、无时限或断言修改。
  这是独立的最终成功记录，不抹掉首次失败，也不证明 Chrome 退出偶发超时
  的机器环境原因已被修复。
- [x] 使用当前源码重新构建九类 QA 制品并逐项静态/隔离安装验收，见
  [本轮制品记录](qa-project-packages-20260910.md)。DMG 首次资源忙及原参数
  重试成功分别记录；没有重试包内 Core 执行。
- [ ] 分发包运行、原生 Desktop/WebKit/VS Code 激活、Windows/Linux/macOS x64。

已通过排队和发布入口的旧身份测试，不据此宣称运行中 Git 的所有删除/替换
时序、外部进程更换目录、请求断连取消均已解决。浏览器断言不替代原生客户端
验收，20 步成功请求也不代表所有路径/错误语义完全等价。

现有 `dist/qa-runtime-20260910-ZPQQT0` 是上一轮 Debug 制品，**不包含此修复**。
新批次 `dist/qa-runtime-20260910-vGGX8Z` 已包含此修复，九类制品构建、静态
对照和隔离 SDK/VSIX/legacy 安装检查完成。原生、跨平台和全功能门禁仍未关闭。
