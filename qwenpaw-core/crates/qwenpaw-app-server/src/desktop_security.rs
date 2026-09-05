//! Stateful compatibility API for the unchanged Console Security page.

use axum::Json;
use axum::Router;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::routing::post;
use chrono::Utc;
use qwenpaw_core::SecuritySettings;
use qwenpaw_core::SkillScannerConfig;
use qwenpaw_core::SkillScannerWhitelistEntry;
use qwenpaw_core::ToolGuardConfig;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;

use super::AppServer;

type ApiError = (StatusCode, Json<Value>);

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route(
            "/api/config/security/tool-guard",
            get(get_tool_guard).put(put_tool_guard),
        )
        .route(
            "/api/config/security/tool-guard/builtin-rules",
            get(get_builtin_rules),
        )
        .route(
            "/api/config/security/sandbox",
            get(get_sandbox).put(put_sandbox),
        )
        .route(
            "/api/config/security/sandbox/deny-paths-protection",
            get(get_deny_paths).put(put_deny_paths),
        )
        .route(
            "/api/config/security/file-guard",
            get(get_file_guard).put(put_file_guard),
        )
        .route(
            "/api/config/security/skill-scanner",
            get(get_skill_scanner).put(put_skill_scanner),
        )
        .route(
            "/api/config/security/skill-scanner/blocked-history",
            get(get_blocked_history).delete(clear_blocked_history),
        )
        .route(
            "/api/config/security/skill-scanner/blocked-history/{index}",
            axum::routing::delete(remove_blocked_entry),
        )
        .route(
            "/api/config/security/skill-scanner/whitelist",
            post(add_to_whitelist),
        )
        .route(
            "/api/config/security/skill-scanner/whitelist/{skill_name}",
            axum::routing::delete(remove_from_whitelist),
        )
        .route(
            "/api/config/security/allow-no-auth-hosts",
            get(get_allow_no_auth_hosts).put(put_allow_no_auth_hosts),
        )
}

async fn get_tool_guard(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let settings = settings(&server)?;
    json_response(&settings.tool_guard)
}

async fn put_tool_guard(
    State(server): State<AppServer>,
    Json(tool_guard): Json<ToolGuardConfig>,
) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_security_lock.lock().await;
    let mut settings = settings(&server)?;
    settings.tool_guard = tool_guard;
    let settings = replace(&server, settings)?;
    json_response(&settings.tool_guard)
}

async fn get_builtin_rules() -> Result<Json<Value>, ApiError> {
    let rules = qwenpaw_core::builtin_tool_guard_rules().map_err(|error| internal(&error))?;
    json_response(&rules)
}

#[derive(Debug, Default, Deserialize)]
struct SandboxQuery {
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct EnabledBody {
    enabled: bool,
}

async fn get_sandbox(
    State(server): State<AppServer>,
    Query(query): Query<SandboxQuery>,
) -> Result<Json<Value>, ApiError> {
    sandbox_value(&server, query.enabled)
}

async fn put_sandbox(
    State(server): State<AppServer>,
    Json(body): Json<EnabledBody>,
) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_security_lock.lock().await;
    let mut settings = settings(&server)?;
    settings.sandbox_enabled = body.enabled;
    replace(&server, settings)?;
    sandbox_value(&server, Some(body.enabled))
}

fn sandbox_value(server: &AppServer, proposed: Option<bool>) -> Result<Json<Value>, ApiError> {
    let (enabled, effective, reason) = server
        .inner
        .core
        .sandbox_status(proposed)
        .map_err(core_error)?;
    Ok(Json(json!({
        "enabled": enabled,
        "effective": effective,
        "reason": reason
    })))
}

async fn get_deny_paths() -> Json<Value> {
    deny_paths_value()
}

async fn put_deny_paths(Json(_body): Json<EnabledBody>) -> Json<Value> {
    deny_paths_value()
}

fn deny_paths_value() -> Json<Value> {
    Json(json!({
        "active": false,
        "protected_paths": [],
        "failed_paths": [],
        "platform_supported": false,
        "message": "Deny paths protection via ACLs is not available in this Rust build."
    }))
}

async fn get_file_guard(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    json_response(&settings(&server)?.file_guard)
}

#[derive(Debug, Default, Deserialize)]
struct FileGuardUpdate {
    enabled: Option<bool>,
    paths: Option<Vec<String>>,
    allow_preview_outside_workspace: Option<bool>,
}

async fn put_file_guard(
    State(server): State<AppServer>,
    Json(body): Json<FileGuardUpdate>,
) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_security_lock.lock().await;
    let mut settings = settings(&server)?;
    if let Some(value) = body.enabled {
        settings.file_guard.enabled = value;
    }
    if let Some(value) = body.paths {
        settings.file_guard.paths = value;
    }
    if let Some(value) = body.allow_preview_outside_workspace {
        settings.file_guard.allow_preview_outside_workspace = value;
    }
    let settings = replace(&server, settings)?;
    json_response(&settings.file_guard)
}

