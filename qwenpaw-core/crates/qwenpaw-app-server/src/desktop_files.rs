use std::collections::HashSet;
use std::convert::Infallible;
use std::io::Write;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use axum::Json;
use axum::Router;
use axum::body::Body;
use axum::extract::DefaultBodyLimit;
use axum::extract::Multipart;
use axum::extract::Path as AxumPath;
use axum::extract::Query;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::http::header::CONTENT_LENGTH;
use axum::http::header::CONTENT_TYPE;
use axum::http::header::ETAG;
use axum::response::Response;
use axum::response::Sse;
use axum::response::sse::Event;
use axum::response::sse::KeepAlive;
use axum::routing::get;
use axum::routing::post;
use chrono::DateTime;
use chrono::SecondsFormat;
use futures_util::stream;
use notify::EventKind;
use notify::RecursiveMode;
use notify::Watcher;
use notify::event::ModifyKind;
use notify::event::RenameMode;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;
use sha2::Digest;
use sha2::Sha256;
use tokio_util::io::ReaderStream;
use url::Url;
use uuid::Uuid;

use super::AppServer;

const MAX_PATH_BYTES: usize = 4_096;
const MAX_DIRECTORY_ENTRIES: usize = 500;
const DEFAULT_DIRECTORY_LIMIT: usize = 200;
const MAX_TEXT_FILE_BYTES: usize = 8 * 1_024 * 1_024;
const MAX_PROFILE_MARKDOWN_BYTES: usize = 1_024 * 1_024;
const MAX_CONTENT_CHUNK_BYTES: usize = 512 * 1_024;
const MAX_UPLOAD_FILE_BYTES: usize = 32 * 1_024 * 1_024;
const MAX_MULTIPART_BODY_BYTES: usize = MAX_UPLOAD_FILE_BYTES + 1_024 * 1_024;
const ATTACHMENTS_DIRECTORY: &str = "attachments";
const WORKSPACE_ATTACHMENTS_DIRECTORY: &str = ".qwenpaw/attachments";

pub(super) type ApiError = (StatusCode, Json<Value>);

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route("/api/workspace/files", get(list_profile_files))
        .route(
            "/api/workspace/files/{file_name}",
            get(load_profile_file)
                .put(save_profile_file)
                .layer(DefaultBodyLimit::max(
                    MAX_PROFILE_MARKDOWN_BYTES + 64 * 1_024,
                )),
        )
        .route("/api/workspace/tree", get(list_directory))
        .route("/api/workspace/file-metadata", get(file_metadata))
        .route(
            "/api/workspace/file-content",
            get(file_content)
                .put(save_file_content)
                .layer(DefaultBodyLimit::max(MAX_TEXT_FILE_BYTES + 64 * 1_024)),
        )
        .route("/api/workspace/file-download", get(download_file))
        .route("/api/workspace/html-file-uri", get(html_file_uri))
        .route("/api/workspace/binary-files/{*path}", get(binary_file))
        .route("/api/workspace/watch", get(watch_workspace))
        .route(
            "/api/workspace/file-upload",
            post(upload_workspace_files).layer(DefaultBodyLimit::max(MAX_MULTIPART_BODY_BYTES)),
        )
        .route(
            "/api/console/upload",
            post(upload_chat_attachment).layer(DefaultBodyLimit::max(MAX_MULTIPART_BODY_BYTES)),
        )
        .route("/api/files/preview/{*path}", get(preview_attachment))
}

