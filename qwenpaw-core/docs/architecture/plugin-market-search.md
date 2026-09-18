# 原插件市场搜索

延续已批准的原功能/原前端等价目标。原 PluginManager 的 Market tab 调用 `GET /api/plugins/market/search`，当前 Rust 缺失。先接通这个独立的真实读取链路，不改变待确认的插件执行路线，不把官方 CDN 目录或安装接口标为完成。

复用既有技能市场的固定 AgentScope Platform 来源和有界 HTTP 请求，不增加用户可控代理 URL，不转发桌面认证/Cookie/凭据。支持原分页、搜索、分类、排序与 featured/trending 参数；空可选字符串不转发，显式 false 不丢失。原 JSON 完整透传，含 success:false；网络、非成功响应、无效或超限 JSON 返回 502，不返回伪空列表。错误正文使用不含 URL/上游正文/凭据的稳定描述，不承诺原 httpx 异常字符串逐字相等。共享现有 15 秒时限、8 MiB 上限及拒绝重定向策略。

参数解析需用原 Python ASGI 对照验证，包括整数/布尔值的正常转换、422 错误结构与多项错误；请求只接受原允许字段，不把未知查询项传到远端。

## Checklist

- [x] 新增真实 loopback HTTP 测试先红后绿；五项普通测试覆盖完整响应、精确参数、无凭据转发、422、上游错误/无效 JSON/超限、真实超时与带 Location 的重定向。
- [x] 原 Python handler 17 组请求参数、完整 JSON 与输入错误对照通过；不把 Python 放入产品运行链路。
- [x] 不改原前端，浏览器验证 Market tab 的加载、搜索、分类/精选/排序、视图、刷新、分页及失败恢复；不点击真实安装或外部链接。重载后按原默认 tab 行为重新打开 Market。
- [x] 普通 Rust 854 通过、37 ignored；相关浏览器/参考 2/2、严格检查与 API inventory 通过，见 [验收记录](../testing/plugin-market-search-acceptance.md)。
- [ ] 官方目录的版本过滤、插件安装/执行/卸载、完整认证、全功能及原生/跨平台验收仍开放。

## 发现的版本身份差异（后续修复）

本步测试时 App Server `lib.rs::version` 给 `GET /api/version` 返回 Cargo Core 版本 `0.2.0`；原产品 `src/qwenpaw/__version__.py` 与桌面制品版本为 `2.2.0b5`。原 `console/src/utils/pluginCompatibility.ts` 按该 HTTP 版本的主版本匹配 `qwenpaw_compat_labels`，因此产品的 `2.x` 插件被判断成不匹配。这不是搜索转发或前端筛选代码的问题。

本步没有修改全局版本接口，浏览器验证也未点击安装确认；因此不声明兼容性提示/安装决策已与原产品等价。后续需区分产品版本、Core/SDK 版本和 App Protocol 版本，用真实产品版本驱动原页面，保持初始化协议和语言 SDK 版本独立，并增加兼容/不兼容标签的原页面红绿测试。

后续 [版本身份修复](product-version-identity.md) 已通过原页面标签、警告、取消和重载测试。这个补充不代表插件后端已兼容，也不代表本步旧制品包含后续源码。

QA 目录：`dist/qa-plugin-market-20260915-3wZuhz`。仅本地协议替身与独立测试数据；不读取真实账号或日常插件目录，本步不覆盖已交付 `CE3oB2`。
