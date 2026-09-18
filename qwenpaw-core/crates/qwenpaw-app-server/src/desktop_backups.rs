//! Signed, bounded backups for the unchanged Console backup workflow.

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::convert::Infallible;
use std::fs;
use std::io::Read;
use std::io::Seek;
use std::io::Write;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;

use axum::Json;
use axum::Router;
use axum::body::Body;
use axum::extract::DefaultBodyLimit;
use axum::extract::Multipart;
use axum::extract::Path as AxumPath;
use axum::extract::State;
use axum::http::StatusCode;
use axum::http::header::CACHE_CONTROL;
use axum::http::header::CONTENT_DISPOSITION;
use axum::http::header::CONTENT_TYPE;
use axum::response::IntoResponse as _;
use axum::response::Response;
use axum::response::Sse;
use axum::response::sse::Event;
use axum::response::sse::KeepAlive;
use axum::routing::get;
use axum::routing::post;
use chrono::DateTime;
use chrono::Utc;
use futures_util::Stream;
use hmac::Hmac;
use hmac::Mac as _;
use rand::RngCore as _;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use sha2::Digest as _;
use sha2::Sha256;
use tokio::sync::Mutex;
use tokio::sync::watch;
use tokio_util::io::ReaderStream;
use tokio_util::sync::CancellationToken;
use tracing::warn;
use uuid::Uuid;
use zip::ZipArchive;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use super::AppServer;
use super::DesktopCredentialStore;
use super::DesktopWorkspace;

#[path = "desktop_backup_restore.rs"]
mod restore;
#[path = "desktop_backup_restore_credentials.rs"]
mod restore_credentials;
#[path = "desktop_backup_restore_state.rs"]
mod restore_state;
#[path = "desktop_backup_restore_workspaces.rs"]
mod restore_workspaces;
#[path = "desktop_backup_secrets.rs"]
mod secrets;

type ApiError = (StatusCode, Json<Value>);
type HmacSha256 = Hmac<Sha256>;

const FORMAT_VERSION: &str = "qwenpaw-rust-backup-v1";
const META_FILE: &str = "meta.json";
const MANIFEST_FILE: &str = "manifest.json";
const WORKSPACE_PREFIX: &str = "data/workspaces/";
const CHECKPOINT_PREFIX: &str = "data/checkpoints/";
const CONFIG_PREFIX: &str = "data/config/";
const SKILL_POOL_PREFIX: &str = "data/skill_pool/";
const SECRETS_FILE: &str = "data/secrets/credentials.json";
const CORE_STATE_FILE: &str = "data/core-state.json";
const AGENT_STATE_FILE: &str = "data/agents.json";
const GLOBAL_AGENT_STATE_FILE: &str = "data/config/agent-registry.json";
const MAX_UPLOAD_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_ARCHIVE_FILES: usize = 20_000;
const MAX_PATH_BYTES: usize = 1_024;
const MAX_JOBS: usize = 20;
const MAX_PENDING_IMPORTS: usize = 16;
const PENDING_IMPORT_TTL: Duration = Duration::from_secs(60 * 60);
const TERMINAL_STATUSES: [&str; 3] = ["completed", "failed", "cancelled"];
static SIGNING_KEY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Clone)]
pub(super) struct BackupsState {
    coordinator: Arc<Mutex<Coordinator>>,
    restoring: Arc<std::sync::atomic::AtomicBool>,
    workers: tokio_util::task::TaskTracker,
}

impl BackupsState {
    pub(super) fn new() -> Self {
        Self {
            coordinator: Arc::new(Mutex::new(Coordinator::default())),
            restoring: Arc::default(),
            workers: tokio_util::task::TaskTracker::new(),
        }
    }
}

#[derive(Default)]
struct Coordinator {
    jobs: HashMap<String, BackupJob>,
    job_order: VecDeque<String>,
    active_job: Option<String>,
    restore_active: bool,
    recovery: Option<restore::FailedRestore>,
    pending_imports: HashMap<String, PendingImport>,
}

struct BackupJob {
    snapshot: BackupJobSnapshot,
    updates: watch::Sender<BackupJobSnapshot>,
    cancellation: CancellationToken,
}