#[derive(Debug, Default, Deserialize)]
struct WorkspaceQuery {
    #[serde(default)]
    path: String,
    #[serde(default)]
    root: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct DirectoryQuery {
    #[serde(default)]
    path: String,
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    root: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct FileContentQuery {
    path: String,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SaveFileRequest {
    content: String,
}

async fn list_profile_files(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let workspace = desktop_workspace(&server)?;
    let root = workspace.selected.read().await.clone();
    let mut reader = tokio::fs::read_dir(&root)
        .await
        .map_err(|_| not_found("Workspace directory could not be read"))?;
    let mut files = Vec::new();
    while let Some(entry) = reader
        .next_entry()
        .await
        .map_err(|_| not_found("Workspace directory could not be read"))?
    {
        let filename = entry.file_name().to_string_lossy().into_owned();
        if Path::new(&filename)
            .extension()
            .is_none_or(|value| value != "md")
        {
            continue;
        }
        let Ok(metadata) = tokio::fs::symlink_metadata(entry.path()).await else {
            continue;
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            continue;
        }
        let modified = metadata.modified().ok();
        files.push((
            modified,
            filename.clone(),
            json!({
                "filename": filename,
                "path": entry.path().to_string_lossy(),
                "size": metadata.len(),
                "created_time": metadata.created().map(timestamp_at).unwrap_or_default(),
                "modified_time": modified.map(timestamp_at).unwrap_or_default()
            }),
        ));
    }
    files.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    Ok(Json(Value::Array(
        files.into_iter().map(|(_, _, value)| value).collect(),
    )))
}

async fn load_profile_file(
    State(server): State<AppServer>,
    AxumPath(file_name): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    let file_name = normalize_profile_filename(&file_name)?;
    let workspace = desktop_workspace(&server)?;
    let root = workspace.selected.read().await.clone();
    let candidate = root.join(&file_name);
    let metadata = tokio::fs::symlink_metadata(&candidate)
        .await
        .map_err(|_| not_found("Profile Markdown file was not found"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(bad_request(
            "Profile Markdown target must be a regular file",
        ));
    }
    let path = resolve_existing_file(&root, Path::new(&file_name))?;
    if metadata.len() > MAX_PROFILE_MARKDOWN_BYTES as u64 {
        return Err(payload_too_large(
            "Profile Markdown file exceeds the 1 MiB limit",
        ));
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|_| not_found("Profile Markdown file could not be read"))?;
    Ok(Json(json!({
        "content": String::from_utf8_lossy(&bytes).trim()
    })))
}

async fn save_profile_file(
    State(server): State<AppServer>,
    AxumPath(file_name): AxumPath<String>,
    Json(request): Json<SaveFileRequest>,
) -> Result<Json<Value>, ApiError> {
    if request.content.len() > MAX_PROFILE_MARKDOWN_BYTES {
        return Err(payload_too_large(
            "Profile Markdown file exceeds the 1 MiB limit",
        ));
    }
    let file_name = normalize_profile_filename(&file_name)?;
    let _guard = server.inner.desktop_agent_settings_lock.lock().await;
    let workspace = desktop_workspace(&server)?;
    let root = workspace.selected.read().await.clone();
    let path = resolve_write_file(&root, Path::new(&file_name))?;
    write_file_atomically(&path, request.content.as_bytes()).await?;
    Ok(Json(json!({"written": true})))
}

fn normalize_profile_filename(value: &str) -> Result<String, ApiError> {
    if value.len() > 255 {
        return Err(bad_request("Profile Markdown file name is invalid"));
    }
    let value = parse_direct_file_name(value)?;
    let filename = if Path::new(value)
        .extension()
        .is_some_and(|extension| extension == "md")
    {
        value.to_owned()
    } else {
        format!("{value}.md")
    };
    if filename.len() > 255 {
        return Err(bad_request("Profile Markdown file name is invalid"));
    }
    Ok(filename)
}

#[derive(Debug, Default, Deserialize)]
struct UploadQuery {
    #[serde(default)]
    path: String,
    #[serde(default)]
    conflict: Option<String>,
    #[serde(default)]
    root: Option<String>,
}

#[derive(Debug)]
struct PendingUpload {
    name: String,
    bytes: Vec<u8>,
}

async fn list_directory(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Query(query): Query<DirectoryQuery>,
) -> Result<Json<Value>, ApiError> {
    let root = resolve_workspace_root(&server, &headers, query.root.as_deref()).await?;
    let relative = parse_relative_path(&query.path)?;
    let directory = resolve_existing(&root, &relative)?;
    if !directory.is_dir() {
        return Err(bad_request("Workspace path is not a directory"));
    }
    let offset = query
        .cursor
        .as_deref()
        .map(str::parse::<usize>)
        .transpose()
        .map_err(|_| bad_request("directory cursor is invalid"))?
        .unwrap_or(0);
    let limit = query
        .limit
        .unwrap_or(DEFAULT_DIRECTORY_LIMIT)
        .clamp(1, MAX_DIRECTORY_ENTRIES);
    let mut reader = tokio::fs::read_dir(&directory)
        .await
        .map_err(|_| not_found("Workspace directory could not be read"))?;
    let mut entries = Vec::new();
    while let Some(entry) = reader
        .next_entry()
        .await
        .map_err(|_| not_found("Workspace directory could not be read"))?
    {
        let Ok(file_type) = entry.file_type().await else {
            continue;
        };
        if file_type.is_symlink() || (!file_type.is_dir() && !file_type.is_file()) {
            continue;
        }
        let Ok(metadata) = entry.metadata().await else {
            continue;
        };
        let entry_path = entry.path();
        let relative_path = relative_display(&root, &entry_path)?;
        entries.push(json!({
            "name": entry.file_name().to_string_lossy(),
            "path": relative_path,
            "kind": if file_type.is_dir() { "directory" } else { "file" },
            "size": file_type.is_file().then_some(metadata.len()),
            "modified_at": modified_at(&metadata),
            "preview_kind": if file_type.is_file() {
                preview_kind(&entry_path)
            } else {
                "directory"
            }
        }));
    }
    entries.sort_by(|left, right| {
        let left_directory = left["kind"] == "directory";
        let right_directory = right["kind"] == "directory";
        right_directory.cmp(&left_directory).then_with(|| {
            left["name"]
                .as_str()
                .unwrap_or_default()
                .to_lowercase()
                .cmp(&right["name"].as_str().unwrap_or_default().to_lowercase())
        })
    });
    if offset > entries.len() {
        return Err(bad_request("directory cursor is outside the result set"));
    }
    let end = offset.saturating_add(limit).min(entries.len());
    let has_more = end < entries.len();
    Ok(Json(json!({
        "directory": relative_display(&root, &directory)?,
        "entries": entries[offset..end].to_vec(),
        "next_cursor": has_more.then(|| end.to_string()),
        "has_more": has_more
    })))
}

async fn file_metadata(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Query(query): Query<WorkspaceQuery>,
) -> Result<Json<Value>, ApiError> {
    let root = resolve_workspace_root(&server, &headers, query.root.as_deref()).await?;
    let relative = parse_relative_path(&query.path)?;
    let path = resolve_existing_file(&root, &relative)?;
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|_| not_found("Workspace file was not found"))?;
    Ok(Json(json!({
        "path": relative_display(&root, &path)?,
        "size": metadata.len(),
        "modified_at": modified_at(&metadata),
        "preview_kind": preview_kind(&path),
        "etag": metadata_etag(&metadata)
    })))
}

async fn file_content(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Query(query): Query<FileContentQuery>,
) -> Result<Json<Value>, ApiError> {
    let root = resolve_workspace_root(&server, &headers, query.root.as_deref()).await?;
    let relative = parse_relative_path(&query.path)?;
    let path = resolve_existing_file(&root, &relative)?;
    let bytes = read_text_file(&path).await?;
    let offset = query.offset.unwrap_or(0);
    if offset > bytes.len() || !bytes.is_char_boundary(offset) {
        return Err(bad_request("file content offset is invalid"));
    }
    let limit = query
        .limit
        .unwrap_or(256 * 1_024)
        .clamp(1, MAX_CONTENT_CHUNK_BYTES);
    let mut end = offset.saturating_add(limit).min(bytes.len());
    while end > offset && !bytes.is_char_boundary(end) {
        end -= 1;
    }
    if end == offset && offset < bytes.len() {
        end += 1;
        while end < bytes.len() && !bytes.is_char_boundary(end) {
            end += 1;
        }
    }
    let eof = end == bytes.len();
    Ok(Json(json!({
        "path": relative_display(&root, &path)?,
        "content": &bytes[offset..end],
        "offset": offset,
        "limit": limit,
        "next_offset": end,
        "eof": eof,
        "truncated": !eof,
        "encoding": "utf-8",
        "etag": content_etag(bytes.as_bytes())
    })))
}

async fn save_file_content(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Query(query): Query<WorkspaceQuery>,
    Json(request): Json<SaveFileRequest>,
) -> Result<Json<Value>, ApiError> {
    if request.content.len() > MAX_TEXT_FILE_BYTES {
        return Err(payload_too_large(
            "Workspace text file exceeds the 8 MiB limit",
        ));
    }
    let root = resolve_workspace_root(&server, &headers, query.root.as_deref()).await?;
    let relative = parse_relative_file_path(&query.path)?;
    let path = resolve_write_file(&root, &relative)?;
    if let Some(expected) = headers
        .get("if-match")
        .and_then(|value| value.to_str().ok())
    {
        let current = tokio::fs::read(&path)
            .await
            .map_err(|_| not_found("Workspace file was not found"))?;
        if expected != content_etag(&current) {
            return Err((
                StatusCode::PRECONDITION_FAILED,
                Json(json!({"detail": "Workspace file changed since it was opened"})),
            ));
        }
    }
    write_file_atomically(&path, request.content.as_bytes()).await?;
    Ok(Json(json!({
        "path": relative_display(&root, &path)?,
        "size": request.content.len(),
        "etag": content_etag(request.content.as_bytes())
    })))
}

async fn download_file(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Query(query): Query<WorkspaceQuery>,
) -> Result<Response, ApiError> {
    let root = resolve_workspace_root(&server, &headers, query.root.as_deref()).await?;
    let relative = parse_relative_path(&query.path)?;
    let path = resolve_existing_file(&root, &relative)?;
    stream_file(path).await
}

async fn binary_file(
    State(server): State<AppServer>,
    headers: HeaderMap,
    AxumPath(path): AxumPath<String>,
) -> Result<Response, ApiError> {
    let root = resolve_workspace_root(&server, &headers, Some("project")).await?;
    let relative = parse_relative_path(&path)?;
    let path = resolve_existing_file(&root, &relative)?;
    stream_file(path).await
}

async fn html_file_uri(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Query(query): Query<WorkspaceQuery>,
) -> Result<Json<Value>, ApiError> {
    let root = resolve_workspace_root(&server, &headers, query.root.as_deref()).await?;
    let relative = parse_relative_path(&query.path)?;
    let path = resolve_existing_file(&root, &relative)?;
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    if !matches!(extension.to_ascii_lowercase().as_str(), "html" | "htm") {
        return Err(bad_request(
            "Workspace HTML resolver accepts only .html or .htm files",
        ));
    }
    let uri = Url::from_file_path(&path)
        .map_err(|()| bad_request("Workspace HTML file URI could not be created"))?;
    Ok(Json(json!({"uri": uri.as_str()})))
}

async fn upload_workspace_files(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Query(query): Query<UploadQuery>,
    multipart: Multipart,
) -> Result<Json<Value>, ApiError> {
    let root = resolve_workspace_root(&server, &headers, query.root.as_deref()).await?;
    let relative = parse_relative_path(&query.path)?;
    let directory = resolve_existing(&root, &relative)?;
    if !directory.is_dir() {
        return Err(bad_request("Workspace upload target is not a directory"));
    }
    let uploads = collect_uploads(multipart, "files").await?;
    if uploads.is_empty() {
        return Err(bad_request("Workspace upload requires at least one file"));
    }
    let conflict = query.conflict.as_deref();
    if !matches!(conflict, None | Some("overwrite" | "skip" | "rename")) {
        return Err(bad_request("Workspace upload conflict mode is invalid"));
    }
    let mut seen = HashSet::new();
    let conflicting = uploads
        .iter()
        .filter(|upload| directory.join(&upload.name).exists() || !seen.insert(upload.name.clone()))
        .map(|upload| upload.name.clone())
        .collect::<Vec<_>>();
    if conflict.is_none() && !conflicting.is_empty() {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({
                "detail": {"code": "upload_conflict", "files": conflicting}
            })),
        ));
    }
    let mut results = Vec::with_capacity(uploads.len());
    for upload in uploads {
        let requested = directory.join(&upload.name);
        let (target, status) = if requested.exists() {
            match conflict {
                Some("skip") => {
                    results.push(json!({
                        "name": upload.name,
                        "path": relative_display(&root, &requested)?,
                        "size": upload.bytes.len(),
                        "status": "skipped"
                    }));
                    continue;
                }
                Some("rename") => (unique_upload_path(&directory, &upload.name)?, "uploaded"),
                Some("overwrite") => (
                    resolve_write_file(
                        &root,
                        &parse_relative_path(&relative_display(&root, &requested)?)?,
                    )?,
                    "uploaded",
                ),
                None => unreachable!("conflicts return before writes"),
                Some(_) => unreachable!("conflict mode was validated"),
            }
        } else {
            (requested, "uploaded")
        };
        write_file_atomically(&target, &upload.bytes).await?;
        results.push(json!({
            "name": upload.name,
            "path": relative_display(&root, &target)?,
            "size": upload.bytes.len(),
            "status": status
        }));
    }
    Ok(Json(json!({"files": results})))
}

