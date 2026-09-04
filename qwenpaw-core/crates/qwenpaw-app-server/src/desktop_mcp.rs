use std::collections::HashMap;

use anyhow::Context as _;
use axum::Json;
use axum::Router;
use axum::extract::Path;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::routing::patch;
use qwenpaw_core::Core;
use qwenpaw_core::McpAccessPolicy;
use qwenpaw_core::McpClientSettings;
use qwenpaw_core::McpOAuthSettings;
use qwenpaw_protocol::ThreadListParams;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use tracing::warn;

use super::AppServer;
use super::DesktopCredentialStore;

const DATA_VERSION: u32 = 1;
const MASKED_VALUE: &str = "********";
const RESERVED_PREFIXES: [&str; 5] = [
    "access-principals/",
    "tools/",
    "toggle/",
    "oauth/",
    "policy/",
];

type ApiError = (StatusCode, Json<Value>);

#[derive(Debug, Serialize, Deserialize)]
struct StoredMcpData {
    version: u32,
    clients: Vec<McpClientSettings>,
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct StoredMcpSecrets {
    headers: HashMap<String, String>,
    env: HashMap<String, String>,
    oauth_access_token: String,
    oauth_refresh_token: String,
}

#[derive(Debug, Deserialize)]
struct CreateRequest {
    client_key: String,
    client: CreateClient,
}

#[derive(Debug, Deserialize)]
struct CreateClient {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default = "default_transport")]
    transport: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    headers: HashMap<String, String>,
    #[serde(default)]
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    cwd: String,
}

#[derive(Debug, Default, Deserialize)]
struct UpdateRequest {
    name: Option<String>,
    description: Option<String>,
    enabled: Option<bool>,
    transport: Option<String>,
    url: Option<String>,
    headers: Option<HashMap<String, String>>,
    command: Option<String>,
    args: Option<Vec<String>>,
    env: Option<HashMap<String, String>>,
    cwd: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ToolWhitelistRequest {
    tools: Option<Vec<String>>,
}

pub(super) fn initialize(
    core: &Core,
    credentials: &dyn DesktopCredentialStore,
) -> anyhow::Result<()> {
    let Some(serialized) = core.read_mcp_data().map_err(anyhow::Error::msg)? else {
        return Ok(());
    };
    let stored: StoredMcpData =
        serde_json::from_str(&serialized).context("stored Desktop MCP configuration is invalid")?;
    anyhow::ensure!(
        stored.version == DATA_VERSION,
        "stored Desktop MCP configuration version is unsupported"
    );
    let clients = stored
        .clients
        .into_iter()
        .map(|client| restore_client_secrets(client, credentials))
        .collect::<anyhow::Result<Vec<_>>>()?;
    core.replace_mcp_client_settings(clients)
        .map_err(anyhow::Error::msg)
        .context("stored Desktop MCP configuration could not be activated")
}

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route("/api/mcp", get(list_clients).post(create_client))
        .route("/api/mcp/access-principals", get(list_access_principals))
        .route(
            "/api/mcp/tools/{*client_key}",
            get(list_tools).put(update_tool_whitelist),
        )
        .route(
            "/api/mcp/policy/{*client_key}",
            get(get_policy).put(update_policy),
        )
        .route("/api/mcp/toggle/{*client_key}", patch(toggle_client))
        .route(
            "/api/mcp/{*client_key}",
            get(get_client).put(update_client).delete(delete_client),
        )
}

async fn list_clients(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    Ok(Json(Value::Array(client_values(&server).await?)))
}

async fn get_client(
    State(server): State<AppServer>,
    Path(client_key): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(client_value(&server, &client_key).await?))
}

async fn create_client(
    State(server): State<AppServer>,
    Json(request): Json<CreateRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let _guard = server.inner.desktop_mcp_lock.lock().await;
    validate_client_key(&request.client_key)?;
    let mut clients = server.inner.core.mcp_client_settings();
    if clients
        .iter()
        .any(|client| client.key == request.client_key)
    {
        return Err(bad_request(&format!(
            "MCP client '{}' already exists. Use PUT to update.",
            request.client_key
        )));
    }
    ensure_unique_name(&clients, &request.client.name, &request.client_key)?;
    ensure_unmasked(&request.client.headers)?;
    ensure_unmasked(&request.client.env)?;
    let oauth = is_remote(&request.client.transport).then(McpOAuthSettings::default);
    clients.push(McpClientSettings {
        key: request.client_key.clone(),
        name: request.client.name,
        description: request.client.description,
        enabled: request.client.enabled,
        transport: request.client.transport,
        url: request.client.url,
        headers: request.client.headers,
        command: request.client.command,
        args: request.client.args,
        env: request.client.env,
        cwd: request.client.cwd,
        tools: None,
        oauth,
        access: McpAccessPolicy::default(),
    });
    persist_clients(&server, clients)?;
    Ok((
        StatusCode::CREATED,
        Json(client_value(&server, &request.client_key).await?),
    ))
}

