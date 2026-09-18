//! Directory-backed `PawApp` contracts for the unchanged Console.

use std::fs;
use std::path::{Component, Path, PathBuf};

use axum::extract::{Path as AxumPath, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Value, json};
use tower_http::services::ServeFile;

use super::AppServer;
use super::desktop_files::ApiError;

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route("/api/pawapps", get(list))
        .route("/api/pawapps/{app_id}", get(detail).delete(uninstall))
        .route("/api/pawapps/{app_id}/settings", get(settings))
        .route("/api/pawapps/{app_id}/static/{*file_path}", get(asset))
}

async fn list(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let apps = scan(server).await?;
    Ok(Json(json!({"total": apps.len(), "apps": apps})))
}

async fn detail(
    State(server): State<AppServer>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    find(server, &id).await.map(Json)
}

async fn settings(
    State(server): State<AppServer>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    let info = find(server, &id).await?;
    Ok(Json(json!({"app_id": id, "settings": info["settings"]})))
}

async fn find(server: AppServer, id: &str) -> Result<Value, ApiError> {
    scan(server)
        .await?
        .into_iter()
        .find(|app| app["id"].as_str() == Some(id))
        .ok_or_else(|| missing_app(id))
}

async fn scan(server: AppServer) -> Result<Vec<Value>, ApiError> {
    tokio::task::spawn_blocking(move || {
        let Some(root) = plugins_directory(&server)? else {
            return Ok(Vec::new());
        };
        let mut apps = Vec::new();
        let entries = fs::read_dir(&root).map_err(|_| internal("PawApps could not be read"))?;
        for entry in entries {
            let entry = entry.map_err(|_| internal("PawApps could not be read"))?;
            // Do not follow app or manifest links outside this installation.
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let path = entry.path().join("plugin.json");
            let Ok(resolved) = path.canonicalize() else {
                continue;
            };
            if !resolved.starts_with(entry.path()) || !resolved.is_file() {
                continue;
            }
            let Some(manifest) = fs::read(&resolved)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            else {
                continue;
            };
            let Some(pawapp) = manifest["meta"]["pawapp"].as_object() else {
                continue;
            };
            if pawapp.is_empty() {
                continue;
            }
            apps.push(app_info(&manifest, &entry.file_name().to_string_lossy()));
        }
        Ok(apps)
    })
    .await
    .map_err(|_| internal("PawApps scan failed"))?
}

fn app_info(manifest: &Value, fallback_id: &str) -> Value {
    let field = |key: &str, fallback: Value| manifest.get(key).cloned().unwrap_or(fallback);
    let meta = &manifest["meta"];
    let pawapp = &meta["pawapp"];
    let app_field =
        |key: &str, fallback: &str| pawapp.get(key).cloned().unwrap_or_else(|| json!(fallback));
    json!({
        "id": field("id", json!(fallback_id)),
        "name": field("name", json!(fallback_id)),
        "version": field("version", json!("0.0.0")),
        "description": field("description", json!("")),
        "description_i18n": manifest.get("description_i18n")
            .filter(|value| !value.is_null()).cloned().unwrap_or_else(|| json!({})),
        "author": field("author", json!("")),
        "category": app_field("category", ""),
        "icon": app_field("icon", ""),
        "icon_url": app_field("icon_url", ""),
        "entry_page": app_field("entry_page", ""),
        "launch_scope": app_field("launch_scope", "page"),
        "status": "installed",
        "settings": meta.get("settings").cloned().unwrap_or_else(|| json!([]))
    })
}

async fn uninstall(
    State(server): State<AppServer>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    tokio::task::spawn_blocking(move || {
        let directory = app_directory(&server, &id)?;
        fs::remove_dir_all(directory).map_err(|_| internal("Uninstall failed"))?;
        Ok(Json(
            json!({"id": id, "message": format!("PawApp '{id}' uninstalled.")}),
        ))
    })
    .await
    .map_err(|_| internal("Uninstall failed"))?
}

async fn asset(
    State(server): State<AppServer>,
    AxumPath((id, file_path)): AxumPath<(String, String)>,
    request: Request,
) -> Result<Response, ApiError> {
    let path = tokio::task::spawn_blocking(move || {
        let directory = app_directory(&server, &id)?;
        let denied = || error(StatusCode::FORBIDDEN, "Access denied");
        // Reject Windows separators and device/drive paths on every host.
        if file_path.contains(['\\', ':']) {
            return Err(denied());
        }
        let mut relative = PathBuf::new();
        for component in Path::new(&file_path).components() {
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
        let resolved = directory
            .join(relative)
            .canonicalize()
            .map_err(|_| missing())?;
        if !resolved.starts_with(&directory) {
            return Err(denied());
        }
        if !resolved.is_file() {
            return Err(missing());
        }
        Ok(resolved)
    })
    .await
    .map_err(|_| internal("PawApp file lookup failed"))??;
    ServeFile::new(path)
        .try_call(request)
        .await
        .map(IntoResponse::into_response)
        .map_err(|_| internal("PawApp file could not be read"))
}

pub(super) fn plugins_directory(server: &AppServer) -> Result<Option<PathBuf>, ApiError> {
    let workspace = server
        .inner
        .desktop_workspace
        .as_ref()
        .ok_or_else(|| internal("Desktop Workspace is unavailable"))?;
    let root = workspace.data_dir.join("plugins");
    let metadata = match fs::symlink_metadata(&root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(internal("PawApps could not be read")),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(error(StatusCode::BAD_REQUEST, "Invalid plugins path"));
    }
    Ok(Some(root))
}

fn app_directory(server: &AppServer, id: &str) -> Result<PathBuf, ApiError> {
    if id.is_empty()
        || matches!(id, "." | "..")
        || id.contains(['/', '\\', ':'])
        || id.chars().any(char::is_control)
    {
        return Err(error(StatusCode::BAD_REQUEST, "Invalid app id"));
    }
    let root = plugins_directory(server)?.ok_or_else(|| missing_app(id))?;
    let directory = root.join(id);
    let metadata = fs::symlink_metadata(&directory).map_err(|_| missing_app(id))?;
    let invalid = || error(StatusCode::BAD_REQUEST, "Invalid app path");
    if metadata.file_type().is_symlink() {
        return Err(invalid());
    }
    if !metadata.is_dir() {
        return Err(missing_app(id));
    }
    let resolved = directory.canonicalize().map_err(|_| invalid())?;
    if resolved.parent() != Some(root.as_path()) {
        return Err(invalid());
    }
    Ok(resolved)
}

fn missing_app(id: &str) -> ApiError {
    error(StatusCode::NOT_FOUND, &format!("PawApp '{id}' not found"))
}

fn internal(message: &str) -> ApiError {
    error(StatusCode::INTERNAL_SERVER_ERROR, message)
}

fn error(status: StatusCode, message: &str) -> ApiError {
    (status, Json(json!({"detail": message})))
}
