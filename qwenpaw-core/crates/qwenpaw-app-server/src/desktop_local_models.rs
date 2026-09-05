//! Managed local GGUF downloads and llama.cpp process lifecycle.

use std::collections::BTreeMap;
use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::net::TcpListener;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use std::time::Instant;

use axum::Json;
use axum::Router;
use axum::extract::Path as AxumPath;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::delete;
use axum::routing::get;
use flate2::read::GzDecoder;
use futures_util::StreamExt as _;
use reqwest::Client;
use reqwest::Url;
use reqwest::redirect::Policy;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use tar::Archive;
use tokio::io::AsyncWriteExt as _;
use tokio::process::Child;
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use zip::ZipArchive;

use super::AppServer;
use super::DesktopWorkspace;
use super::desktop_models;

const LLAMA_CPP_RELEASE_TAG: &str = "b8744";
const LLAMA_CPP_BASE_URL: &str = "https://download.qwenpaw.agentscope.io/files/models/llama_cpp";
const HUGGING_FACE_BASE_URL: &str = "https://huggingface.co";
const MODELSCOPE_BASE_URL: &str = "https://modelscope.cn";
const MIN_MACOS_MAJOR: u64 = 13;
const MIN_MACOS_MINOR: u64 = 3;
const DOWNLOAD_TIMEOUT_SECONDS: u64 = 30;
const DOWNLOAD_SHUTDOWN_TIMEOUT_SECONDS: u64 = 8;
const HEALTH_REQUEST_TIMEOUT_SECONDS: u64 = 2;
const HEALTH_POLL_MILLIS: u64 = 250;
const SERVER_START_TIMEOUT_SECONDS: u64 = 120;
const SERVER_STOP_TIMEOUT_SECONDS: u64 = 8;
const MAX_RUNTIME_ARCHIVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_RUNTIME_ENTRIES: usize = 1_024;
const MAX_RUNTIME_FILE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_RUNTIME_EXTRACTED_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_MODEL_CATALOG_BYTES: usize = 2 * 1024 * 1024;
const MAX_MODEL_FILES: usize = 256;
const MAX_MODEL_FILE_BYTES: u64 = 32 * 1024 * 1024 * 1024;
const MAX_MODEL_TOTAL_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const MAX_REPO_ID_BYTES: usize = 512;
const MAX_REMOTE_PATH_BYTES: usize = 1_024;

type ApiError = (StatusCode, Json<Value>);

/// Download origins used by the managed local-model runtime.
///
/// Defaults match the original `QwenPaw` implementation. Alternate origins are
/// accepted only for HTTPS services or loopback HTTP test fixtures.
#[derive(Clone, Debug)]
pub struct LocalModelDownloadSources {
    pub llama_cpp_base_url: String,
    pub llama_cpp_release_tag: String,
    pub hugging_face_base_url: String,
    pub modelscope_base_url: String,
    pub server_start_timeout: Duration,
}

impl Default for LocalModelDownloadSources {
    fn default() -> Self {
        Self {
            llama_cpp_base_url: String::from(LLAMA_CPP_BASE_URL),
            llama_cpp_release_tag: String::from(LLAMA_CPP_RELEASE_TAG),
            hugging_face_base_url: String::from(HUGGING_FACE_BASE_URL),
            modelscope_base_url: String::from(MODELSCOPE_BASE_URL),
            server_start_timeout: Duration::from_secs(SERVER_START_TIMEOUT_SECONDS),
        }
    }
}

pub(super) struct LocalModelsState {
    sources: LocalModelDownloadSources,
    runtime_download: Mutex<DownloadSlot>,
    model_download: Mutex<DownloadSlot>,
    server: Mutex<ServerSlot>,
    lifecycle: Mutex<()>,
}

impl LocalModelsState {
    pub(super) fn new(sources: LocalModelDownloadSources) -> anyhow::Result<Self> {
        validate_sources(&sources)?;
        Ok(Self {
            sources,
            runtime_download: Mutex::new(DownloadSlot::default()),
            model_download: Mutex::new(DownloadSlot::default()),
            server: Mutex::new(ServerSlot::default()),
            lifecycle: Mutex::new(()),
        })
    }

    pub(super) fn replace_sources(
        &mut self,
        sources: LocalModelDownloadSources,
    ) -> anyhow::Result<()> {
        validate_sources(&sources)?;
        self.sources = sources;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DownloadPhase {
    #[default]
    Idle,
    Pending,
    Downloading,
    Canceling,
    Completed,
    Failed,
    Cancelled,
}

impl DownloadPhase {
    fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Pending => "pending",
            Self::Downloading => "downloading",
            Self::Canceling => "canceling",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    fn is_active(self) -> bool {
        matches!(self, Self::Pending | Self::Downloading | Self::Canceling)
    }
}

#[derive(Clone, Debug)]
struct DownloadProgress {
    phase: DownloadPhase,
    model_name: Option<String>,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    speed_bytes_per_sec: f64,
    source: Option<String>,
    error: Option<String>,
    local_path: Option<String>,
}

impl Default for DownloadProgress {
    fn default() -> Self {
        Self {
            phase: DownloadPhase::Idle,
            model_name: None,
            downloaded_bytes: 0,
            total_bytes: None,
            speed_bytes_per_sec: 0.0,
            source: None,
            error: None,
            local_path: None,
        }
    }
}

impl DownloadProgress {
    fn response(&self) -> Value {
        json!({
            "status": self.phase.as_str(),
            "model_name": self.model_name,
            "downloaded_bytes": self.downloaded_bytes,
            "total_bytes": self.total_bytes,
            "speed_bytes_per_sec": self.speed_bytes_per_sec,
            "source": self.source,
            "error": self.error,
            "local_path": self.local_path
        })
    }
}

#[derive(Default)]
struct DownloadSlot {
    generation: u64,
    progress: DownloadProgress,
    cancellation: Option<CancellationToken>,
}

impl DownloadSlot {
    fn begin(
        &mut self,
        model_name: Option<String>,
        source: Option<String>,
    ) -> Result<(u64, CancellationToken), ApiError> {
        if self.progress.phase.is_active() {
            return Err(conflict("A download is already in progress"));
        }
        self.generation = self.generation.wrapping_add(1);
        let cancellation = CancellationToken::new();
        self.progress = DownloadProgress {
            phase: DownloadPhase::Pending,
            model_name,
            source,
            ..DownloadProgress::default()
        };
        self.cancellation = Some(cancellation.clone());
        Ok((self.generation, cancellation))
    }

    fn cancel(&mut self) {
        if self.progress.phase.is_active() {
            self.progress.phase = DownloadPhase::Canceling;
            if let Some(cancellation) = &self.cancellation {
                cancellation.cancel();
            }
        }
    }
}

#[derive(Default)]
struct ServerSlot {
    child: Option<Child>,
    port: Option<u16>,
    model_id: Option<String>,
    generation: u64,
    transitioning: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum DownloadSource {
    Huggingface,
    Modelscope,
    Auto,
}

impl DownloadSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Huggingface => "huggingface",
            Self::Modelscope => "modelscope",
            Self::Auto => "auto",
        }
    }
}

#[derive(Debug, Deserialize)]
struct StartModelDownloadRequest {
    model_name: String,
    #[serde(default = "default_download_source")]
    source: DownloadSource,
}

#[derive(Debug, Deserialize)]
struct StartServerRequest {
    model_id: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
struct LocalModelInfo {
    id: String,
    name: String,
    size_bytes: u64,
    downloaded: bool,
    source: DownloadSource,
}

#[derive(Clone, Debug)]
struct RemoteModelFile {
    path: String,
    size: Option<u64>,
    url: Url,
}

#[derive(Clone, Copy)]
enum DownloadKind {
    Runtime,
    Model,
}

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route(
            "/api/local-models/server",
            get(server_status).post(start_server).delete(stop_server),
        )
        .route("/api/local-models/server/update", get(server_update_status))
        .route(
            "/api/local-models/server/download",
            get(runtime_download_progress)
                .post(start_runtime_download)
                .delete(cancel_runtime_download),
        )
        .route("/api/local-models/models", get(list_models))
        .route(
            "/api/local-models/models/download",
            get(model_download_progress)
                .post(start_model_download)
                .delete(cancel_model_download),
        )
        .route("/api/local-models/models/{*model_id}", delete(delete_model))
}

