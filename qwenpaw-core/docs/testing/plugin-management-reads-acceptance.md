# 原插件管理页读取验收

2026-09-15，macOS ARM64。方案与 checklist 见 [读取链路](../architecture/plugin-management-reads.md)。日志、命令退出信息及最终哈希报告保存在 `dist/qa-plugin-manager-20260915-1T4n8H`。

## 本轮实现

原 PluginManager 使用 `GET /api/plugins`，此前 Rust 缺少路由，页面将失败请求转成空列表。本轮共用已实现的磁盘发现与资源服务，新增管理列表、`/{id}/status` 和 `/{id}/files/{path}`。不启动 Python、不读取日常插件目录、不访问真实凭据。

列表与原磁盘 fallback 一致，保留完整元数据及 `enabled:true, loaded:false`。状态查询按原未加载语义返回 `enabled:false, loaded:false`，不虚构版本；损坏但存在的 manifest 仍可查询未加载状态。路径检查拒绝非法 ID、越界文件、外部目录或 manifest 链接。全局认证未在本步补齐，不将 loopback admission 视为原 Bearer 认证的等价实现。

两项新增普通测试断言完整 JSON/文件响应，覆盖空目录无写入、重开、disabled、损坏 manifest、缺失记录、Range、MIME/缓存与路径逃逸。原管理页使用实际入口 `/market?tab=plugins`，通过鼠标与键盘检查已安装卡片、版本和未加载状态、名称筛选、无匹配结果、清空筛选、列表/卡片切换、刷新请求和页面重载；没有替换组件或修改原 Console。

原 Python 参考测试从仓库原 handler 提取函数，在 Conda qwenpaw 中以隔离目录和真实 ASGI 执行列表、三种资源的普通/Range/条件 GET、正常/缺失/损坏记录状态，共 13 个完整结果，与 Rust 相等。它不是生产 Python 运行时，也不覆盖安装或热加载。

## 保留的失败与修正

- `reads-red`：两个新测试失败，管理路由实际缺失；共享读取实现后 `reads-green` 两项通过。
- `browser`：新驱动误用不存在的页面入口 `/plugins`，仅显示外壳。查明原页面后改为 `/market?tab=plugins`，未改产品路由。
- `explicit-manager`：新驱动将 DOM 对象作为 JSON 返回，遇到对象引用链过长。状态等待改为布尔值，坐标探测仍返回坐标。
- `explicit-state`：新驱动清空搜索时没有真正选中全部文字，仅删除末字符。补充明确的键盘 selectAll 指令、虚拟键码及输入值断言；保留原有功能断言和超时。

这些驱动错误不记为产品修复。最终相关四项同时通过，没有修改既有浏览器脚本或放宽断言。

## 最终验证

- [x] `workspace`：默认并发 `cargo test --offline --locked --workspace`，849 通过、0 失败、35 ignored，命令耗时 109.20 秒，退出 0。
- [x] `explicit-selection`：原管理页及其 Python 对照、既有 App Center 页面及其 Python 对照，共 4/4，执行 5.27 秒，退出 0。原公开接口另对照 10 个响应。其余 31 个显式项本轮未重跑，不算本轮通过。
- [x] 严格 App Server Clippy 全目标/全特性、警告即错误；workspace fmt；API inventory 检查均退出 0。Python 新脚本无超过 79 字符的行。
- [x] API 清单：370 调用、362 路由、339 已匹配调用、31 未匹配、11 动态未解析；不是功能完成率。
- [x] `final-verification.json`：核对上一批 2930 条源码输入，仅两个既有插件模块/测试入口改变；记录三个新增源码/测试文件及生成清单哈希。原 Console 源码和既有浏览器脚本未变。
- [x] `VQYCTN` 九类制品字节数与 SHA-256 全部不变；source release 仍为 `70bd954627d5413ab9d6390995e552ff64d763588fc286b5a04fd425b4e5814e`。没有重建、覆盖、commit 或 push。

本轮新读取接口尚未进入现有 DMG 等开发快照。安装、上传、市场、后端插件执行、热加载/卸载、完整认证、原生 Desktop、包内 Core 执行、VS Code 激活及跨平台验收仍未完成。历史 SDK EOF 偶发失败没有在本轮复现，但根因仍未关闭。总 goal 保持未完成。
