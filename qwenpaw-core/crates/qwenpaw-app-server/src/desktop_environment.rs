use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Context as _;
use axum::Json;
use axum::Router;
use axum::extract::Path;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::delete;
use axum::routing::get;
use qwenpaw_core::Core;
use serde_json::Value;
use serde_json::json;
use tracing::warn;

use super::AppServer;
use super::DesktopCredentialStore;

type ApiError = (StatusCode, Json<Value>);

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route("/api/envs", get(list_environment).put(save_environment))
        .route("/api/envs/{key}", delete(delete_environment_value))
}

pub(super) fn initialize(
    core: &Core,
    credentials: &dyn DesktopCredentialStore,
) -> anyhow::Result<()> {
    let environment = load_environment(core, credentials)?;
    core.replace_runtime_environment(environment)
        .map_err(anyhow::Error::msg)
        .context("Desktop environment configuration is invalid")
}

async fn list_environment(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_environment_lock.lock().await;
    let environment = load_server_environment(&server)?;
    Ok(Json(environment_response(&environment)))
}

async fn save_environment(
    State(server): State<AppServer>,
    Json(environment): Json<BTreeMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_environment_lock.lock().await;
    replace_environment(&server, &environment).map(Json)
}

async fn delete_environment_value(
    State(server): State<AppServer>,
    Path(key): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_environment_lock.lock().await;
    let mut environment = load_server_environment(&server)?;
    if environment.remove(&key).is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({"detail": format!("Env var '{key}' not found")})),
        ));
    }
    replace_environment(&server, &environment).map(Json)
}

fn replace_environment(
    server: &AppServer,
    environment: &BTreeMap<String, String>,
) -> Result<Value, ApiError> {
    let credentials =
        server.inner.desktop_credentials.as_deref().ok_or_else(|| {
            internal_error("Desktop environment credential storage is unavailable")
        })?;
    let previous = load_environment(&server.inner.core, credentials).map_err(|error| {
        warn!(%error, "failed to load Desktop environment credentials");
        internal_error("Desktop environment credentials could not be loaded")
    })?;
    server
        .inner
        .core
        .replace_runtime_environment(environment.clone())
        .map_err(|error| bad_request(&error.to_string()))?;
    if let Err(error) = replace_credentials(credentials, &previous, environment) {
        let _ = server
            .inner
            .core
            .replace_runtime_environment(previous.clone());
        warn!(%error, "failed to replace Desktop environment credentials");
        return Err(internal_error(
            "Desktop environment credentials could not be saved",
        ));
    }
    let keys = environment.keys().cloned().collect::<Vec<_>>();
    if let Err(error) = server.inner.core.write_environment_keys(&keys) {
        let rollback = replace_credentials(credentials, environment, &previous);
        let _ = server
            .inner
            .core
            .replace_runtime_environment(previous.clone());
        if let Err(rollback_error) = rollback {
            warn!(%rollback_error, "failed to roll back Desktop environment credentials");
        }
        warn!(%error, "failed to persist Desktop environment names");
        return Err(internal_error(
            "Desktop environment configuration could not be saved",
        ));
    }
    Ok(environment_response(environment))
}

fn load_server_environment(server: &AppServer) -> Result<BTreeMap<String, String>, ApiError> {
    let credentials =
        server.inner.desktop_credentials.as_deref().ok_or_else(|| {
            internal_error("Desktop environment credential storage is unavailable")
        })?;
    load_environment(&server.inner.core, credentials).map_err(|error| {
        warn!(%error, "failed to load Desktop environment credentials");
        internal_error("Desktop environment credentials could not be loaded")
    })
}

fn load_environment(
    core: &Core,
    credentials: &dyn DesktopCredentialStore,
) -> anyhow::Result<BTreeMap<String, String>> {
    let mut environment = BTreeMap::new();
    for key in core
        .read_environment_keys()
        .map_err(anyhow::Error::msg)
        .context("failed to read Desktop environment names")?
    {
        if let Some(value) = credentials.load_environment_value(&key)? {
            environment.insert(key, value);
        }
    }
    Ok(environment)
}

fn replace_credentials(
    credentials: &dyn DesktopCredentialStore,
    previous: &BTreeMap<String, String>,
    next: &BTreeMap<String, String>,
) -> anyhow::Result<()> {
    let keys = previous
        .keys()
        .chain(next.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut changed = Vec::<String>::new();
    for key in keys {
        if previous.get(&key) == next.get(&key) {
            continue;
        }
        if let Err(error) =
            credentials.save_environment_value(&key, next.get(&key).map(String::as_str))
        {
            for changed_key in changed.into_iter().rev() {
                if let Err(rollback_error) = credentials.save_environment_value(
                    &changed_key,
                    previous.get(changed_key.as_str()).map(String::as_str),
                ) {
                    warn!(%rollback_error, "failed to roll back one environment credential");
                }
            }
            return Err(error);
        }
        changed.push(key);
    }
    Ok(())
}

fn environment_response(environment: &BTreeMap<String, String>) -> Value {
    Value::Array(
        environment
            .iter()
            .map(|(key, value)| json!({"key": key, "value": value}))
            .collect(),
    )
}

fn bad_request(detail: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, Json(json!({"detail": detail})))
}

fn internal_error(detail: &str) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"detail": detail})),
    )
}