fn default_download_source() -> DownloadSource {
    DownloadSource::Auto
}

async fn server_status(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let state = local_state(&server)?;
    let (installable, install_message) = runtime_installability();
    if !installable {
        return Ok(Json(json!({
            "available": false,
            "installable": false,
            "installed": false,
            "port": null,
            "model_name": null,
            "message": install_message
        })));
    }
    let executable = runtime_executable(&server)?;
    let installed = is_safe_regular_file(&executable);
    if !installed {
        return Ok(Json(json!({
            "available": false,
            "installable": true,
            "installed": false,
            "port": null,
            "model_name": null,
            "message": "llama.cpp is not installed"
        })));
    }

    let generation = {
        let slot = state.server.lock().await;
        slot.child.as_ref().map(|_| slot.generation)
    };
    if let Some(generation) = generation {
        let _ = reap_server_generation(&server, generation).await?;
    }
    let (running, transitioning, port, model_name) = {
        let slot = state.server.lock().await;
        (
            slot.child.is_some(),
            slot.transitioning,
            slot.port,
            slot.model_id.clone(),
        )
    };
    let ready = if running && !transitioning {
        if let Some(port) = port {
            false_if_error(check_health(port).await)
        } else {
            false
        }
    } else {
        false
    };
    let message = if running && transitioning {
        Some("llama.cpp server is starting")
    } else if running && !ready {
        Some("llama.cpp server is not responding")
    } else if !running {
        Some("llama.cpp server is not running, please start the server first")
    } else {
        None
    };
    Ok(Json(json!({
        "available": installed && ready,
        "installable": true,
        "installed": installed,
        "port": port,
        "model_name": model_name,
        "message": message
    })))
}

async fn server_update_status(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    if !runtime_installability().0 {
        return Ok(Json(json!({"has_update": false})));
    }
    let executable = runtime_executable(&server)?;
    if !is_safe_regular_file(&executable) {
        return Ok(Json(json!({"has_update": false})));
    }
    let target = release_number(&local_state(&server)?.sources.llama_cpp_release_tag)
        .ok_or_else(|| internal("Local runtime release is invalid"))?;
    let installed = installed_runtime_version(&executable).await;
    Ok(Json(json!({
        "has_update": installed.is_none_or(|version| target > version)
    })))
}

async fn runtime_download_progress(
    State(server): State<AppServer>,
) -> Result<Json<Value>, ApiError> {
    let progress = local_state(&server)?.runtime_download.lock().await;
    Ok(Json(progress.progress.response()))
}

async fn start_runtime_download(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let state = local_state(&server)?;
    let (installable, message) = runtime_installability();
    if !installable {
        return Err(conflict(&message.unwrap_or_else(|| {
            String::from("Current environment does not support llama.cpp")
        })));
    }
    let server_was_running = {
        let slot = state.server.lock().await;
        slot.child.is_some()
    };
    if server_was_running {
        stop_server_inner(&server, true).await?;
    }
    let source = runtime_download_url(&state.sources)?;
    let (generation, cancellation) = state
        .runtime_download
        .lock()
        .await
        .begin(Some(String::from("llama.cpp")), Some(source.to_string()))?;
    let task_server = server.clone();
    tokio::spawn(async move {
        run_runtime_download(task_server, generation, cancellation, source).await;
    });
    Ok(Json(json!({
        "status": "accepted",
        "message": "llama.cpp download started"
    })))
}

async fn cancel_runtime_download(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    local_state(&server)?.runtime_download.lock().await.cancel();
    Ok(Json(json!({
        "status": "ok",
        "message": "llama.cpp download cancellation requested"
    })))
}

async fn list_models(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let root = models_root(&server)?;
    let downloaded = tokio::task::spawn_blocking(move || scan_downloaded_models(&root))
        .await
        .map_err(|_| internal("Local models could not be inspected"))?
        .map_err(|_| internal("Local models could not be inspected"))?;
    let mut by_id = BTreeMap::<String, LocalModelInfo>::new();
    for mut model in recommended_models() {
        model.downloaded = downloaded.contains_key(&model.id);
        by_id.insert(model.id.clone(), model);
    }
    for (id, size_bytes) in downloaded {
        by_id.entry(id.clone()).or_insert(LocalModelInfo {
            id: id.clone(),
            name: id,
            size_bytes,
            downloaded: true,
            source: DownloadSource::Auto,
        });
    }
    Ok(Json(json!(by_id.into_values().collect::<Vec<_>>())))
}

async fn model_download_progress(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let progress = local_state(&server)?.model_download.lock().await;
    Ok(Json(progress.progress.response()))
}

async fn start_model_download(
    State(server): State<AppServer>,
    Json(body): Json<StartModelDownloadRequest>,
) -> Result<Json<Value>, ApiError> {
    let model_id = normalize_repo_id(&body.model_name)?;
    let state = local_state(&server)?;
    let (generation, cancellation) = state.model_download.lock().await.begin(
        Some(model_id.clone()),
        Some(body.source.as_str().to_owned()),
    )?;
    let task_server = server.clone();
    tokio::spawn(async move {
        run_model_download(task_server, generation, cancellation, model_id, body.source).await;
    });
    Ok(Json(json!({
        "status": "accepted",
        "message": format!("Local model download started: {}", body.model_name.trim())
    })))
}

async fn cancel_model_download(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    local_state(&server)?.model_download.lock().await.cancel();
    Ok(Json(json!({
        "status": "ok",
        "message": "Local model download cancellation requested"
    })))
}

async fn delete_model(
    State(server): State<AppServer>,
    AxumPath(model_id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    let model_id = normalize_repo_id(&model_id)?;
    let state = local_state(&server)?;
    let _lifecycle = state.lifecycle.lock().await;
    {
        let server_slot = state.server.lock().await;
        if server_slot.child.is_some() && server_slot.model_id.as_deref() == Some(&model_id) {
            return Err(conflict("Cannot delete a model while it is running"));
        }
    }
    if state.model_download.lock().await.progress.phase.is_active() {
        return Err(conflict("Cannot delete a model while a download is active"));
    }
    let root = models_root(&server)?;
    let target = model_path(&root, &model_id);
    tokio::task::spawn_blocking(move || remove_model_directory(&root, &target))
        .await
        .map_err(|_| internal("Local model could not be deleted"))?
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => not_found("Downloaded local model not found"),
            _ => bad_request("Downloaded local model could not be deleted"),
        })?;
    Ok(Json(json!({
        "status": "ok",
        "message": format!("Local model deleted: {model_id}")
    })))
}

async fn start_server(
    State(server): State<AppServer>,
    Json(body): Json<StartServerRequest>,
) -> Result<Json<Value>, ApiError> {
    let model_id = normalize_repo_id(&body.model_id)?;
    let (port, supports_multimodal) = start_server_inner(&server, &model_id).await?;
    Ok(Json(json!({
        "port": port,
        "model_info": {
            "id": model_id,
            "name": model_id,
            "supports_multimodal": supports_multimodal,
            "supports_image": supports_multimodal,
            "supports_video": false,
            "probe_source": "probed"
        }
    })))
}

async fn stop_server(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    stop_server_inner(&server, true).await?;
    Ok(Json(json!({
        "status": "ok",
        "message": "llama.cpp server stopped"
    })))
}

fn validate_sources(sources: &LocalModelDownloadSources) -> anyhow::Result<()> {
    for (name, value) in [
        ("llama.cpp", sources.llama_cpp_base_url.as_str()),
        ("Hugging Face", sources.hugging_face_base_url.as_str()),
        ("ModelScope", sources.modelscope_base_url.as_str()),
    ] {
        let url =
            Url::parse(value).map_err(|_| anyhow::anyhow!("{name} download origin is invalid"))?;
        anyhow::ensure!(
            url.query().is_none() && url.fragment().is_none(),
            "{name} download origin must not include a query or fragment"
        );
        anyhow::ensure!(
            url.scheme() == "https" || (url.scheme() == "http" && is_loopback_url(&url)),
            "{name} download origin must use HTTPS or loopback HTTP"
        );
    }
    anyhow::ensure!(
        release_number(&sources.llama_cpp_release_tag).is_some(),
        "llama.cpp release tag must be b followed by digits"
    );
    anyhow::ensure!(
        !sources.server_start_timeout.is_zero()
            && sources.server_start_timeout <= Duration::from_secs(300),
        "local server start timeout must be between 1 millisecond and 300 seconds"
    );
    Ok(())
}

