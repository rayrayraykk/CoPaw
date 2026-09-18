# 目录版本与 Git 排除规则完成时序 — 2026-09-15

范围为原目录版本决策等价、构建前回归发现的 Git 文件完成时序问题；不改原 Console 或插件执行路线。证据目录：`dist/qa-catalog-versions-20260915-i3xatQ/`，失败日志及终态 JSON 均保留。

## 目录版本

最初 3 个红测试证实：大数字 release 等分量溢出会走非法字符串回退，超大 max 被误判非法，NBSP 等 Python 外侧空白会显示虚假的升级提示。新增仅供目录使用的版本适配：同次比较共享十进制保序映射、固定零、保留含字母 local 片段和原版本文本，再调用原有纯 Rust PEP 440 解析/排序；无新依赖。

96 个版本组成 9216 次升级比较，并派生 288 条最低要求/上界校验输入，与原 Python 函数逐项和完整结构一致。覆盖 release/epoch/pre/post/dev/local、大于 u64 的数值与 300 位数字、零填充、别名、local 字典序、非法语法及当前原 `packaging` 同样拒绝的 Unicode 数字。原 113 条完整目录响应和原页面交互也重跑通过。

`official` 筛选回归共 18/18：17 项目录测试（含 Python 对照和原浏览器）以及 1 项既有官方 ACS3 签名向量；不是 18 个独立页面测试。

## Git 门禁发现与修复

第一轮 workspace 在 App Server 处停止：572 passed、1 failed、41 ignored。`identity_exclusion_preserves_user_rules_and_stage_all_and_clean_keep_metadata_private` 立即读取到未追加 marker 的用户规则，后续 workspace 测试当轮尚未运行。

生产 `exclude_core_identity` 仅等待 Tokio `write_all`，没有等待其后台 OS 写入完成。新测试占住唯一 blocking worker，再轮询实际文件追加函数：修复前错误地立刻报告完成，确定性红测试失败；补 `flush().await` 后只有释放 worker 并完成文件写入才返回。另一新测试通过只读文件证明延迟写入失败也会正确返回 HTTP 500 错误，原用户规则保持不变。

该修改只保证写入完成后才交给随后的 Git 操作/读取；不是 `fsync`，不宣称掉电持久化，也不宣称解决整个文件路径并发安全问题。

## 门禁与后续制品

严格检查曾分别提示新版本矩阵测试过长、新 Git 测试变量名相近；已拆分样例生成函数并重命名测试变量，没有降低 lint。最终 workspace 871 passed、0 failed、42 ignored，严格检查和格式通过；优化版 Git 3/3 也通过。新九包及逐包范围见 [构建验收](qa-official-catalog-packages-20260915.md)。极端 Python 整数转换阈值、特殊 Unicode case folding、嵌套元数据/异常结构、插件执行及完整原生/跨平台功能仍开放；当前专项不能代替整个 goal 的完成证明。
