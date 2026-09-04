//! Unchanged Console compatibility routes for Rust built-in tools.

use std::collections::BTreeMap;

use axum::Json;
use axum::Router;
use axum::extract::Path;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::routing::patch;
use qwenpaw_core::BuiltinToolStatus;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;

use super::AppServer;

type ApiError = (StatusCode, Json<Value>);

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route("/api/tools", get(list_tools))
        .route("/api/tools/{tool_name}/toggle", patch(toggle_tool))
        .route(
            "/api/tools/{tool_name}/async-execution",
            patch(update_async_execution),
        )
        .route(
            "/api/tools/{tool_name}/config",
            get(get_tool_config).post(update_tool_config),
        )
}

async fn list_tools(State(server): State<AppServer>) -> Result<Json<Vec<Value>>, ApiError> {
    let tools = server.inner.core.builtin_tools().map_err(internal)?;
    Ok(Json(tools.iter().map(tool_value).collect()))
}

async fn toggle_tool(
    State(server): State<AppServer>,
    Path(tool_name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    require_tool(&server, &tool_name)?;
    let tool = server
        .inner
        .core
        .toggle_builtin_tool(&tool_name)
        .map_err(internal)?;
    Ok(Json(tool_value(&tool)))
}

#[derive(Debug, Deserialize)]
struct AsyncExecutionRequest {
    async_execution: bool,
}

async fn update_async_execution(
    State(server): State<AppServer>,
    Path(tool_name): Path<String>,
    Json(request): Json<AsyncExecutionRequest>,
) -> Result<Json<Value>, ApiError> {
    let tool = require_tool(&server, &tool_name)?;
    if request.async_execution {
        return Err(conflict(&format!(
            "Tool '{tool_name}' does not support asynchronous execution in Rust Core"
        )));
    }
    Ok(Json(tool_value(&tool)))
}

async fn get_tool_config(
    State(server): State<AppServer>,
    Path(tool_name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    require_tool(&server, &tool_name)?;
    Ok(Json(json!({})))
}

#[derive(Debug, Deserialize)]
struct ToolConfigRequest {
    config: BTreeMap<String, Value>,
}

async fn update_tool_config(
    State(server): State<AppServer>,
    Path(tool_name): Path<String>,
    Json(request): Json<ToolConfigRequest>,
) -> Result<Json<Value>, ApiError> {
    require_tool(&server, &tool_name)?;
    let detail = if request.config.is_empty() {
        format!("Tool '{tool_name}' has no configurable fields")
    } else {
        format!("Tool '{tool_name}' does not accept configuration")
    };
    Err(conflict(&detail))
}

fn require_tool(server: &AppServer, tool_name: &str) -> Result<BuiltinToolStatus, ApiError> {
    server
        .inner
        .core
        .builtin_tools()
        .map_err(internal)?
        .into_iter()
        .find(|tool| tool.name == tool_name)
        .ok_or_else(|| not_found(&format!("Tool '{tool_name}' not found")))
}

fn tool_value(tool: &BuiltinToolStatus) -> Value {
    json!({
        "name": tool.name,
        "enabled": tool.enabled,
        "description": tool.description,
        "async_execution": false,
        "icon": "",
        "requires_config": false,
        "config_fields": null,
        "config_values": null
    })
}

fn not_found(detail: &str) -> ApiError {
    (StatusCode::NOT_FOUND, Json(json!({"detail": detail})))
}

fn conflict(detail: &str) -> ApiError {
    (StatusCode::CONFLICT, Json(json!({"detail": detail})))
}

fn internal(error: impl std::fmt::Display) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"detail": error.to_string()})),
    )
}