fn is_loopback_url(url: &Url) -> bool {
    url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    })
}

fn local_state(server: &AppServer) -> Result<&LocalModelsState, ApiError> {
    server
        .inner
        .desktop_local_models
        .as_ref()
        .ok_or_else(|| internal("Local model runtime is unavailable"))
}

fn desktop_workspace(server: &AppServer) -> Result<&DesktopWorkspace, ApiError> {
    server
        .inner
        .desktop_workspace
        .as_ref()
        .ok_or_else(|| internal("Desktop Workspace is unavailable"))
}

fn local_root(server: &AppServer) -> Result<PathBuf, ApiError> {
    Ok(desktop_workspace(server)?.data_dir.join("local-models"))
}

fn runtime_root(server: &AppServer) -> Result<PathBuf, ApiError> {
    Ok(local_root(server)?.join("bin"))
}

fn runtime_executable(server: &AppServer) -> Result<PathBuf, ApiError> {
    Ok(runtime_root(server)?.join(if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    }))
}

fn models_root(server: &AppServer) -> Result<PathBuf, ApiError> {
    Ok(local_root(server)?.join("models"))
}

fn temporary_root(server: &AppServer) -> Result<PathBuf, ApiError> {
    Ok(local_root(server)?.join("tmp"))
}

fn logs_root(server: &AppServer) -> Result<PathBuf, ApiError> {
    Ok(local_root(server)?.join("logs"))
}

fn runtime_installability() -> (bool, Option<String>) {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    if !matches!(os, "macos" | "linux" | "windows") {
        return (false, Some(format!("Unsupported OS: {os}")));
    }
    if !matches!(arch, "aarch64" | "x86_64") {
        return (false, Some(format!("Unsupported architecture: {arch}")));
    }
    if os == "macos" && !supported_macos_version() {
        return (
            false,
            Some(String::from(
                "Unsupported macOS version (requires 13.3 or later)",
            )),
        );
    }
    (true, None)
}

fn supported_macos_version() -> bool {
    if !cfg!(target_os = "macos") {
        return true;
    }
    let output = std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output();
    let Ok(output) = output else {
        return false;
    };
    let version = String::from_utf8_lossy(&output.stdout);
    let mut parts = version.trim().split('.');
    let major = parts.next().and_then(|value| value.parse::<u64>().ok());
    let minor = parts.next().and_then(|value| value.parse::<u64>().ok());
    matches!(
        (major, minor),
        (Some(major), Some(minor))
            if major > MIN_MACOS_MAJOR
                || (major == MIN_MACOS_MAJOR && minor >= MIN_MACOS_MINOR)
    )
}

fn runtime_download_url(sources: &LocalModelDownloadSources) -> Result<Url, ApiError> {
    let filename = runtime_archive_name(&sources.llama_cpp_release_tag)?;
    append_path(
        &sources.llama_cpp_base_url,
        &[sources.llama_cpp_release_tag.as_str(), filename.as_str()],
    )
}

fn runtime_archive_name(tag: &str) -> Result<String, ApiError> {
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        _ => return Err(conflict("Current architecture does not support llama.cpp")),
    };
    let value = match std::env::consts::OS {
        "macos" => format!("llama-{tag}-bin-macos-{arch}.tar.gz"),
        "linux" => format!("llama-{tag}-bin-ubuntu-{arch}.tar.gz"),
        "windows" => windows_runtime_archive(tag, arch),
        _ => {
            return Err(conflict(
                "Current operating system does not support llama.cpp",
            ));
        }
    };
    Ok(value)
}

fn windows_runtime_archive(tag: &str, arch: &str) -> String {
    if arch == "x64"
        && let Some(cuda) = detected_cuda_release()
    {
        return format!("llama-{tag}-bin-win-cuda-{cuda}-{arch}.zip");
    }
    format!("llama-{tag}-bin-win-cpu-{arch}.zip")
}

fn detected_cuda_release() -> Option<&'static str> {
    if !cfg!(target_os = "windows") {
        return None;
    }
    let output = std::process::Command::new("nvidia-smi").output().ok()?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let marker = "CUDA Version:";
    let version = text.split(marker).nth(1)?.split_whitespace().next()?;
    let mut parts = version.split('.');
    match (
        parts.next()?.parse::<u64>().ok()?,
        parts
            .next()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0),
    ) {
        (12, minor) if minor >= 4 => Some("12.4"),
        (13, _) => Some("13.1"),
        _ => None,
    }
}

fn release_number(value: &str) -> Option<u64> {
    value.strip_prefix('b')?.parse().ok()
}

fn append_path(base: &str, segments: &[&str]) -> Result<Url, ApiError> {
    let mut url = Url::parse(base).map_err(|_| internal("Download origin is invalid"))?;
    {
        let mut path = url
            .path_segments_mut()
            .map_err(|()| internal("Download origin is invalid"))?;
        path.pop_if_empty();
        for segment in segments {
            path.push(segment);
        }
    }
    Ok(url)
}

fn is_safe_regular_file(path: &Path) -> bool {
    path.symlink_metadata()
        .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
}

fn normalize_repo_id(value: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > MAX_REPO_ID_BYTES
        || value.chars().any(char::is_control)
        || value.contains('\\')
        || value.starts_with('/')
        || value.ends_with('/')
    {
        return Err(bad_request("Local model repository ID is invalid"));
    }
    let segments = value.split('/').collect::<Vec<_>>();
    if segments.len() != 2
        || segments.iter().any(|segment| {
            segment.is_empty()
                || matches!(*segment, "." | "..")
                || segment.starts_with('.')
                || segment.bytes().any(|byte| {
                    !byte.is_ascii_alphanumeric() && !matches!(byte, b'-' | b'_' | b'.')
                })
        })
    {
        return Err(bad_request("Local model repository ID is invalid"));
    }
    Ok(value.to_owned())
}

fn model_path(root: &Path, model_id: &str) -> PathBuf {
    model_id
        .split('/')
        .fold(root.to_path_buf(), |path, segment| path.join(segment))
}

fn recommended_models() -> Vec<LocalModelInfo> {
    let memory_bytes = physical_memory_bytes().unwrap_or(16 * 1024 * 1024 * 1024);
    let gib = 1024 * 1024 * 1024;
    let entries = if memory_bytes < 4 * gib {
        Vec::new()
    } else if memory_bytes <= 8 * gib {
        vec![
            ("AgentScope/QwenPaw-Flash-2B-Q4_K_M", 1_560_460_768),
            ("AgentScope/QwenPaw-Flash-2B-Q8_0", 2_552_356_320),
        ]
    } else if memory_bytes <= 16 * gib {
        vec![
            ("AgentScope/QwenPaw-Flash-4B-Q4_K_M", 3_066_384_736),
            ("AgentScope/QwenPaw-Flash-4B-Q8_0", 5_157_833_056),
        ]
    } else {
        vec![
            ("AgentScope/QwenPaw-Flash-9B-Q4_K_M", 5_476_080_128),
            ("AgentScope/QwenPaw-Flash-9B-Q8_0", 10_590_617_600),
        ]
    };
    entries
        .into_iter()
        .map(|(id, size_bytes)| LocalModelInfo {
            id: id.to_owned(),
            name: id.split('/').next_back().unwrap_or(id).to_owned(),
            size_bytes,
            downloaded: false,
            source: DownloadSource::Modelscope,
        })
        .collect()
}

fn physical_memory_bytes() -> Option<u64> {
    if cfg!(target_os = "linux") {
        let meminfo = fs::read_to_string("/proc/meminfo").ok()?;
        let kib = meminfo
            .lines()
            .find_map(|line| line.strip_prefix("MemTotal:"))?
            .split_whitespace()
            .next()?
            .parse::<u64>()
            .ok()?;
        return kib.checked_mul(1024);
    }
    if cfg!(target_os = "macos") {
        let output = std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()?;
        return String::from_utf8_lossy(&output.stdout).trim().parse().ok();
    }
    if cfg!(target_os = "windows") {
        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory",
            ])
            .output()
            .ok()?;
        return String::from_utf8_lossy(&output.stdout).trim().parse().ok();
    }
    None
}

