//! Persistent multi-Agent catalog for the unchanged Console.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context as _;
use axum::Json;
use axum::Router;
use axum::extract::Path as AxumPath;
use axum::extract::Query;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::routing::get;
use axum::routing::patch;
use axum::routing::post;
use axum::routing::put;
use qwenpaw_core::Core;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Map;
use serde_json::Value;
use serde_json::json;
use uuid::Uuid;

use super::AppServer;
use super::DesktopWorkspace;

const CATALOG_SCHEMA_VERSION: u32 = 3;
const DEFAULT_AGENT_ID: &str = "default";
const AGENT_HEADER: &str = "x-agent-id";
const MAX_CATALOG_BYTES: u64 = 2 * 1024 * 1024;
pub(super) const MAX_AGENT_CONFIG_BYTES: usize = 512 * 1024;
const MAX_AGENT_NAME_BYTES: usize = 256;
const MAX_AGENT_DESCRIPTION_BYTES: usize = 16 * 1024;
const MAX_AGENTS: usize = 256;
const MAX_COPY_FILES: usize = 1_024;
const MAX_COPY_BYTES: u64 = 64 * 1024 * 1024;
const COPYABLE_MD_FILES: [&str; 5] = [
    "AGENTS.md",
    "SOUL.md",
    "PROFILE.md",
    "HEARTBEAT.md",
    "BOOTSTRAP.md",
];

type ApiError = (StatusCode, Json<Value>);

#[path = "desktop_agent_restore.rs"]
pub(super) mod restore;

#[path = "desktop_workspace_identity.rs"]
pub(super) mod identity;
use identity::WorkspaceDataKey;

#[path = "desktop_agent_publication.rs"]
mod publication;
pub(super) use publication::recover_before_initialization;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentCatalog {
    schema_version: u32,
    revision: u64,
    order: Vec<String>,
    agents: BTreeMap<String, AgentReference>,
    #[serde(default)]
    workspace_keys: BTreeMap<String, WorkspaceDataKey>,
    #[serde(skip)]
    bootstrap_identity: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentReference {
    workspace_dir: String,
    enabled: bool,
    pinned: bool,
    config: Value,
    #[serde(default)]
    data_key: Option<WorkspaceDataKey>,
}

/// One validated registration snapshot, not a lease against external file changes.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct AgentContext {
    pub agent_id: String,
    pub data_key: WorkspaceDataKey,
    pub workspace: PathBuf,
    pub config: Value,
}

impl AgentContext {
    pub(super) fn usage_owner(&self) -> qwenpaw_core::UsageOwner {
        qwenpaw_core::UsageOwner {
            agent_id: self.agent_id.clone(),
            data_key: self.data_key.clone(),
        }
    }

    pub(super) fn model(&self) -> Option<String> {
        self.config
            .pointer("/active_model/model")
            .and_then(Value::as_str)
            .or_else(|| {
                self.config
                    .pointer("/backend_settings/model")
                    .and_then(Value::as_str)
            })
            .map(str::to_owned)
    }

