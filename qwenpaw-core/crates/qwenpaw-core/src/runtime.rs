use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use futures_util::StreamExt;
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
use qwenpaw_storage::StoredThread;
use qwenpaw_storage::StoredToolCall;
use qwenpaw_storage::ThreadStore;
use qwenpaw_tools::ApprovalRequirement;
use qwenpaw_tools::ToolCall;
use qwenpaw_tools::ToolOutput;
use qwenpaw_tools::Workspace;
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

const EVENT_CHANNEL_CAPACITY: usize = 64;
const DEFAULT_LIST_LIMIT: u32 = 50;
const MAX_LIST_LIMIT: usize = 200;
const MAX_AGENT_STEPS: usize = 8;
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
const BASE_URL_SETTING: &str = "base_url";
const DEFAULT_MODEL_SETTING: &str = "default_model";
const PREFERRED_WORKSPACE_SETTING: &str = "preferred_workspace";
const CODING_MODE_SETTING: &str = "coding_mode";
const UI_LANGUAGE_SETTING: &str = "ui_language";
const DEFAULT_UI_LANGUAGE: &str = "en";
const SUPPORTED_UI_LANGUAGES: [&str; 7] = ["en", "zh", "ja", "ru", "pt-BR", "id", "vi"];

pub type TurnEventStream = mpsc::Receiver<CoreEvent>;

#[derive(Clone)]
pub struct Core {
    inner: Arc<CoreInner>,
}

struct CoreInner {
    model: ModelClient,
    mcp: McpManager,
    store: ThreadStore,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    threads: HashMap<String, ThreadRecord>,
    approvals: HashMap<String, PendingApproval>,
}

struct ThreadRecord {
    thread: Thread,
    turns: Vec<Turn>,
    messages: Vec<StoredMessage>,
    active_turn: Option<ActiveTurn>,
}

struct ActiveTurn {
    id: String,
    cancellation: CancellationToken,
}

struct PendingApproval {
    thread_id: String,
    turn_id: String,
    sender: oneshot::Sender<ApprovalDecision>,
}

