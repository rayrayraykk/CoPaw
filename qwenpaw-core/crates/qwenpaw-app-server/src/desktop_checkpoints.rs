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
use super::desktop_chats::CheckpointSessionInfo;

const STATE_VERSION: u32 = 1;
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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct CheckpointState {
    version: u32,
    auto_enabled: bool,
    gc_keep_count: u32,
    gc_keep_days: u32,
    pre_restore_retention_days: u32,
    heads: HashMap<String, String>,
    entries: Vec<CheckpointEntry>,
}

impl Default for CheckpointState {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
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

async fn status(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let context = selected_context(&server).await?;
    let _guard = server.inner.desktop_checkpoint_lock.lock().await;
    let state = read_state_async(context.state_dir.clone()).await?;
    Ok(Json(json!({
        "auto_enabled": state.auto_enabled,
        "has_checkpoints": !state.entries.is_empty(),
        "workspace_dir": context.root_text
    })))
}

async fn graph(
    State(server): State<AppServer>,
    Query(query): Query<GraphQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(500);
    if !(1..=1_000).contains(&limit) {
        return Err(unprocessable("limit must be between 1 and 1000"));
    }
    let context = selected_context(&server).await?;
    let _guard = server.inner.desktop_checkpoint_lock.lock().await;
    let state = read_state_async(context.state_dir.clone()).await?;
    let sessions = super::desktop_chats::checkpoint_sessions(&server, &context.root_text).await?;
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
    Json(request): Json<AutoRequest>,
) -> Result<Json<Value>, ApiError> {
    let context = selected_context(&server).await?;
    let _guard = server.inner.desktop_checkpoint_lock.lock().await;
    let mut state = read_state_async(context.state_dir.clone()).await?;
    state.auto_enabled = request.enabled;
    write_state_async(context.state_dir, state).await?;
    Ok(Json(json!({"auto_enabled": request.enabled})))
}

async fn snapshot(
    State(server): State<AppServer>,
    Json(request): Json<SnapshotRequest>,
) -> Result<Json<Value>, ApiError> {
    validate_snapshot_request(&request)?;
    let context = selected_context(&server).await?;
    let _guard = server.inner.desktop_checkpoint_lock.lock().await;
    let session = resolve_session(&server, &context, &request).await?;
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
    )
    .await?;
    Ok(Json(json!({"ref": entry.ref_name, "commit": entry.commit})))
}

