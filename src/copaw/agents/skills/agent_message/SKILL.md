---
name: agent_message
description: Agent间通信和主动消息发送 - 查询agents/sessions，发送消息到channel，智能体间对话 | Inter-agent communication and proactive messaging - query agents/sessions, send to channels, agent-to-agent dialogue
metadata: { "builtin_skill_version": "1.0", "copaw": { "emoji": "💬" } }
---

# Agent 消息和通信 | Agent Messaging & Communication

## ⚠️ 重要提示 | Important Notice

**Agent间通信时必须标明身份！**  
**Always identify yourself in inter-agent messages!**

```bash
# ✅ 正确 | Correct
--text "[来自智能体 my_agent] 请分析数据"
--text "[Agent my_agent requesting] Analyze the data"

# ❌ 错误（目标agent会混淆）| Wrong (causes confusion)
--text "请分析数据"
--text "Analyze the data"
```

**为什么？** 目标agent需要区分消息来源是用户还是其他agent  
**Why?** Target agent needs to distinguish between user requests and agent requests

---

**⛔ 禁止循环调用！防止Agent死循环**  
**⛔ No Circular Calls! Prevent Agent Infinite Loops**

```bash
# ❌ 危险：Agent A 收到来自 Agent B 的消息，又回调 Agent B
# Dangerous: Agent A receives from Agent B, then calls back Agent B
# 当前agent: agent_a，收到来自 agent_b 的消息
# Current agent: agent_a, received message from agent_b

# ❌ 不能调用消息来源agent！
copaw message ask-agent \
  --from-agent agent_a \
  --to-agent agent_b \
  --text "[来自智能体 agent_a] ..."

# ✅ 正确：调用不同的agent，或不调用
# Correct: Call a different agent, or don't call at all
# ✅ 调用第三方agent
copaw message ask-agent \
  --from-agent agent_a \
  --to-agent agent_c \
  --text "[来自智能体 agent_a] ..."
```

**规则 | Rules**：
- 如果消息来自 Agent X，**不要**再调用 Agent X
- If message is from Agent X, **DO NOT** call Agent X back
- 检查消息来源（从消息文本中提取 `[来自智能体 xxx]` 或 `[Agent xxx]`）
- Check message source (extract from message text: `[来自智能体 xxx]` or `[Agent xxx]`)
- 避免 A→B→A 或 A→B→C→A 的循环调用链
- Avoid circular chains like A→B→A or A→B→C→A

**为什么？** 防止agent间无限循环调用，导致资源耗尽  
**Why?** Prevent infinite loops between agents that exhaust resources

---

## 📋 使用前必读 | Must Read Before Use

### ❌ 常见错误 | Common Mistakes

```bash
# ❌ 错误：直接发送消息，不知道target-user和target-session
copaw message send --agent-id bot --channel console --text "hello"
# Error: Missing required parameters --target-user and --target-session

# ✅ 正确：先查询，再发送
# Step 1: Query
copaw message list-sessions --agent-id bot --channel console
# Get: user_id="alice", session_id="alice_session_001"

# Step 2: Send
copaw message send \
  --agent-id bot \
  --channel console \
  --target-user alice \
  --target-session alice_session_001 \
  --text "hello"
```

### 🎯 记住这个流程 | Remember This Flow

```
永远遵循 | Always Follow:
  查询 → 获取参数 → 使用参数发送
  Query → Get Parameters → Use Parameters to Send
```

---

## 🚀 快速开始 | Quick Start

### 示例：发送消息到channel | Example: Send Message to Channel

```bash
# 步骤1：查询可用sessions（必须先做！）
# Step 1: Query available sessions (must do first!)
copaw message list-sessions --agent-id my_bot --channel console

# 从输出中获取: user_id, session_id
# From output get: user_id, session_id
# Example: "user_id": "alice", "session_id": "alice_console_001"

# 步骤2：发送消息（使用查询到的参数）
# Step 2: Send message (use queried parameters)
copaw message send \
  --agent-id my_bot \
  --channel console \
  --target-user alice \
  --target-session alice_console_001 \
  --text "Hello Alice!"
```

### 示例：Agent间对话 | Example: Inter-agent Dialogue

```bash
# 简单对话（自动生成session ID）
# Simple chat (auto-generated session ID)
copaw message ask-agent \
  --from-agent bot_a \
  --to-agent bot_b \
  --text "[来自智能体 bot_a] 今天天气怎么样？"

# 输出: INFO: Using session_id: bot_a:to:bot_b:1773998835:abc123
# Output: INFO: Using session_id: bot_a:to:bot_b:1773998835:abc123

# 继续对话（复用session ID）
# Continue conversation (reuse session ID)
copaw message ask-agent \
  --from-agent bot_a \
  --to-agent bot_b \
  --session-id "bot_a:to:bot_b:1773998835:abc123" \
  --text "[来自智能体 bot_a] 明天呢？"
```

---

## 中文说明 | Chinese Documentation

使用 `copaw agents` 和 `copaw message` 命令实现智能体间通信和主动消息发送。

### 核心功能

1. **查询可用资源**：发现可通信的agents和sessions
2. **主动发送消息**：向channel用户发送文本消息
3. **智能体间对话**：与其他agent交互并获取响应

### 🔑 核心使用流程

```
步骤1: 查询目标
  ↓
copaw message list-sessions --agent-id <your_agent>
  ↓
获取: target-user, target-session, channel
  ↓
步骤2: 发送消息/对话
  ↓
copaw message send/ask-agent ... (使用步骤1的参数)
```

⚠️ **不可跳过步骤1！** 所有目标参数（user/session）都必须从查询结果中获取，不能猜测。

---

## 一、查询命令（发现目标）

### 1.1 列出所有 Agents

```bash
# 查看所有配置的agents
copaw agents list

# 或使用 message 子命令
copaw message list-agents
```

**返回示例**：
```json
{
  "agents": [
    {
      "id": "default",
      "name": "Default Agent",
      "description": "...",
      "workspace_dir": "/Users/..."
    },
    {
      "id": "finance_bot",
      "name": "Finance Assistant",
      "description": "...",
      "workspace_dir": "/Users/..."
    }
  ]
}
```