struct PendingImport {
    file: tempfile::NamedTempFile,
    trust_mode: Option<TrustMode>,
    created_at: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
struct BackupScope {
    #[serde(default = "enabled")]
    include_agents: bool,
    #[serde(default = "enabled")]
    include_global_config: bool,
    #[serde(default)]
    include_secrets: bool,
    #[serde(default = "enabled")]
    include_skill_pool: bool,
}

impl Default for BackupScope {
    fn default() -> Self {
        Self {
            include_agents: true,
            include_global_config: true,
            include_secrets: false,
            include_skill_pool: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BackupMeta {
    id: String,
    name: String,
    #[serde(default)]
    description: String,
    created_at: DateTime<Utc>,
    version: String,
    scope: BackupScope,
    agent_count: usize,
    qwenpaw_version: String,
    system_info: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    accepted_via_trust: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupJobSnapshot {
    job_id: String,
    backup_id: String,
    status: String,
    phase: String,
    percent: u8,
    current_agent: Option<String>,
    agent_index: usize,
    total_agents: usize,
    result: Option<BackupMeta>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateBackupRequest {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    scope: BackupScope,
    #[serde(default)]
    agents: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DeleteBackupsRequest {
    ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum TrustMode {
    Legacy,
    Foreign,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: String,
    entries: BTreeMap<String, ManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestEntry {
    size: u64,
    sha256: String,
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct SecretSnapshot {
    version: u32,
    #[serde(deserialize_with = "required_api_key")]
    api_key: Option<String>,
    environment: BTreeMap<String, String>,
    agent_settings: BTreeMap<String, String>,
    mcp_clients: BTreeMap<String, String>,
    model_providers: BTreeMap<String, String>,
    #[serde(default)]
    oauth: Option<qwenpaw_core::McpOAuthBackup>,
}

fn required_api_key<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    Option::deserialize(deserializer)
}

#[derive(Clone)]
struct ArchiveSource {
    path: PathBuf,
    archive_name: String,
}

struct BackupInputs {
    workspaces: Vec<(String, String, PathBuf)>,
    agent_ids: Vec<String>,
    agent_snapshot: super::desktop_agents::AgentBackupSnapshot,
    global_agent_snapshot: Option<super::desktop_agents::AgentRegistryBackup>,
}

#[derive(Debug)]
enum ArchiveTrust {
    Local,
    Legacy,
    Foreign,
}

struct ValidatedArchive {
    meta: BackupMeta,
    trust: ArchiveTrust,
}

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route("/api/backups", get(list_backups))
        .route("/api/backups/stream", post(create_backup_stream))
        .route("/api/backups/jobs", post(start_backup_job))
        .route("/api/backups/jobs/active", get(active_backup_job))
        .route("/api/backups/jobs/{job_id}", get(get_backup_job))
        .route("/api/backups/jobs/{job_id}/events", get(backup_job_events))
        .route("/api/backups/jobs/{job_id}/cancel", post(cancel_backup_job))
        .route("/api/backups/delete", post(delete_backups))
        .route("/api/backups/import", post(import_backup))
        .route("/api/backups/{backup_id}", get(get_backup))
        .route("/api/backups/{backup_id}/export", get(export_backup))
        .route(
            "/api/backups/{backup_id}/restore",
            post(restore::restore_backup),
        )
        .layer(DefaultBodyLimit::max(512 * 1024 * 1024))
}

pub(super) fn is_restoring(server: &AppServer) -> bool {
    server
        .inner
        .desktop_backups
        .as_ref()
        .is_some_and(|state| state.restoring.load(std::sync::atomic::Ordering::Acquire))
}

async fn list_backups(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let directory = backups_directory(&server)?;
    let credentials = credentials(&server)?.clone();
    let values =
        tokio::task::spawn_blocking(move || list_archives(&directory, credentials.as_ref()))
            .await
            .map_err(|_| internal("Backup list task failed"))?
            .map_err(|_| internal("Backups could not be listed"))?;
    Ok(Json(Value::Array(values)))
}

async fn get_backup(
    State(server): State<AppServer>,
    AxumPath(backup_id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    validate_backup_id(&backup_id)?;
    let directory = backups_directory(&server)?;
    let credentials = credentials(&server)?.clone();
    let detail = tokio::task::spawn_blocking(move || {
        archive_detail(&directory, &backup_id, credentials.as_ref())
    })
    .await
    .map_err(|_| internal("Backup detail task failed"))?
    .map_err(|_| internal("Backup could not be read"))?
    .ok_or_else(|| not_found("Backup not found"))?;
    Ok(Json(detail))
}

async fn start_backup_job(
    State(server): State<AppServer>,
    Json(request): Json<CreateBackupRequest>,
) -> Result<(StatusCode, Json<BackupJobSnapshot>), ApiError> {
    let snapshot = launch_backup_job(&server, request).await?;
    Ok((StatusCode::ACCEPTED, Json(snapshot)))
}

async fn active_backup_job(
    State(server): State<AppServer>,
) -> Result<Json<Option<BackupJobSnapshot>>, ApiError> {
    let state = backup_state(&server)?;
    let coordinator = state.coordinator.lock().await;
    let snapshot = coordinator
        .active_job
        .as_ref()
        .and_then(|job_id| coordinator.jobs.get(job_id))
        .map(|job| job.snapshot.clone());
    Ok(Json(snapshot))
}

async fn get_backup_job(
    State(server): State<AppServer>,
    AxumPath(job_id): AxumPath<String>,
) -> Result<Json<BackupJobSnapshot>, ApiError> {
    let state = backup_state(&server)?;
    let coordinator = state.coordinator.lock().await;
    coordinator
        .jobs
        .get(&job_id)
        .map(|job| Json(job.snapshot.clone()))
        .ok_or_else(|| not_found("Backup job not found"))
}

async fn cancel_backup_job(
    State(server): State<AppServer>,
    AxumPath(job_id): AxumPath<String>,
) -> Result<Json<BackupJobSnapshot>, ApiError> {
    let state = backup_state(&server)?;
    let mut coordinator = state.coordinator.lock().await;
    let job = coordinator
        .jobs
        .get_mut(&job_id)
        .ok_or_else(|| not_found("Backup job not found"))?;
    if !is_terminal(&job.snapshot.status) {
        job.cancellation.cancel();
        job.snapshot.status = String::from("cancel_requested");
        job.updates.send_replace(job.snapshot.clone());
    }
    Ok(Json(job.snapshot.clone()))
}

async fn backup_job_events(
    State(server): State<AppServer>,
    AxumPath(job_id): AxumPath<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let receiver = subscribe_job(&server, &job_id).await?;
    Ok(snapshot_sse(receiver, false))
}

async fn create_backup_stream(
    State(server): State<AppServer>,
    Json(request): Json<CreateBackupRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let snapshot = launch_backup_job(&server, request).await?;
    let receiver = subscribe_job(&server, &snapshot.job_id).await?;
    Ok(snapshot_sse(receiver, true))
}

fn snapshot_sse(
    mut receiver: watch::Receiver<BackupJobSnapshot>,
    legacy: bool,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = async_stream::stream! {
        loop {
            let snapshot = receiver.borrow_and_update().clone();
            let payload = if legacy {
                legacy_event(&snapshot)
            } else {
                serde_json::to_value(&snapshot).unwrap_or(Value::Null)
            };
            yield Ok(Event::default().data(payload.to_string()));
            if is_terminal(&snapshot.status) {
                break;
            }
            if receiver.changed().await.is_err() {
                break;
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

fn legacy_event(snapshot: &BackupJobSnapshot) -> Value {
    match snapshot.status.as_str() {
        "completed" => json!({"type": "done", "meta": snapshot.result, "percent": 100}),
        "failed" | "cancelled" => json!({
            "type": "error",
            "message": snapshot.error.as_deref().unwrap_or("Backup cancelled")
        }),
        _ if snapshot.phase == "finalizing" => {
            json!({"type": "saving", "percent": snapshot.percent})
        }
        _ if snapshot.current_agent.is_some() => json!({
            "type": "agent",
            "agent_id": snapshot.current_agent,
            "index": snapshot.agent_index,
            "total": snapshot.total_agents,
            "percent": snapshot.percent
        }),
        _ => json!({"type": "start", "total_agents": snapshot.total_agents, "percent": 0}),
    }
}

async fn subscribe_job(
    server: &AppServer,
    job_id: &str,
) -> Result<watch::Receiver<BackupJobSnapshot>, ApiError> {
    let state = backup_state(server)?;
    let coordinator = state.coordinator.lock().await;
    coordinator
        .jobs
        .get(job_id)
        .map(|job| job.updates.subscribe())
        .ok_or_else(|| not_found("Backup job not found"))
}

#[allow(clippy::too_many_lines)]
async fn launch_backup_job(
    server: &AppServer,
    request: CreateBackupRequest,
) -> Result<BackupJobSnapshot, ApiError> {
    let operation = server
        .inner
        .core
        .operation_guard()
        .map_err(|error| conflict(&error.to_string()))?;
    validate_create_request(&request)?;
    let mut agent_snapshot = super::desktop_agents::backup_agent_snapshot(server).await?;
    let global_agent_snapshot = request
        .scope
        .include_global_config
        .then(|| agent_snapshot.registry());
    let all_workspaces = agent_snapshot
        .agents
        .iter()
        .map(|agent| {
            (
                agent.id.clone(),
                agent
                    .config
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(&agent.id)
                    .to_owned(),
                PathBuf::from(&agent.workspace_dir),
            )
        })
        .collect::<Vec<_>>();
    let agent_ids = all_workspaces.iter().map(|(id, _, _)| id.clone()).collect();
    let requested = request
        .agents
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let workspaces = if request.scope.include_agents {
        all_workspaces
            .into_iter()
            .filter(|(id, _, _)| requested.contains(id))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    agent_snapshot
        .agents
        .retain(|agent| request.scope.include_agents && requested.contains(&agent.id));
    let state = backup_state(server)?.clone();
    let job_id = Uuid::now_v7().simple().to_string();
    let backup_id = generated_backup_id();
    let snapshot = BackupJobSnapshot {
        job_id: job_id.clone(),
        backup_id: backup_id.clone(),
        status: String::from("pending"),
        phase: String::from("preparing"),
        percent: 0,
        current_agent: None,
        agent_index: 0,
        total_agents: workspaces.len(),
        result: None,
        error: None,
    };
    let (updates, _) = watch::channel(snapshot.clone());
    let cancellation = server.inner.shutdown.child_token();
    {
        let mut coordinator = state.coordinator.lock().await;
        cleanup_pending_imports(&mut coordinator);
        if coordinator.active_job.is_some() || coordinator.restore_active {
            return Err(conflict("Backup operation already running"));
        }
        coordinator.active_job = Some(job_id.clone());
        coordinator.job_order.push_back(job_id.clone());
        coordinator.jobs.insert(
            job_id.clone(),
            BackupJob {
                snapshot: snapshot.clone(),
                updates,
                cancellation: cancellation.clone(),
            },
        );
        trim_jobs(&mut coordinator);
    }

    let worker_server = server.clone();
    tokio::spawn(async move {
        let _operation = operation.clone();
        update_job(&state, &job_id, |current| {
            if current.status == "pending" {
                current.status = String::from("running");
            }
        })
        .await;
        let task_state = state.clone();
        let progress_job_id = job_id.clone();
        let progress = move |agent: Option<String>, index: usize, total: usize, percent: u8| {
            let state = task_state.clone();
            let job_id = progress_job_id.clone();
            tokio::spawn(async move {
                update_job(&state, &job_id, |current| {
                    current.phase = if percent >= 90 {
                        String::from("finalizing")
                    } else {
                        String::from("agents")
                    };
                    current.current_agent = agent;
                    current.agent_index = index;
                    current.total_agents = total;
                    current.percent = percent;
                })
                .await;
            });
        };
        let result = tokio::task::spawn_blocking(move || {
            let _operation = operation;
            create_archive(
                &worker_server,
                request,
                &backup_id,
                &BackupInputs {
                    workspaces,
                    agent_ids,
                    agent_snapshot,
                    global_agent_snapshot,
                },
                &cancellation,
                progress,
            )
        })
        .await;
        let (status, meta, error) = match result {
            Ok(Ok(meta)) => ("completed", Some(meta), None),
            Ok(Err(CreateError::Cancelled)) => ("cancelled", None, None),
            Ok(Err(CreateError::Failed(message))) => ("failed", None, Some(message)),
            Err(_) => ("failed", None, Some(String::from("Backup worker failed"))),
        };
        update_job(&state, &job_id, |current| {
            current.status = String::from(status);
            current.phase = String::from("finalizing");
            current.percent = if status == "completed" {
                100
            } else {
                current.percent
            };
            current.current_agent = None;
            current.result = meta;
            current.error = error;
        })
        .await;
        let mut coordinator = state.coordinator.lock().await;
        if coordinator.active_job.as_deref() == Some(job_id.as_str()) {
            coordinator.active_job = None;
        }
    });
    Ok(snapshot)
}

async fn update_job(
    state: &BackupsState,
    job_id: &str,
    update: impl FnOnce(&mut BackupJobSnapshot),
) {
    let mut coordinator = state.coordinator.lock().await;
    if let Some(job) = coordinator.jobs.get_mut(job_id) {
        if is_terminal(&job.snapshot.status) {
            return;
        }
        update(&mut job.snapshot);
        job.updates.send_replace(job.snapshot.clone());
        if is_terminal(&job.snapshot.status) && coordinator.active_job.as_deref() == Some(job_id) {
            coordinator.active_job = None;
        }
    }
}

fn trim_jobs(coordinator: &mut Coordinator) {
    while coordinator.job_order.len() > MAX_JOBS {
        let Some(job_id) = coordinator.job_order.pop_front() else {
            break;
        };
        if coordinator.active_job.as_deref() == Some(job_id.as_str()) {
            coordinator.job_order.push_back(job_id);
            break;
        }
        coordinator.jobs.remove(&job_id);
    }
}

async fn delete_backups(
    State(server): State<AppServer>,
    Json(request): Json<DeleteBackupsRequest>,
) -> Result<Json<Value>, ApiError> {
    if request.ids.len() > 1_000 {
        return Err(bad_request("Too many backup IDs"));
    }
    let state = backup_state(&server)?;
    let coordinator = state.coordinator.lock().await;
    if coordinator.active_job.is_some() || coordinator.restore_active {
        return Err(conflict("Backup operation already running"));
    }
    let directory = backups_directory(&server)?;
    let mut deleted = Vec::new();
    let mut failed = Vec::new();
    for id in request.ids {
        if validate_backup_id(&id).is_err() {
            failed.push(json!({"id": id, "reason": "not found"}));
            continue;
        }
        match find_archive(&directory, &id) {
            Ok(Some(path)) => match fs::remove_file(path) {
                Ok(()) => deleted.push(id),
                Err(_) => failed.push(json!({"id": id, "reason": "delete failed"})),
            },
            _ => failed.push(json!({"id": id, "reason": "not found"})),
        }
    }
    Ok(Json(json!({"deleted": deleted, "failed": failed})))
}

async fn export_backup(
    State(server): State<AppServer>,
    AxumPath(backup_id): AxumPath<String>,
) -> Result<Response, ApiError> {
    validate_backup_id(&backup_id)?;
    let path = find_archive(&backups_directory(&server)?, &backup_id)
        .map_err(|_| internal("Backup could not be inspected"))?
        .ok_or_else(|| not_found("Backup not found"))?;
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|_| internal("Backup could not be opened"))?;
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/zip")
        .header(CACHE_CONTROL, "no-store")
        .header(
            CONTENT_DISPOSITION,
            format!("attachment; filename=\"{backup_id}.zip\""),
        )
        .body(Body::from_stream(ReaderStream::new(file)))
        .map_err(|_| internal("Backup response could not be built"))
}

fn enabled() -> bool {
    true
}

fn is_terminal(status: &str) -> bool {
    TERMINAL_STATUSES.contains(&status)
}

fn backup_state(server: &AppServer) -> Result<&BackupsState, ApiError> {
    server
        .inner
        .desktop_backups
        .as_ref()
        .ok_or_else(|| internal("Backup manager is not available"))
}

fn desktop_workspace(server: &AppServer) -> Result<&DesktopWorkspace, ApiError> {
    server
        .inner
        .desktop_workspace
        .as_ref()
        .ok_or_else(|| internal("Desktop Workspace is unavailable"))
}

fn credentials(server: &AppServer) -> Result<&Arc<dyn DesktopCredentialStore>, ApiError> {
    server
        .inner
        .desktop_credentials
        .as_ref()
        .ok_or_else(|| internal("Desktop credential storage is unavailable"))
}

fn backups_directory(server: &AppServer) -> Result<PathBuf, ApiError> {
    Ok(desktop_workspace(server)?.data_dir.join("backups"))
}

fn bad_request(detail: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, Json(json!({"detail": detail})))
}

fn conflict(detail: &str) -> ApiError {
    (StatusCode::CONFLICT, Json(json!({"detail": detail})))
}

fn not_found(detail: &str) -> ApiError {
    (StatusCode::NOT_FOUND, Json(json!({"detail": detail})))
}

fn internal(detail: &str) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"detail": detail})),
    )
}

fn validation_error(code: &str, message: &str) -> ApiError {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"detail": {"code": code, "message": message}})),
    )
}

fn validate_backup_id(id: &str) -> Result<(), ApiError> {
    validate_backup_id_raw(id).map_err(|_| bad_request("Invalid backup id"))
}

fn validate_create_request(request: &CreateBackupRequest) -> Result<(), ApiError> {
    if request.name.trim().is_empty() || request.name.len() > 256 {
        return Err(bad_request("Backup name is invalid"));
    }
    if request.description.len() > 16 * 1024 || request.agents.len() > 256 {
        return Err(bad_request("Backup request is too large"));
    }
    for id in &request.agents {
        validate_backup_id(id)?;
    }
    Ok(())
}

fn generated_backup_id() -> String {
    let suffix = Uuid::now_v7().simple().to_string();
    format!(
        "qwenpaw-{}-{}-{suffix}",
        env!("CARGO_PKG_VERSION"),
        Utc::now().format("%Y%m%dT%H%M%SZ")
    )
}

pub(super) async fn shutdown(server: &AppServer) {
    let Ok(state) = backup_state(server) else {
        return;
    };
    state.workers.close();
    state.workers.wait().await;
    restore::recover_on_shutdown(server).await;
    {
        let mut coordinator = state.coordinator.lock().await;
        if let Some(job_id) = coordinator.active_job.clone()
            && let Some(job) = coordinator.jobs.get_mut(&job_id)
        {
            job.cancellation.cancel();
            job.snapshot.status = String::from("cancel_requested");
            job.updates.send_replace(job.snapshot.clone());
        }
        coordinator.pending_imports.clear();
    }
    for _ in 0..100 {
        if state.coordinator.lock().await.active_job.is_none() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    warn!("timed out waiting for the active backup job to stop");
}

#[derive(Debug)]
enum CreateError {
    Cancelled,
    Failed(String),
}

impl From<std::io::Error> for CreateError {
    fn from(_: std::io::Error) -> Self {
        Self::Failed(String::from("Backup filesystem operation failed"))
    }
}

impl From<zip::result::ZipError> for CreateError {
    fn from(_: zip::result::ZipError) -> Self {
        Self::Failed(String::from("Backup archive operation failed"))
    }
}

#[allow(clippy::too_many_lines)]
fn create_archive(
    server: &AppServer,
    request: CreateBackupRequest,
    backup_id: &str,
    inputs: &BackupInputs,
    cancellation: &CancellationToken,
    progress: impl Fn(Option<String>, usize, usize, u8),
) -> Result<BackupMeta, CreateError> {
    // Checkpoint manifests and referenced archives must not change between
    // validation and copying. This function runs only in a blocking worker.
    let _checkpoints = request
        .scope
        .include_agents
        .then(|| server.inner.desktop_checkpoint_lock.blocking_lock());
    let desktop = desktop_workspace(server)
        .map_err(|_| CreateError::Failed(String::from("Desktop Workspace is unavailable")))?;
    let credentials = credentials(server)
        .map_err(|_| CreateError::Failed(String::from("Credential storage is unavailable")))?;
    let directory = desktop.data_dir.join("backups");
    fs::create_dir_all(&directory)?;
    let mut sources = Vec::new();
    let workspaces = &inputs.workspaces;
    let workspace_bindings = inputs
        .agent_snapshot
        .agents
        .iter()
        .filter_map(|agent| agent.data_key.clone().map(|key| (agent.id.clone(), key)))
        .collect();
    let total_agents = workspaces.len();
    let mut core_snapshot = if request.scope.include_agents || request.scope.include_global_config {
        Some(
            server
                .inner
                .core
                .backup_snapshot(MAX_FILE_BYTES)
                .map_err(|_| CreateError::Failed(String::from("Core data snapshot failed")))?,
        )
    } else {
        None
    };
    // Capture effective MCP configuration and its secret fields together. A
    // bootstrap manager need not have persisted any Desktop MCP settings yet.
    let (mcp_data, secret_snapshot, model_registry) = {
        let _models = (request.scope.include_global_config || request.scope.include_secrets)
            .then(|| server.inner.desktop_models_lock.blocking_lock());
        let _mcp = (request.scope.include_global_config || request.scope.include_secrets)
            .then(|| server.inner.desktop_mcp_lock.blocking_lock());
        let _environment = request
            .scope
            .include_secrets
            .then(|| server.inner.desktop_environment_lock.blocking_lock());
        let data = request
            .scope
            .include_global_config
            .then(|| super::desktop_mcp::backup_data(&server.inner.core))
            .transpose()
            .map_err(|error| CreateError::Failed(error.to_owned()))?;
        let secrets = request
            .scope
            .include_secrets
            .then(|| collect_secret_snapshot(server, &inputs.agent_ids))
            .transpose()
            .map_err(CreateError::Failed)?;
        let models = request
            .scope
            .include_global_config
            .then(|| super::desktop_models::restore_registry_bytes(server))
            .transpose()
            .map_err(|error| CreateError::Failed(error.to_owned()))?;
        (data, secrets, models)
    };
    let owners = if request.scope.include_agents {
        core_snapshot
            .as_ref()
            .map(|snapshot| {
                super::desktop_chats::backup_thread_owners(snapshot, &workspace_bindings)
            })
            .transpose()
            .map_err(|error| CreateError::Failed(error.to_owned()))?
            .unwrap_or_default()
    } else {
        BTreeMap::new()
    };
    let mut checkpoint_states = BTreeMap::new();
    if request.scope.include_agents {
        for (index, (agent_id, _, workspace)) in workspaces.iter().enumerate() {
            check_cancelled(cancellation)?;
            progress(
                Some(agent_id.clone()),
                index,
                total_agents,
                u8::try_from(10 + 75 * index / total_agents.max(1)).unwrap_or(85),
            );
            collect_sources(
                workspace,
                &format!("{WORKSPACE_PREFIX}{agent_id}"),
                &mut sources,
                cancellation,
                |relative| {
                    desktop.data_dir.starts_with(workspace)
                        && workspace.join(relative).starts_with(&desktop.data_dir)
                },
            )?;
            let checkpoint = super::desktop_checkpoints::scoped_backup_sources(
                server,
                workspace,
                workspace_bindings.get(agent_id).ok_or_else(|| {
                    CreateError::Failed(String::from("Checkpoint Workspace binding is missing"))
                })?,
                owners
                    .get(agent_id)
                    .unwrap_or(&std::collections::BTreeSet::new()),
            )
            .map_err(|_| {
                CreateError::Failed(String::from("Checkpoint data is invalid or unsafe"))
            })?;
            if let Some(state) = checkpoint.state {
                checkpoint_states
                    .insert(format!("{CHECKPOINT_PREFIX}{agent_id}/state.json"), state);
            }
            sources.extend(
                checkpoint
                    .archives
                    .into_iter()
                    .map(|relative| ArchiveSource {
                        path: checkpoint.directory.join(&relative),
                        archive_name: format!("{CHECKPOINT_PREFIX}{agent_id}/{relative}"),
                    }),
            );
        }
    }
    if request.scope.include_global_config {
        collect_sources(
            &desktop.data_dir,
            CONFIG_PREFIX.trim_end_matches('/'),
            &mut sources,
            cancellation,
            |relative| {
                let first = relative.components().next();
                matches!(
                    first,
                    Some(Component::Normal(name))
                        if name == "backups"
                            || name == "agents"
                            || name == "workspaces"
                            || name == "skill_pool"
                            || name == "local-models"
                            || name == "checkpoints"
                ) || relative == Path::new("agent-registry.json")
                    || relative == Path::new("models/registry.json")
                    || (desktop.data_dir.join(relative).is_file()
                        && !is_global_config_file(relative))
            },
        )?;
    }
    if request.scope.include_skill_pool {
        collect_sources(
            &desktop.data_dir.join("skill_pool"),
            SKILL_POOL_PREFIX.trim_end_matches('/'),
            &mut sources,
            cancellation,
            |_| false,
        )?;
    }
    let source_bytes = sources.iter().try_fold(0_u64, |total, source| {
        fs::metadata(&source.path).map(|metadata| total.saturating_add(metadata.len()))
    })?;
    if sources.len() > MAX_ARCHIVE_FILES || source_bytes > MAX_ARCHIVE_BYTES {
        return Err(CreateError::Failed(String::from(
            "Backup contains too many files",
        )));
    }
    sources.sort_by(|left, right| left.archive_name.cmp(&right.archive_name));
    let mut names = std::collections::BTreeSet::new();
    if sources
        .iter()
        .any(|source| !names.insert(source.archive_name.to_lowercase()))
    {
        return Err(CreateError::Failed(String::from(
            "Backup contains conflicting file names",
        )));
    }

    let created_at = Utc::now();
    let mut meta = BackupMeta {
        id: backup_id.to_owned(),
        name: request.name,
        description: request.description,
        created_at,
        version: String::from(FORMAT_VERSION),
        scope: request.scope,
        agent_count: total_agents,
        qwenpaw_version: String::from(env!("CARGO_PKG_VERSION")),
        system_info: json!({
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "backend": "rust-core"
        }),
        signature: None,
        accepted_via_trust: Some(false),
    };
    let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut manifest = Manifest {
        format: String::from(FORMAT_VERSION),
        entries: BTreeMap::new(),
    };
    {
        let mut writer = ZipWriter::new(temporary.as_file_mut());
        for source in &sources {
            check_cancelled(cancellation)?;
            write_source(&mut writer, source, options, &mut manifest, cancellation)?;
        }
        for (name, bytes) in &checkpoint_states {
            check_cancelled(cancellation)?;
            write_bytes(&mut writer, name, bytes, options, &mut manifest)?;
        }
        if let Some(bytes) = &model_registry {
            write_bytes(
                &mut writer,
                "data/config/models/registry.json",
                bytes,
                options,
                &mut manifest,
            )?;
        }
        if meta.scope.include_agents {
            let bytes = serde_json::to_vec(&inputs.agent_snapshot)
                .map_err(|_| CreateError::Failed(String::from("Agent snapshot failed")))?;
            write_bytes(
                &mut writer,
                AGENT_STATE_FILE,
                &bytes,
                options,
                &mut manifest,
            )?;
        }
        if let Some(registry) = &inputs.global_agent_snapshot {
            let bytes = serde_json::to_vec(registry)
                .map_err(|_| CreateError::Failed(String::from("Agent registry snapshot failed")))?;
            write_bytes(
                &mut writer,
                GLOBAL_AGENT_STATE_FILE,
                &bytes,
                options,
                &mut manifest,
            )?;
        }
        if meta.scope.include_agents || meta.scope.include_global_config {
            let mut snapshot = core_snapshot.take().ok_or_else(|| {
                CreateError::Failed(String::from("Core data snapshot is missing"))
            })?;
            let selected = workspaces
                .iter()
                .map(|(id, _, _)| id.as_str())
                .collect::<std::collections::BTreeSet<_>>();
            super::desktop_chats::filter_backup_threads(
                snapshot
                    .settings
                    .get("desktop_chat_catalog_data")
                    .map(String::as_str),
                &selected,
                &mut snapshot.threads,
                &workspace_bindings,
            )
            .map_err(|error| CreateError::Failed(error.to_owned()))?;
            let thread_ids = snapshot
                .threads
                .iter()
                .map(|thread| thread.thread.id.as_str())
                .collect::<std::collections::BTreeSet<_>>();
            filter_backup_settings(
                &mut snapshot.settings,
                &selected,
                &thread_ids,
                meta.scope.include_global_config,
                &inputs
                    .agent_snapshot
                    .agents
                    .iter()
                    .filter_map(|agent| agent.data_key.clone().map(|key| (agent.id.clone(), key)))
                    .collect(),
            )
            .map_err(|error| CreateError::Failed(error.to_owned()))?;
            super::desktop_usage::filter_backup(&mut snapshot, &selected, &workspace_bindings)
                .map_err(|error| CreateError::Failed(error.to_owned()))?;
            if let Some(mcp_data) = mcp_data {
                snapshot
                    .settings
                    .insert(String::from("desktop_mcp_data"), mcp_data);
            }
            let bytes = serde_json::to_vec(&snapshot)
                .map_err(|_| CreateError::Failed(String::from("Core data snapshot failed")))?;
            write_bytes(&mut writer, CORE_STATE_FILE, &bytes, options, &mut manifest)?;
        }
        if meta.scope.include_secrets {
            let snapshot = secret_snapshot
                .as_ref()
                .ok_or_else(|| CreateError::Failed(String::from("Secret snapshot is missing")))?;
            let bytes = serde_json::to_vec(&snapshot)
                .map_err(|_| CreateError::Failed(String::from("Secret snapshot failed")))?;
            write_bytes(&mut writer, SECRETS_FILE, &bytes, options, &mut manifest)?;
        }
        progress(None, total_agents, total_agents, 90);
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|_| CreateError::Failed(String::from("Backup manifest failed")))?;
        let key = signing_key(credentials.as_ref()).map_err(CreateError::Failed)?;
        meta.signature = Some(sign_meta(&meta, &manifest_bytes, &key)?);
        let meta_bytes = serde_json::to_vec_pretty(&meta)
            .map_err(|_| CreateError::Failed(String::from("Backup metadata failed")))?;
        writer.start_file(MANIFEST_FILE, options)?;
        writer.write_all(&manifest_bytes)?;
        writer.start_file(META_FILE, options)?;
        writer.write_all(&meta_bytes)?;
        writer.finish()?;
    }
    temporary.as_file_mut().flush()?;
    temporary.as_file().sync_all()?;
    if temporary.as_file().metadata()?.len() > MAX_ARCHIVE_BYTES {
        return Err(CreateError::Failed(String::from(
            "Backup archive is too large",
        )));
    }
    let state = backup_state(server)
        .map_err(|_| CreateError::Failed(String::from("Backup manager is unavailable")))?;
    let mut coordinator = state.coordinator.blocking_lock();
    check_cancelled(cancellation)?;
    let destination = directory.join(format!("{backup_id}.zip"));
    temporary
        .persist_noclobber(destination)
        .map_err(|_| CreateError::Failed(String::from("Backup could not be published")))?;
    if let Some(job_id) = coordinator.active_job.clone()
        && let Some(job) = coordinator.jobs.get_mut(&job_id)
        && job.snapshot.backup_id == meta.id
    {
        job.snapshot.status = String::from("completed");
        job.snapshot.phase = String::from("finalizing");
        job.snapshot.percent = 100;
        job.snapshot.current_agent = None;
        job.snapshot.result = Some(meta.clone());
        job.updates.send_replace(job.snapshot.clone());
        coordinator.active_job = None;
    }
    Ok(meta)
}

fn collect_sources(
    root: &Path,
    archive_root: &str,
    sources: &mut Vec<ArchiveSource>,
    cancellation: &CancellationToken,
    skip: impl Fn(&Path) -> bool + Copy,
) -> Result<(), CreateError> {
    if !root.is_dir() {
        return Ok(());
    }
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        check_cancelled(cancellation)?;
        let mut entries = fs::read_dir(&directory)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            check_cancelled(cancellation)?;
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|_| CreateError::Failed(String::from("Backup path is invalid")))?;
            if skip(relative)
                || entry
                    .file_name()
                    .eq_ignore_ascii_case(super::desktop_agents::identity::MARKER_NAME)
                || entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(super::desktop_restore_files::RECOVERY_PREFIX)
            {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                pending.push(path);
            } else if metadata.is_file() {
                if metadata.len() > MAX_FILE_BYTES {
                    return Err(CreateError::Failed(String::from(
                        "Backup contains an oversized file",
                    )));
                }
                let archive_name = format!(
                    "{archive_root}/{}",
                    relative.to_string_lossy().replace('\\', "/")
                );
                validate_archive_name(&archive_name)
                    .map_err(|_| CreateError::Failed(String::from("Backup path is invalid")))?;
                sources.push(ArchiveSource { path, archive_name });
                if sources.len() > MAX_ARCHIVE_FILES {
                    return Err(CreateError::Failed(String::from(
                        "Backup contains too many files",
                    )));
                }
            }
        }
    }
    Ok(())
}

