use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::io::Seek;
use std::io::Write;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use axum::Json;
use axum::Router;
use axum::extract::Query;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::routing::delete;
use axum::routing::get;
use axum::routing::patch;
use axum::routing::post;
use qwenpaw_core::ThreadCheckpoint;
use qwenpaw_protocol::Item;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use sha2::Digest;
use sha2::Sha256;
use tempfile::NamedTempFile;
use uuid::Uuid;
use zip::CompressionMethod;
use zip::ZipArchive;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use super::AppServer;
use super::desktop_agents::AgentContext;
use super::desktop_agents::identity::WorkspaceDataKey;
use super::desktop_chats::CheckpointSessionInfo;

#[path = "desktop_checkpoint_runtime.rs"]
pub(crate) mod runtime;

#[path = "desktop_checkpoint_quiescence.rs"]
pub(crate) mod quiescence;

const STATE_VERSION: u32 = 2;
const MAX_SNAPSHOT_IDENTITY_BYTES: u64 = 1024 * 1024;
const MAX_STATE_BYTES: u64 = 8 * 1_024 * 1_024;
const MAX_CHECKPOINTS: usize = 5_000;
const MAX_SNAPSHOT_FILES: usize = 100_000;
const MAX_SNAPSHOT_BYTES: u64 = 512 * 1_024 * 1_024;
const MAX_THREAD_BYTES: usize = 32 * 1_024 * 1_024;
const MAX_ID_BYTES: usize = 1_024;
const MAX_NAME_CHARS: usize = 200;
const DEFAULT_GC_KEEP_COUNT: u32 = 20;
const DEFAULT_GC_KEEP_DAYS: u32 = 7;
const DEFAULT_PRE_RESTORE_DAYS: u32 = 7;
const MAX_GC_KEEP_COUNT: u32 = 1_000_000;
const MAX_GC_DAYS: u32 = 36_500;
const MILLIS_PER_DAY: u64 = 86_400_000;

type ApiError = (StatusCode, Json<Value>);

#[path = "desktop_checkpoint_backup_restore.rs"]
pub(super) mod backup_restore;

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route("/api/workspace/checkpoints/status", get(status))
        .route("/api/workspace/checkpoints/graph", get(graph))
        .route("/api/workspace/checkpoints/auto", patch(set_auto))
        .route("/api/workspace/checkpoints/snapshot", post(snapshot))
        .route(
            "/api/workspace/checkpoints/restore/preview",
            post(preview_restore),
        )
        .route("/api/workspace/checkpoints/restore", post(restore))
        .route("/api/workspace/checkpoints/gc/preview", post(preview_gc))
        .route("/api/workspace/checkpoints/gc", post(gc))
        .route(
            "/api/workspace/checkpoints/gc/settings",
            get(get_gc_settings).patch(update_gc_settings),
        )
        .route("/api/workspace/checkpoints", delete(reset))
}