impl Core {
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
        let snapshots = store.load_all().map_err(CoreError::storage)?;
        let mut threads = HashMap::new();
        for mut snapshot in snapshots {
            recover_interrupted_turns(&mut snapshot);
            ensure_system_message(&mut snapshot);
            store.upsert(&snapshot).map_err(CoreError::storage)?;
            threads.insert(
                snapshot.thread.id.clone(),
                ThreadRecord {
                    thread: snapshot.thread,
                    turns: snapshot.turns,
                    messages: snapshot.messages,
                    active_turn: None,
                },
            );
        }
        Ok(Self {
            inner: Arc::new(CoreInner {
                model: ModelClient::new(model_config).map_err(|error| CoreError::model(&error))?,
                mcp,
                store,
                state: Mutex::new(State {
                    threads,
                    approvals: HashMap::new(),
                }),
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
        let workspace_path = match params.workspace_root {
            Some(path) => std::path::PathBuf::from(path),
            None => std::env::current_dir().map_err(CoreError::workspace)?,
        };
        let workspace = Workspace::open(&workspace_path).map_err(CoreError::workspace)?;
        let timestamp = now();
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
            thread: thread.clone(),
            turns: Vec::new(),
            messages: vec![StoredMessage::text("system", SYSTEM_PROMPT)],
            active_turn: None,
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

    /// Validates, persists, and applies non-secret model configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when a value is invalid or SQLite persistence fails.
    pub fn write_config(
        &self,
        params: ConfigWriteParams,
    ) -> Result<ConfigWriteResponse, CoreError> {
        let current = self.inner.model.config_snapshot();
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
        self.inner.model.replace_config(next.clone());
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
        let current = self.inner.model.config_snapshot();
        let next = ModelConfig {
            api_key,
            base_url: current.base_url,
            default_model: current.default_model,
        }
        .normalize()
        .map_err(CoreError::config)?;
        self.inner.model.replace_config(next);
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
        let language = language.trim();
        validate_ui_language(language)?;
        self.inner
            .store
            .write_settings(&[(UI_LANGUAGE_SETTING, language)])
            .map_err(CoreError::storage)?;
        Ok(language.to_owned())
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
        let thread_id = params.thread_id;
        let workspace_root = {
            let state = self.inner.state.lock().await;
            let record = state
                .threads
                .get(&thread_id)
                .ok_or_else(|| CoreError::ThreadNotFound(thread_id.clone()))?;
            if record.thread.archived {
                return Err(CoreError::ThreadArchived(thread_id));
            }
            if record.active_turn.is_some() {
                return Err(CoreError::ThreadBusy(thread_id));
            }
            record.thread.workspace_root.clone()
        };
        let text = compose_user_input(&params.input, workspace_root.as_deref())?;
        if text.is_empty() {
            return Err(CoreError::EmptyInput);
        }
        if text.len() > MAX_TURN_INPUT_BYTES {
            return Err(CoreError::InputTooLarge {
                actual_bytes: text.len(),
                max_bytes: MAX_TURN_INPUT_BYTES,
            });
        }
        let turn_id = new_id("turn");
        let turn = Turn {
            id: turn_id.clone(),
            thread_id: thread_id.clone(),
            status: TurnStatus::InProgress,
            items: vec![Item::UserMessage {
                id: new_id("item"),
                text: text.clone(),
            }],
            error: None,
        };
        let cancellation = CancellationToken::new();
        let snapshot = {
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
            record.messages.push(StoredMessage::text("user", text));
            record.turns.push(turn.clone());
            record.thread.status = ThreadStatus::Active;
            record.thread.updated_at = now();
            record.active_turn = Some(ActiveTurn {
                id: turn_id.clone(),
                cancellation: cancellation.clone(),
            });
            record.snapshot()
        };
        self.inner
            .store
            .upsert(&snapshot)
            .map_err(CoreError::storage)?;
        let (event_tx, event_rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        let core = self.clone();
        tokio::spawn(async move {
            core.run_turn(thread_id, turn_id, cancellation, event_tx)
                .await;
        });
        Ok((TurnStartResponse { turn }, event_rx))
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
        self.inner.mcp.clients().await.map_err(CoreError::mcp)
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
        self.inner
            .mcp
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
        self.inner
            .mcp
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
        self.inner
            .mcp
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
    ) {
        let Some((turn, model, workspace_root)) = self.turn_context(&thread_id, &turn_id).await
        else {
            return;
        };
        send_event(
            &event_tx,
            CoreEvent::TurnStarted(TurnStartedNotification { turn }),
        )
        .await;
        let workspace = match Workspace::open(Path::new(&workspace_root)) {
            Ok(workspace) => workspace,
            Err(error) => {
                self.finish_turn(
                    &thread_id,
                    &turn_id,
                    TurnOutcome::Failed(error.to_string()),
                    &event_tx,
                )
                .await;
                return;
            }
        };
        let mut tools = qwenpaw_tools::definitions();
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
            definitions = self.inner.mcp.definitions() => definitions,
        };
        tools.extend(mcp_tools);
        let mut outcome = TurnOutcome::Failed(String::from("agent exceeded maximum steps"));
        for _ in 0..MAX_AGENT_STEPS {
            let messages = self.messages_snapshot(&thread_id).await;
            let step = self
                .run_model_step(
                    ModelStepRequest {
                        thread_id: &thread_id,
                        turn_id: &turn_id,
                        model: &model,
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
                    &thread_id,
                    &turn_id,
                    &workspace,
                    step.tool_calls,
                    &cancellation,
                    &event_tx,
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

    async fn execute_tool_calls(
        &self,
        thread_id: &str,
        turn_id: &str,
        workspace: &Workspace,
        calls: Vec<ToolCall>,
        cancellation: &CancellationToken,
        event_tx: &mpsc::Sender<CoreEvent>,
    ) -> Result<(), TurnOutcome> {
        for call in calls {
            let requires_approval = Workspace::approval_requirement(&call)
                == ApprovalRequirement::Required
                || self.inner.mcp.contains_tool(&call.name).await;
            let output = if requires_approval {
                match self
                    .request_approval(thread_id, turn_id, workspace, &call, cancellation, event_tx)
                    .await
                {
                    ApprovalOutcome::Approved => {
                        self.execute_tool(workspace, &call, cancellation).await?
                    }
                    ApprovalOutcome::Denied => Ok(ToolOutput {
                        content: String::from("Tool execution was denied by the user."),
                        is_error: true,
                    }),
                    ApprovalOutcome::Interrupted => return Err(TurnOutcome::Interrupted),
                }
            } else {
                self.execute_tool(workspace, &call, cancellation).await?
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
            model,
            messages,
            tools,
        } = request;
        let stream = tokio::select! {
            () = cancellation.cancelled() => return Err(ModelStepError::Interrupted),
            stream = self.inner.model.chat_stream(model, messages, tools) => stream,
        };
        let mut stream = stream.map_err(|error| ModelStepError::Failed(error.to_string()))?;
        let agent_item_id = new_id("item");
        let mut agent_started = false;
        let mut text = String::new();
        let mut calls = BTreeMap::<usize, ToolCallBuilder>::new();
        loop {
            tokio::select! {
                () = cancellation.cancelled() => return Err(ModelStepError::Interrupted),
                event = stream.next() => {
                    match event {
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
                        Some(Err(error)) => {
                            return Err(ModelStepError::Failed(error.to_string()));
                        }
                        None => break,
                    }
                }
            }
        }
        let tool_calls = calls
            .into_values()
            .map(ToolCallBuilder::build)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ModelStep {
            agent_item_id,
            text,
            tool_calls,
        })
    }

    async fn execute_tool(
        &self,
        workspace: &Workspace,
        call: &ToolCall,
        cancellation: &CancellationToken,
    ) -> Result<Result<ToolOutput, String>, TurnOutcome> {
        if self.inner.mcp.contains_tool(&call.name).await {
            return tokio::select! {
                () = cancellation.cancelled() => {
                    self.inner.mcp.cancel_tool(&call.name).await;
                    Err(TurnOutcome::Interrupted)
                }
                output = self.inner.mcp.call_tool(&call.name, &call.arguments) => {
                    Ok(output
                        .map(|output| ToolOutput {
                            content: output.content,
                            is_error: output.is_error,
                        })
                        .map_err(|error| error.to_string()))
                }
            };
        }
        tokio::select! {
            () = cancellation.cancelled() => Err(TurnOutcome::Interrupted),
            output = workspace.execute(call) => Ok(output.map_err(|error| error.to_string())),
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
            record.messages.push(StoredMessage::assistant_tool_calls(
                step.text.clone(),
                stored_calls,
            ));
            if let Some(item) = &agent_item {
                turn.items.push(item.clone());
            }
            turn.items.extend(tool_items.clone());
            record.snapshot()
        };
        self.inner
            .store
            .upsert(&snapshot)
            .map_err(CoreError::storage)?;
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
            record
                .messages
                .push(StoredMessage::tool_result(call.id.clone(), output.content));
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

    async fn finish_turn(
        &self,
        thread_id: &str,
        turn_id: &str,
        outcome: TurnOutcome,
        event_tx: &mpsc::Sender<CoreEvent>,
    ) {
        let (completed_turn, snapshot) = {
            let mut state = self.inner.state.lock().await;
            let Some(record) = state.threads.get_mut(thread_id) else {
                return;
            };
            let Some(turn) = record.turns.iter_mut().find(|turn| turn.id == turn_id) else {
                return;
            };
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
            record.thread.updated_at = now();
            record.active_turn = None;
            let completed_turn = turn.clone();
            (completed_turn, record.snapshot())
        };
        if let Err(error) = self.inner.store.upsert(&snapshot) {
            warn!(%error, "failed to persist completed turn");
        }
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

#[derive(Debug)]
struct ModelStep {
    agent_item_id: String,
    text: String,
    tool_calls: Vec<ToolCall>,
}

struct ModelStepRequest<'a> {
    thread_id: &'a str,
    turn_id: &'a str,
    model: &'a str,
    messages: &'a [StoredMessage],
    tools: &'a [serde_json::Value],
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

impl ThreadRecord {
    fn snapshot(&self) -> StoredThread {
        StoredThread {
            thread: self.thread.clone(),
            turns: self.turns.clone(),
            messages: self.messages.clone(),
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
            UserInput::Text { .. } => None,
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