pub(super) async fn maybe_create_auto_checkpoint(server: &AppServer, thread_id: &str) {
    let Ok(thread) = server.inner.core.read_thread(thread_id).await else {
        return;
    };
    let Some(workspace_root) = thread.thread.workspace_root else {
        return;
    };
    let workspace_root = PathBuf::from(workspace_root);
    let Ok(context) = context_for_root(server, &workspace_root) else {
        return;
    };
    let _guard = server.inner.desktop_checkpoint_lock.lock().await;
    let Ok(state) = read_state_async(context.state_dir.clone()).await else {
        return;
    };
    if !state.auto_enabled {
        return;
    }
    let Ok(sessions) = super::desktop_chats::checkpoint_sessions(server, &context.root_text).await
    else {
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
    if let Err(error) = create_snapshot_async(
        context,
        checkpoint,
        session,
        String::from("auto"),
        String::new(),
    )
    .await
    {
        tracing::warn!(detail = %error.1.0, "automatic checkpoint failed");
    }
}

async fn preview_restore(
    State(server): State<AppServer>,
    Json(request): Json<RestoreRequest>,
) -> Result<Json<Value>, ApiError> {
    restore_impl(&server, request, true).await
}

async fn restore(
    State(server): State<AppServer>,
    Json(request): Json<RestoreRequest>,
) -> Result<Json<Value>, ApiError> {
    restore_impl(&server, request, false).await
}

async fn restore_impl(
    server: &AppServer,
    request: RestoreRequest,
    dry_run: bool,
) -> Result<Json<Value>, ApiError> {
    validate_restore_request(&request, dry_run)?;
    let context = selected_context(server).await?;
    let _guard = server.inner.desktop_checkpoint_lock.lock().await;
    let state = read_state_async(context.state_dir.clone()).await?;
    let target = resolve_checkpoint(&state, &request.commit)?.clone();
    if target.session_id != request.session_id
        || target.user_id != request.user_id
        || target.channel != request.channel
    {
        return Err(bad_request(
            "Checkpoint does not belong to the requested session",
        ));
    }
    let sessions = super::desktop_chats::checkpoint_sessions(server, &context.root_text).await?;
    let session = sessions
        .into_iter()
        .find(|session| {
            session.thread_id == target.thread_id
                && session.session_id == request.session_id
                && session.user_id == request.user_id
                && session.channel == request.channel
        })
        .ok_or_else(|| not_found("Checkpoint session was not found in this Workspace"))?;
    let memory_directories = super::desktop_agent_settings::memory_directories(&server.inner.core)?;
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
    )
    .await?;
    let mut state = read_state_async(context.state_dir.clone()).await?;
    let apply_context = context.clone();
    let apply_target = target.clone();
    let apply_paths = mutation_paths.clone();
    tokio::task::spawn_blocking(move || {
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
    state
        .heads
        .insert(target.session_key.clone(), target.commit.clone());
    if let Err(error) = write_state_async(context.state_dir.clone(), state).await {
        rollback_to_safety(&context, &safety, &mutation_paths).await;
        return Err(error);
    }
    if let Err(error) = server
        .inner
        .core
        .restore_thread_checkpoint(&target.thread_id, prepared.checkpoint)
        .await
    {
        rollback_to_safety(&context, &safety, &mutation_paths).await;
        if let Ok(mut rollback_state) = read_state_async(context.state_dir.clone()).await {
            rollback_state
                .heads
                .insert(safety.session_key.clone(), safety.commit.clone());
            if write_state_async(context.state_dir.clone(), rollback_state)
                .await
                .is_err()
            {
                tracing::error!("Checkpoint head rollback failed after Thread restore rejection");
            }
        }
        return Err(core_error(error));
    }
    Ok(Json(applied_response))
}

async fn rollback_to_safety(
    context: &WorkspaceContext,
    safety: &CheckpointEntry,
    paths: &HashSet<String>,
) {
    let rollback_context = context.clone();
    let rollback_safety = safety.clone();
    let rollback_paths = paths.clone();
    let rollback = tokio::task::spawn_blocking(move || {
        apply_archive_paths_sync(&rollback_context, &rollback_safety, &rollback_paths)
    })
    .await;
    if !matches!(rollback, Ok(Ok(()))) {
        tracing::error!("Checkpoint file rollback failed");
    }
}

async fn preview_gc(
    State(server): State<AppServer>,
    Json(request): Json<GcRequest>,
) -> Result<Json<Value>, ApiError> {
    gc_impl(&server, request, true).await
}

async fn gc(
    State(server): State<AppServer>,
    Json(request): Json<GcRequest>,
) -> Result<Json<Value>, ApiError> {
    gc_impl(&server, request, false).await
}

async fn gc_impl(
    server: &AppServer,
    request: GcRequest,
    dry_run: bool,
) -> Result<Json<Value>, ApiError> {
    validate_gc_request(&request)?;
    let context = selected_context(server).await?;
    let _guard = server.inner.desktop_checkpoint_lock.lock().await;
    let mut state = read_state_async(context.state_dir.clone()).await?;
    let keep_count = request.keep_count.unwrap_or(state.gc_keep_count);
    let keep_days = request.keep_days.unwrap_or(state.gc_keep_days);
    let pre_restore_days = request
        .pre_restore_days
        .unwrap_or(state.pre_restore_retention_days);
    let (deleted, kept) = gc_selection(&state, keep_count, keep_days, pre_restore_days);
    if !dry_run {
        let deleted_commits = deleted
            .iter()
            .map(|entry| entry.commit.as_str())
            .collect::<HashSet<_>>();
        state
            .entries
            .retain(|entry| !deleted_commits.contains(entry.commit.as_str()));
        write_state_async(context.state_dir.clone(), state).await?;
        let archives = context.state_dir.join("snapshots");
        let paths = deleted
            .iter()
            .map(|entry| archives.join(format!("{}.zip", entry.commit)))
            .collect::<Vec<_>>();
        let compact = request.compact;
        let live = kept
            .iter()
            .map(|entry| format!("{}.zip", entry.commit))
            .collect::<HashSet<_>>();
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
    let current = current_file_hashes(&context.root)?;
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
                entry
                    .read_to_end(&mut bytes)
                    .map_err(|_| internal("Checkpoint Thread state could not be read"))?;
                thread = Some(
                    serde_json::from_slice::<ThreadCheckpoint>(&bytes)
                        .map_err(|_| internal("Checkpoint Thread state is invalid"))?,
                );
            }
            "checkpoint.id" => {
                if checkpoint_id_seen || entry.size() > 256 {
                    return Err(internal("Checkpoint archive identifier is invalid"));
                }
                checkpoint_id_seen = true;
            }
            _ => {
                let Some(path) = name.strip_prefix("files/") else {
                    return Err(internal("Checkpoint archive contains an unknown entry"));
                };
                validate_relative_path(path)
                    .map_err(|_| internal("Checkpoint archive contains an unsafe path"))?;
                let hash = reader_digest(&mut entry)?;
                if file_hashes.insert(path.to_owned(), hash).is_some() {
                    return Err(internal("Checkpoint archive contains duplicate paths"));
                }
            }
        }
    }
    let checkpoint = thread.ok_or_else(|| internal("Checkpoint archive has no Thread state"))?;
    if !checkpoint_id_seen
        || checkpoint.thread.id != target.thread_id
        || checkpoint.thread.workspace_root.as_deref() != Some(context.root_text.as_str())
    {
        return Err(internal(
            "Checkpoint archive identity does not match its metadata",
        ));
    }
    Ok(SnapshotIndex {
        checkpoint,
        file_hashes,
    })
}