async fn upload_chat_attachment(
    State(server): State<AppServer>,
    multipart: Multipart,
) -> Result<Json<Value>, ApiError> {
    let mut uploads = collect_uploads(multipart, "file").await?;
    if uploads.len() != 1 {
        return Err(bad_request(
            "Console attachment upload requires exactly one file",
        ));
    }
    let upload = uploads.pop().expect("one attachment should be present");
    let workspace = desktop_workspace(&server)?;
    let attachments = workspace.data_dir.join(ATTACHMENTS_DIRECTORY);
    tokio::fs::create_dir_all(&attachments)
        .await
        .map_err(|_| internal_error("Attachment directory could not be created"))?;
    let stored_name = format!("{}-{}", Uuid::now_v7(), upload.name);
    let target = attachments.join(&stored_name);
    write_new_file(&target, &upload.bytes).await?;
    Ok(Json(json!({
        "url": stored_name,
        "file_name": upload.name,
        "stored_name": target
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
    })))
}

async fn preview_attachment(
    State(server): State<AppServer>,
    AxumPath(path): AxumPath<String>,
) -> Result<Response, ApiError> {
    let name = parse_direct_file_name(&path)?;
    let attachments = desktop_workspace(&server)?
        .data_dir
        .join(ATTACHMENTS_DIRECTORY);
    let path = attachments.join(name);
    let canonical = path
        .canonicalize()
        .map_err(|_| not_found("Attachment was not found"))?;
    let attachments = attachments
        .canonicalize()
        .map_err(|_| not_found("Attachment directory was not found"))?;
    if !canonical.starts_with(&attachments) || !canonical.is_file() {
        return Err(not_found("Attachment was not found"));
    }
    stream_file(canonical).await
}