async fn update_client(
    State(server): State<AppServer>,
    Path(client_key): Path<String>,
    Json(request): Json<UpdateRequest>,
) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_mcp_lock.lock().await;
    let mut clients = server.inner.core.mcp_client_settings();
    let index = client_index(&clients, &client_key)?;
    if let Some(name) = &request.name {
        ensure_unique_name(&clients, name, &client_key)?;
    }
    let client = &mut clients[index];
    if let Some(value) = request.name {
        client.name = value;
    }
    if let Some(value) = request.description {
        client.description = value;
    }
    if let Some(value) = request.enabled {
        client.enabled = value;
    }
    if let Some(value) = request.transport {
        client.transport = value;
    }
    if let Some(value) = request.url {
        client.url = value;
    }
    if let Some(value) = request.headers {
        client.headers = merge_masked(value, &client.headers)?;
    }
    if let Some(value) = request.command {
        client.command = value;
    }
    if let Some(value) = request.args {
        client.args = value;
    }
    if let Some(value) = request.env {
        client.env = merge_masked(value, &client.env)?;
    }
    if let Some(value) = request.cwd {
        client.cwd = value;
    }
    if is_remote(&client.transport) {
        client.oauth.get_or_insert_with(McpOAuthSettings::default);
    } else {
        client.oauth = None;
    }
    persist_clients(&server, clients)?;
    Ok(Json(client_value(&server, &client_key).await?))
}

async fn toggle_client(
    State(server): State<AppServer>,
    Path(client_key): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_mcp_lock.lock().await;
    let mut clients = server.inner.core.mcp_client_settings();
    let index = client_index(&clients, &client_key)?;
    clients[index].enabled = !clients[index].enabled;
    persist_clients(&server, clients)?;
    Ok(Json(client_value(&server, &client_key).await?))
}

async fn delete_client(
    State(server): State<AppServer>,
    Path(client_key): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_mcp_lock.lock().await;
    let mut clients = server.inner.core.mcp_client_settings();
    let index = client_index(&clients, &client_key)?;
    let _ = server.inner.core.revoke_mcp_oauth(&client_key).await;
    clients.remove(index);
    persist_clients(&server, clients)?;
    if let Some(credentials) = &server.inner.desktop_credentials
        && let Err(error) = credentials.save_mcp_client_secrets(&client_key, None)
    {
        warn!(%error, %client_key, "failed to delete obsolete MCP credentials");
    }
    Ok(Json(json!({
        "message": format!("MCP client '{client_key}' deleted successfully")
    })))
}

async fn list_tools(
    State(server): State<AppServer>,
    Path(client_key): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !server
        .inner
        .core
        .mcp_client_settings()
        .iter()
        .any(|client| client.key == client_key)
    {
        return Err(not_found(&client_key));
    }
    let tools = server
        .inner
        .core
        .list_mcp_tools(&client_key)
        .await
        .map_err(|error| bad_gateway(&error.to_string()))?;
    Ok(Json(Value::Array(
        tools
            .into_iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "enabled": tool.enabled,
                    "input_schema": tool.input_schema
                })
            })
            .collect(),
    )))
}

async fn update_tool_whitelist(
    State(server): State<AppServer>,
    Path(client_key): Path<String>,
    Json(request): Json<ToolWhitelistRequest>,
) -> Result<Json<Value>, ApiError> {
    let guard = server.inner.desktop_mcp_lock.lock().await;
    let mut clients = server.inner.core.mcp_client_settings();
    let index = client_index(&clients, &client_key)?;
    clients[index].tools = request.tools;
    persist_clients(&server, clients)?;
    drop(guard);
    match list_tools(State(server), Path(client_key)).await {
        Ok(response) => Ok(response),
        Err(_) => Ok(Json(Value::Array(Vec::new()))),
    }
}

async fn get_policy(
    State(server): State<AppServer>,
    Path(client_key): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let clients = server.inner.core.mcp_client_settings();
    let index = client_index(&clients, &client_key)?;
    Ok(Json(
        serde_json::to_value(&clients[index].access)
            .map_err(|error| internal_json_error(&error))?,
    ))
}

async fn update_policy(
    State(server): State<AppServer>,
    Path(client_key): Path<String>,
    Json(policy): Json<McpAccessPolicy>,
) -> Result<Json<Value>, ApiError> {
    let guard = server.inner.desktop_mcp_lock.lock().await;
    let mut clients = server.inner.core.mcp_client_settings();
    let index = client_index(&clients, &client_key)?;
    clients[index].access = policy;
    persist_clients(&server, clients)?;
    drop(guard);
    get_policy(State(server), Path(client_key)).await
}

