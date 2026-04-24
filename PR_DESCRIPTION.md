## Description

This PR fixes a UX bug where the `tavily_search` MCP client remains disabled after users add the `TAVILY_API_KEY` via the Settings UI.

**Root Cause:** The default factory for `tavily_search` captured `os.getenv("TAVILY_API_KEY")` at instantiation time, creating a frozen snapshot in `agent.json`. When users later added the API key through Settings → Environment Variables UI, the change was correctly saved to `envs.json` and injected into `os.environ`, but the MCP client configuration in `agent.json` was never updated.

**Solution:** Remove the dynamic environment variable coupling. The default configuration now explicitly sets:
- `enabled=False` (disabled by default, requiring explicit user opt-in)
- `env={"TAVILY_API_KEY": ""}` (empty string, user must configure via MCP settings UI)

This approach provides a clearer, more predictable workflow: users see the client is disabled, open MCP settings, add their API key, enable the client, and changes take effect immediately on next agent run.

**Related Issue:** Fixes #3703

**Security Considerations:** None. This change removes implicit environment variable behavior in favor of explicit user configuration through the UI.

## Type of Change

- [x] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation
- [ ] Refactoring

## Component(s) Affected

- [x] Core / Backend (app, agents, config, providers, utils, local_models)
- [ ] Console (frontend web UI)
- [ ] Channels (DingTalk, Feishu, QQ, Discord, iMessage, etc.)
- [ ] Skills
- [ ] CLI
- [ ] Documentation (website)
- [ ] Tests
- [ ] CI/CD
- [ ] Scripts / Deploy

## Checklist

- [x] I ran `pre-commit run --all-files` locally and it passes
- [x] If pre-commit auto-fixed files, I committed those changes and reran checks
- [ ] I ran tests locally (`pytest` or as relevant) and they pass
- [ ] Documentation updated (if needed)
- [x] Ready for review

### For Channel Changes (DingTalk, Feishu, QQ, Console, etc.)

- [ ] I ran `./scripts/check-channels.sh` (or `./scripts/check-channels.sh --changed`) and it passes
- [ ] **Contract test** exists in `tests/contract/channels/test_<channel>_contract.py` (REQUIRED)
- [ ] Contract test implements `create_instance()` with proper channel initialization
- [ ] All 19 contract verification points pass (see `tests/contract/channels/__init__.py`)
- [ ] **Optional**: Unit tests in `tests/unit/channels/test_<channel>.py` for complex internal logic

## Testing

### Manual Testing Steps

1. **Before the fix (reproduce the bug):**
   - Start QwenPaw without `TAVILY_API_KEY` set
   - Verify `tavily_search` MCP client is disabled in agent.json
   - Add `TAVILY_API_KEY` via Settings → Environment Variables UI
   - Restart agent → **Bug**: tavily_search still disabled

2. **After the fix:**
   - Fresh install: `tavily_search` is disabled by default with empty API key
   - User opens MCP settings UI
   - User fills in `TAVILY_API_KEY` field
   - User toggles the client to enabled
   - Restart agent → **Fixed**: tavily_search works immediately

3. **Verify no regression:**
   - Existing users with tavily_search already configured should continue working
   - Other MCP clients are unaffected

## Local Verification Evidence

```bash
pre-commit run --all-files
# All checks passed ✅

check python ast.........................................................Passed
mypy.....................................................................Passed
black....................................................................Passed
flake8...................................................................Passed
pylint...................................................................Passed
```

## Additional Notes

### Design Decision: Why Not Auto-Resolve from Environment?

The initial suggestion in #3703 was to add a `mode="before"` validator that re-reads `os.environ` during deserialization. However, this approach was rejected in favor of explicit user configuration because:

1. **Explicit is better than implicit**: Users should know they need to configure the API key
2. **No magic**: Configuration behavior is predictable and transparent
3. **Simpler code**: No complex validator logic trying to reconcile stored vs. live environment state
4. **Clear workflow**: The disabled state clearly signals "you need to configure this"

This follows the principle of making configuration explicit and user-driven rather than auto-magical.