fn filter_backup_settings(
    settings: &mut BTreeMap<String, String>,
    agent_ids: &std::collections::BTreeSet<&str>,
    thread_ids: &std::collections::BTreeSet<&str>,
    include_global_config: bool,
    workspace_bindings: &BTreeMap<String, super::desktop_agents::identity::WorkspaceDataKey>,
) -> Result<(), &'static str> {
    let mut scoped = BTreeMap::new();
    for key in [
        "desktop_chat_catalog_data",
        "desktop_inbox_data",
        "desktop_mail_access_control_data",
        "desktop_channel_config_data",
        "desktop_cron_data",
        "desktop_heartbeat_data",
    ] {
        let Some(value) = settings.remove(key) else {
            continue;
        };
        if agent_ids.is_empty() {
            continue;
        }
        let value = match key {
            "desktop_chat_catalog_data" => super::desktop_chats::filter_backup_data(
                &value,
                agent_ids,
                thread_ids,
                workspace_bindings,
            )?,
            "desktop_inbox_data" => super::desktop_inbox::filter_backup_data(&value, agent_ids)?,
            "desktop_channel_config_data" => {
                super::desktop_channels::filter_backup_data(&value, agent_ids, workspace_bindings)?
            }
            "desktop_mail_access_control_data" => {
                super::desktop_mail_access_control::filter_backup_data(
                    &value,
                    agent_ids,
                    workspace_bindings,
                )?
            }
            "desktop_cron_data" => {
                match super::desktop_cron::filter_for_bindings(&value, workspace_bindings)? {
                    Some(value) => value,
                    None => continue,
                }
            }
            // Heartbeat currently belongs to the default Agent.
            _ if agent_ids.contains("default") => value,
            _ => continue,
        };
        scoped.insert(key.to_owned(), value);
    }
    if !include_global_config {
        settings.clear();
    }
    settings.extend(scoped);
    Ok(())
}

