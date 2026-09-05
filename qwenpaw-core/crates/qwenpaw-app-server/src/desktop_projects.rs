//! Project creation and import endpoints for the unchanged Console.

use std::collections::HashSet;
use std::convert::Infallible;
use std::io::Cursor;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::process::Stdio;
use std::time::Duration;

use axum::Json;
use axum::Router;
use axum::extract::Multipart;
use axum::extract::Query;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::Sse;
use axum::response::sse::Event;
use axum::response::sse::KeepAlive;
use axum::routing::post;
use futures_util::stream;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use uuid::Uuid;
use zip::ZipArchive;

use super::AppServer;

const PROJECTS_DIRECTORY: &str = "coding_projects";
const MAX_PROJECT_NAME_BYTES: usize = 255;
const MAX_IMPORT_ENTRIES: usize = 100_000;
const MAX_UPLOAD_BYTES: usize = 32 * 1024 * 1024;
const MAX_ZIP_ENTRIES: usize = 10_000;
const MAX_ZIP_EXPANDED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_CLONE_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const CLONE_TIMEOUT: Duration = Duration::from_secs(300);
const GIT_TIMEOUT: Duration = Duration::from_secs(30);

const EXCLUDED_NAMES: [&str; 10] = [
    "node_modules",
    ".next",
    "dist",
    "build",
    "__pycache__",
    ".cache",
    ".venv",
    "venv",
    ".mypy_cache",
    ".tox",
];

const SENSITIVE_NAMES: [&str; 17] = [
    ".ssh",
    ".aws",
    ".gnupg",
    ".kube",
    ".docker",
    ".azure",
    ".claude",
    ".password-store",
    ".env",
    ".netrc",
    ".npmrc",
    ".yarnrc",
    ".pypirc",
    ".gitconfig",
    ".git-credentials",
    ".terraformrc",
    ".vault-token",
];

const SENSITIVE_SEQUENCES: [&[&str]; 11] = [
    &[".config", "gcloud"],
    &[".config", "nix"],
    &[".config", "gh"],
    &["library", "keychains"],
    &["library", "cookies"],
    &["library", "application support", "google", "chrome"],
    &["library", "application support", "firefox"],
    &["appdata", "roaming", "gcloud"],
    &["appdata", "roaming", "github cli"],
    &["appdata", "local", "google", "chrome", "user data"],
    &["appdata", "roaming", "mozilla", "firefox"],
];

type ApiError = (StatusCode, Json<Value>);

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route(
            "/api/workspace/project-directory/create",
            post(create_project),
        )
        .route(
            "/api/workspace/project-directory/import-local",
            post(import_local),
        )
        .route(
            "/api/workspace/project-directory/upload-zip",
            post(upload_zip),
        )
        .route(
            "/api/workspace/project-directory/clone",
            post(clone_project),
        )
}

#[derive(Debug, Deserialize)]
struct CreateProjectRequest {
    name: String,
}