struct WorkspaceWatchState {
    receiver: tokio::sync::mpsc::Receiver<Value>,
    _watcher: notify::RecommendedWatcher,
}

async fn watch_workspace(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Query(query): Query<WorkspaceQuery>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let root = resolve_workspace_root(&server, &headers, query.root.as_deref()).await?;
    let (sender, receiver) = tokio::sync::mpsc::channel(256);
    let event_root = root.clone();
    let mut watcher = notify::recommended_watcher(move |event| {
        let payload = match event {
            Ok(event) => workspace_watch_payload(&event_root, &event),
            Err(error) => {
                tracing::warn!(%error, "Workspace watcher reported an error");
                None
            }
        };
        if let Some(payload) = payload {
            let _ = sender.blocking_send(payload);
        }
    })
    .map_err(|error| {
        tracing::warn!(%error, "Workspace watcher could not start");
        internal_error("Workspace watcher could not start")
    })?;
    watcher
        .watch(&root, RecursiveMode::Recursive)
        .map_err(|error| {
            tracing::warn!(%error, root = %root.display(), "Workspace path could not be watched");
            internal_error("Workspace path could not be watched")
        })?;
    let state = WorkspaceWatchState {
        receiver,
        _watcher: watcher,
    };
    let events = stream::unfold(state, |mut state| async move {
        state.receiver.recv().await.map(|payload| {
            let event = Ok(Event::default().data(payload.to_string()));
            (event, state)
        })
    });
    Ok(Sse::new(events).keep_alive(KeepAlive::default()))
}