fn is_global_config_file(path: &Path) -> bool {
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "json" | "yaml" | "yml" | "toml"
            )
        })
}

fn write_source<W: Write + Seek>(
    writer: &mut ZipWriter<W>,
    source: &ArchiveSource,
    options: SimpleFileOptions,
    manifest: &mut Manifest,
    cancellation: &CancellationToken,
) -> Result<(), CreateError> {
    if manifest.entries.len() >= MAX_ARCHIVE_FILES {
        return Err(CreateError::Failed(String::from(
            "Backup contains too many files",
        )));
    }
    if !fs::symlink_metadata(&source.path)?.file_type().is_file() {
        return Err(CreateError::Failed(String::from(
            "Backup source is no longer a regular file",
        )));
    }
    writer.start_file(&source.archive_name, options)?;
    let mut input = fs::File::open(&source.path)?;
    let archived_bytes: u64 = manifest.entries.values().map(|entry| entry.size).sum();
    let mut digest = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        check_cancelled(cancellation)?;
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        size = size.saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
        if size > MAX_FILE_BYTES || archived_bytes.saturating_add(size) > MAX_ARCHIVE_BYTES {
            return Err(CreateError::Failed(String::from(
                "Backup contains an oversized file",
            )));
        }
        digest.update(&buffer[..count]);
        writer.write_all(&buffer[..count])?;
    }
    manifest.entries.insert(
        source.archive_name.clone(),
        ManifestEntry {
            size,
            sha256: format!("{:x}", digest.finalize()),
        },
    );
    Ok(())
}

