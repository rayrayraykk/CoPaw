# TypeScript SDK 优雅关闭验收

日期：2026-09-10。承接 [stdio 生命周期设计](../architecture/stdio-host-lifecycle.md)
中的 SDK 接入待办。没有改动 Rust 内核、原前端或 Python/Rust SDK，也未执行
分发包 Core、读取真实凭据、清理缓存或旧制品、commit/push。

## 本轮实现

`QwenPaw.close()` 先阻止请求，发送 stdin EOF，持续读取 stdout/stderr，等待
自己启动的子进程退出。重复及 close 回调内重入共享一个 Promise；等待
30 秒超时后才强制终止，再等最多 5 秒，且返回失败，不声称保存成功。
非零退出和信号退出同样报错；同步 `dispose()` 保留立即终止契约。

SDK 不添加 SQLite 或 Agent 执行逻辑。仅验收测试使用 Node 24 的只读 SQLite
检查；SDK 运行时仍声明 Node >=18，本轮没有在所有 Node/OS 版本实际运行。

## 证据

- 最初 EOF 收尾标记测试失败（文件不存在），异常退出测试未得到预期错误。
  修复后通过。最早一次测试编译遗漏必填协议字段，补齐测试参数后才进行
  上述红/绿比较，没有修改协议类型来迁就夹具。
- 真实子进程 EOF 后分别输出 2 MiB stdout 和 stderr，然后延迟写标记。
  SDK 等到标记写入与正常退出；准确收到初始化和关闭前已写入请求，待处理
  请求被拒绝，关闭后不接受新请求，重复和回调重入共用结果。
- 真实子进程的非零退出、同步 dispose 后 close、30 秒不响应 EOF 的超时
  均验证明确失败。超时测试等待真实时间，确认强制终止后仍报错且无收尾
  标记。dispose 场景不证明所有操作系统的全部信号时序。
- 实际 source Core 保持回环模型请求进行中，SDK close 后**先只读 SQLite，
  再重开 Core**：磁盘 Thread 为 idle，Turn 为 interrupted；完整 Turn 与
  关闭前相比只有终态变化，两次重开得到与磁盘相同的完整 Thread/Turn。
  这是针对正常存储的保存检查，不依靠启动恢复把 inProgress 修成 interrupted。
- 最初重开检查虽然通过，但源码审查发现启动恢复可能造成假阳性，因此
  增加了上述重开前磁盘读取，重新执行源码和实际安装包测试。

## 构建和安装后验证

- 源 TypeScript 编译和 **9/9，30.386 秒**，无跳过，包括原协议、真实 Core
  创建 Thread、图片快照与本轮 5 项关闭测试。
- [最终 SDK 包](../../../dist/qa-typescript-close-20260910-Iie6Ho/qwenpaw-sdk-0.2.0.tgz)
  SHA-256：`931e3f7b4b1181710bf0a72f2192d34c6e5797621a9707936692884876a7df66`。
- `check-typescript-package.mjs` 在新空目录打包、离线安装，校验实际 package
  入口和全部 `dist/src` 文件摘要，使用安装后的实现执行原 6 个编译测试文件；
  **9/9，32.415 秒**，失败/跳过/取消均为 0。测试源码未替换断言或业务 API；
  仅把测试的相对 `src` 目录链接到实际安装目录，保留共享协议 fixture 路径。
- [verification.json](../../../dist/qa-typescript-close-20260910-Iie6Ho/verification.json)
  的 pack/install/tests 均 exit 0，`packagedCoreExecuted: false`。连接 source
  release Core SHA-256：
  `e630c45861b71c1b97c0e222d7a5a8cdcf3a1968e3defc44b129e6d61f3c4860`，检查前后未变。
- 较早批次 `qa-typescript-close-20260910-9Ng458` 的 9 项也通过，但尚未加入
  重开前 SQLite 检查；保留其日志，以最终 `Iie6Ho` 为加强后验收依据。
- 原前端源码零 diff；格式空白检查和新发布检查脚本语法检查通过。Shell
  默认 PATH 没有 Node，语法检查改用固定 Node 24.18.1 路径后成功。
- 安装后 SDK 验收结束后，VS Code 再次编译与 **57/57，0.185 秒**通过，无
  跳过；仍是源码客户端检查，不是 VS Code 原生激活。所有本轮测试与构建
  进程最终均已结束，没有启动或重试任何分发 Core。

## 未完成项

- Python/Rust SDK 各自的 EOF、持续读取、线程/worker 等待与异常退出实现。
- 默认 CLI/SDK Workspace 初始化、后台任务单实例归属、凭据优先级。
- Core 最终 upsert 失败目前只 warning，退出成功不等于所有写入成功确认；
  仍需要故障注入与错误传播实现。
- 九类整套分发重新构建、包内运行、原生 Desktop/VS Code 激活与跨平台验收。
  旧整套 DMG 批次 `vGGX8Z` 不包含 stdio 收尾和本轮 SDK 改动。

上一轮 Rust 697、显式浏览器/参考 26、前端 2453 的基线见
[服务端验收](stdio-host-lifecycle-acceptance.md)；本轮未改这些源码，未重复运行
这些整套测试，不把旧记录描述为本轮重新执行。