#[derive(Debug, Deserialize)]
struct ImportLocalRequest {
    path: String,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UploadZipQuery {
    name: String,
}

#[derive(Debug, Deserialize)]
struct CloneProjectRequest {
    url: String,
    #[serde(default)]
    name: Option<String>,
}

async fn create_project(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Json(request): Json<CreateProjectRequest>,
) -> Result<Json<Value>, ApiError> {
    let agent_id = super::desktop_agents::requested_agent_id(&headers)?;
    super::desktop_agents::workspace_for_agent(&server, &agent_id).await?;
    let _guard = server.inner.desktop_project_lock.lock().await;
    let target = project_destination(&server, &request.name)?;
    tokio::fs::create_dir_all(&target)
        .await
        .map_err(|_| internal_error("Project directory could not be created"))?;
    run_git_init(&target).await?;
    activate_project(&server, &agent_id, &target).await?;
    Ok(project_response(&target))
}

async fn import_local(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Json(request): Json<ImportLocalRequest>,
) -> Result<Json<Value>, ApiError> {
    let agent_id = super::desktop_agents::requested_agent_id(&headers)?;
    super::desktop_agents::workspace_for_agent(&server, &agent_id).await?;
    let source = canonical_import_source(&request.path)?;
    validate_import_source(&source)?;
    let destination_name = request
        .name
        .as_deref()
        .unwrap_or_else(|| path_name(&source));
    let _guard = server.inner.desktop_project_lock.lock().await;
    let target = project_destination(&server, destination_name)?;
    if target.starts_with(&source) {
        return Err(bad_request(
            "Project destination cannot be inside the imported source",
        ));
    }
    let source_for_copy = source.clone();
    let target_for_copy = target.clone();
    let excluded =
        tokio::task::spawn_blocking(move || copy_import_tree(&source_for_copy, &target_for_copy))
            .await
            .map_err(|_| internal_error("Local project import task failed"))??;
    activate_project(&server, &agent_id, &target).await?;
    let mut response = project_response(&target).0;
    if !excluded.is_empty() {
        response["excluded"] = json!(excluded);
    }
    Ok(Json(response))
}

async fn upload_zip(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Query(query): Query<UploadZipQuery>,
    multipart: Multipart,
) -> Result<Json<Value>, ApiError> {
    let agent_id = super::desktop_agents::requested_agent_id(&headers)?;
    super::desktop_agents::workspace_for_agent(&server, &agent_id).await?;
    let content = read_zip_upload(multipart).await?;
    let _guard = server.inner.desktop_project_lock.lock().await;
    let target = project_destination(&server, &query.name)?;
    let staging = projects_base(&server)?.join(format!(".qwenpaw-upload-{}", Uuid::now_v7()));
    let staging_for_extract = staging.clone();
    let extraction =
        tokio::task::spawn_blocking(move || extract_zip_safely(&content, &staging_for_extract))
            .await
            .map_err(|_| internal_error("Project archive extraction task failed"))?;
    if let Err(error) = extraction {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error);
    }
    let staging_for_merge = staging.clone();
    let target_for_merge = target.clone();
    let merge_result = tokio::task::spawn_blocking(move || {
        merge_staging_tree(&staging_for_merge, &target_for_merge)
    })
    .await
    .map_err(|_| internal_error("Project archive merge task failed"))?;
    let _ = std::fs::remove_dir_all(&staging);
    merge_result?;
    if !target.join(".git").is_dir() {
        run_git_init(&target).await?;
    }
    activate_project(&server, &agent_id, &target).await?;
    Ok(project_response(&target))
}

async fn clone_project(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Json(request): Json<CloneProjectRequest>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let agent_id = super::desktop_agents::requested_agent_id(&headers)?;
    super::desktop_agents::workspace_for_agent(&server, &agent_id).await?;
    let url = request.url.trim();
    if url.is_empty() || url.len() > 8_192 || url.chars().any(char::is_control) {
        return Err(bad_request("URL cannot be empty or invalid"));
    }
    let name = match request
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        Some(name) => name.to_owned(),
        None => repository_name(url)?,
    };
    let target = project_destination(&server, &name)?;
    let url = url.to_owned();
    let (event_tx, event_rx) = tokio::sync::mpsc::channel(64);
    tokio::spawn(async move {
        let _guard = server.inner.desktop_project_lock.lock().await;
        stream_clone(&server, &agent_id, &url, &target, &event_tx).await;
    });
    let events = stream::unfold(event_rx, |mut receiver| async move {
        receiver.recv().await.map(|event| (event, receiver))
    });
    Ok(Sse::new(events).keep_alive(KeepAlive::default()))
}