fn write_bytes<W: Write + Seek>(
    writer: &mut ZipWriter<W>,
    name: &str,
    bytes: &[u8],
    options: SimpleFileOptions,
    manifest: &mut Manifest,
) -> Result<(), CreateError> {
    if manifest.entries.len() >= MAX_ARCHIVE_FILES {
        return Err(CreateError::Failed(String::from(
            "Backup contains too many files",
        )));
    }
    let archived_bytes: u64 = manifest.entries.values().map(|entry| entry.size).sum();
    if bytes.len() as u64 > MAX_FILE_BYTES
        || archived_bytes.saturating_add(bytes.len() as u64) > MAX_ARCHIVE_BYTES
    {
        return Err(CreateError::Failed(String::from(
            "Backup payload is too large",
        )));
    }
    writer.start_file(name, options)?;
    writer.write_all(bytes)?;
    manifest.entries.insert(
        String::from(name),
        ManifestEntry {
            size: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            sha256: format!("{:x}", Sha256::digest(bytes)),
        },
    );
    Ok(())
}

fn check_cancelled(cancellation: &CancellationToken) -> Result<(), CreateError> {
    if cancellation.is_cancelled() {
        Err(CreateError::Cancelled)
    } else {
        Ok(())
    }
}

fn signing_key(credentials: &dyn DesktopCredentialStore) -> Result<Vec<u8>, String> {
    let _guard = SIGNING_KEY_LOCK
        .lock()
        .map_err(|_| String::from("Backup signing lock is unavailable"))?;
    if let Some(encoded) = credentials
        .load_backup_signing_key()
        .map_err(|_| String::from("Backup signing key could not be loaded"))?
    {
        return decode_hex(&encoded)
            .filter(|key| key.len() == 32)
            .ok_or_else(|| String::from("Backup signing key is invalid"));
    }
    let mut key = [0_u8; 32];
    rand::rng().fill_bytes(&mut key);
    let encoded = encode_hex(&key);
    credentials
        .save_backup_signing_key(&encoded)
        .map_err(|_| String::from("Backup signing key could not be saved"))?;
    Ok(key.to_vec())
}

