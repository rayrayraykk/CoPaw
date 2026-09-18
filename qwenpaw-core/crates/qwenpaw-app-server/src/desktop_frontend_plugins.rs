//! Installed frontend bundles consumed by the unchanged Console loader.

use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use axum::extract::{Path as AxumPath, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Value, json};
use tower_http::services::ServeFile;
use tower_http::set_header::SetResponseHeaderLayer;

use super::AppServer;
use super::desktop_files::ApiError;
use super::desktop_pawapps::plugins_directory;

const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route("/api/plugins", get(list))
        .route("/api/plugins/{plugin_id}/status", get(status))
        .route("/api/plugins/{plugin_id}/files/{*file_path}", get(asset))
        .route("/api/frontend_plugin", get(list))
        .route(
            "/api/frontend_plugin/{plugin_id}/files/{*file_path}",
            get(asset),
        )
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-cache, no-store, must-revalidate"),
        ))
}

async fn list(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    tokio::task::spawn_blocking(move || {
        let Some(root) = plugins_directory(&server)? else {
            return Ok(Json(json!([])));
        };
        let mut entries = fs::read_dir(root)
            .map_err(|_| internal())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| internal())?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        let mut plugins = Vec::new();
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.')
                || name.ends_with(".disabled")
                || !entry.file_type().is_ok_and(|kind| kind.is_dir())
            {
                continue;
            }
            let Some(manifest) = read_manifest(&entry.path()) else {
                continue;
            };
            let id = manifest.get("id").cloned().unwrap_or_else(|| json!(name));
            plugins.push(json!({
                "id":id, "name":manifest.get("name").unwrap_or(&id),
                "version":manifest.get("version").unwrap_or(&json!("0.0.0")),
                "description":manifest.get("description").unwrap_or(&json!("")),
                "author":manifest.get("author").unwrap_or(&json!("")),
                "enabled":true, "loaded":false,
                "plugin_type":plugin_type(&manifest),
                "frontend_entry":manifest["entry"]["frontend"]
            }));
        }
        Ok(Json(json!(plugins)))
    })
    .await
    .map_err(|_| internal())?
}

