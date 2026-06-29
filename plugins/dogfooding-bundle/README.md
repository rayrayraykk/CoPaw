# Dogfooding Bundle Plugin

Internal org bundle — install once, get all bundled capabilities.

> 中文文档见 [README_zh.md](README_zh.md)

**Version:** 1.1.0 · **Minimum QwenPaw:** 1.1.7

---

## Included Capabilities

| Capability | Description |
|------------|-------------|
| **AgentScope Dogfooding Provider** | Registers `agentscope-dogfooding` LLM Provider at `http://proxy.agentscope.design/v1`, default model `qwen3.7-max-dogfooding` (Qwen3.7-Max-DogFooding) |
| **Console Feedback UI** | Shows Poor / OK / Great buttons under each AI reply; Poor opens a reason picker |
| **DingTalk Feedback Cards** | Sends actionCard rating cards after AI replies, with secondary reason collection |
| **Data Backflow** | Q&A detail + user feedback per org tracking spec; local jsonl + AgentTrack OTLP export |
| **AgentTrack Startup Hook** | Initialises AgentTrack SDK (`app_name="qwenpaw"`) at application startup |
| **Alibaba SSO Login** | Console plugin page supports corp SSO (new-tab login + paste callback code) |
| **/feedback Command** | Rewrites `/feedback` queries into agent prompts that guide the user through a feedback form |
| **Dogfooding Account API** | Exposes `POST /api/dogfooding-account/` to save the dogfooding user account (emp id) |

## Installation

> **Prerequisite**: AgentTrack SDK requires Alibaba's internal PyPI.
> Ensure network access to `artlab.alibaba-inc.com`.

```bash
# Install from zip
qwenpaw plugin install ~/Desktop/dogfooding-bundle-1.1.0.zip

# Or from local directory
qwenpaw plugin install plugins/bundle/dogfooding-bundle

# Verify
qwenpaw plugin list
qwenpaw plugin info dogfooding-bundle

# Restart the app to load plugin changes
qwenpaw app
```

> **Note:** Python plugin changes require a **restart** of `qwenpaw app` (no hot reload).
> After rebuilding `web/index.js`, hard-refresh the browser (`Cmd+Shift+R`).

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
After installing, complete Alibaba SSO on the dogfooding plugin page. The **API key is saved automatically** to the AgentScope Dogfooding provider config — no manual copy/paste in Settings → Models.

- Default model: `qwen3.7-max-dogfooding` (display name Qwen3.7-Max-DogFooding, multimodal)
- Proxy URL: `http://proxy.agentscope.design/v1` (legacy IP:8081 configs are migrated on install)

### Console Feedback

When the dogfooding model is active, each AI reply shows rating buttons:

| Button | `score_label` | `score` |
|--------|---------------|---------|
| Poor | `bad` | 1 |
| OK | `fine` | 2 |
| Great | `good` | 3 |

Choosing Poor requires selecting at least one reason. If `trace_id` is missing on a
just-streamed reply, the backend backfills it from the conversation.

### Alibaba SSO Login (Console plugin page)

Local dev uses `http://127.0.0.1` callbacks, but the SSO server only accepts `https://`
redirect URIs. The plugin uses **new-tab login + paste code**:

1. Click **Alibaba SSO Login** on the dogfooding plugin page
2. Complete SSO in the new tab
3. Paste the `code` (or full callback URL) back into the plugin page and submit
4. Emp id is saved to `dogfooding/user_account.json`; **API key is auto-written** to the AgentScope Dogfooding provider config (verify under Settings → Models)

### DingTalk Feedback

The DingTalk channel sends actionCard rating cards after AI replies; choosing Poor
triggers secondary reason collection.

### Data Backflow / Tracking

#### Local backup

All records are appended to:

```
{WORKING_DIR}/dogfooding/backflow/records.jsonl
```

`WORKING_DIR` defaults to `~/.copaw` (if present) or `~/.qwenpaw`, overridable via
`QWENPAW_WORKING_DIR`.

#### Platform export (AgentTrack)

`AgentTrack.init(app_name="qwenpaw")` runs at startup. Spans are exported via OTLP to
the EagleEye / Sunfire ingestion endpoint.

| Record type | Span name | When |
|-------------|-----------|------|
| Q&A detail | `dogfooding.qa_detail` | After each dogfooding turn completes |
| User feedback | `fu.track.interaction.feedback` | After the user submits a rating |

Feedback spans are nested under the corresponding **chat span** (linked via
`eagleeye.rpc_id`), not as orphan root nodes in the trace tree.

#### Tracking fields

**Common**

