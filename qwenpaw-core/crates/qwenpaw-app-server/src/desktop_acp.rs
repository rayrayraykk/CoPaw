//! ACP configuration, Node runtime discovery, and control-command detection.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use axum::Json;
use axum::Router;
use axum::extract::Path as AxumPath;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::routing::get;
use axum::routing::post;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use tokio::process::Command;
use tokio::time::timeout;

use super::AppServer;

const NODE_RUNTIME_SCHEMA_VERSION: u32 = 1;
const NODE_RUNTIME_CONFIG_MAX_BYTES: u64 = 16 * 1024;
const MAX_ACP_AGENTS: usize = 256;
const MAX_ACP_AGENT_NAME_BYTES: usize = 255;
const MAX_ACP_COMMAND_BYTES: usize = 4_096;
const MAX_ACP_ARGUMENTS: usize = 512;
const MAX_ACP_ENVIRONMENT_VARIABLES: usize = 512;
const MAX_ACP_STRING_BYTES: usize = 16 * 1024;
const MAX_NODE_PATH_BYTES: usize = 4_096;
const DEFAULT_STDIO_BUFFER_LIMIT_BYTES: u64 = 50 * 1024 * 1024;
const PROCESS_VERSION_TIMEOUT: Duration = Duration::from_secs(5);
const DESKTOP_NODE_RUNTIME_ENV: &str = "QWENPAW_DESKTOP_NODE_RUNTIME";

type ApiError = (StatusCode, Json<Value>);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
struct AcpAgentConfig {
    enabled: bool,
    command: String,
    args: Vec<String>,
    env: BTreeMap<String, String>,
    trusted: bool,
    tool_parse_mode: String,
    stdio_buffer_limit_bytes: u64,
}

impl Default for AcpAgentConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            command: String::new(),
            args: Vec::new(),
            env: BTreeMap::new(),
            trusted: true,
            tool_parse_mode: String::from("call_title"),
            stdio_buffer_limit_bytes: DEFAULT_STDIO_BUFFER_LIMIT_BYTES,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
struct AcpConfig {
    node_path: String,
    agents: BTreeMap<String, AcpAgentConfig>,
}

