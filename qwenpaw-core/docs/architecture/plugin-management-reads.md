# 原插件管理页读取链路

延续用户批准的原前端功能等价方案。App Center 已通过 `/api/frontend_plugin` 获取插件，但原 PluginManager 使用 `/api/plugins`，当前 Rust 缺失该入口导致页面把已安装目录显示为空。

本步共用已实现的目录发现和文件服务，补齐 GET `/api/plugins`、`/{id}/status`、`/{id}/files/{path}`。状态遵守原未加载 runtime 的磁盘语义：存在目录和 manifest 则 `loaded:false, enabled:false`，不虚构版本或运行状态。列表仍按原磁盘 fallback 返回完整元数据和 `enabled:true, loaded:false`；这两个 enabled 字段语义按原接口分别保留。

文件路径、链接逃逸、缓存和 MIME 规则沿用现有 Rust 检查。状态读取不解析 manifest 内容，因此损坏的 manifest 可以表示“存在但未加载”；拒绝外部链接和非法 ID。沿用当前 loopback HTTP / restore admission 约束，不在本步重定义全局认证或冒称原 Bearer 认证链路已完整实现。

## Checklist

- [x] 列表/状态/静态文件先红后绿，重开、损坏 manifest、未找到与路径逃逸均有完整响应断言。
- [x] 原管理页真实显示已安装记录，搜索、卡片/列表切换、刷新、重载通过；前端与既有浏览器脚本不改。
- [x] 普通 Rust 849 通过、35 ignored；相关原 Python/浏览器专项 4/4、严格检查及 API 清单验证通过。未将其余 ignored 计作本轮通过，见 [验收记录](../testing/plugin-management-reads-acceptance.md)。
- [ ] 安装/上传、市场、热加载/卸载、后端插件执行、完整认证及原生/跨平台门禁；不能随读取入口勾选完成。

QA 目录 `dist/qa-plugin-manager-20260915-1T4n8H`。本轮不重建或覆盖已交付的 `VQYCTN` 九类开发快照，不读取日常插件目录或真实凭据。