fn sign_meta(meta: &BackupMeta, manifest_bytes: &[u8], key: &[u8]) -> Result<String, CreateError> {
    let mut unsigned = meta.clone();
    unsigned.signature = None;
    let metadata = serde_json::to_vec(&unsigned)
        .map_err(|_| CreateError::Failed(String::from("Backup metadata failed")))?;
    let mut mac = HmacSha256::new_from_slice(key)
        .map_err(|_| CreateError::Failed(String::from("Backup signing failed")))?;
    mac.update(FORMAT_VERSION.as_bytes());
    mac.update(&[0]);
    mac.update(&metadata);
    mac.update(&[0]);
    mac.update(manifest_bytes);
    Ok(format!(
        "hmac-sha256-v1:{}",
        encode_hex(&mac.finalize().into_bytes())
    ))
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let high = hex_nibble(pair[0])?;
            let low = hex_nibble(pair[1])?;
            Some((high << 4) | low)
        })
        .collect()
}

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn collect_secret_snapshot(
    server: &AppServer,
    agent_ids: &[String],
) -> Result<SecretSnapshot, String> {
    let credentials =
        credentials(server).map_err(|_| String::from("Credential storage is unavailable"))?;
    let mut snapshot = SecretSnapshot {
        version: 1,
        environment: super::desktop_environment::backup_values(
            &server.inner.core,
            credentials.as_ref(),
        )
        .map_err(str::to_owned)?,
        mcp_clients: super::desktop_mcp::backup_effective_secrets(&server.inner.core)
            .map_err(str::to_owned)?,
        oauth: Some(
            server
                .inner
                .core
                .backup_oauth_credentials()
                .map_err(|_| String::from("OAuth credentials could not be loaded"))?,
        ),
        ..SecretSnapshot::default()
    };
    for key in secrets::known_keys(server, agent_ids).map_err(str::to_owned)? {
        if matches!(
            key,
            restore_credentials::CredentialKey::McpClient(_)
                | restore_credentials::CredentialKey::Environment(_)
        ) {
            continue;
        }
        let Some(value) = key
            .load(credentials.as_ref())
            .map_err(|_| String::from("Backup credential could not be loaded"))?
        else {
            continue;
        };
        match key {
            restore_credentials::CredentialKey::ApiKey => snapshot.api_key = Some(value),
            restore_credentials::CredentialKey::Environment(key) => {
                snapshot.environment.insert(key, value);
            }
            restore_credentials::CredentialKey::McpClient(key) => {
                snapshot.mcp_clients.insert(key, value);
            }
            restore_credentials::CredentialKey::AgentSetting(key) => {
                if let Some(provider) = key.strip_prefix("model-provider-api-key:") {
                    snapshot.model_providers.insert(provider.to_owned(), value);
                } else {
                    snapshot.agent_settings.insert(key, value);
                }
            }
        }
    }
    // Runtime-injected values are authoritative for the active provider,
    // including an explicit absence. Never attach a key to a different URL.
    if let Some((provider, value)) =
        super::desktop_models::backup_runtime_credential(server).map_err(str::to_owned)?
    {
        if provider == "openai-compatible" {
            snapshot.api_key = value;
        } else if let Some(value) = value {
            snapshot.model_providers.insert(provider, value);
        } else {
            snapshot.model_providers.remove(&provider);
        }
    }
    snapshot.validate().map_err(str::to_owned)?;
    Ok(snapshot)
}

fn list_archives(
    directory: &Path,
    credentials: &dyn DesktopCredentialStore,
) -> Result<Vec<Value>, String> {
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let _ = credentials;
    let mut entries = Vec::new();
    for entry in fs::read_dir(directory).map_err(|_| String::from("Backup directory failed"))? {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if path.extension().and_then(std::ffi::OsStr::to_str) != Some("zip") {
            continue;
        }
        if !fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_file()) {
            continue;
        }
        let Ok(meta) = read_archive_meta(&path) else {
            continue;
        };
        if validate_backup_id_raw(&meta.id).is_ok() {
            entries.push(meta);
        }
    }
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.created_at));
    Ok(entries.into_iter().map(public_meta).collect())
}

fn archive_detail(
    directory: &Path,
    backup_id: &str,
    credentials: &dyn DesktopCredentialStore,
) -> Result<Option<Value>, String> {
    let Some(path) = find_archive(directory, backup_id)? else {
        return Ok(None);
    };
    let key = signing_key(credentials)?;
    let validated = validate_archive(&path, &key)?;
    let file = fs::File::open(path).map_err(|_| String::from("Backup could not be opened"))?;
    let mut archive = ZipArchive::new(file).map_err(|_| String::from("Backup is invalid"))?;
    let mut stats = serde_json::Map::new();
    if archive.file_names().any(|name| name == AGENT_STATE_FILE) {
        let snapshot: super::desktop_agents::AgentBackupSnapshot =
            read_json_entry(&mut archive, AGENT_STATE_FILE, MAX_FILE_BYTES)?;
        for agent in snapshot.agents {
            stats.insert(
                agent.id.clone(),
                json!({
                    "files": 0_u64,
                    "size": 0_u64,
                    "name": agent.config.get("name").and_then(Value::as_str).unwrap_or(&agent.id)
                }),
            );
        }
    }
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|_| String::from("Backup is invalid"))?;
        let Some(relative) = entry.name().strip_prefix(WORKSPACE_PREFIX) else {
            continue;
        };
        let Some((agent_id, file_name)) = relative.split_once('/') else {
            continue;
        };
        if file_name.is_empty() {
            continue;
        }
        let value = stats
            .entry(String::from(agent_id))
            .or_insert_with(|| json!({"files": 0_u64, "size": 0_u64}));
        if let Some(object) = value.as_object_mut() {
            let files = object.get("files").and_then(Value::as_u64).unwrap_or(0) + 1;
            let size = object.get("size").and_then(Value::as_u64).unwrap_or(0) + entry.size();
            object.insert(String::from("files"), json!(files));
            object.insert(String::from("size"), json!(size));
        }
    }
    for (agent_id, stats) in &mut stats {
        if let Ok(config) = read_json_entry::<Value, _>(
            &mut archive,
            &format!("{WORKSPACE_PREFIX}{agent_id}/agent.json"),
            512 * 1024,
        ) && let Some(name) = config.get("name").and_then(Value::as_str)
        {
            stats["name"] = json!(name);
        }
    }
    let mut result = public_meta(validated.meta);
    if let Some(object) = result.as_object_mut() {
        object.insert(String::from("workspace_stats"), Value::Object(stats));
    }
    Ok(Some(result))
}

fn public_meta(mut meta: BackupMeta) -> Value {
    if meta.signature.is_none() {
        meta.accepted_via_trust = None;
    }
    meta.signature = None;
    serde_json::to_value(meta).unwrap_or(Value::Null)
}

fn find_archive(directory: &Path, backup_id: &str) -> Result<Option<PathBuf>, String> {
    let canonical = directory.join(format!("{backup_id}.zip"));
    validate_backup_id_raw(backup_id)?;
    if fs::symlink_metadata(&canonical).is_ok_and(|metadata| metadata.file_type().is_file()) {
        return Ok(Some(canonical));
    }
    if !directory.is_dir() {
        return Ok(None);
    }
    for entry in fs::read_dir(directory).map_err(|_| String::from("Backup directory failed"))? {
        let path = entry
            .map_err(|_| String::from("Backup directory failed"))?
            .path();
        if path.extension().and_then(std::ffi::OsStr::to_str) != Some("zip")
            || !fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_file())
        {
            continue;
        }
        let Ok(file) = fs::File::open(&path) else {
            continue;
        };
        let Ok(mut archive) = ZipArchive::new(file) else {
            continue;
        };
        let Ok(meta) = read_json_entry::<BackupMeta, _>(&mut archive, META_FILE, 256 * 1024) else {
            continue;
        };
        if meta.id == backup_id {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

#[allow(clippy::too_many_lines)]
fn validate_archive(path: &Path, key: &[u8]) -> Result<ValidatedArchive, String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| String::from("Backup is unavailable"))?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_ARCHIVE_BYTES {
        return Err(String::from("Backup archive is too large"));
    }
    let file = fs::File::open(path).map_err(|_| String::from("Backup could not be opened"))?;
    let mut archive = ZipArchive::new(file).map_err(|_| String::from("Backup is not a ZIP"))?;
    validate_open_archive(&mut archive, key)
}