### 1.2 列出 Sessions 和 Users

```bash
# 查看指定agent的所有sessions
copaw message list-sessions --agent-id my_bot

# 过滤特定channel
copaw message list-sessions --agent-id my_bot --channel dingtalk

# 限制返回数量
copaw message list-sessions --agent-id my_bot --limit 10
```

**返回信息**：
- `sessions`：所有会话列表（包含session_id）
- `unique_users`：聚合的用户信息（channels、session_count、last_active）
- `inter_agent_sessions`：智能体间通信的sessions

**Session ID的作用**：
- 📝 **决定对话历史**：相同session_id = 有上下文的连续对话
- 🔄 **复用session**：要继续之前的对话，必须使用相同的session_id
- 🆕 **自动生成**：CLI默认自动生成新session_id（每次都是新对话）
- 💡 **如何接着聊**：先用此命令查询已有session_id，然后在ask-agent中用`--session-id`复用

**重要**：
1. 发送消息前，**必须**先用此命令查询目标是否存在
2. 如果要继续之前的对话，**记录并复用**返回的session_id

---

## 二、发送消息到 Channel

向已存在的用户/session发送文本消息。

⚠️ **重要前提**：
- ✅ 必须先用 `copaw message list-sessions` 查询有效的 `target-user` 和 `target-session`
- ✅ 所有参数（`--agent-id`, `--channel`, `--target-user`, `--target-session`, `--text`）都是**必填**
- ❌ 不能发送到不存在的用户或session

### 2.1 完整使用流程（必须遵循）⭐

**步骤1：查询可用的sessions（必需步骤）**

```bash
# 查询指定agent的所有sessions
copaw message list-sessions --agent-id my_bot

# 输出示例：
{
  "unique_users": [
    {
      "user_id": "alice",
      "channels": ["console", "dingtalk"],
      "session_count": 2,
      "last_active": "2026-03-20T10:30:00"
    }
  ],
  "sessions": [
    {
      "session_id": "alice_session_001",
      "user_id": "alice",
      "channel": "console",
      ...
    }
  ]
}
```

**步骤2：使用查询到的参数发送消息**

```bash
# 使用步骤1查询到的 user_id, channel, session_id
copaw message send \
  --agent-id my_bot \
  --channel console \
  --target-user alice \
  --target-session alice_session_001 \
  --text "Hello from my_bot!"
```

### 2.2 实用示例

**示例1：发送到console channel**

```bash
# 1. 先查询
copaw message list-sessions --agent-id my_bot --channel console

# 2. 从输出中找到：user_id=alice, session_id=alice_console_20240320
copaw message send \
  --agent-id my_bot \
  --channel console \
  --target-user alice \
  --target-session alice_console_20240320 \
  --text "任务完成通知：数据分析已完成"
```

**示例2：发送到DingTalk**

```bash
# 1. 先查询DingTalk的sessions
copaw message list-sessions --agent-id sales_bot --channel dingtalk

# 2. 从输出中找到：user_id=dt_user_123, session_id=dt_session_456
copaw message send \
  --agent-id sales_bot \
  --channel dingtalk \
  --target-user dt_user_123 \
  --target-session dt_session_456 \
  --text "您的订单已确认，预计3天内送达"
```

**示例3：使用jq自动提取参数**

```bash
# 查询并自动提取第一个console session的参数
SESSIONS=$(copaw message list-sessions --agent-id my_bot --channel console)
TARGET_USER=$(echo "$SESSIONS" | jq -r '.sessions[0].user_id')
TARGET_SESSION=$(echo "$SESSIONS" | jq -r '.sessions[0].session_id')

# 发送消息
copaw message send \
  --agent-id my_bot \
  --channel console \
  --target-user "$TARGET_USER" \
  --target-session "$TARGET_SESSION" \
  --text "自动化消息：定时任务执行成功"
```

### 2.3 必填参数说明

| 参数 | 说明 | 如何获取 |
|------|------|---------|
| `--agent-id` | 发送者的agent ID（你自己） | 系统提示的 Agent Identity 或 `copaw message list-agents` |
| `--channel` | 目标channel | `copaw message list-sessions` 的返回中查看 |
| `--target-user` | 目标用户ID（**必须已存在**） | `copaw message list-sessions` 查询获取 |
| `--target-session` | 目标会话ID（**必须已存在**） | `copaw message list-sessions` 查询获取 |
| `--text` | 消息文本内容 | 你想发送的内容 |

⚠️ **常见错误**：
- ❌ 直接发送，不先查询 → 报错：session不存在
- ❌ 猜测user_id或session_id → 报错：目标不存在
- ❌ 漏掉任何一个参数 → 报错：缺少必需参数

### 2.4 使用建议

1. **查询先行**（必须）：先用 `list-sessions` 确认target-user和target-session存在
2. **参数验证**：确保所有5个必填参数都已提供
3. **获取agent-id**：从系统提示的 Agent Identity 部分读取（`Your agent id is ...`）
4. **验证channel**：用 `copaw channels list` 确认channel已配置
5. **错误处理**：如果发送失败，检查channel配置和用户权限

---

## 三、智能体间通信（ask-agent）

向其他agent发送消息并获取响应，实现agent协作。

### ⛔ 3.0 循环调用检测（必读！）

**在调用其他agent前，务必检查是否会造成循环调用！**

#### 核心规则

**检查上下文中的消息来源**，确保 `--to-agent` 不等于来源agent

```bash
# 你是 agent_a，收到来自 agent_b 的消息

# ❌ 禁止：回调来源agent
copaw message ask-agent \
  --from-agent agent_a \
  --to-agent agent_b \
  --text "[来自智能体 agent_a] ..."
→ 会造成 A→B→A→B... 死循环

# ✅ 允许：调用第三方agent
copaw message ask-agent \
  --from-agent agent_a \
  --to-agent agent_c \
  --text "[来自智能体 agent_a] ..."

# ✅ 允许：通过channel发送给用户
copaw message send \
  --agent-id agent_a \
  --channel console \
  --target-user ... \
  --target-session ... \
  --text "..."
```