fn read_manifest(directory: &Path) -> Option<Value> {
    let directory = directory.canonicalize().ok()?;
    let path = directory.join("plugin.json").canonicalize().ok()?;
    if !path.starts_with(&directory) || !path.is_file() {
        return None;
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return None;
    }
    let manifest: Value = serde_json::from_slice(&bytes).ok()?;
    for key in ["id", "version"] {
        if manifest[key].as_str().is_none_or(str::is_empty) {
            return None;
        }
    }
    if !manifest["entry"]["frontend"].is_null() && !manifest["entry"]["frontend"].is_string() {
        return None;
    }
    manifest.is_object().then_some(manifest)
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64() != Some(0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}

fn plugin_type(manifest: &Value) -> &str {
    if let Some(
        kind @ ("tool" | "provider" | "hook" | "command" | "channel" | "frontend" | "app"
        | "general"),
    ) = manifest["type"].as_str()
    {
        return kind;
    }
    let meta = &manifest["meta"];
    for (keys, kind) in [
        (&["tools", "tool_name"][..], "tool"),
        (&["chat_model", "provider_id"][..], "provider"),
        (&["hook_type"][..], "hook"),
        (&["command_name", "commands"][..], "command"),
        (&["channel"][..], "channel"),
    ] {
        if keys.iter().any(|key| truthy(&meta[key])) {
            return kind;
        }
    }
    if truthy(&manifest["entry"]["frontend"]) {
        "frontend"
    } else {
        "general"
    }
}

async fn asset(
    State(server): State<AppServer>,
    AxumPath((id, file_path)): AxumPath<(String, String)>,
    mut request: Request,
) -> Result<Response, ApiError> {
    let path = tokio::task::spawn_blocking(move || resolve_asset(&server, &id, &file_path))
        .await
        .map_err(|_| internal())??;
    let mime = match path.extension().and_then(|value| value.to_str()) {
        Some("js" | "mjs") => Some("application/javascript"),
        Some("css") => Some("text/css; charset=utf-8"),
        _ => None,
    };
    let cache = if hashed_asset(&path) {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    // The original FileResponse serves GET bodies even on revalidation;
    // do not reuse a stale entry bundle based on coarse Last-Modified dates.
    request.headers_mut().remove(header::IF_MODIFIED_SINCE);
    let mut response = ServeFile::new(&path)
        .try_call(request)
        .await
        .map(IntoResponse::into_response)
        .map_err(|_| internal())?;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    if let Some(mime) = mime {
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    }
    Ok(response)
}

async fn status(
    State(server): State<AppServer>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    tokio::task::spawn_blocking(move || {
        let missing = || error(StatusCode::NOT_FOUND, &format!("Plugin '{id}' not found."));
        let directory = resolve_directory(&server, &id).map_err(|error| {
            if error.0 == StatusCode::NOT_FOUND {
                missing()
            } else {
                error
            }
        })?;
        let manifest = directory
            .join("plugin.json")
            .canonicalize()
            .map_err(|_| missing())?;
        if !manifest.starts_with(&directory) {
            return Err(error(StatusCode::FORBIDDEN, "Access denied"));
        }
        if !manifest.is_file() {
            return Err(missing());
        }
        Ok(Json(json!({"id":id,"loaded":false,"enabled":false})))
    })
    .await
    .map_err(|_| internal())?
}

fn resolve_directory(server: &AppServer, id: &str) -> Result<PathBuf, ApiError> {
    let denied = || error(StatusCode::FORBIDDEN, "Access denied");
    if id.is_empty()
        || matches!(id, "." | "..")
        || id.contains(['/', '\\', ':'])
        || id.chars().any(char::is_control)
    {
        return Err(denied());
    }
    let missing_plugin = || error(StatusCode::NOT_FOUND, &format!("Plugin '{id}' not found"));
    let root = plugins_directory(server)?.ok_or_else(missing_plugin)?;
    let directory = root.join(id);
    if !fs::symlink_metadata(&directory).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink()) {
        return Err(missing_plugin());
    }
    let directory = directory.canonicalize().map_err(|_| missing_plugin())?;
    if directory.parent() != Some(root.as_path()) {
        return Err(denied());
    }
    Ok(directory)
}

fn resolve_asset(server: &AppServer, id: &str, file_path: &str) -> Result<PathBuf, ApiError> {
    let denied = || error(StatusCode::FORBIDDEN, "Access denied");
    if file_path.contains(['\\', ':']) {
        return Err(denied());
    }
    let directory = resolve_directory(server, id)?;
    if read_manifest(&directory).is_none() {
        return Err(error(
            StatusCode::NOT_FOUND,
            &format!("Plugin '{id}' not found"),
        ));
    }
    let mut relative = PathBuf::new();
    for component in Path::new(file_path).components() {
        match component {
            Component::Normal(name) => relative.push(name),
            Component::CurDir => (),
            Component::ParentDir if relative.pop() => (),
            _ => return Err(denied()),
        }
    }
    let missing = || {
        error(
            StatusCode::NOT_FOUND,
            &format!("File not found: {file_path}"),
        )
    };
    let path = directory
        .join(relative)
        .canonicalize()
        .map_err(|_| missing())?;
    if !path.starts_with(directory) {
        return Err(denied());
    }
    if !path.is_file() {
        return Err(missing());
    }
    Ok(path)
}

fn hashed_asset(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    let Some((stem, extension)) = name.rsplit_once('.') else {
        return false;
    };
    let Some((_, hash)) = stem.rsplit_once('-') else {
        return false;
    };
    !extension.is_empty()
        && extension
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && hash.len() >= 8
        && hash.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

fn error(status: StatusCode, detail: &str) -> ApiError {
    (status, Json(json!({"detail":detail})))
}

fn internal() -> ApiError {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Plugin files could not be read",
    )
}
