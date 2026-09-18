# 产品、Core 与协议的版本身份

修复已发现的原前端兼容判断差异，不改变已批准架构或引入 Python 运行时。

| 使用者 | 版本来源/当前值 |
| --- | --- |
| 原 Console 的 HTTP `/api/version.version`、插件兼容标签 | 产品版本 `2.2.0b5` |
| Rust CLI、Cargo crates、SDK 初始化 serverInfo.version | Core/SDK 版本 `0.2.0`，保持原状 |
| App Protocol 握手和 HTTP protocolVersion | 协议版本 `3`，保持原状 |
| Tauri 原打包脚本 | 同产品版本的 SemVer 表示 `2.2.0-beta.5`，不修改签名或更新配置 |

Rust App Server 内保存明确的产品版本常量，使独立 Core 构建/运行不依赖 Python 文件、解释器或父仓库。产品发版时须与原产品版本同步；显式源文件校验会比较 `src/qwenpaw/__version__.py`，仅测试时读取文本，不 import Python。该校验需要原仓库，单独标记显式执行，不能静默跳过。

不增加运行时环境变量覆盖、不改前端标签、不把 Core 版本改成产品版本，也不改变 HTTP 的既有 backend/protocolVersion 字段。原 UI 对不兼容插件仍需确认；兼容项应显示正确标签。测试不安装插件，不把标签校验当作后端插件兼容证明。

## Checklist

- [x] 新 HTTP 完整响应与 SDK 初始化身份测试先红后绿。
- [x] 原页面 `2.x` 兼容标签、`1.x` 不兼容标签/确认框/取消及重载正确，原前端不改；没有安装请求。
- [x] 显式产品版本来源校验通过；Core/协议版本仍独立。
- [x] 普通 Rust 855 通过、0 失败、39 ignored；新专项 3/3、相关浏览器/参考 2/2、严格检查与原网络契约通过，见 [验收记录](../testing/product-version-identity-acceptance.md)。
- [ ] 原插件运行时、官方目录、安装及全功能/原生/跨平台验收仍开放。

QA：`dist/qa-product-version-20260915-g6fkfR`。本步不覆盖已交付 `CE3oB2`，不读取真实凭据或日常数据，不启动原生窗口/包内 Core。