**检查清单**：
- [ ] 已识别当前消息来源agent（从上下文获取）
- [ ] `--to-agent` 不等于消息来源agent
- [ ] 如无需调用其他agent，直接响应或通过channel发送

### 3.1 基础用法（推荐）

```bash
# 自动生成唯一session（并发安全，无历史上下文）
copaw message ask-agent \
  --from-agent bot_a \
  --to-agent bot_b \
  --text "[来自智能体 bot_a] 今天天气怎么样？"

# 输出示例（stderr）：
# INFO: Using session_id: bot_a:to:bot_b:1773998835:abc12345
#
# （stdout - 回复内容）：
# 今天天气晴朗...
```

**重要提示**：
- ⚠️ **必须在消息开头说明身份**：使用 `[来自智能体 <your_agent_id>]` 前缀
- 避免目标agent混淆消息来源（区分agent请求 vs 用户请求）
- 格式示例：`"[来自智能体 bot_a] 请帮我分析数据"`

**Session管理原理**：
- 📝 **Session ID = 对话历史**：相同session_id表示同一段对话，包含历史上下文
- 🆕 **默认行为**：自动生成唯一session ID（格式：`{from}:to:{to}:{timestamp_ms}:{uuid_short}`）
- 🔄 **默认结果**：每次调用都是**全新对话**，无历史上下文
- ⚡ **优势**：并发安全，避免多个请求冲突
- 📌 **适用场景**：单次问答、独立请求、并发调用
- 📤 **返回值**：CLI会在stderr输出生成的session_id，可以捕获并复用

**关键理解**：
- ❌ 自动生成的session = 每次都是新对话，**目标agent看不到之前的内容**
- ✅ 如果要接着聊，必须**捕获并复用**输出的session_id（见3.2节）

### 3.2 复用 Session（上下文对话） ⭐

**关键场景**：需要让目标agent记住之前的对话内容，实现连续对话。

#### 步骤1：查询已有sessions（可选，如果不知道session_id）

```bash
# 查询之前与bot_b的对话sessions
copaw message list-sessions --agent-id bot_b

# 输出示例：
# "inter_agent_sessions": [
#   {
#     "from_agent": "bot_a",
#     "to_agent": "bot_b",
#     "session_id": "bot_a:to:bot_b:1773998835:523e5fb5",
#     "last_active": "2026-03-20T10:30:00Z"
#   }
# ]
```

#### 步骤2：复用session_id进行对话

**方式A：自定义session ID（推荐用于脚本）**

```bash
# 第一次对话（建立session，使用自定义ID）
copaw message ask-agent \
  --from-agent bot_a \
  --to-agent bot_b \
  --session-id "bot_a:to:bot_b:conv001" \
  --text "[来自智能体 bot_a] 我想了解量子计算"

# 响应：<解释量子计算的内容>

# 继续对话（复用相同session，目标agent能看到之前的对话）
copaw message ask-agent \
  --from-agent bot_a \
  --to-agent bot_b \
  --session-id "bot_a:to:bot_b:conv001" \
  --text "[来自智能体 bot_a] 能详细解释一下量子纠缠吗？"

# 响应：<基于之前对话的上下文，继续解释量子纠缠>
```

**方式B：捕获自动生成的session ID（推荐用于命令行）**

```bash
# 第一次对话，捕获stderr中的session_id
SESSION_ID=$(copaw message ask-agent \
  --from-agent bot_a \
  --to-agent bot_b \
  --text "[来自智能体 bot_a] 我想了解量子计算" \
  2>&1 >/dev/null | grep "Using session_id:" | awk '{print $4}')

echo "Captured session: $SESSION_ID"
# 输出：Captured session: bot_a:to:bot_b:1773998835:abc12345

# 继续对话（使用捕获的session_id）
copaw message ask-agent \
  --from-agent bot_a \
  --to-agent bot_b \
  --session-id "$SESSION_ID" \
  --text "[来自智能体 bot_a] 能详细解释一下量子纠缠吗？"
```

**方式C：使用JSON输出模式**

```bash
# 第一次对话，使用--json-output
RESPONSE=$(copaw message ask-agent \
  --from-agent bot_a \
  --to-agent bot_b \
  --text "[来自智能体 bot_a] 我想了解量子计算" \
  --json-output)

# 提取session_id和文本内容
SESSION_ID=$(echo "$RESPONSE" | jq -r '.session_id')
ANSWER=$(echo "$RESPONSE" | jq -r '.output[-1].content[] | select(.type=="text") | .text')

echo "Session: $SESSION_ID"
echo "Answer: $ANSWER"

# 继续对话
copaw message ask-agent \
  --from-agent bot_a \
  --to-agent bot_b \
  --session-id "$SESSION_ID" \
  --text "[来自智能体 bot_a] 能详细解释一下量子纠缠吗？"
```

**对比说明**：

| 方式 | Session ID | 对话上下文 | 使用场景 |
|------|-----------|----------|---------|
| 自动生成（默认） | 每次不同 | ❌ 无历史 | 独立问答、并发调用 |
| 显式指定 | 相同 | ✅ 有历史 | 连续对话、上下文依赖 |

**示例对比**：

```bash
# ❌ 错误做法：想接着聊，但没有捕获session ID
copaw message ask-agent --from-agent a --to-agent b \
  --text "[来自智能体 a] 量子计算是什么？"
# INFO: Using session_id: a:to:b:1773998835:abc123
# 响应：<解释量子计算>

copaw message ask-agent --from-agent a --to-agent b \
  --text "[来自智能体 a] 刚才说的能详细点吗？"
# INFO: Using session_id: a:to:b:1773998840:xyz789 ← 新session！
# 响应：什么？你没问过我问题啊 ← 对话历史丢失！

# ✅ 正确做法1：使用自定义session ID
CONV_SESSION="a:to:b:conv_quantum"

copaw message ask-agent --from-agent a --to-agent b \
  --session-id "$CONV_SESSION" \
  --text "[来自智能体 a] 量子计算是什么？"
# INFO: Using session_id: a:to:b:conv_quantum
# 响应：<解释量子计算>

copaw message ask-agent --from-agent a --to-agent b \
  --session-id "$CONV_SESSION" \
  --text "[来自智能体 a] 刚才说的能详细点吗？"
# INFO: Using session_id: a:to:b:conv_quantum ← 同一session！
# 响应：当然，关于量子计算我刚才提到... ← 有上下文！

# ✅ 正确做法2：捕获自动生成的session ID
SESSION_ID=$(copaw message ask-agent \
  --from-agent a --to-agent b \
  --text "[来自智能体 a] 量子计算是什么？" \
  2>&1 >/dev/null | grep "Using session_id:" | awk '{print $4}')

copaw message ask-agent --from-agent a --to-agent b \
  --session-id "$SESSION_ID" \
  --text "[来自智能体 a] 刚才说的能详细点吗？"
# 响应：当然，关于量子计算我刚才提到... ← 有上下文！
```

