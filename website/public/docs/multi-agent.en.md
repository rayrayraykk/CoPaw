# Multi-Agent

CoPaw supports **multi-agent**, allowing you to run multiple independent AI agents in a single CoPaw instance, each with its own configuration, memory, skills, and conversation history. Additionally, agents can collaborate with each other to accomplish more complex tasks.

> This feature was introduced in **v0.1.0**.

---

## Part 1: Multi-Agent Workspace

### What is Multi-Agent?

Simply put, **multi-agent** lets you run multiple "personas" in one CoPaw, where each persona:

- Has its own **personality and specialization** (configured via different persona files)
- Remembers **its own conversations** (no cross-talk)
- Uses **different skills** (one good at code, another at writing)
- Connects to **different channels** (one for DingTalk, one for Discord)

Think of it as having multiple assistants, each with their own specialty.

---

## Why Use Multi-Agent?

### Use Case 1: Functional Separation

You might need:

- A **daily assistant** - casual chat, lookup info, manage todos
- A **code assistant** - focused on code review and development
- A **writing assistant** - focused on document writing and editing

Each agent focuses on its domain without interference.

### Use Case 2: Platform Separation

You might use CoPaw across multiple platforms:

- **DingTalk** - work-related conversations
- **Discord** - community discussions
- **Console** - personal use

Different platforms' conversations and configs stay completely isolated.

### Use Case 3: Testing vs Production

You might need:

- **Production agent** - stable config for daily work
- **Test agent** - experiment with new features without affecting production

---

## How to Use? (Recommended Method)

### Managing Agents in Console

> This is the simplest way - **no command-line required**.

#### 1. View and Switch Agents

After starting CoPaw, you'll see the **Agent Selector** in the **top-right corner** of the console:

```
┌───────────────────────────────────┐
│  Current Agent  [Default ▼] (1)   │
└───────────────────────────────────┘
```

Click the dropdown to:

- View all agents' names and descriptions
- Switch to another agent
- See the current agent's ID

After switching, the page auto-refreshes to show the new agent's config and data.

#### 2. Create a New Agent

Go to **Settings → Agent Management** page:

1. Click "Create Agent" button
2. Fill in the information:
   - **Name**: Give the agent a name (e.g., "Code Assistant")
   - **Description**: Explain the agent's purpose (optional)
   - **ID**: Leave empty for auto-generation, or customize (e.g., "coder")
3. Click "OK"

After creation, the new agent appears in the list and you can immediately switch to it.

#### 3. Configure Agent-Specific Settings

After switching to an agent, you can configure it individually:

- **Channels** - Go to "Control → Channels" page to enable/configure channels
- **Skills** - Go to "Agent → Skills" page to enable/disable skills
- **Tools** - Go to "Agent → Tools" page to toggle built-in tools
- **Persona** - Go to "Agent → Workspace" page to edit AGENTS.md and SOUL.md

These settings **only affect the current agent** and won't impact other agents.

#### 4. Edit and Delete Agents

In **Settings → Agent Management** page:

- Click "Edit" button to modify agent's name and description
- Click "Delete" button to remove agent (default agent cannot be deleted)

---

## Example Scenarios

### Example 1: Work-Life Separation

**Scenario**: You want to separate work and personal conversations.

**Setup**:

1. Create two agents in console:

   - `work` - work assistant
   - `personal` - personal assistant

2. For `work` agent:

   - Enable DingTalk channel
   - Enable code and document-related skills
   - Configure formal persona (AGENTS.md)

3. For `personal` agent:
   - Enable Discord or console
   - Enable entertainment and news skills
   - Configure casual persona

**Usage**: Automatically use `work` agent on DingTalk, `personal` agent on Discord.

### Example 2: Specialized Assistant Team

**Scenario**: You want assistants for different professional domains.

**Setup**:

1. Create three agents:

   - `coder` - code assistant (enable code review, file operation skills)
   - `writer` - writing assistant (enable document processing, news digest skills)
   - `planner` - task assistant (enable cron, email skills)

2. Switch to the appropriate agent as needed.

**Benefits**: Each agent focuses on its domain with precise persona and uncluttered conversation history.

### Example 3: Multi-Language Support

**Scenario**: You need both Chinese and English assistants.

**Setup**:

1. Create two agents:

   - `zh-assistant` - Chinese assistant (language: "zh")
   - `en-assistant` - English assistant (language: "en")

2. Edit their AGENTS.md and SOUL.md in corresponding languages.

**Usage**: Switch to `zh-assistant` for Chinese conversations, `en-assistant` for English.

---

## FAQ