async fn stream_clone(
    server: &AppServer,
    agent_id: &str,
    url: &str,
    target: &Path,
    event_tx: &tokio::sync::mpsc::Sender<Result<Event, Infallible>>,
) {
    if let Some(parent) = target.parent()
        && tokio::fs::create_dir_all(parent).await.is_err()
    {
        send_clone_event(
            event_tx,
            json!({
                "type": "error",
                "detail": "Project directory could not be created"
            }),
        )
        .await;
        return;
    }
    let status = match run_clone_process(url, target, event_tx).await {
        Ok(status) => status,
        Err(detail) => {
            send_clone_event(event_tx, json!({"type": "error", "detail": detail})).await;
            return;
        }
    };
    if !status.success() {
        send_clone_event(
            event_tx,
            json!({
                "type": "error",
                "detail": format!("git clone exited with code {}", status.code().unwrap_or(-1))
            }),
        )
        .await;
        return;
    }
    if activate_project(server, agent_id, target).await.is_err() {
        send_clone_event(
            event_tx,
            json!({
                "type": "error",
                "detail": "Cloned project could not be activated"
            }),
        )
        .await;
        return;
    }
    send_clone_event(
        event_tx,
        json!({
            "type": "done",
            "path": target.to_string_lossy(),
            "name": path_name(target)
        }),
    )
    .await;
}

async fn run_clone_process(
    url: &str,
    target: &Path,
    event_tx: &tokio::sync::mpsc::Sender<Result<Event, Infallible>>,
) -> Result<ExitStatus, &'static str> {
    let mut command = Command::new("git");
    command
        .args(["clone", "--progress", "--", url])
        .arg(target)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let Ok(mut child) = command.spawn() else {
        return Err("Git executable could not be started");
    };
    let Some(mut stderr) = child.stderr.take() else {
        let _ = child.kill().await;
        return Err("Git progress could not be captured");
    };
    let streamed = tokio::time::timeout(CLONE_TIMEOUT, async {
        let mut chunk = [0_u8; 4_096];
        let mut pending = Vec::new();
        let mut total = 0_usize;
        loop {
            let read = stderr.read(&mut chunk).await.map_err(|_| ())?;
            if read == 0 {
                break;
            }
            total = total.saturating_add(read);
            if total > MAX_CLONE_OUTPUT_BYTES {
                return Err(());
            }
            pending.extend_from_slice(&chunk[..read]);
            emit_progress_lines(&mut pending, event_tx).await;
        }
        if !pending.is_empty() {
            let line = String::from_utf8_lossy(&pending).trim().to_owned();
            if !line.is_empty() {
                send_clone_event(event_tx, json!({"type": "log", "line": line})).await;
            }
        }
        child.wait().await.map_err(|_| ())
    })
    .await;
    let status = match streamed {
        Ok(Ok(status)) => status,
        Ok(Err(())) => {
            let _ = child.kill().await;
            return Err("git clone output or process failed");
        }
        Err(_) => {
            let _ = child.kill().await;
            return Err("git clone timed out");
        }
    };
    Ok(status)
}

async fn emit_progress_lines(
    pending: &mut Vec<u8>,
    event_tx: &tokio::sync::mpsc::Sender<Result<Event, Infallible>>,
) {
    loop {
        let Some(index) = pending
            .iter()
            .position(|byte| matches!(byte, b'\r' | b'\n'))
        else {
            return;
        };
        let line = String::from_utf8_lossy(&pending[..index]).trim().to_owned();
        let mut consumed = index + 1;
        if pending.get(index) == Some(&b'\r') && pending.get(consumed) == Some(&b'\n') {
            consumed += 1;
        }
        pending.drain(..consumed);
        if !line.is_empty() {
            send_clone_event(event_tx, json!({"type": "log", "line": line})).await;
        }
    }
}

async fn send_clone_event(
    event_tx: &tokio::sync::mpsc::Sender<Result<Event, Infallible>>,
    payload: Value,
) {
    let _ = event_tx
        .send(Ok(Event::default().data(payload.to_string())))
        .await;
}