**注意事项**：
- ⚠️ 复用session时需注意并发问题（多个请求同时使用同一session会报错）
- 💡 建议：一次对话结束后，再开始下一轮复用同一session
- 📋 首次对话必须包含身份标识，后续对话可简化（session已建立上下文）
- 🔍 使用 `list-sessions` 查询已有的session_id，避免猜测

### 3.3 转发响应到 Channel

```bash
# 询问其他agent并将结果发送给用户
copaw message ask-agent \
  --from-agent monitor \
  --to-agent analyst \
  --text "[来自智能体 monitor] 分析最近的错误日志" \
  --channel dingtalk \
  --target-user manager_001 \
  --target-session alert_session
```

转发响应需要同时指定：
- `--channel`：目标channel
- `--target-user`：接收者用户ID
- `--target-session`：接收者会话ID

### 3.4 流式响应（Stream Mode）

```bash
# 实时流式输出（适合长响应）
copaw message ask-agent \
  --from-agent ui \
  --to-agent research \
  --text "[来自智能体 ui] 写一篇关于人工智能的文章" \
  --mode stream
```

- `--mode final`（默认）：等待完整响应
- `--mode stream`：实时增量输出（SSE）

### 3.5 JSON 输出格式

```bash
# 获取完整JSON响应（包含metadata）
copaw message ask-agent \
  --from-agent bot_a \
  --to-agent bot_b \
  --text "[来自智能体 bot_a] 测试" \
  --json-output
```

### 3.6 其他选项

```bash
# 自定义超时时间（默认300秒）
copaw message ask-agent \
  --from-agent bot_a \
  --to-agent bot_b \
  --text "[来自智能体 bot_a] 复杂任务" \
  --timeout 600

# 强制新建session（即使指定了--session-id）
copaw message ask-agent \
  --from-agent bot_a \
  --to-agent bot_b \
  --session-id "old_session" \
  --new-session \
  --text "[来自智能体 bot_a] 新对话"
```

---

## 四、完整工作流程示例

### 示例1：智能体协作处理用户请求

```bash
# 步骤1：查询可用的agents
copaw message list-agents

# 步骤2：向专家agent提问（明确标识身份）
copaw message ask-agent \
  --from-agent general_assistant \
  --to-agent finance_expert \
  --text "[来自智能体 general_assistant] 分析Q1财报数据"

# 步骤3：将结果发送给用户
copaw message send \
  --agent-id general_assistant \
  --channel dingtalk \
  --target-user user_123 \
  --target-session session_456 \
  --text "财报分析已完成：..."
```

### 示例2：定期查询其他agent并通知

```bash
# 配合 cron skill 实现定期agent查询
# 在cron任务中调用
copaw message ask-agent \
  --from-agent monitor \
  --to-agent system_checker \
  --text "[来自智能体 monitor - 定时检查] 检查系统状态" \
  --channel console \
  --target-user admin \
  --target-session monitoring
```

### 示例3：多agent协作链

```bash
# Agent A 询问 Agent B
RESPONSE_B=$(copaw message ask-agent \
  --from-agent agent_a \
  --to-agent agent_b \
  --text "[来自智能体 agent_a] 数据收集" | tail -1)

# Agent A 将 Agent B 的响应转发给 Agent C
copaw message ask-agent \
  --from-agent agent_a \
  --to-agent agent_c \
  --text "[来自智能体 agent_a] 分析这些数据: $RESPONSE_B"
```

---

## 五、最佳实践

### 5.0 查询先行原则（最重要！）⭐

**铁律：发送消息前必须先查询目标参数**

```bash
# ❌ 错误：直接猜测参数
copaw message send --agent-id bot --channel console \
  --target-user alice --target-session "guess_123" --text "hi"
# 结果：报错，session不存在

# ✅ 正确：查询→获取→使用
# Step 1: 查询
SESSIONS=$(copaw message list-sessions --agent-id bot --channel console)

# Step 2: 提取参数
USER=$(echo "$SESSIONS" | jq -r '.sessions[0].user_id')
SESSION=$(echo "$SESSIONS" | jq -r '.sessions[0].session_id')

# Step 3: 使用查询到的参数
copaw message send --agent-id bot --channel console \
  --target-user "$USER" --target-session "$SESSION" --text "hi"
# 结果：发送成功
```

**为什么必须查询？**
- target-user 和 target-session 是系统生成的，无法猜测
- 只有实际存在的session才能接收消息
- 查询可以确保参数有效，避免浪费调用

**记住：** 
1. `send` 命令：5个参数（agent-id, channel, target-user, target-session, text）**全部必填**
2. target-user 和 target-session **必须**从 `list-sessions` 查询获取
3. 不要猜测、不要硬编码，永远先查询

### 5.1 消息身份标识（重要！）

**必须在agent间消息中标明发送者身份**，避免目标agent混淆：

✅ **正确格式**：
```bash
--text "[来自智能体 my_agent] 请分析数据"
--text "[Agent my_agent requesting] Analyze the data"
```

❌ **错误格式**（会导致混淆）：
```bash
--text "请分析数据"  # 目标agent会误认为是用户请求
```

