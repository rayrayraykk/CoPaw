use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::sync::RwLock as SyncRwLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use futures_util::StreamExt;
use qwenpaw_mcp::McpAccessEffect;
use qwenpaw_mcp::McpClientSettings;
use qwenpaw_mcp::McpManager;
use qwenpaw_protocol::AgentMessageDeltaNotification;
use qwenpaw_protocol::ApprovalDecision;
use qwenpaw_protocol::ConfigReadResponse;
use qwenpaw_protocol::ConfigWriteParams;
use qwenpaw_protocol::ConfigWriteResponse;
use qwenpaw_protocol::CoreConfig;
use qwenpaw_protocol::CoreEvent;
use qwenpaw_protocol::ErrorInfo;
use qwenpaw_protocol::Item;
use qwenpaw_protocol::ItemCompletedNotification;
use qwenpaw_protocol::ItemStartedNotification;
use qwenpaw_protocol::ModelInfo;
use qwenpaw_protocol::ModelListResponse;
use qwenpaw_protocol::Thread;
use qwenpaw_protocol::ThreadArchiveParams;
use qwenpaw_protocol::ThreadArchiveResponse;
use qwenpaw_protocol::ThreadListParams;
use qwenpaw_protocol::ThreadListResponse;
use qwenpaw_protocol::ThreadReadResponse;
use qwenpaw_protocol::ThreadResumeParams;
use qwenpaw_protocol::ThreadResumeResponse;
use qwenpaw_protocol::ThreadStartParams;
use qwenpaw_protocol::ThreadStartResponse;
use qwenpaw_protocol::ThreadStatus;
use qwenpaw_protocol::ToolApprovalRequestedNotification;
use qwenpaw_protocol::ToolApprovalResolvedNotification;
use qwenpaw_protocol::ToolApprovalRespondParams;
use qwenpaw_protocol::ToolApprovalRespondResponse;
use qwenpaw_protocol::Turn;
use qwenpaw_protocol::TurnCompletedNotification;
use qwenpaw_protocol::TurnInterruptParams;
use qwenpaw_protocol::TurnInterruptResponse;
use qwenpaw_protocol::TurnStartParams;
use qwenpaw_protocol::TurnStartResponse;
use qwenpaw_protocol::TurnStartedNotification;
use qwenpaw_protocol::TurnStatus;
use qwenpaw_protocol::UserInput;
use qwenpaw_protocol::WorkspaceInfo;
use qwenpaw_protocol::WorkspaceListResponse;
use qwenpaw_protocol::WorkspaceReadResponse;
use qwenpaw_storage::StoredFunctionCall;
use qwenpaw_storage::StoredMessage;
use qwenpaw_storage::StoredModelCall;
use qwenpaw_storage::StoredThread;
use qwenpaw_storage::StoredToolCall;
use qwenpaw_storage::StoredTurnMetadata;
use qwenpaw_storage::StoredUsageRecord;
use qwenpaw_storage::ThreadStore;
use qwenpaw_storage::UsageOwner;
use qwenpaw_tools::ApprovalRequirement;
use qwenpaw_tools::ToolCall;
use qwenpaw_tools::ToolOutput;
use qwenpaw_tools::Workspace;
use serde_json::Value;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tracing::warn;
use uuid::Uuid;

use crate::model::ModelClient;
use crate::model::ModelConfig;
use crate::model::ModelConfigError;
use crate::model::ModelEvent;
use crate::model::ModelRuntime;
use crate::model::ModelUsage;
use crate::model_options::ModelRequestOptions;
use crate::security::BlockedSkillRecord;
use crate::security::SecurityApprovalMode;
use crate::security::SecurityPolicy;
use crate::security::SecuritySettings;
use crate::security::ToolGuardEffect;
use crate::security::decode_security_settings;
use crate::security::encode_security_settings;
use crate::security::trim_blocked_history;
use crate::tool_calls::ToolCallControlError;
use crate::tool_calls::ToolCallCoordinator;
use crate::tool_calls::ToolCallSnapshot;
use crate::tool_calls::ToolCallSubscription;
use crate::tool_calls::ToolCancellationReason;

#[path = "runtime_backup.rs"]
mod backup;
#[cfg(test)]
#[path = "runtime_model_options_tests.rs"]
mod model_options_tests;
#[path = "runtime_agent_publication.rs"]
mod publication;
#[path = "runtime_quiescence.rs"]
mod quiescence;
pub use backup::CoreOperationGuard;
pub use backup::CoreRestoreGuard;
pub use quiescence::CoreThreadQuiescenceGuard;

const EVENT_CHANNEL_CAPACITY: usize = 64;
const DEFAULT_LIST_LIMIT: u32 = 50;
const MAX_LIST_LIMIT: usize = 200;
const MAX_TURN_INPUT_BYTES: usize = 262_144;
const MAX_FILE_REFERENCES: usize = 32;
const MAX_FILE_REFERENCE_PATH_BYTES: usize = 4_096;
const MAX_AGENT_RESPONSE_BYTES: usize = 1_048_576;
const MAX_TOOL_CALLS_PER_STEP: usize = 16;
const MAX_TOOL_CALL_ID_BYTES: usize = 1_024;
const MAX_TOOL_NAME_BYTES: usize = 256;
const MAX_TOOL_ARGUMENT_BYTES: usize = 65_536;
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(120);
const SYSTEM_PROMPT: &str = "You are QwenPaw, a coding agent working inside the configured workspace. Use list_files and search_text to discover relevant code, then read_file before editing. Prefer replace_text for small exact edits and write_file for complete file replacement. Use shell for build or test commands. Respect denied tool calls and report only what was actually verified.";
const SYSTEM_PROMPT_FILES_SETTING: &str = "desktop_system_prompt_files";
const DEFAULT_SYSTEM_PROMPT_FILES: [&str; 3] = ["AGENTS.md", "SOUL.md", "PROFILE.md"];
const MAX_SYSTEM_PROMPT_FILES: usize = 64;
const MAX_SYSTEM_PROMPT_FILENAME_BYTES: usize = 255;
const MAX_SYSTEM_PROMPT_FILE_BYTES: u64 = 1024 * 1024;
const MAX_SYSTEM_PROMPT_BYTES: usize = 2 * 1024 * 1024;
const BASE_URL_SETTING: &str = "base_url";
const DEFAULT_MODEL_SETTING: &str = "default_model";
const PREFERRED_WORKSPACE_SETTING: &str = "preferred_workspace";
const CODING_MODE_SETTING: &str = "coding_mode";
const UI_LANGUAGE_SETTING: &str = "ui_language";
const ENVIRONMENT_KEYS_SETTING: &str = "desktop_environment_keys";
const CRON_DATA_SETTING: &str = "desktop_cron_data";
const ACCESS_CONTROL_DATA_SETTING: &str = "desktop_access_control_data";
const MAIL_ACCESS_CONTROL_DATA_SETTING: &str = "desktop_mail_access_control_data";
const INBOX_DATA_SETTING: &str = "desktop_inbox_data";
const CHAT_CATALOG_DATA_SETTING: &str = "desktop_chat_catalog_data";
const CHANNEL_CONFIG_DATA_SETTING: &str = qwenpaw_storage::CHANNEL_CONFIG_DATA_KEY;
const AGENT_SETTINGS_DATA_SETTING: &str = "desktop_agent_settings_data";
const HEARTBEAT_DATA_SETTING: &str = "desktop_heartbeat_data";
const MCP_DATA_SETTING: &str = "desktop_mcp_data";
const SECURITY_DATA_SETTING: &str = "desktop_security_data";
const BUILTIN_TOOL_OVERRIDES_SETTING: &str = "builtin_tool_overrides";
const TOOL_OFFLOAD_POLICY_SETTING: &str = "tool_offload_policy";
const DEFAULT_UI_LANGUAGE: &str = "en";
const SUPPORTED_UI_LANGUAGES: [&str; 7] = ["en", "zh", "ja", "ru", "pt-BR", "id", "vi"];
const MAX_ENVIRONMENT_VARIABLES: usize = 256;
const MAX_ENVIRONMENT_KEY_BYTES: usize = 256;
const MAX_ENVIRONMENT_VALUE_BYTES: usize = 65_536;
const MAX_CHECKPOINT_MESSAGES: usize = 10_000;

pub type TurnEventStream = mpsc::Receiver<CoreEvent>;

/// Tool approval behavior selected by the active Agent profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolApprovalLevel {
    Strict,
    Smart,
    Auto,
    Off,
}

/// Runtime settings that can be applied without restarting Rust Core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRuntimeConfig {
    pub max_agent_steps: usize,
    pub shell_timeout_ms: u64,
    pub shell_executable: String,
    pub approval_level: ToolApprovalLevel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinToolStatus {
    pub name: String,
    pub description: String,
    pub enabled: bool,
}

impl Default for AgentRuntimeConfig {
    fn default() -> Self {
        Self {
            max_agent_steps: 100,
            shell_timeout_ms: 60_000,
            shell_executable: String::new(),
            approval_level: ToolApprovalLevel::Auto,
        }
    }
}

impl AgentRuntimeConfig {
    fn validate(&self) -> Result<(), CoreError> {
        if !(1..=500).contains(&self.max_agent_steps) {
            return Err(CoreError::Config(String::from(
                "Agent max steps must be between 1 and 500",
            )));
        }
        if !(1_000..=600_000).contains(&self.shell_timeout_ms) {
            return Err(CoreError::Config(String::from(
                "Agent shell timeout must be between 1 and 600 seconds",
            )));
        }
        if self.shell_executable.len() > 4_096
            || self.shell_executable.chars().any(char::is_control)
        {
            return Err(CoreError::Config(String::from(
                "Agent shell executable is invalid",
            )));
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct Core {
    inner: Arc<CoreInner>,
}

struct CoreInner {
    operations: Arc<tokio::sync::RwLock<()>>,
    model: ModelClient,
    mcp: SyncRwLock<McpManager>,
    security: SyncRwLock<SecurityPolicy>,
    store: ThreadStore,
    state: Mutex<State>,
    runtime_environment: SyncRwLock<BTreeMap<String, String>>,
    agent_runtime_config: SyncRwLock<AgentRuntimeConfig>,
    builtin_tool_overrides: SyncRwLock<BTreeMap<String, bool>>,
    tool_calls: ToolCallCoordinator,
    system_prompt_files: SyncRwLock<Vec<String>>,
    final_persistence_failed: AtomicBool,
}

#[derive(Default)]
struct State {
    threads: HashMap<String, ThreadRecord>,
    approvals: HashMap<String, PendingApproval>,
    usage_records: Vec<StoredUsageRecord>,
}

struct ThreadRecord {
    execution: Arc<tokio::sync::RwLock<()>>,
    thread: Thread,
    turns: Vec<Turn>,
    messages: Vec<StoredMessage>,
    turn_metadata: Vec<StoredTurnMetadata>,
    active_turn: Option<ActiveTurn>,
    checkpoint: Option<StoredThread>,
    persisted_turns: HashSet<String>,
}

struct ActiveTurn {
    id: String,
    cancellation: CancellationToken,
    usage_owner: Option<UsageOwner>,
}

struct PendingApproval {
    thread_id: String,
    turn_id: String,
    sender: oneshot::Sender<ApprovalDecision>,
}

impl Core {
    /// Captures a bounded logical backup without copying a live database file.
    ///
    /// # Errors
    ///
    /// Returns an error if storage is invalid or exceeds the supplied bound.
    pub fn backup_snapshot(
        &self,
        max_bytes: u64,
    ) -> Result<qwenpaw_storage::StoreBackup, CoreError> {
        self.inner
            .store
            .backup_snapshot(max_bytes)
            .map_err(CoreError::storage)
    }

    /// Creates an ephemeral in-memory core.
    ///
    /// # Panics
    ///
    /// Panics when SQLite cannot initialize an in-memory database. Durable
    /// runtimes should use [`Self::persistent`] and handle its error.
    #[must_use]
    pub fn new(model_config: ModelConfig) -> Self {
        let mcp = McpManager::from_env()
            .unwrap_or_else(|error| panic!("MCP configuration failed: {error}"));
        Self::new_with_mcp(model_config, mcp)
    }

    /// Creates an ephemeral core with an explicitly supplied MCP manager.
    ///
    /// # Panics
    ///
    /// Panics when SQLite cannot initialize an in-memory database.
    #[must_use]
    pub fn new_with_mcp(model_config: ModelConfig, mcp: McpManager) -> Self {
        let store = ThreadStore::in_memory()
            .unwrap_or_else(|error| panic!("in-memory thread store failed: {error}"));
        Self::from_store(model_config, mcp, store)
            .unwrap_or_else(|error| panic!("in-memory core initialization failed: {error}"))
    }

    /// Opens a durable core backed by the SQLite database at `path`.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot be opened or a stored thread
    /// snapshot cannot be loaded.
    pub fn persistent(model_config: ModelConfig, path: &Path) -> Result<Self, CoreError> {
        let mcp = McpManager::from_env().map_err(CoreError::mcp)?;
        let store = ThreadStore::open(path).map_err(CoreError::storage)?;
        Self::from_store(model_config, mcp, store)
    }

    fn from_store(
        mut model_config: ModelConfig,
        mcp: McpManager,
        store: ThreadStore,
    ) -> Result<Self, CoreError> {
        if let Some(base_url) = store
            .read_setting(BASE_URL_SETTING)
            .map_err(CoreError::storage)?
        {
            model_config.base_url = base_url;
        }
        if let Some(default_model) = store
            .read_setting(DEFAULT_MODEL_SETTING)
            .map_err(CoreError::storage)?
        {
            model_config.default_model = default_model;
        }
        let model_config = model_config.normalize().map_err(CoreError::config)?;
        let offload_on_deadline = match store
            .read_setting(TOOL_OFFLOAD_POLICY_SETTING)
            .map_err(CoreError::storage)?
            .as_deref()
        {
            None | Some("keep_foreground") => false,
            Some("offload") => true,
            Some(_) => {
                return Err(CoreError::Config(String::from(
                    "stored tool offload policy is invalid",
                )));
            }
        };
        let mut builtin_tool_overrides = match store
            .read_setting(BUILTIN_TOOL_OVERRIDES_SETTING)
            .map_err(CoreError::storage)?
        {
            Some(value) => {
                serde_json::from_str::<BTreeMap<String, bool>>(&value).map_err(|_| {
                    CoreError::Config(String::from("stored built-in tool overrides are invalid"))
                })?
            }
            None => BTreeMap::new(),
        };
        builtin_tool_overrides.retain(|name, enabled| qwenpaw_tools::is_builtin(name) && !*enabled);
        let system_prompt_files = match store
            .read_setting(SYSTEM_PROMPT_FILES_SETTING)
            .map_err(CoreError::storage)?
        {
            Some(value) => {
                let files = serde_json::from_str::<Vec<String>>(&value).map_err(|_| {
                    CoreError::Config(String::from("stored system prompt files are invalid"))
                })?;
                validate_system_prompt_files(&files)?;
                files
            }
            None => default_system_prompt_files(),
        };
        let security = match store
            .read_setting(SECURITY_DATA_SETTING)
            .map_err(CoreError::storage)?
        {
            Some(value) => {
                SecurityPolicy::new(decode_security_settings(&value).map_err(CoreError::Config)?)
                    .map_err(CoreError::Config)?
            }
            None => SecurityPolicy::default(),
        };
        let snapshots = store.load_all().map_err(CoreError::storage)?;
        let usage_records = store.load_usage().map_err(CoreError::storage)?;
        let mut threads = HashMap::new();
        for mut snapshot in snapshots {
            recover_interrupted_turns(&mut snapshot);
            ensure_system_message(&mut snapshot);
            store.upsert(&snapshot).map_err(CoreError::storage)?;
            threads.insert(
                snapshot.thread.id.clone(),
                ThreadRecord::from_stored(snapshot),
            );
        }
        let environment = mcp.runtime_environment().clone();
        validate_environment(&environment)?;
        Ok(Self {
            inner: Arc::new(CoreInner {
                operations: Arc::new(tokio::sync::RwLock::new(())),
                model: ModelClient::new(model_config).map_err(|error| CoreError::model(&error))?,
                mcp: SyncRwLock::new(mcp),
                security: SyncRwLock::new(security),
                store,
                state: Mutex::new(State {
                    threads,
                    approvals: HashMap::new(),
                    usage_records,
                }),
                runtime_environment: SyncRwLock::new(environment),
                agent_runtime_config: SyncRwLock::new(AgentRuntimeConfig::default()),
                builtin_tool_overrides: SyncRwLock::new(builtin_tool_overrides),
                tool_calls: ToolCallCoordinator::new(offload_on_deadline),
                system_prompt_files: SyncRwLock::new(system_prompt_files),
                final_persistence_failed: AtomicBool::new(false),
            }),
        })
    }

    /// Creates and persists a new thread.
    ///
    /// # Errors
    ///
    /// Returns an error when the workspace is invalid or the new thread cannot
    /// be persisted.
    pub async fn start_thread(
        &self,
        params: ThreadStartParams,
    ) -> Result<ThreadStartResponse, CoreError> {
        let _operation = self.operation_guard()?;
        let workspace_path = match params.workspace_root {
            Some(path) => std::path::PathBuf::from(path),
            None => std::env::current_dir().map_err(CoreError::workspace)?,
        };
        let workspace = Workspace::open(&workspace_path).map_err(CoreError::workspace)?;
        let timestamp = now();
        let system_prompt =
            build_workspace_system_prompt(workspace.root(), &self.system_prompt_files()?);
        let thread = Thread {
            id: new_id("thr"),
            model: params
                .model
                .unwrap_or_else(|| self.inner.model.default_model()),
            workspace_root: Some(workspace.root().to_string_lossy().into_owned()),
            status: ThreadStatus::Idle,
            archived: false,
            created_at: timestamp,
            updated_at: timestamp,
        };
        let record = ThreadRecord {
            execution: Arc::default(),
            thread: thread.clone(),
            turns: Vec::new(),
            messages: vec![StoredMessage::text("system", system_prompt)],
            turn_metadata: Vec::new(),
            active_turn: None,
            checkpoint: None,
            persisted_turns: HashSet::new(),
        };
        self.inner
            .store
            .upsert(&record.snapshot())
            .map_err(CoreError::storage)?;
        self.inner
            .state
            .lock()
            .await
            .threads
            .insert(thread.id.clone(), record);
        Ok(ThreadStartResponse { thread })
    }

    pub async fn list_threads(&self, params: ThreadListParams) -> ThreadListResponse {
        let state = self.inner.state.lock().await;
        let mut threads = state
            .threads
            .values()
            .filter(|record| params.include_archived || !record.thread.archived)
            .map(|record| record.thread.clone())
            .collect::<Vec<_>>();
        threads.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| right.id.cmp(&left.id))
        });
        let offset = params
            .cursor
            .as_deref()
            .and_then(|cursor| cursor.parse::<usize>().ok())
            .unwrap_or_default();
        let limit = usize::try_from(params.limit.unwrap_or(DEFAULT_LIST_LIMIT))
            .unwrap_or(50)
            .clamp(1, MAX_LIST_LIMIT);
        let data = threads
            .iter()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect::<Vec<_>>();
        let consumed = offset.saturating_add(data.len());
        let next_cursor = (consumed < threads.len()).then(|| consumed.to_string());
        ThreadListResponse { data, next_cursor }
    }