fn workspace_watch_payload(root: &Path, event: &notify::Event) -> Option<Value> {
    let changes = event_changes(root, event);
    (!changes.is_empty()).then(|| {
        json!({
            "type": "file_change",
            "events": changes
        })
    })
}

fn event_changes(root: &Path, event: &notify::Event) -> Vec<Value> {
    let mut changes = HashSet::new();
    match &event.kind {
        EventKind::Create(_) => add_event_paths(&mut changes, root, &event.paths, "added"),
        EventKind::Remove(_) => add_event_paths(&mut changes, root, &event.paths, "deleted"),
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if event.paths.len() >= 2 => {
            add_event_paths(&mut changes, root, &event.paths[..1], "deleted");
            add_event_paths(&mut changes, root, &event.paths[1..], "added");
        }
        EventKind::Modify(_) | EventKind::Any => {
            add_event_paths(&mut changes, root, &event.paths, "modified");
        }
        EventKind::Access(_) | EventKind::Other => {}
    }
    let mut changes = changes
        .into_iter()
        .map(|(change, path)| json!({"change": change, "path": path}))
        .collect::<Vec<_>>();
    changes.sort_by(|left, right| {
        left["path"]
            .as_str()
            .cmp(&right["path"].as_str())
            .then_with(|| left["change"].as_str().cmp(&right["change"].as_str()))
    });
    changes
}

fn add_event_paths(
    changes: &mut HashSet<(&'static str, String)>,
    root: &Path,
    paths: &[PathBuf],
    change: &'static str,
) {
    for path in paths {
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        if relative.as_os_str().is_empty()
            || relative.components().any(|component| {
                component
                    .as_os_str()
                    .to_string_lossy()
                    .starts_with(".qwenpaw-write-")
            })
        {
            continue;
        }
        let path = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        changes.insert((change, path));
    }
}

pub(super) async fn console_user_input(
    server: &AppServer,
    input: &[Value],
    workspace_root: &Path,
) -> Result<Vec<qwenpaw_protocol::UserInput>, ApiError> {
    let message = input
        .iter()
        .rev()
        .find(|message| message.get("role").and_then(Value::as_str) == Some("user"))
        .ok_or_else(|| bad_request("input must contain a user message"))?;
    let content = message
        .get("content")
        .ok_or_else(|| bad_request("user message content is required"))?;
    if let Some(text) = content.as_str() {
        if text.trim().is_empty() {
            return Err(bad_request("user message text must not be empty"));
        }
        return Ok(vec![qwenpaw_protocol::UserInput::Text {
            text: text.to_owned(),
        }]);
    }
    let parts = content
        .as_array()
        .ok_or_else(|| bad_request("user message content must be text or an array"))?;
    let mut result = Vec::new();
    for part in parts {
        match part.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = part.get("text").and_then(Value::as_str)
                    && !text.trim().is_empty()
                {
                    result.push(qwenpaw_protocol::UserInput::Text {
                        text: text.to_owned(),
                    });
                }
            }
            Some("file" | "image" | "video" | "audio") => {
                let stored = attachment_reference(part)?;
                let relative =
                    copy_attachment_into_workspace(server, workspace_root, &stored).await?;
                result.push(qwenpaw_protocol::UserInput::FileReference {
                    path: relative,
                    start_line: None,
                    end_line: None,
                });
            }
            Some(_) => {
                return Err((
                    StatusCode::NOT_IMPLEMENTED,
                    Json(json!({"detail": "Console message content type is not supported"})),
                ));
            }
            None => return Err(bad_request("Console message content type is required")),
        }
    }
    if result.is_empty() {
        return Err(bad_request("user message content must not be empty"));
    }
    Ok(result)
}

fn attachment_reference(part: &Value) -> Result<String, ApiError> {
    let kind = part.get("type").and_then(Value::as_str).unwrap_or_default();
    let value = match kind {
        "file" => part.get("file_url").and_then(Value::as_str),
        "image" => part.get("image_url").and_then(Value::as_str),
        "video" => part.get("video_url").and_then(Value::as_str),
        "audio" => part.get("data").and_then(Value::as_str),
        _ => None,
    }
    .ok_or_else(|| bad_request("attachment URL is required"))?;
    let without_query = value.split('?').next().unwrap_or(value);
    let stored = without_query
        .rsplit('/')
        .next()
        .ok_or_else(|| bad_request("attachment URL is invalid"))?;
    Ok(parse_direct_file_name(stored)?.to_owned())
}