**身份标识建议**：
- 使用方括号 `[来自智能体 <agent_id>]` 或 `[Agent <agent_id> requesting]`
- 放在消息开头，清晰醒目
- 可选：说明请求目的，如 `[来自智能体 monitor - 定时检查]`

### 5.2 安全的并发策略

- **默认行为**：让系统自动生成唯一session ID
- **显式session**：仅在需要上下文连续性时使用
- **并发控制**：多个agent同时调用时，避免共享session ID

### 5.3 错误处理

```bash
# 检查命令执行结果
if copaw message send --agent-id bot --channel console \
   --target-user alice --target-session s1 --text "test"; then
  echo "发送成功"
else
  echo "发送失败，检查参数和配置"
fi
```

### 5.4 查询验证

**发送消息前的检查清单**：
1. ✅ `copaw message list-agents` - 确认目标agent存在
2. ✅ `copaw message list-sessions --agent-id X` - 确认session/user存在
3. ✅ `copaw channels list` - 确认channel已配置
4. ✅ 验证自己的agent_id（系统提示中查找）
5. ✅ **在消息中标明身份** - 使用 `[来自智能体 <your_id>]` 前缀
6. ✅ **检查循环调用** - 确保 `--to-agent` 不是消息来源agent

### 5.5 日志和调试

- 使用 `--json-output` 查看完整响应结构
- 检查 `~/.copaw/logs/` 中的日志文件
- 用 `copaw app` 的日志输出排查问题

### ⛔ 5.6 循环调用防止（重要！）

**永远不要回调发送消息给你的agent！**

#### 核心规则

**检查上下文中的消息来源**，确保 `--to-agent` 不等于来源agent

```
示例：你是 agent_a，收到来自 agent_b 的消息

❌ 禁止：
copaw message ask-agent --from-agent agent_a --to-agent agent_b
→ 会造成 A→B→A→B... 无限循环

✅ 允许：
copaw message ask-agent --from-agent agent_a --to-agent agent_c
→ 调用第三方，不会循环

✅ 允许：
copaw message send --agent-id agent_a --channel console --target-user ...
→ 直接回复用户，不经过 agent_b
```

**关键要点**：
- 上下文中已包含消息来源信息，检查后确保不回调它
- 替代方案：调用其他agent或通过channel发送结果

---

## 六、命令速查表

| 命令 | 用途 | 必需前置步骤 | 示例 |
|------|------|-------------|------|
| `copaw message list-agents` | 列出所有agents | 无 | `copaw message list-agents` |
| `copaw message list-sessions` | 查询sessions和users | 无 | `copaw message list-sessions --agent-id bot` |
| `copaw message send` | 发送消息到channel | ⚠️ **必须先 list-sessions** | `copaw message send --agent-id bot --channel console --target-user <从查询获取> --target-session <从查询获取> --text "..."` |
| `copaw message ask-agent` | agent间通信 | 可选 list-agents | `copaw message ask-agent --from-agent a --to-agent b --text "[来自智能体 a] ..."` |

### 关键记忆点

- ✅ `send` 命令：5个参数全部必填，必须先查询获取 target-user 和 target-session
- ✅ `ask-agent` 命令：自动生成session ID，从stderr输出可捕获复用
- ✅ Session ID决定对话历史：相同session = 有上下文，不同session = 新对话
- ✅ Agent间消息必须带身份前缀：`[来自智能体 <your_id>] ...`

---

## English Documentation

Use `copaw agents` and `copaw message` commands for inter-agent communication and proactive messaging.

### Core Features

1. **Query Resources**: Discover available agents and sessions
2. **Send Messages**: Send text messages to channel users
3. **Agent Dialogue**: Interact with other agents and get responses

### 🔑 Core Usage Flow

```
Step 1: Query targets
  ↓
copaw message list-sessions --agent-id <your_agent>
  ↓
Get: target-user, target-session, channel
  ↓
Step 2: Send message/dialogue
  ↓
copaw message send/ask-agent ... (use parameters from Step 1)
```

⚠️ **Don't skip Step 1!** All target parameters (user/session) must be obtained from query results, don't guess.

---

## I. Query Commands (Resource Discovery)

### 1.1 List All Agents

```bash
# View all configured agents
copaw agents list

# Or use message subcommand
copaw message list-agents
```

**Example Response**:
```json
{
  "agents": [
    {
      "id": "default",
      "name": "Default Agent",
      "description": "...",
      "workspace_dir": "/Users/..."
    }
  ]
}
```

### 1.2 List Sessions and Users

```bash
# View all sessions for a specific agent
copaw message list-sessions --agent-id my_bot

# Filter by channel
copaw message list-sessions --agent-id my_bot --channel dingtalk

# Limit results
copaw message list-sessions --agent-id my_bot --limit 10
```

**Returned Information**:
- `sessions`: All chat sessions (includes session_id)
- `unique_users`: Aggregated user info (channels, session_count, last_active)
- `inter_agent_sessions`: Inter-agent communication sessions

**Session ID Purpose**:
- 📝 **Determines conversation history**: Same session_id = continuous conversation with context
- 🔄 **Reuse session**: To continue previous conversation, must use same session_id
- 🆕 **Auto-generated**: CLI auto-generates new session_id by default (new conversation each time)
- 💡 **How to continue chatting**: Query existing session_id with this command, then reuse with `--session-id` in ask-agent

**Important**: 
1. **Always query first** before sending messages to verify target exists
2. If you want to continue previous conversation, **record and reuse** the returned session_id

---

## II. Send Messages to Channels

Send text messages to existing users/sessions.

⚠️ **Important Prerequisites**:
- ✅ Must first use `copaw message list-sessions` to query valid `target-user` and `target-session`
- ✅ All parameters (`--agent-id`, `--channel`, `--target-user`, `--target-session`, `--text`) are **REQUIRED**
- ❌ Cannot send to non-existent users or sessions

### 2.1 Complete Usage Flow (Must Follow) ⭐

**Step 1: Query available sessions (REQUIRED)**