fn scan_downloaded_models(root: &Path) -> std::io::Result<BTreeMap<String, u64>> {
    let mut models = BTreeMap::new();
    let owners = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(models),
        Err(error) => return Err(error),
    };
    for owner in owners {
        let owner = owner?;
        let owner_meta = owner.path().symlink_metadata()?;
        if !owner_meta.is_dir() || owner_meta.file_type().is_symlink() {
            continue;
        }
        for repository in fs::read_dir(owner.path())? {
            let repository = repository?;
            let repository_meta = repository.path().symlink_metadata()?;
            if !repository_meta.is_dir() || repository_meta.file_type().is_symlink() {
                continue;
            }
            let (size, has_model) = model_directory_summary(&repository.path())?;
            if has_model {
                let id = format!(
                    "{}/{}",
                    owner.file_name().to_string_lossy(),
                    repository.file_name().to_string_lossy()
                );
                if normalize_repo_id(&id).is_ok() {
                    models.insert(id, size);
                }
            }
        }
    }
    Ok(models)
}

fn model_directory_summary(root: &Path) -> std::io::Result<(u64, bool)> {
    let mut pending = vec![root.to_path_buf()];
    let mut size = 0_u64;
    let mut has_model = false;
    let mut entries = 0_usize;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            entries = entries.saturating_add(1);
            if entries > MAX_MODEL_FILES * 4 {
                return Err(std::io::Error::other("local model contains too many files"));
            }
            let path = entry.path();
            let metadata = path.symlink_metadata()?;
            if metadata.file_type().is_symlink() {
                return Err(std::io::Error::other(
                    "local model contains a symbolic link",
                ));
            }
            if metadata.is_dir() {
                pending.push(path);
            } else if metadata.is_file() {
                size = size
                    .checked_add(metadata.len())
                    .ok_or_else(|| std::io::Error::other("local model is too large"))?;
                if path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("gguf"))
                    && !path.file_name().is_some_and(|name| {
                        name.to_string_lossy()
                            .to_ascii_lowercase()
                            .starts_with("mmproj")
                    })
                {
                    has_model = true;
                }
            }
        }
    }
    Ok((size, has_model))
}

fn remove_model_directory(root: &Path, target: &Path) -> std::io::Result<()> {
    if !target.exists() {
        return Err(std::io::Error::from(std::io::ErrorKind::NotFound));
    }
    let relative = target
        .strip_prefix(root)
        .map_err(|_| std::io::Error::other("local model path is invalid"))?;
    if relative.components().count() != 2 {
        return Err(std::io::Error::other("local model path is invalid"));
    }
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(segment) = component else {
            return Err(std::io::Error::other("local model path is invalid"));
        };
        current.push(segment);
        let metadata = current.symlink_metadata()?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(std::io::Error::other("local model path is unsafe"));
        }
    }
    fs::remove_dir_all(target)?;
    if let Some(owner) = target.parent()
        && owner.parent() == Some(root)
        && fs::read_dir(owner)?.next().is_none()
    {
        fs::remove_dir(owner)?;
    }
    Ok(())
}

fn false_if_error(result: Result<bool, ApiError>) -> bool {
    result.unwrap_or(false)
}

async fn run_runtime_download(
    server: AppServer,
    generation: u64,
    cancellation: CancellationToken,
    source: Url,
) {
    let result = runtime_download_inner(&server, generation, &cancellation, source).await;
    finish_download(
        &server,
        DownloadKind::Runtime,
        generation,
        &cancellation,
        result,
    )
    .await;
}

async fn runtime_download_inner(
    server: &AppServer,
    generation: u64,
    cancellation: &CancellationToken,
    source: Url,
) -> Result<String, String> {
    let temporary = temporary_root(server).map_err(api_detail)?;
    tokio::fs::create_dir_all(&temporary)
        .await
        .map_err(|error| format!("llama.cpp staging directory could not be created: {error}"))?;
    let staging = temporary.join(format!("runtime-{}", Uuid::now_v7()));
    tokio::fs::create_dir(&staging)
        .await
        .map_err(|error| format!("llama.cpp staging directory could not be created: {error}"))?;
    let archive_name = source
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| String::from("llama.cpp archive URL is invalid"))?;
    let archive_path = staging.join(archive_name);
    let result = async {
        download_file(
            server,
            DownloadKind::Runtime,
            generation,
            cancellation,
            source,
            &archive_path,
            MAX_RUNTIME_ARCHIVE_BYTES,
        )
        .await?;
        if cancellation.is_cancelled() {
            return Err(String::from("Download cancelled"));
        }
        let extract_root = staging.join("extracted");
        let archive = archive_path.clone();
        let extracted = extract_root.clone();
        tokio::task::spawn_blocking(move || extract_runtime_archive(&archive, &extracted))
            .await
            .map_err(|_| String::from("llama.cpp extraction task failed"))??;
        if cancellation.is_cancelled() {
            return Err(String::from("Download cancelled"));
        }
        let install_source = flattened_archive_root(&extract_root)?;
        let executable = install_source.join(if cfg!(windows) {
            "llama-server.exe"
        } else {
            "llama-server"
        });
        if !is_safe_regular_file(&executable) {
            return Err(String::from("llama.cpp archive is missing llama-server"));
        }
        make_runtime_executable(&executable)?;
        installed_runtime_version(&executable)
            .await
            .ok_or_else(|| String::from("Downloaded llama-server failed its version check"))?;
        if cancellation.is_cancelled() {
            return Err(String::from("Download cancelled"));
        }
        let destination = runtime_root(server).map_err(api_detail)?;
        atomic_replace_directory(&install_source, &destination)?;
        Ok(destination.to_string_lossy().into_owned())
    }
    .await;
    let _ = tokio::fs::remove_dir_all(&staging).await;
    result
}

async fn run_model_download(
    server: AppServer,
    generation: u64,
    cancellation: CancellationToken,
    model_id: String,
    source: DownloadSource,
) {
    let result = model_download_inner(&server, generation, &cancellation, &model_id, source).await;
    finish_download(
        &server,
        DownloadKind::Model,
        generation,
        &cancellation,
        result,
    )
    .await;
}

async fn model_download_inner(
    server: &AppServer,
    generation: u64,
    cancellation: &CancellationToken,
    model_id: &str,
    source: DownloadSource,
) -> Result<String, String> {
    let state = local_state(server).map_err(api_detail)?;
    let files = remote_model_files(&state.sources, model_id, source).await?;
    validate_remote_model_files(&files)?;
    let total = files.iter().try_fold(0_u64, |sum, file| {
        file.size.and_then(|size| sum.checked_add(size))
    });
    set_download_total(server, DownloadKind::Model, generation, total).await;
    let temporary = temporary_root(server).map_err(api_detail)?;
    tokio::fs::create_dir_all(&temporary)
        .await
        .map_err(|error| format!("Model staging directory could not be created: {error}"))?;
    let staging = temporary.join(format!("model-{}", Uuid::now_v7()));
    tokio::fs::create_dir(&staging)
        .await
        .map_err(|error| format!("Model staging directory could not be created: {error}"))?;
    let result = async {
        for file in files {
            if cancellation.is_cancelled() {
                return Err(String::from("Download cancelled"));
            }
            let relative = safe_remote_path(&file.path)?;
            let destination = staging.join(relative);
            if let Some(parent) = destination.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|error| format!("Model directory could not be created: {error}"))?;
            }
            download_file(
                server,
                DownloadKind::Model,
                generation,
                cancellation,
                file.url,
                &destination,
                MAX_MODEL_FILE_BYTES,
            )
            .await?;
        }
        if cancellation.is_cancelled() {
            return Err(String::from("Download cancelled"));
        }
        let (_, has_model) = model_directory_summary(&staging)
            .map_err(|error| format!("Downloaded model is invalid: {error}"))?;
        if !has_model {
            return Err(String::from(
                "Repository does not contain a GGUF model file",
            ));
        }
        if cancellation.is_cancelled() {
            return Err(String::from("Download cancelled"));
        }
        let root = models_root(server).map_err(api_detail)?;
        let destination = model_path(&root, model_id);
        atomic_replace_directory(&staging, &destination)?;
        Ok(destination.to_string_lossy().into_owned())
    }
    .await;
    let _ = tokio::fs::remove_dir_all(&staging).await;
    result
}