#[allow(clippy::too_many_lines)]
fn validate_open_archive<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    key: &[u8],
) -> Result<ValidatedArchive, String> {
    if archive.len() > MAX_ARCHIVE_FILES + 2 {
        return Err(String::from("Backup contains too many files"));
    }
    let meta = read_json_entry::<BackupMeta, _>(archive, META_FILE, 256 * 1024)?;
    validate_backup_id_raw(&meta.id)?;
    if meta.version != FORMAT_VERSION {
        return Err(String::from("Unsupported backup version"));
    }
    if meta.name.trim().is_empty()
        || meta.name.len() > 256
        || meta.description.len() > 16 * 1024
        || meta.agent_count > 256
    {
        return Err(String::from("Backup metadata is invalid"));
    }
    let manifest = read_json_entry::<Manifest, _>(archive, MANIFEST_FILE, 8 * 1024 * 1024)?;
    if manifest.format != FORMAT_VERSION || manifest.entries.len() > MAX_ARCHIVE_FILES {
        return Err(String::from("Backup manifest is invalid"));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| String::from("Backup is invalid"))?;
        let name = String::from(entry.name());
        validate_archive_name(&name)?;
        if !seen.insert(name.to_lowercase()) {
            return Err(String::from("Backup contains duplicate paths"));
        }
        if let Some(mode) = entry.unix_mode()
            && mode & 0o170_000 != 0
            && mode & 0o170_000 != 0o100_000
        {
            return Err(String::from("Backup contains a non-regular file"));
        }
        if name == META_FILE || name == MANIFEST_FILE {
            continue;
        }
        if !(name == CORE_STATE_FILE
            || name == AGENT_STATE_FILE
            || name == SECRETS_FILE
            || name.starts_with(WORKSPACE_PREFIX)
            || name.starts_with(CHECKPOINT_PREFIX)
            || name.starts_with(CONFIG_PREFIX)
            || name.starts_with(SKILL_POOL_PREFIX))
        {
            return Err(String::from("Backup contains an unsupported payload"));
        }
        let expected = manifest
            .entries
            .get(&name)
            .ok_or_else(|| String::from("Backup contains an unlisted file"))?;
        if entry.size() != expected.size || entry.size() > MAX_FILE_BYTES {
            return Err(String::from("Backup file size is invalid"));
        }
        total = total.saturating_add(entry.size());
        if total > MAX_ARCHIVE_BYTES {
            return Err(String::from("Backup expands beyond its size limit"));
        }
        let mut digest = Sha256::new();
        let mut actual = 0_u64;
        let mut buffer = vec![0_u8; 64 * 1024];
        loop {
            let count = entry
                .read(&mut buffer)
                .map_err(|_| String::from("Backup file could not be read"))?;
            if count == 0 {
                break;
            }
            actual = actual.saturating_add(count as u64);
            if actual > expected.size {
                return Err(String::from("Backup file exceeds its declared size"));
            }
            digest.update(&buffer[..count]);
        }
        if actual != expected.size || format!("{:x}", digest.finalize()) != expected.sha256 {
            return Err(String::from("Backup file integrity check failed"));
        }
    }
    if manifest
        .entries
        .keys()
        .any(|name| !seen.contains(&name.to_lowercase()))
    {
        return Err(String::from("Backup manifest references a missing file"));
    }
    let global_agents = if manifest.entries.contains_key(GLOBAL_AGENT_STATE_FILE) {
        let registry: super::desktop_agents::AgentRegistryBackup =
            read_json_entry(archive, GLOBAL_AGENT_STATE_FILE, MAX_FILE_BYTES)?;
        super::desktop_agents::validate_registry_backup(&registry)?;
        if !meta.scope.include_global_config {
            return Err(String::from("Backup global Agent registry is invalid"));
        }
        Some(registry)
    } else {
        None
    };
    if manifest.entries.contains_key(AGENT_STATE_FILE) {
        let snapshot: super::desktop_agents::AgentBackupSnapshot =
            read_json_entry(archive, AGENT_STATE_FILE, MAX_FILE_BYTES)?;
        super::desktop_agents::validate_backup_snapshot(&snapshot)?;
        if let Some(registry) = &global_agents
            && snapshot
                .agents
                .iter()
                .any(|agent| !registry.agents.contains(&agent.reference()))
        {
            return Err(String::from("Backup Agent snapshots disagree"));
        }
        if !meta.scope.include_agents || snapshot.agents.len() != meta.agent_count {
            return Err(String::from(
                "Backup Agent scope does not match its metadata",
            ));
        }
        let ids = snapshot
            .agents
            .iter()
            .map(|agent| agent.id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        for id in &ids {
            validate_archive_name(&format!("{WORKSPACE_PREFIX}{id}/marker"))?;
        }
        for name in manifest.entries.keys() {
            if let Some(relative) = name
                .strip_prefix(WORKSPACE_PREFIX)
                .or_else(|| name.strip_prefix(CHECKPOINT_PREFIX))
                && !relative
                    .split_once('/')
                    .is_some_and(|(id, _)| ids.contains(id))
            {
                return Err(String::from("Backup payload references an unlisted Agent"));
            }
        }
    }
    if manifest.entries.contains_key(SECRETS_FILE) {
        if !meta.scope.include_secrets {
            return Err(String::from(
                "Backup credential payload is outside its declared scope",
            ));
        }
        let snapshot: SecretSnapshot = read_json_entry(archive, SECRETS_FILE, MAX_FILE_BYTES)?;
        snapshot.validate().map_err(str::to_owned)?;
    }
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|_| String::from("Backup manifest is invalid"))?;
    let trust = match meta.signature.as_deref() {
        None => ArchiveTrust::Legacy,
        Some(signature) if verify_meta_signature(&meta, &manifest_bytes, key, signature) => {
            ArchiveTrust::Local
        }
        Some(_) => ArchiveTrust::Foreign,
    };
    Ok(ValidatedArchive { meta, trust })
}

fn verify_meta_signature(
    meta: &BackupMeta,
    manifest_bytes: &[u8],
    key: &[u8],
    signature: &str,
) -> bool {
    let Some(encoded) = signature.strip_prefix("hmac-sha256-v1:") else {
        return false;
    };
    let Some(expected) = decode_hex(encoded) else {
        return false;
    };
    let mut unsigned = meta.clone();
    unsigned.signature = None;
    let Ok(metadata) = serde_json::to_vec(&unsigned) else {
        return false;
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(key) else {
        return false;
    };
    mac.update(FORMAT_VERSION.as_bytes());
    mac.update(&[0]);
    mac.update(&metadata);
    mac.update(&[0]);
    mac.update(manifest_bytes);
    mac.verify_slice(&expected).is_ok()
}

fn read_json_entry<T: for<'de> Deserialize<'de>, R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
    limit: u64,
) -> Result<T, String> {
    let mut entry = archive
        .by_name(name)
        .map_err(|_| String::from("Backup metadata is missing"))?;
    if entry.size() > limit {
        return Err(String::from("Backup metadata is too large"));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
    entry
        .by_ref()
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| String::from("Backup metadata could not be read"))?;
    if bytes.len() as u64 > limit {
        return Err(String::from("Backup metadata is too large"));
    }
    serde_json::from_slice(&bytes).map_err(|_| String::from("Backup metadata is invalid"))
}

pub(super) fn validate_archive_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > MAX_PATH_BYTES
        || name.contains('\\')
        || name.contains('\0')
        || name.starts_with('/')
        || name.chars().any(char::is_control)
    {
        return Err(String::from("Backup contains an unsafe path"));
    }
    for part in name.split('/') {
        let base = part
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with(['.', ' '])
            || part.contains([':', '*', '?', '"', '<', '>', '|'])
            || matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (base.len() == 4
                && (base.starts_with("COM") || base.starts_with("LPT"))
                && matches!(base.as_bytes()[3], b'1'..=b'9'))
        {
            return Err(String::from("Backup contains a non-portable path"));
        }
    }
    let path = Path::new(name);
    if path.components().any(|component| {
        !matches!(component, Component::Normal(_))
            || component.as_os_str().to_string_lossy() == ".."
    }) {
        return Err(String::from("Backup contains an unsafe path"));
    }
    Ok(())
}

fn validate_backup_id_raw(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 200
        || id == "."
        || id == ".."
        || id
            .bytes()
            .any(|byte| !(byte.is_ascii_alphanumeric() || b"._-".contains(&byte)))
    {
        return Err(String::from("Backup ID is invalid"));
    }
    validate_archive_name(id)
}

