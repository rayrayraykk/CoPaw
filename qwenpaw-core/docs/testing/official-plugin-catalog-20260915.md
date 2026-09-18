# 官方插件目录阶段验收 — 2026-09-15

实现范围为 `GET /api/plugins/catalog`，保持原 Console 源码不变。生产使用纯 Rust；Python 仅在 Conda `qwenpaw` 测试环境加载原定义作为对照，不启动原产品、不接触日常数据。方案及未完成边界见 [官方目录](../architecture/official-plugin-catalog.md)。

## 结果

证据保存在仓库忽略目录 `dist/qa-official-catalog-20260915-paT6e9/`，每条命令包含独立日志和终态 JSON；非零退出记录未被覆盖。

| 检查 | 结果 |
| --- | --- |
| `cargo test --offline --locked --workspace` | 866 passed，0 failed，41 ignored；含 2 个 doc tests。ignored 不算自动验收通过 |
| 官方目录专项 `--include-ignored` | 13/13，含原 Python 对照和原页面浏览器验收；最终整组 30.13 秒 |
| 原 Python 完整响应 | 100 组 installed/catalog 版本组合、12 组兼容约束、1 条缺省/标量输入；同一 113 条目录比较整个响应 |
| 原 Official Plugins 页面 | 安装/升级标签和按钮文案、名称搜索、类别、列表/卡片、刷新、上游失败提示/恢复、重载；没有点击安装/升级/重装 |
| 网络/文件边界 | 主索引错误和缺失 product；非法路径、坏 JSON、gzip header/magic/多 member、压缩前后 8 MiB、真实 30 秒超时、拒绝重定向、不转发 authorization/cookie；缺省/disabled/后端 manifest、超大/非法 manifest、越界 symlink |
| 严格/格式 | workspace all-targets Clippy `-D warnings`，`cargo fmt --all -- --check` 通过 |
| 浏览器公共工具 | 16/16，未修改已有 DevTools/关闭诊断脚本 |
| API 清单 | 370 calls、364 routes、341 registered calls、29 missing calls、11 unresolved；扫描器 3/3。这些数量不是功能完成比例 |

## 失败与修正过程

1. 最初 2 个 HTTP 红测试收到非 JSON 空响应，随后实现读取入口转绿；未将未打印的状态码当作证据。
2. 对尚未进入锁文件的新包执行 `cargo update -p` 失败；离线解析曾降低五个无关包版本。已精确恢复它们，最终从锁文件移除 `pep440_rs 0.7.3`、`unicode-width 0.2.2`、`unscanny 0.1.0` 和一条直接依赖后，其 SHA-256 与阶段前完全相同，没有留下无关升级/降级。
3. 第一轮页面测试 10 passed / 1 failed。诊断确认同名搜索框同时存在于隐藏和可见标签页：脚本首次 `querySelector` 得到隐藏空值，而活动输入框的实际值为 `" new "`，页面已正确筛选。新驱动改为验证可见输入框；原前端未改。
4. 随后的整组 12 passed / 1 failed：刷新后立即断言响应数组存在新条目，没有等待 CDP 响应事件，且状态查询仍可能定位到隐藏刷新按钮。新驱动增加有截止时间的真实响应等待，并限定可见按钮。独立浏览器验收 1/1 后，最终整组 13/13，未以单项重试覆盖失败整组。

## 来源与交付边界

核对基线 `dist/qa-runtime-20260914-xic9ei/` 的 2934 个构建来源，只允许本次两个 Cargo manifests、Cargo.lock、market router/source fixture 的改动；旧 47 个脚本与原前端均未变。新增目录模块/测试和两个对照驱动另外记录哈希。所有 9 个旧制品及 source release Core 哈希保持不变，故旧 DMG 等**尚不包含此项目录实现**。

本轮没有发布、commit/push、真实凭据、原生 Desktop 窗口、VS Code 激活或 packaged Core 执行；没有绕过历史安全限制，也没有新增 Windows/Linux/macOS x64 实机结论。没有重跑整组原前端 2453 测试或全部浏览器套件；本轮结论仅为上述回归与来源核对。

目录不是安装/后端执行。极端版本数值、Unicode 数字、非标准嵌套字段字符串转换和异常结构错误映射仍开放；即使当前全部专项通过，也不宣称任意 Python 输入已完全等价。插件执行路线、其他原功能缺口及历史偶发问题继续沿原计划推进。