```bash
# Query all sessions for specified agent
copaw message list-sessions --agent-id my_bot

# Example output:
{
  "unique_users": [
    {
      "user_id": "alice",
      "channels": ["console", "dingtalk"],
      "session_count": 2,
      "last_active": "2026-03-20T10:30:00"
    }
  ],
  "sessions": [
    {
      "session_id": "alice_session_001",
      "user_id": "alice",
      "channel": "console",
      ...
    }
  ]
}
```

**Step 2: Send message using queried parameters**

```bash
# Use user_id, channel, session_id from Step 1
copaw message send \
  --agent-id my_bot \
  --channel console \
  --target-user alice \
  --target-session alice_session_001 \
  --text "Hello from my_bot!"
```

### 2.2 Practical Examples

**Example 1: Send to console channel**

```bash
# 1. Query first
copaw message list-sessions --agent-id my_bot --channel console

# 2. Find from output: user_id=alice, session_id=alice_console_20240320
copaw message send \
  --agent-id my_bot \
  --channel console \
  --target-user alice \
  --target-session alice_console_20240320 \
  --text "Task completion notification: Data analysis finished"
```

**Example 2: Send to DingTalk**

```bash
# 1. Query DingTalk sessions
copaw message list-sessions --agent-id sales_bot --channel dingtalk

# 2. Find from output: user_id=dt_user_123, session_id=dt_session_456
copaw message send \
  --agent-id sales_bot \
  --channel dingtalk \
  --target-user dt_user_123 \
  --target-session dt_session_456 \
  --text "Your order is confirmed, delivery in 3 days"
```

**Example 3: Auto-extract parameters with jq**

```bash
# Query and auto-extract first console session parameters
SESSIONS=$(copaw message list-sessions --agent-id my_bot --channel console)
TARGET_USER=$(echo "$SESSIONS" | jq -r '.sessions[0].user_id')
TARGET_SESSION=$(echo "$SESSIONS" | jq -r '.sessions[0].session_id')

# Send message
copaw message send \
  --agent-id my_bot \
  --channel console \
  --target-user "$TARGET_USER" \
  --target-session "$TARGET_SESSION" \
  --text "Automated message: Scheduled task executed successfully"
```

### 2.3 Required Parameters Explanation

| Parameter | Description | How to Obtain |
|-----------|-------------|---------------|
| `--agent-id` | Sender agent ID (yourself) | System prompt Agent Identity or `copaw message list-agents` |
| `--channel` | Target channel | Check in `copaw message list-sessions` output |
| `--target-user` | Target user ID (**must exist**) | Query with `copaw message list-sessions` |
| `--target-session` | Target session ID (**must exist**) | Query with `copaw message list-sessions` |
| `--text` | Message text content | Your message content |

⚠️ **Common Errors**:
- ❌ Send directly without querying → Error: session doesn't exist
- ❌ Guess user_id or session_id → Error: target not found
- ❌ Missing any parameter → Error: required parameter missing

### 2.4 Usage Recommendations

1. **Query First** (REQUIRED): Use `list-sessions` to confirm target-user and target-session exist
2. **Validate Parameters**: Ensure all 5 required parameters are provided
3. **Get agent-id**: Read from system prompt Agent Identity section (`Your agent id is ...`)
4. **Verify Channel**: Use `copaw channels list` to confirm channel is configured
5. **Error Handling**: If send fails, check channel configuration and user permissions

---

## III. Inter-Agent Communication (ask-agent)

Send messages to other agents and receive responses.

### ⛔ 3.0 Circular Call Prevention (Must Read!)

**Always check for circular calls before calling other agents!**

#### Core Rule

**Check message source in your context**, ensure `--to-agent` does NOT equal source agent

```bash
# You are agent_a, received message from agent_b

# ❌ Forbidden: Call back source agent
copaw message ask-agent \
  --from-agent agent_a \
  --to-agent agent_b \
  --text "[Agent agent_a requesting] ..."
→ Creates A→B→A→B... infinite loop

# ✅ Allowed: Call third-party agent
copaw message ask-agent \
  --from-agent agent_a \
  --to-agent agent_c \
  --text "[Agent agent_a requesting] ..."

# ✅ Allowed: Send to user via channel
copaw message send \
  --agent-id agent_a \
  --channel console \
  --target-user ... \
  --target-session ... \
  --text "..."
```

**Checklist** before calling `ask-agent`:
- [ ] Identified message source agent (from context)
- [ ] `--to-agent` does NOT equal source agent
- [ ] If no need to call others, respond directly or send via channel

### 3.1 Basic Usage (Recommended)

```bash
# Auto-generate unique session (concurrency-safe, no history)
copaw message ask-agent \
  --from-agent bot_a \
  --to-agent bot_b \
  --text "[Agent bot_a requesting] What's the weather today?"

# Output example (stderr):
# INFO: Using session_id: bot_a:to:bot_b:1773998835:abc12345
#
# (stdout - response content):
# The weather is sunny today...
```

**Important Note**:
- ⚠️ **Always identify yourself in messages**: Use `[Agent <your_agent_id> requesting]` prefix
- Prevents target agent from confusing message source (agent request vs user request)
- Format example: `"[Agent bot_a requesting] Please analyze the data"`

**Session Management Principles**:
- 📝 **Session ID = Conversation History**: Same session_id means continuous conversation with context
- 🆕 **Default Behavior**: Auto-generates unique session ID (format: `{from}:to:{to}:{timestamp_ms}:{uuid_short}`)
- 🔄 **Default Result**: Each call is a **brand new conversation**, no history context
- ⚡ **Advantage**: Concurrency-safe, avoids conflicts
- 📌 **Use Cases**: One-off questions, independent requests, concurrent calls
- 📤 **Return Value**: CLI outputs generated session_id to stderr for capture and reuse

**Key Understanding**:
- ❌ Auto-generated session = New conversation each time, **target agent can't see previous messages**
- ✅ To continue chatting, must **capture and reuse** the outputted session_id (see section 3.2)

### 3.2 Reuse Session (Contextual Conversation) ⭐

**Critical Scenario**: Need target agent to remember previous conversation content for continuous dialogue.

#### Step 1: Query existing sessions (optional, if you don't know session_id)