    /// Restores an archived thread to the active thread list.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::ThreadNotFound`] when `thread_id` is unknown or a
    /// storage error when the restored snapshot cannot be persisted.
    pub async fn resume_thread(
        &self,
        params: &ThreadResumeParams,
    ) -> Result<ThreadResumeResponse, CoreError> {
        let _operation = self.operation_guard()?;
        let (thread, snapshot) = {
            let mut state = self.inner.state.lock().await;
            let record = state
                .threads
                .get_mut(&params.thread_id)
                .ok_or_else(|| CoreError::ThreadNotFound(params.thread_id.clone()))?;
            if !record.thread.archived {
                return Ok(ThreadResumeResponse {
                    thread: record.thread.clone(),
                });
            }
            record.thread.archived = false;
            record.thread.updated_at = now();
            (record.thread.clone(), record.snapshot())
        };
        self.inner
            .store
            .upsert(&snapshot)
            .map_err(CoreError::storage)?;
        Ok(ThreadResumeResponse { thread })
    }

    /// Archives an idle thread and hides it from the default thread list.
    ///
    /// # Errors
    ///
    /// Returns an error when the thread does not exist, has an active turn, or
    /// the archived snapshot cannot be persisted.
    pub async fn archive_thread(
        &self,
        params: &ThreadArchiveParams,
    ) -> Result<ThreadArchiveResponse, CoreError> {
        let _operation = self.operation_guard()?;
        let (thread, snapshot) = {
            let mut state = self.inner.state.lock().await;
            let record = state
                .threads
                .get_mut(&params.thread_id)
                .ok_or_else(|| CoreError::ThreadNotFound(params.thread_id.clone()))?;
            if record.active_turn.is_some() {
                return Err(CoreError::ThreadBusy(params.thread_id.clone()));
            }
            if record.thread.archived {
                return Ok(ThreadArchiveResponse {
                    thread: record.thread.clone(),
                });
            }
            record.thread.archived = true;
            record.thread.updated_at = now();
            (record.thread.clone(), record.snapshot())
        };
        self.inner
            .store
            .upsert(&snapshot)
            .map_err(CoreError::storage)?;
        Ok(ThreadArchiveResponse { thread })
    }

    /// Reads a thread and all turns currently held by the runtime.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::ThreadNotFound`] when `thread_id` is unknown.
    pub async fn read_thread(&self, thread_id: &str) -> Result<ThreadReadResponse, CoreError> {
        let state = self.inner.state.lock().await;
        let record = state
            .threads
            .get(thread_id)
            .ok_or_else(|| CoreError::ThreadNotFound(thread_id.to_owned()))?;
        Ok(ThreadReadResponse {
            thread: record.thread.clone(),
            turns: record.turns.clone(),
        })
    }

    /// Reads immutable user media snapshots for in-process history adapters.
    ///
    /// # Errors
    ///
    /// Returns an error when the Thread is unknown. Only authorized in-process
    /// history adapters should expose these private media snapshots.
    pub async fn read_user_inputs(
        &self,
        thread_id: &str,
    ) -> Result<Vec<qwenpaw_storage::StoredUserInput>, CoreError> {
        let state = self.inner.state.lock().await;
        let record = state
            .threads
            .get(thread_id)
            .ok_or_else(|| CoreError::ThreadNotFound(thread_id.to_owned()))?;
        Ok(record
            .messages
            .iter()
            .filter_map(|message| message.user_input.clone())
            .collect())
    }

    /// Returns immutable Thread snapshots for in-process statistics adapters.
    ///
    /// The snapshots include no credentials and preserve the same persisted
    /// data used for restart recovery.
    pub async fn statistics_snapshots(&self) -> Vec<StoredThread> {
        let state = self.inner.state.lock().await;
        let mut snapshots = state
            .threads
            .values()
            .map(ThreadRecord::snapshot)
            .collect::<Vec<_>>();
        snapshots.sort_by(|left, right| left.thread.id.cmp(&right.thread.id));
        snapshots
    }

    /// Returns the immutable global model usage ledger.
    pub async fn usage_records(&self) -> Vec<StoredUsageRecord> {
        self.inner.state.lock().await.usage_records.clone()
    }

    /// Exports the last successfully saved complete conversation boundary.
    ///
    /// During a Turn, or after its final save fails, retain the preceding
    /// boundary without discarding replies already visible to the client.
    ///
    /// # Errors
    ///
    /// Returns an error when the Thread is missing or has inconsistent active
    /// state. The returned value contains conversation state but no secrets.
    pub async fn export_thread_checkpoint(
        &self,
        thread_id: &str,
    ) -> Result<StoredThread, CoreError> {
        let state = self.inner.state.lock().await;
        let record = state
            .threads
            .get(thread_id)
            .ok_or_else(|| CoreError::ThreadNotFound(thread_id.to_owned()))?;
        if let Some(checkpoint) = &record.checkpoint {
            return Ok(checkpoint.clone());
        }
        if record.thread.status == ThreadStatus::Active {
            return Err(CoreError::ThreadBusy(thread_id.to_owned()));
        }
        Ok(record.snapshot())
    }

    /// Reports a successful final Turn write observed by this runtime.
    ///
    /// Missing, active, failed-save, loaded, and restored Turns have no receipt.
    /// A later save cannot create a receipt for an earlier failed write. This
    /// reports Store success, not a guarantee against power loss.
    pub async fn turn_was_persisted(&self, thread_id: &str, turn_id: &str) -> bool {
        self.inner
            .state
            .lock()
            .await
            .threads
            .get(thread_id)
            .is_some_and(|record| record.persisted_turns.contains(turn_id))
    }

    /// Checks final-write failures observed during this Core instance's lifetime.
    ///
    /// Hosts must stop admission and drain their producers before using this
    /// as an exit check. Later writes or restores cannot acknowledge an earlier
    /// failed final write. This does not wait for active work or retry storage.
    ///
    /// # Errors
    ///
    /// Returns a sanitized storage error if any final Turn write failed.
    pub fn check_final_persistence(&self) -> Result<(), CoreError> {
        if self.inner.final_persistence_failed.load(Ordering::Relaxed) {
            return Err(CoreError::Storage(String::from(
                "one or more final turn writes failed in this Core instance",
            )));
        }
        Ok(())
    }

    /// Validates conversation content before staging an imported checkpoint.
    /// The caller separately validates the target Thread and Workspace identity.
    ///
    /// # Errors
    ///
    /// Rejects foreign/in-progress Turns or an excessive model-message count.
    pub fn validate_thread_checkpoint(checkpoint: &StoredThread) -> Result<(), CoreError> {
        if checkpoint.turns.iter().any(|turn| {
            turn.thread_id != checkpoint.thread.id || turn.status == TurnStatus::InProgress
        }) {
            return Err(CoreError::Checkpoint(String::from(
                "checkpoint contains invalid Turn state",
            )));
        }
        if checkpoint.messages.len() > MAX_CHECKPOINT_MESSAGES {
            return Err(CoreError::Checkpoint(String::from(
                "checkpoint contains too many model messages",
            )));
        }
        Ok(())
    }

    /// Replaces an idle Thread's conversation with an exported checkpoint.
    ///
    /// Runtime identity, model, Workspace, and archive state remain owned by
    /// the current Thread. Only Turns and model conversation messages are
    /// restored, so a checkpoint cannot move a Thread across Workspaces.
    ///
    /// # Errors
    ///
    /// Returns an error for a missing or active Thread, a mismatched snapshot,
    /// or a durable storage failure.
    pub async fn restore_thread_checkpoint(
        &self,
        thread_id: &str,
        mut checkpoint: StoredThread,
    ) -> Result<ThreadReadResponse, CoreError> {
        let _operation = self.operation_guard()?;
        let mut state = self.inner.state.lock().await;
        let current = state
            .threads
            .get(thread_id)
            .ok_or_else(|| CoreError::ThreadNotFound(thread_id.to_owned()))?;
        if current.active_turn.is_some() || current.thread.status == ThreadStatus::Active {
            return Err(CoreError::ThreadBusy(thread_id.to_owned()));
        }
        if checkpoint.thread.id != thread_id {
            return Err(CoreError::Checkpoint(String::from(
                "checkpoint Thread identity does not match",
            )));
        }
        if checkpoint.thread.workspace_root != current.thread.workspace_root {
            return Err(CoreError::Checkpoint(String::from(
                "checkpoint Workspace does not match the current Thread",
            )));
        }
        Self::validate_thread_checkpoint(&checkpoint)?;
        ensure_system_message(&mut checkpoint);
        let mut thread = current.thread.clone();
        thread.status = ThreadStatus::Idle;
        thread.updated_at = now();
        let replacement = ThreadRecord {
            execution: current.execution.clone(),
            thread: thread.clone(),
            turns: checkpoint.turns,
            messages: checkpoint.messages,
            turn_metadata: checkpoint.turn_metadata,
            active_turn: None,
            checkpoint: None,
            persisted_turns: HashSet::new(),
        };
        self.inner
            .store
            .upsert(&replacement.snapshot())
            .map_err(CoreError::storage)?;
        let response = ThreadReadResponse {
            thread,
            turns: replacement.turns.clone(),
        };
        state.threads.insert(thread_id.to_owned(), replacement);
        Ok(response)
    }

    /// Deletes an idle thread from runtime state and durable storage.
    ///
    /// # Errors
    ///
    /// Returns an error when the thread is missing, active, or cannot be
    /// removed from storage.
    pub async fn delete_thread(&self, thread_id: &str) -> Result<Thread, CoreError> {
        let _operation = self.operation_guard()?;
        let mut state = self.inner.state.lock().await;
        let thread = state
            .threads
            .get(thread_id)
            .ok_or_else(|| CoreError::ThreadNotFound(thread_id.to_owned()))?
            .thread
            .clone();
        if thread.status == ThreadStatus::Active
            || state
                .threads
                .get(thread_id)
                .is_some_and(|record| record.active_turn.is_some())
        {
            return Err(CoreError::ThreadBusy(thread_id.to_owned()));
        }
        self.inner
            .store
            .delete(thread_id)
            .map_err(CoreError::storage)?;
        state.threads.remove(thread_id);
        state
            .approvals
            .retain(|_, approval| approval.thread_id != thread_id);
        Ok(thread)
    }

    /// Rebinds an idle Thread to an explicitly selected Workspace directory.
    ///
    /// # Errors
    ///
    /// Returns an error when the Workspace is invalid, the Thread is missing,
    /// archived, active, or the updated snapshot cannot be persisted.
    pub async fn set_thread_workspace(
        &self,
        thread_id: &str,
        workspace_root: &Path,
    ) -> Result<Thread, CoreError> {
        let _operation = self.operation_guard()?;
        let workspace = Workspace::open(workspace_root).map_err(CoreError::workspace)?;
        let workspace_root = workspace.root().to_string_lossy().into_owned();
        let (thread, snapshot) = {
            let mut state = self.inner.state.lock().await;
            let record = state
                .threads
                .get_mut(thread_id)
                .ok_or_else(|| CoreError::ThreadNotFound(thread_id.to_owned()))?;
            if record.thread.archived {
                return Err(CoreError::ThreadArchived(thread_id.to_owned()));
            }
            if record.active_turn.is_some() {
                return Err(CoreError::ThreadBusy(thread_id.to_owned()));
            }
            if record.thread.workspace_root.as_deref() == Some(&workspace_root) {
                return Ok(record.thread.clone());
            }
            record.thread.workspace_root = Some(workspace_root);
            record.thread.updated_at = now();
            (record.thread.clone(), record.snapshot())
        };
        self.inner
            .store
            .upsert(&snapshot)
            .map_err(CoreError::storage)?;
        Ok(thread)
    }