async fn download_file(
    server: &AppServer,
    kind: DownloadKind,
    generation: u64,
    cancellation: &CancellationToken,
    url: Url,
    destination: &Path,
    maximum: u64,
) -> Result<(), String> {
    let client = download_client()?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("Download request failed: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Download request failed: {error}"))?;
    let content_length = response.content_length();
    if content_length.is_some_and(|size| size > maximum) {
        return Err(String::from("Download exceeds the allowed size"));
    }
    if matches!(kind, DownloadKind::Runtime) {
        set_download_total(server, kind, generation, content_length).await;
    }
    let mut output = tokio::fs::File::create(destination)
        .await
        .map_err(|error| format!("Download file could not be created: {error}"))?;
    let mut stream = response.bytes_stream();
    let started = Instant::now();
    let initial = current_downloaded(server, kind, generation).await;
    let mut written = 0_u64;
    while let Some(chunk) = tokio::select! {
        () = cancellation.cancelled() => return Err(String::from("Download cancelled")),
        chunk = stream.next() => chunk,
    } {
        let chunk = chunk.map_err(|error| format!("Download stream failed: {error}"))?;
        written = written
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| String::from("Download size overflow"))?;
        if written > maximum {
            return Err(String::from("Download exceeds the allowed size"));
        }
        output
            .write_all(&chunk)
            .await
            .map_err(|error| format!("Download file could not be written: {error}"))?;
        let downloaded = initial.saturating_add(written);
        if matches!(kind, DownloadKind::Model) && downloaded > MAX_MODEL_TOTAL_BYTES {
            return Err(String::from("Model repository exceeds the allowed size"));
        }
        let speed = bytes_per_second(written, started.elapsed());
        update_download_bytes(server, kind, generation, downloaded, speed).await;
    }
    output
        .flush()
        .await
        .map_err(|error| format!("Download file could not be flushed: {error}"))?;
    Ok(())
}

fn download_client() -> Result<Client, String> {
    Client::builder()
        .redirect(Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                return attempt.error("too many download redirects");
            }
            let target = attempt.url();
            if target.scheme() == "https" || (target.scheme() == "http" && is_loopback_url(target))
            {
                attempt.follow()
            } else {
                attempt.error("download redirect must use HTTPS or loopback HTTP")
            }
        }))
        .connect_timeout(Duration::from_secs(DOWNLOAD_TIMEOUT_SECONDS))
        .build()
        .map_err(|error| format!("Download client could not be created: {error}"))
}

async fn current_downloaded(server: &AppServer, kind: DownloadKind, generation: u64) -> u64 {
    let Ok(state) = local_state(server) else {
        return 0;
    };
    let slot = match kind {
        DownloadKind::Runtime => state.runtime_download.lock().await,
        DownloadKind::Model => state.model_download.lock().await,
    };
    if slot.generation == generation {
        slot.progress.downloaded_bytes
    } else {
        0
    }
}

async fn update_download_bytes(
    server: &AppServer,
    kind: DownloadKind,
    generation: u64,
    downloaded: u64,
    speed: f64,
) {
    let Ok(state) = local_state(server) else {
        return;
    };
    let mut slot = match kind {
        DownloadKind::Runtime => state.runtime_download.lock().await,
        DownloadKind::Model => state.model_download.lock().await,
    };
    if slot.generation == generation {
        slot.progress.phase = DownloadPhase::Downloading;
        slot.progress.downloaded_bytes = downloaded;
        slot.progress.speed_bytes_per_sec = speed;
    }
}

async fn set_download_total(
    server: &AppServer,
    kind: DownloadKind,
    generation: u64,
    total: Option<u64>,
) {
    let Ok(state) = local_state(server) else {
        return;
    };
    let mut slot = match kind {
        DownloadKind::Runtime => state.runtime_download.lock().await,
        DownloadKind::Model => state.model_download.lock().await,
    };
    if slot.generation == generation {
        slot.progress.total_bytes = total;
    }
}

async fn finish_download(
    server: &AppServer,
    kind: DownloadKind,
    generation: u64,
    cancellation: &CancellationToken,
    result: Result<String, String>,
) {
    let Ok(state) = local_state(server) else {
        return;
    };
    let mut slot = match kind {
        DownloadKind::Runtime => state.runtime_download.lock().await,
        DownloadKind::Model => state.model_download.lock().await,
    };
    if slot.generation != generation {
        return;
    }
    slot.progress.speed_bytes_per_sec = 0.0;
    if cancellation.is_cancelled() {
        slot.progress.phase = DownloadPhase::Cancelled;
        slot.progress.error = None;
    } else {
        match result {
            Ok(path) => {
                slot.progress.phase = DownloadPhase::Completed;
                slot.progress.local_path = Some(path);
                slot.progress.error = None;
            }
            Err(error) => {
                slot.progress.phase = DownloadPhase::Failed;
                slot.progress.error = Some(error);
            }
        }
    }
    slot.cancellation = None;
}

fn extract_runtime_archive(archive_path: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination)
        .map_err(|error| format!("llama.cpp extraction directory could not be created: {error}"))?;
    if archive_path
        .extension()
        .is_some_and(|extension| extension == "zip")
    {
        extract_runtime_zip(archive_path, destination)
    } else {
        extract_runtime_tar_gz(archive_path, destination)
    }
}

fn extract_runtime_zip(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let file = File::open(archive_path)
        .map_err(|error| format!("llama.cpp archive could not be opened: {error}"))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| format!("llama.cpp ZIP archive is invalid: {error}"))?;
    if archive.len() > MAX_RUNTIME_ENTRIES {
        return Err(String::from("llama.cpp archive contains too many entries"));
    }
    let mut total = 0_u64;
    let mut seen = HashSet::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("llama.cpp ZIP entry is invalid: {error}"))?;
        if entry.size() > MAX_RUNTIME_FILE_BYTES {
            return Err(String::from("llama.cpp archive entry is too large"));
        }
        total = total
            .checked_add(entry.size())
            .ok_or_else(|| String::from("llama.cpp extracted size overflow"))?;
        if total > MAX_RUNTIME_EXTRACTED_BYTES {
            return Err(String::from(
                "llama.cpp archive expands beyond the size limit",
            ));
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170_000 == 0o120_000)
        {
            return Err(String::from(
                "llama.cpp archive cannot contain symbolic links",
            ));
        }
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| String::from("llama.cpp archive contains an unsafe path"))?
            .clone();
        validate_archive_path(&relative)?;
        if !seen.insert(relative.clone()) {
            return Err(String::from("llama.cpp archive contains duplicate paths"));
        }
        let target = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&target)
                .map_err(|error| format!("llama.cpp directory could not be created: {error}"))?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    format!("llama.cpp directory could not be created: {error}")
                })?;
            }
            let mut output = File::create(&target)
                .map_err(|error| format!("llama.cpp file could not be created: {error}"))?;
            std::io::copy(&mut entry, &mut output)
                .map_err(|error| format!("llama.cpp file could not be extracted: {error}"))?;
        }
    }
    Ok(())
}

fn extract_runtime_tar_gz(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let file = File::open(archive_path)
        .map_err(|error| format!("llama.cpp archive could not be opened: {error}"))?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    let entries = archive
        .entries()
        .map_err(|error| format!("llama.cpp tar archive is invalid: {error}"))?;
    let mut count = 0_usize;
    let mut total = 0_u64;
    let mut seen = HashSet::new();
    for entry in entries {
        let mut entry =
            entry.map_err(|error| format!("llama.cpp tar entry is invalid: {error}"))?;
        count = count.saturating_add(1);
        if count > MAX_RUNTIME_ENTRIES {
            return Err(String::from("llama.cpp archive contains too many entries"));
        }
        let kind = entry.header().entry_type();
        if !kind.is_file() && !kind.is_dir() {
            return Err(String::from(
                "llama.cpp archive contains an unsupported entry",
            ));
        }
        let size = entry.size();
        if size > MAX_RUNTIME_FILE_BYTES {
            return Err(String::from("llama.cpp archive entry is too large"));
        }
        total = total
            .checked_add(size)
            .ok_or_else(|| String::from("llama.cpp extracted size overflow"))?;
        if total > MAX_RUNTIME_EXTRACTED_BYTES {
            return Err(String::from(
                "llama.cpp archive expands beyond the size limit",
            ));
        }
        let relative = entry
            .path()
            .map_err(|_| String::from("llama.cpp archive path is invalid"))?
            .to_path_buf();
        validate_archive_path(&relative)?;
        if !seen.insert(relative.clone()) {
            return Err(String::from("llama.cpp archive contains duplicate paths"));
        }
        let target = destination.join(relative);
        if kind.is_dir() {
            fs::create_dir_all(&target)
                .map_err(|error| format!("llama.cpp directory could not be created: {error}"))?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    format!("llama.cpp directory could not be created: {error}")
                })?;
            }
            entry
                .unpack(&target)
                .map_err(|error| format!("llama.cpp file could not be extracted: {error}"))?;
        }
    }
    Ok(())
}