    pub(super) fn project(&self) -> Result<PathBuf, ApiError> {
        match self.config.get("project_dir").and_then(Value::as_str) {
            Some(path) => canonical_registered_workspace(path),
            None => Ok(self.workspace.clone()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AgentBackupSnapshot {
    pub version: u32,
    pub agents: Vec<BackupAgent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct BackupAgent {
    pub id: String,
    pub workspace_dir: String,
    pub enabled: bool,
    pub pinned: bool,
    pub config: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_key: Option<WorkspaceDataKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AgentRegistryBackup {
    pub version: u32,
    pub agents: Vec<BackupAgentReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct BackupAgentReference {
    pub id: String,
    pub workspace_dir: String,
    pub enabled: bool,
    pub pinned: bool,
}

impl AgentBackupSnapshot {
    pub(super) fn registry(&self) -> AgentRegistryBackup {
        AgentRegistryBackup {
            version: 1,
            agents: self.agents.iter().map(BackupAgent::reference).collect(),
        }
    }
}

impl BackupAgent {
    pub(super) fn reference(&self) -> BackupAgentReference {
        BackupAgentReference {
            id: self.id.clone(),
            workspace_dir: self.workspace_dir.clone(),
            enabled: self.enabled,
            pinned: self.pinned,
        }
    }
}

#[derive(Debug, Deserialize)]
struct CreateAgentRequest {
    #[serde(default)]
    id: Option<String>,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    workspace_dir: Option<String>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    skill_names: Option<Vec<String>>,
    #[serde(default)]
    active_model: Option<Value>,
    #[serde(default = "default_backend")]
    backend: String,
    #[serde(default)]
    backend_settings: Map<String, Value>,
    #[serde(default)]
    mail: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
struct CopyAgentRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default = "enabled")]
    copy_agent_json: bool,
    #[serde(default = "enabled")]
    copy_md_files: bool,
    #[serde(default)]
    copy_skills: bool,
    #[serde(default)]
    copy_jobs: bool,
}

#[derive(Debug, Deserialize)]
struct ReorderRequest {
    agent_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct EnabledRequest {
    enabled: bool,
}

#[derive(Debug, Deserialize)]
struct PinnedRequest {
    pinned: bool,
}

#[derive(Debug, Deserialize)]
struct MemoryScopeQuery {
    #[serde(default = "default_memory_scope")]
    scope: String,
}

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route("/api/agent/", get(agent_root))
        .route("/api/agent/health", get(agent_health))
        .route("/api/agent/admin/status", get(agent_process_status))
        .route("/api/agent/shutdown", post(agent_shutdown))
        .route("/api/agent/admin/shutdown", post(agent_shutdown))
        .route("/api/agents", get(list_agents).post(create_agent))
        .route("/api/agents/order", put(reorder_agents))
        .route("/api/agents/{agent_id}/copy", post(copy_agent))
        .route(
            "/api/agents/{agent_id}/model-settings",
            patch(update_model_settings),
        )
        .route(
            "/api/agents/{agent_id}/backend-settings",
            patch(update_backend_settings),
        )
        .route(
            "/api/agents/{agent_id}/memory/reindex",
            post(rebuild_memory_index),
        )
        .route(
            "/api/agents/{agent_id}/memory/reindex/undo",
            post(undo_memory_reindex),
        )
        .route(
            "/api/agents/{agent_id}/memory/runtime-status",
            get(memory_runtime_status),
        )
        .route("/api/agents/{agent_id}/memory/status", get(memory_status))
        .route("/api/agents/{agent_id}/memory/graph", get(memory_graph))
        .route("/api/agents/{agent_id}/toggle", patch(toggle_agent))
        .route("/api/agents/{agent_id}/pin", patch(pin_agent))
        .route(
            "/api/agents/{agent_id}",
            get(get_agent).put(update_agent).delete(delete_agent),
        )
}

async fn agent_root() -> Json<Value> {
    Json(json!({
        "name": "QwenPaw Rust Core",
        "status": "running",
        "backend": "rust-core"
    }))
}

async fn agent_health() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

async fn agent_process_status() -> Json<Value> {
    Json(json!({
        "status": "running",
        "pid": std::process::id(),
        "backend": "rust-core"
    }))
}

async fn agent_shutdown(State(server): State<AppServer>) -> Json<Value> {
    server.inner.shutdown.cancel();
    Json(json!({"success": true}))
}

/// Resolve template-write authority without treating project selection as a base.
pub(super) fn initialization_root(workspace: &DesktopWorkspace) -> anyhow::Result<Option<PathBuf>> {
    let catalog = match read_catalog(workspace) {
        Ok(catalog) => catalog,
        Err((StatusCode::NOT_FOUND, _)) => return Ok(Some(workspace.initial.clone())),
        Err(error) => return Err(api_error_message(error)),
    };
    if let Ok(context) = context_from_catalog(&catalog, DEFAULT_AGENT_ID, false) {
        Ok(Some(context.workspace))
    } else {
        // Keep the existing request-time rejection boundary. Do not repair
        // an invalid root or stop unrelated Agents merely to copy templates.
        tracing::warn!("Skipping templates for an unavailable default Workspace binding");
        Ok(None)
    }
}

pub(super) fn initialize(
    core: &Core,
    workspace: &DesktopWorkspace,
    default_workspace: &Path,
) -> anyhow::Result<()> {
    fs::create_dir_all(catalog_directory(workspace)).with_context(|| {
        format!(
            "failed to create Rust Agent catalog directory {}",
            catalog_directory(workspace).display()
        )
    })?;
    let catalog = match read_catalog(workspace) {
        Ok(catalog) => {
            validate_catalog(&catalog, workspace)
                .map_err(api_error_message)
                .context("Rust Agent catalog is invalid")?;
            if catalog.bootstrap_identity {
                write_catalog(workspace, &catalog)
                    .map_err(api_error_message)
                    .context("failed to establish Workspace identities")?;
            }
            catalog
        }
        Err((StatusCode::NOT_FOUND, _)) => {
            let project = workspace
                .selected
                .try_read()
                .context("initial project selection is unavailable")?;
            let catalog = default_catalog(core, default_workspace, &project);
            write_catalog(workspace, &catalog)
                .map_err(api_error_message)
                .context("failed to initialize Rust Agent catalog")?;
            catalog
        }
        Err(error) => {
            return Err(api_error_message(error)).context("Rust Agent catalog could not open");
        }
    };
    validate_catalog(&catalog, workspace)
        .map_err(api_error_message)
        .context("Rust Agent catalog is invalid")?;
    let default = catalog
        .agents
        .get(DEFAULT_AGENT_ID)
        .context("Default Rust Agent is missing")?;
    if let Some(running) = default.config.get("running")
        && let Ok(runtime) = super::desktop_agent_settings::runtime_config(running)
    {
        core.replace_agent_runtime_config(runtime)
            .map_err(anyhow::Error::msg)
            .context("failed to apply default Rust Agent runtime config")?;
    }
    Ok(())
}

pub(super) fn requested_agent_id(headers: &HeaderMap) -> Result<String, ApiError> {
    let Some(value) = headers.get(AGENT_HEADER) else {
        return Ok(String::from(DEFAULT_AGENT_ID));
    };
    let value = value
        .to_str()
        .map_err(|_| bad_request("X-Agent-Id is invalid"))?
        .trim();
    if value.is_empty() {
        return Ok(String::from(DEFAULT_AGENT_ID));
    }
    validate_agent_id(value, true)?;
    Ok(value.to_owned())
}

pub(super) async fn workspace_for_request(
    server: &AppServer,
    headers: &HeaderMap,
) -> Result<PathBuf, ApiError> {
    let agent_id = requested_agent_id(headers)?;
    workspace_for_agent(server, &agent_id).await
}

pub(super) async fn workspace_for_agent(
    server: &AppServer,
    agent_id: &str,
) -> Result<PathBuf, ApiError> {
    Ok(context_for_agent(server, agent_id).await?.workspace)
}

pub(super) async fn context_for_agent(
    server: &AppServer,
    agent_id: &str,
) -> Result<AgentContext, ApiError> {
    validate_agent_id(agent_id, true)?;
    let _guard = server.agent_catalog_guard().await?;
    let workspace = desktop_workspace(server)?;
    let catalog = read_catalog(workspace)?;
    resolved_context(server, &catalog, agent_id, true)
}

pub(super) async fn registered_data_key(
    server: &AppServer,
    agent_id: &str,
) -> Result<Option<WorkspaceDataKey>, ApiError> {
    let _guard = server.agent_catalog_guard().await?;
    let catalog = read_catalog(desktop_workspace(server)?)?;
    Ok(catalog
        .agents
        .get(agent_id)
        .and_then(|agent| agent.data_key.clone()))
}

fn context_from_catalog(
    catalog: &AgentCatalog,
    agent_id: &str,
    require_enabled: bool,
) -> Result<AgentContext, ApiError> {
    let agent = catalog
        .agents
        .get(agent_id)
        .ok_or_else(|| not_found(&format!("Agent '{agent_id}' not found")))?;
    if require_enabled && !agent.enabled {
        return Err(forbidden(&format!("Agent '{agent_id}' is disabled")));
    }
    let context = AgentContext {
        agent_id: agent_id.to_owned(),
        data_key: agent
            .data_key
            .clone()
            .ok_or_else(|| internal("Agent Workspace identity is missing"))?,
        workspace: canonical_registered_workspace(&agent.workspace_dir)?,
        config: agent.config.clone(),
    };
    if catalog
        .workspace_keys
        .get(context.workspace.to_string_lossy().as_ref())
        != Some(&context.data_key)
        || !identity::matches_marker(&context.workspace, &context.data_key)?
    {
        return Err(conflict(&format!(
            "Agent '{}' Workspace binding has changed",
            context.agent_id
        )));
    }
    Ok(context)
}

fn resolved_context(
    server: &AppServer,
    catalog: &AgentCatalog,
    agent_id: &str,
    require_enabled: bool,
) -> Result<AgentContext, ApiError> {
    let mut context = context_from_catalog(catalog, agent_id, require_enabled)?;
    context.config =
        super::desktop_channels::profile_view(server, &context.data_key, &context.config)?;
    Ok(context)
}

pub(super) async fn context_for_data_key(
    server: &AppServer,
    key: &WorkspaceDataKey,
) -> Result<AgentContext, ApiError> {
    let _guard = server.agent_catalog_guard().await?;
    let catalog = read_catalog(desktop_workspace(server)?)?;
    let id = catalog
        .agents
        .iter()
        .find_map(|(id, agent)| (agent.data_key.as_ref() == Some(key)).then_some(id.as_str()))
        .ok_or_else(|| not_found("Cron Workspace has no registered Agent"))?;
    resolved_context(server, &catalog, id, true)
}

/// Match the original mail-enabled aggregation and broadcast inventory.
pub(super) async fn mail_contexts(server: &AppServer) -> Result<Vec<AgentContext>, ApiError> {
    let _guard = server.agent_catalog_guard().await?;
    let catalog = read_catalog(desktop_workspace(server)?)?;
    catalog
        .order
        .iter()
        .filter(|id| {
            let agent = &catalog.agents[*id];
            agent.enabled
                && agent.config.pointer("/mail/push/access_control_enabled")
                    == Some(&Value::Bool(true))
                && agent
                    .config
                    .pointer("/mail/push/mode")
                    .and_then(Value::as_str)
                    .unwrap_or("off")
                    != "off"
        })
        .map(|id| resolved_context(server, &catalog, id, false))
        .collect()
}

pub(super) async fn mail_context(
    server: &AppServer,
    agent_id: &str,
) -> Result<Option<AgentContext>, ApiError> {
    let _guard = server.agent_catalog_guard().await?;
    let catalog = read_catalog(desktop_workspace(server)?)?;
    if !catalog.agents.contains_key(agent_id) {
        return Ok(None);
    }
    resolved_context(server, &catalog, agent_id, false).map(Some)
}

/// Match the original global trend's registered-directory inventory.
pub(super) async fn statistics_bindings(
    server: &AppServer,
) -> Result<BTreeMap<String, WorkspaceDataKey>, ApiError> {
    if server.inner.desktop_workspace.is_none() {
        return Ok(BTreeMap::from([(
            String::from("default"),
            default_data_key(server)?,
        )]));
    }
    let _guard = server.agent_catalog_guard().await?;
    let catalog = read_catalog(desktop_workspace(server)?)?;
    Ok(catalog
        .agents
        .keys()
        .filter_map(|id| {
            context_from_catalog(&catalog, id, false)
                .ok()
                .map(|context| (context.agent_id, context.data_key))
        })
        .collect())
}

// Cron callers hold the Cron lock; restore owns the Core operation barrier.
// The default registration cannot be deleted or rebound by ordinary requests.
pub(super) fn default_data_key(server: &AppServer) -> Result<WorkspaceDataKey, ApiError> {
    server.ensure_agent_publication_available()?;
    if server.inner.desktop_workspace.is_none() {
        return Ok(WorkspaceDataKey::LegacyAgent(String::from(
            DEFAULT_AGENT_ID,
        )));
    }
    let catalog = read_catalog(desktop_workspace(server)?)?;
    catalog
        .agents
        .get(DEFAULT_AGENT_ID)
        .and_then(|agent| agent.data_key.clone())
        .ok_or_else(|| internal("Default Workspace identity is missing"))
}

pub(super) async fn agent_workspaces(
    server: &AppServer,
) -> Result<Vec<(String, String, PathBuf)>, ApiError> {
    Ok(backup_agent_snapshot(server)
        .await?
        .agents
        .into_iter()
        .map(|agent| {
            let name = config_string(&agent.config, "name").unwrap_or_else(|| agent.id.clone());
            (agent.id, name, PathBuf::from(agent.workspace_dir))
        })
        .collect())
}

pub(super) async fn backup_agent_snapshot(
    server: &AppServer,
) -> Result<AgentBackupSnapshot, ApiError> {
    let _guard = server.agent_catalog_guard().await?;
    let catalog = read_catalog(desktop_workspace(server)?)?;
    let mut agents = Vec::with_capacity(catalog.order.len());
    for agent_id in &catalog.order {
        let Some(agent) = catalog.agents.get(agent_id) else {
            continue;
        };
        let context = context_from_catalog(&catalog, agent_id, false)?;
        let root = context.workspace.to_string_lossy().into_owned();
        let mut config = agent.config.clone();
        config["workspace_dir"] = json!(root);
        agents.push(BackupAgent {
            id: agent_id.clone(),
            workspace_dir: root,
            enabled: agent.enabled,
            pinned: agent.pinned,
            config,
            data_key: agent.data_key.clone(),
        });
    }
    Ok(AgentBackupSnapshot { version: 2, agents })
}

pub(super) fn validate_backup_snapshot(snapshot: &AgentBackupSnapshot) -> Result<(), String> {
    if !matches!(snapshot.version, 1 | 2) || snapshot.agents.len() > MAX_AGENTS {
        return Err(String::from("Backup Agent snapshot is invalid"));
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut keys = std::collections::BTreeSet::new();
    for agent in &snapshot.agents {
        match (&agent.data_key, snapshot.version) {
            (None, 1) => {}
            (Some(key), 2) if key.is_valid() && keys.insert(key) => {}
            _ => return Err(String::from("Backup Workspace identity is invalid")),
        }
        validate_agent_id(&agent.id, true)
            .map_err(|_| String::from("Backup Agent ID is invalid"))?;
        validate_config_bounds(&agent.config)
            .map_err(|_| String::from("Backup Agent config is invalid"))?;
        if !ids.insert(agent.id.to_lowercase())
            || agent.workspace_dir.is_empty()
            || agent.workspace_dir.len() > 4096
            || agent.workspace_dir.chars().any(char::is_control)
            || agent.config.get("id").and_then(Value::as_str) != Some(agent.id.as_str())
            || agent.config.get("workspace_dir").and_then(Value::as_str)
                != Some(agent.workspace_dir.as_str())
            || (agent.id == DEFAULT_AGENT_ID && (!agent.enabled || !agent.pinned))
        {
            return Err(String::from("Backup Agent snapshot entry is invalid"));
        }
    }
    Ok(())
}

pub(super) fn validate_registry_backup(registry: &AgentRegistryBackup) -> Result<(), String> {
    if registry.version != 1
        || registry.agents.len() > MAX_AGENTS
        || registry
            .agents
            .first()
            .is_none_or(|agent| agent.id != DEFAULT_AGENT_ID)
    {
        return Err(String::from("Backup global Agent registry is invalid"));
    }
    let mut ids = std::collections::BTreeSet::new();
    for agent in &registry.agents {
        validate_agent_id(&agent.id, true)
            .map_err(|_| String::from("Backup Agent ID is invalid"))?;
        if !ids.insert(agent.id.to_lowercase())
            || agent.workspace_dir.is_empty()
            || agent.workspace_dir.len() > 4096
            || agent.workspace_dir.chars().any(char::is_control)
            || (agent.id == DEFAULT_AGENT_ID && (!agent.enabled || !agent.pinned))
        {
            return Err(String::from("Backup global Agent reference is invalid"));
        }
    }
    Ok(())
}

pub(super) async fn config_for_agent(
    server: &AppServer,
    agent_id: &str,
) -> Result<Value, ApiError> {
    Ok(context_for_agent(server, agent_id).await?.config)
}

pub(super) async fn project_for_agent(
    server: &AppServer,
    agent_id: &str,
) -> Result<PathBuf, ApiError> {
    context_for_agent(server, agent_id).await?.project()
}

/// Publish a project only to the registration that admitted the operation.
pub(super) async fn set_project_for_context(
    server: &AppServer,
    expected: &AgentContext,
    project: Option<&Path>,
) -> Result<PathBuf, ApiError> {
    let _operation = server
        .inner
        .core
        .operation_guard()
        .map_err(|error| internal(&error.to_string()))?;
    let _guard = server.agent_catalog_guard().await?;
    let workspace = desktop_workspace(server)?;
    let mut catalog = read_catalog(workspace)?;
    let current = context_from_catalog(&catalog, &expected.agent_id, true)?;
    if current.data_key != expected.data_key || current.workspace != expected.workspace {
        return Err(conflict("Agent Workspace binding has changed"));
    }
    let selected = match project {
        Some(path) => canonical_registered_workspace(&path.to_string_lossy())?,
        None => current.workspace.clone(),
    };
    let previous = catalog.clone();
    let had_config = match fs::symlink_metadata(current.workspace.join("agent.json")) {
        Ok(metadata) if metadata.is_file() => true,
        Ok(_) => return Err(bad_request("Agent config is not a regular file")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => return Err(internal("Agent config could not be inspected")),
    };
    let mut next = current.config.as_object().cloned().unwrap_or_default();
    next.insert(
        "project_dir".into(),
        project.map_or(Value::Null, |_| json!(selected)),
    );
    let next = Value::Object(next);
    write_agent_config(&current.workspace, &next)?;
    catalog.agents.get_mut(&current.agent_id).unwrap().config = next;
    bump_catalog(&mut catalog);
    if let Err(error) = write_catalog(workspace, &catalog) {
        if restore_project_config(&current, had_config).is_err() {
            return Err(internal(
                "Project selection failed and Agent config rollback failed",
            ));
        }
        return Err(error);
    }
    if current.agent_id == DEFAULT_AGENT_ID {
        if let Err(error) = server.inner.core.write_preferred_workspace(&selected) {
            let config_restored = restore_project_config(&current, had_config);
            let catalog_restored = write_catalog(workspace, &previous);
            if config_restored.is_err() || catalog_restored.is_err() {
                return Err(internal(
                    "Project selection failed and registration rollback failed",
                ));
            }
            return Err(internal(&error.to_string()));
        }
        selected.clone_into(&mut *workspace.selected.write().await);
    }
    Ok(selected)
}

fn restore_project_config(previous: &AgentContext, had_config: bool) -> Result<(), ApiError> {
    if had_config {
        write_agent_config(&previous.workspace, &previous.config)
    } else {
        // Remove only the config file this failed publication just created.
        fs::remove_file(previous.workspace.join("agent.json"))
            .map_err(|_| internal("New Agent config could not be rolled back"))
    }
}

pub(super) async fn replace_config_field(
    server: &AppServer,
    agent_id: &str,
    field: &str,
    value: Value,
) -> Result<Value, ApiError> {
    validate_agent_id(agent_id, true)?;
    let _guard = server.agent_catalog_guard().await?;
    let workspace = desktop_workspace(server)?;
    let mut catalog = read_catalog(workspace)?;
    let context = context_from_catalog(&catalog, agent_id, true)?;
    let agent = catalog
        .agents
        .get_mut(agent_id)
        .expect("validated Agent exists");
    let mut next = agent.config.as_object().cloned().unwrap_or_default();
    next.insert(field.to_owned(), value);
    let next = Value::Object(next);
    validate_config_bounds(&next)?;
    let _channels_guard = server.inner.desktop_channel_config_lock.lock().await;
    let response = super::desktop_channels::profile_view(server, &context.data_key, &next)?;
    let agent_workspace = context.workspace;
    let previous = agent.config.clone();
    write_agent_config(&agent_workspace, &next)?;
    agent.config.clone_from(&next);
    bump_catalog(&mut catalog);
    if let Err(error) = write_catalog(workspace, &catalog) {
        let _ = write_agent_config(&agent_workspace, &previous);
        return Err(error);
    }
    Ok(response)
}

async fn list_agents(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let _guard = server.agent_catalog_guard().await?;
    let catalog = read_catalog(desktop_workspace(&server)?)?;
    let agents = catalog
        .order
        .iter()
        .filter_map(|agent_id| {
            catalog
                .agents
                .get(agent_id)
                .map(|agent| agent_summary(agent_id, agent))
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({"agents": agents})))
}

async fn get_agent(
    State(server): State<AppServer>,
    AxumPath(agent_id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    let _guard = server.agent_catalog_guard().await?;
    let catalog = read_catalog(desktop_workspace(&server)?)?;
    let agent = catalog
        .agents
        .get(&agent_id)
        .ok_or_else(|| not_found(&format!("Agent '{agent_id}' not found")))?;
    let key = agent
        .data_key
        .as_ref()
        .ok_or_else(|| internal("Agent Workspace identity is missing"))?;
    Ok(Json(public_config(&super::desktop_channels::profile_view(
        &server,
        key,
        &agent.config,
    )?)))
}

async fn create_agent(
    State(server): State<AppServer>,
    Json(body): Json<CreateAgentRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    validate_name(&body.name)?;
    validate_description(&body.description)?;
    validate_backend(&body.backend)?;
    let language = normalize_language(body.language.as_deref())?;
    let _lifecycle = server.inner.desktop_agent_lifecycle_lock.lock().await;
    let _guard = server.agent_catalog_guard().await?;
    let workspace = desktop_workspace(&server)?;
    let mut catalog = read_catalog(workspace)?;
    if catalog.agents.len() >= MAX_AGENTS {
        return Err(payload_too_large("Too many Agents are configured"));
    }
    let agent_id = match body
        .id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        Some(agent_id) => {
            validate_agent_id(agent_id, false)?;
            if catalog.agents.contains_key(agent_id) {
                return Err(bad_request(&format!(
                    "Agent ID '{agent_id}' already exists."
                )));
            }
            agent_id.to_owned()
        }
        None => generated_agent_id(&catalog),
    };
    let (agent_workspace, auto_workspace, data_key) = resolve_new_workspace(
        workspace,
        body.workspace_dir.as_deref(),
        &agent_id,
        &catalog,
    )?;
    validate_initial_skills(workspace, body.skill_names.as_deref().unwrap_or_default())?;
    initialize_workspace(&agent_workspace, &language)?;
    if let Err(error) = install_initial_skills(
        workspace,
        &agent_workspace,
        body.skill_names.as_deref().unwrap_or_default(),
    ) {
        cleanup_auto_workspace(&agent_workspace, auto_workspace);
        return Err(error);
    }
    let mut config = default_agent_config(
        &agent_id,
        &body.name,
        &body.description,
        &agent_workspace,
        &body.backend,
        &language,
        body.active_model.as_ref(),
    );
    config["backend_settings"] = Value::Object(body.backend_settings);
    if let Some(mail) = body.mail {
        config["mail"] = mail;
    }
    let secret = take_mail_secret(&mut config)?;
    validate_config_bounds(&config)?;
    if let Err(error) = save_agent_secret(&server, &agent_id, secret.as_deref()) {
        cleanup_auto_workspace(&agent_workspace, auto_workspace);
        return Err(error);
    }
    if let Err(error) = write_agent_config(&agent_workspace, &config) {
        let _ = save_agent_secret(&server, &agent_id, None);
        cleanup_auto_workspace(&agent_workspace, auto_workspace);
        return Err(error);
    }
    catalog.workspace_keys.insert(
        agent_workspace.to_string_lossy().into_owned(),
        data_key.clone(),
    );
    catalog.order.push(agent_id.clone());
    catalog.agents.insert(
        agent_id.clone(),
        AgentReference {
            workspace_dir: agent_workspace.to_string_lossy().into_owned(),
            enabled: true,
            pinned: false,
            config,
            data_key: Some(data_key),
        },
    );
    bump_catalog(&mut catalog);
    if let Err(error) = write_catalog(workspace, &catalog) {
        let _ = save_agent_secret(&server, &agent_id, None);
        cleanup_auto_workspace(&agent_workspace, auto_workspace);
        return Err(error);
    }
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": agent_id,
            "workspace_dir": agent_workspace.to_string_lossy(),
            "enabled": true,
            "pinned": false
        })),
    ))
}

async fn copy_agent(
    State(server): State<AppServer>,
    AxumPath(source_id): AxumPath<String>,
    Json(body): Json<CopyAgentRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if !body.copy_agent_json {
        return Err(bad_request("copy_agent_json must be true"));
    }
    let _lifecycle = server.inner.desktop_agent_lifecycle_lock.lock().await;
    // Scheduler paths resolve Agent configuration while holding the Cron lock.
    let _cron_guard = if body.copy_jobs {
        Some(server.inner.desktop_cron_lock.lock().await)
    } else {
        None
    };
    let _guard = server.agent_catalog_guard().await?;
    let workspace = desktop_workspace(&server)?;
    let mut catalog = read_catalog(workspace)?;
    let source = catalog
        .agents
        .get(&source_id)
        .cloned()
        .ok_or_else(|| not_found(&format!("Agent '{source_id}' not found")))?;
    let new_id = generated_agent_id(&catalog);
    let target = workspace.data_dir.join("workspaces").join(&new_id);
    let language = config_string(&source.config, "language").unwrap_or_else(|| String::from("en"));
    let source_context = context_from_catalog(&catalog, &source_id, false)?;
    let source_workspace = source_context.workspace;
    let name = copied_agent_name(body.name.as_deref(), &source.config, &source_id)?;
    let mut config = source.config;
    config["id"] = Value::String(new_id.clone());
    config["name"] = Value::String(name);
    config["workspace_dir"] = Value::String(target.to_string_lossy().into_owned());
    config["channels"] = json!({});
    let secret = load_agent_secret(&server, &source_id)?;
    let data_key = identity::bind_workspace(&mut catalog, &target, true)?;
    let jobs = if body.copy_jobs {
        super::desktop_cron::prepare_copy(&server, &source_context.data_key, &new_id, &data_key)?
    } else {
        None
    };
    catalog.order.push(new_id.clone());
    catalog.agents.insert(
        new_id.clone(),
        AgentReference {
            workspace_dir: target.to_string_lossy().into_owned(),
            enabled: true,
            pinned: false,
            config: config.clone(),
            data_key: Some(data_key.clone()),
        },
    );
    bump_catalog(&mut catalog);
    fs::create_dir_all(workspace.data_dir.join("workspaces"))
        .map_err(|_| internal("Agent Workspace parent could not be created"))?;
    // Never reuse or clean up an existing unregistered workspace.
    fs::create_dir(&target).map_err(|_| internal("Agent Workspace could not be created"))?;
    let mut recovery_required = false;
    let result = (|| {
        identity::write_marker(&target, &data_key)?;
        initialize_workspace_selective(
            &target,
            &language,
            body.copy_md_files,
            body.copy_skills,
            false,
        )?;
        copy_selected_workspace_files(&source_workspace, &target, &body)?;
        save_agent_secret(&server, &new_id, secret.as_deref())?;
        write_agent_config(&target, &config)?;
        if let Some(jobs) = &jobs {
            jobs.apply(&server)?;
        }
        if let Err(error) = write_catalog(workspace, &catalog) {
            if let Some(jobs) = &jobs
                && jobs.rollback(&server).is_err()
            {
                recovery_required = true;
                return Err(internal(
                    "Agent copy rollback failed; workspace and credentials retained for recovery",
                ));
            }
            return Err(error);
        }
        Ok(())
    })();
    if let Err(error) = result {
        if !recovery_required {
            cleanup_copied_workspace(&server, &new_id, &target)?;
        }
        return Err(error);
    }
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": new_id,
            "workspace_dir": target.to_string_lossy(),
            "enabled": true,
            "pinned": false
        })),
    ))
}