```bash
# Query previous conversation sessions with bot_b
copaw message list-sessions --agent-id bot_b

# Example output:
# "inter_agent_sessions": [
#   {
#     "from_agent": "bot_a",
#     "to_agent": "bot_b",
#     "session_id": "bot_a:to:bot_b:1773998835:523e5fb5",
#     "last_active": "2026-03-20T10:30:00Z"
#   }
# ]
```

#### Step 2: Reuse session_id for conversation

**Method A: Custom Session ID (recommended for scripts)**

```bash
# First conversation (establish session with custom ID)
copaw message ask-agent \
  --from-agent bot_a \
  --to-agent bot_b \
  --session-id "bot_a:to:bot_b:conv001" \
  --text "[Agent bot_a requesting] Tell me about quantum computing"

# Response: <Explanation about quantum computing>

# Continue conversation (reuse same session, target sees history)
copaw message ask-agent \
  --from-agent bot_a \
  --to-agent bot_b \
  --session-id "bot_a:to:bot_b:conv001" \
  --text "[Agent bot_a] Can you explain quantum entanglement?"

# Response: <Continues based on previous context>
```

**Method B: Capture Auto-generated Session ID (recommended for command line)**

```bash
# First conversation, capture session_id from stderr
SESSION_ID=$(copaw message ask-agent \
  --from-agent bot_a \
  --to-agent bot_b \
  --text "[Agent bot_a requesting] Tell me about quantum computing" \
  2>&1 >/dev/null | grep "Using session_id:" | awk '{print $4}')

echo "Captured session: $SESSION_ID"
# Output: Captured session: bot_a:to:bot_b:1773998835:abc12345

# Continue conversation (use captured session_id)
copaw message ask-agent \
  --from-agent bot_a \
  --to-agent bot_b \
  --session-id "$SESSION_ID" \
  --text "[Agent bot_a] Can you explain quantum entanglement?"
```

**Method C: Using JSON Output Mode**

```bash
# First conversation with --json-output
RESPONSE=$(copaw message ask-agent \
  --from-agent bot_a \
  --to-agent bot_b \
  --text "[Agent bot_a requesting] Tell me about quantum computing" \
  --json-output)

# Extract session_id and text content
SESSION_ID=$(echo "$RESPONSE" | jq -r '.session_id')
ANSWER=$(echo "$RESPONSE" | jq -r '.output[-1].content[] | select(.type=="text") | .text')

echo "Session: $SESSION_ID"
echo "Answer: $ANSWER"

# Continue conversation
copaw message ask-agent \
  --from-agent bot_a \
  --to-agent bot_b \
  --session-id "$SESSION_ID" \
  --text "[Agent bot_a] Can you explain quantum entanglement?"
```

**Comparison Table**:

| Method | Session ID | Conversation Context | Use Case |
|--------|-----------|---------------------|----------|
| Auto-generated (default) | Different each time | ❌ No history | Independent Q&A, concurrent calls |
| Explicitly specified | Same | ✅ Has history | Continuous chat, context-dependent |

**Example Comparison**:

```bash
# ❌ Wrong: Want to continue, but didn't capture session ID
copaw message ask-agent --from-agent a --to-agent b \
  --text "[Agent a requesting] What is quantum computing?"
# INFO: Using session_id: a:to:b:1773998835:abc123
# Response: <Quantum computing explanation>

copaw message ask-agent --from-agent a --to-agent b \
  --text "[Agent a] Can you elaborate on what you just said?"
# INFO: Using session_id: a:to:b:1773998840:xyz789 ← New session!
# Response: What? You haven't asked me anything ← Context lost!

# ✅ Correct Method 1: Use custom session ID
CONV_SESSION="a:to:b:conv_quantum"

copaw message ask-agent --from-agent a --to-agent b \
  --session-id "$CONV_SESSION" \
  --text "[Agent a requesting] What is quantum computing?"
# INFO: Using session_id: a:to:b:conv_quantum
# Response: <Quantum computing explanation>

copaw message ask-agent --from-agent a --to-agent b \
  --session-id "$CONV_SESSION" \
  --text "[Agent a] Can you elaborate on what you just said?"
# INFO: Using session_id: a:to:b:conv_quantum ← Same session!
# Response: Sure, about quantum computing I just mentioned... ← Has context!

# ✅ Correct Method 2: Capture auto-generated session ID
SESSION_ID=$(copaw message ask-agent \
  --from-agent a --to-agent b \
  --text "[Agent a requesting] What is quantum computing?" \
  2>&1 >/dev/null | grep "Using session_id:" | awk '{print $4}')

copaw message ask-agent --from-agent a --to-agent b \
  --session-id "$SESSION_ID" \
  --text "[Agent a] Can you elaborate on what you just said?"
# Response: Sure, about quantum computing I just mentioned... ← Has context!
```

**Warning**: 
- ⚠️ Be careful with concurrent requests to the same session (will cause errors)
- 💡 Recommendation: Wait for one conversation round to finish before starting next with same session
- 📋 First message must include identity, subsequent messages can be simplified (context established)
- 🔍 Use `list-sessions` to query existing session_ids instead of guessing

### 3.3 Forward Response to Channel

```bash
# Ask another agent and send result to user
copaw message ask-agent \
  --from-agent monitor \
  --to-agent analyst \
  --text "[Agent monitor requesting] Analyze recent error logs" \
  --channel dingtalk \
  --target-user manager_001 \
  --target-session alert_session
```

### 3.4 Stream Mode

```bash
# Real-time streaming output (for long responses)
copaw message ask-agent \
  --from-agent ui \
  --to-agent research \
  --text "[Agent ui requesting] Write an article about AI" \
  --mode stream
```

- `--mode final` (default): Wait for complete response
- `--mode stream`: Real-time incremental output (SSE)

### 3.5 JSON Output Format

```bash
# Get full JSON response (with metadata)
copaw message ask-agent \
  --from-agent bot_a \
  --to-agent bot_b \
  --text "[Agent bot_a requesting] Test message" \
  --json-output
```

---

## IV. Complete Workflow Examples

### Example 1: Agent Collaboration