- `sam` = `idealab_talk.chat.{conversation_id}.{trace_id}`
- `trace_id` — per-turn unique ID (EagleEye format)
- `modelId` — `Qwen3.7-Max-DogFooding`
- `product_code` — `qwenpaw`

**User feedback (`fu.track.interaction.feedback`)**

- `gmkey` = `CLK`
- `logkey` = `fu.track.interaction.feedback`
- `score` / `score_label` — see table above
- `feedback_reason` / `feedback_comment`

**Q&A detail (`qa_detail`)**

- `prompt_message` / `response_message`
- `channel_type` — `console`, `web`, `dingtalk`, etc.

#### Feedback API

```bash
curl -X POST http://127.0.0.1:8088/api/dogfooding-feedback/ \
  -H 'Content-Type: application/json' \
  -d '{
    "trace_id": "7f00000117827167540001001aa7be00",
    "conversation_id": "1782710278779-tvd77qt",
    "score_label": "bad",
    "channel_type": "web",
    "feedback_reason": "incorrect result"
  }'
```

`trace_id` is optional; the backend backfills from `conversation_id` when omitted.

### AgentTrack Monitoring

No extra configuration required. Confirm via logs:

```
INFO | AgentTrack initialized (app_name=qwenpaw)
INFO | Bundle SpanProcessor registered
```

Search by `trace_id` on the AgentTrack / EagleEye platform to see chat spans with
`dogfooding.qa_detail` and `fu.track.interaction.feedback` children; `score` and
`score_label` appear in span attributes.

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

Save the current dogfooding user account under the working directory:

```bash
curl -X POST http://127.0.0.1:8088/api/dogfooding-account/ \
  -H 'Content-Type: application/json' \
  -d '{"user_account":"287738"}'
```

Writes `{WORKING_DIR}/dogfooding/user_account.json` for tracking `user_id` /
`alibaba.base.emp_id`.

## Frontend Development

After editing `src/index.tsx`:

```bash
cd plugins/bundle/dogfooding-bundle
npm install
npm run build   # outputs web/index.js
```

## File Structure

```
dogfooding-bundle/
├── plugin.json          # Plugin manifest (type: bundle)
├── plugin.py            # Entry — provider, tracking, feedback API, hooks
├── tracking.py          # Local jsonl + AgentTrack span export
├── feedback_service.py  # Feedback logic + DingTalk card payloads
├── channel_hooks.py     # DingTalk feedback card hooks
├── query_rewriter.py    # /feedback command prompt rewriting
├── requirements.txt     # Python dependencies
├── src/index.tsx        # Console frontend (feedback UI + SSO login)
├── web/index.js         # Built bundle (plugin.json entry.frontend)
├── README.md            # This file (English)
└── README_zh.md         # Chinese documentation
```

## Startup Log Example

```
INFO | Dogfooding Bundle: AgentScope Dogfooding provider registered
INFO | Dogfooding Bundle: AgentTrack startup hook registered
INFO | Dogfooding feedback runtime hook registered
INFO | Dogfooding feedback API registered at POST /api/dogfooding-feedback/
INFO | Dogfooding account API registered at POST /api/dogfooding-account/
INFO | Dogfooding Bundle fully registered
INFO | AgentTrack initialized (app_name=qwenpaw)
INFO | Bundle SpanProcessor registered
INFO | Patched finalize_console_turn_usage for QA backflow
```

## Troubleshooting

### Plugin not loaded

```bash
qwenpaw plugin list
tail -f ~/.copaw/qwenpaw.log | grep -i dogfooding
```

### Feedback buttons not visible

1. Confirm the active model is **Qwen3.7-Max-DogFooding** (AgentScope Dogfooding provider)
2. Hard-refresh the browser (`Cmd+Shift+R`)
3. Rebuild and reinstall `web/index.js`

### score / score_label missing on platform

1. Confirm `agenttrack-sdk` is installed and logs show `AgentTrack initialized`
2. Restart `qwenpaw app` before submitting feedback
3. Look for span `fu.track.interaction.feedback` in Trace details

### Feedback appears as orphan node instead of under chat

Use the latest plugin build with `eagleeye.rpc_id` linking and restart the app.

### AgentTrack init failed

1. Check `agenttrack-sdk` installation (internal PyPI required)
2. Look for `Failed to import AgentTrack SDK` in logs
3. QwenPaw still starts, but platform export and auto-instrumentation are degraded

### /feedback command not responding

1. Confirm `Patched AgentRunner.query_handler` appears in logs
2. Command is case-sensitive and must start with `/feedback`

### Model connection failed

Use `http://proxy.agentscope.design/v1`. Avoid legacy `121.43.136.192:8081`
(may be blocked on some networks).