fn validate_archive_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().len() > MAX_REMOTE_PATH_BYTES
        || path.components().next().is_none()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(String::from("llama.cpp archive contains an unsafe path"));
    }
    Ok(())
}

fn flattened_archive_root(extracted: &Path) -> Result<PathBuf, String> {
    let entries = fs::read_dir(extracted)
        .map_err(|error| format!("llama.cpp extraction could not be inspected: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("llama.cpp extraction could not be inspected: {error}"))?;
    if entries.len() == 1 {
        let only = &entries[0];
        let metadata = only
            .path()
            .symlink_metadata()
            .map_err(|error| format!("llama.cpp extraction could not be inspected: {error}"))?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            return Ok(only.path());
        }
    }
    Ok(extracted.to_path_buf())
}

#[cfg(unix)]
fn make_runtime_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;
    let metadata = path
        .metadata()
        .map_err(|error| format!("llama-server permissions are unavailable: {error}"))?;
    let mut permissions = metadata.permissions();
    permissions.set_mode(permissions.mode() | 0o700);
    fs::set_permissions(path, permissions)
        .map_err(|error| format!("llama-server could not be made executable: {error}"))
}

#[cfg(not(unix))]
fn make_runtime_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn atomic_replace_directory(source: &Path, destination: &Path) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| String::from("Installation path is invalid"))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Installation directory could not be created: {error}"))?;
    let backup = parent.join(format!(".backup-{}", Uuid::now_v7()));
    let had_destination = destination.exists();
    if had_destination {
        fs::rename(destination, &backup)
            .map_err(|error| format!("Existing installation could not be staged: {error}"))?;
    }
    if let Err(error) = fs::rename(source, destination) {
        if had_destination {
            let _ = fs::rename(&backup, destination);
        }
        return Err(format!("New installation could not be published: {error}"));
    }
    if had_destination {
        let _ = fs::remove_dir_all(backup);
    }
    Ok(())
}

fn safe_remote_path(value: &str) -> Result<PathBuf, String> {
    if value.is_empty() || value.len() > MAX_REMOTE_PATH_BYTES || value.contains('\\') {
        return Err(String::from("Model repository contains an unsafe path"));
    }
    let path = PathBuf::from(value);
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(String::from("Model repository contains an unsafe path"));
    }
    Ok(path)
}

fn validate_remote_model_files(files: &[RemoteModelFile]) -> Result<(), String> {
    if files.is_empty() || files.len() > MAX_MODEL_FILES {
        return Err(String::from(
            "Model repository has no GGUF files or too many files",
        ));
    }
    let mut paths = HashSet::new();
    let mut total = 0_u64;
    let mut has_model = false;
    for file in files {
        let path = safe_remote_path(&file.path)?;
        if !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("gguf"))
        {
            return Err(String::from("Only GGUF model files may be downloaded"));
        }
        if !paths.insert(path.clone()) {
            return Err(String::from("Model repository contains duplicate paths"));
        }
        if let Some(size) = file.size {
            if size > MAX_MODEL_FILE_BYTES {
                return Err(String::from("Model file exceeds the allowed size"));
            }
            total = total
                .checked_add(size)
                .ok_or_else(|| String::from("Model repository size overflow"))?;
            if total > MAX_MODEL_TOTAL_BYTES {
                return Err(String::from("Model repository exceeds the allowed size"));
            }
        }
        if !path.file_name().is_some_and(|name| {
            name.to_string_lossy()
                .to_ascii_lowercase()
                .starts_with("mmproj")
        }) {
            has_model = true;
        }
    }
    if !has_model {
        return Err(String::from("Repository is missing a primary GGUF model"));
    }
    Ok(())
}

async fn remote_model_files(
    sources: &LocalModelDownloadSources,
    model_id: &str,
    source: DownloadSource,
) -> Result<Vec<RemoteModelFile>, String> {
    match source {
        DownloadSource::Huggingface => hugging_face_files(sources, model_id).await,
        DownloadSource::Modelscope => modelscope_files(sources, model_id).await,
        DownloadSource::Auto => match modelscope_files(sources, model_id).await {
            Ok(files) => Ok(files),
            Err(modelscope_error) => hugging_face_files(sources, model_id)
                .await
                .map_err(|hugging_face_error| {
                    format!(
                        "ModelScope failed: {modelscope_error}; Hugging Face failed: {hugging_face_error}"
                    )
                }),
        },
    }
}

async fn hugging_face_files(
    sources: &LocalModelDownloadSources,
    model_id: &str,
) -> Result<Vec<RemoteModelFile>, String> {
    let mut segments = model_id.split('/');
    let owner = segments.next().unwrap_or_default();
    let repository = segments.next().unwrap_or_default();
    let url = append_path(
        &sources.hugging_face_base_url,
        &["api", "models", owner, repository],
    )
    .map_err(api_detail)?;
    let value = fetch_catalog(url).await?;
    let siblings = value
        .get("siblings")
        .and_then(Value::as_array)
        .ok_or_else(|| String::from("Hugging Face model catalog is invalid"))?;
    let mut files = Vec::new();
    for sibling in siblings {
        let Some(path) = sibling.get("rfilename").and_then(Value::as_str) else {
            continue;
        };
        if !path.to_ascii_lowercase().ends_with(".gguf") {
            continue;
        }
        let size = sibling
            .get("size")
            .and_then(Value::as_u64)
            .or_else(|| sibling.pointer("/lfs/size").and_then(Value::as_u64));
        let mut download = append_path(
            &sources.hugging_face_base_url,
            &[owner, repository, "resolve", "main"],
        )
        .map_err(api_detail)?;
        {
            let mut target = download
                .path_segments_mut()
                .map_err(|()| String::from("Hugging Face download URL is invalid"))?;
            for segment in path.split('/') {
                target.push(segment);
            }
        }
        download.query_pairs_mut().append_pair("download", "true");
        files.push(RemoteModelFile {
            path: path.to_owned(),
            size,
            url: download,
        });
    }
    Ok(files)
}

async fn modelscope_files(
    sources: &LocalModelDownloadSources,
    model_id: &str,
) -> Result<Vec<RemoteModelFile>, String> {
    let mut segments = model_id.split('/');
    let owner = segments.next().unwrap_or_default();
    let repository = segments.next().unwrap_or_default();
    let mut catalog = append_path(
        &sources.modelscope_base_url,
        &["api", "v1", "models", owner, repository, "repo", "files"],
    )
    .map_err(api_detail)?;
    catalog
        .query_pairs_mut()
        .append_pair("Revision", "master")
        .append_pair("Recursive", "True");
    let value = fetch_catalog(catalog).await?;
    let list = value
        .as_array()
        .or_else(|| value.pointer("/Data/Files").and_then(Value::as_array))
        .or_else(|| value.get("Files").and_then(Value::as_array))
        .ok_or_else(|| String::from("ModelScope model catalog is invalid"))?;
    let mut files = Vec::new();
    for entry in list {
        let path = ["Path", "Name", "path", "name"]
            .into_iter()
            .find_map(|key| entry.get(key).and_then(Value::as_str));
        let Some(path) = path else {
            continue;
        };
        if !path.to_ascii_lowercase().ends_with(".gguf") {
            continue;
        }
        let size = ["Size", "size"]
            .into_iter()
            .find_map(|key| entry.get(key).and_then(Value::as_u64));
        let mut download = append_path(
            &sources.modelscope_base_url,
            &["api", "v1", "models", owner, repository, "repo"],
        )
        .map_err(api_detail)?;
        download
            .query_pairs_mut()
            .append_pair("Revision", "master")
            .append_pair("FilePath", path);
        files.push(RemoteModelFile {
            path: path.to_owned(),
            size,
            url: download,
        });
    }
    Ok(files)
}