async fn copy_attachment_into_workspace(
    server: &AppServer,
    workspace_root: &Path,
    stored_name: &str,
) -> Result<String, ApiError> {
    let attachments = desktop_workspace(server)?
        .data_dir
        .join(ATTACHMENTS_DIRECTORY);
    let source = attachments.join(parse_direct_file_name(stored_name)?);
    let source = source
        .canonicalize()
        .map_err(|_| not_found("Attachment was not found"))?;
    let attachments = attachments
        .canonicalize()
        .map_err(|_| not_found("Attachment directory was not found"))?;
    if !source.starts_with(&attachments) || !source.is_file() {
        return Err(not_found("Attachment was not found"));
    }
    let workspace_root = workspace_root
        .canonicalize()
        .map_err(|_| bad_request("Workspace directory does not exist"))?;
    let destination_dir = ensure_workspace_attachment_directory(&workspace_root).await?;
    let destination = destination_dir.join(stored_name);
    if !destination.exists() {
        tokio::fs::copy(&source, &destination)
            .await
            .map_err(|_| internal_error("Attachment could not be copied into the Workspace"))?;
    }
    relative_display(&workspace_root, &destination)
}

async fn ensure_workspace_attachment_directory(root: &Path) -> Result<PathBuf, ApiError> {
    let mut current = root.to_path_buf();
    for component in WORKSPACE_ATTACHMENTS_DIRECTORY.split('/') {
        current.push(component);
        match tokio::fs::symlink_metadata(&current).await {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(bad_request(
                    "Workspace attachment directory must not contain symlinks",
                ));
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(bad_request(
                    "Workspace attachment path must contain only directories",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                tokio::fs::create_dir(&current).await.map_err(|_| {
                    internal_error("Workspace attachment directory could not be created")
                })?;
            }
            Err(_) => {
                return Err(internal_error(
                    "Workspace attachment directory could not be inspected",
                ));
            }
        }
    }
    let canonical = current
        .canonicalize()
        .map_err(|_| internal_error("Workspace attachment directory could not be resolved"))?;
    if !canonical.starts_with(root) {
        return Err(bad_request(
            "Workspace attachment directory resolves outside the Workspace",
        ));
    }
    Ok(canonical)
}

pub(super) async fn resolve_workspace_root(
    server: &AppServer,
    headers: &HeaderMap,
    requested_root: Option<&str>,
) -> Result<PathBuf, ApiError> {
    let workspace = desktop_workspace(server)?;
    match requested_root.unwrap_or("project") {
        "workspace" => Ok(workspace.initial.clone()),
        "project" => {
            if let Some(chat_id) = header_value(headers, "x-chat-id")? {
                let thread_id = server
                    .inner
                    .desktop_session_aliases
                    .read()
                    .await
                    .client_to_thread
                    .get(chat_id)
                    .cloned()
                    .unwrap_or_else(|| chat_id.to_owned());
                if let Ok(thread) = server.inner.core.read_thread(&thread_id).await
                    && let Some(root) = thread.thread.workspace_root
                {
                    return canonical_directory(&root);
                }
            }
            if let Some(path) = header_value(headers, "x-session-project-dir")? {
                return canonical_directory(path);
            }
            Ok(workspace.selected.read().await.clone())
        }
        root if root.starts_with("project:") => Err((
            StatusCode::NOT_IMPLEMENTED,
            Json(json!({"detail": "Rust Core currently supports one project directory per chat"})),
        )),
        _ => Err(bad_request("Workspace root is invalid")),
    }
}

fn desktop_workspace(server: &AppServer) -> Result<&super::DesktopWorkspace, ApiError> {
    server.inner.desktop_workspace.as_ref().ok_or_else(|| {
        (
            StatusCode::NOT_IMPLEMENTED,
            Json(json!({"detail": "Desktop Workspace is unavailable"})),
        )
    })
}

fn header_value<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, ApiError> {
    headers
        .get(name)
        .map(|value| {
            value
                .to_str()
                .map_err(|_| bad_request("Workspace request header is invalid"))
        })
        .transpose()
}

fn parse_relative_path(value: &str) -> Result<PathBuf, ApiError> {
    if value.len() > MAX_PATH_BYTES || value.chars().any(char::is_control) {
        return Err(bad_request("Workspace path is invalid"));
    }
    let path = Path::new(value);
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
        && !value.is_empty()
    {
        return Err(bad_request(
            "Workspace path must be relative without traversal",
        ));
    }
    Ok(path.to_path_buf())
}

fn parse_relative_file_path(value: &str) -> Result<PathBuf, ApiError> {
    let path = parse_relative_path(value)?;
    if path.as_os_str().is_empty() || path.file_name().is_none() {
        return Err(bad_request("Workspace file path is required"));
    }
    Ok(path)
}

fn parse_direct_file_name(value: &str) -> Result<&str, ApiError> {
    if value.is_empty()
        || value.len() > 512
        || value == "."
        || value == ".."
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\'))
    {
        return Err(bad_request("file name is invalid"));
    }
    Ok(value)
}

fn canonical_directory(value: &str) -> Result<PathBuf, ApiError> {
    let path = PathBuf::from(value)
        .canonicalize()
        .map_err(|_| bad_request("Workspace directory does not exist"))?;
    if !path.is_dir() {
        return Err(bad_request("Workspace path is not a directory"));
    }
    Ok(path)
}

fn resolve_existing(root: &Path, relative: &Path) -> Result<PathBuf, ApiError> {
    let candidate = root.join(relative);
    let path = candidate
        .canonicalize()
        .map_err(|_| not_found("Workspace path was not found"))?;
    if !path.starts_with(root) {
        return Err(bad_request("Workspace path resolves outside the Workspace"));
    }
    Ok(path)
}

fn resolve_existing_file(root: &Path, relative: &Path) -> Result<PathBuf, ApiError> {
    let path = resolve_existing(root, relative)?;
    if !path.is_file() {
        return Err(not_found("Workspace file was not found"));
    }
    Ok(path)
}

fn resolve_write_file(root: &Path, relative: &Path) -> Result<PathBuf, ApiError> {
    let candidate = root.join(relative);
    if candidate.exists() {
        let metadata = candidate
            .symlink_metadata()
            .map_err(|_| not_found("Workspace file was not found"))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(bad_request("Workspace write target must be a regular file"));
        }
        return resolve_existing_file(root, relative);
    }
    let parent = candidate
        .parent()
        .ok_or_else(|| bad_request("Workspace write path is invalid"))?
        .canonicalize()
        .map_err(|_| not_found("Workspace parent directory was not found"))?;
    if !parent.starts_with(root) {
        return Err(bad_request(
            "Workspace write path resolves outside the Workspace",
        ));
    }
    let name = candidate
        .file_name()
        .ok_or_else(|| bad_request("Workspace file name is invalid"))?;
    Ok(parent.join(name))
}