fn copied_agent_name(
    requested: Option<&str>,
    source: &Value,
    source_id: &str,
) -> Result<String, ApiError> {
    let name = requested
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map_or_else(
            || {
                format!(
                    "{} Copy",
                    config_string(source, "name").unwrap_or_else(|| source_id.to_owned())
                )
            },
            str::to_owned,
        );
    validate_name(&name)?;
    Ok(name)
}

fn cleanup_copied_workspace(
    server: &AppServer,
    agent_id: &str,
    target: &Path,
) -> Result<(), ApiError> {
    save_agent_secret(server, agent_id, None).map_err(|_| {
        internal("Agent copy credential cleanup failed; workspace retained for recovery")
    })?;
    fs::remove_dir_all(target)
        .map_err(|_| internal("Agent copy failed; temporary workspace cleanup requires recovery"))
}

async fn update_agent(
    State(server): State<AppServer>,
    AxumPath(agent_id): AxumPath<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    update_agent_config(&server, &agent_id, body, UpdateKind::General).await
}

async fn update_model_settings(
    State(server): State<AppServer>,
    AxumPath(agent_id): AxumPath<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    update_agent_config(&server, &agent_id, body, UpdateKind::Model).await
}

async fn update_backend_settings(
    State(server): State<AppServer>,
    AxumPath(agent_id): AxumPath<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    update_agent_config(&server, &agent_id, body, UpdateKind::Backend).await
}

