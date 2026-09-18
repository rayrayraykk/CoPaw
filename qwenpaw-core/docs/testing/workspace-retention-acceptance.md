# Workspace 保留安全与本机 QA 制品

日期：2026-09-09；计划 §14.2.24.44。全功能目标仍未完成。

## 已复现并修复

原 `routers/agents.py::delete_agent` 只移除注册，不删除 Workspace。Rust 创建的 `auto_workspace` 原先表示自动选址，却被直接作为失败时递归清理的依据：先删除一个 Agent，再创建相同 ID，凭据保存失败会把原目录整体删除；自动路径为已有符号链接时，清理甚至发生在其 canonical 目标目录。

新增实际 HTTP 失败注入先得到 **2 通过、2 失败**，两项失败均为原文件已消失。修复把清理所有权限定为本次独占 `create_dir` 成功；已有目录或链接不授予清理权。没有通过禁止复用 ID/目录或取消所有新目录清理来规避问题，也不使用“先 exists 再 mkdir”判定目录是否属于本次操作。

最终定向 **4/4**（0.11 秒）：新自动目录失败清理；删除后保留的自动目录、原配置与用户文件不被该凭据失败路径删除；自定义目录保留；已有自动链接及其目标数据保留。链接测试仅在 Unix 运行，Windows 代码路径可编译不等于 Windows 实机验收。此处不声称已提供整个创建流程跨文件/凭据/索引的事务，也未声称防御同权限进程的所有文件系统替换竞态。

## 源码验证

- 工作区普通测试 **492/492**，App Server **272/272**（18.74 秒）；最终显式整组 **15/15**（245.08 秒），包含 14 项原页面和原 APScheduler 16 案例差分。
- 严格 App Server Clippy 通过（6.84 秒）；fmt/diff 通过，`console/src` 零 diff。
- Core release 构建通过（约 70 秒），SHA-256：`32e4175cadc808652aeb731d0853925675a278bdc218f0e7f9437e0e0443010f`。
- 显式使用该二进制：TS SDK **4/4**、conda Python SDK **5/5**（0.927 秒）、VS Code **57/57** 与编译通过。
- Console API inventory **3/3** 和快照通过：370 项调用、38 项未注册 Rust 路由。没有把调用清单当作完整业务验收。

## 原数据归属的后续边界

原聊天由 `workspace/service_factories.py::create_chat_service` 读取所选 Workspace 的 chats.json；任务同样从所选 Workspace 创建管理器。但 `app/inbox_store.py` 使用全局 inbox_events.json，保留当时的 Agent ID；`agent_stats` 路由则按 Workspace 选择统计范围。后续不能只替换 Cron owner，也不能无差别重写所有旧记录的 Agent ID。

统一 Workspace 数据身份、当前注册身份和运行实例的绑定仍需实现。同 ID 换目录、同目录换 ID、返回原目录、重启和范围备份必须一起验证。当前非默认 Cron HTTP 501 和后台门禁仍保留；修复清理风险不代表这些功能已完成。

## 新制品验证

本轮新输出目录为 `qwenpaw/dist/qa-runtime-20260909-3AxXvy`，旧包保留。WebUI 生产构建、Core staging、桌面 app 构建/临时签名和 ZIP 成功；第一次 hdiutil create 返回“资源忙”且没有生成目标 DMG。只读检查没有挂载镜像或残留镜像进程，相同源目录的一次显式重试成功（退出码 0），随后用构建脚本的 `--after-desktop` 接续其余产物，没有重做或覆盖前面日志。资源忙的根因尚未定位，不能伪称首次全流程成功。

九类包已生成，但**这一候选没有通过完整验收**：`qualification.json` 记录 24 个检查阶段，22 个退出 0、2 个失败（python-sdk-packaged-core 与 package-smoke）。构建指纹为 `d8a09f6f9d3b4b1651e61741e47082e1b3a924010c083dc951d3ca9963e5518d`，覆盖 2814 个源码输入，明确记录 dirty worktree，未提交/推送。