async fn list_access_principals(State(server): State<AppServer>) -> Json<Value> {
    let aliases = server.inner.desktop_session_aliases.read().await;
    let threads = server
        .inner
        .core
        .list_threads(ThreadListParams {
            limit: Some(100),
            include_archived: true,
            ..ThreadListParams::default()
        })
        .await;
    Json(Value::Array(
        threads
            .data
            .into_iter()
            .map(|thread| {
                let session_id = aliases
                    .thread_to_client
                    .get(&thread.id)
                    .cloned()
                    .unwrap_or_else(|| thread.id.clone());
                json!({
                    "source_type": "channel",
                    "source_value": "console",
                    "subject_type": "user",
                    "subject_value": thread.id,
                    "label": format!("console / {session_id}"),
                    "chat_id": thread.id,
                    "chat_name": "",
                    "session_id": session_id,
                    "updated_at": chrono::DateTime::from_timestamp(thread.updated_at, 0)
                        .map(|value| value.to_rfc3339())
                })
            })
            .collect(),
    ))
}

async fn client_values(server: &AppServer) -> Result<Vec<Value>, ApiError> {
    let settings = server.inner.core.mcp_client_settings();
    let infos = server
        .inner
        .core
        .list_mcp_clients()
        .await
        .map_err(|error| internal_error(&error.to_string()))?;
    Ok(infos
        .into_iter()
        .map(|client| {
            let access = settings
                .iter()
                .find(|setting| setting.key == client.key)
                .map(|setting| &setting.access)
                .cloned()
                .unwrap_or_default();
            let oauth_status = client.oauth_status.map(|status| {
                json!({
                    "authorized": status.authorized,
                    "expires_at": status.expires_at,
                    "scope": status.scope,
                    "client_id": status.client_id
                })
            });
            json!({
                "key": client.key,
                "name": client.name,
                "description": client.description,
                "enabled": client.enabled,
                "transport": client.transport,
                "url": client.url,
                "headers": client.headers,
                "command": client.command,
                "args": client.args,
                "env": client.env,
                "cwd": client.cwd,
                "tools": client.tools,
                "oauth_status": oauth_status,
                "access_summary": {
                    "default_effect": access.default_effect,
                    "overrides_count": access.client_overrides.len()
                        + access.tool_defaults.len()
                        + access.tool_overrides.len()
                }
            })
        })
        .collect())
}

async fn client_value(server: &AppServer, client_key: &str) -> Result<Value, ApiError> {
    client_values(server)
        .await?
        .into_iter()
        .find(|client| client.get("key").and_then(Value::as_str) == Some(client_key))
        .ok_or_else(|| not_found(client_key))
}

fn persist_clients(server: &AppServer, clients: Vec<McpClientSettings>) -> Result<(), ApiError> {
    server
        .inner
        .core
        .validate_mcp_client_settings(clients.clone())
        .map_err(|error| bad_request(&error.to_string()))?;
    let credentials = server
        .inner
        .desktop_credentials
        .as_deref()
        .ok_or_else(|| internal_error("Desktop MCP credential storage is unavailable"))?;
    let previous = server.inner.core.mcp_client_settings();
    replace_credentials(credentials, &previous, &clients).map_err(|error| {
        warn!(%error, "failed to replace Desktop MCP credentials");
        internal_error("Desktop MCP credentials could not be saved")
    })?;
    let stored = StoredMcpData {
        version: DATA_VERSION,
        clients: clients.iter().cloned().map(without_secrets).collect(),
    };
    let serialized = serde_json::to_string(&stored).map_err(|error| internal_json_error(&error))?;
    if let Err(error) = server.inner.core.write_mcp_data(&serialized) {
        let _ = replace_credentials(credentials, &clients, &previous);
        return Err(internal_error(&error.to_string()));
    }
    server
        .inner
        .core
        .replace_mcp_client_settings(clients)
        .map_err(|error| internal_error(&error.to_string()))
}

fn replace_credentials(
    credentials: &dyn DesktopCredentialStore,
    previous: &[McpClientSettings],
    next: &[McpClientSettings],
) -> anyhow::Result<()> {
    let mut keys = previous
        .iter()
        .chain(next)
        .map(|client| client.key.clone())
        .collect::<Vec<_>>();
    keys.sort();
    keys.dedup();
    let mut changed = Vec::<String>::new();
    for key in keys {
        let old = previous.iter().find(|client| client.key == key);
        let new = next.iter().find(|client| client.key == key);
        if old.map(client_secrets) == new.map(client_secrets) {
            continue;
        }
        let value = new
            .map(client_secrets)
            .map(|secrets| serde_json::to_string(&secrets))
            .transpose()?;
        if let Err(error) = credentials.save_mcp_client_secrets(&key, value.as_deref()) {
            for changed_key in changed.into_iter().rev() {
                let old_value = previous
                    .iter()
                    .find(|client| client.key == changed_key)
                    .map(client_secrets)
                    .map(|secrets| serde_json::to_string(&secrets))
                    .transpose()?;
                let _ = credentials.save_mcp_client_secrets(&changed_key, old_value.as_deref());
            }
            return Err(error);
        }
        changed.push(key);
    }
    Ok(())
}

