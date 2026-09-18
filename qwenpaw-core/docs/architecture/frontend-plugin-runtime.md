# 原前端插件加载入口

延续用户已批准的 Rust Core / 原前端不改方案。现有 PawApps 目录接口只能展示应用；`/api/frontend_plugin` 固定空数组、文件入口缺失，原 `loadAllPlugins` / `loadPawApp` 因此不能执行入口 JS 并注册应用路由。

## 实现与边界

```text
新安装 data_dir/plugins/<directory>/plugin.json
  → Rust 原格式目录发现（sorted、隐藏/disabled 排除、loaded=false）
  → GET /api/frontend_plugin
  → 原 Console loader 请求 /api/frontend_plugin/{id}/files/{entry}
  → 原 Blob import + window.QwenPaw SDK 注册
  → 原 App Center 打开 /apps/{id}、交互和重载
```

按原 Python 未初始化 loader 的目录读取契约返回所有合法记录，而不是虚构已加载的 Rust 后端。插件类型遵守显式 type 与原 meta 推断顺序；应用应显式声明 type=app。保留 manifest id 与目录名原有语义，不擅自重命名或寻找别的目录。隐藏和 `.disabled` 不发现；读取不创建目录。仅使用新版本显式 data_dir，不读取旧产品目录。

文件服务支持 JS/MJS/CSS MIME、Range 和原缓存规则：有至少 8 位内容标识的文件长期缓存，其余 no-cache。CSS 包含原 charset=utf-8；GET 不根据 If-Modified-Since 返回 304，与原 FileResponse 一致。Rust 额外保留 HEAD 支持，但原 FastAPI GET 路由对 HEAD 返回 405，不能宣称 HEAD 等价。拒绝目录、manifest 或资源逃逸链接，以及所有宿主上的 Windows 分隔符/设备路径；manifest 最多 1 MiB，超限/损坏记录不公布，id/version 必须为非空字符串。文件响应流式读取，不把插件文件整体装入内存。路径校验不是对抗任意本机并发替换的完整保证，该门禁继续保留。

本次不新增 iframe 接口（原后端没有该接口）。不实现或宣称 Python 后端插件可在 Rust 内执行；插件安装/上传、市场、后端 runtime/权限/卸载热清理仍需独立完整实现。此步骤接通实际前端插件加载，不替代总体功能目标。

## Checklist

- [x] 目录发现、完整 JSON、静态文件/缓存/路径安全四项先红后绿；新增原 Python 对照确认并修正 CSS charset 和 GET revalidation 差异。
- [x] 原 App Center 使用 fixture JS 注册应用；实际点击打开、按钮交互、返回与重载，不改 console/src 或既有浏览器脚本。最终新驱动单项及 32 项完整 App Server 专项均通过。
- [x] 直接执行原 Python 目录函数和静态 handler，比较列表及 GET/Range/If-Modified-Since 共 10 个响应的状态、正文、MIME、缓存；不初始化旧产品或访问真实凭据。
- [x] 执行完整 Rust、严格 Clippy/fmt、相关原页面回归，更新 API inventory 与验收记录：串行普通 846、显式 33、原前端 2453 通过；默认并发失败单独保留，见下一项。
- [ ] 默认并发完整 Rust 无失败：SDK EOF 5 秒超时复现两次，串行/单项通过不构成根因修复。
- [ ] 后端插件运行时、安装管理、原生与跨平台完整验收；不能随本切片勾选完成。