| 产物 | 本轮实际证据 |
| --- | --- |
| macOS arm64 QA DMG | 哈希、镜像验证/只读挂载、签名完整性通过；从挂载镜像直接运行内嵌 Core，SDK 握手、图像输入、模型往返与重开历史通过；不是原生窗口验收 |
| 桌面 ZIP | 解包签名与内嵌 Core 同样通过上述 SDK 场景 |
| 独立 Core tar.gz | 哈希和解包通过；首次 Python SDK 两项错误，独立 --version 探针遭 SIGKILL，**未通过启动验收** |
| WebUI tar.gz | 解包资源一致，真实包内 Core 启动后的原 Models 页检查通过；后续启动成功不关闭独立 Core 的先前失败 |
| TypeScript SDK tgz | 在新目录安装，使用桌面/平台包内 Core 的握手、模型/图像与重开通过；未把源码控制组代替失败的 Core tar 场景 |
| Python SDK wheel | 独立安装；源码 Core 控制组 5/5（0.970 秒），独立 Core 包场景 3/5、2 errors（23.646 秒） |
| Universal VSIX | 使用隔离 VS Code 用户/扩展目录安装成功；源码 Core 对照的包内 client 通过，依赖失败 Core tar 的真实组合未通过 |
| darwin-arm64 VSIX | 隔离安装、内嵌 Core 首次 --version、模型/图像/重开和包内 client Thread CRUD 通过 |
| 保留的 Python 产品 wheel | 隔离安装/import/version/TUI help 通过；CLI 单测 855/855（16.39 秒）、集成 36/36（73.16 秒）；不能代证 Rust CLI/TUI 全功能已经实现 |

Console 生产资源共 1311 个文件，在桌面 ZIP、DMG、WebUI 和旧 Python wheel 中逐文件完全一致，树哈希 `74ca39444b3b61637102f3ce163bfc9987c056ba4c4386897896f656097de976`。DMG SHA-256 为 `64bec88dd504995a02cdabdfb6ea00fd80071b277a0a13fd2e0327ff9b0e2a39`；桌面 Core 显式临时签名后的 SHA-256 为 `2a1771d51876a0b8974ea729032f518572b5b5f1066291db61539b10658b39b1`，按同样签名处理的源码副本逐字节匹配。详细产物哈希见同目录 SHA256SUMS / build-manifest.json，不能仅因清单存在就视为验收通过。

### 独立 Core 首次启动调查

失败不是 SDK 超时的推测：独立 native-execution.json 中 Core tar 的 --version 进程约 11.12 秒后收到 SIGKILL，`timedOut=false`、`terminatedByProbe=false`、无 stdout/stderr；源码、桌面 ZIP/DMG 和平台 VSIX 的同轮探针通过。静态 codesign 校验仍有效。系统日志有该路径 AMFI 签名信息，但源码/通过的路径也有类似记录，因此不能仅凭“no CMS blob”认定根因。Apple 的 [Code Signing Tasks](https://developer.apple.com/library/archive/documentation/Security/Conceptual/CodeSigningGuide/Procedures/Procedures.html) 区分签名检查与系统策略评估；本轮没有以静态验证替代实际执行。

在新 `/tmp/qwenpaw-signing-probe-uiLJI5` 中构建 raw 与显式临时签名的 tar 对照，各用独立解包目录做首次 SDK 和 --version。四项全部失败（约 11.68–12.95 秒）；两项随后独立重查仍遭 SIGKILL。仓库内新的 `dist/signing-probe-NrcmHP` 两个副本也失败，不能归因于只限制 /tmp。从本机已编译源码创建不携带扩展属性的全新副本，字节哈希与源码完全相同，仍约 11.90 秒后遭 SIGKILL，直接 shell 执行亦退出 137。该实验没有修改既有包或清除其属性，也没有关闭系统安全策略。显式临时签名和复制扩展属性两种简单解释均不足以给出修复，因此**没有据此改打包脚本或重签候选来伪造通过**。

日志中也可见系统策略评估 allowed/cache，没有捕获明确的 invalid-page/crash 报告；当时内存空闲比例为 76%。具体终止原因仍待进一步取证，不宣称系统策略拒绝、内存不足或 SDK 本身就是根因。调查需要继续关联具体子进程 PID/终止事件；不能仅在原路径重试成功后关闭缺陷。

后续已新增按 PID 关联的探针：失败的实际 tar 和字节副本有阿里终端防护非白名单标记，同名同哈希的源码控制组没有该标记并正常启动。尚未得到直接处置记录，需要终端防护侧确认；详见 [策略核查材料](core-startup-policy-investigation.md)。原候选的失败状态保持，不据此修改系统防护或宣称完成修复。

所有构建、验收及一次性对照进程已结束，DMG 已卸载，原失败记录和对照目录保留。macOS 原生桌面窗口、生产签名/公证、Windows/Linux 实机及完整各端功能仍不由这些检查代证。桌面启动仍使用应用自己的数据目录，建议仅在独立测试账户体验该候选，不覆盖日常安装。
