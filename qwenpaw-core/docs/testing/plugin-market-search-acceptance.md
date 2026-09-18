# 插件市场搜索验收

2026-09-15，macOS ARM64。实现范围见 [方案与 checklist](../architecture/plugin-market-search.md)。QA 目录为 `dist/qa-plugin-market-20260915-3wZuhz`，现有 `CE3oB2` 九类开发包本轮不重建。

## 实际实现

新增 `GET /api/plugins/market/search`，复用 Rust 技能市场固定 Platform 来源与有界请求工具。只转发原 page_number/page_size/search/category/sort_by/is_featured/is_trending；保持空可选字段省略、显式 false、整数精度和原布尔别名。未知参数、Authorization、Cookie、Agent 与 API key 请求头不会被转发。

远端 JSON 完整透传，包括 success:false 与扩展字段。502 明确区分非成功状态、无效/超限响应和网络错误；不输出上游正文、URL 或凭据，不以成功空列表掩盖失败。网络错误文本做了有意的安全归一化，不声称与原 httpx 异常字符串逐字相等。

五项普通测试验证真实 HTTP 完整查询/响应、422 多字段错误且不请求远端、503/302/非法 JSON/超过 8 MiB、原 15 秒真实请求超时，以及带 Location 的 307 不访问目标地址。后者直接验证搜索所用共享传输；不是假装只有无 Location 的 302 就证明拒绝跟随跳转。

原 Python 参考在 Conda qwenpaw 内提取未修改的 search handler，通过实际 FastAPI 参数解析与隔离 HTTPX transport 执行 17 组查询。比较完整 status/body/query，包括默认值、空字段、重复字段、正负号/小数零/下划线、超过 i64 的整数、非法数字和全部布尔错误；不启动旧产品或接触外部服务。

真实浏览器使用原 Console 构建，打开 Plugin Market，完成自动下一页、搜索、分类、Featured/Trending 互斥、排序、卡片/列表切换、刷新、502 原提示与恢复、页面重载后重新打开 Market。上游收到相应实际参数，UI 未被 mock 替换；未点击安装、发布或外链。

## 保留的失败

- `reads-red` 两项失败：原路由实际为 404，不能得到目录或正确错误；接入实现后 `reads-green` 两项通过。
- 首次 `explicit` 中 Python 对照通过、浏览器失败。新驱动用了默认 `.ant-select-item-option-content`，但原 `App.tsx` 配置 `prefixCls="qwenpaw"`；下拉内容已真实展开而选择器没有找到元素。只修正新脚本两处为原前缀，未改前端、既有驱动、断言或超时。
- 修正后 `explicit-selectors` 两项同时通过，4.92 秒。没有用单项成功覆盖失败日志。

## 最终检查

- [x] `workspace`：默认并发完整 Rust 854 通过、0 失败、37 ignored，退出 0；命令耗时 130.34 秒。新增五项普通测试均实际执行，包括超时与重定向。
- [x] 新浏览器与原 Python 对照 2/2 通过。其余 35 项显式测试及原 Console 2453 项本轮未重跑，不计作本轮通过。
- [x] App Server 严格 Clippy 全目标/全特性、警告即错误；workspace fmt；API inventory 三项 Node 测试与清单防漂移均通过。Python 新脚本每行不超过 79 字符。
- [x] 清单为 370 调用、363 路由、340 已匹配调用、30 未匹配、11 动态未解析；不是功能完成率。
- [x] [verification.json](../../../dist/qa-plugin-market-20260915-3wZuhz/verification.json) 于 2026-09-14T20:37:49.000Z 通过：2931 条旧来源仅市场模块和测试入口两项改变；记录四个新增源码/测试文件与生成清单哈希，既有 42 个脚本全部未变。
- [x] 原 Console 未改，九类旧制品及 source release `ab663c72…` 哈希保持不变；未 commit/push、访问真实账号、执行包内 Core 或启动原生客户端。

## 未完成项

新市场搜索代码尚未进入现有 DMG。官方 CDN 目录、插件安装/上传/执行/卸载及完整认证仍待完成；插件执行路线仍待用户确认。

另发现原页面的版本兼容判断受到现有 HTTP `/api/version` 的影响：它返回 Core `0.2.0`，而产品为 `2.2.0b5`，原 `2.x` 插件标签因而误判不匹配。本轮只完成浏览/查询链路，未将兼容提示或安装决策算作等价；后续应区分产品版本、Core/SDK 版本和协议版本，并用原 UI 红绿测试修复。

历史 SDK EOF、剩余原功能、原生 Desktop、VS Code 激活、包内 Core 和跨平台验收仍开放，总 goal 未完成。
