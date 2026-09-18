# 原技能市场验收

使用原 `console/src` production build、真实 Chrome、临时 Rust Core/SQLite、工作区/技能池，以及四来源的本机 HTTP 模拟服务。AK/SK/STS 均为测试假值；显式屏蔽宿主同名凭据与 Hub 地址配置，不访问真实钥匙串或公有市场。

## 执行

在 `qwenpaw-core` 目录、Node 24+ 与 Chrome 可用且 `console/dist` 已构建时执行：

```sh
cargo test -p qwenpaw-app-server desktop_market --all-features
cargo test -p qwenpaw-app-server original_market_browser \
  --all-features -- --ignored --nocapture
```

浏览器测试默认 ignored，必须显式运行。可通过 `QWENPAW_CHROME` 指定浏览器路径。

## 检查内容

- providers/categories/空搜索完整响应：四来源顺序、中文/英文分类及缺少 Aliyun SK 时不可用。
- QwenPaw/ModelScope 归一化、语言回退、原生分类、页码/总量；结果遵守请求对象中的来源顺序。
- ClawHub 搜索字段优先级与旧实现一致，关键字 overfetch 500 后本地切页；浏览和 Aliyun 分别走两页 cursor/token。
- Aliyun ACS3 对照[官方请求结构和签名文档](https://www.alibabacloud.com/help/en/sdk/product-overview/v3-request-structure-and-signature)的固定向量，实际本机请求检查 action/version/STS、不同 nonce；不把 SK 暴露在响应或鉴权头中。
- 单来源 HTTP/结构错误只进入该来源 errors，不覆盖其他来源结果，也不返回上游原始错误体。
- QwenPaw UUID 平铺 ZIP、owner/name ZIP 与版本提示；ModelScope 指定分支 ZIP；ClawHub detail/version/SKILL.md/引用文件；Aliyun GetSkillContent，均实际导入工作区与技能池并比对内容。
- 完整结果对象、installed_from、enabled、重名失败、缺失引用文件、跨平台危险路径、空云端内容。
- 下载卡在响应头前时取消：确认工作进程已释放而非只改状态，结果为空、磁盘无技能，之后重试成功。
- 持有任务状态锁时释放下载，断言安装等待同一提交边界且尚无 live 目录；释放后排在提交后面的取消返回 completed，磁盘、manifest 和结果一致。该回归先在旧提交顺序下失败，再验证修复。
- 原页面分类按钮、搜索输入及分类自动清空、第二页追加、结果卡、详情抽屉 Save、安装队列 Done、导航到原 Skills 页后技能仍存在。使用实际网络请求断言，不替换前端 API。

## 本机记录与边界

2026-09-09：10 个普通测试通过；原市场页浏览器门禁通过上述交互和分页检查。测试曾因错误假设“搜索保留分类”失败，查阅原 hook 后修正测试；未修改前端交互。另验证 Authorization/STS 头的敏感标记使诊断输出不包含假凭据。

最终回归：工作区 347/347 普通 Rust 测试与 4/4 显式浏览器门禁通过；最新 release Core 的 TypeScript SDK 3/3、Python SDK 4/4 和 VS Code 57/57 客户端测试通过，均未跳过真实 Core 连接。严格 Clippy、格式/diff 和静态调用清单校验通过。

这些检查不代表真实公有市场网络/账号、非市场 Hub 来源（GitHub/skills.sh 等）、Windows/Linux 安装态，或新版 DMG/SDK/VSIX 分发包已经验收。Skill Pool 沿用原同步导入接口，不虚构工作区同款后台取消能力。