async fn fetch_catalog(url: Url) -> Result<Value, String> {
    let response = download_client()?
        .get(url)
        .send()
        .await
        .map_err(|error| format!("Model catalog request failed: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Model catalog request failed: {error}"))?;
    if response
        .content_length()
        .is_some_and(|size| size > MAX_MODEL_CATALOG_BYTES as u64)
    {
        return Err(String::from("Model catalog exceeds the allowed size"));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("Model catalog could not be read: {error}"))?;
    if bytes.len() > MAX_MODEL_CATALOG_BYTES {
        return Err(String::from("Model catalog exceeds the allowed size"));
    }
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("Model catalog is invalid JSON: {error}"))
}

async fn installed_runtime_version(executable: &Path) -> Option<u64> {
    if !is_safe_regular_file(executable) {
        return None;
    }
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        Command::new(executable).arg("--version").output(),
    )
    .await
    .ok()?
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    text.split(|character: char| !character.is_ascii_digit())
        .filter_map(|value| value.parse::<u64>().ok())
        .find(|value| *value >= 1_000)
}

async fn start_server_inner(server: &AppServer, model_id: &str) -> Result<(u16, bool), ApiError> {
    start_server_with_activation(server, model_id, true).await
}

#[allow(clippy::too_many_lines)]
async fn start_server_with_activation(
    server: &AppServer,
    model_id: &str,
    make_active: bool,
) -> Result<(u16, bool), ApiError> {
    let state = local_state(server)?;
    let _lifecycle = state.lifecycle.lock().await;
    let executable = runtime_executable(server)?;
    if !is_safe_regular_file(&executable) {
        return Err(conflict("llama.cpp is not installed"));
    }
    let model_directory = model_path(&models_root(server)?, model_id);
    let (model, mmproj) =
        tokio::task::spawn_blocking(move || resolve_model_files(&model_directory))
            .await
            .map_err(|_| internal("Local model files could not be inspected"))?
            .map_err(|error| bad_request(&error))?;
    if let Some(mut previous) = take_server_child(state).await {
        terminate_child(&mut previous).await;
        desktop_models::clear_local_runtime(server).await?;
    }
    let (config, _) = desktop_models::local_model_config(server).await?;
    let port = reserve_port(config.port)?;
    let log_directory = logs_root(server)?;
    fs::create_dir_all(&log_directory)
        .map_err(|_| internal("Local model log directory could not be created"))?;
    let log_path = log_directory.join("llama-server.log");
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|_| internal("Local model log file could not be opened"))?;
    let stderr = stdout
        .try_clone()
        .map_err(|_| internal("Local model log file could not be opened"))?;
    let mut command = Command::new(&executable);
    command
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .arg("--model")
        .arg(&model)
        .arg("--alias")
        .arg(model_id)
        .arg("--log-file")
        .arg(&log_path)
        .arg("--gpu-layers")
        .arg("auto")
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .kill_on_drop(true);
    if config.max_context_length > 0 {
        command
            .arg("--ctx-size")
            .arg(config.max_context_length.to_string());
    }
    if let Some(mmproj) = &mmproj {
        command.arg("--mmproj").arg(mmproj);
    }
    let child = command
        .spawn()
        .map_err(|_| internal("llama.cpp server could not be started"))?;
    let generation = {
        let mut slot = state.server.lock().await;
        slot.generation = slot.generation.wrapping_add(1);
        slot.child = Some(child);
        slot.port = Some(port);
        slot.model_id = Some(model_id.to_owned());
        slot.transitioning = true;
        slot.generation
    };
    let deadline = Instant::now() + state.sources.server_start_timeout;
    loop {
        let exited = {
            let mut slot = state.server.lock().await;
            slot.child
                .as_mut()
                .and_then(|child| child.try_wait().ok().flatten())
                .is_some()
        };
        if exited {
            clear_server_slot(state, generation).await;
            return Err(internal("llama.cpp server exited before becoming ready"));
        }
        if check_health(port).await.unwrap_or(false) {
            break;
        }
        if Instant::now() >= deadline {
            if let Some(mut child) = take_server_child(state).await {
                terminate_child(&mut child).await;
            }
            return Err(internal("llama.cpp server did not become ready in time"));
        }
        tokio::time::sleep(Duration::from_millis(HEALTH_POLL_MILLIS)).await;
    }
    let supports_multimodal = mmproj.is_some();
    if let Err(error) = desktop_models::register_local_runtime(
        server,
        model_id,
        port,
        supports_multimodal,
        make_active,
    )
    .await
    {
        if let Some(mut child) = take_server_child(state).await {
            terminate_child(&mut child).await;
        }
        return Err(error);
    }
    {
        let mut slot = state.server.lock().await;
        if slot.generation == generation {
            slot.transitioning = false;
        }
    }
    spawn_server_monitor(server.clone(), generation);
    Ok((port, supports_multimodal))
}

async fn stop_server_inner(server: &AppServer, clear_provider: bool) -> Result<(), ApiError> {
    let state = local_state(server)?;
    let _lifecycle = state.lifecycle.lock().await;
    if let Some(mut child) = take_server_child(state).await {
        terminate_child(&mut child).await;
    }
    if clear_provider {
        desktop_models::clear_local_runtime(server).await?;
    }
    Ok(())
}

async fn take_server_child(state: &LocalModelsState) -> Option<Child> {
    let mut slot = state.server.lock().await;
    slot.generation = slot.generation.wrapping_add(1);
    slot.port = None;
    slot.model_id = None;
    slot.transitioning = false;
    slot.child.take()
}

async fn clear_server_slot(state: &LocalModelsState, generation: u64) {
    let mut slot = state.server.lock().await;
    if slot.generation == generation {
        slot.child = None;
        slot.port = None;
        slot.model_id = None;
        slot.transitioning = false;
    }
}

async fn terminate_child(child: &mut Child) {
    if child.try_wait().ok().flatten().is_some() {
        return;
    }
    let _ = child.start_kill();
    let _ = tokio::time::timeout(
        Duration::from_secs(SERVER_STOP_TIMEOUT_SECONDS),
        child.wait(),
    )
    .await;
}

fn spawn_server_monitor(server: AppServer, generation: u64) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                () = server.inner.shutdown.cancelled() => return,
                () = tokio::time::sleep(Duration::from_secs(1)) => {}
            }
            match reap_server_generation(&server, generation).await {
                Ok(Some(false)) => {}
                Ok(Some(true) | None) | Err(_) => return,
            }
        }
    });
}

async fn reap_server_generation(
    server: &AppServer,
    generation: u64,
) -> Result<Option<bool>, ApiError> {
    let state = local_state(server)?;
    let exited = {
        let mut slot = state.server.lock().await;
        if slot.generation != generation {
            return Ok(None);
        }
        slot.child
            .as_mut()
            .map(tokio::process::Child::try_wait)
            .transpose()
            .map_err(|_| internal("Local server status is unavailable"))?
            .flatten()
            .is_some()
    };
    if !exited {
        return Ok(Some(false));
    }

    let _lifecycle = state.lifecycle.lock().await;
    let reaped = {
        let mut slot = state.server.lock().await;
        if slot.generation != generation {
            return Ok(None);
        }
        let exited = slot
            .child
            .as_mut()
            .map(tokio::process::Child::try_wait)
            .transpose()
            .map_err(|_| internal("Local server status is unavailable"))?
            .flatten()
            .is_some();
        if exited {
            slot.child = None;
            slot.port = None;
            slot.model_id = None;
            slot.transitioning = false;
        }
        exited
    };
    if reaped {
        desktop_models::clear_local_runtime(server).await?;
    }
    Ok(Some(reaped))
}

