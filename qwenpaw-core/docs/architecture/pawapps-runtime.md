# PawApps：原页面契约与 Rust 实现

本切片执行已批准的 `rust-core-refactor-plan.md` §14.2.24，不修改
`console/src`，不读取或迁移 Python 用户目录，不使用真实凭据。

## 证据与范围

原实现为 `src/qwenpaw/app/routers/pawapps.py`，当前原页面为
`console/src/pages/AppCenter/index.tsx`，入口 `/market`；`/apps/:appId` 是内页。
仓库还保留 `Settings/PawApps/index.tsx`，但当前 builtinRoutes 没有注册它。
本切片开始前 Rust 的列表是固定空数组。
原后端优先使用 PluginRegistry，未加载注册表时扫描工作目录下的 `plugins`。
Rust 本切片使用新版本显式 data directory 下的 `plugins`，共享给各 Agent，
不随 selected project 改变；读取不创建目录。

实现目录扫描、详情、settings、静态资源和目录卸载。返回原 manifest 的
`meta.pawapp` 字段与原默认值，保留 `description_i18n` 和 `meta.settings`。
忽略普通插件、缺失或无效 manifest。卸载仅处理请求指名的直接子目录，
真实删除测试只操作 TempDir 中由测试创建的应用，不清理用户文件。
静态文件保留 MIME、HEAD 和 Range，不用整个文件读入内存的响应替代流式下载。
拒绝跨目录、跨平台路径前缀和逃逸符号链接，不允许插件根目录指向外部目录。

原 Python 路由**没有** `/pawapps/{id}/iframe`，虽然前端 API 模块有该方法；
原列表也不返回 `home_page`，保留的 Settings 页面因此显示“无前端 UI”。
当前 App Center 使用 `entry_page` 与动态路由注册表，而不是 iframe。
本切片不擅自将 `entry_page` 转成 `home_page` 或创造 iframe URL。
注册表/插件执行、安装/上传、动态前端插件加载仍是后续真实功能，不能以
目录扫描取代它们，也不能将整个 PawApps/Plugins 门禁勾选完成。

## Checklist

- [x] 核对原后端、原页面、现有空实现和数据目录边界。
- [x] 先运行目录生命周期、完整 JSON、静态下载和安全边界失败回归：修正夹具后 6/6 失败，原列表为空、缺失路由 404。
- [x] 实现 Rust 目录接口，去除空列表占位。
- [x] 验证刷新/重新构造服务后发现、删除后缺失及旁路文件保持不变：初步 6/6 通过。
- [x] Rust 专项、工作区回归、fmt、严格 Clippy。
- [x] 用原 App Center 验证列表、图标、搜索/分类、刷新、取消/确认卸载及重载。
- [x] 直接运行原 Python 路由，逐项比较目录生命周期的真实 HTTP 状态和完整 JSON。
- [x] 更新真实路由 inventory，确认 `console/src` 零 diff。
- [ ] 最新各端完整运行验收；本切片九类 QA 已重建并完成分层检查，见 [制品记录](../testing/qa-pawapps-packages-20260910.md)，原生/跨平台门禁仍未完成。

本记录不关闭原生安全策略、外部凭据、跨平台实机及默认 CLI/SDK 宿主缺口。
具体命令结果、失败与最新构建范围见 [验收记录](../testing/pawapps-directory-acceptance.md)。
静态路径与符号链接检查不等于已证明对抗本机进程并发替换目录的安全性；
目录检查后到实际打开文件之间的竞态仍需独立硬化验收。