```bash
# Step 1: Query available agents
copaw message list-agents

# Step 2: Ask expert agent (identify yourself)
copaw message ask-agent \
  --from-agent general_assistant \
  --to-agent finance_expert \
  --text "[Agent general_assistant requesting] Analyze Q1 financial report"

# Step 3: Send result to user
copaw message send \
  --agent-id general_assistant \
  --channel dingtalk \
  --target-user user_123 \
  --target-session session_456 \
  --text "Financial analysis completed: ..."
```

---

## V. Best Practices

### 5.0 Query-First Principle (Most Important!) ⭐

**Golden Rule: Always query target parameters before sending messages**

```bash
# ❌ Wrong: Directly guess parameters
copaw message send --agent-id bot --channel console \
  --target-user alice --target-session "guess_123" --text "hi"
# Result: Error, session doesn't exist

# ✅ Correct: Query → Extract → Use
# Step 1: Query
SESSIONS=$(copaw message list-sessions --agent-id bot --channel console)

# Step 2: Extract parameters
USER=$(echo "$SESSIONS" | jq -r '.sessions[0].user_id')
SESSION=$(echo "$SESSIONS" | jq -r '.sessions[0].session_id')

# Step 3: Use queried parameters
copaw message send --agent-id bot --channel console \
  --target-user "$USER" --target-session "$SESSION" --text "hi"
# Result: Success
```

**Why must query?**
- target-user and target-session are system-generated, cannot be guessed
- Only actual existing sessions can receive messages
- Query ensures parameters are valid, avoids wasting calls

**Remember:** 
1. `send` command: All 5 parameters (agent-id, channel, target-user, target-session, text) are **REQUIRED**
2. target-user and target-session **MUST** be obtained from `list-sessions` query
3. Don't guess, don't hardcode, always query first

### 5.1 Message Identity (Critical!)

**Always identify yourself in inter-agent messages** to prevent confusion:

✅ **Correct Format**:
```bash
--text "[Agent my_agent requesting] Please analyze the data"
--text "[来自智能体 my_agent] 请分析数据"
```

❌ **Wrong Format** (causes confusion):
```bash
--text "Please analyze the data"  # Target agent thinks it's from a user
```

**Identity Guidelines**:
- Use square brackets: `[Agent <agent_id> requesting]` or `[来自智能体 <agent_id>]`
- Place at message beginning for clarity
- Optional: Add purpose, e.g., `[Agent monitor - scheduled check]`

### 5.2 Concurrency-Safe Strategy

- **Default behavior**: Let system auto-generate unique session IDs
- **Explicit session**: Only use when context continuity is needed
- **Concurrency control**: Avoid sharing session IDs across concurrent calls

### 5.3 Query Before Send

**Pre-send checklist**:
1. ✅ `copaw message list-agents` - Verify target agent exists
2. ✅ `copaw message list-sessions --agent-id X` - Verify session/user exists
3. ✅ `copaw channels list` - Verify channel is configured
4. ✅ Get your agent_id from system prompt (Agent Identity section)
5. ✅ **Identify yourself in message** - Use `[Agent <your_id> requesting]` prefix
6. ✅ **Check for circular calls** - Ensure `--to-agent` is NOT the source agent

### ⛔ 5.4 Circular Call Prevention (Critical!)

**Never call back the agent that sent you the message!**

#### Core Rule

**Check message source in your context**, ensure `--to-agent` does NOT equal source agent

```
Example: You are agent_a, received message from agent_b

❌ Forbidden:
copaw message ask-agent --from-agent agent_a --to-agent agent_b
→ Results in A→B→A→B... infinite loop

✅ Allowed:
copaw message ask-agent --from-agent agent_a --to-agent agent_c
→ Call third-party, no loop

✅ Allowed:
copaw message send --agent-id agent_a --channel console --target-user ...
→ Reply to user directly, not through agent_b
```

**Key Points**:
- Message source info is in your context, verify before calling
- Alternative: call different agent or send result via channel

---

## VI. Command Quick Reference

| Command | Purpose | Example |
|---------|---------|---------|
| `copaw agents list` | List all agents | `copaw agents list` |
| `copaw message list-agents` | List all agents (same as above) | `copaw message list-agents` |
| `copaw message list-sessions` | Query sessions and users | `copaw message list-sessions --agent-id bot` |
| `copaw message send` | Send message to channel | `copaw message send --agent-id bot ...` |
| `copaw message ask-agent` | Inter-agent communication | `copaw message ask-agent --from-agent a --to-agent b --text "[Agent a requesting] ..."` |

---

## 重要提示 | Important Notes

1. **消息身份标识 | Message Identity (Critical!)**：
   - 中文：**必须**在agent间消息开头标明身份：`[来自智能体 <your_id>]`
   - English: **Must** identify yourself at message beginning: `[Agent <your_id> requesting]`
   - **原因 | Reason**: 避免目标agent混淆消息来源（agent vs user）
   - **Why**: Prevents target agent from confusing message source (agent vs user)

2. **Agent ID 来源 | Agent ID Source**：
   - 中文：从系统提示的 "Agent Identity" 部分获取（`Your agent id is ...`）
   - English: Get from system prompt "Agent Identity" section (`Your agent id is ...`)

3. **查询先行 | Query First**：
   - 中文：发送消息前**必须**先查询目标是否存在
   - English: **Always** query before sending to verify target exists

4. **并发安全 | Concurrency Safety**：
   - 中文：默认使用自动生成的唯一session，避免并发冲突
   - English: Use auto-generated unique sessions by default to avoid concurrency issues

5. **错误排查 | Troubleshooting**：
   - 中文：检查 `~/.copaw/logs/` 日志文件
   - English: Check `~/.copaw/logs/` for log files

6. **⛔ 循环调用防止 | Circular Call Prevention**：
   - 中文：**禁止**调用消息来源agent（从消息中提取来源，确保 `--to-agent` 不等于来源）
   - English: **Never** call back the source agent (extract source from message, ensure `--to-agent` ≠ source)
   - **原因 | Reason**: 防止 A→B→A 无限循环，导致资源耗尽
   - **Why**: Prevents A→B→A infinite loops that exhaust resources
