# Rust Version Fresh-Start Notice

The Rust version of QwenPaw starts with a new, empty data store. It does not
import, scan, modify, or share the Python product's data directory.

## What users should expect

- Existing chats, turns, memory, Workspace state, schedules, and settings do
  not appear in the Rust version.
- Model and provider credentials must be configured again. Credentials are not
  copied from Python files or environment snapshots. After configuration, the
  Rust Desktop stores the model API key in macOS Keychain, Windows Credential
  Manager, or Linux Secret Service rather than SQLite.
- MCP OAuth grants are also configured again and remain only in the operating
  system credential store; Rust Core never imports Python OAuth records.
- Desktop stores Rust Core state under the Tauri application data directory in
  the versioned `rust-core-v1` subdirectory.
- The old Python product and its data remain unchanged. Returning to the old
  version continues to use that old data. New Desktop bundles contain only
  Rust Core and do not provide an environment-variable fallback to Python.

Do not copy a Python database or data directory into `rust-core-v1`. The Rust
schema and ownership rules are independent. Any future manual export feature
must be designed and reviewed as a separate feature; it is not part of startup
or upgrade behavior.
