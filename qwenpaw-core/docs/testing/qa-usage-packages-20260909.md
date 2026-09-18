# Workspace 使用量归属版本 QA 制品

日期：2026-09-09；计划 §14.2.24.51。仅本机 macOS arm64 QA，不是生产签名、公证或全平台/全功能验收。

输出目录（产品仓库）：`dist/qa-runtime-20260909-K8SPWI/`。旧包与旧失败日志保留。来源为当前 dirty 工作树，不将 Git HEAD 当作完整源码版本。

## 源码基线

普通 Rust **544/544**，严格 Clippy/fmt/diff，原页面和调度参考整组 **17/17**，最新 release 与 TS **4/4**、Python **5/5**、VS Code **57/57** 和编译通过。详见 [用量归属验收](usage-workspace-ownership-acceptance.md)。Core SHA-256：`dde0c21f48f427e961eaaa5a4b024f66ae10bf2a304acfc9fcb2de518f547361`。这些源码测试不替代安装态。

## 构建记录

- 原 `console/src` 零 diff；production build、Core stage、Tauri App、QA ad-hoc 签名和 ZIP 通过。不使用生产签名私钥，不改变系统防护策略。
- 首次 DMG `hdiutil create -fs HFS+ ... -format UDZO` 返回“资源忙”，原 `desktop-dmg.log` 保留。只读 `hdiutil info` 无遗留挂载，磁盘可用约 58 GiB，目标 DMG 尚不存在。
- 对相同目标单独重试相同参数，命令正常退出并创建 DMG；没有修改签名、隔离属性或防护配置。随后使用构建脚本已有的 `--after-desktop` 入口继续其他包，不重新构建已完成的桌面 App。

## 验收清单

- [x] 九类文件及来源清单生成，SHA256SUMS 全部通过；来源清单包含 2832 个文件，记录 dirty 工作树。
- [x] DMG/ZIP 挂载或解包、签名结构及原前端/Core 内容核对。1311 个 Console 文件逐一相同，资源树摘要仍为 `74ca39444b3b61637102f3ce163bfc9987c056ba4c4386897896f656097de976`。
- [ ] Core tar、TypeScript/Python SDK、两类 VSIX 独立安装与协议测试。
- [ ] WebUI 解包后真实浏览器验收。
- [x] 保留的 legacy Python wheel 独立安装及 CLI/TUI 测试；它不证明 Rust CLI/TUI 等价。
- [ ] 桌面完整原生 GUI、各平台安装态与原产品全部交互；历史安装态 SIGKILL 仍未关闭。

## 首次分发验收结果

`qualification.json` 为 **22/24**，非零退出。首次底层原生启动探测沿用旧脚本的并行执行，因此不能将它描述为所有底层检查均逐项串行。后续已把该探测改为 for/await 顺序执行，探测器 3/3 单测通过；同一批制品的串行复验写入 `sequential-checks/`，不改签名、字节、路径或超时，不覆盖首次记录。

| 制品 | 首次结果与边界 |
| --- | --- |
| DMG、桌面 ZIP | 镜像/解包/签名和资源匹配通过；包内 Core 的版本、SDK 握手、模型 Turn、图片及重启历史通过；首次版本探测约 15.8 秒，尚未验证桌面完整原生窗口 |
| 独立 Core tar | 与源 Core 字节相同，但 PID 39225 在 11.113 秒后 SIGKILL；不是探测超时或探测器发出终止，stdout/stderr 为空 |
| TypeScript SDK tgz | 临时安装后连接 DMG/ZIP/平台 VSIX Core 以及源 Core 的真实协议场景通过；独立 tar 的启动门禁失败，不能算所有搭配通过 |
| Python SDK wheel | 临时安装后连接独立 tar 为 3/5、两个 TransportClosedError；源 Core 对照 5/5（0.978 秒） |
| universal VSIX | 独立安装通过；依赖的 tar 启动失败使该搭配未通过，源 Core 对照客户端 CRUD 通过 |
| darwin-arm64 VSIX | 独立安装、内置 Core 选择、包内客户端 CRUD 以及内置 Core 的模型/图片/恢复通过；不替代 VS Code GUI/扩展宿主全功能 |
| WebUI tar | 解包资源完全一致；稍后同一个 tar Core 路径的服务启动成功，真实 Chrome Models 页面/API 通过。不能据此关闭首次 SIGKILL 或证明所有页面安装态 |
| legacy Python wheel | 隔离安装、版本和 TUI help、855 项单测（17.38 秒）及 36 项 CLI 集成（70.63 秒）通过；保留 1 项 audioop 弃用警告，不证明 Rust CLI/TUI 等价 |

签名后的桌面 Core 摘要为 `b4010c48aeeafacff49374937d9e8cc783ea71ec2d0a14ce25af8ebf8a1b5819`，符合正常 QA 打包的签名参考，不是修改失败 tar 的签名来重试。首次验收正常卸载 DMG；串行复验只读挂回同一个挂载点，结束后再次卸载。

## 同路径串行复验

`sequential-checks/package-smoke.json` 和 `native-execution.json` 全部通过。四处分发 Core（tar/ZIP/DMG/平台 VSIX）及源 Core 控制组分别顺序探测，实际耗时 5/5/12/4/3 毫秒；逐项断言下一项开始时间不早于上一项结束，确认没有启动探测重叠。各分发位置通过真实 SDK 握手、模型 Turn、图片输入、重启历史；universal 和平台 VSIX 的实际打包客户端也通过 Thread CRUD。

随后仍使用同一路径的临时安装 Python SDK 和 tar Core，重跑 **5/5**（0.615 秒）通过。原 3/5 与 SIGKILL 不覆盖，串行成功不改写为首次分发 24/24。后续成功有系统状态已变化的可能，不能推断是并行探测导致首次失败；没有移动/重签失败文件、修改防护配置或向产品增加自动启动重试。

复验结束后 `hdiutil info` 无挂载，磁盘约 57 GiB 可用。源 Core 及制品未因验收脚本的串行改动重建；`source-inputs.json` 是打包当时的快照，后续脚本和文档更新不在该快照内。完整原生 GUI、首次启动可靠性、所有原产品功能及 Windows/Linux 安装态仍不能勾选完成。