async fn read_zip_upload(mut multipart: Multipart) -> Result<Vec<u8>, ApiError> {
    let mut content = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| bad_request("multipart upload is invalid"))?
    {
        if field.name() != Some("file") {
            continue;
        }
        if content.is_some() {
            return Err(bad_request("Project ZIP upload requires exactly one file"));
        }
        let bytes = field
            .bytes()
            .await
            .map_err(|_| bad_request("Project ZIP upload could not be read"))?;
        if bytes.len() > MAX_UPLOAD_BYTES {
            return Err(payload_too_large("Project ZIP exceeds the 32 MiB limit"));
        }
        content = Some(bytes.to_vec());
    }
    content.ok_or_else(|| bad_request("Project ZIP upload requires a file"))
}

fn extract_zip_safely(content: &[u8], staging: &Path) -> Result<(), ApiError> {
    let mut archive = ZipArchive::new(Cursor::new(content))
        .map_err(|_| bad_request("Uploaded project is not a valid ZIP archive"))?;
    if archive.len() > MAX_ZIP_ENTRIES {
        return Err(payload_too_large("Project ZIP contains too many entries"));
    }
    let mut expanded = 0_u64;
    let mut entries = Vec::with_capacity(archive.len());
    let mut seen = HashSet::new();
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|_| bad_request("Project ZIP entry could not be read"))?;
        let enclosed = file
            .enclosed_name()
            .ok_or_else(|| bad_request("Project ZIP contains an unsafe path"))?
            .clone();
        validate_archive_path(&enclosed)?;
        if file.is_symlink() {
            return Err(bad_request("Project ZIP cannot contain symbolic links"));
        }
        expanded = expanded.saturating_add(file.size());
        if expanded > MAX_ZIP_EXPANDED_BYTES {
            return Err(payload_too_large(
                "Project ZIP expands beyond the 512 MiB limit",
            ));
        }
        if !seen.insert(enclosed.clone()) {
            return Err(bad_request("Project ZIP contains duplicate paths"));
        }
        entries.push((enclosed, file.is_dir()));
    }
    std::fs::create_dir_all(staging)
        .map_err(|_| internal_error("Project ZIP staging directory could not be created"))?;
    for (index, (relative, is_dir)) in entries.into_iter().enumerate() {
        let mut file = archive
            .by_index(index)
            .map_err(|_| bad_request("Project ZIP entry could not be read"))?;
        let target = staging.join(&relative);
        if is_dir {
            std::fs::create_dir_all(&target)
                .map_err(|_| internal_error("Project ZIP directory could not be created"))?;
            continue;
        }
        let parent = target
            .parent()
            .ok_or_else(|| bad_request("Project ZIP contains an unsafe path"))?;
        std::fs::create_dir_all(parent)
            .map_err(|_| internal_error("Project ZIP directory could not be created"))?;
        let mut output = std::fs::File::create(&target)
            .map_err(|_| internal_error("Project ZIP file could not be created"))?;
        std::io::copy(&mut file, &mut output)
            .map_err(|_| internal_error("Project ZIP file could not be extracted"))?;
    }
    Ok(())
}

fn validate_archive_path(path: &Path) -> Result<(), ApiError> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(bad_request("Project ZIP contains an unsafe path"));
    }
    Ok(())
}

fn merge_staging_tree(staging: &Path, target: &Path) -> Result<(), ApiError> {
    if !target.exists() {
        std::fs::rename(staging, target)
            .map_err(|_| internal_error("Imported project could not be installed"))?;
        return Ok(());
    }
    if target
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
        || !target.is_dir()
    {
        return Err(bad_request("Project destination is not a safe directory"));
    }
    copy_tree(staging, staging, target, false, &mut Vec::new(), &mut 0)
}

fn copy_import_tree(source: &Path, target: &Path) -> Result<Vec<String>, ApiError> {
    let mut excluded = Vec::new();
    let mut entries = 0_usize;
    copy_tree(source, source, target, true, &mut excluded, &mut entries)?;
    excluded.sort();
    excluded.dedup();
    Ok(excluded)
}