    #[must_use]
    pub fn read_config(&self) -> ConfigReadResponse {
        ConfigReadResponse {
            config: protocol_config(&self.inner.model.config_snapshot()),
        }
    }

    /// Captures model identity and its effective credential together for an
    /// explicitly authorized application backup or restore. Unlike `read_config`,
    /// this may contain a secret and must never be exposed by configuration APIs.
    #[must_use]
    pub fn backup_model_config(&self) -> ModelConfig {
        self.inner.model.config_snapshot()
    }

    /// Validates, persists, and applies non-secret model configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when a value is invalid or SQLite persistence fails.
    pub fn write_config(
        &self,
        params: ConfigWriteParams,
    ) -> Result<ConfigWriteResponse, CoreError> {
        let _operation = self.operation_guard()?;
        let mut runtime = self.inner.model.write_runtime();
        let current = runtime.config.clone();
        let previous_base = current.base_url.clone();
        let next = ModelConfig {
            api_key: current.api_key,
            base_url: params.base_url.unwrap_or(current.base_url),
            default_model: params.default_model.unwrap_or(current.default_model),
        }
        .normalize()
        .map_err(CoreError::config)?;
        self.inner
            .store
            .write_settings(&[
                (BASE_URL_SETTING, next.base_url.as_str()),
                (DEFAULT_MODEL_SETTING, next.default_model.as_str()),
            ])
            .map_err(CoreError::storage)?;
        if next.base_url != previous_base {
            runtime.options = ModelRequestOptions::default();
        }
        runtime.config = next.clone();
        Ok(ConfigWriteResponse {
            config: protocol_config(&next),
        })
    }

    /// Replaces the process-only model API key without persisting it to SQLite.
    ///
    /// # Errors
    ///
    /// Returns an error when the API key violates the bounded secret format.
    pub fn set_runtime_api_key(&self, api_key: Option<String>) -> Result<(), CoreError> {
        let _operation = self.operation_guard()?;
        let mut runtime = self.inner.model.write_runtime();
        let current = runtime.config.clone();
        let next = ModelConfig {
            api_key,
            base_url: current.base_url,
            default_model: current.default_model,
        }
        .normalize()
        .map_err(CoreError::config)?;
        runtime.config = next;
        Ok(())
    }

    /// Applies provider identity, credentials and request options atomically.
    /// Only the URL and default model are persisted here. The application owns
    /// durable provider settings; headers and API keys stay out of Core SQLite.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid options, restore contention or persistence
    /// failure. None of the live model settings change on failure.
    pub fn configure_model_runtime(
        &self,
        config: ModelConfig,
        options: ModelRequestOptions,
    ) -> Result<(), CoreError> {
        let _operation = self.operation_guard()?;
        let config = config.normalize().map_err(CoreError::config)?;
        options.validate().map_err(CoreError::config)?;
        let mut runtime = self.inner.model.write_runtime();
        self.inner
            .store
            .write_settings(&[
                (BASE_URL_SETTING, config.base_url.as_str()),
                (DEFAULT_MODEL_SETTING, config.default_model.as_str()),
            ])
            .map_err(CoreError::storage)?;
        *runtime = ModelRuntime { config, options };
        Ok(())
    }

    /// Reads the non-secret preferred Workspace used by Desktop clients.
    ///
    /// # Errors
    ///
    /// Returns an error when the persisted setting cannot be read.
    pub fn read_preferred_workspace(&self) -> Result<Option<String>, CoreError> {
        self.inner
            .store
            .read_setting(PREFERRED_WORKSPACE_SETTING)
            .map_err(CoreError::storage)
    }

    /// Validates and persists the preferred Workspace used by Desktop clients.
    ///
    /// # Errors
    ///
    /// Returns an error when the directory is invalid or persistence fails.
    pub fn write_preferred_workspace(&self, root: &Path) -> Result<String, CoreError> {
        let _operation = self.operation_guard()?;
        let workspace = Workspace::open(root).map_err(CoreError::workspace)?;
        let root = workspace.root().to_string_lossy().into_owned();
        self.inner
            .store
            .write_settings(&[(PREFERRED_WORKSPACE_SETTING, &root)])
            .map_err(CoreError::storage)?;
        Ok(root)
    }

    /// Reads whether Desktop Coding Mode is enabled for the built-in agent.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot be read or the stored value is
    /// invalid.
    pub fn read_coding_mode(&self) -> Result<bool, CoreError> {
        match self
            .inner
            .store
            .read_setting(CODING_MODE_SETTING)
            .map_err(CoreError::storage)?
            .as_deref()
        {
            None | Some("false") => Ok(false),
            Some("true") => Ok(true),
            Some(_) => Err(CoreError::Config(String::from(
                "stored Coding Mode setting is invalid",
            ))),
        }
    }

    /// Persists whether Desktop Coding Mode is enabled for the built-in agent.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite persistence fails.
    pub fn write_coding_mode(&self, enabled: bool) -> Result<bool, CoreError> {
        let _operation = self.operation_guard()?;
        let value = if enabled { "true" } else { "false" };
        self.inner
            .store
            .write_settings(&[(CODING_MODE_SETTING, value)])
            .map_err(CoreError::storage)?;
        Ok(enabled)
    }

    /// Reads the global language preference used by Desktop/WebUI clients.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot be read or the stored value is not
    /// supported by the unchanged Console.
    pub fn read_ui_language(&self) -> Result<String, CoreError> {
        let language = self
            .inner
            .store
            .read_setting(UI_LANGUAGE_SETTING)
            .map_err(CoreError::storage)?
            .unwrap_or_else(|| String::from(DEFAULT_UI_LANGUAGE));
        validate_ui_language(&language)?;
        Ok(language)
    }

    /// Validates and persists the global Desktop/WebUI language preference.
    ///
    /// # Errors
    ///
    /// Returns an error when the language is unsupported or SQLite
    /// persistence fails.
    pub fn write_ui_language(&self, language: &str) -> Result<String, CoreError> {
        let _operation = self.operation_guard()?;
        let language = language.trim();
        validate_ui_language(language)?;
        self.inner
            .store
            .write_settings(&[(UI_LANGUAGE_SETTING, language)])
            .map_err(CoreError::storage)?;
        Ok(language.to_owned())
    }

    /// Reads the names of Desktop environment variables stored in the secure
    /// platform credential store.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite or the stored key list is invalid.
    pub fn read_environment_keys(&self) -> Result<Vec<String>, CoreError> {
        let Some(serialized) = self
            .inner
            .store
            .read_setting(ENVIRONMENT_KEYS_SETTING)
            .map_err(CoreError::storage)?
        else {
            return Ok(Vec::new());
        };
        let keys = serde_json::from_str::<Vec<String>>(&serialized).map_err(|_| {
            CoreError::Config(String::from("stored environment key list is invalid"))
        })?;
        validate_environment_keys(&keys)?;
        Ok(keys)
    }

    /// Persists the non-secret names of Desktop environment variables.
    ///
    /// # Errors
    ///
    /// Returns an error when a name is invalid or SQLite cannot be written.
    pub fn write_environment_keys(&self, keys: &[String]) -> Result<Vec<String>, CoreError> {
        let _operation = self.operation_guard()?;
        let mut keys = keys.to_vec();
        keys.sort();
        keys.dedup();
        validate_environment_keys(&keys)?;
        let serialized =
            serde_json::to_string(&keys).map_err(|error| CoreError::Config(error.to_string()))?;
        self.inner
            .store
            .write_settings(&[(ENVIRONMENT_KEYS_SETTING, &serialized)])
            .map_err(CoreError::storage)?;
        Ok(keys)
    }

    /// Reads the serialized Desktop cron configuration and runtime metadata.
    ///
    /// # Errors
    ///
    /// Returns an error when the Core settings store cannot be read.
    pub fn read_cron_data(&self) -> Result<Option<String>, CoreError> {
        self.inner
            .store
            .read_setting(CRON_DATA_SETTING)
            .map_err(CoreError::storage)
    }

    /// Atomically persists serialized Desktop cron configuration and metadata.
    ///
    /// # Errors
    ///
    /// Returns an error when the Core settings store cannot be written.
    pub fn write_cron_data(&self, value: &str) -> Result<(), CoreError> {
        let _operation = self.operation_guard()?;
        self.inner
            .store
            .write_settings(&[(CRON_DATA_SETTING, value)])
            .map_err(CoreError::storage)
    }

    /// Reads the serialized Desktop channel access-control configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when the Core settings store cannot be read.
    pub fn read_access_control_data(&self) -> Result<Option<String>, CoreError> {
        self.inner
            .store
            .read_setting(ACCESS_CONTROL_DATA_SETTING)
            .map_err(CoreError::storage)
    }

    /// Atomically persists serialized Desktop channel access-control data.
    ///
    /// # Errors
    ///
    /// Returns an error when the Core settings store cannot be written.
    pub fn write_access_control_data(&self, value: &str) -> Result<(), CoreError> {
        let _operation = self.operation_guard()?;
        self.inner
            .store
            .write_settings(&[(ACCESS_CONTROL_DATA_SETTING, value)])
            .map_err(CoreError::storage)
    }

    /// Reads the serialized Desktop mail access-control data.
    ///
    /// # Errors
    ///
    /// Returns an error when the Core settings store cannot be read.
    pub fn read_mail_access_control_data(&self) -> Result<Option<String>, CoreError> {
        self.inner
            .store
            .read_setting(MAIL_ACCESS_CONTROL_DATA_SETTING)
            .map_err(CoreError::storage)
    }

    /// Atomically persists serialized Desktop mail access-control data.
    ///
    /// # Errors
    ///
    /// Returns an error when the Core settings store cannot be written.
    pub fn write_mail_access_control_data(&self, value: &str) -> Result<(), CoreError> {
        let _operation = self.operation_guard()?;
        self.inner
            .store
            .write_settings(&[(MAIL_ACCESS_CONTROL_DATA_SETTING, value)])
            .map_err(CoreError::storage)
    }

    /// Reads the serialized Desktop Inbox events and execution traces.
    ///
    /// # Errors
    ///
    /// Returns an error when the Core settings store cannot be read.
    pub fn read_inbox_data(&self) -> Result<Option<String>, CoreError> {
        self.inner
            .store
            .read_setting(INBOX_DATA_SETTING)
            .map_err(CoreError::storage)
    }

    /// Atomically persists serialized Desktop Inbox events and traces.
    ///
    /// # Errors
    ///
    /// Returns an error when the Core settings store cannot be written.
    pub fn write_inbox_data(&self, value: &str) -> Result<(), CoreError> {
        let _operation = self.operation_guard()?;
        self.inner
            .store
            .write_settings(&[(INBOX_DATA_SETTING, value)])
            .map_err(CoreError::storage)
    }

    /// Reads serialized Desktop chat catalog metadata.
    ///
    /// # Errors
    ///
    /// Returns an error when the Core settings store cannot be read.
    pub fn read_chat_catalog_data(&self) -> Result<Option<String>, CoreError> {
        self.inner
            .store
            .read_setting(CHAT_CATALOG_DATA_SETTING)
            .map_err(CoreError::storage)
    }

    /// Atomically persists serialized Desktop chat catalog metadata.
    ///
    /// # Errors
    ///
    /// Returns an error when the Core settings store cannot be written.
    pub fn write_chat_catalog_data(&self, value: &str) -> Result<(), CoreError> {
        let _operation = self.operation_guard()?;
        self.inner
            .store
            .write_settings(&[(CHAT_CATALOG_DATA_SETTING, value)])
            .map_err(CoreError::storage)
    }

    /// Reads the serialized Desktop channel configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when the Core settings store cannot be read.
    pub fn read_channel_config_data(&self) -> Result<Option<String>, CoreError> {
        self.inner
            .store
            .read_setting(CHANNEL_CONFIG_DATA_SETTING)
            .map_err(CoreError::storage)
    }

    /// Atomically persists the serialized Desktop channel configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when the Core settings store cannot be written.
    pub fn write_channel_config_data(&self, value: &str) -> Result<(), CoreError> {
        let _operation = self.operation_guard()?;
        self.inner
            .store
            .write_settings(&[(CHANNEL_CONFIG_DATA_SETTING, value)])
            .map_err(CoreError::storage)
    }

    /// Reads the serialized non-secret Desktop Agent settings.
    ///
    /// # Errors
    ///
    /// Returns an error when the Core settings store cannot be read.
    pub fn read_agent_settings_data(&self) -> Result<Option<String>, CoreError> {
        self.inner
            .store
            .read_setting(AGENT_SETTINGS_DATA_SETTING)
            .map_err(CoreError::storage)
    }

    /// Atomically persists serialized non-secret Desktop Agent settings.
    ///
    /// # Errors
    ///
    /// Returns an error when the Core settings store cannot be written.
    pub fn write_agent_settings_data(&self, value: &str) -> Result<(), CoreError> {
        let _operation = self.operation_guard()?;
        self.inner
            .store
            .write_settings(&[(AGENT_SETTINGS_DATA_SETTING, value)])
            .map_err(CoreError::storage)
    }

    /// Reads the serialized Desktop Heartbeat configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when the Core settings store cannot be read.
    pub fn read_heartbeat_data(&self) -> Result<Option<String>, CoreError> {
        self.inner
            .store
            .read_setting(HEARTBEAT_DATA_SETTING)
            .map_err(CoreError::storage)
    }

    /// Atomically persists the serialized Desktop Heartbeat configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when the Core settings store cannot be written.
    pub fn write_heartbeat_data(&self, value: &str) -> Result<(), CoreError> {
        let _operation = self.operation_guard()?;
        self.inner
            .store
            .write_settings(&[(HEARTBEAT_DATA_SETTING, value)])
            .map_err(CoreError::storage)
    }

    /// Reads serialized non-secret Desktop MCP configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when the Core settings store cannot be read.
    pub fn read_mcp_data(&self) -> Result<Option<String>, CoreError> {
        self.inner
            .store
            .read_setting(MCP_DATA_SETTING)
            .map_err(CoreError::storage)
    }

    /// Atomically persists serialized non-secret Desktop MCP configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when the Core settings store cannot be written.
    pub fn write_mcp_data(&self, value: &str) -> Result<(), CoreError> {
        let _operation = self.operation_guard()?;
        self.inner
            .store
            .write_settings(&[(MCP_DATA_SETTING, value)])
            .map_err(CoreError::storage)
    }

    /// Returns a snapshot of the durable Security configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when the runtime lock is poisoned.
    pub fn security_settings(&self) -> Result<SecuritySettings, CoreError> {
        self.inner
            .security
            .read()
            .map(|policy| policy.settings().clone())
            .map_err(|_| CoreError::Config(String::from("Security runtime lock is poisoned")))
    }

    /// Encodes effective Security settings in the versioned persistence format.
    /// This includes defaults that have not yet been written to the database.
    ///
    /// # Errors
    ///
    /// Returns an error when settings cannot be read or encoded.
    pub fn backup_security_data(&self) -> Result<String, CoreError> {
        encode_security_settings(&self.security_settings()?).map_err(CoreError::Config)
    }

