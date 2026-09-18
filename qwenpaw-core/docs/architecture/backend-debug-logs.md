# Backend Debug：真实日志链路

本切片属于已批准的原前端全交互等价计划，不修改 `console/src`。

## 当前缺口与契约

原 `app/routers/console.py` 接口默认返回末尾 200 行，允许 20–1000 行，
最多读取末尾 512 KiB；JSON 包含 path、exists、请求行数 lines、秒级
updated_at、文件总字节数 size、无末尾换行的 content。原日志默认 5 MiB、
3 个备份，支持 `QWENPAW_LOG_MAX_SIZE` 和 `QWENPAW_LOG_MAX_BACKUPS`。
前端每 3 秒刷新，支持手动刷新、排序、级别/文本筛选及复制。

改造前 Rust 接口固定返回空对象形状，CLI tracing 只写 stderr；Tauri 捕获
stderr 到自身日志，但 Core 的 Debug API 不可读取该日志。另有兼容问题：
前端筛选 WARNING，而 tracing 默认级别文本是 WARN。

## 实现边界

日志属于整个安装的 Workspace 宿主，不属于当前选中的 Agent/project。
路径固定为新版本显式 data directory 下的 `qwenpaw.log`，不接受请求传入
任意路径，不扫描/迁移 Python 工作目录或桌面壳的旧日志。App Server 向宿主
提供日志写入句柄；CLI 保留 stderr，并在 Workspace 初始化成功后接入文件。
stdio 的 stdout 仍只有协议 JSON，Desktop 的 ready marker 仍按原协议输出。
初始 Workspace/凭据初始化失败仍通过 stderr 报告，不承诺启动前日志入文件。

文件写入追加并有界轮转；持锁完成一条写入，轮转前释放文件句柄以兼容
Windows。备份为同目录 qwenpaw.log.1 等，0 backups 保留原 Python 的不轮转
语义。读取使用已打开文件的元数据与最多 512 KiB 快照，拒绝符号链接与
非普通文件，不因外部日志路径或查询参数读取宿主其他文件。复用仓库已有
cap-std/cap-fs-ext 依赖提供相对目录文件访问，不自行实现 unsafe 系统调用。

日志格式使用纯文本、WARNING 级别名，保留 stderr 诊断。不能把临时文件
fixtures 能读到内容当成真实 tracing 已接线；必须同时验证写入与原页面。

## Checklist

- [x] 核对 Python 尾部读取、轮转、前端轮询/过滤和 Rust/Tauri 日志路径。
- [x] 失败回归：完整 JSON、行数校验、尾部读取上限、UTF-8/换行、缺失文件、安全路径。
- [x] 原 Python 路由/尾部读取的隔离参考对照。
- [x] 追加/重开/大小轮转/保留份数及文件访问失败的写入测试。
- [x] 接入宿主 tracing，证明 WARNING 过滤和 stdout 协议不受影响。
- [x] 原 Debug 页刷新、自动刷新、级别/文本筛选、排序和复制载荷验收（不操作系统剪贴板）。
- [x] Rust、SDK/CLI、原前端回归及真实 inventory 更新。
- [x] 重建本机九类 QA 制品并完成静态/隔离安装检查；包内启动、原生和跨平台门禁仍未关闭。

证据和未关闭门禁见 [Debug 日志验收](../testing/backend-debug-logs-acceptance.md)。