impl Default for AcpConfig {
    fn default() -> Self {
        Self {
            node_path: String::new(),
            agents: default_acp_agents(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct NodeRuntimeFile {
    schema_version: u32,
    node_path: String,
}

impl Default for NodeRuntimeFile {
    fn default() -> Self {
        Self {
            schema_version: NODE_RUNTIME_SCHEMA_VERSION,
            node_path: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct NodeRuntimeCandidate {
    key: String,
    label: String,
    node_path: String,
    npx_path: String,
    node_version: String,
    npx_version: String,
    available: bool,
    reason_code: String,
    reason: String,
}

impl NodeRuntimeCandidate {
    fn unavailable(key: &str, label: &str, reason_code: &str, reason: &str) -> Self {
        Self {
            key: key.to_owned(),
            label: label.to_owned(),
            node_path: String::new(),
            npx_path: String::new(),
            node_version: String::new(),
            npx_version: String::new(),
            available: false,
            reason_code: reason_code.to_owned(),
            reason: reason.to_owned(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct NodeRuntimeStatus {
    node_path: String,
    effective_node_path: String,
    candidates: Vec<NodeRuntimeCandidate>,
}

#[derive(Debug, Deserialize)]
struct NodeRuntimeUpdate {
    node_path: String,
}

#[derive(Debug, Deserialize)]
struct CommandCheckRequest {
    text: String,
}

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route("/api/config/acp", get(get_acp_config).put(put_acp_config))
        .route(
            "/api/config/acp/node-runtime",
            get(get_node_runtime).put(put_node_runtime),
        )
        .route(
            "/api/config/acp/{agent_name}",
            get(get_acp_agent_config).put(put_acp_agent_config),
        )
        .route("/api/commands/check", post(check_command))
}

async fn get_acp_config(
    State(server): State<AppServer>,
    headers: HeaderMap,
) -> Result<Json<AcpConfig>, ApiError> {
    let agent_id = super::desktop_agents::requested_agent_id(&headers)?;
    let config = super::desktop_agents::config_for_agent(&server, &agent_id).await?;
    Ok(Json(acp_config_from_agent(&config)?))
}

async fn put_acp_config(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Json(mut config): Json<AcpConfig>,
) -> Result<Json<AcpConfig>, ApiError> {
    validate_acp_config(&config, false)?;
    merge_default_agents(&mut config.agents);
    let agent_id = super::desktop_agents::requested_agent_id(&headers)?;
    let _guard = server.inner.desktop_acp_lock.lock().await;
    super::desktop_agents::replace_config_field(
        &server,
        &agent_id,
        "acp",
        serde_json::to_value(&config).map_err(|_| internal("ACP config could not be encoded"))?,
    )
    .await?;
    Ok(Json(config))
}

async fn get_acp_agent_config(
    State(server): State<AppServer>,
    headers: HeaderMap,
    AxumPath(agent_name): AxumPath<String>,
) -> Result<Json<AcpAgentConfig>, ApiError> {
    let agent_name = validate_agent_name(&agent_name)?;
    let agent_id = super::desktop_agents::requested_agent_id(&headers)?;
    let config = super::desktop_agents::config_for_agent(&server, &agent_id).await?;
    let config = acp_config_from_agent(&config)?;
    config
        .agents
        .get(agent_name)
        .cloned()
        .map(Json)
        .ok_or_else(|| not_found(&format!("ACP agent '{agent_name}' not found")))
}

async fn put_acp_agent_config(
    State(server): State<AppServer>,
    headers: HeaderMap,
    AxumPath(agent_name): AxumPath<String>,
    Json(config): Json<AcpAgentConfig>,
) -> Result<Json<AcpAgentConfig>, ApiError> {
    let agent_name = validate_agent_name(&agent_name)?.to_owned();
    validate_acp_agent(&config, true)?;
    let agent_id = super::desktop_agents::requested_agent_id(&headers)?;
    let _guard = server.inner.desktop_acp_lock.lock().await;
    let current = super::desktop_agents::config_for_agent(&server, &agent_id).await?;
    let mut acp = acp_config_from_agent(&current)?;
    acp.agents.insert(agent_name, config.clone());
    super::desktop_agents::replace_config_field(
        &server,
        &agent_id,
        "acp",
        serde_json::to_value(&acp).map_err(|_| internal("ACP config could not be encoded"))?,
    )
    .await?;
    Ok(Json(config))
}

async fn get_node_runtime(
    State(server): State<AppServer>,
) -> Result<Json<NodeRuntimeStatus>, ApiError> {
    let _guard = server.inner.desktop_acp_lock.lock().await;
    let config = read_node_runtime_file(&server)?;
    Ok(Json(node_runtime_status(&config.node_path).await))
}

async fn put_node_runtime(
    State(server): State<AppServer>,
    Json(body): Json<NodeRuntimeUpdate>,
) -> Result<Json<NodeRuntimeStatus>, ApiError> {
    let node_path = body.node_path.trim();
    validate_node_path(node_path)?;
    if !node_path.is_empty() {
        let candidate = resolve_node_runtime(node_path, "custom", "custom").await;
        if !candidate.available {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "detail": {
                        "reason_code": candidate.reason_code,
                        "reason": candidate.reason
                    }
                })),
            ));
        }
    }
    let _guard = server.inner.desktop_acp_lock.lock().await;
    write_node_runtime_file(
        &server,
        &NodeRuntimeFile {
            schema_version: NODE_RUNTIME_SCHEMA_VERSION,
            node_path: node_path.to_owned(),
        },
    )?;
    Ok(Json(node_runtime_status(node_path).await))
}

async fn check_command(Json(body): Json<CommandCheckRequest>) -> Result<Json<Value>, ApiError> {
    if body.text.len() > MAX_ACP_STRING_BYTES {
        return Err(payload_too_large("Command text is too large"));
    }
    let token = body
        .text
        .split_whitespace()
        .next()
        .map(str::to_ascii_lowercase);
    let is_control_command = token.as_deref().is_some_and(|candidate| {
        matches!(
            candidate,
            "/approval" | "/approve" | "/deny" | "/stop" | "/model" | "/skills" | "/checkpoint"
        )
    });
    Ok(Json(json!({
        "is_control_command": is_control_command,
        "command_token": token.filter(|_| is_control_command)
    })))
}

fn acp_config_from_agent(agent_config: &Value) -> Result<AcpConfig, ApiError> {
    let mut config = match agent_config.get("acp") {
        None | Some(Value::Null) => AcpConfig::default(),
        Some(value) => serde_json::from_value::<AcpConfig>(value.clone())
            .map_err(|_| internal("Stored ACP config is invalid"))?,
    };
    validate_acp_config(&config, false).map_err(|_| internal("Stored ACP config is invalid"))?;
    merge_default_agents(&mut config.agents);
    Ok(config)
}

fn default_acp_agents() -> BTreeMap<String, AcpAgentConfig> {
    BTreeMap::from([
        (
            String::from("opencode"),
            enabled_acp_agent("opencode", &["acp"], "update_detail"),
        ),
        (
            String::from("qwen_code"),
            enabled_acp_agent("qwen", &["--acp"], "call_detail"),
        ),
        (
            String::from("claude_code"),
            enabled_acp_agent(
                "npx",
                &["-y", "@zed-industries/claude-agent-acp"],
                "update_detail",
            ),
        ),
        (
            String::from("codex"),
            enabled_acp_agent("npx", &["-y", "@zed-industries/codex-acp"], "call_detail"),
        ),
    ])
}

fn enabled_acp_agent(command: &str, args: &[&str], tool_parse_mode: &str) -> AcpAgentConfig {
    AcpAgentConfig {
        enabled: true,
        command: command.to_owned(),
        args: args.iter().map(|argument| (*argument).to_owned()).collect(),
        tool_parse_mode: tool_parse_mode.to_owned(),
        ..AcpAgentConfig::default()
    }
}

fn merge_default_agents(agents: &mut BTreeMap<String, AcpAgentConfig>) {
    for (name, config) in default_acp_agents() {
        agents.entry(name).or_insert(config);
    }
}

fn validate_acp_config(config: &AcpConfig, validate_modes: bool) -> Result<(), ApiError> {
    validate_node_path(&config.node_path)?;
    if config.agents.len() > MAX_ACP_AGENTS {
        return Err(payload_too_large("Too many ACP agents are configured"));
    }
    for (name, agent) in &config.agents {
        validate_agent_name(name)?;
        validate_acp_agent(agent, validate_modes)?;
    }
    Ok(())
}

fn validate_acp_agent(config: &AcpAgentConfig, validate_mode: bool) -> Result<(), ApiError> {
    if config.command.len() > MAX_ACP_COMMAND_BYTES || config.command.chars().any(char::is_control)
    {
        return Err(bad_request("ACP command is invalid"));
    }
    if config.args.len() > MAX_ACP_ARGUMENTS || config.env.len() > MAX_ACP_ENVIRONMENT_VARIABLES {
        return Err(payload_too_large("ACP agent config has too many entries"));
    }
    if config
        .args
        .iter()
        .any(|value| value.len() > MAX_ACP_STRING_BYTES || value.contains('\0'))
        || config.env.iter().any(|(key, value)| {
            key.is_empty()
                || key.len() > MAX_ACP_STRING_BYTES
                || key.contains(['=', '\0'])
                || value.len() > MAX_ACP_STRING_BYTES
                || value.contains('\0')
        })
    {
        return Err(bad_request(
            "ACP agent arguments or environment are invalid",
        ));
    }
    if config.stdio_buffer_limit_bytes == 0 {
        return Err(bad_request(
            "stdio_buffer_limit_bytes must be greater than zero",
        ));
    }
    if validate_mode
        && !matches!(
            config.tool_parse_mode.as_str(),
            "call_title" | "update_detail" | "call_detail"
        )
    {
        return Err(bad_request(
            "Invalid tool_parse_mode. Allowed values: call_detail, call_title, update_detail",
        ));
    }
    Ok(())
}

fn validate_agent_name(name: &str) -> Result<&str, ApiError> {
    let name = name.trim();
    if name.is_empty()
        || name.len() > MAX_ACP_AGENT_NAME_BYTES
        || name
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\'))
    {
        return Err(bad_request("ACP agent name is invalid"));
    }
    Ok(name)
}

fn validate_node_path(path: &str) -> Result<(), ApiError> {
    if path.len() > MAX_NODE_PATH_BYTES || path.chars().any(char::is_control) {
        Err(bad_request("Node path is invalid"))
    } else {
        Ok(())
    }
}

async fn node_runtime_status(configured: &str) -> NodeRuntimeStatus {
    let mut candidates = Vec::new();
    if let Ok(bundled) = env::var(DESKTOP_NODE_RUNTIME_ENV)
        && !bundled.trim().is_empty()
    {
        append_unique(
            &mut candidates,
            resolve_node_runtime(&bundled, "bundled", "bundled").await,
        );
    }
    match find_executable_in_path("node") {
        Some(node) => {
            append_unique(
                &mut candidates,
                resolve_node_runtime(&node.to_string_lossy(), "system", "system").await,
            );
        }
        None => candidates.push(NodeRuntimeCandidate::unavailable(
            "system",
            "system",
            "system_node_missing",
            "System Node was not detected",
        )),
    }
    if !configured.is_empty()
        && !candidates
            .iter()
            .any(|candidate| same_path(configured, &candidate.node_path))
    {
        append_unique(
            &mut candidates,
            resolve_node_runtime(configured, "custom", "custom").await,
        );
    }
    let effective_node_path = if configured.is_empty() {
        None
    } else {
        candidates
            .iter()
            .find(|candidate| candidate.available && same_path(configured, &candidate.node_path))
            .map(|candidate| candidate.node_path.clone())
    }
    .or_else(|| {
        ["bundled", "system"].into_iter().find_map(|key| {
            candidates
                .iter()
                .find(|candidate| candidate.key == key && candidate.available)
                .map(|candidate| candidate.node_path.clone())
        })
    })
    .unwrap_or_default();
    NodeRuntimeStatus {
        node_path: configured.to_owned(),
        effective_node_path,
        candidates,
    }
}

async fn resolve_node_runtime(value: &str, key: &str, label: &str) -> NodeRuntimeCandidate {
    let node = normalize_node_path(value);
    let mut candidate = NodeRuntimeCandidate::unavailable(key, label, "", "");
    candidate.node_path = node.to_string_lossy().into_owned();
    if !node.is_file() {
        candidate.reason_code = String::from("node_missing");
        candidate.reason = String::from("Node path does not exist");
        return candidate;
    }
    let (node_version, error) = executable_version(&node).await;
    if let Some(error) = error {
        candidate.reason_code = String::from("version_check_failed");
        candidate.reason = error;
        return candidate;
    }
    candidate.node_version = node_version;
    let Some(npx) = npx_path(&node) else {
        candidate.reason_code = String::from("npx_missing");
        candidate.reason = String::from("npx was not found");
        return candidate;
    };
    candidate.npx_path = npx.to_string_lossy().into_owned();
    let (npx_version, error) = executable_version(&npx).await;
    if let Some(error) = error {
        candidate.reason_code = String::from("version_check_failed");
        candidate.reason = error;
        return candidate;
    }
    candidate.npx_version = npx_version;
    candidate.available = true;
    candidate
}

async fn executable_version(path: &Path) -> (String, Option<String>) {
    let mut command = Command::new(path);
    command
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = match timeout(PROCESS_VERSION_TIMEOUT, command.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            return (
                String::new(),
                Some(format!(
                    "version check failed: {}",
                    bounded_error(&error.to_string())
                )),
            );
        }
        Err(_) => {
            return (String::new(), Some(String::from("version check timed out")));
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if output.status.success() {
        return (
            bounded_error(if stdout.is_empty() { &stderr } else { &stdout }),
            None,
        );
    }
    let detail = if stderr.is_empty() { stdout } else { stderr };
    (
        String::new(),
        Some(if detail.is_empty() {
            String::from("version check failed")
        } else {
            bounded_error(&detail)
        }),
    )
}

fn normalize_node_path(value: &str) -> PathBuf {
    let expanded = expand_home(value.trim());
    let path = expanded;
    if path.is_dir() {
        #[cfg(windows)]
        return path.join("node.exe");
        #[cfg(not(windows))]
        return path.join("bin").join("node");
    }
    path
}

fn expand_home(value: &str) -> PathBuf {
    if value == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(value));
    }
    if let Some(relative) = value
        .strip_prefix("~/")
        .or_else(|| value.strip_prefix("~\\"))
        && let Some(home) = dirs::home_dir()
    {
        return home.join(relative);
    }
    PathBuf::from(value)
}

fn find_executable_in_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for directory in env::split_paths(&path) {
        #[cfg(windows)]
        let names = [
            format!("{name}.exe"),
            format!("{name}.cmd"),
            name.to_owned(),
        ];
        #[cfg(not(windows))]
        let names = [name.to_owned()];
        for candidate_name in names {
            let candidate = directory.join(candidate_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn npx_path(node: &Path) -> Option<PathBuf> {
    let parent = node.parent()?;
    #[cfg(windows)]
    let names = ["npx.cmd", "npx.exe", "npx"];
    #[cfg(not(windows))]
    let names = ["npx"];
    names
        .into_iter()
        .map(|name| parent.join(name))
        .find(|candidate| candidate.is_file())
}

fn append_unique(candidates: &mut Vec<NodeRuntimeCandidate>, candidate: NodeRuntimeCandidate) {
    if !candidate.node_path.is_empty()
        && candidates
            .iter()
            .any(|existing| same_path(&candidate.node_path, &existing.node_path))
    {
        return;
    }
    candidates.push(candidate);
}

fn same_path(left: &str, right: &str) -> bool {
    if left.is_empty() || right.is_empty() {
        return false;
    }
    let left = absolute_path(&normalize_node_path(left));
    let right = absolute_path(&normalize_node_path(right));
    #[cfg(windows)]
    {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

fn absolute_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(path)
        }
    })
}

fn bounded_error(value: &str) -> String {
    value.chars().take(512).collect()
}

fn read_node_runtime_file(server: &AppServer) -> Result<NodeRuntimeFile, ApiError> {
    let path = node_runtime_file_path(server)?;
    let metadata = match fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(NodeRuntimeFile::default());
        }
        Err(_) => return Err(internal("ACP Node runtime config could not be inspected")),
    };
    if !metadata.is_file() || metadata.len() > NODE_RUNTIME_CONFIG_MAX_BYTES {
        return Err(internal("ACP Node runtime config is invalid"));
    }
    let bytes =
        fs::read(path).map_err(|_| internal("ACP Node runtime config could not be read"))?;
    let config = serde_json::from_slice::<NodeRuntimeFile>(&bytes)
        .map_err(|_| internal("ACP Node runtime config is invalid"))?;
    if config.schema_version != NODE_RUNTIME_SCHEMA_VERSION
        || validate_node_path(&config.node_path).is_err()
    {
        return Err(internal("ACP Node runtime config is invalid"));
    }
    Ok(config)
}

fn write_node_runtime_file(server: &AppServer, config: &NodeRuntimeFile) -> Result<(), ApiError> {
    let path = node_runtime_file_path(server)?;
    let parent = path
        .parent()
        .ok_or_else(|| internal("ACP Node runtime config path is invalid"))?;
    fs::create_dir_all(parent)
        .map_err(|_| internal("ACP Node runtime config directory could not be created"))?;
    let bytes = serde_json::to_vec_pretty(config)
        .map_err(|_| internal("ACP Node runtime config could not be encoded"))?;
    if bytes.len() as u64 > NODE_RUNTIME_CONFIG_MAX_BYTES {
        return Err(payload_too_large("ACP Node runtime config is too large"));
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|_| internal("ACP Node runtime config could not be staged"))?;
    temporary
        .write_all(&bytes)
        .and_then(|()| temporary.flush())
        .map_err(|_| internal("ACP Node runtime config could not be staged"))?;
    temporary
        .persist(path)
        .map_err(|_| internal("ACP Node runtime config could not be persisted"))?;
    Ok(())
}

fn node_runtime_file_path(server: &AppServer) -> Result<PathBuf, ApiError> {
    server
        .inner
        .desktop_workspace
        .as_ref()
        .map(|workspace| {
            workspace
                .data_dir
                .join(".qwenpaw-core")
                .join("acp-node-runtime.json")
        })
        .ok_or_else(|| internal("Desktop Workspace is unavailable"))
}

fn bad_request(detail: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, Json(json!({"detail": detail})))
}

fn not_found(detail: &str) -> ApiError {
    (StatusCode::NOT_FOUND, Json(json!({"detail": detail})))
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
    fn default_acp_config_matches_python_builtins() {
        let config = AcpConfig::default();
        assert_eq!(
            config.agents.keys().map(String::as_str).collect::<Vec<_>>(),
            vec!["claude_code", "codex", "opencode", "qwen_code"]
        );
        assert_eq!(
            config.agents["codex"],
            enabled_acp_agent("npx", &["-y", "@zed-industries/codex-acp"], "call_detail")
        );
    }

    #[test]
    fn validates_acp_agent_names_modes_and_limits() {
        assert_eq!(validate_agent_name(" custom ").expect("valid"), "custom");
        assert!(validate_agent_name("../custom").is_err());
        let config = AcpAgentConfig {
            tool_parse_mode: String::from("unknown"),
            ..AcpAgentConfig::default()
        };
        assert!(validate_acp_agent(&config, true).is_err());
        assert!(validate_acp_agent(&config, false).is_ok());
        assert!(
            validate_acp_agent(
                &AcpAgentConfig {
                    stdio_buffer_limit_bytes: 0,
                    ..AcpAgentConfig::default()
                },
                false
            )
            .is_err()
        );
    }

    #[test]
    fn compares_directory_and_executable_node_paths() {
        let directory = tempfile::tempdir().expect("temp directory");
        let bin = directory.path().join("bin");
        fs::create_dir_all(&bin).expect("bin directory");
        let node = bin.join("node");
        fs::write(&node, b"node").expect("node file");
        assert!(same_path(
            &directory.path().to_string_lossy(),
            &node.to_string_lossy()
        ));
    }
}