    /// Validates, persists, and hot-reloads a complete Security configuration.
    /// Active turns retain the immutable policy snapshot captured at start.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid rules, persistence failure, or a poisoned
    /// runtime lock.
    pub fn replace_security_settings(
        &self,
        mut settings: SecuritySettings,
    ) -> Result<SecuritySettings, CoreError> {
        let _operation = self.operation_guard()?;
        settings.allow_no_auth_hosts =
            crate::security::normalize_ip_hosts(&settings.allow_no_auth_hosts)
                .map_err(CoreError::Config)?;
        trim_blocked_history(&mut settings);
        let policy = SecurityPolicy::new(settings.clone()).map_err(CoreError::Config)?;
        let serialized = encode_security_settings(&settings).map_err(CoreError::Config)?;
        let mut active =
            self.inner.security.write().map_err(|_| {
                CoreError::Config(String::from("Security runtime lock is poisoned"))
            })?;
        self.inner
            .store
            .write_settings(&[(SECURITY_DATA_SETTING, &serialized)])
            .map_err(CoreError::storage)?;
        *active = policy;
        Ok(settings)
    }

    /// Appends a real blocked Skill scan result to durable bounded history.
    ///
    /// # Errors
    ///
    /// Returns an error when Security state cannot be read or persisted.
    pub fn record_blocked_skill(&self, record: BlockedSkillRecord) -> Result<(), CoreError> {
        let _operation = self.operation_guard()?;
        let mut settings = self.security_settings()?;
        settings.blocked_skill_history.push(record);
        self.replace_security_settings(settings).map(|_| ())
    }

    /// Returns configured and effective shell sandbox status.
    ///
    /// A proposed value can be supplied for Console preview without changing
    /// durable state.
    ///
    /// # Errors
    ///
    /// Returns an error when Security state cannot be read.
    pub fn sandbox_status(
        &self,
        proposed: Option<bool>,
    ) -> Result<(bool, bool, Option<String>), CoreError> {
        let enabled = proposed.unwrap_or(self.security_settings()?.sandbox_enabled);
        if !enabled {
            return Ok((false, false, None));
        }
        let effective = qwenpaw_tools::shell_sandbox_available();
        Ok((
            true,
            effective,
            (!effective).then(|| String::from("unsupported")),
        ))
    }

    /// Returns the ordered Markdown files used to compose the Agent system prompt.
    ///
    /// # Errors
    ///
    /// Returns an error when the runtime lock is poisoned.
    pub fn system_prompt_files(&self) -> Result<Vec<String>, CoreError> {
        self.inner
            .system_prompt_files
            .read()
            .map(|files| files.clone())
            .map_err(|_| CoreError::Config(String::from("system prompt file lock is poisoned")))
    }

    /// Validates, persists, and hot-reloads the ordered system prompt file list.
    ///
    /// Existing threads use the new list at the start of their next turn.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid filenames, serialization, persistence, or
    /// a poisoned runtime lock.
    pub fn replace_system_prompt_files(&self, files: Vec<String>) -> Result<(), CoreError> {
        let _operation = self.operation_guard()?;
        validate_system_prompt_files(&files)?;
        let serialized = serde_json::to_string(&files).map_err(CoreError::storage)?;
        self.inner
            .store
            .write_settings(&[(SYSTEM_PROMPT_FILES_SETTING, &serialized)])
            .map_err(CoreError::storage)?;
        *self.inner.system_prompt_files.write().map_err(|_| {
            CoreError::Config(String::from("system prompt file lock is poisoned"))
        })? = files;
        Ok(())
    }

    /// Returns the settings used by future Agent turns.
    ///
    /// # Errors
    ///
    /// Returns an error when the runtime settings lock is unavailable.
    pub fn agent_runtime_config(&self) -> Result<AgentRuntimeConfig, CoreError> {
        self.inner
            .agent_runtime_config
            .read()
            .map(|config| config.clone())
            .map_err(|_| CoreError::Config(String::from("Agent runtime settings lock failed")))
    }

    /// Replaces settings used by future Agent turns without restarting Core.
    ///
    /// # Errors
    ///
    /// Returns an error when a value is unsafe or the runtime lock is
    /// unavailable.
    pub fn replace_agent_runtime_config(
        &self,
        config: AgentRuntimeConfig,
    ) -> Result<(), CoreError> {
        let _operation = self.operation_guard()?;
        config.validate()?;
        *self
            .inner
            .agent_runtime_config
            .write()
            .map_err(|_| CoreError::Config(String::from("Agent runtime settings lock failed")))? =
            config;
        Ok(())
    }

    /// Returns the Rust built-in tools and their effective enabled states.
    ///
    /// # Errors
    ///
    /// Returns an error when the runtime state lock is poisoned.
    pub fn builtin_tools(&self) -> Result<Vec<BuiltinToolStatus>, CoreError> {
        let overrides = self.builtin_tool_overrides()?;
        Ok(qwenpaw_tools::builtin_metadata()
            .into_iter()
            .map(|tool| BuiltinToolStatus {
                enabled: overrides.get(&tool.name).copied().unwrap_or(true),
                name: tool.name,
                description: tool.description,
            })
            .collect())
    }

    /// Atomically toggles and persists one Rust built-in tool.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown tool, failed persistence, or poisoned
    /// runtime state.
    pub fn toggle_builtin_tool(&self, tool_name: &str) -> Result<BuiltinToolStatus, CoreError> {
        let _operation = self.operation_guard()?;
        let metadata = qwenpaw_tools::builtin_metadata()
            .into_iter()
            .find(|tool| tool.name == tool_name)
            .ok_or_else(|| CoreError::Config(format!("unknown built-in tool: {tool_name}")))?;
        let mut overrides =
            self.inner.builtin_tool_overrides.write().map_err(|_| {
                CoreError::Config(String::from("built-in tool state lock is poisoned"))
            })?;
        let enabled = !overrides.get(tool_name).copied().unwrap_or(true);
        persist_builtin_tool_override(&self.inner.store, &mut overrides, tool_name, enabled)?;
        Ok(BuiltinToolStatus {
            name: metadata.name,
            description: metadata.description,
            enabled,
        })
    }

    /// Atomically persists one Rust built-in tool's enabled state.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown tool, failed persistence, or poisoned
    /// runtime state.
    pub fn set_builtin_tool_enabled(
        &self,
        tool_name: &str,
        enabled: bool,
    ) -> Result<BuiltinToolStatus, CoreError> {
        let _operation = self.operation_guard()?;
        let metadata = qwenpaw_tools::builtin_metadata()
            .into_iter()
            .find(|tool| tool.name == tool_name)
            .ok_or_else(|| CoreError::Config(format!("unknown built-in tool: {tool_name}")))?;
        let mut overrides =
            self.inner.builtin_tool_overrides.write().map_err(|_| {
                CoreError::Config(String::from("built-in tool state lock is poisoned"))
            })?;
        persist_builtin_tool_override(&self.inner.store, &mut overrides, tool_name, enabled)?;
        Ok(BuiltinToolStatus {
            name: metadata.name,
            description: metadata.description,
            enabled,
        })
    }

    fn builtin_tool_overrides(&self) -> Result<BTreeMap<String, bool>, CoreError> {
        self.inner
            .builtin_tool_overrides
            .read()
            .map(|overrides| overrides.clone())
            .map_err(|_| CoreError::Config(String::from("built-in tool state lock is poisoned")))
    }

    fn builtin_tool_enabled(&self, tool_name: &str) -> Result<bool, CoreError> {
        Ok(self
            .builtin_tool_overrides()?
            .get(tool_name)
            .copied()
            .unwrap_or(true))
    }

    fn enabled_builtin_tool_definitions(&self) -> Result<Vec<Value>, CoreError> {
        let overrides = self.builtin_tool_overrides()?;
        Ok(qwenpaw_tools::definitions()
            .into_iter()
            .filter(|definition| {
                qwenpaw_tools::definition_name(definition)
                    .is_some_and(|name| overrides.get(name).copied().unwrap_or(true))
            })
            .collect())
    }

    #[must_use]
    pub fn tool_offload_policy(&self) -> String {
        if self.inner.tool_calls.offload_on_deadline() {
            String::from("offload")
        } else {
            String::from("keep_foreground")
        }
    }

    /// Persists and immediately applies the default long-running tool policy.
    ///
    /// # Errors
    ///
    /// Returns an error for an unsupported policy or failed persistence.
    pub fn set_tool_offload_policy(&self, policy: &str) -> Result<String, CoreError> {
        let _operation = self.operation_guard()?;
        let enabled = match policy {
            "keep_foreground" => false,
            "offload" => true,
            _ => {
                return Err(CoreError::Config(format!(
                    "unsupported tool offload policy: {policy}"
                )));
            }
        };
        self.inner
            .store
            .write_settings(&[(TOOL_OFFLOAD_POLICY_SETTING, policy)])
            .map_err(CoreError::storage)?;
        self.inner.tool_calls.set_offload_on_deadline(enabled);
        Ok(policy.to_owned())
    }

    pub async fn list_tool_calls(&self, thread_id: &str) -> Vec<ToolCallSnapshot> {
        self.inner.tool_calls.list(thread_id).await
    }

    /// Returns one session-scoped tool call, including recently completed calls.
    ///
    /// # Errors
    ///
    /// Returns `NotFound` when the call does not belong to the supplied Thread.
    pub async fn tool_call(
        &self,
        thread_id: &str,
        tool_call_id: &str,
    ) -> Result<ToolCallSnapshot, ToolCallControlError> {
        self.inner.tool_calls.get(thread_id, tool_call_id).await
    }

    /// Subscribes to final output for an active or recently completed tool call.
    ///
    /// # Errors
    ///
    /// Returns `NotFound` when the call does not belong to the supplied Thread.
    pub async fn subscribe_tool_call(
        &self,
        thread_id: &str,
        tool_call_id: &str,
    ) -> Result<ToolCallSubscription, ToolCallControlError> {
        self.inner
            .tool_calls
            .subscribe(thread_id, tool_call_id)
            .await
    }

    /// Moves one bounded active tool execution into the background.
    ///
    /// # Errors
    ///
    /// Returns an error for a missing call or an invalid lifecycle transition.
    pub async fn offload_tool_call(
        &self,
        thread_id: &str,
        tool_call_id: &str,
    ) -> Result<ToolCallSnapshot, ToolCallControlError> {
        let _operation = self
            .operation_guard()
            .map_err(|_| ToolCallControlError::Conflict)?;
        self.inner
            .tool_calls
            .request_offload(thread_id, tool_call_id)
            .await
    }

    /// Cancels one active tool without cancelling the owning Turn.
    ///
    /// # Errors
    ///
    /// Returns an error for a missing or completed call.
    pub async fn cancel_tool_call(
        &self,
        thread_id: &str,
        tool_call_id: &str,
        force: bool,
    ) -> Result<ToolCallSnapshot, ToolCallControlError> {
        let _operation = self
            .operation_guard()
            .map_err(|_| ToolCallControlError::Conflict)?;
        self.inner
            .tool_calls
            .cancel(thread_id, tool_call_id, force)
            .await
    }

    /// Changes one active tool's offload or hard-kill deadline.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid input, a missing call, or an invalid state.
    pub async fn extend_tool_call_deadline(
        &self,
        thread_id: &str,
        tool_call_id: &str,
        target: &str,
        seconds: Option<f64>,
        no_deadline: bool,
    ) -> Result<ToolCallSnapshot, ToolCallControlError> {
        let _operation = self
            .operation_guard()
            .map_err(|_| ToolCallControlError::Conflict)?;
        self.inner
            .tool_calls
            .extend_deadline(thread_id, tool_call_id, target, seconds, no_deadline)
            .await
    }

    /// Validates an environment snapshot without changing runtime or storage.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid names, values, or size limits.
    pub fn validate_runtime_environment(
        environment: &BTreeMap<String, String>,
    ) -> Result<(), CoreError> {
        validate_environment(environment)
    }

    /// Replaces the environment inherited by future Agent child processes.
    ///
    /// # Errors
    ///
    /// Returns an error when a key/value is invalid or the runtime lock is
    /// unavailable.
    pub fn replace_runtime_environment(
        &self,
        environment: BTreeMap<String, String>,
    ) -> Result<(), CoreError> {
        let _operation = self.operation_guard()?;
        validate_environment(&environment)?;
        // Match restore's MCP -> environment lock order and prepare both
        // guards before changing either runtime view.
        let mut mcp = self
            .inner
            .mcp
            .write()
            .map_err(|_| CoreError::Config(String::from("MCP runtime lock is poisoned")))?;
        let mut runtime = self
            .inner
            .runtime_environment
            .write()
            .map_err(|_| CoreError::Config(String::from("environment runtime lock failed")))?;
        let manager = mcp.with_environment(environment.clone());
        *runtime = environment;
        *mcp = manager;
        Ok(())
    }

    /// Captures the effective, application-owned child-process environment.
    ///
    /// Values may be secrets. This does not enumerate the host environment and
    /// must not be returned from non-secret configuration or logging APIs.
    ///
    /// # Errors
    ///
    /// Returns an error when the runtime lock is unavailable.
    pub fn runtime_environment(&self) -> Result<BTreeMap<String, String>, CoreError> {
        self.inner
            .runtime_environment
            .read()
            .map_err(|_| CoreError::Config(String::from("environment runtime lock failed")))
            .map(|environment| environment.clone())
    }

    fn open_runtime_workspace(&self, root: &str) -> Result<Workspace, CoreError> {
        let environment = self.runtime_environment()?;
        Workspace::open(Path::new(root))
            .map(|workspace| workspace.with_environment(environment))
            .map_err(CoreError::workspace)
    }