fn resolve_model_files(root: &Path) -> Result<(PathBuf, Option<PathBuf>), String> {
    let metadata = root
        .symlink_metadata()
        .map_err(|_| String::from("Downloaded local model not found"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(String::from("Downloaded local model path is unsafe"));
    }
    let mut pending = vec![root.to_path_buf()];
    let mut models = Vec::new();
    let mut projections = Vec::new();
    let mut count = 0_usize;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)
            .map_err(|error| format!("Downloaded local model could not be read: {error}"))?
        {
            let entry = entry
                .map_err(|error| format!("Downloaded local model could not be read: {error}"))?;
            count = count.saturating_add(1);
            if count > MAX_MODEL_FILES * 4 {
                return Err(String::from(
                    "Downloaded local model contains too many files",
                ));
            }
            let path = entry.path();
            let metadata = path
                .symlink_metadata()
                .map_err(|_| String::from("Downloaded local model path is unsafe"))?;
            if metadata.file_type().is_symlink() {
                return Err(String::from(
                    "Downloaded local model contains a symbolic link",
                ));
            }
            if metadata.is_dir() {
                pending.push(path);
            } else if metadata.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("gguf"))
            {
                if path.file_name().is_some_and(|name| {
                    name.to_string_lossy()
                        .to_ascii_lowercase()
                        .starts_with("mmproj")
                }) {
                    projections.push(path);
                } else {
                    models.push(path);
                }
            }
        }
    }
    models.sort();
    projections.sort();
    let model = models
        .into_iter()
        .next()
        .ok_or_else(|| String::from("Downloaded repository is missing a GGUF model"))?;
    Ok((model, projections.into_iter().next()))
}

fn reserve_port(configured: Option<u16>) -> Result<u16, ApiError> {
    let address = (std::net::Ipv4Addr::LOCALHOST, configured.unwrap_or(0));
    let listener = TcpListener::bind(address)
        .map_err(|_| conflict("Configured local model server port is unavailable"))?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|_| internal("Local model server port could not be reserved"))
}

async fn check_health(port: u16) -> Result<bool, ApiError> {
    let client = Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(HEALTH_REQUEST_TIMEOUT_SECONDS))
        .build()
        .map_err(|_| internal("Local model health client could not be created"))?;
    let url = format!("http://127.0.0.1:{port}/health");
    match client.get(url).send().await {
        Ok(response) => Ok(response.status().as_u16() < 500),
        Err(_) => Ok(false),
    }
}

pub(super) async fn resume(server: &AppServer) {
    let Ok(Some((model_id, was_active))) =
        desktop_models::persisted_local_runtime_model(server).await
    else {
        return;
    };
    let runtime = runtime_executable(server).is_ok_and(|path| is_safe_regular_file(&path));
    let model = models_root(server)
        .ok()
        .map(|root| model_path(&root, &model_id))
        .is_some_and(|path| resolve_model_files(&path).is_ok());
    if !runtime || !model {
        let _ = desktop_models::clear_local_runtime(server).await;
        return;
    }
    if start_server_with_activation(server, &model_id, was_active)
        .await
        .is_err()
    {
        let _ = desktop_models::clear_local_runtime(server).await;
    }
}

pub(super) async fn shutdown(server: &AppServer) {
    let Ok(state) = local_state(server) else {
        return;
    };
    state.runtime_download.lock().await.cancel();
    state.model_download.lock().await.cancel();
    let deadline = Instant::now() + Duration::from_secs(DOWNLOAD_SHUTDOWN_TIMEOUT_SECONDS);
    loop {
        let runtime_active = state
            .runtime_download
            .lock()
            .await
            .progress
            .phase
            .is_active();
        let model_active = state.model_download.lock().await.progress.phase.is_active();
        if (!runtime_active && !model_active) || Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let _ = stop_server_inner(server, false).await;
}

fn api_detail((_, Json(value)): ApiError) -> String {
    value
        .get("detail")
        .and_then(Value::as_str)
        .unwrap_or("Local model operation failed")
        .to_owned()
}

#[allow(clippy::cast_precision_loss)]
fn bytes_per_second(bytes: u64, elapsed: Duration) -> f64 {
    bytes as f64 / elapsed.as_secs_f64().max(0.001)
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

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn validates_sources_repository_ids_and_remote_paths() {
        let mut sources = LocalModelDownloadSources::default();
        assert!(validate_sources(&sources).is_ok());
        sources.hugging_face_base_url = String::from("http://example.com");
        assert_eq!(
            validate_sources(&sources).unwrap_err().to_string(),
            "Hugging Face download origin must use HTTPS or loopback HTTP"
        );
        sources.hugging_face_base_url = String::from("http://127.0.0.1:8080");
        assert!(validate_sources(&sources).is_ok());

        assert_eq!(
            normalize_repo_id("AgentScope/QwenPaw-Flash-4B-Q4_K_M").unwrap(),
            "AgentScope/QwenPaw-Flash-4B-Q4_K_M"
        );
        assert!(normalize_repo_id("../model").is_err());
        assert!(normalize_repo_id("owner/model/extra").is_err());
        assert_eq!(
            safe_remote_path("weights/model.gguf").unwrap(),
            PathBuf::from("weights/model.gguf")
        );
        assert!(safe_remote_path("../model.gguf").is_err());
    }

    #[test]
    fn rejects_zip_traversal_and_symbolic_links() {
        let temporary = tempfile::tempdir().unwrap();
        let traversal = temporary.path().join("traversal.zip");
        {
            let file = File::create(&traversal).unwrap();
            let mut archive = zip::ZipWriter::new(file);
            archive
                .start_file::<_, ()>("../llama-server", zip::write::SimpleFileOptions::default())
                .unwrap();
            archive.write_all(b"invalid").unwrap();
            archive.finish().unwrap();
        }
        assert!(
            extract_runtime_zip(&traversal, &temporary.path().join("one"))
                .unwrap_err()
                .contains("unsafe path")
        );

        let symbolic = temporary.path().join("symbolic.zip");
        {
            let file = File::create(&symbolic).unwrap();
            let mut archive = zip::ZipWriter::new(file);
            archive
                .add_symlink::<_, _, ()>(
                    "llama-server",
                    "outside",
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            archive.finish().unwrap();
        }
        assert!(
            extract_runtime_zip(&symbolic, &temporary.path().join("two"))
                .unwrap_err()
                .contains("symbolic links")
        );
    }

    #[test]
    fn extracts_valid_runtime_and_resolves_primary_and_projection_models() {
        let temporary = tempfile::tempdir().unwrap();
        let archive_path = temporary.path().join("runtime.zip");
        {
            let file = File::create(&archive_path).unwrap();
            let mut archive = zip::ZipWriter::new(file);
            archive
                .start_file::<_, ()>(
                    "llama-b8744/llama-server",
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            archive.write_all(b"runtime").unwrap();
            archive.finish().unwrap();
        }
        let extracted = temporary.path().join("extracted");
        extract_runtime_zip(&archive_path, &extracted).unwrap();
        assert_eq!(
            flattened_archive_root(&extracted).unwrap(),
            extracted.join("llama-b8744")
        );

        let models = temporary.path().join("models");
        fs::create_dir_all(models.join("nested")).unwrap();
        fs::write(models.join("nested/model.gguf"), b"model").unwrap();
        fs::write(models.join("mmproj-model.gguf"), b"projection").unwrap();
        let (model, projection) = resolve_model_files(&models).unwrap();
        assert_eq!(model, models.join("nested/model.gguf"));
        assert_eq!(projection, Some(models.join("mmproj-model.gguf")));
    }

    #[test]
    fn validates_remote_gguf_catalog_as_a_whole() {
        let base = Url::parse("https://example.com/model.gguf").unwrap();
        let files = vec![
            RemoteModelFile {
                path: String::from("model.gguf"),
                size: Some(1024),
                url: base.clone(),
            },
            RemoteModelFile {
                path: String::from("mmproj-model.gguf"),
                size: Some(128),
                url: base,
            },
        ];
        assert!(validate_remote_model_files(&files).is_ok());
        let projection_only = vec![RemoteModelFile {
            path: String::from("mmproj-model.gguf"),
            size: Some(128),
            url: Url::parse("https://example.com/mmproj.gguf").unwrap(),
        }];
        assert_eq!(
            validate_remote_model_files(&projection_only).unwrap_err(),
            "Repository is missing a primary GGUF model"
        );
    }
}
