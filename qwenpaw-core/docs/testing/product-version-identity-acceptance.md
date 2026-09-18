# 产品版本身份验收

后续交付：本页修复已纳入 [xic9ei 九类开发包](qa-product-version-packages-20260915.md)。下文保留源码修复阶段自身的实际测试和未重建边界。

日期：2026-09-15（Asia/Shanghai）。对应 [设计与 checklist](../architecture/product-version-identity.md) 和主计划 §14.2.24.68。

## 实现边界

App Server 的 HTTP `/api/version.version` 使用 Rust 内部产品版本 `2.2.0b5`；SDK 初始化仍返回 Core `0.2.0`，App Protocol 仍为 `3`。不新增运行时配置、不执行 Python、不改原 Console。显式防漂移测试仅读取原 `src/qwenpaw/__version__.py` 文本；独立 Core 默认测试不要求父仓库。

原插件页面以产品主版本匹配兼容标签。新的 headless Chrome 测试验证 `2.x` 绿色、`1.x` 橙色，真实鼠标悬停后点击不兼容插件的 Install，确认警告包含 `2.2.0b5`，取消后无安装请求，重载后重新打开 Market 仍显示正确标签。没有点击兼容插件安装或“Install anyway”，不声称后端插件兼容。

## 实际执行结果

| 检查 | 结果 |
| --- | --- |
| HTTP/SDK 完整响应、源版本对照、原页面专项 | 3/3，4.91 秒 |
| 既有市场浏览器与原 Python 17 组请求/完整响应对照 | 2/2，5.18 秒 |
| `cargo test --offline --locked --workspace` | 855 通过、0 失败、39 ignored；命令 134.26 秒 |
| 原 Console HTTP 网络契约 | 包含在 workspace 中的 36/36，含版本/静态页面/关闭认证测试 |
| Rust SDK 连接真实测试 Core | 包含在 workspace 中的 3/3 |
| App Server Clippy 全 targets/features，`-D warnings` | 通过 |
| workspace fmt、新浏览器脚本语法检查 | 通过 |
| 来源、原前端、既有脚本、旧制品核验 | 通过 |

39 个 ignored 不等于全部重跑；本轮显式执行其中 4 个（版本来源/浏览器、市场浏览器/Python）。原 Console 2453 单测及此前其余显式场景未在本轮重跑，之前结果见 [完整前端回归](full-ui-regression-20260915.md)。

先红后绿的证据保留：`identity-red` 三项均因错误版本失败；修复后 `identity-green` 和 `identity-scroll` 的 HTTP/SDK 与源对照通过，但新驱动未先悬停卡片，按钮命中检查超时。DOM 命中诊断及原 CSS 确认 `.cardActions` 在悬停/聚焦前禁用鼠标事件。仅修正新驱动的真实悬停操作，`identity-hover` 三项全部通过；未修改原 UI、旧驱动或放宽标签断言。

## 可追溯性与未完成项

本地证据目录：`dist/qa-product-version-20260915-g6fkfR`，含命令日志/终态、`verify.mjs`、`verification.json`。最终复核时间为 `2026-09-14T20:54:03.875Z`。

对比 `CE3oB2` 的 2931 条来源，已有文件差异仅此前市场入口/测试，以及本轮版本入口/HTTP 契约；新增版本测试与浏览器驱动分别记录哈希。44 个既有额外脚本保持一致；`console/src` 无改动；此前市场实现、参考脚本与 inventory 均保持哈希一致。九类旧包以及 source release `ab663c72…` 未改变。本轮没有重建包：旧包不包含此前市场搜索和本轮产品版本修复。

安装/上传/卸载、官方目录与后端插件运行时、完整原功能、原生 Desktop/VS Code 激活、包内 Core 与跨平台运行仍未验收。历史 SDK EOF 偶发问题没有因本轮通过而关闭。没有读取系统凭据、真实账号或日常数据，没有 commit/push。总 goal 继续开放。