fn copy_tree(
    root: &Path,
    source: &Path,
    target: &Path,
    filter_import: bool,
    excluded: &mut Vec<String>,
    entries: &mut usize,
) -> Result<(), ApiError> {
    std::fs::create_dir_all(target)
        .map_err(|_| internal_error("Imported project directory could not be created"))?;
    for item in std::fs::read_dir(source)
        .map_err(|_| internal_error("Local project directory could not be read"))?
    {
        *entries += 1;
        if *entries > MAX_IMPORT_ENTRIES {
            return Err(payload_too_large("Local project contains too many entries"));
        }
        let item = item.map_err(|_| internal_error("Local project entry could not be read"))?;
        let name = item.file_name();
        let name_text = name.to_string_lossy();
        let source_path = item.path();
        let relative = source_path
            .strip_prefix(root)
            .unwrap_or(&source_path)
            .to_string_lossy()
            .into_owned();
        let metadata = std::fs::symlink_metadata(&source_path)
            .map_err(|_| internal_error("Local project metadata could not be read"))?;
        if is_link_or_junction(&metadata) {
            continue;
        }
        let sensitive =
            is_sensitive_component(&name_text) || contains_sensitive_sequence(&source_path);
        let excluded_build = EXCLUDED_NAMES.iter().any(|excluded| name_text == *excluded);
        if filter_import && (sensitive || excluded_build) {
            if sensitive {
                excluded.push(relative);
            }
            continue;
        }
        let target_path = target.join(&name);
        if target_path
            .symlink_metadata()
            .is_ok_and(|existing| is_link_or_junction(&existing))
        {
            return Err(bad_request("Project destination contains a symbolic link"));
        }
        if metadata.is_dir() {
            copy_tree(
                root,
                &source_path,
                &target_path,
                filter_import,
                excluded,
                entries,
            )?;
        } else if metadata.is_file() {
            std::fs::copy(&source_path, &target_path)
                .map_err(|_| internal_error("Local project file could not be copied"))?;
        }
    }
    Ok(())
}

fn is_sensitive_component(name: &str) -> bool {
    SENSITIVE_NAMES
        .iter()
        .any(|sensitive| name.eq_ignore_ascii_case(sensitive))
}

fn contains_sensitive_sequence(path: &Path) -> bool {
    let components = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().to_ascii_lowercase()),
            _ => None,
        })
        .collect::<Vec<_>>();
    SENSITIVE_SEQUENCES.iter().any(|sequence| {
        components.windows(sequence.len()).any(|window| {
            window
                .iter()
                .zip(*sequence)
                .all(|(actual, expected)| actual == expected)
        })
    })
}

#[cfg(windows)]
fn is_link_or_junction(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_link_or_junction(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn canonical_import_source(path: &str) -> Result<PathBuf, ApiError> {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed.len() > 4_096 || trimmed.chars().any(char::is_control) {
        return Err(bad_request("Local project path is invalid"));
    }
    let source = PathBuf::from(trimmed)
        .canonicalize()
        .map_err(|_| bad_request("Path does not exist"))?;
    if !source.is_dir() {
        return Err(bad_request("Not a directory"));
    }
    Ok(source)
}

fn validate_import_source(source: &Path) -> Result<(), ApiError> {
    let home = dirs::home_dir()
        .and_then(|home| home.canonicalize().ok())
        .ok_or_else(|| internal_error("User home directory could not be resolved"))?;
    if source == home {
        return Err(forbidden("Cannot import the entire home directory"));
    }
    let relative = source
        .strip_prefix(&home)
        .map_err(|_| forbidden("Source must be under home directory"))?;
    if relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy()),
            _ => None,
        })
        .any(|component| is_sensitive_component(&component))
        || contains_sensitive_sequence(relative)
    {
        return Err(forbidden("Path contains a sensitive component"));
    }
    Ok(())
}