### Q: Do I need to create multiple agents?

Not necessarily. If your use case is simple, **using only the default agent is perfectly fine**.

Consider creating multiple agents when:

- You need clear functional separation (work/life, dev/writing, etc.)
- Connecting to multiple platforms and want isolated conversation histories
- Need to test new configs without affecting your daily-use agent

### Q: Will switching agents lose my conversations?

No. Each agent's conversation history is saved independently; switching only changes which agent you're currently viewing.

### Q: Do multiple agents increase costs?

No. Agents only call the LLM when in use; idle agents don't incur any fees.

### Q: Can I use multiple agents simultaneously?

Yes. If you configure different agents for DingTalk and Discord, they can respond to their respective channels simultaneously.

### Q: How to delete an agent?

Click the delete button in the "Settings → Agent Management" page in console.

**Note**: After deletion, the workspace directory is retained (to prevent accidental data loss). To completely remove it, manually delete the `~/.copaw/workspaces/{agent_id}` directory.

### Q: Can the default agent be deleted?

Not recommended. The `default` agent is the system's default fallback; deleting it may cause compatibility issues.

### Q: What can agents share?

**Globally Shared**:

- Model provider configuration (API keys, model selection)
- Environment variables (TAVILY_API_KEY, etc.)

**Independent Configuration**:

- Channel settings
- Skill enablement
- Conversation history
- Cron jobs
- Persona files

---

## Upgrading from Single-Agent

If you previously used CoPaw **v0.0.x**, upgrading to **v0.1.0** will **automatically migrate**:

1. **Automatic Migration on First Start**

   - Old configs and data are automatically moved to the `default` agent workspace
   - No manual file operations required

2. **Verify Migration**

   - After starting CoPaw, check the agent list in console
   - You should see an agent named "Default Agent"
   - Your old conversations and configs should still be there

3. **Backup Recommendation**
   Back up your working directory before upgrading:
   ```bash
   cp -r ~/.copaw ~/.copaw.backup
   ```

---

## Part 2: Inter-Agent Collaboration

Agents can communicate and collaborate with each other to handle complex tasks that a single agent cannot accomplish alone.

### What is Agent Collaboration?

An agent can request help from other agents when:

- It needs another agent's **specialized expertise** (e.g., code agent asks writing agent to polish documentation)
- It needs to access another agent's **workspace data** (e.g., read another agent's config files)
- It needs a **second opinion** or professional review
- The user **explicitly requests** a specific agent to participate

### How to Trigger Collaboration?

#### Method 1: User Explicitly Requests

User directly asks for another agent in the conversation:

```
User: Please ask the code assistant to review this code
```

The current agent will automatically identify and call the `code assistant` agent.

#### Method 2: Agent Initiates Proactively

When processing a task, if an agent finds it needs another agent's capabilities, it will initiate collaboration:

```
User: Generate a technical document and polish it with professional language
Agent A: [Generate draft] → [Call writing assistant to polish] → [Return final result]
```

### Collaboration Workflow

1. **Initiating agent** calls `copaw agents list` to view available agents
2. **Initiating agent** uses `copaw agents chat` to send request to target agent
3. **Target agent** processes the request and returns results
4. **Initiating agent** receives results and continues the task
5. For multi-turn exchanges, use `--session-id` to maintain conversation context

### Collaboration Examples

#### Example 1: Cross-Domain Task

```bash
# Scheduler agent needs financial data
copaw agents chat \
  --from-agent scheduler_bot \
  --to-agent finance_bot \
  --text "[Agent scheduler_bot requesting] What are the pending financial tasks for today?"
```

#### Example 2: Multi-Turn Dialogue

```bash
# Round 1: Initial request
copaw agents chat \
  --from-agent code_bot \
  --to-agent writer_bot \
  --text "[Agent code_bot requesting] Please help polish this documentation: ..."

# System returns: [SESSION: code_bot:to:writer_bot:1710912345:a1b2c3d4]

# Round 2: Follow-up (preserving context)
copaw agents chat \
  --from-agent code_bot \
  --to-agent writer_bot \
  --session-id "code_bot:to:writer_bot:1710912345:a1b2c3d4" \
  --text "[Agent code_bot requesting] Please make it more concise"
```

#### Example 3: User-Specified Agent

```bash
# User tells Agent A: "Let the code assistant help me review"
# Agent A executes:
copaw agents list  # First query available agents

copaw agents chat \
  --from-agent assistant_a \
  --to-agent code_reviewer \
  --text "[Agent assistant_a requesting] User explicitly requested your help. Please review the following code: ..."
```

### Collaboration Best Practices

