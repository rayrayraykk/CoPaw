# Dogfooding Bundle Plugin

Internal org bundle — install once, get all bundled capabilities.

> 中文文档见 [README_zh.md](README_zh.md)

---

## Included Capabilities

| Capability | Description |
|------------|-------------|
| **AgentScope Dogfooding Provider** | Registers the `agentscope-dogfooding` LLM Provider, routing requests through the AgentScope proxy |
| **AgentTrack Startup Hook** | Initialises AgentTrack SDK (`app_name="qwenpaw"`) at application startup |
| **/feedback Command** | Rewrites `/feedback` queries into agent prompts that guide the user through a feedback form |
| **Dogfooding Account API** | Exposes `POST /api/dogfooding-account/` to save the dogfooding user account |

## Installation

> **Prerequisite**: AgentTrack SDK requires Alibaba's internal PyPI.
> Ensure network access to `artlab.alibaba-inc.com`.

```bash
# Install from local directory
qwenpaw plugin install plugins/dogfooding-bundle

# Verify
qwenpaw plugin list
qwenpaw plugin info dogfooding-bundle
```

## Uninstallation

```bash
qwenpaw plugin uninstall dogfooding-bundle
```

## Dependencies

Dependencies in `requirements.txt` are installed automatically from
Alibaba's internal PyPI:

```
--index-url https://artlab.alibaba-inc.com/1/pypi/simple
agenttrack-sdk[agentscope]==0.9.4
harbor
wrapt<2.0.0
```

> **Why `wrapt<2.0.0`?**
> `agenttrack-sdk 0.9.4` calls `wrap_function_wrapper(module=..., name=..., wrapper=...)` using
> keyword arguments, but `wrapt 2.x` changed that parameter to positional-only, breaking all
> AgentScope / OpenAI instrumentation. Pinning to `wrapt<2.0.0` (i.e. 1.17.x) restores it.

## Usage

### AgentScope Dogfooding Provider

After installation, select **AgentScope Dogfooding** as the provider in
QwenPaw settings and enter your API key.

Default model: `qwen3.6-plus-dogfooding` (Qwen 3.6 Plus, multimodal support)

### AgentTrack Monitoring

No action required — initialised automatically on startup. Confirm via logs:

```
INFO | AgentTrack initialized (app_name=qwenpaw)
```

### /feedback Command

**Interactive mode** (no arguments):

```
User:  /feedback
Agent: Thank you for your feedback! Please rate this conversation: ...
```

**Quick mode** (with content):

```
User:  /feedback the result was wrong
Agent: Based on your description, I understand your rating is: Poor ...
```

### Dogfooding Account API

Save the current dogfooding user account under QwenPaw's working directory:

```bash
curl -X POST http://127.0.0.1:8088/api/dogfooding-account/ \
  -H 'Content-Type: application/json' \
  -d '{"user_account":"12345567"}'
```

The API writes `dogfooding/user_account.json` in the QwenPaw working directory.

## File Structure

```
dogfooding-bundle/
├── plugin.json          # Plugin manifest (type: bundle)
├── plugin.py            # Entry point — single register() for all capabilities
├── query_rewriter.py    # Prompt rewriting logic for /feedback command
├── requirements.txt     # Python dependencies
├── README.md            # This file (English)
└── README_zh.md         # Chinese documentation
```

## Startup Log Example

```
INFO | Dogfooding Bundle: AgentScope Dogfooding provider registered
INFO | Dogfooding Bundle: AgentTrack startup hook registered
INFO | Dogfooding Bundle: /feedback query-rewrite hook registered
INFO | Dogfooding account API registered at POST /api/dogfooding-account/
INFO | Dogfooding Bundle fully registered
INFO | AgentTrack initialized (app_name=qwenpaw)
INFO | Patched AgentRunner.query_handler for /feedback command
```