#[derive(Clone, Copy)]
enum UpdateKind {
    General,
    Model,
    Backend,
}

#[allow(clippy::too_many_lines)]
async fn update_agent_config(
    server: &AppServer,
    agent_id: &str,
    body: Value,
    kind: UpdateKind,
) -> Result<Json<Value>, ApiError> {
    let submitted = body
        .as_object()
        .ok_or_else(|| bad_request("Agent config must be an object"))?;
    let _guard = server.inner.desktop_agents_lock.lock().await;
    server.recover_agent_publication()?;
    let workspace = desktop_workspace(server)?;
    let mut catalog = read_catalog(workspace)?;
    let context = context_from_catalog(&catalog, agent_id, false)?;
    let agent = catalog
        .agents
        .get_mut(agent_id)
        .ok_or_else(|| not_found(&format!("Agent '{agent_id}' not found")))?;
    if matches!(kind, UpdateKind::Backend)
        && config_string(&agent.config, "backend").as_deref() == Some("qwenpaw")
    {
        return Err(conflict(
            "QwenPaw models use the native model configuration",
        ));
    }
    let allowed = match kind {
        UpdateKind::General => None,
        UpdateKind::Model => Some(
            &[
                "fallback_models",
                "fallback_policy",
                "subagent_model",
                "thinking_level",
            ][..],
        ),
        UpdateKind::Backend => Some(&["model", "reasoning_effort"][..]),
    };
    let mut next = agent.config.as_object().cloned().unwrap_or_default();
    if matches!(kind, UpdateKind::Backend) {
        let settings = next
            .entry(String::from("backend_settings"))
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(|| internal("Stored backend settings are invalid"))?;
        for (key, value) in submitted {
            if !allowed.is_some_and(|fields| fields.contains(&key.as_str())) {
                return Err(bad_request("Unknown backend settings field"));
            }
            if value.is_null() {
                settings.remove(key);
            } else if value.as_str().is_some_and(|value| !value.trim().is_empty()) {
                settings.insert(key.clone(), value.clone());
            } else {
                return Err(bad_request(
                    "Backend settings values must be strings or null",
                ));
            }
        }
    } else {
        for (key, value) in submitted {
            if matches!(kind, UpdateKind::Model)
                && !allowed.is_some_and(|fields| fields.contains(&key.as_str()))
            {
                return Err(bad_request("Unknown model settings field"));
            }
            if matches!(kind, UpdateKind::Model)
                && matches!(
                    key.as_str(),
                    "fallback_models" | "fallback_policy" | "thinking_level"
                )
                && value.is_null()
            {
                return Err(bad_request(&format!("Field '{key}' cannot be null")));
            }
            if key != "channels" {
                next.insert(key.clone(), value.clone());
            }
        }
    }
    next.insert(String::from("id"), Value::String(agent_id.to_owned()));
    next.insert(
        String::from("workspace_dir"),
        Value::String(agent.workspace_dir.clone()),
    );
    let name = next
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| bad_request("Agent name is required"))?;
    validate_name(name)?;
    validate_description(
        next.get("description")
            .and_then(Value::as_str)
            .unwrap_or(""),
    )?;
    validate_backend(
        next.get("backend")
            .and_then(Value::as_str)
            .unwrap_or("qwenpaw"),
    )?;
    let _channels_guard = server.inner.desktop_channel_config_lock.lock().await;
    let channels = super::desktop_channels::prepare_profile_update(
        server,
        &context.data_key,
        &agent.config,
        matches!(kind, UpdateKind::General)
            .then(|| submitted.get("channels"))
            .flatten(),
    )?;
    let mut next = Value::Object(next);
    let submitted_secret = take_mail_secret(&mut next)?;
    validate_config_bounds(&next)?;
    agent.config.clone_from(&next);
    bump_catalog(&mut catalog);
    let mut response = public_config(&next);
    response["channels"] = channels.channels.clone();
    publication::publish(
        server,
        &context,
        &catalog,
        &next,
        submitted_secret.as_deref(),
        &channels,
    )?;
    Ok(Json(response))
}