fn project_destination(server: &AppServer, name: &str) -> Result<PathBuf, ApiError> {
    let name = validate_project_name(name)?;
    let base = projects_base(server)?;
    std::fs::create_dir_all(&base)
        .map_err(|_| internal_error("Project storage directory could not be created"))?;
    let base = base
        .canonicalize()
        .map_err(|_| internal_error("Project storage directory could not be resolved"))?;
    let target = base.join(name);
    if target
        .symlink_metadata()
        .is_ok_and(|metadata| is_link_or_junction(&metadata))
    {
        return Err(bad_request("Project destination contains a symbolic link"));
    }
    if target.exists() {
        let resolved = target
            .canonicalize()
            .map_err(|_| bad_request("Project destination could not be resolved"))?;
        if !resolved.starts_with(&base) || !resolved.is_dir() {
            return Err(bad_request("Project destination is not a safe directory"));
        }
    }
    Ok(target)
}

fn projects_base(server: &AppServer) -> Result<PathBuf, ApiError> {
    let workspace = server.inner.desktop_workspace.as_ref().ok_or_else(|| {
        (
            StatusCode::NOT_IMPLEMENTED,
            Json(json!({"detail": "Desktop Workspace is unavailable"})),
        )
    })?;
    Ok(workspace.initial.join(PROJECTS_DIRECTORY))
}

fn validate_project_name(name: &str) -> Result<&str, ApiError> {
    let name = name.trim();
    if name.is_empty()
        || name.len() > MAX_PROJECT_NAME_BYTES
        || matches!(name, "." | "..")
        || name.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
        })
        || name.ends_with([' ', '.'])
        || is_windows_reserved_name(name)
    {
        return Err(bad_request("Invalid project name"));
    }
    Ok(name)
}

fn is_windows_reserved_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or_default();
    let upper = stem.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && matches!(&upper[..3], "COM" | "LPT")
            && matches!(upper.as_bytes()[3], b'1'..=b'9'))
}

fn repository_name(url: &str) -> Result<String, ApiError> {
    let mut name = url
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\', ':'])
        .next()
        .unwrap_or_default()
        .to_owned();
    if Path::new(&name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("git"))
    {
        name.truncate(name.len() - 4);
    }
    validate_project_name(&name)?;
    Ok(name)
}

async fn run_git_init(path: &Path) -> Result<(), ApiError> {
    let mut command = Command::new("git");
    command
        .arg("init")
        .current_dir(path)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let status = tokio::time::timeout(GIT_TIMEOUT, command.status())
        .await
        .map_err(|_| internal_error("git init timed out"))?
        .map_err(|_| internal_error("Git executable could not be started"))?;
    if !status.success() {
        return Err(internal_error("git init failed"));
    }
    Ok(())
}

async fn activate_project(server: &AppServer, agent_id: &str, path: &Path) -> Result<(), ApiError> {
    let selected = path
        .canonicalize()
        .map_err(|_| internal_error("Project directory could not be resolved"))?;
    if agent_id == "default" {
        let selected = server
            .inner
            .core
            .write_preferred_workspace(&selected)
            .map(PathBuf::from)
            .map_err(|error| internal_error(&error.to_string()))?;
        let workspace = server
            .inner
            .desktop_workspace
            .as_ref()
            .ok_or_else(|| internal_error("Desktop Workspace is unavailable"))?;
        selected.clone_into(&mut *workspace.selected.write().await);
    }
    super::desktop_agents::replace_config_field(
        server,
        agent_id,
        "project_dir",
        Value::String(selected.to_string_lossy().into_owned()),
    )
    .await?;
    Ok(())
}

fn project_response(path: &Path) -> Json<Value> {
    Json(json!({
        "path": path.to_string_lossy(),
        "name": path_name(path)
    }))
}

fn path_name(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
}

fn bad_request(message: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, Json(json!({"detail": message})))
}

fn forbidden(message: &str) -> ApiError {
    (StatusCode::FORBIDDEN, Json(json!({"detail": message})))
}

fn payload_too_large(message: &str) -> ApiError {
    (
        StatusCode::PAYLOAD_TOO_LARGE,
        Json(json!({"detail": message})),
    )
}

fn internal_error(message: &str) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"detail": message})),
    )
}
