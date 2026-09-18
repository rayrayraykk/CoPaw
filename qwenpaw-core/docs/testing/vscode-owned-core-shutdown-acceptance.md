# VS Code 自有 Core 收尾验收

日期：2026-09-14，macOS ARM64。范围见
[实施方案与 checklist](../architecture/vscode-owned-core-shutdown.md)。

## 改动与结果

扩展的 `OwnedCoreProcess` 在 spawn 后即跟踪子进程 close，EOF 后持续排空
stdout/stderr，等待进程和输出流都关闭。30 秒超时后只终止自有子进程，
再等最多 5 秒；非零/信号退出和两种超时结果均明确失败。
`CoreClient.dispose` 复用关闭 Promise 并立即拒绝新 RPC；启动失败也等待
EOF 清理，但不覆盖原始握手错误。`deactivate` 返回异步关闭结果。

资源 manager 在重启/异常断连重连前等待旧资源，关闭开始后阻止新 factory。
异步释放失败会保留，后续 get/restart/close 不能绕过失败启动替代进程；
用户需调查后重新创建扩展宿主。重入和重复关闭共享结果。

## 顺序执行与保留失败

输出：产品仓库 `dist/qa-vscode-owned-close-20260914-Xuhd1O`。

- `red.log`：原 manager **8 通过 / 2 失败**，新增重启及断连重连回归均发现
  `calls=2`，而旧资源尚未完成释放；期望替代 factory 此时尚未调用。
- `green-initial.log`：修复后 **10/10**。
- `green-expanded.log`：异步资源、延迟子进程、大输出、信号/非零退出和
  模拟时钟超时回归合计 **20/20**。
- `real-core.log`：测试错误使用 `InstanceType` 引用 private constructor，
  TypeScript 编译失败；改为类型导入，没有修改生产构造器可见性。
- `real-core-fixed.log`：真正的 `CoreClient.start` + source release Core，
  正常与最终保存故障两项 **2/2**。VS Code API 是测试替身，不是真正激活。
- `vscode-full.log`：补齐启动失败 EOF 清理/原错误保留和异常断连释放失败后，
  编译及扩展完整 **73/73，0.213 秒**，无跳过/取消/失败。Node 模拟计时器
  测试不代表真的等待了 30 秒或证明 OS 强制退出的行为。

真实 Core 测试在关闭后、重开前只读 SQLite 核对完整 Thread/Turn：正常路径
已保存 interrupted；故障路径保留原 inProgress 且关闭报告退出码 1，重复
关闭返回同一失败。去掉测试故障后才重开验证恢复，不用恢复掩盖关闭时未保存。
所有运行对照使用未改变的 source Core `143799...`，不是包内 Core。

## 两种新 VSIX

只更新本次受影响的两个扩展包，不重打其余七类未改变的制品。
在独立生成目录打包，未删除/改签原有 Core 或覆盖旧 VSIX。

| 制品 | 字节数 | SHA-256 |
| --- | --- | --- |
| 通用 VSIX | 30397 | `171cd65b7ef09b8a0ac5f726c58eb63cecd77e5c329d667415df43aa0e723334` |
| macOS ARM64 VSIX | 15959025 | `8b3714f6b980f1b9f2fb9f2fdf8bf959f4b6b11360add579f92c0c453c614c29` |

两次 VSCE 均退出 0，`packages.json` 已正常写入；随后外层日志包装器也试图
向同名 `packages.json` 写命令状态，被排他创建拒绝（EEXIST，包装器退出 1）。
原清单没有覆盖，没有重新打包。`packages-wrapper-failure.json` 保留该记录；
不能把外层脚本记为通过。后续独立核对两个现有文件大小/哈希并解包安装。

`installation.json` 通过：两个全新 VS Code 用户/扩展目录分别安装，未激活。
每份 **16** 个生产 JavaScript 文件及 README 与当前构建逐字节相同；通用
包不含 Core，ARM64 包内 Core SHA-256 与 `143799...` source release 一致，
manifest 保持 `packageKind=qa`。通过独立测试目录链接安装路径，每份实际
安装代码的 manager/子进程/CoreClient 收尾专项均 **24/24**，无跳过。
安装测试同样显式连接 source Core，不启动包内 Core。

`verification.json` 于 **09:31:49 UTC** 通过：73 项源码结果、构建/安装的
执行顺序、两个制品大小/哈希、八个本次输入摘要、相对上一批恰好五个
已有输入变化均已核对；旧九个制品字节未变化，原前端零变更且没有遗留
source Core 进程。三个新增源码/测试文件另列摘要，没有混入旧批次来源证明。

- [通用 VSIX](../../../dist/qa-vscode-owned-close-20260914-Xuhd1O/qwenpaw-vscode-universal-0.2.0-QA.vsix)
- [macOS ARM64 VSIX](../../../dist/qa-vscode-owned-close-20260914-Xuhd1O/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix)

原 `console/src` 零变更；Rust/三语言 SDK 源码和 source release 未因本步修改。
上一批 `g6i9VJ` 的两个旧 VSIX 不包含本修复，不能继续当作当前扩展版本的
源码证明；其余七类文件保留，本轮未重新执行它们的安装态测试。

未使用真实 key/keychain、未修改日常应用数据、未 commit/push/发布。
原生激活/退出、包内 Core 首次启动、跨平台、共享连接及完整原功能仍开放。
