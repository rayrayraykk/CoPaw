# 原前端插件加载与 App Center 打开

2026-09-15，macOS ARM64；实现边界和 checklist 见 [方案](../architecture/frontend-plugin-runtime.md)。QA 日志目录为 `dist/qa-frontend-plugin-20260915-V9i0uL`。本轮未修改原 Console 源码、既有浏览器脚本、用户数据或旧包，也未调用真实插件账号。

## 实际功能

Rust 不再固定返回空的 `/api/frontend_plugin` 列表。新 data_dir 下的插件 manifest 按原目录读取契约发现，原 loader 能获取入口 JS，通过原 `window.QwenPaw` SDK 注册页面；`/api/frontend_plugin/{id}/files/{path}` 提供真实资源。列表保留 `loaded:false`，没有声称 Python 后端插件已由 Rust 加载。

四项普通测试覆盖完整目录响应、重开/disabled/隐藏排除、原类型推断、文件更新、JS/MJS/CSS、缓存、GET/HEAD/Range、缺失资源、跨平台路径拒绝、外部符号链接和 manifest 大小限制。所有写入/删除只针对测试创建的临时目录。

原浏览器场景从原 App Center 卡片用鼠标打开 `Demo`，由实际下载的 JS 注册并渲染有状态 React 组件；点击按钮从 0 变为 1，刷新仍能重新加载，返回列表后再次打开。没有用浏览器注入组件或替换原 loader 来使页面通过；诊断脚本仅读取事件和元素位置。

原 Python 对照从仓库 AST 取出未改函数及真实 PluginManifest 模型，只替换目录定位与未使用的 loader 包环境；通过 ASGI 实际执行目录和文件响应，不初始化 Python 产品或把 Python 放入 Rust 运行链路。比较列表 + 3 种文件的普通 GET、Range GET、If-Modified-Since GET 共 10 个结果，包括完整列表、状态、正文、MIME 和 Cache-Control。原 HEAD 为 405，Rust HEAD 是额外能力，单独测试而不纳入相等声明。

## 保留的失败

- 首次新增测试编译遇到 assert_eq 宏导入歧义；显式导入后，四项真实红测试失败：列表仍空、资源路由缺失。
- 首次实现四项中 3 项通过，缓存测试发现全局 no-store 覆盖插件缓存。仅调整新插件路由的响应头层，主页面和其他 API 的默认缓存策略不变。
- Python 对照最初在 HEAD 的 405 响应取缓存头失败；明确原路由范围后，GET 对照发现 CSS 缺少 charset；修正实现后通过。补充条件 GET 后又复现 304/空正文与原 200/正文的差异，按原 handler 行为修正。
- 浏览器首次和一次复测通过，但后续出现返回后重新打开失败。新增驱动将元素发现/坐标读取分开，且未等待 history 返回与布局稳定，曾读取到已移除元素，后续命中诊断也观察到卡片坐标落在页面容器上。当前驱动等待实际 popstate、元素一帧位置稳定及命中检查，鼠标移入后再测位置，只点击一次；未延长超时、删除重新打开断言或改前端。最终新驱动单项和完整 App Server 专项组均通过；这不证明所有原生/导航时序。
- 严格 Clippy 首次报冗余 raw-string 标记和可简化布尔表达式，按建议等价修正，不放宽 lint。
- 完整 workspace 曾有 846 项通过，但后续 `workspace-final`、`workspace-verified` 两次在 Rust SDK `stdio_shutdown_waits_for_eof_drains_both_pipes_and_closes_clones` 的 5 秒等待超时；后一次没有并行编译，不能把原因定为编译负载。限定到完整测试名的单项对照通过（此前未限定模块名且使用 `--exact` 的一次执行匹配 0 项，不算通过证据）。SDK 代码本轮未改，根因未关闭；最终串行全量通过只提供不同调度条件下的对照，不覆盖或抹去默认并发失败。

## 当前验证状态

- [x] 四项功能单测通过；原 Python 对照和原页面场景分别成功执行。
- [x] 最终 App Server 显式组 `explicit-app-server`：32/32，355.79 秒，退出 0；包含本轮两项新增测试与原有 30 项，实际页面和 Python 参考测试分别记录，不把它们全部称为浏览器测试。
- [x] CLI 原 Debug 页面 `explicit-cli`：1/1，15.37 秒；与 App Server 合计 33 个显式项全部通过，无忽略。
- [x] 原 Console 全量 `frontend-all`：295 个文件、2453 项全部通过，72.91 秒。日志保留 jsdom 未实现提示和错误边界用例的预期报错，不能将其当作真实浏览器覆盖。
- [x] 最终严格 App Server Clippy（全目标/全特性、警告即错误）和 workspace fmt 检查通过；API 清单验证通过。
- [x] API inventory 生成与 3 项 Node 测试通过：370 个调用、359 个路由、337 个已匹配调用、33 个未匹配、11 个动态未解析。不是功能完成率。
- [x] 最终串行完整 Rust `workspace-serial`：846 通过、0 失败、33 ignored，退出 0，命令耗时 212.20 秒；SDK 组 21/21（30.58 秒）。33 个 ignored 已在上述专项分别执行，不计入普通通过数。未改变测试时限或永久串行化默认测试配置。
- [ ] 默认并发完整 Rust 无失败：SDK EOF 超时已复现两次，串行通过不关闭该门禁。下一步优先补全失败时的子进程/管道阶段诊断，缩小并发复现条件，再实现有红绿证据的修复。
- [x] `verification.json` 于 2026-09-14T19:08:54.448Z 核对 2927 个既有源码输入，确认原前端及 Console 脚本未变、上一轮工具校验两文件未变；记录本轮新增源码/测试及 API 清单摘要。九个旧制品与 source release SHA-256 均未改变。校验结果明确 `defaultParallelWorkspacePassed:false`、`goalComplete:false`，不代表全功能验收通过。

本轮不重建九类分发包；`6BiWlu` 仍是上一批开发快照，不含本轮插件加载和上一轮工具列表校验。原生 Desktop、包内 Core、VS Code 激活、跨平台、后端插件运行时/安装上传/市场以及其余原功能门禁保持开放，总 goal 未完成。