async fn read_text_file(path: &Path) -> Result<String, ApiError> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|_| not_found("Workspace file was not found"))?;
    if metadata.len() > MAX_TEXT_FILE_BYTES as u64 {
        return Err(payload_too_large(
            "Workspace text file exceeds the 8 MiB limit",
        ));
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|_| not_found("Workspace file could not be read"))?;
    String::from_utf8(bytes).map_err(|_| bad_request("Workspace file is not valid UTF-8"))
}

async fn write_file_atomically(path: &Path, bytes: &[u8]) -> Result<(), ApiError> {
    let parent = path
        .parent()
        .ok_or_else(|| bad_request("Workspace write path is invalid"))?;
    let parent = parent.to_path_buf();
    let path = path.to_path_buf();
    let bytes = bytes.to_vec();
    tokio::task::spawn_blocking(move || {
        let permissions = std::fs::metadata(&path)
            .ok()
            .map(|metadata| metadata.permissions());
        if permissions.is_none() {
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(path)?;
            file.write_all(&bytes)?;
            file.flush()?;
            return file.sync_all();
        }
        let mut temporary = tempfile::Builder::new()
            .prefix(".qwenpaw-write-")
            .tempfile_in(parent)?;
        if let Some(permissions) = permissions {
            temporary.as_file().set_permissions(permissions)?;
        }
        temporary.write_all(&bytes)?;
        temporary.flush()?;
        temporary.as_file().sync_all()?;
        temporary.persist(path).map_err(|error| error.error)?;
        Ok::<(), std::io::Error>(())
    })
    .await
    .map_err(|_| internal_error("Workspace atomic write task failed"))?
    .map_err(|error| {
        tracing::warn!(%error, "Workspace atomic replace failed");
        internal_error("Workspace file could not be replaced")
    })
}

async fn write_new_file(path: &Path, bytes: &[u8]) -> Result<(), ApiError> {
    use tokio::io::AsyncWriteExt;

    let mut file = tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .await
        .map_err(|_| internal_error("Attachment file could not be created"))?;
    file.write_all(bytes)
        .await
        .map_err(|_| internal_error("Attachment file could not be written"))?;
    file.flush()
        .await
        .map_err(|_| internal_error("Attachment file could not be flushed"))
}

async fn collect_uploads(
    mut multipart: Multipart,
    expected_field: &str,
) -> Result<Vec<PendingUpload>, ApiError> {
    let mut uploads = Vec::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| bad_request("multipart upload is invalid"))?
    {
        if field.name() != Some(expected_field) {
            continue;
        }
        let name = field
            .file_name()
            .ok_or_else(|| bad_request("uploaded file name is required"))?
            .to_owned();
        parse_direct_file_name(&name)?;
        let bytes = field
            .bytes()
            .await
            .map_err(|_| bad_request("uploaded file could not be read"))?;
        if bytes.len() > MAX_UPLOAD_FILE_BYTES {
            return Err(payload_too_large("uploaded file exceeds the 32 MiB limit"));
        }
        uploads.push(PendingUpload {
            name,
            bytes: bytes.to_vec(),
        });
    }
    Ok(uploads)
}

fn unique_upload_path(directory: &Path, name: &str) -> Result<PathBuf, ApiError> {
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("file");
    let extension = path.extension().and_then(|value| value.to_str());
    for index in 1..=10_000 {
        let candidate_name = extension.map_or_else(
            || format!("{stem} ({index})"),
            |extension| format!("{stem} ({index}).{extension}"),
        );
        let candidate = directory.join(candidate_name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(internal_error(
        "a unique upload file name could not be allocated",
    ))
}

async fn stream_file(path: PathBuf) -> Result<Response, ApiError> {
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|_| not_found("file was not found"))?;
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|_| not_found("file could not be opened"))?;
    let content_type = mime_guess::from_path(&path).first_or_octet_stream();
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, content_type.as_ref())
        .header(CONTENT_LENGTH, metadata.len())
        .header(ETAG, metadata_etag(&metadata))
        .body(Body::from_stream(ReaderStream::new(file)))
        .map_err(|_| internal_error("file response could not be created"))
}