#### When to Use Collaboration

✅ **Recommended**:

- Task clearly needs another agent's specialty
- Need to access another agent's workspace data
- User explicitly requests a specific agent to participate
- Need professional review or second opinion

❌ **Not Recommended**:

- Current agent can complete the task directly
- Just a simple Q&A that doesn't need specialized skills
- Insufficient information; should confirm with user first
- Avoid circular calls (don't call back the agent that just messaged you)

#### Message Format Recommendation

When communicating between agents, use this format:

```
[Agent <initiator_id> requesting] <specific request>
```

Examples:

```
[Agent scheduler_bot requesting] Please provide today's financial task list
[Agent code_bot requesting] User explicitly asked for your review. Please review the following code...
```

#### Session Management

- **New conversation**: When first contacting an agent, don't pass `--session-id`
- **Continuation**: When context is needed, must pass the `--session-id` from previous response
- **View history**: Use `copaw chats list --agent-id <your_agent>` to view all sessions

### Important Notes

- **Avoid circular calls**: If you just received a message from Agent B, don't immediately call Agent B again
- **Query before calling**: Use `copaw agents list` to confirm the target agent exists; don't guess IDs
- **Maintain session continuity**: Always pass `--session-id` for multi-turn conversations
- **Identify yourself**: Include the initiator's identity in messages to help the target agent understand the request source

---

## Advanced: CLI and API

> If you're not familiar with command-line or APIs, you can skip this section. All features are available in the console.

### CLI Commands

All multi-agent-aware CLI commands accept the `--agent-id` parameter (defaults to `default`):

```bash
# View specific agent's configuration
copaw channels list --agent-id abc123
copaw cron list --agent-id abc123
copaw skills list --agent-id abc123

# Create cron job for specific agent
copaw cron create \
  --agent-id abc123 \
  --type agent \
  --name "Check Todos" \
  --cron "0 9 * * *" \
  --channel console \
  --target-user "user1" \
  --target-session "session1" \
  --text "What are my todos?"
```

**Commands Supporting `--agent-id`**:

- `copaw channels` - channel management
- `copaw cron` - cron jobs
- `copaw daemon` - runtime status
- `copaw chats` - chat management
- `copaw skills` - skill management

**Commands NOT Supporting `--agent-id`** (global operations):

- `copaw init` - initialization
- `copaw providers` - model providers
- `copaw models` - model configuration
- `copaw env` - environment variables

### REST API

#### Agent Management API

| Endpoint                        | Method | Description     |
| ------------------------------- | ------ | --------------- |
| `/api/agents`                   | GET    | List all agents |
| `/api/agents`                   | POST   | Create agent    |
| `/api/agents/{agent_id}`        | GET    | Get agent info  |
| `/api/agents/{agent_id}`        | PUT    | Update agent    |
| `/api/agents/{agent_id}`        | DELETE | Delete agent    |
| `/api/agents/{agent_id}/active` | POST   | Activate agent  |

#### Agent-Scoped API

All agent-specific APIs support the `X-Agent-Id` HTTP header:

```bash
# Get specific agent's chat list
curl -H "X-Agent-Id: abc123" http://localhost:7860/api/chats

# Create cron job for specific agent
curl -X POST http://localhost:7860/api/cron/jobs \
  -H "X-Agent-Id: abc123" \
  -H "Content-Type: application/json" \
  -d '{ ... }'
```

API endpoints supporting `X-Agent-Id`:

- `/api/chats/*` - chat management
- `/api/cron/*` - cron jobs
- `/api/config/*` - channel and heartbeat config
- `/api/skills/*` - skill management
- `/api/tools/*` - tool management
- `/api/mcp/*` - MCP client management
- `/api/agent/*` - workspace files and memory

### Configuration File Structure

If you need to directly edit configuration files:

#### Old Structure (v0.0.x)

```
~/.copaw/
├── config.json          # All config
├── chats.json
├── jobs.json
├── AGENTS.md
└── ...
```

#### New Structure (v0.1.0+)

```
~/.copaw/
├── config.json          # Global config (providers, agents.profiles)
└── workspaces/
    ├── default/         # Default agent workspace
    │   ├── agent.json   # Agent-specific config
    │   ├── chats.json
    │   ├── jobs.json
    │   ├── AGENTS.md
    │   └── ...
    └── abc123/          # Other agent
        └── ...
```

---

## Related Pages

- [CLI Commands](./cli) - Detailed CLI reference
- [Configuration & Working Directory](./config) - Config file structure
- [Console](./console) - Web management interface
- [Skills](./skills) - Skill system