fn current_file_hashes(root: &Path) -> Result<HashMap<String, String>, ApiError> {
    let mut hashes = HashMap::new();
    let mut total = 0_u64;
    for (path, relative) in collect_workspace_files(root)? {
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

fn archive_entry_is_link(entry: &zip::read::ZipFile<'_, fs::File>) -> bool {
    entry
        .unix_mode()
        .is_some_and(|mode| mode & 0o170_000 == 0o120_000)
}

fn apply_archive_paths_sync(
    context: &WorkspaceContext,
    target: &CheckpointEntry,
    paths: &HashSet<String>,
) -> Result<(), ApiError> {
    if paths.is_empty() {
        return Ok(());
    }
    let snapshot = load_snapshot_index(context, target)?;
    for path in paths {
        validate_relative_path(path)?;
    }
    ensure_state_directories(&context.state_dir)?;
    let transaction = tempfile::tempdir_in(&context.state_dir)
        .map_err(|_| internal("Checkpoint restore transaction could not be created"))?;
    let staged = transaction.path().join("staged");
    let backup = transaction.path().join("backup");
    fs::create_dir_all(&staged)
        .and_then(|()| fs::create_dir_all(&backup))
        .map_err(|_| internal("Checkpoint restore transaction could not be prepared"))?;
    let archive_path = checkpoint_archive_path(context, target)?;
    let input = fs::File::open(archive_path)
        .map_err(|_| internal("Checkpoint archive could not be opened"))?;
    let mut archive =
        ZipArchive::new(input).map_err(|_| internal("Checkpoint archive is invalid"))?;
    let mut originally_missing = HashSet::new();
    let mut ordered = paths.iter().cloned().collect::<Vec<_>>();
    ordered.sort_unstable();
    for path in &ordered {
        let relative = relative_path_buf(path)?;
        let current = inspect_workspace_target(&context.root, &relative)?;
        match current {
            Some(current) => {
                let backup_path = backup.join(&relative);
                if let Some(parent) = backup_path.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|_| internal("Checkpoint backup could not be prepared"))?;
                }
                fs::copy(current, backup_path)
                    .map_err(|_| internal("Checkpoint backup could not be written"))?;
            }
            None => {
                originally_missing.insert(path.clone());
            }
        }
        if snapshot.file_hashes.contains_key(path) {
            let mut source = archive
                .by_name(&format!("files/{path}"))
                .map_err(|_| internal("Checkpoint archive file is missing"))?;
            let staged_path = staged.join(&relative);
            if let Some(parent) = staged_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|_| internal("Checkpoint restore file could not be staged"))?;
            }
            let mut output = fs::File::create(staged_path)
                .map_err(|_| internal("Checkpoint restore file could not be staged"))?;
            std::io::copy(&mut source, &mut output)
                .and_then(|_| output.sync_all())
                .map_err(|_| internal("Checkpoint restore file could not be staged"))?;
        }
    }
    let mut applied = Vec::new();
    for path in &ordered {
        let relative = relative_path_buf(path)?;
        let target_path = prepare_workspace_target(&context.root, &relative)?;
        let result = if snapshot.file_hashes.contains_key(path) {
            atomic_copy(&staged.join(&relative), &target_path)
        } else {
            match fs::remove_file(&target_path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(_) => Err(internal("Checkpoint restore file could not be deleted")),
            }
        };
        if let Err(error) = result {
            if rollback_paths(&context.root, &backup, &originally_missing, &applied).is_err() {
                return Err(internal(
                    "Checkpoint restore failed and its file rollback also failed",
                ));
            }
            return Err(error);
        }
        applied.push(path.clone());
    }
    Ok(())
}

