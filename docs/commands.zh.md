# 系统命令

> **试验性功能**：系统命令尚未充分覆盖所有场景与边界情况，使用中仍可能遇到报错或异常，请以实际行为为准。

系统命令是一组以 `/` 开头的特殊指令，让你可以**直接控制对话状态**，而不需要等 AI 理解你的意图。

| 命令       | 需要等待 | 压缩摘要      | 长期记忆    | 消息历史      |
| ---------- | -------- | ------------- | ----------- | ------------- |
| `/compact` | ⏳ 是    | 📦 生成新摘要 | ✅ 后台保存 | 🏷️ 标记已压缩 |
| `/new`     | ⚡ 否    | 🗑️ 清空       | ✅ 后台保存 | 🏷️ 标记已压缩 |
| `/clear`   | ⚡ 否    | 🗑️ 清空       | ❌ 不保存   | 🗑️ 完全清空   |
| `/history` | ⚡ 否    | -             | -           | 📖 只读查看   |

---

## /compact - 压缩当前对话

手动触发对话压缩，将当前对话消息浓缩成摘要（**需要等待**），同时后台保存到长期记忆。

```
/compact
```

**返回示例：**

```
**Compact Complete!**
- Messages compacted: 12
**Compressed Summary:**
用户请求帮助构建用户认证系统，已完成登录接口的实现...
- Summary task started in background
```

> 💡 与自动压缩不同，`/compact` 会压缩**所有**当前消息，而不是只压缩超出阈值的部分。

---

## /new - 清空上下文并保存记忆

**立即清空当前上下文**，开始全新对话。后台同时保存历史到长期记忆。

```
/new
```

**返回示例：**

```
**New Conversation Started!**
- Summary task started in background
- Ready for new conversation
```

---

## /clear - 清空上下文（不保存记忆）

**立即清空当前上下文**，包括消息历史和压缩摘要。**不会**保存到长期记忆。

```
/clear
```

**返回示例：**

```
**History Cleared!**
- Compressed summary reset
- Memory is now empty
```

> ⚠️ **警告**：`/clear` 是**不可逆**的！与 `/new` 不同，清除的内容不会被保存。

---

## /history - 查看当前对话历史

显示当前对话中所有未压缩的消息列表。

```
/history
```

**返回示例：**

```
**Conversation History**
- Total messages: 5

[1] **user**: 帮我写一个 Python 函数...
[2] **assistant**: 好的，我来帮你写一个函数...
[3] **user**: 能不能加上错误处理？
...
```
