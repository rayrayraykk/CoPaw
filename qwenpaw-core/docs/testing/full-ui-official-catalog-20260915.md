# NvQ0h0 原前端全组回归

官方目录九包构建后的补充回归全部通过：App Server 41 项、CLI 1 项、原 Console 295 文件/2453 项、共享 DevTools 与关闭诊断 16 项。原前端、产品源码、既有浏览器驱动、超时及九包均未改动。这证明当前覆盖场景通过，不代表全部原功能或原生客户端验收完成。

## Checklist

- [x] 对照 NvQ0h0 清单，核对 2940 个构建来源、50 个额外脚本、全部 42 个显式项及九包哈希。
- [x] 优化版 App Server 全组 41 passed、0 failed、0 ignored，366.08 秒，退出 0。
- [x] App Server 结束后串行执行 CLI Debug 原页面：1 passed、0 failed、0 ignored，15.27 秒，退出 0。
- [x] 原 Console 全量 295 文件/2453 项通过，117.58 秒，退出 0；仅使用既定 `--maxWorkers=4` 限制并发。
- [x] 共享 DevTools/关闭诊断单测 16 passed、0 failed、0 cancelled、0 skipped，退出 0。
- [x] 测试后重新核对全部来源、脚本、九包及 source release Core，原 `console/src` Git 状态无改动。
- [ ] 完整原功能、插件后端执行、原生客户端与跨平台仍开放；本轮不关闭历史浏览器退出、SDK EOF 或焦点问题的根因调查。

## 范围与证据

证据目录：`dist/qa-full-ui-catalog-20260915-oZAQct/`；[最终校验](../../../dist/qa-full-ui-catalog-20260915-oZAQct/verification.json) 于 `2026-09-14T22:33:54.315Z` 通过。每个命令均有独立日志、起止时间、退出码和 signal，显式项逐一列于结果文件。42 项分为 **31 个浏览器场景、10 个原 Python 对照、1 个产品源码版本文本检查**，不能统称 42 个浏览器测试。

使用 Conda `qwenpaw`、Node 24、离线锁定 Rust 依赖、隔离 headless Chrome 与本地协议替身。App Server 和 CLI 浏览器组串行运行；测试凭据存储不访问真实账号或系统 Keychain。Console 的 jsdom 限制提示和预期错误边界堆栈保留在完整日志中。

首次汇总校验退出 1：新增 DevTools 统计解析误以为 Node 输出 TAP 的 `# tests`，实际为 spec reporter 的 `ℹ tests`。只修正本阶段 QA 汇总器以识别两种行前缀，仍严格要求 16 通过、零失败/取消/跳过；未修改测试或重新执行成功子集。首次校验的日志及终态保留，修正后的完整来源/结果校验退出 0，日志名为 `verification-reporter-format`。

source release Core SHA-256：`31459d3ba9123ecf3ea1098231434a1f04a6ace8e7ce53d7a1ec28f5ed9ca833`。本阶段无产品代码变更，无需重建包；[九包下载和此前安装后测试](qa-official-catalog-packages-20260915.md) 保持有效。此前普通 Rust workspace 871 项通过属于构建前证据，本轮没有重复执行，不混写成新结果。

没有执行包内 Core、启动原生 Desktop、激活 VS Code 扩展或执行 Windows/Linux/macOS x64 二进制；没有 commit/push、发布或安全策略绕过。插件列表、目录及前端读取通过仍不等于 Python 插件后端可以运行，路线边界见 [插件运行时决策](../architecture/plugin-runtime-decision.md)。总 goal 保持未完成。
