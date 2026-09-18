# PawApps 目录与原 App Center 验收

日期：2026-09-10。范围见 [设计/checklist](../architecture/pawapps-runtime.md)。
仅使用 TempDir、虚构 manifest、内存凭据、回环 HTTP 和独立 Chrome profile。
未访问旧 Python 用户目录或系统钥匙串，未执行分发包里的 Core。

## 实现与初始证据

`desktop_pawapps.rs` 替换固定空列表，提供列表、详情、settings、静态文件
和卸载。读取显式新版本 data directory 下的 `plugins`，不创建空目录。
文件响应使用流式服务并支持 HEAD/Range；卸载仅在测试创建的应用目录验证。

最初测试文件使用了私有 router，先移入内部测试模块；之后修正缺失的
夹具工作目录。这两次测试设施失败不作为功能回归证据。
夹具修好后真实回归 **0/6**：列表仍为空，详情/静态等未注册路由为 404。
实现后初步 **6/6，0.17 秒**。随后增加插件根目录和 manifest 外部链接检查。

新增原浏览器测试不修改 Console：当前 App Center 入口是 `/market`，
`/apps/:appId` 是应用内页。测试初期分别遇到返回 DOM 对象无法序列化、
CSS prefix 错误，以及点击到顶部 Agent 选择器；DOM 诊断证实两个 combobox，
已限定应用搜索栏内的分类框，使用浏览器鼠标事件，不删除分类断言。
专项最终 **1/1，11.41 秒**，覆盖：

- 原应用卡片、版本/描述和真实 SVG 图标请求；
- 名称搜索、无结果、清空搜索及分类筛选/恢复；
- 原刷新按钮触发列表请求；
- 取消确认不发 DELETE，确认只发一次指定应用 DELETE；
- 原页面刷新和整页重载均保留卸载结果，磁盘仅剩另一测试应用。

`pawapps_reference.py` 直接加载原 Python 路由，只替换其目录定位为临时
目录，不导入旧产品初始化。conda qwenpaw 中，六个请求的状态和完整 JSON
与 Rust 顺序结果完全相等：列表、详情、settings、卸载、空列表、缺失详情。
此对照 **1/1** 通过；原静态文件全部响应头、Registry 和运行时执行不在
这六个请求的对照范围。

## 本切片门禁

- [x] 严格 Rust workspace Clippy：**13.59 秒**；修正新增代码的 doc markdown
  和测试夹具参数借用问题后通过，未放宽 lint。
- [x] 新增 JS 语法检查、fmt、`git diff --check` 通过，`console/src` 零 diff。
  原浏览器传输/诊断测试 **16/16，0.461 秒**，无跳过。
- [x] 完整 Rust workspace 回归退出 0，所有非 ignored 用例通过，含新增 7 项
  目录专项；HTTP **36/36，4.41 秒**、Core **126/126，2.99 秒**。
- [x] 原浏览器/参考显式组 **22/22，288.77 秒**，无跳过，含本切片两个新增
  用例；按 `--test-threads=1` 顺序执行，旧备份、检查点、Cron、审批等均通过。
- [x] 原前端 **295 文件、2453/2453，62.52 秒**；生产构建 **38.28 秒**，
  Monaco CSS、压缩资源和初始包体检查通过；接口清单防漂移及其 3 项测试通过。
  重建的 Console **1311 文件**与上一批逐字节一致，树摘要为
  `931aaed10507a21a239153dd25db1c5bcf8dad1887b3a5fc1dfc81e1bce9e688`。
- [x] 最新 source release Core 构建 **52.45 秒**，SHA-256：
  `150924c9a22e7505835615ba32f7e06689be68e6c9e558b32a756d0a502277a9`。
  路径 `qwenpaw-core/target/release/qwenpaw-core`；所有 Cargo 命令均禁用 incremental。
- [x] 使用该 source Core，TypeScript SDK 构建及 **4/4，0.804 秒**，Python SDK
  **5/5，0.604 秒**顺序通过，无跳过；Python 使用 conda qwenpaw 并断言导入路径。
- [x] VS Code 源码编译与 **57/57，0.176 秒**，无跳过；真实 Core 协议用例使用
  上述 source binary，没有激活真实安装扩展或执行 VSIX 内的 Core。
- [ ] 本次源码的各端制品重建、逐个安装/运行验证。
  - [x] 九类独立 QA 输出：Desktop DMG/ZIP、Core、WebUI、TS/Python SDK、两种 VSIX、legacy wheel。
  - [x] 每个包的来源/摘要、资源内容、DMG 只读挂载和签名完整性检查。
  - [x] SDK 离线隔离安装后连接 source Core；VSIX 仅隔离安装，不激活。
  - [x] legacy wheel 隔离安装后的原 CLI/集成回归。
  - [ ] 包内启动、原生窗口与跨平台实机仍受原有边界限制，不用静态检查冒充运行通过。

清单已重新生成：370 调用点、358 静态 Rust 路由、336 注册调用点、335
非占位、1 占位、34 未注册、11 静态未解析表达式。它不是行为完成率；
还包括原 Python 没有的 iframe 路由、动态路径和 URL base 等待分类项。

## 仍未完成

插件注册表、安装/上传、加载/卸载运行时以及动态前端组件执行不能用此目录
实现代替。原页面“打开应用”尚未验收。跨平台实机与并发文件替换防护也
未由本机常规路径测试证明。旧 `qa-runtime-20260909-VVMQCa` 不包含本次功能；
新的 `qa-runtime-20260910-z8rImE` 九类制品已重建并完成静态/隔离安装检查，
详见 [制品验收](qa-pawapps-packages-20260910.md)。包内 Core 安全策略阻塞保持原状。

清理范围尚未获确认，没有删除用户缓存、旧包或未提交改动；没有 commit/push。