fn preview_kind(path: &Path) -> &'static str {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    if mime.type_() == mime_guess::mime::IMAGE {
        return "image";
    }
    if mime == mime_guess::mime::APPLICATION_PDF {
        return "pdf";
    }
    if mime == mime_guess::mime::TEXT_CSV {
        return "csv";
    }
    if mime.type_() == mime_guess::mime::TEXT || is_known_text_file(path) {
        return "text";
    }
    "binary"
}

fn is_known_text_file(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "c" | "cc"
            | "cpp"
            | "css"
            | "go"
            | "h"
            | "hpp"
            | "html"
            | "htm"
            | "java"
            | "js"
            | "json"
            | "jsx"
            | "md"
            | "py"
            | "rs"
            | "sh"
            | "toml"
            | "ts"
            | "tsx"
            | "txt"
            | "xml"
            | "yaml"
            | "yml"
    )
}

fn modified_at(metadata: &std::fs::Metadata) -> String {
    let Ok(modified) = metadata.modified() else {
        return String::new();
    };
    timestamp_at(modified)
}

fn timestamp_at(value: std::time::SystemTime) -> String {
    let Ok(duration) = value.duration_since(UNIX_EPOCH) else {
        return String::new();
    };
    let Ok(seconds) = i64::try_from(duration.as_secs()) else {
        return String::new();
    };
    let Some(value) = DateTime::from_timestamp(seconds, duration.subsec_nanos()) else {
        return String::new();
    };
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn metadata_etag(metadata: &std::fs::Metadata) -> String {
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok());
    let seconds = modified.as_ref().map_or(0, std::time::Duration::as_secs);
    let nanos = modified.map_or(0, |value| value.subsec_nanos());
    format!("\"m-{}-{seconds}-{nanos}\"", metadata.len())
}

fn content_etag(bytes: &[u8]) -> String {
    format!("\"sha256-{:x}\"", Sha256::digest(bytes))
}

fn relative_display(root: &Path, path: &Path) -> Result<String, ApiError> {
    path.strip_prefix(root)
        .map_err(|_| bad_request("Workspace path resolves outside the Workspace"))
        .map(|relative| {
            relative
                .components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
        })
}

fn bad_request(message: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, Json(json!({"detail": message})))
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

fn internal_error(message: &str) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"detail": message})),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_absolute_parent_and_control_paths() {
        assert!(parse_relative_path("../secret").is_err());
        assert!(parse_relative_path("safe/../../secret").is_err());
        assert!(parse_relative_path("bad\0name").is_err());
        let absolute = std::env::temp_dir().join("qwenpaw-absolute");
        assert!(parse_relative_path(&absolute.to_string_lossy()).is_err());
        assert_eq!(
            parse_relative_path("safe/nested.txt").expect("relative path should parse"),
            PathBuf::from("safe").join("nested.txt")
        );
    }

    #[test]
    fn rejects_an_existing_symlink_that_escapes_the_workspace() {
        let root = tempfile::tempdir().expect("temporary Workspace should be created");
        let outside = tempfile::tempdir().expect("outside directory should be created");
        let outside_file = outside.path().join("secret.txt");
        std::fs::write(&outside_file, "secret").expect("outside file should be written");
        let link = root.path().join("escape.txt");
        if !create_file_symlink(&outside_file, &link) {
            return;
        }
        let root = root
            .path()
            .canonicalize()
            .expect("Workspace should resolve");
        assert!(resolve_existing_file(&root, Path::new("escape.txt")).is_err());
    }

    #[tokio::test]
    async fn refuses_to_create_attachment_directories_through_a_symlink() {
        let root = tempfile::tempdir().expect("temporary Workspace should be created");
        let outside = tempfile::tempdir().expect("outside directory should be created");
        let link = root.path().join(".qwenpaw");
        if !create_directory_symlink(outside.path(), &link) {
            return;
        }
        let root = root
            .path()
            .canonicalize()
            .expect("Workspace should resolve");
        assert!(ensure_workspace_attachment_directory(&root).await.is_err());
        assert!(!outside.path().join("attachments").exists());
    }

    #[cfg(unix)]
    fn create_file_symlink(source: &Path, target: &Path) -> bool {
        std::os::unix::fs::symlink(source, target).is_ok()
    }

    #[cfg(windows)]
    fn create_file_symlink(source: &Path, target: &Path) -> bool {
        std::os::windows::fs::symlink_file(source, target).is_ok()
    }

    #[cfg(unix)]
    fn create_directory_symlink(source: &Path, target: &Path) -> bool {
        std::os::unix::fs::symlink(source, target).is_ok()
    }

    #[cfg(windows)]
    fn create_directory_symlink(source: &Path, target: &Path) -> bool {
        std::os::windows::fs::symlink_dir(source, target).is_ok()
    }
}