#[allow(clippy::too_many_lines)]
async fn import_backup(
    State(server): State<AppServer>,
    mut multipart: Multipart,
) -> Result<Response, ApiError> {
    let operation = server
        .inner
        .core
        .operation_guard()
        .map_err(|error| conflict(&error.to_string()))?;
    let directory = backups_directory(&server)?;
    fs::create_dir_all(&directory).map_err(|_| internal("Backup directory is unavailable"))?;
    let mut upload = None;
    let mut trust_mode = None;
    let mut pending_token = None;
    let mut fields = std::collections::BTreeSet::new();
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|_| bad_request("Invalid backup upload"))?
    {
        let name = field.name().unwrap_or_default().to_owned();
        if !fields.insert(name.clone()) {
            return Err(bad_request("Duplicate upload field"));
        }
        match name.as_str() {
            "file" => {
                if field.content_type().is_some_and(|mime| {
                    !matches!(
                        mime,
                        "application/zip"
                            | "application/x-zip-compressed"
                            | "application/octet-stream"
                    )
                }) {
                    return Err(bad_request("Expected a zip file"));
                }
                let temporary = tempfile::NamedTempFile::new_in(&directory)
                    .map_err(|_| internal("Backup upload could not be staged"))?;
                let mut output = tokio::fs::File::from_std(
                    temporary
                        .reopen()
                        .map_err(|_| internal("Backup upload could not be staged"))?,
                );
                let mut bytes = 0_u64;
                while let Some(chunk) = field
                    .chunk()
                    .await
                    .map_err(|_| bad_request("Invalid backup upload"))?
                {
                    bytes = bytes.saturating_add(chunk.len() as u64);
                    if bytes > MAX_UPLOAD_BYTES {
                        return Err(bad_request("Backup upload is too large"));
                    }
                    tokio::io::AsyncWriteExt::write_all(&mut output, &chunk)
                        .await
                        .map_err(|_| internal("Backup upload failed"))?;
                }
                tokio::io::AsyncWriteExt::flush(&mut output)
                    .await
                    .map_err(|_| internal("Backup upload failed"))?;
                upload = Some(temporary);
            }
            "trust_mode" | "pending_token" => {
                let mut bytes = Vec::new();
                while let Some(chunk) = field
                    .chunk()
                    .await
                    .map_err(|_| bad_request("Invalid upload field"))?
                {
                    if bytes.len().saturating_add(chunk.len()) > 256 {
                        return Err(bad_request("Upload field is too large"));
                    }
                    bytes.extend_from_slice(&chunk);
                }
                let value =
                    String::from_utf8(bytes).map_err(|_| bad_request("Invalid upload field"))?;
                if name == "trust_mode" {
                    trust_mode = Some(match value.as_str() {
                        "legacy" => TrustMode::Legacy,
                        "foreign" => TrustMode::Foreign,
                        _ => return Err(bad_request("Invalid trust_mode")),
                    });
                } else {
                    pending_token = Some(value);
                }
            }
            _ => return Err(bad_request("Unknown upload field")),
        }
    }
    let state = backup_state(&server)?.clone();
    let mut coordinator = state.coordinator.clone().lock_owned().await;
    cleanup_pending_imports(&mut coordinator);
    if coordinator.restore_active || coordinator.active_job.is_some() {
        return Err(conflict("Backup operation already running"));
    }
    let overwrite = pending_token.is_some();
    let temporary = if let Some(token) = pending_token {
        if upload.is_some() || trust_mode.is_some() {
            return Err(bad_request(
                "pending_token cannot be combined with upload fields",
            ));
        }
        let pending = coordinator
            .pending_imports
            .remove(&token)
            .ok_or_else(|| bad_request("Invalid or expired pending_token"))?;
        trust_mode = pending.trust_mode;
        pending.file
    } else {
        upload.ok_or_else(|| bad_request("file is required"))?
    };
    let credentials = credentials(&server)?.clone();
    // The owned guard keeps publication serialized even if the HTTP client disconnects.
    tokio::task::spawn_blocking(move || {
        let _operation = operation;
        let key = signing_key(credentials.as_ref())
            .map_err(|_| internal("Backup signing key is unavailable"))?;
        let validated =
            validate_archive(temporary.path(), &key).map_err(|message| bad_request(&message))?;
        require_trust(&validated.trust, trust_mode)?;
        let id = validated.meta.id.clone();
        let existing =
            find_archive(&directory, &id).map_err(|_| internal("Backup could not be inspected"))?;
        if let Some(existing) = existing.as_ref().filter(|_| !overwrite) {
            let existing_meta =
                read_archive_meta(existing).map_err(|_| internal("Existing backup is invalid"))?;
            if coordinator.pending_imports.len() >= MAX_PENDING_IMPORTS {
                return Err(bad_request("Too many pending backup imports"));
            }
            let token = Uuid::now_v7().simple().to_string();
            coordinator.pending_imports.insert(
                token.clone(),
                PendingImport {
                    file: temporary,
                    trust_mode,
                    created_at: SystemTime::now(),
                },
            );
            return Ok((
                StatusCode::CONFLICT,
                Json(json!({
                    "detail": "backup_conflict",
                    "existing": public_meta(existing_meta),
                    "pending_token": token
                })),
            )
                .into_response());
        }
        let mut meta = validated.meta;
        let prepared = if matches!(validated.trust, ArchiveTrust::Local) {
            temporary
        } else {
            meta.accepted_via_trust = Some(true);
            resign_archive(temporary.path(), &directory, &mut meta, &key)
                .map_err(|_| internal("Trusted backup could not be signed"))?
        };
        let destination = directory.join(format!("{id}.zip"));
        prepared
            .persist(&destination)
            .map_err(|_| internal("Backup could not be published"))?;
        if let Some(existing) = existing.filter(|path| path != &destination) {
            fs::remove_file(existing)
                .map_err(|_| internal("Old backup filename could not be removed"))?;
        }
        Ok(Json(public_meta(meta)).into_response())
    })
    .await
    .map_err(|_| internal("Backup import task failed"))?
}

fn cleanup_pending_imports(coordinator: &mut Coordinator) {
    coordinator.pending_imports.retain(|_, pending| {
        pending
            .created_at
            .elapsed()
            .is_ok_and(|age| age < PENDING_IMPORT_TTL)
    });
}

fn require_trust(trust: &ArchiveTrust, mode: Option<TrustMode>) -> Result<(), ApiError> {
    match (trust, mode) {
        (ArchiveTrust::Local, _)
        | (ArchiveTrust::Legacy, Some(TrustMode::Legacy))
        | (ArchiveTrust::Foreign, Some(TrustMode::Foreign)) => Ok(()),
        (ArchiveTrust::Legacy, _) => Err(validation_error(
            "backup_legacy_unsigned",
            "Explicit trust is required for an unsigned backup",
        )),
        (ArchiveTrust::Foreign, _) => Err(validation_error(
            "backup_signature_mismatch",
            "Explicit trust is required for a backup not signed by this installation",
        )),
    }
}

fn read_archive_meta(path: &Path) -> Result<BackupMeta, String> {
    let file = fs::File::open(path).map_err(|_| String::from("Backup could not be opened"))?;
    let mut archive = ZipArchive::new(file).map_err(|_| String::from("Backup is invalid"))?;
    read_json_entry(&mut archive, META_FILE, 256 * 1024)
}

fn resign_archive(
    source: &Path,
    directory: &Path,
    meta: &mut BackupMeta,
    key: &[u8],
) -> Result<tempfile::NamedTempFile, CreateError> {
    let mut archive = ZipArchive::new(fs::File::open(source)?)?;
    resign_open_archive(&mut archive, directory, meta, key)
}

fn resign_open_archive<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    directory: &Path,
    meta: &mut BackupMeta,
    key: &[u8],
) -> Result<tempfile::NamedTempFile, CreateError> {
    let manifest = read_json_entry::<Manifest, _>(archive, MANIFEST_FILE, 8 * 1024 * 1024)
        .map_err(CreateError::Failed)?;
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|_| CreateError::Failed(String::from("Backup manifest is invalid")))?;
    meta.signature = Some(sign_meta(meta, &manifest_bytes, key)?);
    let mut output = tempfile::NamedTempFile::new_in(directory)?;
    let mut writer = ZipWriter::new(output.as_file_mut());
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        if entry.name() != META_FILE {
            writer.raw_copy_file(entry)?;
        }
    }
    writer.start_file(META_FILE, SimpleFileOptions::default())?;
    serde_json::to_writer(&mut writer, meta)
        .map_err(|_| CreateError::Failed(String::from("Backup metadata is invalid")))?;
    writer.finish()?;
    output.as_file().sync_all()?;
    Ok(output)
}

#[cfg(test)]
#[path = "desktop_backups_tests.rs"]
mod tests;