async fn delete_agent(
    State(server): State<AppServer>,
    AxumPath(agent_id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    if agent_id == DEFAULT_AGENT_ID {
        return Err(bad_request("Cannot delete the default agent"));
    }
    let _lifecycle =
        super::desktop_checkpoints::quiescence::admit_agent_control(&server, &agent_id).await?;
    let cron_guard = server.inner.desktop_cron_lock.lock().await;
    let agent_guard = server.agent_catalog_guard().await?;
    let workspace = desktop_workspace(&server)?;
    let mut catalog = read_catalog(workspace)?;
    if !catalog.agents.contains_key(&agent_id) {
        return Err(not_found(&format!("Agent '{agent_id}' not found")));
    }
    let previous_secret = load_agent_secret(&server, &agent_id)?;
    catalog.agents.remove(&agent_id);
    catalog.order.retain(|configured| configured != &agent_id);
    bump_catalog(&mut catalog);
    save_agent_secret(&server, &agent_id, None)?;
    if let Err(error) = write_catalog(workspace, &catalog) {
        let _ = save_agent_secret(&server, &agent_id, previous_secret.as_deref());
        return Err(error);
    }
    let completed = super::desktop_cron::cancel_agent(&server, &agent_id);
    let mut console_completed = super::desktop_console_runs::cancel_agent(&server, &agent_id);
    console_completed.extend(super::protocol_runs::cancel_agent(&server, &agent_id));
    let checkpoints = super::desktop_checkpoints::runtime::cancel_agent(&server, &agent_id);
    drop(agent_guard);
    drop(cron_guard);
    super::desktop_cron::drain_runs(completed).await?;
    super::desktop_console_runs::drain_runs(console_completed).await?;
    super::desktop_checkpoints::runtime::drain(checkpoints).await;
    Ok(Json(json!({"success": true, "agent_id": agent_id})))
}

async fn toggle_agent(
    State(server): State<AppServer>,
    AxumPath(agent_id): AxumPath<String>,
    Json(body): Json<EnabledRequest>,
) -> Result<Json<Value>, ApiError> {
    if agent_id == DEFAULT_AGENT_ID && !body.enabled {
        return Err(bad_request("Cannot disable the default agent"));
    }
    // The request middleware retains its operation lease after disconnection.
    // Keep only this lock while draining: completion needs Cron and Agent locks.
    let _lifecycle =
        super::desktop_checkpoints::quiescence::admit_agent_control(&server, &agent_id).await?;
    let cron_guard = server.inner.desktop_cron_lock.lock().await;
    let agent_guard = server.agent_catalog_guard().await?;
    let workspace = desktop_workspace(&server)?;
    let mut catalog = read_catalog(workspace)?;
    let agent = catalog
        .agents
        .get_mut(&agent_id)
        .ok_or_else(|| not_found(&format!("Agent '{agent_id}' not found")))?;
    let jobs = if !agent.enabled && body.enabled {
        super::desktop_cron::prepare_restart(
            &server,
            agent
                .data_key
                .as_ref()
                .ok_or_else(|| internal("Agent Workspace identity is missing"))?,
            chrono::Utc::now(),
        )?
    } else {
        None
    };
    agent.enabled = body.enabled;
    bump_catalog(&mut catalog);
    if let Some(jobs) = &jobs {
        jobs.apply(&server)?;
    }
    if let Err(error) = write_catalog(workspace, &catalog) {
        if let Some(jobs) = &jobs
            && jobs.rollback(&server).is_err()
        {
            return Err(internal(
                "Agent restart rollback failed; Cron state requires recovery",
            ));
        }
        return Err(error);
    }
    let completed = if body.enabled {
        Vec::new()
    } else {
        super::desktop_cron::cancel_agent(&server, &agent_id)
    };
    let mut console_completed = if body.enabled {
        Vec::new()
    } else {
        super::desktop_console_runs::cancel_agent(&server, &agent_id)
    };
    if !body.enabled {
        console_completed.extend(super::protocol_runs::cancel_agent(&server, &agent_id));
    }
    let checkpoints = if body.enabled {
        Vec::new()
    } else {
        super::desktop_checkpoints::runtime::cancel_agent(&server, &agent_id)
    };
    drop(agent_guard);
    drop(cron_guard);
    super::desktop_cron::drain_runs(completed).await?;
    super::desktop_console_runs::drain_runs(console_completed).await?;
    super::desktop_checkpoints::runtime::drain(checkpoints).await;
    Ok(Json(json!({
        "success": true,
        "agent_id": agent_id,
        "enabled": body.enabled
    })))
}

async fn pin_agent(
    State(server): State<AppServer>,
    AxumPath(agent_id): AxumPath<String>,
    Json(body): Json<PinnedRequest>,
) -> Result<Json<Value>, ApiError> {
    if agent_id == DEFAULT_AGENT_ID && !body.pinned {
        return Err(bad_request("Cannot unpin the default agent"));
    }
    let _guard = server.agent_catalog_guard().await?;
    let workspace = desktop_workspace(&server)?;
    let mut catalog = read_catalog(workspace)?;
    let agent = catalog
        .agents
        .get_mut(&agent_id)
        .ok_or_else(|| not_found(&format!("Agent '{agent_id}' not found")))?;
    agent.pinned = if agent_id == DEFAULT_AGENT_ID {
        true
    } else {
        body.pinned
    };
    catalog.order = grouped_order(&catalog, &catalog.order);
    bump_catalog(&mut catalog);
    write_catalog(workspace, &catalog)?;
    Ok(Json(json!({
        "success": true,
        "agent_id": agent_id,
        "pinned": body.pinned || agent_id == DEFAULT_AGENT_ID
    })))
}

async fn reorder_agents(
    State(server): State<AppServer>,
    Json(body): Json<ReorderRequest>,
) -> Result<Json<Value>, ApiError> {
    let _guard = server.agent_catalog_guard().await?;
    let workspace = desktop_workspace(&server)?;
    let mut catalog = read_catalog(workspace)?;
    let mut unique = body.agent_ids.clone();
    unique.sort();
    unique.dedup();
    let configured = catalog.agents.keys().cloned().collect::<Vec<_>>();
    let mut expected = configured;
    expected.sort();
    if unique != expected || body.agent_ids.len() != expected.len() {
        return Err(bad_request(
            "Each configured agent ID must appear exactly once.",
        ));
    }
    if grouped_order(&catalog, &body.agent_ids) != body.agent_ids {
        return Err(bad_request(
            "Agent order must keep default first and pinned agents before unpinned agents.",
        ));
    }
    catalog.order = body.agent_ids;
    bump_catalog(&mut catalog);
    write_catalog(workspace, &catalog)?;
    Ok(Json(json!({"success": true, "agent_ids": catalog.order})))
}

async fn rebuild_memory_index(
    State(server): State<AppServer>,
    AxumPath(agent_id): AxumPath<String>,
    Query(query): Query<MemoryScopeQuery>,
) -> Result<Json<Value>, ApiError> {
    require_agent(&server, &agent_id).await?;
    if !matches!(query.scope.as_str(), "all" | "bm25" | "embedding") {
        return Err(bad_request("Memory reindex scope is invalid"));
    }
    Err(bad_request(
        "Memory index rebuild is only supported by ReMe Light",
    ))
}

async fn undo_memory_reindex(
    State(server): State<AppServer>,
    AxumPath(agent_id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    require_agent(&server, &agent_id).await?;
    Err(bad_request(
        "Embedding index undo is only supported by ReMe Light",
    ))
}

async fn memory_runtime_status(
    State(server): State<AppServer>,
    AxumPath(agent_id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    require_agent(&server, &agent_id).await?;
    Ok(Json(memory_runtime()))
}

async fn memory_status(
    State(server): State<AppServer>,
    AxumPath(agent_id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    require_agent(&server, &agent_id).await?;
    Ok(Json(json!({
        "components": {},
        "components_total": "0 B",
        "process_rss": "0 B",
        "runtime": memory_runtime()
    })))
}

async fn memory_graph(
    State(server): State<AppServer>,
    AxumPath(agent_id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    require_agent(&server, &agent_id).await?;
    Ok(Json(json!({"version": 1, "nodes": [], "edges": []})))
}

fn memory_runtime() -> Value {
    json!({
        "worker": {
            "status": "idle",
            "queue_pending": 0,
            "tasks_running": 0
        },
        "auto_memory": {"enabled": false, "interval": 0},
        "tasks": [],
        "recent": {"last_error": null},
        "reindexing": false,
        "embedding_reindex_required": false,
        "embedding_reindex_undo_available": false
    })
}

async fn require_agent(server: &AppServer, agent_id: &str) -> Result<(), ApiError> {
    let _guard = server.agent_catalog_guard().await?;
    let catalog = read_catalog(desktop_workspace(server)?)?;
    if catalog.agents.contains_key(agent_id) {
        Ok(())
    } else {
        Err(not_found(&format!("Agent '{agent_id}' not found")))
    }
}

fn desktop_workspace(server: &AppServer) -> Result<&DesktopWorkspace, ApiError> {
    server
        .inner
        .desktop_workspace
        .as_ref()
        .ok_or_else(|| internal("Desktop Workspace is unavailable"))
}

fn catalog_directory(workspace: &DesktopWorkspace) -> PathBuf {
    workspace.data_dir.join("agents")
}

pub(super) fn catalog_path(workspace: &DesktopWorkspace) -> PathBuf {
    catalog_directory(workspace).join("catalog.json")
}

fn default_catalog(core: &Core, default_workspace: &Path, project: &Path) -> AgentCatalog {
    let data_key = WorkspaceDataKey::Workspace(Uuid::now_v7());
    let config = core.read_config().config;
    let active_model = json!({
        "provider_id": "openai-compatible",
        "model": config.default_model
    });
    let mut agent_config = default_agent_config(
        DEFAULT_AGENT_ID,
        "QwenPaw",
        "Rust Core",
        default_workspace,
        "qwenpaw",
        "en",
        Some(&active_model),
    );
    if project != default_workspace {
        agent_config["project_dir"] = json!(project.to_string_lossy());
    }
    AgentCatalog {
        schema_version: CATALOG_SCHEMA_VERSION,
        revision: 0,
        bootstrap_identity: true,
        order: vec![String::from(DEFAULT_AGENT_ID)],
        workspace_keys: BTreeMap::from([(
            default_workspace.to_string_lossy().into_owned(),
            data_key.clone(),
        )]),
        agents: BTreeMap::from([(
            String::from(DEFAULT_AGENT_ID),
            AgentReference {
                workspace_dir: default_workspace.to_string_lossy().into_owned(),
                enabled: true,
                pinned: true,
                config: agent_config,
                data_key: Some(data_key),
            },
        )]),
    }
}

fn default_agent_config(
    id: &str,
    name: &str,
    description: &str,
    workspace: &Path,
    backend: &str,
    language: &str,
    active_model: Option<&Value>,
) -> Value {
    json!({
        "id": id,
        "name": name,
        "description": description,
        "workspace_dir": workspace.to_string_lossy(),
        "backend": backend,
        "backend_settings": {},
        "approval_level": "smart",
        "active_model": active_model,
        "fallback_models": [],
        "fallback_policy": {"enabled": false, "target_scope": "configured"},
        "subagent_model": null,
        "thinking_level": "inherit",
        "language": language,
        "channels": {},
        "mcp": {},
        "heartbeat": {},
        "running": super::desktop_agent_settings::default_running_config(),
        "audio_mode": "auto",
        "transcription_provider_type": "disabled",
        "transcription_provider_id": "",
        "llm_routing": {},
        "system_prompt_files": ["AGENTS.md", "SOUL.md", "PROFILE.md"],
        "tools": {},
        "security": {},
        "mail": null
    })
}

fn read_catalog(workspace: &DesktopWorkspace) -> Result<AgentCatalog, ApiError> {
    let path = catalog_path(workspace);
    let metadata = fs::metadata(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            not_found("Rust Agent catalog not found")
        } else {
            internal("Rust Agent catalog could not be inspected")
        }
    })?;
    if !metadata.is_file() || metadata.len() > MAX_CATALOG_BYTES {
        return Err(internal("Rust Agent catalog is invalid"));
    }
    let bytes = fs::read(&path).map_err(|_| internal("Rust Agent catalog could not be read"))?;
    let mut catalog = serde_json::from_slice::<AgentCatalog>(&bytes)
        .map_err(|_| internal("Rust Agent catalog is invalid"))?;
    identity::hydrate_legacy(&mut catalog)?;
    validate_catalog(&catalog, workspace)?;
    Ok(catalog)
}

fn write_catalog(workspace: &DesktopWorkspace, catalog: &AgentCatalog) -> Result<(), ApiError> {
    validate_catalog(catalog, workspace)?;
    let bytes = serde_json::to_vec_pretty(catalog)
        .map_err(|_| internal("Rust Agent catalog could not be encoded"))?;
    if bytes.len() as u64 > MAX_CATALOG_BYTES {
        return Err(payload_too_large("Rust Agent catalog is too large"));
    }
    if catalog.bootstrap_identity {
        identity::bootstrap(catalog)?;
    }
    let directory = catalog_directory(workspace);
    fs::create_dir_all(&directory)
        .map_err(|_| internal("Rust Agent catalog directory could not be created"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(&directory)
        .map_err(|_| internal("Rust Agent catalog could not be staged"))?;
    temporary
        .write_all(&bytes)
        .and_then(|()| temporary.flush())
        .map_err(|_| internal("Rust Agent catalog could not be staged"))?;
    temporary
        .persist(catalog_path(workspace))
        .map_err(|_| internal("Rust Agent catalog could not be persisted"))?;
    Ok(())
}

fn validate_catalog(catalog: &AgentCatalog, workspace: &DesktopWorkspace) -> Result<(), ApiError> {
    identity::validate(catalog)?;
    if catalog.schema_version != CATALOG_SCHEMA_VERSION
        || !catalog.agents.contains_key(DEFAULT_AGENT_ID)
        || catalog.order.first().map(String::as_str) != Some(DEFAULT_AGENT_ID)
        || catalog.order.len() != catalog.agents.len()
    {
        return Err(internal("Rust Agent catalog structure is invalid"));
    }
    let mut order = catalog.order.clone();
    order.sort();
    order.dedup();
    let mut ids = catalog.agents.keys().cloned().collect::<Vec<_>>();
    ids.sort();
    if order != ids || grouped_order(catalog, &catalog.order) != catalog.order {
        return Err(internal("Rust Agent catalog order is invalid"));
    }
    for (agent_id, agent) in &catalog.agents {
        validate_agent_id(agent_id, true)?;
        validate_config_bounds(&agent.config)?;
        let configured_id = agent.config.get("id").and_then(Value::as_str);
        let configured_workspace = agent.config.get("workspace_dir").and_then(Value::as_str);
        if configured_id != Some(agent_id)
            || configured_workspace != Some(agent.workspace_dir.as_str())
            || (agent_id == DEFAULT_AGENT_ID && (!agent.enabled || !agent.pinned))
        {
            return Err(internal("Rust Agent catalog entry is invalid"));
        }
        validate_registered_workspace_path(&agent.workspace_dir)?;
    }
    let default = catalog
        .agents
        .get(DEFAULT_AGENT_ID)
        .ok_or_else(|| internal("Default Agent is missing"))?;
    if default.workspace_dir.is_empty() || workspace.initial.as_os_str().is_empty() {
        return Err(internal("Default Agent Workspace is invalid"));
    }
    Ok(())
}

fn grouped_order(catalog: &AgentCatalog, order: &[String]) -> Vec<String> {
    let mut pinned = Vec::new();
    let mut regular = Vec::new();
    for agent_id in order {
        if agent_id == DEFAULT_AGENT_ID {
            continue;
        }
        if catalog
            .agents
            .get(agent_id)
            .is_some_and(|agent| agent.pinned)
        {
            pinned.push(agent_id.clone());
        } else {
            regular.push(agent_id.clone());
        }
    }
    let mut grouped = vec![String::from(DEFAULT_AGENT_ID)];
    grouped.extend(pinned);
    grouped.extend(regular);
    grouped
}

fn agent_summary(agent_id: &str, agent: &AgentReference) -> Value {
    let backend = config_string(&agent.config, "backend").unwrap_or_else(default_backend);
    let backend_settings = agent
        .config
        .get("backend_settings")
        .and_then(Value::as_object);
    json!({
        "id": agent_id,
        "name": config_string(&agent.config, "name").unwrap_or_else(|| agent_id.to_owned()),
        "description": config_string(&agent.config, "description").unwrap_or_default(),
        "workspace_dir": agent.workspace_dir,
        "enabled": agent.enabled,
        "pinned": agent.pinned,
        "startup_status": if agent.enabled { "running" } else { "disabled" },
        "backend": backend,
        "backend_capabilities": if backend == "qwenpaw" { json!({"workspace_ui": true}) } else { json!({}) },
        "backend_model": backend_settings.and_then(|settings| settings.get("model")).cloned(),
        "backend_reasoning_effort": backend_settings.and_then(|settings| settings.get("reasoning_effort")).cloned(),
        "active_model": agent.config.get("active_model").cloned().unwrap_or(Value::Null),
        "managed_by_app": Value::Null,
        "available_in_chat": true
    })
}

fn public_config(config: &Value) -> Value {
    let mut public = config.clone();
    if let Some(credential) = public
        .pointer_mut("/mail/credential")
        .and_then(Value::as_object_mut)
    {
        credential.remove("auth_code");
    }
    public
}

fn resolve_new_workspace(
    desktop: &DesktopWorkspace,
    requested: Option<&str>,
    agent_id: &str,
    catalog: &AgentCatalog,
) -> Result<(PathBuf, bool, WorkspaceDataKey), ApiError> {
    let (candidate, auto) = match requested.map(str::trim).filter(|path| !path.is_empty()) {
        None => (desktop.data_dir.join("workspaces").join(agent_id), true),
        Some(path) => {
            if path.len() > 4_096 || path.chars().any(char::is_control) {
                return Err(bad_request("workspace_dir is invalid"));
            }
            let raw = Path::new(path);
            if raw
                .components()
                .any(|component| component == Component::ParentDir)
            {
                return Err(bad_request(
                    "workspace_dir must not contain '..' path segments",
                ));
            }
            let expanded = if path == "~" {
                dirs::home_dir().ok_or_else(|| bad_request("Home directory is unavailable"))?
            } else if let Some(relative) = path.strip_prefix("~/") {
                dirs::home_dir()
                    .ok_or_else(|| bad_request("Home directory is unavailable"))?
                    .join(relative)
            } else if raw.is_absolute() {
                raw.to_path_buf()
            } else {
                desktop.data_dir.join("workspaces").join(raw)
            };
            (expanded, false)
        }
    };
    let created = create_workspace_directory(&candidate)?;
    let canonical = candidate
        .canonicalize()
        .map_err(|_| bad_request("Agent Workspace could not be resolved"))?;
    if !canonical.is_dir() {
        return Err(bad_request("Agent Workspace is not a directory"));
    }
    if catalog.agents.values().any(|agent| {
        Path::new(&agent.workspace_dir)
            .canonicalize()
            .is_ok_and(|workspace| workspace == canonical)
    }) {
        return Err(bad_request("Agent Workspace is already registered"));
    }
    let data_key = identity::key_for_workspace(catalog, &canonical, created)?;
    identity::write_marker(&canonical, &data_key)?;
    Ok((canonical, auto && created, data_key))
}

fn create_workspace_directory(candidate: &Path) -> Result<bool, ApiError> {
    let parent = candidate
        .parent()
        .ok_or_else(|| bad_request("Agent Workspace parent is unavailable"))?;
    fs::create_dir_all(parent)
        .map_err(|_| bad_request("Agent Workspace parent could not be created"))?;
    // Only an exclusive successful mkdir grants rollback ownership. An
    // existing directory or link may hold data retained by Agent deletion.
    match fs::create_dir(candidate) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(_) => Err(bad_request("Agent Workspace could not be created")),
    }
}

fn canonical_registered_workspace(path: &str) -> Result<PathBuf, ApiError> {
    validate_registered_workspace_path(path)?;
    let workspace = PathBuf::from(path)
        .canonicalize()
        .map_err(|_| internal("Registered Agent Workspace is unavailable"))?;
    if !workspace.is_dir() {
        return Err(internal("Registered Agent Workspace is unavailable"));
    }
    Ok(workspace)
}

fn validate_registered_workspace_path(path: &str) -> Result<(), ApiError> {
    if path.is_empty() || path.len() > 4_096 || path.chars().any(char::is_control) {
        return Err(internal("Registered Agent Workspace is invalid"));
    }
    if !Path::new(path).is_absolute()
        || Path::new(path)
            .components()
            .any(|component| component == Component::ParentDir)
    {
        return Err(internal("Registered Agent Workspace is invalid"));
    }
    Ok(())
}

fn initialize_workspace(workspace: &Path, language: &str) -> Result<(), ApiError> {
    initialize_workspace_selective(workspace, language, true, true, true)
}

fn initialize_workspace_selective(
    workspace: &Path,
    language: &str,
    templates: bool,
    skills: bool,
    jobs: bool,
) -> Result<(), ApiError> {
    fs::create_dir_all(workspace.join("sessions"))
        .and_then(|()| fs::create_dir_all(workspace.join("memory")))
        .map_err(|_| internal("Agent Workspace directories could not be initialized"))?;
    if skills {
        fs::create_dir_all(workspace.join("skills"))
            .map_err(|_| internal("Agent Skills directory could not be initialized"))?;
        write_json_if_missing(
            &workspace.join("skill.json"),
            &json!({"version": 0, "skills": {}}),
        )?;
    }
    if templates {
        super::desktop_agent_settings::copy_agent_templates_to(workspace, language, false)?;
    }
    if jobs {
        write_json_if_missing(
            &workspace.join("jobs.json"),
            &json!({"version": 1, "jobs": []}),
        )?;
    }
    write_json_if_missing(
        &workspace.join("chats.json"),
        &json!({"version": 1, "chats": []}),
    )
}

fn write_json_if_missing(path: &Path, value: &Value) -> Result<(), ApiError> {
    if path.exists() {
        return Ok(());
    }
    let parent = path
        .parent()
        .ok_or_else(|| internal("Agent file path is invalid"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|_| internal("Agent file could not be staged"))?;
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|_| internal("Agent file could not be encoded"))?;
    temporary
        .write_all(&bytes)
        .map_err(|_| internal("Agent file could not be staged"))?;
    temporary.persist_noclobber(path).map_err(|error| {
        if error.error.kind() == std::io::ErrorKind::AlreadyExists {
            internal("Agent file appeared during initialization")
        } else {
            internal("Agent file could not be installed")
        }
    })?;
    Ok(())
}

fn write_agent_config(workspace: &Path, config: &Value) -> Result<(), ApiError> {
    validate_config_bounds(config)?;
    let bytes = serde_json::to_vec_pretty(config)
        .map_err(|_| internal("Agent config could not be encoded"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(workspace)
        .map_err(|_| internal("Agent config could not be staged"))?;
    temporary
        .write_all(&bytes)
        .and_then(|()| temporary.flush())
        .map_err(|_| internal("Agent config could not be staged"))?;
    temporary
        .persist(workspace.join("agent.json"))
        .map_err(|_| internal("Agent config could not be persisted"))?;
    Ok(())
}

fn validate_initial_skills(
    desktop: &DesktopWorkspace,
    skill_names: &[String],
) -> Result<(), ApiError> {
    if skill_names.len() > 256 {
        return Err(payload_too_large("Too many initial Skills"));
    }
    for name in skill_names {
        validate_file_component(name, "Skill name")?;
        let source = desktop.data_dir.join("skill_pool").join(name);
        if !source.join("SKILL.md").is_file() {
            return Err(bad_request(&format!("Skill '{name}' is not in Skill Pool")));
        }
    }
    Ok(())
}

fn install_initial_skills(
    desktop: &DesktopWorkspace,
    workspace: &Path,
    skill_names: &[String],
) -> Result<(), ApiError> {
    for name in skill_names {
        copy_directory_bounded(
            &desktop.data_dir.join("skill_pool").join(name),
            &workspace.join("skills").join(name),
        )?;
    }
    Ok(())
}

fn copy_selected_workspace_files(
    source: &Path,
    target: &Path,
    request: &CopyAgentRequest,
) -> Result<(), ApiError> {
    if request.copy_md_files {
        for filename in COPYABLE_MD_FILES {
            let source_file = source.join(filename);
            if source_file.is_file() {
                fs::copy(&source_file, target.join(filename))
                    .map_err(|_| internal("Agent Markdown file could not be copied"))?;
            }
        }
    }
    if request.copy_skills && source.join("skills").is_dir() {
        copy_directory_bounded(&source.join("skills"), &target.join("skills"))?;
        let manifest = source.join("skill.json");
        if manifest.is_file() {
            fs::copy(manifest, target.join("skill.json"))
                .map_err(|_| internal("Agent Skill manifest could not be copied"))?;
        }
    }
    Ok(())
}

fn copy_directory_bounded(source: &Path, target: &Path) -> Result<(), ApiError> {
    if target.exists() {
        fs::remove_dir_all(target)
            .map_err(|_| internal("Agent copy target could not be prepared"))?;
    }
    fs::create_dir_all(target).map_err(|_| internal("Agent copy target could not be created"))?;
    let mut stack = vec![(source.to_path_buf(), target.to_path_buf())];
    let mut file_count = 0_usize;
    let mut byte_count = 0_u64;
    while let Some((from, to)) = stack.pop() {
        for entry in fs::read_dir(from).map_err(|_| internal("Agent source could not be read"))? {
            let entry = entry.map_err(|_| internal("Agent source could not be read"))?;
            let kind = entry
                .file_type()
                .map_err(|_| internal("Agent source entry could not be inspected"))?;
            if kind.is_symlink() {
                let _ = fs::remove_dir_all(target);
                return Err(bad_request(
                    "Agent copy source cannot contain symbolic links",
                ));
            }
            let destination = to.join(entry.file_name());
            if kind.is_dir() {
                fs::create_dir_all(&destination)
                    .map_err(|_| internal("Agent copy directory could not be created"))?;
                stack.push((entry.path(), destination));
            } else if kind.is_file() {
                file_count = file_count.saturating_add(1);
                byte_count = byte_count.saturating_add(
                    entry
                        .metadata()
                        .map_err(|_| internal("Agent source entry could not be inspected"))?
                        .len(),
                );
                if file_count > MAX_COPY_FILES || byte_count > MAX_COPY_BYTES {
                    let _ = fs::remove_dir_all(target);
                    return Err(payload_too_large("Agent copy exceeds limits"));
                }
                fs::copy(entry.path(), destination)
                    .map_err(|_| internal("Agent source file could not be copied"))?;
            }
        }
    }
    Ok(())
}

fn take_mail_secret(config: &mut Value) -> Result<Option<String>, ApiError> {
    let Some(credential) = config.pointer_mut("/mail/credential") else {
        return Ok(None);
    };
    let credential = credential
        .as_object_mut()
        .ok_or_else(|| bad_request("Agent mail credential must be an object"))?;
    let secret = credential.remove("auth_code");
    match secret {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(secret)) if secret.len() <= 16 * 1024 => Ok(Some(secret)),
        Some(Value::String(_)) => Err(payload_too_large("Agent mail auth code is too large")),
        Some(_) => Err(bad_request("Agent mail auth code must be a string")),
    }
}

fn secret_key(agent_id: &str) -> String {
    crate::desktop_credentials::agent_mail_secret_key(agent_id)
}

fn load_agent_secret(server: &AppServer, agent_id: &str) -> Result<Option<String>, ApiError> {
    let credentials = server
        .inner
        .desktop_credentials
        .as_ref()
        .ok_or_else(|| internal("System credential storage is unavailable"))?;
    credentials
        .load_agent_setting_secret(&secret_key(agent_id))
        .map_err(|_| internal("System credential storage is unavailable"))
}

fn save_agent_secret(
    server: &AppServer,
    agent_id: &str,
    secret: Option<&str>,
) -> Result<(), ApiError> {
    let credentials = server
        .inner
        .desktop_credentials
        .as_ref()
        .ok_or_else(|| internal("System credential storage is unavailable"))?;
    credentials
        .save_agent_setting_secret(&secret_key(agent_id), secret)
        .map_err(|_| internal("System credential storage is unavailable"))
}

pub(super) fn validate_agent_id(agent_id: &str, allow_default: bool) -> Result<(), ApiError> {
    if !qwenpaw_storage::is_valid_agent_id(agent_id) {
        return Err(bad_request(&format!(
            "Agent ID '{agent_id}' contains invalid characters. Only letters, digits, hyphens, and underscores are allowed. Cannot start or end with '-' or '_'."
        )));
    }
    if !allow_default && agent_id == DEFAULT_AGENT_ID {
        return Err(bad_request(
            "Agent ID 'default' is reserved and cannot be used.",
        ));
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<(), ApiError> {
    let name = name.trim();
    if name.is_empty() || name.len() > MAX_AGENT_NAME_BYTES || name.chars().any(char::is_control) {
        return Err(bad_request("Agent name is invalid"));
    }
    Ok(())
}

fn validate_description(description: &str) -> Result<(), ApiError> {
    if description.len() > MAX_AGENT_DESCRIPTION_BYTES || description.chars().any(char::is_control)
    {
        return Err(bad_request("Agent description is invalid"));
    }
    Ok(())
}

fn validate_backend(backend: &str) -> Result<(), ApiError> {
    if backend == "qwenpaw" {
        Ok(())
    } else {
        Err(conflict(&format!(
            "Agent backend '{backend}' is not available in Rust Core"
        )))
    }
}

fn normalize_language(language: Option<&str>) -> Result<String, ApiError> {
    let language = language.unwrap_or("en").trim().to_ascii_lowercase();
    if matches!(language.as_str(), "en" | "id" | "ru" | "zh") {
        Ok(language)
    } else {
        Err(bad_request("Agent language is invalid"))
    }
}

fn validate_config_bounds(config: &Value) -> Result<(), ApiError> {
    let bytes =
        serde_json::to_vec(config).map_err(|_| bad_request("Agent config could not be encoded"))?;
    if bytes.len() > MAX_AGENT_CONFIG_BYTES {
        return Err(payload_too_large("Agent config is too large"));
    }
    validate_value(config, 0)
}

fn validate_value(value: &Value, depth: usize) -> Result<(), ApiError> {
    if depth > 20 {
        return Err(payload_too_large("Agent config is too deeply nested"));
    }
    match value {
        Value::Array(values) => {
            if values.len() > 2_048 {
                return Err(payload_too_large("Agent config has too many items"));
            }
            for value in values {
                validate_value(value, depth + 1)?;
            }
        }
        Value::Object(values) => {
            if values.len() > 2_048 {
                return Err(payload_too_large("Agent config has too many fields"));
            }
            for (key, value) in values {
                if key.len() > 256 || key.chars().any(char::is_control) {
                    return Err(bad_request("Agent config field is invalid"));
                }
                validate_value(value, depth + 1)?;
            }
        }
        Value::String(value) if value.len() > 64 * 1024 => {
            return Err(payload_too_large("Agent config string is too large"));
        }
        _ => {}
    }
    Ok(())
}

fn validate_file_component(value: &str, label: &str) -> Result<(), ApiError> {
    if value.is_empty()
        || value.len() > 255
        || value == "."
        || value == ".."
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\'))
    {
        return Err(bad_request(&format!("{label} is invalid")));
    }
    Ok(())
}

fn generated_agent_id(catalog: &AgentCatalog) -> String {
    loop {
        let id = Uuid::now_v7()
            .simple()
            .to_string()
            .chars()
            .skip(26)
            .collect::<String>();
        if !catalog.agents.contains_key(&id) {
            return id;
        }
    }
}

fn config_string(config: &Value, key: &str) -> Option<String> {
    config.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn bump_catalog(catalog: &mut AgentCatalog) {
    catalog.revision = catalog.revision.saturating_add(1);
}

fn cleanup_auto_workspace(workspace: &Path, auto: bool) {
    if auto {
        let _ = fs::remove_dir_all(workspace);
    }
}

fn default_backend() -> String {
    String::from("qwenpaw")
}

const fn enabled() -> bool {
    true
}

fn default_memory_scope() -> String {
    String::from("all")
}

fn api_error_message(error: ApiError) -> anyhow::Error {
    let (_, Json(body)) = error;
    anyhow::anyhow!(
        body.get("detail")
            .and_then(Value::as_str)
            .unwrap_or("Rust Agent catalog error")
            .to_owned()
    )
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

fn forbidden(detail: &str) -> ApiError {
    (StatusCode::FORBIDDEN, Json(json!({"detail": detail})))
}

fn payload_too_large(detail: &str) -> ApiError {
    (
        StatusCode::PAYLOAD_TOO_LARGE,
        Json(json!({"detail": detail})),
    )
}

fn internal(detail: &str) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"detail": detail})),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_agent_ids_and_groups_pinned_order() {
        assert!(validate_agent_id("agent-1", false).is_ok());
        assert!(validate_agent_id("_agent", false).is_err());
        assert!(validate_agent_id("default", false).is_err());
        let workspace = tempfile::tempdir().expect("Workspace should be created");
        let config =
            |id: &str| default_agent_config(id, id, "", workspace.path(), "qwenpaw", "en", None);
        let catalog = AgentCatalog {
            schema_version: CATALOG_SCHEMA_VERSION,
            revision: 0,
            bootstrap_identity: false,
            workspace_keys: BTreeMap::new(),
            order: vec![
                String::from("default"),
                String::from("regular"),
                String::from("pinned"),
            ],
            agents: BTreeMap::from([
                (
                    String::from("default"),
                    AgentReference {
                        workspace_dir: workspace.path().to_string_lossy().into_owned(),
                        enabled: true,
                        pinned: true,
                        config: config("default"),
                        data_key: None,
                    },
                ),
                (
                    String::from("regular"),
                    AgentReference {
                        workspace_dir: workspace.path().to_string_lossy().into_owned(),
                        enabled: true,
                        pinned: false,
                        config: config("regular"),
                        data_key: None,
                    },
                ),
                (
                    String::from("pinned"),
                    AgentReference {
                        workspace_dir: workspace.path().to_string_lossy().into_owned(),
                        enabled: true,
                        pinned: true,
                        config: config("pinned"),
                        data_key: None,
                    },
                ),
            ]),
        };
        assert_eq!(
            grouped_order(&catalog, &catalog.order),
            vec!["default", "pinned", "regular"]
        );
    }
}