#[derive(Clone)]
struct WorkspaceContext {
    root: PathBuf,
    root_text: String,
    state_dir: PathBuf,
    control_dir: PathBuf,
    identity: CheckpointIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckpointIdentity {
    data_key: WorkspaceDataKey,
    workspace_root: String,
}

impl CheckpointIdentity {
    fn is_valid(&self) -> bool {
        self.data_key.is_valid()
            && !self.workspace_root.is_empty()
            && self.workspace_root.len() <= 256 * 1024
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotIdentity {
    version: u32,
    id: Uuid,
    workspace: CheckpointIdentity,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckpointState {
    version: u32,
    identity: CheckpointIdentity,
    auto_enabled: bool,
    gc_keep_count: u32,
    gc_keep_days: u32,
    pre_restore_retention_days: u32,
    heads: HashMap<String, String>,
    entries: Vec<CheckpointEntry>,
}

impl CheckpointState {
    fn new(identity: CheckpointIdentity) -> Self {
        Self {
            version: STATE_VERSION,
            identity,
            auto_enabled: false,
            gc_keep_count: DEFAULT_GC_KEEP_COUNT,
            gc_keep_days: DEFAULT_GC_KEEP_DAYS,
            pre_restore_retention_days: DEFAULT_PRE_RESTORE_DAYS,
            heads: HashMap::new(),
            entries: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckpointEntry {
    ref_name: String,
    kind: String,
    session_key: String,
    name: String,
    commit: String,
    timestamp_ms: u64,
    subject: String,
    query: Option<String>,
    channel: String,
    restore_index: Option<u32>,
    parent_commit: Option<String>,
    user_id: String,
    session_id: String,
    thread_id: String,
}

#[derive(Debug, Default, Deserialize)]
struct GraphQuery {
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AutoRequest {
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotRequest {
    session_id: String,
    #[serde(default)]
    user_id: String,
    #[serde(default = "default_channel")]
    channel: String,
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RestoreRequest {
    commit: String,
    session_id: String,
    #[serde(default)]
    user_id: String,
    #[serde(default = "default_channel")]
    channel: String,
    #[serde(default)]
    include_memory: bool,
    #[serde(default)]
    include_files: bool,
    #[serde(default)]
    files: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct GcRequest {
    compact: bool,
    keep_count: Option<u32>,
    keep_days: Option<u32>,
    pre_restore_days: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GcSettings {
    gc_keep_count: u32,
    gc_keep_days: u32,
    pre_restore_retention_days: u32,
}

struct PreparedRestore {
    checkpoint: ThreadCheckpoint,
    restored_paths: Vec<String>,
    deleted_paths: Vec<String>,
    file_paths: Vec<String>,
}

struct SnapshotIndex {
    checkpoint: ThreadCheckpoint,
    file_hashes: HashMap<String, String>,
}

async fn status(
    State(server): State<AppServer>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let (_lifecycle, _, context) = selected_context(&server, &headers).await?;
    let _guard = server.inner.desktop_checkpoint_lock.lock().await;
    let state = read_state_async(context.clone()).await?;
    Ok(Json(json!({
        "auto_enabled": state.auto_enabled,
        "has_checkpoints": !state.entries.is_empty(),
        "workspace_dir": context.root_text
    })))
}

async fn graph(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Query(query): Query<GraphQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(500);
    if !(1..=1_000).contains(&limit) {
        return Err(unprocessable("limit must be between 1 and 1000"));
    }
    let (_lifecycle, agent, context) = selected_context(&server, &headers).await?;
    let _guard = server.inner.desktop_checkpoint_lock.lock().await;
    let state = read_state_async(context.clone()).await?;
    let sessions = super::desktop_chats::bound_checkpoint_sessions(&server, &agent).await?;
    let titles = sessions
        .iter()
        .map(|session| {
            (
                (
                    session.channel.as_str(),
                    session.user_id.as_str(),
                    session.session_id.as_str(),
                ),
                session.title.as_str(),
            )
        })
        .collect::<HashMap<_, _>>();
    let mut entries = state.entries.clone();
    entries.sort_by(|left, right| {
        right
            .timestamp_ms
            .cmp(&left.timestamp_ms)
            .then_with(|| right.commit.cmp(&left.commit))
    });
    let heads = state.heads.values().cloned().collect::<HashSet<_>>();
    // Match the original Console contract, which treats a full page as
    // potentially truncated even when the total happens to equal the limit.
    let truncated = entries.len() >= limit;
    entries.truncate(limit);
    let nodes = entries
        .iter()
        .map(|entry| {
            checkpoint_node(
                entry,
                &heads,
                titles
                    .get(&(
                        entry.channel.as_str(),
                        entry.user_id.as_str(),
                        entry.session_id.as_str(),
                    ))
                    .copied(),
            )
        })
        .collect::<Vec<_>>();
    let session_values = sessions
        .into_iter()
        .map(|session| {
            json!({
                "session_key": checkpoint_session_key(
                    &session.channel,
                    &session.user_id,
                    &session.session_id,
                ),
                "session_id": session.session_id,
                "user_id": session.user_id,
                "channel": session.channel,
                "title": session.title,
                "archived": session.archived
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "summary": {
            "total": nodes.len(),
            "auto": entries.iter().filter(|entry| entry.kind == "auto").count(),
            "snapshots": entries.iter().filter(|entry| entry.kind == "snap").count(),
            "safety": entries.iter().filter(|entry| entry.kind == "pre-restore").count(),
            "heads": entries.iter().filter(|entry| heads.contains(entry.commit.as_str())).count()
        },
        "nodes": nodes,
        "sessions": session_values,
        "truncated": truncated
    })))
}

async fn set_auto(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Json(request): Json<AutoRequest>,
) -> Result<Json<Value>, ApiError> {
    let (_lifecycle, _, context) = selected_context(&server, &headers).await?;
    let _guard = server.inner.desktop_checkpoint_lock.lock().await;
    let mut state = read_state_async(context.clone()).await?;
    state.auto_enabled = request.enabled;
    write_state_async(context.state_dir, state).await?;
    if !request.enabled {
        runtime::cancel_workspace(&server, &context.identity.data_key);
    }
    Ok(Json(json!({"auto_enabled": request.enabled})))
}

async fn snapshot(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Json(request): Json<SnapshotRequest>,
) -> Result<Json<Value>, ApiError> {
    validate_snapshot_request(&request)?;
    let (_lifecycle, agent, context) = selected_context(&server, &headers).await?;
    let _guard = server.inner.desktop_checkpoint_lock.lock().await;
    let session = resolve_session(&server, &agent, &request).await?;
    let checkpoint = server
        .inner
        .core
        .export_thread_checkpoint(&session.thread_id)
        .await
        .map_err(core_error)?;
    let entry = create_snapshot_async(
        context,
        checkpoint,
        session,
        String::from("snap"),
        request.name,
        None,
    )
    .await?;
    Ok(Json(json!({"ref": entry.ref_name, "commit": entry.commit})))
}

pub(super) fn auto_snapshot_eligible(turn: &qwenpaw_protocol::Turn, query: Option<&str>) -> bool {
    turn.status == qwenpaw_protocol::TurnStatus::Completed
        && query.is_none_or(|text| {
            // Python str.lstrip also includes the four information separators.
            !text
                .trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'))
                .starts_with('/')
        })
}

pub(super) fn console_query(input: &[Value]) -> Option<String> {
    let content = input.last()?.get("content")?;
    let text = if let Some(text) = content.as_str() {
        text.to_owned()
    } else {
        content
            .as_array()?
            .iter()
            .filter_map(|part| {
                (part["type"] == "text")
                    .then(|| part["text"].as_str())
                    .flatten()
            })
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    };
    (!text.is_empty()).then_some(text)
}

pub(super) async fn schedule_auto_checkpoint(
    server: &AppServer,
    thread_id: &str,
    turn_id: &str,
    agent: &AgentContext,
    query: Option<String>,
    cancellation: &tokio_util::sync::CancellationToken,
) {
    if !server
        .inner
        .core
        .turn_was_persisted(thread_id, turn_id)
        .await
    {
        return;
    }
    let _guard = server.inner.desktop_checkpoint_lock.lock().await;
    if cancellation.is_cancelled() {
        return;
    }
    let Ok(context) = context_for_root(server, &agent.workspace, &agent.data_key) else {
        return;
    };
    let Ok(state) = read_state_async(context).await else {
        return;
    };
    if !state.auto_enabled {
        return;
    }
    let Ok(sessions) = super::desktop_chats::bound_checkpoint_sessions(server, agent).await else {
        return;
    };
    let Some(session) = sessions
        .into_iter()
        .find(|session| session.thread_id == thread_id)
    else {
        return;
    };
    let _ = runtime::enqueue(
        server,
        agent,
        thread_id,
        checkpoint_session_key(&session.channel, &session.user_id, &session.session_id),
        query,
        cancellation,
    );
}

pub(super) async fn maybe_create_auto_checkpoint(
    server: &AppServer,
    thread_id: &str,
    agent: &AgentContext,
    query: Option<String>,
    cancellation: &tokio_util::sync::CancellationToken,
) {
    // Completion must not reacquire lifecycle admission: deletion may already
    // hold it while waiting for this run to drain.
    let Some(_guard) = quiescence::admit_auto(server, &agent.data_key, cancellation).await else {
        return;
    };
    if cancellation.is_cancelled() {
        return;
    }
    let Ok(context) = context_for_root(server, &agent.workspace, &agent.data_key) else {
        return;
    };
    let Ok(state) = read_state_async(context.clone()).await else {
        return;
    };
    if !state.auto_enabled {
        return;
    }
    let Ok(sessions) = super::desktop_chats::bound_checkpoint_sessions(server, agent).await else {
        return;
    };
    let Some(session) = sessions
        .into_iter()
        .find(|session| session.thread_id == thread_id)
    else {
        return;
    };
    let Ok(checkpoint) = server.inner.core.export_thread_checkpoint(thread_id).await else {
        return;
    };
    let result = create_snapshot_async(
        context.clone(),
        checkpoint,
        session,
        String::from("auto"),
        String::new(),
        query,
    )
    .await;
    if let Err(error) = result {
        tracing::warn!(detail = %error.1.0, "automatic checkpoint failed");
    } else if !quiescence::is_paused(server, &agent.data_key)
        && server
            .inner
            .desktop_checkpoint_runtime
            .gc_due(&agent.data_key)
    {
        let thread_id = thread_id.to_owned();
        let result = tokio::task::spawn_blocking(move || automatic_gc(&context, &thread_id)).await;
        if !matches!(result, Ok(Ok(()))) {
            tracing::warn!("automatic checkpoint cleanup failed");
        }
    }
}

fn automatic_gc(context: &WorkspaceContext, thread: &str) -> Result<(), ApiError> {
    let state = read_state(&context.state_dir, &context.identity)?;
    let (deleted, _) = gc_selection(
        &state,
        state.gc_keep_count,
        state.gc_keep_days,
        state.pre_restore_retention_days,
        false,
        Some(thread),
    );
    let deleted = deleted
        .into_iter()
        .map(|entry| entry.commit)
        .collect::<HashSet<_>>();
    prune_commits(context, state, &deleted)
}

pub(super) struct SessionDeletion {
    context: WorkspaceContext,
    state: CheckpointState,
}

/// The caller holds lifecycle and checkpoint locks through Thread deletion.
pub(super) async fn prepare_session_deletion(
    server: &AppServer,
    agent: &AgentContext,
) -> Result<SessionDeletion, ApiError> {
    let context = context_for_root(server, &agent.workspace, &agent.data_key)?;
    let state = read_state_async(context.clone()).await?;
    Ok(SessionDeletion { context, state })
}

impl SessionDeletion {
    pub(super) async fn apply(
        self,
        server: &AppServer,
        threads: &[String],
    ) -> Result<(), ApiError> {
        runtime::cancel_threads(server, &self.context.identity.data_key, threads);
        let deleted = self
            .state
            .entries
            .iter()
            .filter(|entry| threads.contains(&entry.thread_id))
            .map(|entry| entry.commit.clone())
            .collect::<HashSet<_>>();
        tokio::task::spawn_blocking(move || prune_commits(&self.context, self.state, &deleted))
            .await
            .map_err(|_| internal("Checkpoint session cleanup task failed"))?
    }
}

fn prune_commits(
    context: &WorkspaceContext,
    mut state: CheckpointState,
    deleted: &HashSet<String>,
) -> Result<(), ApiError> {
    if deleted.is_empty() {
        return Ok(());
    }
    state
        .entries
        .retain(|entry| !deleted.contains(&entry.commit));
    state.heads.retain(|_, commit| !deleted.contains(commit));
    write_state(&context.state_dir, &state)?;
    for commit in deleted {
        let path = context
            .state_dir
            .join("snapshots")
            .join(format!("{commit}.zip"));
        if let Err(error) = fs::remove_file(path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%error, "unreferenced checkpoint archive could not be removed");
        }
    }
    Ok(())
}

async fn preview_restore(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Json(request): Json<RestoreRequest>,
) -> Result<Json<Value>, ApiError> {
    restore_impl(&server, &headers, request, true).await
}

async fn restore(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Json(request): Json<RestoreRequest>,
) -> Result<Json<Value>, ApiError> {
    restore_impl(&server, &headers, request, false).await
}

async fn restore_impl(
    server: &AppServer,
    headers: &HeaderMap,
    request: RestoreRequest,
    dry_run: bool,
) -> Result<Json<Value>, ApiError> {
    validate_restore_request(&request, dry_run)?;
    let (lifecycle, agent, context) = selected_context(server, headers).await?;
    let mut lifecycle = Some(lifecycle);
    let cron = server.inner.desktop_cron_lock.lock().await;
    let checkpoint_guard = server.inner.desktop_checkpoint_lock.lock().await;
    let state = read_state_async(context.clone()).await?;
    let target = resolve_checkpoint(&state, &request.commit)?.clone();
    if target.session_id != request.session_id
        || target.user_id != request.user_id
        || target.channel != request.channel
    {
        return Err(bad_request(
            "Checkpoint does not belong to the requested session",
        ));
    }
    let sessions = super::desktop_chats::bound_checkpoint_sessions(server, &agent).await?;
    let session = sessions
        .into_iter()
        .find(|session| {
            session.thread_id == target.thread_id
                && session.session_id == request.session_id
                && session.user_id == request.user_id
                && session.channel == request.channel
        })
        .ok_or_else(|| not_found("Checkpoint session was not found in this Workspace"))?;
    let (_restoration, _checkpoint_guard) = if dry_run {
        drop(cron);
        (None, checkpoint_guard)
    } else {
        let frozen = quiescence::freeze(server, &agent)?;
        drop(checkpoint_guard);
        drop(cron);
        drop(lifecycle.take());
        let frozen = frozen
            .drain(&agent, std::time::Duration::from_secs(30))
            .await?;
        (
            Some(frozen),
            server.inner.desktop_checkpoint_lock.lock().await,
        )
    };
    let context = context_for_root(server, &agent.workspace, &agent.data_key)?;
    let memory_directories = super::desktop_agent_settings::memory_directories_for_agent(&agent)?;
    let prepare_memory_directories = memory_directories.clone();
    let prepare_context = context.clone();
    let prepare_target = target.clone();
    let include_memory = request.include_memory;
    let include_files = request.include_files;
    let prepared = tokio::task::spawn_blocking(move || {
        prepare_restore_sync(
            &prepare_context,
            &prepare_target,
            &prepare_memory_directories,
            include_memory,
            include_files,
        )
    })
    .await
    .map_err(|error| internal(&format!("Checkpoint restore task failed: {error}")))??;
    let response = restore_value(
        &target,
        &prepared,
        dry_run,
        request.include_memory,
        request.include_files,
        None,
        None,
    );
    if dry_run {
        return Ok(Json(response));
    }
    apply_prepared_restore(
        server,
        context,
        target,
        session,
        memory_directories,
        request,
        prepared,
    )
    .await
}

async fn apply_prepared_restore(
    server: &AppServer,
    context: WorkspaceContext,
    target: CheckpointEntry,
    session: CheckpointSessionInfo,
    memory_directories: (PathBuf, PathBuf),
    request: RestoreRequest,
    prepared: PreparedRestore,
) -> Result<Json<Value>, ApiError> {
    let requested_files = request.files.unwrap_or_default();
    let allowed = prepared.file_paths.iter().collect::<HashSet<_>>();
    if requested_files.iter().any(|path| !allowed.contains(path)) {
        return Err(bad_request(
            "Selected restore files do not match the preview",
        ));
    }
    let selected_files = requested_files.into_iter().collect::<HashSet<_>>();
    let mut mutation_paths = prepared
        .restored_paths
        .iter()
        .chain(prepared.deleted_paths.iter())
        .filter(|path| path.as_str() != conversation_path(&target))
        .filter(|path| request.include_memory && is_memory_path(path, &memory_directories))
        .cloned()
        .collect::<HashSet<_>>();
    if request.include_files {
        mutation_paths.extend(selected_files.iter().cloned());
    }
    let validation_root = context.root.clone();
    let validation_paths = mutation_paths.clone();
    tokio::task::spawn_blocking(move || {
        for path in validation_paths {
            inspect_workspace_target(&validation_root, &relative_path_buf(&path)?)?;
        }
        Ok::<(), ApiError>(())
    })
    .await
    .map_err(|error| internal(&format!("Checkpoint restore task failed: {error}")))??;
    let current_checkpoint = server
        .inner
        .core
        .export_thread_checkpoint(&session.thread_id)
        .await
        .map_err(core_error)?;
    let safety = create_snapshot_async(
        context.clone(),
        current_checkpoint,
        session,
        String::from("pre-restore"),
        format!("Before restore to {}", target.commit),
        None,
    )
    .await?;
    let mut state = read_state_async(context.clone()).await?;
    let apply_context = context.clone();
    let apply_target = target.clone();
    let apply_paths = mutation_paths.clone();
    let mut files = tokio::task::spawn_blocking(move || {
        apply_archive_paths_sync(&apply_context, &apply_target, &apply_paths)
    })
    .await
    .map_err(|error| internal(&format!("Checkpoint restore task failed: {error}")))??;
    let applied_response = restore_value(
        &target,
        &prepared,
        false,
        request.include_memory,
        request.include_files,
        Some(&safety.ref_name),
        Some(&selected_files),
    );
    let rollback_state = state.clone();
    state
        .heads
        .insert(target.session_key.clone(), target.commit.clone());
    if let Err(error) = write_state_async(context.state_dir.clone(), state).await {
        rollback_restored_files(files).await?;
        return Err(error);
    }
    if let Err(error) = server
        .inner
        .core
        .restore_thread_checkpoint(&target.thread_id, prepared.checkpoint)
        .await
    {
        let file_rollback = rollback_restored_files(files).await;
        let head_rollback = write_state_async(context.state_dir.clone(), rollback_state).await;
        file_rollback?;
        head_rollback?;
        return Err(core_error(error));
    }
    // Once the Thread is durable, dropping a pending cleanup task must never
    // undo only the files. Mark the file transaction committed before await.
    files.commit();
    if tokio::task::spawn_blocking(move || drop(files))
        .await
        .is_err()
    {
        tracing::warn!("Checkpoint restore committed but recovery cleanup did not finish");
    }
    Ok(Json(applied_response))
}

async fn rollback_restored_files(
    mut files: super::desktop_restore_files::RestoreFiles,
) -> Result<(), ApiError> {
    tokio::task::spawn_blocking(move || {
        files.rollback().map_err(|_| {
            internal("Checkpoint file rollback failed; recovery data has been retained")
        })
    })
    .await
    .map_err(|_| internal("Checkpoint file rollback task failed"))?
}

async fn preview_gc(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Json(request): Json<GcRequest>,
) -> Result<Json<Value>, ApiError> {
    gc_impl(&server, &headers, request, true).await
}

async fn gc(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Json(request): Json<GcRequest>,
) -> Result<Json<Value>, ApiError> {
    gc_impl(&server, &headers, request, false).await
}

async fn gc_impl(
    server: &AppServer,
    headers: &HeaderMap,
    request: GcRequest,
    dry_run: bool,
) -> Result<Json<Value>, ApiError> {
    validate_gc_request(&request)?;
    let (_lifecycle, _, context) = selected_context(server, headers).await?;
    let _guard = server.inner.desktop_checkpoint_lock.lock().await;
    let mut state = read_state_async(context.clone()).await?;
    let keep_count = request.keep_count.unwrap_or(state.gc_keep_count);
    let keep_days = request.keep_days.unwrap_or(state.gc_keep_days);
    let pre_restore_days = request
        .pre_restore_days
        .unwrap_or(state.pre_restore_retention_days);
    let (deleted, kept) = gc_selection(
        &state,
        keep_count,
        keep_days,
        pre_restore_days,
        request.compact,
        None,
    );
    if !dry_run {
        let deleted_commits = deleted
            .iter()
            .map(|entry| entry.commit.as_str())
            .collect::<HashSet<_>>();
        state
            .entries
            .retain(|entry| !deleted_commits.contains(entry.commit.as_str()));
        let live = state
            .entries
            .iter()
            .map(|entry| format!("{}.zip", entry.commit))
            .collect::<HashSet<_>>();
        write_state_async(context.state_dir.clone(), state).await?;
        let archives = context.state_dir.join("snapshots");
        let paths = deleted
            .iter()
            .map(|entry| archives.join(format!("{}.zip", entry.commit)))
            .collect::<Vec<_>>();
        let compact = request.compact;
        tokio::task::spawn_blocking(move || {
            for path in paths {
                match fs::remove_file(path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        tracing::warn!(%error, "unused checkpoint archive could not be removed");
                    }
                }
            }
            if compact && let Ok(reader) = fs::read_dir(&archives) {
                for item in reader.flatten() {
                    let name = item.file_name().to_string_lossy().into_owned();
                    let path = item.path();
                    if Path::new(&name)
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
                        && !live.contains(&name)
                        && path.symlink_metadata().is_ok_and(|metadata| {
                            metadata.is_file() && !metadata.file_type().is_symlink()
                        })
                    {
                        let _ = fs::remove_file(path);
                    }
                }
            }
        })
        .await
        .map_err(|error| internal(&format!("Checkpoint GC task failed: {error}")))?;
    }
    Ok(Json(json!({
        "deleted_refs": deleted.iter().map(|entry| entry.ref_name.as_str()).collect::<Vec<_>>(),
        "kept_refs": kept.iter().map(|entry| entry.ref_name.as_str()).collect::<Vec<_>>(),
        "dry_run": dry_run
    })))
}

fn validate_restore_request(request: &RestoreRequest, dry_run: bool) -> Result<(), ApiError> {
    if request.commit.len() < 7
        || request.commit.len() > 1_024
        || request.commit.chars().any(char::is_control)
        || !valid_identifier(&request.session_id)
        || !valid_identifier_or_empty(&request.user_id)
        || !valid_identifier(&request.channel)
    {
        return Err(unprocessable("Checkpoint restore request is invalid"));
    }
    let files = request.files.as_deref().unwrap_or_default();
    if files.len() > MAX_SNAPSHOT_FILES {
        return Err(payload_too_large(
            "Checkpoint restore contains more than 100000 files",
        ));
    }
    let mut unique = HashSet::new();
    for path in files {
        validate_relative_path(path)?;
        if !unique.insert(path) {
            return Err(unprocessable(
                "Checkpoint restore contains duplicate file paths",
            ));
        }
    }
    if !dry_run && request.include_files && files.is_empty() {
        return Err(bad_request(
            "Select at least one file before restoring files.",
        ));
    }
    if !request.include_files && !files.is_empty() {
        return Err(unprocessable(
            "Checkpoint restore files require include_files",
        ));
    }
    Ok(())
}

fn resolve_checkpoint<'a>(
    state: &'a CheckpointState,
    target: &str,
) -> Result<&'a CheckpointEntry, ApiError> {
    let matches = state
        .entries
        .iter()
        .filter(|entry| {
            entry.commit == target
                || entry.commit.starts_with(target)
                || entry.ref_name == target
                || (entry.kind == "snap" && entry.name == target)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [entry] => Ok(*entry),
        [] => Err(not_found("Checkpoint was not found")),
        _ => Err(bad_request("Checkpoint target is ambiguous")),
    }
}

fn prepare_restore_sync(
    context: &WorkspaceContext,
    target: &CheckpointEntry,
    memory_directories: &(PathBuf, PathBuf),
    include_memory: bool,
    include_files: bool,
) -> Result<PreparedRestore, ApiError> {
    let snapshot = load_snapshot_index(context, target)?;
    let current = current_file_hashes(context)?;
    let mut paths = snapshot
        .file_hashes
        .keys()
        .chain(current.keys())
        .cloned()
        .collect::<HashSet<_>>();
    let mut restored_paths = vec![conversation_path(target)];
    let mut deleted_paths = Vec::new();
    let mut file_paths = Vec::new();
    for path in paths.drain() {
        let memory = is_memory_path(&path, memory_directories);
        if (memory && !include_memory) || (!memory && !include_files) {
            continue;
        }
        let before = snapshot.file_hashes.get(&path);
        let after = current.get(&path);
        if before == after {
            continue;
        }
        if before.is_some() {
            restored_paths.push(path.clone());
        } else {
            deleted_paths.push(path.clone());
        }
        if !memory {
            file_paths.push(path);
        }
    }
    restored_paths[1..].sort_unstable();
    deleted_paths.sort_unstable();
    file_paths.sort_unstable();
    Ok(PreparedRestore {
        checkpoint: snapshot.checkpoint,
        restored_paths,
        deleted_paths,
        file_paths,
    })
}

fn load_snapshot_index(
    context: &WorkspaceContext,
    target: &CheckpointEntry,
) -> Result<SnapshotIndex, ApiError> {
    let archive_path = checkpoint_archive_path(context, target)?;
    verify_archive_digest(&archive_path, &target.commit)?;
    let input = fs::File::open(&archive_path)
        .map_err(|_| internal("Checkpoint archive could not be opened"))?;
    let mut archive =
        ZipArchive::new(input).map_err(|_| internal("Checkpoint archive is invalid"))?;
    if archive.len() > MAX_SNAPSHOT_FILES + 2 {
        return Err(internal("Checkpoint archive exceeds its entry limit"));
    }
    let mut total = 0_u64;
    let mut thread = None;
    let mut checkpoint_id_seen = false;
    let mut file_hashes = HashMap::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| internal("Checkpoint archive entry could not be read"))?;
        if entry.is_dir() || archive_entry_is_link(&entry) {
            return Err(internal("Checkpoint archive contains an unsafe entry"));
        }
        total = total.saturating_add(entry.size());
        if total > MAX_SNAPSHOT_BYTES {
            return Err(internal("Checkpoint archive exceeds its size limit"));
        }
        let name = entry.name().to_owned();
        match name.as_str() {
            "thread.json" => {
                if thread.is_some() || entry.size() > MAX_THREAD_BYTES as u64 {
                    return Err(internal("Checkpoint archive contains invalid Thread state"));
                }
                let mut bytes =
                    Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(MAX_THREAD_BYTES));
                let expected = entry.size();
                entry
                    .by_ref()
                    .take(expected.saturating_add(1))
                    .read_to_end(&mut bytes)
                    .map_err(|_| internal("Checkpoint Thread state could not be read"))?;
                if bytes.len() as u64 != expected {
                    return Err(internal(
                        "Checkpoint Thread size does not match its metadata",
                    ));
                }
                thread = Some(
                    serde_json::from_slice::<ThreadCheckpoint>(&bytes)
                        .map_err(|_| internal("Checkpoint Thread state is invalid"))?,
                );
            }
            "checkpoint.id" => {
                if checkpoint_id_seen || entry.size() > MAX_SNAPSHOT_IDENTITY_BYTES {
                    return Err(internal("Checkpoint archive identifier is invalid"));
                }
                checkpoint_id_seen = true;
                let expected = entry.size();
                let mut bytes = Vec::new();
                entry
                    .by_ref()
                    .take(expected.saturating_add(1))
                    .read_to_end(&mut bytes)
                    .map_err(|_| internal("Checkpoint identity could not be read"))?;
                if bytes.len() as u64 != expected
                    || decode_snapshot_identity(&bytes)?.workspace != context.identity
                {
                    return Err(conflict(
                        "Checkpoint archive Workspace identity does not match",
                    ));
                }
            }
            _ => {
                let Some(path) = name.strip_prefix("files/") else {
                    return Err(internal("Checkpoint archive contains an unknown entry"));
                };
                validate_relative_path(path)
                    .map_err(|_| internal("Checkpoint archive contains an unsafe path"))?;
                let destination = context.root.join(relative_path_buf(path)?);
                let control = PathBuf::from(context.control_dir.to_string_lossy().to_lowercase());
                let root = PathBuf::from(context.root.to_string_lossy().to_lowercase());
                let destination_key = PathBuf::from(destination.to_string_lossy().to_lowercase());
                if is_control_path(context, &destination)
                    || (control.starts_with(root) && control.starts_with(destination_key))
                {
                    return Err(internal("Checkpoint archive contains Core control data"));
                }
                let hash = bounded_entry_digest(&mut entry)?;
                if file_hashes.insert(path.to_owned(), hash).is_some() {
                    return Err(internal("Checkpoint archive contains duplicate paths"));
                }
            }
        }
    }
    let checkpoint = thread.ok_or_else(|| internal("Checkpoint archive has no Thread state"))?;
    if !checkpoint_id_seen || checkpoint.thread.id != target.thread_id {
        return Err(internal(
            "Checkpoint archive identity does not match its metadata",
        ));
    }
    Ok(SnapshotIndex {
        checkpoint,
        file_hashes,
    })
}

fn current_file_hashes(context: &WorkspaceContext) -> Result<HashMap<String, String>, ApiError> {
    let mut hashes = HashMap::new();
    let mut total = 0_u64;
    for (path, relative) in collect_workspace_files(context)? {
        let metadata = fs::symlink_metadata(&path)
            .map_err(|_| bad_request("Workspace changed while preparing the restore"))?;
        total = total.saturating_add(metadata.len());
        if total > MAX_SNAPSHOT_BYTES {
            return Err(payload_too_large(
                "Workspace content exceeds the 512 MiB restore limit",
            ));
        }
        let mut input = fs::File::open(path)
            .map_err(|_| bad_request("Workspace changed while preparing the restore"))?;
        hashes.insert(relative, reader_digest(&mut input)?);
    }
    Ok(hashes)
}

fn checkpoint_archive_path(
    context: &WorkspaceContext,
    target: &CheckpointEntry,
) -> Result<PathBuf, ApiError> {
    let path = context
        .state_dir
        .join("snapshots")
        .join(format!("{}.zip", target.commit));
    let metadata =
        fs::symlink_metadata(&path).map_err(|_| internal("Checkpoint archive is missing"))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_SNAPSHOT_BYTES
    {
        return Err(internal("Checkpoint archive is not a valid regular file"));
    }
    Ok(path)
}

fn verify_archive_digest(path: &Path, expected: &str) -> Result<(), ApiError> {
    let mut input =
        fs::File::open(path).map_err(|_| internal("Checkpoint archive could not be opened"))?;
    let actual = reader_digest(&mut input)?;
    if actual != expected {
        return Err(internal("Checkpoint archive integrity check failed"));
    }
    Ok(())
}

fn reader_digest(reader: &mut impl Read) -> Result<String, ApiError> {
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1_024].into_boxed_slice();
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|_| internal("Checkpoint content could not be read"))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn bounded_entry_digest(entry: &mut zip::read::ZipFile<'_, fs::File>) -> Result<String, ApiError> {
    let expected = entry.size();
    let mut reader = entry.take(expected.saturating_add(1));
    let digest = reader_digest(&mut reader)?;
    if reader.limit() != 1 {
        return Err(internal(
            "Checkpoint entry size does not match its metadata",
        ));
    }
    Ok(digest)
}
fn archive_entry_is_link(entry: &zip::read::ZipFile<'_, fs::File>) -> bool {
    entry
        .unix_mode()
        .is_some_and(|mode| mode & 0o170_000 == 0o120_000)
}

fn apply_archive_paths_sync(
    context: &WorkspaceContext,
    target: &CheckpointEntry,
    paths: &HashSet<String>,
) -> Result<super::desktop_restore_files::RestoreFiles, ApiError> {
    let mut files = super::desktop_restore_files::RestoreFiles::default();
    if paths.is_empty() {
        return Ok(files);
    }
    let snapshot = load_snapshot_index(context, target)?;
    for path in paths {
        validate_relative_path(path)?;
    }
    let archive_path = checkpoint_archive_path(context, target)?;
    let input = fs::File::open(archive_path)
        .map_err(|_| internal("Checkpoint archive could not be opened"))?;
    let mut archive =
        ZipArchive::new(input).map_err(|_| internal("Checkpoint archive is invalid"))?;
    let mut ordered = paths.iter().cloned().collect::<Vec<_>>();
    ordered.sort_unstable();
    for path in &ordered {
        let relative = relative_path_buf(path)?;
        let target_path = prepare_workspace_target(&context.root, &relative, &mut files)?;
        if snapshot.file_hashes.contains_key(path) {
            let mut source = archive
                .by_name(&format!("files/{path}"))
                .map_err(|_| internal("Checkpoint archive file is missing"))?;
            files
                .stage_replace(&target_path, |staged_path| {
                    let parent = staged_path.parent().expect("staged file has a parent");
                    let mut output = NamedTempFile::new_in(parent)?;
                    std::io::copy(&mut source, &mut output)?;
                    output.as_file_mut().sync_all()?;
                    output
                        .persist_noclobber(staged_path)
                        .map(|_| ())
                        .map_err(|error| error.error)
                })
                .map_err(|_| internal("Checkpoint restore file could not be staged"))?;
        } else {
            files
                .stage_delete(&target_path)
                .map_err(|_| internal("Checkpoint restore deletion could not be staged"))?;
        }
    }
    files.apply().map_err(|_| {
        internal("Checkpoint file restore failed; any incomplete rollback data has been retained")
    })?;
    Ok(files)
}

fn inspect_workspace_target(root: &Path, relative: &Path) -> Result<Option<PathBuf>, ApiError> {
    let mut current = root.to_path_buf();
    let components = relative.components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(component) = component else {
            return Err(bad_request("Checkpoint restore path is invalid"));
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if is_link_or_junction(&metadata) => {
                return Err(bad_request(
                    "Checkpoint restore path must not contain links",
                ));
            }
            Ok(metadata) if index + 1 == components.len() && metadata.is_file() => {
                return Ok(Some(current));
            }
            Ok(metadata) if index + 1 < components.len() && metadata.is_dir() => {}
            Ok(_) => {
                return Err(bad_request(
                    "Checkpoint restore path must contain only regular files and directories",
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(internal("Checkpoint restore path could not be inspected")),
        }
    }
    Ok(None)
}

fn prepare_workspace_target(
    root: &Path,
    relative: &Path,
    files: &mut super::desktop_restore_files::RestoreFiles,
) -> Result<PathBuf, ApiError> {
    let parent = relative
        .parent()
        .ok_or_else(|| bad_request("Checkpoint restore path is invalid"))?;
    let mut current = root.to_path_buf();
    for component in parent.components() {
        let Component::Normal(component) = component else {
            return Err(bad_request("Checkpoint restore path is invalid"));
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if is_link_or_junction(&metadata) || !metadata.is_dir() => {
                return Err(bad_request(
                    "Checkpoint restore path must contain only regular directories",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                files
                    .create_directory(&current)
                    .map_err(|_| internal("Checkpoint restore directory could not be created"))?;
            }
            Err(_) => return Err(internal("Checkpoint restore path could not be inspected")),
        }
    }
    let canonical_parent = current
        .canonicalize()
        .map_err(|_| internal("Checkpoint restore directory could not be resolved"))?;
    if !canonical_parent.starts_with(root) {
        return Err(bad_request("Checkpoint restore path escaped its Workspace"));
    }
    let name = relative
        .file_name()
        .ok_or_else(|| bad_request("Checkpoint restore path is invalid"))?;
    let target = canonical_parent.join(name);
    match fs::symlink_metadata(&target) {
        Ok(metadata) if is_link_or_junction(&metadata) || !metadata.is_file() => Err(bad_request(
            "Checkpoint restore target must be a regular file",
        )),
        Ok(_) => Ok(target),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(target),
        Err(_) => Err(internal("Checkpoint restore target could not be inspected")),
    }
}

fn restore_value(
    target: &CheckpointEntry,
    prepared: &PreparedRestore,
    dry_run: bool,
    include_memory: bool,
    include_files: bool,
    pre_restore_ref: Option<&str>,
    selected_files: Option<&HashSet<String>>,
) -> Value {
    let file_paths = selected_files.map_or_else(
        || prepared.file_paths.clone(),
        |selected| {
            prepared
                .file_paths
                .iter()
                .filter(|path| selected.contains(path.as_str()))
                .cloned()
                .collect()
        },
    );
    let all_file_paths = prepared.file_paths.iter().collect::<HashSet<_>>();
    let include_path = |path: &&String| {
        !all_file_paths.contains(path)
            || selected_files.is_none_or(|selected| selected.contains(path.as_str()))
    };
    let restored_paths = prepared
        .restored_paths
        .iter()
        .filter(include_path)
        .cloned()
        .collect::<Vec<_>>();
    let deleted_paths = prepared
        .deleted_paths
        .iter()
        .filter(include_path)
        .cloned()
        .collect::<Vec<_>>();
    json!({
        "target": target.commit,
        "commit": target.commit,
        "restored_paths": restored_paths,
        "deleted_paths": deleted_paths,
        "file_paths": file_paths,
        "pre_restore_ref": pre_restore_ref,
        "dry_run": dry_run,
        "include_memory": include_memory,
        "include_files": include_files
    })
}

fn conversation_path(target: &CheckpointEntry) -> String {
    format!("sessions/{}.json", target.session_id)
}

fn is_memory_path(path: &str, directories: &(PathBuf, PathBuf)) -> bool {
    if path == "MEMORY.md" {
        return true;
    }
    [&directories.0, &directories.1]
        .into_iter()
        .any(|directory| {
            let prefix = directory
                .components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            path == prefix || path.starts_with(&format!("{prefix}/"))
        })
}

fn validate_relative_path(path: &str) -> Result<(), ApiError> {
    if path.is_empty()
        || path.len() > 4_096
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.ends_with('/')
        || path.contains('\\')
        || path.chars().any(char::is_control)
        || path
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | "..") || part.contains(':'))
    {
        return Err(bad_request("Checkpoint restore path is unsafe"));
    }
    Ok(())
}

fn relative_path_buf(path: &str) -> Result<PathBuf, ApiError> {
    validate_relative_path(path)?;
    Ok(path.split('/').collect())
}

fn validate_gc_request(request: &GcRequest) -> Result<(), ApiError> {
    if request
        .keep_count
        .is_some_and(|value| value > MAX_GC_KEEP_COUNT)
        || request.keep_days.is_some_and(|value| value > MAX_GC_DAYS)
        || request
            .pre_restore_days
            .is_some_and(|value| value > MAX_GC_DAYS)
    {
        return Err(unprocessable("Checkpoint GC setting is out of range"));
    }
    Ok(())
}

fn gc_selection(
    state: &CheckpointState,
    keep_count: u32,
    keep_days: u32,
    pre_restore_days: u32,
    compact: bool,
    thread: Option<&str>,
) -> (Vec<CheckpointEntry>, Vec<CheckpointEntry>) {
    let now = now_millis();
    let regular_cutoff = now.saturating_sub(u64::from(keep_days) * MILLIS_PER_DAY);
    let safety_cutoff = now.saturating_sub(u64::from(pre_restore_days) * MILLIS_PER_DAY);
    let heads = state.heads.values().collect::<HashSet<_>>();
    let mut by_session = HashMap::<&str, Vec<&CheckpointEntry>>::new();
    let scoped = state
        .entries
        .iter()
        .filter(|entry| thread.is_none_or(|id| entry.thread_id == id));
    for entry in scoped.clone().filter(|entry| entry.kind == "auto") {
        by_session
            .entry(entry.session_key.as_str())
            .or_default()
            .push(entry);
    }
    let mut retained = HashSet::new();
    for entries in by_session.values_mut() {
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.timestamp_ms));
        for entry in entries
            .iter()
            .take(usize::try_from(keep_count).unwrap_or(usize::MAX))
        {
            retained.insert(entry.commit.as_str());
        }
    }
    let mut deleted = Vec::new();
    let mut kept = Vec::new();
    for entry in scoped.filter(|entry| matches!(entry.kind.as_str(), "auto" | "pre-restore")) {
        let keep = heads.contains(&entry.commit)
            || if entry.kind == "auto" {
                !compact
                    && (retained.contains(entry.commit.as_str())
                        || entry.timestamp_ms >= regular_cutoff)
            } else {
                entry.timestamp_ms >= safety_cutoff
            };
        if keep {
            kept.push(entry.clone());
        } else {
            deleted.push(entry.clone());
        }
    }
    deleted.sort_by_key(|entry| std::cmp::Reverse(entry.timestamp_ms));
    kept.sort_by_key(|entry| std::cmp::Reverse(entry.timestamp_ms));
    (deleted, kept)
}

#[cfg(windows)]
fn is_link_or_junction(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_link_or_junction(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

async fn get_gc_settings(
    State(server): State<AppServer>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let (_lifecycle, _, context) = selected_context(&server, &headers).await?;
    let _guard = server.inner.desktop_checkpoint_lock.lock().await;
    let state = read_state_async(context.clone()).await?;
    Ok(Json(gc_settings_value(&state)))
}

async fn update_gc_settings(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Json(settings): Json<GcSettings>,
) -> Result<Json<Value>, ApiError> {
    validate_gc_settings(&settings)?;
    let (_lifecycle, _, context) = selected_context(&server, &headers).await?;
    let _guard = server.inner.desktop_checkpoint_lock.lock().await;
    let mut state = read_state_async(context.clone()).await?;
    state.gc_keep_count = settings.gc_keep_count;
    state.gc_keep_days = settings.gc_keep_days;
    state.pre_restore_retention_days = settings.pre_restore_retention_days;
    write_state_async(context.state_dir, state).await?;
    Ok(Json(json!(settings)))
}

async fn reset(
    State(server): State<AppServer>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let (_lifecycle, _, context) = selected_context(&server, &headers).await?;
    let _guard = server.inner.desktop_checkpoint_lock.lock().await;
    read_state_async(context.clone()).await?;
    runtime::cancel_workspace(&server, &context.identity.data_key);
    reset_async(context.state_dir).await?;
    Ok(Json(json!({"reset": true, "auto_enabled": false})))
}

async fn selected_context<'a>(
    server: &'a AppServer,
    headers: &HeaderMap,
) -> Result<
    (
        tokio::sync::MutexGuard<'a, ()>,
        AgentContext,
        WorkspaceContext,
    ),
    ApiError,
> {
    let (lifecycle, agent) = quiescence::admit_agent(server, headers).await?;
    let context = context_for_root(server, &agent.workspace, &agent.data_key)?;
    Ok((lifecycle, agent, context))
}

#[cfg(test)]
pub(super) async fn backup_sources(
    server: &AppServer,
    root: &Path,
) -> Result<(PathBuf, Vec<String>), ApiError> {
    let root = root
        .canonicalize()
        .map_err(|_| bad_request("Workspace directory is unavailable"))?;
    let agent = super::desktop_agents::backup_agent_snapshot(server)
        .await?
        .agents
        .into_iter()
        .find(|agent| Path::new(&agent.workspace_dir) == root)
        .ok_or_else(|| not_found("Checkpoint Workspace is not registered"))?;
    let key = agent
        .data_key
        .ok_or_else(|| internal("Checkpoint Workspace identity is missing"))?;
    let context = context_for_root(server, &root, &key)?;
    let state = read_state(&context.state_dir, &context.identity)?;
    let mut files = Vec::new();
    if context.state_dir.join("state.json").is_file() {
        files.push(String::from("state.json"));
    }
    let mut seen = HashSet::new();
    for entry in &state.entries {
        // Validate nested snapshots before another archive can carry them.
        // Orphan/staging files are not part of the checkpoint state.
        load_snapshot_index(&context, entry)?;
        if seen.insert(&entry.commit) {
            files.push(format!("snapshots/{}.zip", entry.commit));
        }
    }
    Ok((context.state_dir, files))
}

pub(super) struct ScopedCheckpointBackup {
    pub(super) directory: PathBuf,
    pub(super) state: Option<Vec<u8>>,
    pub(super) archives: Vec<String>,
}

/// Inject mixed historical ownership to exercise export filtering. The live
/// producer must never recreate this intentionally inconsistent test state.
#[cfg(test)]
pub(super) async fn seed_mixed_owner_snapshot(server: &AppServer, thread_id: &str) {
    let default = super::desktop_agents::context_for_agent(server, "default")
        .await
        .unwrap();
    let owner = super::desktop_chats::approval_session_info(server, thread_id)
        .await
        .unwrap();
    let session = super::desktop_chats::checkpoint_sessions(server, &owner.agent)
        .await
        .unwrap()
        .into_iter()
        .find(|session| session.thread_id == thread_id)
        .unwrap();
    let context = context_for_root(server, &default.workspace, &default.data_key).unwrap();
    let checkpoint = server
        .inner
        .core
        .export_thread_checkpoint(thread_id)
        .await
        .unwrap();
    let _guard = server.inner.desktop_checkpoint_lock.lock().await;
    create_snapshot_async(
        context,
        checkpoint,
        session,
        String::from("auto"),
        String::new(),
        None,
    )
    .await
    .unwrap();
}

pub(super) fn scoped_backup_sources(
    server: &AppServer,
    root: &Path,
    key: &WorkspaceDataKey,
    thread_ids: &std::collections::BTreeSet<String>,
) -> Result<ScopedCheckpointBackup, ApiError> {
    let context = context_for_root(server, root, key)?;
    let mut state = read_state(&context.state_dir, &context.identity)?;
    let excluded = state
        .entries
        .iter()
        .filter(|entry| !thread_ids.contains(&entry.thread_id))
        .map(|entry| entry.commit.clone())
        .collect::<HashSet<_>>();
    state
        .entries
        .retain(|entry| thread_ids.contains(&entry.thread_id));
    state.heads.retain(|_, commit| !excluded.contains(commit));
    for entry in &mut state.entries {
        if entry
            .parent_commit
            .as_ref()
            .is_some_and(|parent| excluded.contains(parent))
        {
            entry.parent_commit = None;
        }
    }
    validate_state(&state).map_err(internal)?;
    let mut archives = Vec::new();
    for entry in &state.entries {
        let snapshot = load_snapshot_index(&context, entry)?;
        super::Core::validate_thread_checkpoint(&snapshot.checkpoint).map_err(core_error)?;
        archives.push(format!("snapshots/{}.zip", entry.commit));
    }
    let state_path = context.state_dir.join("state.json");
    let bytes = if !state_path.exists() {
        None
    } else if excluded.is_empty() {
        Some(fs::read(state_path).map_err(|_| internal("Checkpoint state could not be read"))?)
    } else {
        Some(
            serde_json::to_vec(&state)
                .map_err(|_| internal("Checkpoint state could not be encoded"))?,
        )
    };
    Ok(ScopedCheckpointBackup {
        directory: context.state_dir,
        state: bytes,
        archives,
    })
}

pub(super) fn state_directory(control: &Path, key: &WorkspaceDataKey) -> PathBuf {
    let encoded = serde_json::to_vec(key).expect("Workspace identity is serializable");
    control
        .join("checkpoints")
        .join(format!("workspace-{}", hex_digest(&encoded)))
}

fn context_for_root(
    server: &AppServer,
    root: &Path,
    key: &WorkspaceDataKey,
) -> Result<WorkspaceContext, ApiError> {
    let workspace = server
        .inner
        .desktop_workspace
        .as_ref()
        .ok_or_else(|| not_implemented("Desktop Workspace is unavailable"))?;
    let root = root
        .canonicalize()
        .map_err(|_| bad_request("Workspace directory is unavailable"))?;
    if !root.is_dir() {
        return Err(bad_request("Workspace directory is unavailable"));
    }
    if !super::desktop_agents::identity::matches_marker(&root, key)? {
        return Err(conflict("Checkpoint Workspace binding has changed"));
    }
    let root_text = root.to_string_lossy().into_owned();
    server.inner.desktop_checkpoint_runtime.touch(key);
    Ok(WorkspaceContext {
        identity: CheckpointIdentity {
            data_key: key.clone(),
            workspace_root: root_text.clone(),
        },
        root,
        root_text,
        state_dir: state_directory(&workspace.data_dir, key),
        control_dir: workspace.data_dir.clone(),
    })
}

async fn resolve_session(
    server: &AppServer,
    agent: &AgentContext,
    request: &SnapshotRequest,
) -> Result<CheckpointSessionInfo, ApiError> {
    super::desktop_chats::bound_checkpoint_sessions(server, agent)
        .await?
        .into_iter()
        .find(|session| {
            session.session_id == request.session_id
                && session.user_id == request.user_id
                && session.channel == request.channel
        })
        .ok_or_else(|| not_found("Checkpoint session was not found in this Workspace"))
}

async fn create_snapshot_async(
    context: WorkspaceContext,
    checkpoint: ThreadCheckpoint,
    session: CheckpointSessionInfo,
    kind: String,
    name: String,
    query: Option<String>,
) -> Result<CheckpointEntry, ApiError> {
    tokio::task::spawn_blocking(move || {
        create_snapshot_sync(&context, &checkpoint, &session, &kind, &name, query)
    })
    .await
    .map_err(|error| internal(&format!("Checkpoint task failed: {error}")))?
}

fn create_snapshot_sync(
    context: &WorkspaceContext,
    checkpoint: &ThreadCheckpoint,
    session: &CheckpointSessionInfo,
    kind: &str,
    name: &str,
    query: Option<String>,
) -> Result<CheckpointEntry, ApiError> {
    let mut state = read_state(&context.state_dir, &context.identity)?;
    if state.entries.len() >= MAX_CHECKPOINTS {
        return Err(payload_too_large(
            "Checkpoint count reached the 5000 item limit",
        ));
    }
    ensure_state_directories(&context.state_dir)?;
    let archives = context.state_dir.join("snapshots");
    let mut temporary = NamedTempFile::new_in(&archives)
        .map_err(|_| internal("Checkpoint archive could not be created"))?;
    write_snapshot_archive(temporary.as_file_mut(), context, checkpoint)?;
    temporary
        .as_file_mut()
        .sync_all()
        .map_err(|_| internal("Checkpoint archive could not be finalized"))?;
    temporary
        .as_file_mut()
        .rewind()
        .map_err(|_| internal("Checkpoint archive could not be verified"))?;
    let commit = reader_digest(temporary.as_file_mut())?;
    let archive_path = archives.join(format!("{commit}.zip"));
    if !archive_path.exists() {
        temporary
            .persist(&archive_path)
            .map_err(|_| internal("Checkpoint archive could not be installed"))?;
    }
    let timestamp_ms = now_millis();
    let key = checkpoint_session_key(&session.channel, &session.user_id, &session.session_id);
    let parent_commit = state.heads.get(&key).cloned();
    let label = if name.trim().is_empty() {
        match kind {
            "auto" => String::from("Auto checkpoint"),
            "pre-restore" => String::from("Before restore"),
            _ => String::from("Snapshot"),
        }
    } else {
        name.trim().to_owned()
    };
    let ref_name = match kind {
        "auto" => format!("refs/auto/{key}/{timestamp_ms}-{}", Uuid::now_v7()),
        "pre-restore" => format!("refs/pre-restore/{timestamp_ms}-{key}-{}", Uuid::now_v7()),
        _ => format!("refs/snap/{key}/{timestamp_ms}-{}", Uuid::now_v7()),
    };
    let query = query.or_else(|| latest_user_query(checkpoint));
    let subject = match kind {
        "auto" => format!("auto {key} {timestamp_ms}"),
        "pre-restore" => format!("pre-restore {key} {timestamp_ms}"),
        _ => format!("snapshot {key} {label}"),
    };
    let entry = CheckpointEntry {
        ref_name,
        kind: kind.to_owned(),
        session_key: key.clone(),
        name: label,
        commit,
        timestamp_ms,
        subject,
        query,
        channel: session.channel.clone(),
        restore_index: None,
        parent_commit,
        user_id: session.user_id.clone(),
        session_id: session.session_id.clone(),
        thread_id: session.thread_id.clone(),
    };
    state.entries.push(entry.clone());
    state.heads.insert(key, entry.commit.clone());
    if let Err(error) = write_state(&context.state_dir, &state) {
        let _ = fs::remove_file(archive_path);
        return Err(error);
    }
    Ok(entry)
}

fn write_snapshot_archive(
    output: &mut fs::File,
    context: &WorkspaceContext,
    checkpoint: &ThreadCheckpoint,
) -> Result<(), ApiError> {
    let thread = serde_json::to_vec(checkpoint)
        .map_err(|_| internal("Thread checkpoint could not be serialized"))?;
    if thread.len() > MAX_THREAD_BYTES {
        return Err(payload_too_large(
            "Thread checkpoint exceeds the 32 MiB limit",
        ));
    }
    let files = collect_workspace_files(context)?;
    let identity = serde_json::to_vec(&SnapshotIdentity {
        version: STATE_VERSION,
        id: Uuid::now_v7(),
        workspace: context.identity.clone(),
    })
    .map_err(|_| internal("Checkpoint identity could not be encoded"))?;
    if identity.len() as u64 > MAX_SNAPSHOT_IDENTITY_BYTES {
        return Err(payload_too_large(
            "Checkpoint identity exceeds its size limit",
        ));
    }
    let mut total = u64::try_from(thread.len() + identity.len()).unwrap_or(u64::MAX);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o600);
    let mut writer = ZipWriter::new(output);
    writer
        .start_file("thread.json", options)
        .map_err(|_| internal("Checkpoint archive could not be written"))?;
    writer
        .write_all(&thread)
        .map_err(|_| internal("Checkpoint archive could not be written"))?;
    writer
        .start_file("checkpoint.id", options)
        .map_err(|_| internal("Checkpoint archive could not be written"))?;
    writer
        .write_all(&identity)
        .map_err(|_| internal("Checkpoint archive could not be written"))?;
    for (path, relative) in files {
        let metadata = fs::symlink_metadata(&path)
            .map_err(|_| bad_request("Workspace changed while creating the checkpoint"))?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(bad_request(
                "Workspace changed while creating the checkpoint",
            ));
        }
        total = total.saturating_add(metadata.len());
        if total > MAX_SNAPSHOT_BYTES {
            return Err(payload_too_large(
                "Checkpoint content exceeds the 512 MiB limit",
            ));
        }
        writer
            .start_file(format!("files/{relative}"), options)
            .map_err(|_| internal("Checkpoint archive could not be written"))?;
        let mut input = fs::File::open(path)
            .map_err(|_| bad_request("Workspace changed while creating the checkpoint"))?;
        std::io::copy(&mut input, &mut writer)
            .map_err(|_| internal("Checkpoint archive could not be written"))?;
    }
    writer
        .finish()
        .map_err(|_| internal("Checkpoint archive could not be finalized"))?;
    Ok(())
}

fn collect_workspace_files(context: &WorkspaceContext) -> Result<Vec<(PathBuf, String)>, ApiError> {
    let root = &context.root;
    let mut directories = vec![root.clone()];
    let mut files = Vec::new();
    while let Some(directory) = directories.pop() {
        let reader = fs::read_dir(&directory)
            .map_err(|_| bad_request("Workspace could not be read for checkpointing"))?;
        for item in reader {
            let item = item.map_err(|_| bad_request("Workspace could not be read"))?;
            let path = item.path();
            if is_control_path(context, &path) {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)
                .map_err(|_| bad_request("Workspace could not be inspected"))?;
            if is_link_or_junction(&metadata) {
                continue;
            }
            let relative = relative_path(root, &path)?;
            if metadata.is_dir() {
                if !excluded_directory(&relative) {
                    directories.push(path);
                }
            } else if metadata.is_file() && !excluded_file(&relative) {
                files.push((path, relative));
                if files.len() > MAX_SNAPSHOT_FILES {
                    return Err(payload_too_large(
                        "Checkpoint contains more than 100000 files",
                    ));
                }
            }
        }
    }
    files.sort_by(|left, right| left.1.cmp(&right.1));
    Ok(files)
}

fn is_control_path(context: &WorkspaceContext, path: &Path) -> bool {
    if !context.control_dir.starts_with(&context.root) {
        return false;
    }
    if path.starts_with(&context.control_dir) {
        return true;
    }
    // Resolve the closest existing ancestor to catch filesystem case and
    // Unicode aliases without creating a target or following a missing suffix.
    let mut ancestor = path;
    loop {
        if let Ok(resolved) = ancestor.canonicalize() {
            return resolved.starts_with(&context.control_dir);
        }
        let Some(parent) = ancestor.parent() else {
            return false;
        };
        if !parent.starts_with(&context.root) {
            return false;
        }
        ancestor = parent;
    }
}

fn relative_path(root: &Path, path: &Path) -> Result<String, ApiError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| bad_request("Workspace path escaped its root"))?;
    let mut parts = Vec::new();
    for component in relative.components() {
        let Component::Normal(part) = component else {
            return Err(bad_request("Workspace contains an invalid path"));
        };
        let part = part
            .to_str()
            .ok_or_else(|| bad_request("Workspace contains a non-UTF-8 path"))?;
        if part.chars().any(char::is_control) {
            return Err(bad_request("Workspace contains an invalid path"));
        }
        parts.push(part);
    }
    Ok(parts.join("/"))
}

fn excluded_directory(path: &str) -> bool {
    path.split('/').any(|part| {
        part.starts_with(super::desktop_restore_files::RECOVERY_PREFIX)
            || matches!(
                part,
                ".git"
                    | ".qwenpaw"
                    | ".svn"
                    | "checkpoints"
                    | "node_modules"
                    | "target"
                    | "dist"
                    | "build"
                    | "__pycache__"
                    | ".venv"
                    | "venv"
                    | "env"
            )
    })
}

fn excluded_file(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    let excluded_extension = Path::new(name).extension().is_some_and(|extension| {
        ["pyc", "pyo", "log"]
            .iter()
            .any(|value| extension.eq_ignore_ascii_case(value))
    });
    name == ".DS_Store"
        || name.eq_ignore_ascii_case(super::desktop_agents::identity::MARKER_NAME)
        || excluded_extension
}

async fn read_state_async(context: WorkspaceContext) -> Result<CheckpointState, ApiError> {
    tokio::task::spawn_blocking(move || read_state(&context.state_dir, &context.identity))
        .await
        .map_err(|error| internal(&format!("Checkpoint task failed: {error}")))?
}

async fn write_state_async(path: PathBuf, state: CheckpointState) -> Result<(), ApiError> {
    tokio::task::spawn_blocking(move || write_state(&path, &state))
        .await
        .map_err(|error| internal(&format!("Checkpoint task failed: {error}")))?
}

fn read_state(
    state_dir: &Path,
    identity: &CheckpointIdentity,
) -> Result<CheckpointState, ApiError> {
    validate_state_parent(state_dir)?;
    let path = state_dir.join("state.json");
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let legacy = state_dir
                .parent()
                .ok_or_else(|| internal("Checkpoint directory is invalid"))?
                .join(hex_digest(identity.workspace_root.as_bytes()));
            match fs::symlink_metadata(legacy) {
                Ok(_) => {
                    return Err(conflict(
                        "Legacy checkpoint data has no proven Workspace binding; the original files have been preserved",
                    ));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(internal("Legacy checkpoint data could not be inspected")),
            }
            return Ok(CheckpointState::new(identity.clone()));
        }
        Err(_) => return Err(internal("Checkpoint state could not be inspected")),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(internal("Checkpoint state is not a regular file"));
    }
    if metadata.len() > MAX_STATE_BYTES {
        return Err(internal("Checkpoint state exceeds its size limit"));
    }
    let bytes = fs::read(path).map_err(|_| internal("Checkpoint state could not be read"))?;
    let state = serde_json::from_slice::<CheckpointState>(&bytes)
        .map_err(|_| internal("Checkpoint state is invalid"))?;
    validate_state(&state).map_err(internal)?;
    if state.identity != *identity {
        return Err(conflict(
            "Checkpoint state Workspace identity does not match",
        ));
    }
    Ok(state)
}

fn decode_snapshot_identity(bytes: &[u8]) -> Result<SnapshotIdentity, ApiError> {
    let identity: SnapshotIdentity = serde_json::from_slice(bytes)
        .map_err(|_| internal("Checkpoint archive identity is invalid or unbound"))?;
    if identity.version != STATE_VERSION || identity.id.is_nil() || !identity.workspace.is_valid() {
        return Err(internal("Checkpoint archive identity is invalid"));
    }
    Ok(identity)
}

fn write_state(state_dir: &Path, state: &CheckpointState) -> Result<(), ApiError> {
    validate_state(state).map_err(internal)?;
    ensure_state_directories(state_dir)?;
    let bytes = serde_json::to_vec(state)
        .map_err(|_| internal("Checkpoint state could not be serialized"))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_STATE_BYTES {
        return Err(payload_too_large("Checkpoint state exceeds its size limit"));
    }
    let mut temporary = NamedTempFile::new_in(state_dir)
        .map_err(|_| internal("Checkpoint state could not be written"))?;
    temporary
        .write_all(&bytes)
        .and_then(|()| temporary.as_file_mut().sync_all())
        .map_err(|_| internal("Checkpoint state could not be written"))?;
    temporary
        .persist(state_dir.join("state.json"))
        .map_err(|_| internal("Checkpoint state could not be installed"))?;
    Ok(())
}

fn ensure_state_directories(state_dir: &Path) -> Result<(), ApiError> {
    let parent = state_dir
        .parent()
        .ok_or_else(|| internal("Checkpoint data path is invalid"))?;
    match fs::symlink_metadata(parent) {
        Ok(metadata) if metadata.is_dir() && !is_link_or_junction(&metadata) => {}
        Ok(_) => return Err(internal("Checkpoint data root is not a directory")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(parent)
            .map_err(|_| internal("Checkpoint data directory could not be created"))?,
        Err(_) => return Err(internal("Checkpoint data directory could not be inspected")),
    }
    for directory in [state_dir, &state_dir.join("snapshots")] {
        match fs::symlink_metadata(directory) {
            Ok(metadata) if metadata.is_dir() && !is_link_or_junction(&metadata) => {}
            Ok(_) => return Err(internal("Checkpoint data path is not a directory")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(directory)
                .map_err(|_| internal("Checkpoint data directory could not be created"))?,
            Err(_) => return Err(internal("Checkpoint data directory could not be inspected")),
        }
    }
    Ok(())
}

fn validate_state_parent(state_dir: &Path) -> Result<(), ApiError> {
    let Some(parent) = state_dir.parent() else {
        return Err(internal("Checkpoint data path is invalid"));
    };
    match fs::symlink_metadata(parent) {
        Ok(metadata) if metadata.is_dir() && !is_link_or_junction(&metadata) => Ok(()),
        Ok(_) => Err(internal("Checkpoint data root is not a directory")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(internal("Checkpoint data directory could not be inspected")),
    }
}

async fn reset_async(state_dir: PathBuf) -> Result<(), ApiError> {
    tokio::task::spawn_blocking(move || {
        validate_state_parent(&state_dir)?;
        match fs::symlink_metadata(&state_dir) {
            Ok(metadata) if metadata.is_dir() && !is_link_or_junction(&metadata) => {
                fs::remove_dir_all(state_dir)
                    .map_err(|_| internal("Checkpoint state could not be reset"))
            }
            Ok(_) => Err(internal("Checkpoint data path is not a directory")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(internal("Checkpoint data directory could not be inspected")),
        }
    })
    .await
    .map_err(|error| internal(&format!("Checkpoint task failed: {error}")))?
}

fn validate_state(state: &CheckpointState) -> Result<(), &'static str> {
    if state.version != STATE_VERSION || !state.identity.is_valid() {
        return Err("Checkpoint state version is unsupported");
    }
    if state.entries.len() > MAX_CHECKPOINTS
        || state.gc_keep_count > MAX_GC_KEEP_COUNT
        || state.gc_keep_days > MAX_GC_DAYS
        || state.pre_restore_retention_days > MAX_GC_DAYS
    {
        return Err("Checkpoint state exceeds its configured limits");
    }
    let mut refs = HashSet::new();
    let mut commits = HashSet::new();
    for entry in &state.entries {
        if !matches!(entry.kind.as_str(), "auto" | "snap" | "pre-restore")
            || !valid_identifier(&entry.session_id)
            || !valid_identifier_or_empty(&entry.user_id)
            || !valid_identifier(&entry.channel)
            || !valid_identifier(&entry.thread_id)
            || entry.name.chars().count() > MAX_NAME_CHARS
            || entry.name.chars().any(char::is_control)
            || entry.commit.len() != 64
            || !entry.commit.bytes().all(|value| value.is_ascii_hexdigit())
            || !refs.insert(entry.ref_name.as_str())
            || !commits.insert(entry.commit.as_str())
        {
            return Err("Checkpoint state contains an invalid entry");
        }
    }
    if state
        .heads
        .iter()
        .any(|(key, commit)| key.is_empty() || !commits.contains(commit.as_str()))
    {
        return Err("Checkpoint state contains an invalid head");
    }
    Ok(())
}

fn validate_snapshot_request(request: &SnapshotRequest) -> Result<(), ApiError> {
    if !valid_identifier(&request.session_id)
        || !valid_identifier_or_empty(&request.user_id)
        || !valid_identifier(&request.channel)
        || request.name.chars().count() > MAX_NAME_CHARS
        || request.name.chars().any(char::is_control)
    {
        return Err(unprocessable("Checkpoint snapshot request is invalid"));
    }
    Ok(())
}

fn validate_gc_settings(settings: &GcSettings) -> Result<(), ApiError> {
    if settings.gc_keep_count > MAX_GC_KEEP_COUNT
        || settings.gc_keep_days > MAX_GC_DAYS
        || settings.pre_restore_retention_days > MAX_GC_DAYS
    {
        return Err(unprocessable("Checkpoint GC setting is out of range"));
    }
    Ok(())
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_ID_BYTES && !value.chars().any(char::is_control)
}

fn valid_identifier_or_empty(value: &str) -> bool {
    value.len() <= MAX_ID_BYTES && !value.chars().any(char::is_control)
}

fn checkpoint_node(
    entry: &CheckpointEntry,
    heads: &HashSet<String>,
    session_title: Option<&str>,
) -> Value {
    json!({
        "ref": entry.ref_name,
        "kind": entry.kind,
        "session_key": entry.session_key,
        "name": entry.name,
        "commit": entry.commit,
        "sha": entry.commit.chars().take(12).collect::<String>(),
        "timestamp_ms": entry.timestamp_ms,
        "subject": entry.subject,
        "query": entry.query,
        "channel": entry.channel,
        "restore_index": entry.restore_index,
        "parent_commit": entry.parent_commit,
        "is_head": heads.contains(entry.commit.as_str()),
        "user_id": entry.user_id,
        "session_id": entry.session_id,
        "session_title": session_title.unwrap_or("")
    })
}

fn checkpoint_session_key(channel: &str, user_id: &str, session_id: &str) -> String {
    let identity = serde_json::to_vec(&[channel, user_id, session_id]).unwrap_or_default();
    format!("session-{}", hex_digest(&identity))
}

fn latest_user_query(checkpoint: &ThreadCheckpoint) -> Option<String> {
    checkpoint
        .turns
        .iter()
        .rev()
        .flat_map(|turn| turn.items.iter().rev())
        .find_map(|item| match item {
            Item::UserMessage { text, .. } => Some(text.chars().take(120).collect()),
            _ => None,
        })
}

fn gc_settings_value(state: &CheckpointState) -> Value {
    json!({
        "gc_keep_count": state.gc_keep_count,
        "gc_keep_days": state.gc_keep_days,
        "pre_restore_retention_days": state.pre_restore_retention_days
    })
}

fn default_channel() -> String {
    String::from("console")
}

fn conflict(message: &str) -> ApiError {
    (StatusCode::CONFLICT, Json(json!({"detail":message})))
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

fn hex_digest(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn core_error(error: qwenpaw_core::CoreError) -> ApiError {
    let status = match &error {
        qwenpaw_core::CoreError::ThreadNotFound(_) => StatusCode::NOT_FOUND,
        qwenpaw_core::CoreError::ThreadBusy(_) | qwenpaw_core::CoreError::ThreadArchived(_) => {
            StatusCode::CONFLICT
        }
        qwenpaw_core::CoreError::Checkpoint(_) => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    let detail = error.to_string();
    drop(error);
    (status, Json(json!({"detail": detail})))
}

fn bad_request(message: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, Json(json!({"detail": message})))
}

fn unprocessable(message: &str) -> ApiError {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(json!({"detail": message})),
    )
}

fn not_found(message: &str) -> ApiError {
    (StatusCode::NOT_FOUND, Json(json!({"detail": message})))
}

fn payload_too_large(message: &str) -> ApiError {
    (
        StatusCode::PAYLOAD_TOO_LARGE,
        Json(json!({"detail": message})),
    )
}

fn not_implemented(message: &str) -> ApiError {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({"detail": message})),
    )
}

fn internal(message: &str) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"detail": message})),
    )
}