fn restore_client_secrets(
    mut client: McpClientSettings,
    credentials: &dyn DesktopCredentialStore,
) -> anyhow::Result<McpClientSettings> {
    let secrets = credentials
        .load_mcp_client_secrets(&client.key)?
        .map(|value| serde_json::from_str::<StoredMcpSecrets>(&value))
        .transpose()
        .context("stored Desktop MCP credential is invalid")?
        .unwrap_or_default();
    client.headers = secrets.headers;
    client.env = secrets.env;
    if let Some(oauth) = &mut client.oauth {
        oauth.access_token = secrets.oauth_access_token;
        oauth.refresh_token = secrets.oauth_refresh_token;
    }
    Ok(client)
}

fn without_secrets(mut client: McpClientSettings) -> McpClientSettings {
    client.headers.clear();
    client.env.clear();
    if let Some(oauth) = &mut client.oauth {
        oauth.access_token.clear();
        oauth.refresh_token.clear();
    }
    client
}

fn client_secrets(client: &McpClientSettings) -> StoredMcpSecrets {
    StoredMcpSecrets {
        headers: client.headers.clone(),
        env: client.env.clone(),
        oauth_access_token: client
            .oauth
            .as_ref()
            .map(|oauth| oauth.access_token.clone())
            .unwrap_or_default(),
        oauth_refresh_token: client
            .oauth
            .as_ref()
            .map(|oauth| oauth.refresh_token.clone())
            .unwrap_or_default(),
    }
}

fn client_index(clients: &[McpClientSettings], key: &str) -> Result<usize, ApiError> {
    clients
        .iter()
        .position(|client| client.key == key)
        .ok_or_else(|| not_found(key))
}

fn ensure_unique_name(
    clients: &[McpClientSettings],
    name: &str,
    client_key: &str,
) -> Result<(), ApiError> {
    let desired = name.trim().to_lowercase();
    if clients.iter().any(|client| {
        client.key != client_key
            && (client.key.trim().to_lowercase() == desired
                || client.name.trim().to_lowercase() == desired)
    }) {
        return Err(bad_request(&format!(
            "MCP client name '{name}' already exists"
        )));
    }
    Ok(())
}

fn validate_client_key(key: &str) -> Result<(), ApiError> {
    let trimmed = key.trim();
    if trimmed.is_empty() || trimmed != key || key.contains('/') {
        return Err(bad_request("MCP client key is invalid"));
    }
    let lower = key.to_lowercase();
    if RESERVED_PREFIXES
        .iter()
        .any(|prefix| lower == prefix.trim_end_matches('/') || lower.starts_with(prefix))
    {
        return Err(bad_request("MCP client key uses a reserved route prefix"));
    }
    Ok(())
}

fn merge_masked(
    next: HashMap<String, String>,
    previous: &HashMap<String, String>,
) -> Result<HashMap<String, String>, ApiError> {
    next.into_iter()
        .map(|(key, value)| {
            if value == MASKED_VALUE {
                previous
                    .get(&key)
                    .cloned()
                    .map(|value| (key.clone(), value))
                    .ok_or_else(|| bad_request(&format!("masked MCP secret '{key}' is unknown")))
            } else {
                Ok((key, value))
            }
        })
        .collect()
}

fn ensure_unmasked(values: &HashMap<String, String>) -> Result<(), ApiError> {
    if values.values().any(|value| value == MASKED_VALUE) {
        return Err(bad_request("a new MCP secret cannot use the masked value"));
    }
    Ok(())
}

fn is_remote(transport: &str) -> bool {
    matches!(transport, "streamable_http" | "sse" | "http")
}

fn default_true() -> bool {
    true
}

fn default_transport() -> String {
    String::from("stdio")
}

fn not_found(client_key: &str) -> ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(json!({"detail": format!("MCP client '{client_key}' not found")})),
    )
}

fn bad_request(detail: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, Json(json!({"detail": detail})))
}

fn bad_gateway(detail: &str) -> ApiError {
    (StatusCode::BAD_GATEWAY, Json(json!({"detail": detail})))
}

fn internal_error(detail: &str) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"detail": detail})),
    )
}

fn internal_json_error(error: &serde_json::Error) -> ApiError {
    internal_error(&error.to_string())
}
