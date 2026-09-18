# OpenRouter 原模型管理验收

使用未修改的 `console/src` production build、真实 Chrome、临时 Rust Core/SQLite 和本机 HTTP 模型目录。凭据只进入内存 fixture；不访问 OpenRouter 外网或真实钥匙串，不产生付费模型调用。

## 执行

Node 24+ 和 Chrome/Chromium 可用，前端已通过 `npm --prefix console run build` 构建。在 `qwenpaw-core` 目录执行：

```sh
cargo test -p qwenpaw-app-server openrouter --all-features
cargo test -p qwenpaw-app-server original_openrouter_browser \
  --all-features -- --ignored --nocapture
```

浏览器用例默认 ignored，必须显式执行；可通过 `QWENPAW_CHROME` 指定浏览器程序。Python/SDK 回归使用 qwenpaw conda 环境，但本用例不依赖 Python Agent 内核。

## 验收行为

- 原 Cloud Providers → OpenRouter → Models → Add Models。
- 从实际模型目录加载系列；搜索 beta 后只取消该系列，保留 alpha。
- 使用原 Image 和 Free Models Only 开关筛选；断言实际 POST 请求完整对象及排除付费视频模型。
- 使用原 Add 按钮写入模型，刷新到新 Document 后重开原管理弹窗，模型仍存在。
- 点击原 Test Multimodal；实际服务读取目录能力元数据而非发送聊天探测，失败不会覆盖已有能力。
- 检查浏览器异常、每个非预期 API 错误、Rust 注册表中的模型及文档来源标记。

普通 HTTP 测试另覆盖扩展发现 key 更新、自定义头覆盖、无秘密写入注册表、Core 重开加载、三页游标、错误与能力保留；价格筛选沿用旧实现的 per-token 比较，输入/输出模态各自是任一匹配，`is_free=false` 表示不限制。

这些测试不代表真实账号 OAuth、OpenRouter 外网可用性、安装态跨平台验收或新的 DMG/SDK/VSIX 分发包已经完成。

## 本机记录

2026-09-09：六个普通 OpenRouter 测试通过；显式 Chrome 门禁通过系列筛选、添加、刷新和元数据探测，原页面业务源码零改动。该门禁与两个 Backup 浏览器门禁一同重跑通过；Rust 工作区共 337 个普通测试通过。三页目录回归先复现共享传输重复追加游标的问题，再验证修复后的完整请求序列。