    pub async fn list_workspaces(&self) -> WorkspaceListResponse {
        let state = self.inner.state.lock().await;
        let mut workspaces = BTreeMap::<String, WorkspaceInfo>::new();
        for record in state.threads.values() {
            let Some(root) = &record.thread.workspace_root else {
                continue;
            };
            let workspace = workspaces.entry(root.clone()).or_insert(WorkspaceInfo {
                root: root.clone(),
                thread_count: 0,
                archived_thread_count: 0,
                updated_at: record.thread.updated_at,
            });
            workspace.thread_count = workspace.thread_count.saturating_add(1);
            if record.thread.archived {
                workspace.archived_thread_count = workspace.archived_thread_count.saturating_add(1);
            }
            workspace.updated_at = workspace.updated_at.max(record.thread.updated_at);
        }
        let mut data = workspaces.into_values().collect::<Vec<_>>();
        data.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.root.cmp(&right.root))
        });
        WorkspaceListResponse { data }
    }

    /// Reads an already registered Workspace without probing arbitrary paths.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::WorkspaceNotFound`] when no Thread uses `root`.
    pub async fn read_workspace(&self, root: &str) -> Result<WorkspaceReadResponse, CoreError> {
        let workspace = self
            .list_workspaces()
            .await
            .data
            .into_iter()
            .find(|workspace| workspace.root == root)
            .ok_or_else(|| CoreError::WorkspaceNotFound(root.to_owned()))?;
        Ok(WorkspaceReadResponse { workspace })
    }

    /// Starts one agent turn and returns its live event stream.
    ///
    /// # Errors
    ///
    /// Returns an error when the thread does not exist, already has an active
    /// turn, or the supplied input contains no text or file references.
    pub async fn start_turn(
        &self,
        params: TurnStartParams,
    ) -> Result<(TurnStartResponse, TurnEventStream), CoreError> {
        let model = self
            .inner
            .model
            .with_runtime(self.inner.model.runtime_snapshot());
        let runtime = self.agent_runtime_config()?;
        self.start_turn_with_client(params, model, None, runtime, None)
            .await
    }

    /// Starts a turn with application-resolved provider settings, or the current
    /// global default when no selection is supplied. Credentials
    /// remain private to this turn; global settings and other turns are unchanged.
    /// All model steps in this turn retain the same provider snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid model settings or input, an unavailable
    /// thread, or storage failure, as with [`Self::start_turn`].
    pub async fn start_turn_with_model(
        &self,
        params: TurnStartParams,
        selection: Option<(ModelConfig, ModelRequestOptions)>,
    ) -> Result<(TurnStartResponse, TurnEventStream), CoreError> {
        self.start_turn_with_runtime(params, selection, self.agent_runtime_config()?)
            .await
    }

    /// Starts a turn with settings resolved by a trusted embedding application.
    ///
    /// Provider credentials and Agent runtime settings are private snapshots for
    /// this turn. Global defaults and concurrent turns are never changed. This is
    /// a host API, not a wire-protocol override: hosts must resolve authorization
    /// before selecting an approval level, especially `Off`.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid runtime or model settings before recording
    /// input, or for the same input, thread, and storage errors as `start_turn`.
    pub async fn start_turn_with_runtime(
        &self,
        params: TurnStartParams,
        selection: Option<(ModelConfig, ModelRequestOptions)>,
        agent_runtime: AgentRuntimeConfig,
    ) -> Result<(TurnStartResponse, TurnEventStream), CoreError> {
        self.start_turn_with_owner(params, selection, agent_runtime, None)
            .await
    }

    /// Starts a turn with an immutable usage identity resolved by a trusted host.
    ///
    /// The host must validate Workspace ownership. Wire clients cannot provide
    /// this identity. Unattributed embedding calls retain the default ledger.
    ///
    /// # Errors
    /// Returns an error for invalid identity/runtime/model settings before input
    /// is recorded, or for the same thread and storage errors as `start_turn`.
    pub async fn start_turn_with_owner(
        &self,
        params: TurnStartParams,
        selection: Option<(ModelConfig, ModelRequestOptions)>,
        agent_runtime: AgentRuntimeConfig,
        usage_owner: Option<UsageOwner>,
    ) -> Result<(TurnStartResponse, TurnEventStream), CoreError> {
        let model = self.resolve_turn_client(selection, &agent_runtime, usage_owner.as_ref())?;
        let selected_model = model.default_model();
        self.start_turn_with_client(
            params,
            model,
            Some(selected_model),
            agent_runtime,
            usage_owner,
        )
        .await
    }

    /// Applies trusted provider/runtime/owner settings without replacing the
    /// Thread's selected model. All steps retain the same provider snapshot;
    /// model-specific request options use the Thread model, not the provider's
    /// default. Hosts must authorize runtime and ownership before this call.
    ///
    /// # Errors
    /// Rejects invalid host settings before recording input, or returns the
    /// same input, admission and storage errors as `start_turn`.
    pub async fn start_turn_with_thread_model(
        &self,
        params: TurnStartParams,
        selection: Option<(ModelConfig, ModelRequestOptions)>,
        agent_runtime: AgentRuntimeConfig,
        usage_owner: Option<UsageOwner>,
    ) -> Result<(TurnStartResponse, TurnEventStream), CoreError> {
        let model = self.resolve_turn_client(selection, &agent_runtime, usage_owner.as_ref())?;
        self.start_turn_with_client(params, model, None, agent_runtime, usage_owner)
            .await
    }

    fn resolve_turn_client(
        &self,
        selection: Option<(ModelConfig, ModelRequestOptions)>,
        agent_runtime: &AgentRuntimeConfig,
        usage_owner: Option<&UsageOwner>,
    ) -> Result<ModelClient, CoreError> {
        if usage_owner.is_some_and(|owner| !owner.is_valid()) {
            return Err(CoreError::Config(String::from("Invalid usage ownership")));
        }
        agent_runtime.validate()?;
        let runtime = match selection {
            Some((config, options)) => {
                let config = config.normalize().map_err(CoreError::config)?;
                options.validate().map_err(CoreError::config)?;
                ModelRuntime { config, options }
            }
            None => self.inner.model.runtime_snapshot(),
        };
        Ok(self.inner.model.with_runtime(runtime))
    }

    async fn start_turn_with_client(
        &self,
        params: TurnStartParams,
        model: ModelClient,
        selected_model: Option<String>,
        runtime_config: AgentRuntimeConfig,
        usage_owner: Option<UsageOwner>,
    ) -> Result<(TurnStartResponse, TurnEventStream), CoreError> {
        let operation = self.operation_guard()?;
        let thread_id = params.thread_id;
        let (execution, workspace_root) = self.turn_execution_guard(&thread_id).await?;
        let (message, item) = self
            .prepare_user_message(params.input, workspace_root)
            .await?;
        let turn_id = new_id("turn");
        let started_at = now();
        let turn = Turn {
            id: turn_id.clone(),
            thread_id: thread_id.clone(),
            status: TurnStatus::InProgress,
            items: vec![item],
            error: None,
        };
        let cancellation = CancellationToken::new();
        {
            let mut state = self.inner.state.lock().await;
            let record = state
                .threads
                .get_mut(&thread_id)
                .ok_or_else(|| CoreError::ThreadNotFound(thread_id.clone()))?;
            if record.thread.archived {
                return Err(CoreError::ThreadArchived(thread_id));
            }
            if record.active_turn.is_some() {
                return Err(CoreError::ThreadBusy(thread_id));
            }
            let mut snapshot = record.snapshot();
            if let Some(selected_model) = selected_model {
                snapshot.thread.model = selected_model;
            }
            snapshot.messages.push(message);
            snapshot.turns.push(turn.clone());
            snapshot.turn_metadata.push(StoredTurnMetadata {
                turn_id: turn_id.clone(),
                started_at,
                completed_at: None,
                model_calls: Vec::new(),
            });
            snapshot.thread.status = ThreadStatus::Active;
            snapshot.thread.updated_at = started_at;
            self.inner
                .store
                .upsert(&snapshot)
                .map_err(CoreError::storage)?;
            if record.checkpoint.is_none() {
                record.checkpoint = Some(record.snapshot());
            }
            record.thread = snapshot.thread;
            record.turns = snapshot.turns;
            record.messages = snapshot.messages;
            record.turn_metadata = snapshot.turn_metadata;
            record.active_turn = Some(ActiveTurn {
                id: turn_id.clone(),
                cancellation: cancellation.clone(),
                usage_owner,
            });
        }
        let (event_tx, event_rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        let core = self.clone();
        tokio::spawn(async move {
            let _operation = operation;
            // Keep the fence through final persistence, even if the event
            // consumer disconnects or observes an idle status earlier.
            let _execution = execution;
            core.run_turn(
                thread_id,
                turn_id,
                cancellation,
                event_tx,
                model,
                runtime_config,
            )
            .await;
        });
        Ok((TurnStartResponse { turn }, event_rx))
    }

    async fn prepare_user_message(
        &self,
        input: Vec<UserInput>,
        workspace_root: Option<String>,
    ) -> Result<(StoredMessage, Item), CoreError> {
        let text = compose_user_input(&input, workspace_root.as_deref())?;
        let has_images = input
            .iter()
            .any(|part| matches!(part, UserInput::Image { .. }));
        if text.is_empty() && !has_images {
            return Err(CoreError::EmptyInput);
        }
        if text.len() > MAX_TURN_INPUT_BYTES {
            return Err(CoreError::InputTooLarge {
                actual_bytes: text.len(),
                max_bytes: MAX_TURN_INPUT_BYTES,
            });
        }
        let item_id = new_id("item");
        let (public_input, user_input) = if has_images {
            let guard = self.security_policy().settings().file_guard.clone();
            crate::media::prepare_async(input, workspace_root, guard, item_id.clone()).await?
        } else {
            (Vec::new(), None)
        };
        let mut message = StoredMessage::text("user", text.clone());
        message.user_input = user_input;
        Ok((
            message,
            Item::UserMessage {
                id: item_id,
                text,
                input: has_images.then_some(public_input),
            },
        ))
    }

    /// Requests cancellation of a matching active turn.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::ThreadNotFound`] when the target thread is unknown.
    pub async fn interrupt_turn(
        &self,
        params: &TurnInterruptParams,
    ) -> Result<TurnInterruptResponse, CoreError> {
        let state = self.inner.state.lock().await;
        let record = state
            .threads
            .get(&params.thread_id)
            .ok_or_else(|| CoreError::ThreadNotFound(params.thread_id.clone()))?;
        let accepted = record.active_turn.as_ref().is_some_and(|active| {
            if active.id == params.turn_id {
                active.cancellation.cancel();
                true
            } else {
                false
            }
        });
        Ok(TurnInterruptResponse { accepted })
    }

    pub async fn respond_tool_approval(
        &self,
        params: ToolApprovalRespondParams,
    ) -> ToolApprovalRespondResponse {
        let pending = self
            .inner
            .state
            .lock()
            .await
            .approvals
            .remove(&params.approval_id);
        let accepted = pending.is_some_and(|pending| pending.sender.send(params.decision).is_ok());
        ToolApprovalRespondResponse { accepted }
    }

    #[must_use]
    pub fn list_models(&self) -> ModelListResponse {
        let model = self.inner.model.default_model();
        ModelListResponse {
            data: vec![ModelInfo {
                display_name: model.clone(),
                id: model,
                is_default: true,
            }],
        }
    }

    /// Returns configured MCP clients with secret values redacted.
    ///
    /// # Errors
    ///
    /// Returns an error when the secure OAuth credential store cannot be read.
    pub async fn list_mcp_clients(&self) -> Result<Vec<qwenpaw_mcp::McpClientInfo>, CoreError> {
        self.mcp_manager().clients().await.map_err(CoreError::mcp)
    }

    #[must_use]
    pub fn mcp_client_settings(&self) -> Vec<McpClientSettings> {
        self.mcp_manager().settings()
    }

    /// Validates a complete MCP configuration without activating it.
    ///
    /// # Errors
    ///
    /// Returns an error when a client or access policy is invalid.
    pub fn validate_mcp_client_settings(
        &self,
        settings: Vec<McpClientSettings>,
    ) -> Result<(), CoreError> {
        self.mcp_manager()
            .reconfigured(settings)
            .map(|_| ())
            .map_err(CoreError::mcp)
    }

    /// Validates a restore candidate's MCP settings using its hydrated environment.
    /// This does not start connections or access the credential store.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid settings or unresolved/invalid environment bindings.
    pub fn validate_mcp_client_bindings(
        &self,
        settings: Vec<McpClientSettings>,
    ) -> Result<(), CoreError> {
        self.mcp_manager()
            .reconfigured(settings)
            .and_then(|manager| manager.validate_environment_bindings())
            .map_err(CoreError::mcp)
    }

    /// Atomically activates a complete MCP configuration for new turns.
    ///
    /// Existing turns retain the manager snapshot they started with.
    ///
    /// # Errors
    ///
    /// Returns an error when validation fails or the runtime lock is poisoned.
    pub fn replace_mcp_client_settings(
        &self,
        settings: Vec<McpClientSettings>,
    ) -> Result<(), CoreError> {
        let _operation = self.operation_guard()?;
        let mut manager = self
            .inner
            .mcp
            .write()
            .map_err(|_| CoreError::Config(String::from("MCP runtime lock is poisoned")))?;
        *manager = manager.reconfigured(settings).map_err(CoreError::mcp)?;
        Ok(())
    }

    /// Discovers all tools exposed by one configured MCP client.
    ///
    /// # Errors
    ///
    /// Returns an error when the client is unknown or discovery fails.
    pub async fn list_mcp_tools(
        &self,
        server_id: &str,
    ) -> Result<Vec<qwenpaw_mcp::McpToolInfo>, CoreError> {
        let _operation = self.operation_guard()?;
        self.mcp_manager()
            .tools(server_id)
            .await
            .map_err(CoreError::mcp)
    }

    fn mcp_manager(&self) -> McpManager {
        self.inner
            .mcp
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn security_policy(&self) -> SecurityPolicy {
        self.inner
            .security
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Starts interactive OAuth for a configured remote MCP client.
    ///
    /// # Errors
    ///
    /// Returns an error when discovery, registration, or callback setup fails.
    pub async fn start_mcp_oauth(
        &self,
        server_id: &str,
        options: qwenpaw_mcp::McpOAuthStartOptions,
    ) -> Result<qwenpaw_mcp::McpOAuthStartResponse, CoreError> {
        let _operation = self.operation_guard()?;
        self.mcp_manager()
            .start_oauth(server_id, options)
            .await
            .map_err(CoreError::mcp)
    }

    /// Returns secure-store OAuth status for a configured MCP client.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown client or credential-store failure.
    pub async fn mcp_oauth_status(
        &self,
        server_id: &str,
    ) -> Result<qwenpaw_mcp::McpOAuthStatus, CoreError> {
        self.mcp_manager()
            .oauth_status(server_id)
            .await
            .map_err(CoreError::mcp)
    }

    /// Revokes secure-store OAuth credentials for a configured MCP client.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown client or credential-store failure.
    pub async fn revoke_mcp_oauth(&self, server_id: &str) -> Result<(), CoreError> {
        let _operation = self.operation_guard()?;
        self.mcp_manager()
            .revoke_oauth(server_id)
            .await
            .map_err(CoreError::mcp)
    }

    async fn run_turn(
        &self,
        thread_id: String,
        turn_id: String,
        cancellation: CancellationToken,
        event_tx: mpsc::Sender<CoreEvent>,
        model_client: ModelClient,
        runtime_config: AgentRuntimeConfig,
    ) {
        let Some((model, workspace)) = self
            .prepare_turn_runtime(&thread_id, &turn_id, &event_tx)
            .await
        else {
            return;
        };
        let mcp = self.mcp_manager();
        let security = self.security_policy();
        let mcp_tools = tokio::select! {
            () = cancellation.cancelled() => {
                self.finish_turn(
                    &thread_id,
                    &turn_id,
                    TurnOutcome::Interrupted,
                    &event_tx,
                )
                .await;
                return;
            }
            definitions = mcp.definitions() => definitions,
        };
        let mut outcome = TurnOutcome::Failed(String::from("agent exceeded maximum steps"));
        for _ in 0..runtime_config.max_agent_steps {
            let mut tools = match self.enabled_builtin_tool_definitions() {
                Ok(tools) => tools,
                Err(error) => {
                    outcome = TurnOutcome::Failed(error.to_string());
                    break;
                }
            };
            tools.extend(mcp_tools.clone());
            let messages = self.messages_snapshot(&thread_id).await;
            let step = self
                .run_model_step(
                    ModelStepRequest {
                        thread_id: &thread_id,
                        turn_id: &turn_id,
                        model: &model,
                        client: &model_client,
                        messages: &messages,
                        tools: &tools,
                    },
                    &cancellation,
                    &event_tx,
                )
                .await;
            let step = match step {
                Ok(step) => step,
                Err(ModelStepError::Interrupted) => {
                    outcome = TurnOutcome::Interrupted;
                    break;
                }
                Err(ModelStepError::Failed(message)) => {
                    outcome = TurnOutcome::Failed(message);
                    break;
                }
            };
            if let Err(error) = self
                .record_model_step(&thread_id, &turn_id, &step, &event_tx)
                .await
            {
                outcome = TurnOutcome::Failed(error.to_string());
                break;
            }
            if step.tool_calls.is_empty() {
                outcome = TurnOutcome::Completed;
                break;
            }
            if let Err(tool_outcome) = self
                .execute_tool_calls(
                    step.tool_calls,
                    ToolExecutionRequest {
                        thread_id: &thread_id,
                        turn_id: &turn_id,
                        workspace: &workspace,
                        runtime_config: &runtime_config,
                        mcp: &mcp,
                        security: &security,
                        cancellation: &cancellation,
                        event_tx: &event_tx,
                    },
                )
                .await
            {
                outcome = tool_outcome;
                break;
            }
        }
        self.remove_turn_approvals(&thread_id, &turn_id).await;
        self.finish_turn(&thread_id, &turn_id, outcome, &event_tx)
            .await;
    }

    async fn prepare_turn_runtime(
        &self,
        thread_id: &str,
        turn_id: &str,
        event_tx: &mpsc::Sender<CoreEvent>,
    ) -> Option<(String, Workspace)> {
        let (turn, model, workspace_root) = self.turn_context(thread_id, turn_id).await?;
        send_event(
            event_tx,
            CoreEvent::TurnStarted(TurnStartedNotification { turn }),
        )
        .await;
        let workspace = match self.open_runtime_workspace(&workspace_root) {
            Ok(workspace) => workspace,
            Err(error) => {
                self.fail_turn(thread_id, turn_id, error.to_string(), event_tx)
                    .await;
                return None;
            }
        };
        if !self
            .prepare_turn_system_prompt(thread_id, turn_id, &workspace, event_tx)
            .await
        {
            return None;
        }
        Some((model, workspace))
    }

    async fn refresh_system_prompt(
        &self,
        thread_id: &str,
        workspace: &Workspace,
    ) -> Result<(), CoreError> {
        let prompt = build_workspace_system_prompt(workspace.root(), &self.system_prompt_files()?);
        let mut state = self.inner.state.lock().await;
        let record = state
            .threads
            .get_mut(thread_id)
            .ok_or_else(|| CoreError::ThreadNotFound(thread_id.to_owned()))?;
        if let Some(message) = record.messages.first_mut()
            && message.role == "system"
        {
            if message.content == prompt {
                return Ok(());
            }
            message.content = prompt;
        } else {
            record
                .messages
                .insert(0, StoredMessage::text("system", prompt));
        }
        self.inner
            .store
            .upsert(&record.snapshot())
            .map_err(CoreError::storage)
    }

    async fn prepare_turn_system_prompt(
        &self,
        thread_id: &str,
        turn_id: &str,
        workspace: &Workspace,
        event_tx: &mpsc::Sender<CoreEvent>,
    ) -> bool {
        if let Err(error) = self.refresh_system_prompt(thread_id, workspace).await {
            self.fail_turn(thread_id, turn_id, error.to_string(), event_tx)
                .await;
            return false;
        }
        true
    }

    async fn execute_tool_calls(
        &self,
        calls: Vec<ToolCall>,
        request: ToolExecutionRequest<'_>,
    ) -> Result<(), TurnOutcome> {
        let ToolExecutionRequest {
            thread_id,
            turn_id,
            workspace,
            runtime_config,
            mcp,
            security,
            cancellation,
            event_tx,
        } = request;
        for call in calls {
            let mcp_effect = mcp
                .tool_access_effect(&call.name, "console", thread_id)
                .await;
            let security_effect = security.evaluate(
                &call.name,
                &call.arguments,
                match runtime_config.approval_level {
                    ToolApprovalLevel::Strict => SecurityApprovalMode::Strict,
                    ToolApprovalLevel::Smart => SecurityApprovalMode::Smart,
                    ToolApprovalLevel::Auto => SecurityApprovalMode::Auto,
                    ToolApprovalLevel::Off => SecurityApprovalMode::Off,
                },
            );
            let baseline_requires_approval = match mcp_effect {
                Some(McpAccessEffect::Ask) => true,
                Some(McpAccessEffect::Allow | McpAccessEffect::Deny) => false,
                None => Workspace::approval_requirement(&call) == ApprovalRequirement::Required,
            };
            let normally_requires_approval =
                baseline_requires_approval || matches!(security_effect, ToolGuardEffect::Ask(_));
            let requires_approval = match runtime_config.approval_level {
                ToolApprovalLevel::Strict => true,
                ToolApprovalLevel::Smart | ToolApprovalLevel::Auto => normally_requires_approval,
                ToolApprovalLevel::Off => false,
            };
            let output = if mcp_effect == Some(McpAccessEffect::Deny) {
                Ok(ToolOutput {
                    content: String::from("Tool execution was denied by the MCP access policy."),
                    is_error: true,
                })
            } else if let ToolGuardEffect::Deny(message) = security_effect {
                Ok(ToolOutput {
                    content: message,
                    is_error: true,
                })
            } else if requires_approval {
                match self
                    .request_approval(thread_id, turn_id, workspace, &call, cancellation, event_tx)
                    .await
                {
                    ApprovalOutcome::Approved => {
                        self.execute_tracked_tool(
                            thread_id,
                            workspace,
                            &call,
                            runtime_config,
                            mcp,
                            security,
                            cancellation,
                        )
                        .await?
                    }
                    ApprovalOutcome::Denied => Ok(ToolOutput {
                        content: String::from("Tool execution was denied by the user."),
                        is_error: true,
                    }),
                    ApprovalOutcome::Interrupted => return Err(TurnOutcome::Interrupted),
                }
            } else {
                self.execute_tracked_tool(
                    thread_id,
                    workspace,
                    &call,
                    runtime_config,
                    mcp,
                    security,
                    cancellation,
                )
                .await?
            };
            let output = output.unwrap_or_else(|error| ToolOutput {
                content: error,
                is_error: true,
            });
            self.record_tool_result(thread_id, turn_id, &call, output, event_tx)
                .await
                .map_err(|error| TurnOutcome::Failed(error.to_string()))?;
        }
        Ok(())
    }

    async fn run_model_step(
        &self,
        request: ModelStepRequest<'_>,
        cancellation: &CancellationToken,
        event_tx: &mpsc::Sender<CoreEvent>,
    ) -> Result<ModelStep, ModelStepError> {
        let ModelStepRequest {
            thread_id,
            turn_id,
            client,
            ..
        } = request;
        let stream = tokio::select! {
            () = cancellation.cancelled() => return Err(ModelStepError::Interrupted),
            stream = client.chat_stream(request.model, request.messages, request.tools) => stream,
        };
        let mut stream = stream.map_err(|error| ModelStepError::Failed(error.to_string()))?;
        let agent_item_id = new_id("item");
        let mut agent_started = false;
        let mut text = String::new();
        let mut calls = BTreeMap::<usize, ToolCallBuilder>::new();
        let mut usage = None;
        let mut provider_content = BTreeMap::new();
        let mut provider_id = String::from("openai-compatible");
        loop {
            tokio::select! {
                () = cancellation.cancelled() => return Err(ModelStepError::Interrupted),
                event = stream.next() => {
                    match event {
                        Some(Ok(ModelEvent::ProviderIdentity(value))) => { provider_id = value; }
                        Some(Ok(ModelEvent::TextDelta(delta))) => {
                            if text.len().saturating_add(delta.len()) > MAX_AGENT_RESPONSE_BYTES {
                                return Err(ModelStepError::Failed(format!(
                                    "model response exceeded {MAX_AGENT_RESPONSE_BYTES} bytes"
                                )));
                            }
                            if !agent_started {
                                send_event(
                                    event_tx,
                                    CoreEvent::ItemStarted(ItemStartedNotification {
                                        thread_id: thread_id.to_owned(),
                                        turn_id: turn_id.to_owned(),
                                        item: Item::AgentMessage {
                                            id: agent_item_id.clone(),
                                            text: String::new(),
                                        },
                                    }),
                                )
                                .await;
                                agent_started = true;
                            }
                            text.push_str(&delta);
                            send_event(
                                event_tx,
                                CoreEvent::AgentMessageDelta(
                                    AgentMessageDeltaNotification {
                                        thread_id: thread_id.to_owned(),
                                        turn_id: turn_id.to_owned(),
                                        item_id: agent_item_id.clone(),
                                        delta,
                                    },
                                ),
                            )
                            .await;
                        }
                        Some(Ok(ModelEvent::ToolCallDelta {
                            index,
                            id,
                            name,
                            arguments,
                        })) => {
                            if !calls.contains_key(&index) && calls.len() == MAX_TOOL_CALLS_PER_STEP {
                                return Err(ModelStepError::Failed(format!(
                                    "model returned more than {MAX_TOOL_CALLS_PER_STEP} tool calls"
                                )));
                            }
                            calls.entry(index).or_default().push(id, name, arguments)?;
                        }
                        Some(Ok(ModelEvent::Usage(value))) => {
                            usage = Some(value);
                        }
                        Some(Ok(ModelEvent::ProviderContent { protocol, content })) => {
                            provider_content.insert(protocol.to_owned(), content);
                        }
                        Some(Err(error)) => {
                            return Err(ModelStepError::Failed(error.to_string()));
                        }
                        None => break,
                    }
                }
            }
        }
        Ok(ModelStep {
            provider_id,
            agent_item_id,
            text,
            tool_calls: calls
                .into_values()
                .map(ToolCallBuilder::build)
                .collect::<Result<Vec<_>, _>>()?,
            usage,
            provider_content,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_tracked_tool(
        &self,
        thread_id: &str,
        workspace: &Workspace,
        call: &ToolCall,
        runtime_config: &AgentRuntimeConfig,
        mcp: &McpManager,
        security: &SecurityPolicy,
        turn_cancellation: &CancellationToken,
    ) -> Result<Result<ToolOutput, String>, TurnOutcome> {
        // Each detached task retains a lease through its final publication.
        let execution_operation = self
            .operation_guard()
            .map_err(|error| TurnOutcome::Failed(error.to_string()))?;
        let publication_operation = self
            .operation_guard()
            .map_err(|error| TurnOutcome::Failed(error.to_string()))?;
        let timeout =
            qwenpaw_tools::effective_shell_timeout_ms(call, runtime_config.shell_timeout_ms)
                .map(Duration::from_millis);
        let max_internal_timeout =
            timeout.map(|_| Duration::from_millis(qwenpaw_tools::MAX_SHELL_TIMEOUT_MS));
        let mut lease = self
            .inner
            .tool_calls
            .begin(
                thread_id,
                &call.id,
                &call.name,
                timeout,
                max_internal_timeout,
                turn_cancellation,
            )
            .await
            .map_err(|error| TurnOutcome::Failed(error.to_string()))?;
        let core = self.clone();
        let workspace = workspace.clone();
        let call_for_execution = call.clone();
        let runtime_config = runtime_config.clone();
        let mcp = mcp.clone();
        let security = security.clone();
        let turn_cancellation = turn_cancellation.clone();
        let tool_cancellation = lease.cancellation.clone();
        let mut execution = tokio::spawn(async move {
            let _operation = execution_operation;
            core.execute_tool(
                &workspace,
                &call_for_execution,
                &runtime_config,
                &mcp,
                &security,
                &turn_cancellation,
                &tool_cancellation,
            )
            .await
        });
        tokio::select! {
            biased;
            result = &mut execution => {
                let _operation = publication_operation;
                self.finish_foreground_tool(&call.id, result).await
            }
            () = lease.wait_for_offload() => {
                let tool_calls = self.inner.tool_calls.clone();
                let call_id = call.id.clone();
                let tool_name = call.name.clone();
                tokio::spawn(async move {
                    let _operation = publication_operation;
                    let output = background_tool_output(&tool_name, execution.await);
                    tool_calls.finish(&call_id, &output).await;
                });
                Ok(Ok(ToolOutput {
                    content: format!(
                        "Tool '{}' was moved to the background (call_id={}). Continue other work; do not run the same call again.",
                        call.name, call.id
                    ),
                    is_error: false,
                }))
            }
        }
    }

    async fn finish_foreground_tool(
        &self,
        tool_call_id: &str,
        result: Result<Result<Result<ToolOutput, String>, TurnOutcome>, tokio::task::JoinError>,
    ) -> Result<Result<ToolOutput, String>, TurnOutcome> {
        match result {
            Ok(Ok(Ok(output))) => {
                self.inner.tool_calls.finish(tool_call_id, &output).await;
                Ok(Ok(output))
            }
            Ok(Ok(Err(error))) => {
                self.inner
                    .tool_calls
                    .finish(
                        tool_call_id,
                        &ToolOutput {
                            content: error.clone(),
                            is_error: true,
                        },
                    )
                    .await;
                Ok(Err(error))
            }
            Ok(Err(outcome)) => {
                self.inner
                    .tool_calls
                    .finish(
                        tool_call_id,
                        &ToolOutput {
                            content: turn_outcome_message(&outcome),
                            is_error: true,
                        },
                    )
                    .await;
                Err(outcome)
            }
            Err(error) => {
                let message = format!("tool execution task failed: {error}");
                self.inner
                    .tool_calls
                    .finish(
                        tool_call_id,
                        &ToolOutput {
                            content: message.clone(),
                            is_error: true,
                        },
                    )
                    .await;
                Err(TurnOutcome::Failed(message))
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_tool(
        &self,
        workspace: &Workspace,
        call: &ToolCall,
        runtime_config: &AgentRuntimeConfig,
        mcp: &McpManager,
        security: &SecurityPolicy,
        turn_cancellation: &CancellationToken,
        tool_cancellation: &CancellationToken,
    ) -> Result<Result<ToolOutput, String>, TurnOutcome> {
        if mcp.contains_tool(&call.name).await {
            return tokio::select! {
                biased;
                () = turn_cancellation.cancelled() => {
                    mcp.cancel_tool(&call.name).await;
                    Err(TurnOutcome::Interrupted)
                }
                () = tool_cancellation.cancelled() => {
                    mcp.cancel_tool(&call.name).await;
                    Ok(Err(self.tool_cancellation_message(&call.id).await))
                }
                output = mcp.call_tool(&call.name, &call.arguments) => {
                    Ok(output
                        .map(|output| ToolOutput {
                            content: output.content,
                            is_error: output.is_error,
                        })
                        .map_err(|error| error.to_string()))
                }
            };
        }
        if qwenpaw_tools::is_builtin(&call.name)
            && !self
                .builtin_tool_enabled(&call.name)
                .map_err(|error| TurnOutcome::Failed(error.to_string()))?
        {
            return Ok(Err(format!("Tool '{}' is disabled", call.name)));
        }
        tokio::select! {
            biased;
            () = turn_cancellation.cancelled() => Err(TurnOutcome::Interrupted),
            () = tool_cancellation.cancelled() => {
                Ok(Err(self.tool_cancellation_message(&call.id).await))
            }
            output = workspace.execute_with_shell_config_timeout_and_sandbox(
                call,
                runtime_config.shell_timeout_ms,
                (!runtime_config.shell_executable.is_empty())
                    .then_some(runtime_config.shell_executable.as_str()),
                (call.name == "shell").then_some(qwenpaw_tools::MAX_SHELL_TIMEOUT_MS),
                security.sandbox_active(),
            ) => Ok(output.map_err(|error| error.to_string())),
        }
    }

    async fn tool_cancellation_message(&self, tool_call_id: &str) -> String {
        match self
            .inner
            .tool_calls
            .cancellation_reason(tool_call_id)
            .await
        {
            Some(ToolCancellationReason::Timeout) => {
                String::from("Tool execution was cancelled due to timeout.")
            }
            Some(ToolCancellationReason::User) | None => {
                String::from("Tool execution was cancelled by the user.")
            }
        }
    }

    async fn record_model_step(
        &self,
        thread_id: &str,
        turn_id: &str,
        step: &ModelStep,
        event_tx: &mpsc::Sender<CoreEvent>,
    ) -> Result<(), CoreError> {
        let agent_item = (!step.text.is_empty()).then(|| Item::AgentMessage {
            id: step.agent_item_id.clone(),
            text: step.text.clone(),
        });
        let tool_items = step
            .tool_calls
            .iter()
            .map(|call| Item::ToolCall {
                id: new_id("item"),
                call_id: call.id.clone(),
                name: call.name.clone(),
                arguments: call.arguments.clone(),
            })
            .collect::<Vec<_>>();
        let stored_calls = step
            .tool_calls
            .iter()
            .map(|call| StoredToolCall {
                id: call.id.clone(),
                kind: String::from("function"),
                function: StoredFunctionCall {
                    name: call.name.clone(),
                    arguments: call.arguments.clone(),
                },
            })
            .collect();
        let usage_observed = step.usage.is_some();
        let usage = step.usage.clone().unwrap_or_default();
        let recorded_at = now();
        let (snapshot, usage_record) = {
            let mut state = self.inner.state.lock().await;
            let record = state
                .threads
                .get_mut(thread_id)
                .ok_or_else(|| CoreError::ThreadNotFound(thread_id.to_owned()))?;
            let turn = record
                .turns
                .iter_mut()
                .find(|turn| turn.id == turn_id)
                .ok_or_else(|| CoreError::TurnNotFound(turn_id.to_owned()))?;
            let mut message = StoredMessage::assistant_tool_calls(step.text.clone(), stored_calls);
            message.provider_content.clone_from(&step.provider_content);
            record.messages.push(message);
            if let Some(item) = &agent_item {
                turn.items.push(item.clone());
            }
            turn.items.extend(tool_items.clone());
            let metadata = record
                .turn_metadata
                .iter_mut()
                .find(|metadata| metadata.turn_id == turn_id)
                .ok_or_else(|| CoreError::TurnNotFound(turn_id.to_owned()))?;
            let model_call = stored_model_call(
                &step.provider_id,
                &record.thread.model,
                &usage,
                usage_observed,
            );
            metadata.model_calls.push(model_call.clone());
            let owner = record
                .active_turn
                .as_ref()
                .and_then(|active| active.usage_owner.as_ref());
            let usage_record = usage_record(thread_id, turn_id, recorded_at, model_call, owner);
            (record.snapshot(), usage_record)
        };
        if let Some(usage_record) = &usage_record {
            self.inner
                .store
                .upsert_with_usage(&snapshot, usage_record)
                .map_err(CoreError::storage)?;
        } else {
            self.inner
                .store
                .upsert(&snapshot)
                .map_err(CoreError::storage)?;
        }
        if let Some(usage_record) = usage_record {
            self.inner
                .state
                .lock()
                .await
                .usage_records
                .push(usage_record);
        }
        send_model_step_events(thread_id, turn_id, agent_item, tool_items, event_tx).await;
        Ok(())
    }

    async fn record_tool_result(
        &self,
        thread_id: &str,
        turn_id: &str,
        call: &ToolCall,
        output: ToolOutput,
        event_tx: &mpsc::Sender<CoreEvent>,
    ) -> Result<(), CoreError> {
        let item = Item::ToolResult {
            id: new_id("item"),
            call_id: call.id.clone(),
            content: output.content.clone(),
            is_error: output.is_error,
        };
        let snapshot = {
            let mut state = self.inner.state.lock().await;
            let record = state
                .threads
                .get_mut(thread_id)
                .ok_or_else(|| CoreError::ThreadNotFound(thread_id.to_owned()))?;
            let turn = record
                .turns
                .iter_mut()
                .find(|turn| turn.id == turn_id)
                .ok_or_else(|| CoreError::TurnNotFound(turn_id.to_owned()))?;
            let mut message = StoredMessage::tool_result(call.id.clone(), output.content);
            message.tool_error = output.is_error.then_some(true);
            record.messages.push(message);
            turn.items.push(item.clone());
            record.snapshot()
        };
        self.inner
            .store
            .upsert(&snapshot)
            .map_err(CoreError::storage)?;
        send_event(
            event_tx,
            CoreEvent::ItemStarted(ItemStartedNotification {
                thread_id: thread_id.to_owned(),
                turn_id: turn_id.to_owned(),
                item: item.clone(),
            }),
        )
        .await;
        send_event(
            event_tx,
            CoreEvent::ItemCompleted(ItemCompletedNotification {
                thread_id: thread_id.to_owned(),
                turn_id: turn_id.to_owned(),
                item,
            }),
        )
        .await;
        Ok(())
    }

    async fn request_approval(
        &self,
        thread_id: &str,
        turn_id: &str,
        workspace: &Workspace,
        call: &ToolCall,
        cancellation: &CancellationToken,
        event_tx: &mpsc::Sender<CoreEvent>,
    ) -> ApprovalOutcome {
        let approval_id = new_id("approval");
        let (sender, receiver) = oneshot::channel();
        self.inner.state.lock().await.approvals.insert(
            approval_id.clone(),
            PendingApproval {
                thread_id: thread_id.to_owned(),
                turn_id: turn_id.to_owned(),
                sender,
            },
        );
        send_event(
            event_tx,
            CoreEvent::ToolApprovalRequested(ToolApprovalRequestedNotification {
                thread_id: thread_id.to_owned(),
                turn_id: turn_id.to_owned(),
                approval_id: approval_id.clone(),
                call_id: call.id.clone(),
                tool_name: call.name.clone(),
                arguments: call.arguments.clone(),
                workspace_root: workspace.root().to_string_lossy().into_owned(),
            }),
        )
        .await;
        let decision = tokio::select! {
            () = cancellation.cancelled() => None,
            result = receiver => result.ok(),
            () = tokio::time::sleep(APPROVAL_TIMEOUT) => Some(ApprovalDecision::Denied),
        };
        self.inner.state.lock().await.approvals.remove(&approval_id);
        let Some(decision) = decision else {
            return ApprovalOutcome::Interrupted;
        };
        send_event(
            event_tx,
            CoreEvent::ToolApprovalResolved(ToolApprovalResolvedNotification {
                thread_id: thread_id.to_owned(),
                turn_id: turn_id.to_owned(),
                approval_id,
                decision,
            }),
        )
        .await;
        match decision {
            ApprovalDecision::Approved => ApprovalOutcome::Approved,
            ApprovalDecision::Denied => ApprovalOutcome::Denied,
        }
    }

    async fn turn_context(&self, thread_id: &str, turn_id: &str) -> Option<(Turn, String, String)> {
        let state = self.inner.state.lock().await;
        let record = state.threads.get(thread_id)?;
        let turn = record.turns.iter().find(|turn| turn.id == turn_id)?.clone();
        Some((
            turn,
            record.thread.model.clone(),
            record.thread.workspace_root.clone()?,
        ))
    }

    async fn messages_snapshot(&self, thread_id: &str) -> Vec<StoredMessage> {
        self.inner
            .state
            .lock()
            .await
            .threads
            .get(thread_id)
            .map_or_else(Vec::new, |record| record.messages.clone())
    }

    async fn remove_turn_approvals(&self, thread_id: &str, turn_id: &str) {
        self.inner
            .state
            .lock()
            .await
            .approvals
            .retain(|_, pending| pending.thread_id != thread_id || pending.turn_id != turn_id);
    }

    async fn fail_turn(
        &self,
        thread_id: &str,
        turn_id: &str,
        message: String,
        event_tx: &mpsc::Sender<CoreEvent>,
    ) {
        self.finish_turn(thread_id, turn_id, TurnOutcome::Failed(message), event_tx)
            .await;
    }

    async fn finish_turn(
        &self,
        thread_id: &str,
        turn_id: &str,
        outcome: TurnOutcome,
        event_tx: &mpsc::Sender<CoreEvent>,
    ) {
        let completed_at = now();
        let completed_turn = {
            let mut state = self.inner.state.lock().await;
            let Some(record) = state.threads.get_mut(thread_id) else {
                return;
            };
            let Some(turn_index) = record.turns.iter().position(|turn| turn.id == turn_id) else {
                return;
            };
            let turn = &mut record.turns[turn_index];
            match outcome {
                TurnOutcome::Completed => {
                    turn.status = TurnStatus::Completed;
                    record.thread.status = ThreadStatus::Idle;
                }
                TurnOutcome::Interrupted => {
                    turn.status = TurnStatus::Interrupted;
                    record.thread.status = ThreadStatus::Idle;
                }
                TurnOutcome::Failed(message) => {
                    turn.status = TurnStatus::Failed;
                    turn.error = Some(ErrorInfo { message });
                    record.thread.status = ThreadStatus::Error;
                }
            }
            if let Some(metadata) = record
                .turn_metadata
                .iter_mut()
                .find(|metadata| metadata.turn_id == turn_id)
            {
                metadata.completed_at = Some(completed_at);
            }
            record.thread.updated_at = completed_at;
            // Serialize final persistence with admission. Do not publish an
            // idle slot until this write can no longer overwrite a new Turn.
            match self.inner.store.upsert(&record.snapshot()) {
                Ok(()) => {
                    record.persisted_turns.insert(turn_id.to_owned());
                    record.checkpoint = None;
                }
                Err(error) => {
                    warn!(%error, "failed to persist completed turn");
                    self.inner
                        .final_persistence_failed
                        .store(true, Ordering::Relaxed);
                    let turn = &mut record.turns[turn_index];
                    let failure = "Failed to persist the final turn; the latest state may not survive restart.";
                    let message = turn.error.take().map_or_else(
                        || failure.to_owned(),
                        |original| format!("{}\n{failure}", original.message),
                    );
                    turn.status = TurnStatus::Failed;
                    turn.error = Some(ErrorInfo { message });
                    record.thread.status = ThreadStatus::Error;
                }
            }
            record.active_turn = None;
            record.turns[turn_index].clone()
        };
        send_event(
            event_tx,
            CoreEvent::TurnCompleted(TurnCompletedNotification {
                turn: completed_turn,
            }),
        )
        .await;
    }
}

fn validate_ui_language(language: &str) -> Result<(), CoreError> {
    if SUPPORTED_UI_LANGUAGES.contains(&language) {
        return Ok(());
    }
    Err(CoreError::Config(format!(
        "UI language must be one of: {}",
        SUPPORTED_UI_LANGUAGES.join(", ")
    )))
}

fn validate_environment_keys(keys: &[String]) -> Result<(), CoreError> {
    if keys.len() > MAX_ENVIRONMENT_VARIABLES {
        return Err(CoreError::Config(format!(
            "environment contains more than {MAX_ENVIRONMENT_VARIABLES} variables"
        )));
    }
    let mut unique = keys.to_vec();
    unique.sort();
    unique.dedup();
    if unique.len() != keys.len() {
        return Err(CoreError::Config(String::from(
            "environment contains duplicate variable names",
        )));
    }
    for key in keys {
        if !valid_environment_key(key) {
            return Err(CoreError::Config(format!(
                "environment variable name is invalid: {key}"
            )));
        }
    }
    Ok(())
}

fn validate_environment(environment: &BTreeMap<String, String>) -> Result<(), CoreError> {
    let keys = environment.keys().cloned().collect::<Vec<_>>();
    validate_environment_keys(&keys)?;
    for (key, value) in environment {
        if value.len() > MAX_ENVIRONMENT_VALUE_BYTES || value.contains('\0') {
            return Err(CoreError::Config(format!(
                "environment variable value is invalid: {key}"
            )));
        }
    }
    Ok(())
}

fn valid_environment_key(key: &str) -> bool {
    if key.is_empty() || key.len() > MAX_ENVIRONMENT_KEY_BYTES {
        return false;
    }
    let mut characters = key.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

#[derive(Debug)]
struct ModelStep {
    provider_id: String,
    agent_item_id: String,
    text: String,
    tool_calls: Vec<ToolCall>,
    usage: Option<ModelUsage>,
    provider_content: BTreeMap<String, Vec<Value>>,
}

struct ModelStepRequest<'a> {
    thread_id: &'a str,
    turn_id: &'a str,
    model: &'a str,
    client: &'a ModelClient,
    messages: &'a [StoredMessage],
    tools: &'a [serde_json::Value],
}

fn stored_model_call(
    provider_id: &str,
    model: &str,
    usage: &ModelUsage,
    usage_observed: bool,
) -> StoredModelCall {
    StoredModelCall {
        provider_id: provider_id.to_owned(),
        model: model.to_owned(),
        prompt_tokens: usage.prompt_tokens,
        completion_tokens: usage.completion_tokens,
        cache_read_tokens: usage.cache_read_tokens,
        cache_write_tokens: usage.cache_write_tokens,
        cache_eligible_input_tokens: usage.cache_eligible_input_tokens,
        cache_observed: usage.cache_observed,
        usage_observed,
    }
}

fn usage_record(
    thread_id: &str,
    turn_id: &str,
    recorded_at: i64,
    call: StoredModelCall,
    owner: Option<&UsageOwner>,
) -> Option<StoredUsageRecord> {
    (call.usage_observed && (call.prompt_tokens > 0 || call.completion_tokens > 0)).then(|| {
        StoredUsageRecord {
            id: new_id("usage"),
            thread_id: thread_id.to_owned(),
            turn_id: turn_id.to_owned(),
            agent_id: owner.map_or_else(|| String::from("default"), |owner| owner.agent_id.clone()),
            data_key: owner.map(|owner| owner.data_key.clone()),
            recorded_at,
            call,
        }
    })
}

fn persist_builtin_tool_override(
    store: &ThreadStore,
    overrides: &mut BTreeMap<String, bool>,
    tool_name: &str,
    enabled: bool,
) -> Result<(), CoreError> {
    let mut next = overrides.clone();
    if enabled {
        next.remove(tool_name);
    } else {
        next.insert(tool_name.to_owned(), false);
    }
    let serialized = serde_json::to_string(&next).map_err(CoreError::storage)?;
    store
        .write_settings(&[(BUILTIN_TOOL_OVERRIDES_SETTING, &serialized)])
        .map_err(CoreError::storage)?;
    *overrides = next;
    Ok(())
}

async fn send_model_step_events(
    thread_id: &str,
    turn_id: &str,
    agent_item: Option<Item>,
    tool_items: Vec<Item>,
    event_tx: &mpsc::Sender<CoreEvent>,
) {
    if let Some(item) = agent_item {
        send_event(
            event_tx,
            CoreEvent::ItemCompleted(ItemCompletedNotification {
                thread_id: thread_id.to_owned(),
                turn_id: turn_id.to_owned(),
                item,
            }),
        )
        .await;
    }
    for item in tool_items {
        send_event(
            event_tx,
            CoreEvent::ItemStarted(ItemStartedNotification {
                thread_id: thread_id.to_owned(),
                turn_id: turn_id.to_owned(),
                item: item.clone(),
            }),
        )
        .await;
        send_event(
            event_tx,
            CoreEvent::ItemCompleted(ItemCompletedNotification {
                thread_id: thread_id.to_owned(),
                turn_id: turn_id.to_owned(),
                item,
            }),
        )
        .await;
    }
}

struct ToolExecutionRequest<'a> {
    thread_id: &'a str,
    turn_id: &'a str,
    workspace: &'a Workspace,
    runtime_config: &'a AgentRuntimeConfig,
    mcp: &'a McpManager,
    security: &'a SecurityPolicy,
    cancellation: &'a CancellationToken,
    event_tx: &'a mpsc::Sender<CoreEvent>,
}

#[derive(Debug, Default)]
struct ToolCallBuilder {
    id: String,
    name: String,
    arguments: String,
}

impl ToolCallBuilder {
    fn push(
        &mut self,
        id: Option<String>,
        name: Option<String>,
        arguments: Option<String>,
    ) -> Result<(), ModelStepError> {
        if let Some(id) = id {
            self.id.push_str(&id);
        }
        if let Some(name) = name {
            self.name.push_str(&name);
        }
        if let Some(arguments) = arguments {
            self.arguments.push_str(&arguments);
        }
        if self.id.len() > MAX_TOOL_CALL_ID_BYTES
            || self.name.len() > MAX_TOOL_NAME_BYTES
            || self.arguments.len() > MAX_TOOL_ARGUMENT_BYTES
        {
            return Err(ModelStepError::Failed(String::from(
                "model returned an oversized tool call",
            )));
        }
        Ok(())
    }

    fn build(self) -> Result<ToolCall, ModelStepError> {
        if self.id.is_empty() || self.name.is_empty() {
            return Err(ModelStepError::Failed(String::from(
                "model returned an incomplete tool call",
            )));
        }
        Ok(ToolCall {
            id: self.id,
            name: self.name,
            arguments: self.arguments,
        })
    }
}

enum ModelStepError {
    Interrupted,
    Failed(String),
}

enum ApprovalOutcome {
    Approved,
    Denied,
    Interrupted,
}

enum TurnOutcome {
    Completed,
    Interrupted,
    Failed(String),
}

fn background_tool_output(
    tool_name: &str,
    result: Result<Result<Result<ToolOutput, String>, TurnOutcome>, tokio::task::JoinError>,
) -> ToolOutput {
    match result {
        Ok(Ok(Ok(output))) => output,
        Ok(Ok(Err(error))) => ToolOutput {
            content: error,
            is_error: true,
        },
        Ok(Err(outcome)) => ToolOutput {
            content: turn_outcome_message(&outcome),
            is_error: true,
        },
        Err(error) => ToolOutput {
            content: format!("background tool '{tool_name}' task failed: {error}"),
            is_error: true,
        },
    }
}

fn turn_outcome_message(outcome: &TurnOutcome) -> String {
    match outcome {
        TurnOutcome::Completed => String::from("Tool execution completed without output."),
        TurnOutcome::Interrupted => String::from("Tool execution was interrupted."),
        TurnOutcome::Failed(error) => error.clone(),
    }
}

fn default_system_prompt_files() -> Vec<String> {
    DEFAULT_SYSTEM_PROMPT_FILES
        .iter()
        .map(|filename| (*filename).to_owned())
        .collect()
}

fn validate_system_prompt_files(files: &[String]) -> Result<(), CoreError> {
    if files.len() > MAX_SYSTEM_PROMPT_FILES {
        return Err(CoreError::Config(format!(
            "system prompt files exceed the {MAX_SYSTEM_PROMPT_FILES}-file limit"
        )));
    }
    let mut unique = HashSet::with_capacity(files.len());
    for filename in files {
        let path = Path::new(filename);
        let mut components = path.components();
        let valid_component = matches!(components.next(), Some(std::path::Component::Normal(_)))
            && components.next().is_none();
        if filename.is_empty()
            || filename.len() > MAX_SYSTEM_PROMPT_FILENAME_BYTES
            || filename.chars().any(char::is_control)
            || filename.contains('/')
            || filename.contains('\\')
            || !filename.to_ascii_lowercase().ends_with(".md")
            || !valid_component
        {
            return Err(CoreError::Config(format!(
                "invalid system prompt filename: {filename}"
            )));
        }
        if !unique.insert(filename) {
            return Err(CoreError::Config(format!(
                "duplicate system prompt filename: {filename}"
            )));
        }
    }
    Ok(())
}

fn build_workspace_system_prompt(root: &Path, files: &[String]) -> String {
    let mut parts = Vec::new();
    let mut total_bytes = 0usize;
    for filename in files {
        let path = root.join(filename);
        let Ok(metadata) = path.symlink_metadata() else {
            continue;
        };
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() > MAX_SYSTEM_PROMPT_FILE_BYTES
        {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        if total_bytes.saturating_add(bytes.len()) > MAX_SYSTEM_PROMPT_BYTES {
            break;
        }
        let content = String::from_utf8_lossy(&bytes);
        let content = strip_yaml_frontmatter(content.trim());
        let content = if filename == "AGENTS.md" {
            strip_prompt_section(strip_prompt_section(content, "heartbeat"), "memory")
        } else {
            content.to_owned()
        };
        let content = content.trim();
        if content.is_empty() {
            continue;
        }
        total_bytes = total_bytes.saturating_add(bytes.len());
        parts.push(format!("# {filename}\n\n{content}"));
    }
    if parts.is_empty() {
        String::from(SYSTEM_PROMPT)
    } else {
        parts.join("\n\n")
    }
}

fn strip_yaml_frontmatter(content: &str) -> &str {
    let Some(remainder) = content.strip_prefix("---") else {
        return content;
    };
    let Some(end) = remainder.find("\n---") else {
        return content;
    };
    remainder[end + "\n---".len()..].trim()
}

fn strip_prompt_section(content: impl AsRef<str>, section: &str) -> String {
    let content = content.as_ref();
    let start_marker = format!("<!-- {section}:start -->");
    let end_marker = format!("<!-- {section}:end -->");
    let Some(start) = content.find(&start_marker) else {
        return content.to_owned();
    };
    let Some(relative_end) = content[start + start_marker.len()..].find(&end_marker) else {
        return content.to_owned();
    };
    let end = start + start_marker.len() + relative_end + end_marker.len();
    let mut filtered = String::with_capacity(content.len().saturating_sub(end - start));
    filtered.push_str(&content[..start]);
    filtered.push_str(&content[end..]);
    filtered.trim().to_owned()
}

impl ThreadRecord {
    fn from_stored(snapshot: StoredThread) -> Self {
        Self {
            execution: Arc::default(),
            thread: snapshot.thread,
            turns: snapshot.turns,
            messages: snapshot.messages,
            turn_metadata: snapshot.turn_metadata,
            active_turn: None,
            checkpoint: None,
            persisted_turns: HashSet::new(),
        }
    }

    fn snapshot(&self) -> StoredThread {
        StoredThread {
            thread: self.thread.clone(),
            turns: self.turns.clone(),
            messages: self.messages.clone(),
            turn_metadata: self.turn_metadata.clone(),
        }
    }
}

fn recover_interrupted_turns(snapshot: &mut StoredThread) {
    let mut recovered = false;
    for turn in &mut snapshot.turns {
        if turn.status == TurnStatus::InProgress {
            turn.status = TurnStatus::Interrupted;
            recovered = true;
        }
    }
    if snapshot.thread.status == ThreadStatus::Active || recovered {
        snapshot.thread.status = ThreadStatus::Idle;
        snapshot.thread.updated_at = now();
    }
}

fn ensure_system_message(snapshot: &mut StoredThread) {
    if snapshot
        .messages
        .first()
        .is_none_or(|message| message.role != "system")
    {
        snapshot
            .messages
            .insert(0, StoredMessage::text("system", SYSTEM_PROMPT));
    }
}

async fn send_event(event_tx: &mpsc::Sender<CoreEvent>, event: CoreEvent) {
    if event_tx.send(event).await.is_err() {
        warn!("turn event receiver disconnected");
    }
}

fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::now_v7())
}

fn now() -> i64 {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    i64::try_from(seconds).unwrap_or(i64::MAX)
}

pub(crate) fn compose_user_input(
    input: &[UserInput],
    workspace_root: Option<&str>,
) -> Result<String, CoreError> {
    let mut text = input
        .iter()
        .filter_map(UserInput::text)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let references = input
        .iter()
        .filter_map(|input| match input {
            UserInput::FileReference {
                path,
                start_line,
                end_line,
            } => Some((path, *start_line, *end_line)),
            UserInput::Text { .. } | UserInput::Image { .. } => None,
        })
        .collect::<Vec<_>>();
    if references.len() > MAX_FILE_REFERENCES {
        return Err(CoreError::FileReference(format!(
            "received {} references, exceeding the {MAX_FILE_REFERENCES}-reference limit",
            references.len()
        )));
    }
    if references.is_empty() {
        return Ok(text);
    }
    let root = workspace_root.ok_or_else(|| {
        CoreError::FileReference(String::from(
            "file references require a Thread with a Workspace Root",
        ))
    })?;
    let workspace = Workspace::open(Path::new(root)).map_err(CoreError::workspace)?;
    let mut normalized = Vec::with_capacity(references.len());
    for (path, start_line, end_line) in references {
        if path.is_empty()
            || path.len() > MAX_FILE_REFERENCE_PATH_BYTES
            || path.chars().any(char::is_control)
        {
            return Err(CoreError::FileReference(format!(
                "path must contain 1 through {MAX_FILE_REFERENCE_PATH_BYTES} non-control bytes"
            )));
        }
        match (start_line, end_line) {
            (None, None) => {}
            (Some(start), Some(end)) if start > 0 && start <= end => {}
            _ => {
                return Err(CoreError::FileReference(String::from(
                    "line range must contain 1-based startLine and endLine with startLine <= endLine",
                )));
            }
        }
        let relative = workspace
            .resolve_file_reference(path)
            .map_err(|error| CoreError::FileReference(error.to_string()))?;
        let mut reference = serde_json::json!({"path": relative});
        if let (Some(start), Some(end)) = (start_line, end_line) {
            reference["startLine"] = serde_json::json!(start);
            reference["endLine"] = serde_json::json!(end);
        }
        normalized.push(reference);
    }
    if !text.is_empty() {
        text.push_str("\n\n");
    }
    text.push_str(
        "Workspace file references (contents are not included; use read_file when needed):\n",
    );
    text.push_str(&serde_json::Value::Array(normalized).to_string());
    Ok(text)
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CoreError {
    #[error("Core is restoring a backup; retry after restoration completes")]
    RestoreBusy,
    #[error("timed out waiting for Core operations to stop; backup was not applied")]
    RestoreTimeout,
    #[error("thread not found: {0}")]
    ThreadNotFound(String),
    #[error("turn not found: {0}")]
    TurnNotFound(String),
    #[error("thread already has an active turn: {0}")]
    ThreadBusy(String),
    #[error("thread is archived: {0}")]
    ThreadArchived(String),
    #[error("turn input must contain non-empty text")]
    EmptyInput,
    #[error("turn input is {actual_bytes} bytes, exceeding the {max_bytes}-byte limit")]
    InputTooLarge {
        actual_bytes: usize,
        max_bytes: usize,
    },
    #[error("file reference is invalid: {0}")]
    FileReference(String),
    #[error("image input is invalid: {0}")]
    Media(String),
    #[error("workspace is invalid: {0}")]
    Workspace(String),
    #[error("workspace not found: {0}")]
    WorkspaceNotFound(String),
    #[error("configuration is invalid: {0}")]
    Config(String),
    #[error("model transport failed: {0}")]
    Model(String),
    #[error("thread storage failed: {0}")]
    Storage(String),
    #[error("MCP configuration failed: {0}")]
    Mcp(String),
    #[error("checkpoint is invalid: {0}")]
    Checkpoint(String),
}

impl CoreError {
    fn storage(error: impl std::fmt::Display) -> Self {
        Self::Storage(error.to_string())
    }

    fn workspace(error: impl std::fmt::Display) -> Self {
        Self::Workspace(error.to_string())
    }

    fn config(error: ModelConfigError) -> Self {
        Self::Config(error.to_string())
    }

    fn model(error: &crate::model::ModelError) -> Self {
        Self::Model(error.to_string())
    }

    fn mcp(error: impl std::fmt::Display) -> Self {
        Self::Mcp(error.to_string())
    }
}

fn protocol_config(config: &ModelConfig) -> CoreConfig {
    CoreConfig {
        base_url: config.base_url.clone(),
        default_model: config.default_model.clone(),
        api_key_configured: config.api_key.is_some(),
    }
}

#[cfg(test)]
#[path = "runtime_persistence_order_tests.rs"]
mod persistence_order_tests;

#[cfg(test)]
#[path = "runtime_final_persistence_tests.rs"]
mod final_persistence_tests;

#[cfg(test)]
mod security_snapshot_tests {
    use super::*;

    #[test]
    fn security_hot_reload_preserves_existing_snapshot() {
        let core = Core::new(ModelConfig {
            api_key: None,
            base_url: String::from("http://127.0.0.1:1"),
            default_model: String::from("qwen-test"),
        });
        let snapshot = core.security_policy();
        let mut settings = core.security_settings().expect("settings should read");
        settings.tool_guard.denied_tools = vec![String::from("shell")];
        core.replace_security_settings(settings)
            .expect("Security settings should hot-reload");

        assert_eq!(
            snapshot.evaluate(
                "shell",
                r#"{"command":"echo safe"}"#,
                SecurityApprovalMode::Auto,
            ),
            ToolGuardEffect::Allow
        );
        assert!(matches!(
            core.security_policy().evaluate(
                "shell",
                r#"{"command":"echo safe"}"#,
                SecurityApprovalMode::Auto,
            ),
            ToolGuardEffect::Deny(_)
        ));
    }

    #[test]
    fn concurrent_security_updates_keep_runtime_and_storage_consistent() {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let database = directory.path().join("security.sqlite3");
        let model = ModelConfig {
            api_key: None,
            base_url: String::from("http://127.0.0.1:1"),
            default_model: String::from("qwen-test"),
        };
        let core = Core::persistent(model.clone(), &database).expect("Core should open");
        let barrier = Arc::new(std::sync::Barrier::new(3));
        let handles = ["shell", "read_file"].map(|tool| {
            let core = core.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                let mut settings = core.security_settings().expect("settings should read");
                settings.tool_guard.denied_tools = vec![tool.to_owned()];
                barrier.wait();
                core.replace_security_settings(settings)
                    .expect("concurrent Security update should persist");
            })
        });
        barrier.wait();
        for handle in handles {
            handle.join().expect("Security update thread should finish");
        }
        let active = core
            .security_settings()
            .expect("active settings should read");
        drop(core);
        let reopened = Core::persistent(model, &database).expect("Core should reopen");
        assert_eq!(
            reopened
                .security_settings()
                .expect("stored settings should read"),
            active
        );
    }
}