fn rollback_paths(
    root: &Path,
    backup: &Path,
    originally_missing: &HashSet<String>,
    paths: &[String],
) -> Result<(), ApiError> {
    for path in paths.iter().rev() {
        let relative = relative_path_buf(path)?;
        let target = prepare_workspace_target(root, &relative)?;
        if originally_missing.contains(path) {
            match fs::remove_file(target) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(internal("Checkpoint rollback could not delete a file")),
            }
        } else {
            atomic_copy(&backup.join(relative), &target)?;
        }
    }
    Ok(())
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

fn prepare_workspace_target(root: &Path, relative: &Path) -> Result<PathBuf, ApiError> {
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
                fs::create_dir(&current)
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

fn atomic_copy(source: &Path, target: &Path) -> Result<(), ApiError> {
    let parent = target
        .parent()
        .ok_or_else(|| bad_request("Checkpoint restore target is invalid"))?;
    let permissions = fs::metadata(target)
        .ok()
        .map(|metadata| metadata.permissions());
    let mut input = fs::File::open(source)
        .map_err(|_| internal("Checkpoint restore source could not be opened"))?;
    let mut temporary = NamedTempFile::new_in(parent)
        .map_err(|_| internal("Checkpoint restore file could not be staged"))?;
    if let Some(permissions) = permissions {
        temporary
            .as_file()
            .set_permissions(permissions)
            .map_err(|_| internal("Checkpoint restore permissions could not be applied"))?;
    }
    std::io::copy(&mut input, &mut temporary)
        .and_then(|_| temporary.as_file_mut().sync_all())
        .map_err(|_| internal("Checkpoint restore file could not be written"))?;
    temporary
        .persist(target)
        .map_err(|_| internal("Checkpoint restore file could not be installed"))?;
    Ok(())
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
) -> (Vec<CheckpointEntry>, Vec<CheckpointEntry>) {
    let now = now_millis();
    let regular_cutoff = now.saturating_sub(u64::from(keep_days) * MILLIS_PER_DAY);
    let safety_cutoff = now.saturating_sub(u64::from(pre_restore_days) * MILLIS_PER_DAY);
    let heads = state.heads.values().collect::<HashSet<_>>();
    let mut by_session = HashMap::<&str, Vec<&CheckpointEntry>>::new();
    for entry in &state.entries {
        by_session
            .entry(entry.session_key.as_str())
            .or_default()
            .push(entry);
    }
    let mut retained = HashSet::new();
    for entries in by_session.values_mut() {
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.timestamp_ms));
        for (index, entry) in entries.iter().enumerate() {
            let age_cutoff = if entry.kind == "pre-restore" {
                safety_cutoff
            } else {
                regular_cutoff
            };
            let within_count = entry.kind != "pre-restore"
                && index < usize::try_from(keep_count).unwrap_or(usize::MAX);
            if heads.contains(&entry.commit)
                || within_count
                || (age_cutoff < now && entry.timestamp_ms >= age_cutoff)
            {
                retained.insert(entry.commit.as_str());
            }
        }
    }
    let mut deleted = Vec::new();
    let mut kept = Vec::new();
    for entry in &state.entries {
        if retained.contains(entry.commit.as_str()) {
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

async fn get_gc_settings(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let context = selected_context(&server).await?;
    let _guard = server.inner.desktop_checkpoint_lock.lock().await;
    let state = read_state_async(context.state_dir).await?;
    Ok(Json(gc_settings_value(&state)))
}

async fn update_gc_settings(
    State(server): State<AppServer>,
    Json(settings): Json<GcSettings>,
) -> Result<Json<Value>, ApiError> {
    validate_gc_settings(&settings)?;
    let context = selected_context(&server).await?;
    let _guard = server.inner.desktop_checkpoint_lock.lock().await;
    let mut state = read_state_async(context.state_dir.clone()).await?;
    state.gc_keep_count = settings.gc_keep_count;
    state.gc_keep_days = settings.gc_keep_days;
    state.pre_restore_retention_days = settings.pre_restore_retention_days;
    write_state_async(context.state_dir, state).await?;
    Ok(Json(json!(settings)))
}

async fn reset(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let context = selected_context(&server).await?;
    let _guard = server.inner.desktop_checkpoint_lock.lock().await;
    reset_async(context.state_dir).await?;
    Ok(Json(json!({"reset": true, "auto_enabled": false})))
}

async fn selected_context(server: &AppServer) -> Result<WorkspaceContext, ApiError> {
    let workspace = server
        .inner
        .desktop_workspace
        .as_ref()
        .ok_or_else(|| not_implemented("Desktop Workspace is unavailable"))?;
    context_for_root(server, &workspace.selected.read().await)
}

fn context_for_root(server: &AppServer, root: &Path) -> Result<WorkspaceContext, ApiError> {
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
    let root_text = root.to_string_lossy().into_owned();
    let key = hex_digest(root_text.as_bytes());
    Ok(WorkspaceContext {
        root,
        root_text,
        state_dir: workspace.data_dir.join("checkpoints").join(key),
    })
}

async fn resolve_session(
    server: &AppServer,
    context: &WorkspaceContext,
    request: &SnapshotRequest,
) -> Result<CheckpointSessionInfo, ApiError> {
    super::desktop_chats::checkpoint_sessions(server, &context.root_text)
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
) -> Result<CheckpointEntry, ApiError> {
    tokio::task::spawn_blocking(move || {
        create_snapshot_sync(&context, &checkpoint, &session, &kind, &name)
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
) -> Result<CheckpointEntry, ApiError> {
    let mut state = read_state(&context.state_dir)?;
    if state.entries.len() >= MAX_CHECKPOINTS {
        return Err(payload_too_large(
            "Checkpoint count reached the 5000 item limit",
        ));
    }
    ensure_state_directories(&context.state_dir)?;
    let archives = context.state_dir.join("snapshots");
    let mut temporary = NamedTempFile::new_in(&archives)
        .map_err(|_| internal("Checkpoint archive could not be created"))?;
    write_snapshot_archive(temporary.as_file_mut(), &context.root, checkpoint)?;
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
    let query = latest_user_query(checkpoint);
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
    workspace: &Path,
    checkpoint: &ThreadCheckpoint,
) -> Result<(), ApiError> {
    let thread = serde_json::to_vec(checkpoint)
        .map_err(|_| internal("Thread checkpoint could not be serialized"))?;
    if thread.len() > MAX_THREAD_BYTES {
        return Err(payload_too_large(
            "Thread checkpoint exceeds the 32 MiB limit",
        ));
    }
    let files = collect_workspace_files(workspace)?;
    let mut total = u64::try_from(thread.len()).unwrap_or(u64::MAX);
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
        .write_all(Uuid::now_v7().to_string().as_bytes())
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

fn collect_workspace_files(root: &Path) -> Result<Vec<(PathBuf, String)>, ApiError> {
    let mut directories = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = directories.pop() {
        let reader = fs::read_dir(&directory)
            .map_err(|_| bad_request("Workspace could not be read for checkpointing"))?;
        for item in reader {
            let item = item.map_err(|_| bad_request("Workspace could not be read"))?;
            let path = item.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|_| bad_request("Workspace could not be inspected"))?;
            if metadata.file_type().is_symlink() {
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
        matches!(
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
    name == ".DS_Store" || excluded_extension
}

async fn read_state_async(path: PathBuf) -> Result<CheckpointState, ApiError> {
    tokio::task::spawn_blocking(move || read_state(&path))
        .await
        .map_err(|error| internal(&format!("Checkpoint task failed: {error}")))?
}

async fn write_state_async(path: PathBuf, state: CheckpointState) -> Result<(), ApiError> {
    tokio::task::spawn_blocking(move || write_state(&path, &state))
        .await
        .map_err(|error| internal(&format!("Checkpoint task failed: {error}")))?
}

fn read_state(state_dir: &Path) -> Result<CheckpointState, ApiError> {
    validate_state_parent(state_dir)?;
    let path = state_dir.join("state.json");
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(CheckpointState::default());
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
    Ok(state)
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
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => return Err(internal("Checkpoint data root is not a directory")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(parent)
            .map_err(|_| internal("Checkpoint data directory could not be created"))?,
        Err(_) => return Err(internal("Checkpoint data directory could not be inspected")),
    }
    for directory in [state_dir, &state_dir.join("snapshots")] {
        match fs::symlink_metadata(directory) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
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
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(internal("Checkpoint data root is not a directory")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(internal("Checkpoint data directory could not be inspected")),
    }
}

async fn reset_async(state_dir: PathBuf) -> Result<(), ApiError> {
    tokio::task::spawn_blocking(move || {
        validate_state_parent(&state_dir)?;
        match fs::symlink_metadata(&state_dir) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
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
    if state.version != STATE_VERSION {
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