async fn get_skill_scanner(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    json_response(&settings(&server)?.skill_scanner)
}

async fn put_skill_scanner(
    State(server): State<AppServer>,
    Json(skill_scanner): Json<SkillScannerConfig>,
) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_security_lock.lock().await;
    let mut settings = settings(&server)?;
    settings.skill_scanner = skill_scanner;
    let settings = replace(&server, settings)?;
    json_response(&settings.skill_scanner)
}

async fn get_blocked_history(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    json_response(&settings(&server)?.blocked_skill_history)
}

async fn clear_blocked_history(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_security_lock.lock().await;
    let mut settings = settings(&server)?;
    settings.blocked_skill_history.clear();
    replace(&server, settings)?;
    Ok(Json(json!({"cleared": true})))
}

async fn remove_blocked_entry(
    State(server): State<AppServer>,
    Path(index): Path<usize>,
) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_security_lock.lock().await;
    let mut settings = settings(&server)?;
    if index >= settings.blocked_skill_history.len() {
        return Err(not_found("Entry not found"));
    }
    settings.blocked_skill_history.remove(index);
    replace(&server, settings)?;
    Ok(Json(json!({"removed": true})))
}

#[derive(Debug, Deserialize)]
struct WhitelistAddRequest {
    skill_name: String,
    #[serde(default)]
    content_hash: String,
}

async fn add_to_whitelist(
    State(server): State<AppServer>,
    Json(body): Json<WhitelistAddRequest>,
) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_security_lock.lock().await;
    let skill_name = body.skill_name.trim();
    if skill_name.is_empty() {
        return Err(bad_request("skill_name is required"));
    }
    let mut settings = settings(&server)?;
    if settings
        .skill_scanner
        .whitelist
        .iter()
        .any(|entry| entry.skill_name == skill_name)
    {
        return Err(conflict(&format!(
            "Skill '{skill_name}' is already whitelisted"
        )));
    }
    settings
        .skill_scanner
        .whitelist
        .push(SkillScannerWhitelistEntry {
            skill_name: skill_name.to_owned(),
            content_hash: body.content_hash,
            added_at: Utc::now().to_rfc3339(),
        });
    replace(&server, settings)?;
    Ok(Json(json!({
        "whitelisted": true,
        "skill_name": skill_name
    })))
}

async fn remove_from_whitelist(
    State(server): State<AppServer>,
    Path(skill_name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_security_lock.lock().await;
    let mut settings = settings(&server)?;
    let original_len = settings.skill_scanner.whitelist.len();
    settings
        .skill_scanner
        .whitelist
        .retain(|entry| entry.skill_name != skill_name);
    if settings.skill_scanner.whitelist.len() == original_len {
        return Err(not_found(&format!(
            "Skill '{skill_name}' not found in whitelist"
        )));
    }
    replace(&server, settings)?;
    Ok(Json(json!({"removed": true, "skill_name": skill_name})))
}

async fn get_allow_no_auth_hosts(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        json!({"hosts": settings(&server)?.allow_no_auth_hosts}),
    ))
}

#[derive(Debug, Deserialize)]
struct AllowNoAuthHostsUpdate {
    hosts: Vec<String>,
}

async fn put_allow_no_auth_hosts(
    State(server): State<AppServer>,
    Json(body): Json<AllowNoAuthHostsUpdate>,
) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_security_lock.lock().await;
    let mut settings = settings(&server)?;
    settings.allow_no_auth_hosts =
        qwenpaw_core::normalize_ip_hosts(&body.hosts).map_err(|error| bad_request(&error))?;
    let settings = replace(&server, settings)?;
    Ok(Json(json!({"hosts": settings.allow_no_auth_hosts})))
}

fn settings(server: &AppServer) -> Result<SecuritySettings, ApiError> {
    server.inner.core.security_settings().map_err(core_error)
}

fn replace(server: &AppServer, settings: SecuritySettings) -> Result<SecuritySettings, ApiError> {
    server
        .inner
        .core
        .replace_security_settings(settings)
        .map_err(core_error)
}

fn json_response(value: &impl serde::Serialize) -> Result<Json<Value>, ApiError> {
    serde_json::to_value(value)
        .map(Json)
        .map_err(|error| internal(&error.to_string()))
}

fn core_error(error: qwenpaw_core::CoreError) -> ApiError {
    match error {
        qwenpaw_core::CoreError::Config(message) => bad_request(&message),
        other => internal(&other.to_string()),
    }
}

fn bad_request(detail: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, Json(json!({"detail": detail})))
}

fn not_found(detail: &str) -> ApiError {
    (StatusCode::NOT_FOUND, Json(json!({"detail": detail})))
}

fn conflict(detail: &str) -> ApiError {
    (StatusCode::CONFLICT, Json(json!({"detail": detail})))
}

fn internal(detail: &str) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"detail": detail})),
    )
}
