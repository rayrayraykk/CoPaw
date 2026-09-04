//! Persistent mail access-control contracts for the unchanged Console.

use std::collections::BTreeMap;

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::routing::post;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;

use super::AppServer;

const DEFAULT_AGENT_ID: &str = "default";
const MAX_DATA_BYTES: usize = 2_097_152;
const MAX_AGENTS: usize = 128;
const MAX_ENTRIES: usize = 10_000;
const MAX_BATCH_ENTRIES: usize = 1_000;
const MAX_ADDRESS_BYTES: usize = 4_096;
const MAX_METADATA_BYTES: usize = 16_384;

type ApiError = (StatusCode, Json<Value>);

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route("/api/mail-access-control", get(list_access_control))
        .route("/api/mail-access-control/agents", get(list_agents))
        .route(
            "/api/mail-access-control/pending/all",
            get(list_all_pending),
        )
        .route("/api/mail-access-control/pending/count", get(pending_count))
        .route(
            "/api/mail-access-control/pending/approve",
            post(approve_pending),
        )
        .route("/api/mail-access-control/pending/deny", post(deny_pending))
        .route(
            "/api/mail-access-control/pending/dismiss",
            post(dismiss_pending),
        )
        .route(
            "/api/mail-access-control/pending/remark",
            post(update_pending_remark),
        )
        .route(
            "/api/mail-access-control/whitelist/add",
            post(add_to_whitelist),
        )
        .route(
            "/api/mail-access-control/whitelist/remove",
            post(remove_from_whitelist),
        )
        .route(
            "/api/mail-access-control/blacklist/add",
            post(add_to_blacklist),
        )
        .route(
            "/api/mail-access-control/blacklist/remove",
            post(remove_from_blacklist),
        )
        .route("/api/mail-access-control/remark", post(update_remark))
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct MailUserInfo {
    #[serde(default)]
    remark: String,
    #[serde(default)]
    display_name: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct MailPendingEntry {
    sender_address: String,
    agent_id: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    subject: String,
    #[serde(default)]
    body_preview: String,
    #[serde(default)]
    timestamp: f64,
    #[serde(default)]
    remark: String,
    #[serde(default)]
    uid: i64,
    #[serde(default)]
    date: String,
    #[serde(default)]
    messages: Vec<Value>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct AgentMailAccessControl {
    #[serde(default)]
    whitelist: BTreeMap<String, MailUserInfo>,
    #[serde(default)]
    blacklist: BTreeMap<String, MailUserInfo>,
    #[serde(default)]
    pending: Vec<MailPendingEntry>,
    #[serde(default)]
    approved_replay: Vec<MailPendingEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
struct MailAccessControlData {
    version: u32,
    agents: BTreeMap<String, AgentMailAccessControl>,
}

impl Default for MailAccessControlData {
    fn default() -> Self {
        Self {
            version: 1,
            agents: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ActionEntry {
    #[serde(default)]
    agent_id: String,
    address: String,
    #[serde(default)]
    remark: String,
    #[serde(default)]
    display_name: String,
}

#[derive(Debug, Deserialize)]
struct ActionBody {
    entries: Vec<ActionEntry>,
}

#[derive(Debug, Deserialize)]
struct RemarkBody {
    agent_id: String,
    address: String,
    remark: String,
}

async fn list_agents() -> Json<Value> {
    Json(json!({"agents": [DEFAULT_AGENT_ID]}))
}

async fn list_access_control(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_mail_access_control_lock.lock().await;
    let mut agents = read_data(&server)?.agents;
    agents.entry(String::from(DEFAULT_AGENT_ID)).or_default();
    json_value(agents).map(Json)
}

async fn list_all_pending(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_mail_access_control_lock.lock().await;
    let mut pending = read_data(&server)?
        .agents
        .into_values()
        .flat_map(|agent| agent.pending)
        .collect::<Vec<_>>();
    pending.sort_by(|left, right| right.timestamp.total_cmp(&left.timestamp));
    json_value(pending).map(Json)
}

async fn pending_count(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_mail_access_control_lock.lock().await;
    let count = read_data(&server)?
        .agents
        .values()
        .map(|agent| agent.pending.len())
        .sum::<usize>();
    Ok(Json(json!({"count": count})))
}

async fn add_to_whitelist(
    State(server): State<AppServer>,
    Json(body): Json<ActionBody>,
) -> Result<Json<Value>, ApiError> {
    mutate_list(server, body, ListMutation::WhitelistAdd).await
}

async fn remove_from_whitelist(
    State(server): State<AppServer>,
    Json(body): Json<ActionBody>,
) -> Result<Json<Value>, ApiError> {
    mutate_list(server, body, ListMutation::WhitelistRemove).await
}

async fn add_to_blacklist(
    State(server): State<AppServer>,
    Json(body): Json<ActionBody>,
) -> Result<Json<Value>, ApiError> {
    mutate_list(server, body, ListMutation::BlacklistAdd).await
}

async fn remove_from_blacklist(
    State(server): State<AppServer>,
    Json(body): Json<ActionBody>,
) -> Result<Json<Value>, ApiError> {
    mutate_list(server, body, ListMutation::BlacklistRemove).await
}

#[derive(Clone, Copy)]
enum ListMutation {
    WhitelistAdd,
    WhitelistRemove,
    BlacklistAdd,
    BlacklistRemove,
}

impl ListMutation {
    const fn validates_address(self) -> bool {
        matches!(self, Self::WhitelistAdd | Self::BlacklistAdd)
    }

    const fn broadcasts(self) -> bool {
        self.validates_address()
    }
}

async fn mutate_list(
    server: AppServer,
    body: ActionBody,
    mutation: ListMutation,
) -> Result<Json<Value>, ApiError> {
    validate_action_body(&body, mutation.validates_address())?;
    let _guard = server.inner.desktop_mail_access_control_lock.lock().await;
    let mut data = read_data(&server)?;
    let mut count = 0;
    for entry in body.entries {
        let address = normalize_address(&entry.address);
        for agent_id in target_agents(&entry.agent_id, mutation.broadcasts()) {
            let agent = data.agents.entry(agent_id).or_default();
            match mutation {
                ListMutation::WhitelistAdd => add_whitelist(agent, &entry, &address),
                ListMutation::WhitelistRemove => {
                    agent.whitelist.remove(&address);
                }
                ListMutation::BlacklistAdd => add_blacklist(agent, &entry, &address),
                ListMutation::BlacklistRemove => {
                    agent.blacklist.remove(&address);
                }
            }
            count += 1;
        }
    }
    persist(&server, &data)?;
    Ok(action_response(count))
}

fn add_whitelist(agent: &mut AgentMailAccessControl, entry: &ActionEntry, address: &str) {
    let existing = agent.whitelist.get(address);
    agent.whitelist.insert(
        String::from(address),
        MailUserInfo {
            remark: preserve_if_empty(&entry.remark, existing.map(|item| &item.remark)),
            display_name: preserve_if_empty(
                &entry.display_name,
                existing.map(|item| &item.display_name),
            ),
        },
    );
    agent.blacklist.remove(address);
    agent.pending.retain(|item| item.sender_address != address);
}

fn add_blacklist(agent: &mut AgentMailAccessControl, entry: &ActionEntry, address: &str) {
    let existing = agent.blacklist.get(address);
    agent.blacklist.insert(
        String::from(address),
        MailUserInfo {
            remark: preserve_if_empty(&entry.remark, existing.map(|item| &item.remark)),
            display_name: preserve_if_empty(
                &entry.display_name,
                existing.map(|item| &item.display_name),
            ),
        },
    );
    agent.whitelist.remove(address);
    agent.pending.retain(|item| item.sender_address != address);
    agent
        .approved_replay
        .retain(|item| item.sender_address != address);
}

async fn approve_pending(
    State(server): State<AppServer>,
    Json(body): Json<ActionBody>,
) -> Result<Json<Value>, ApiError> {
    move_pending(server, body, PendingAction::Approve).await
}

async fn deny_pending(
    State(server): State<AppServer>,
    Json(body): Json<ActionBody>,
) -> Result<Json<Value>, ApiError> {
    move_pending(server, body, PendingAction::Deny).await
}

async fn dismiss_pending(
    State(server): State<AppServer>,
    Json(body): Json<ActionBody>,
) -> Result<Json<Value>, ApiError> {
    move_pending(server, body, PendingAction::Dismiss).await
}

#[derive(Clone, Copy)]
enum PendingAction {
    Approve,
    Deny,
    Dismiss,
}

async fn move_pending(
    server: AppServer,
    body: ActionBody,
    action: PendingAction,
) -> Result<Json<Value>, ApiError> {
    validate_action_body(&body, !matches!(action, PendingAction::Dismiss))?;
    let mail_access_control_guard = server.inner.desktop_mail_access_control_lock.lock().await;
    let mut data = read_data(&server)?;
    let mut count = 0;
    let mut inbox_read_targets = Vec::new();
    for entry in body.entries {
        let address = normalize_address(&entry.address);
        for agent_id in target_agents(&entry.agent_id, false) {
            let agent = data.agents.entry(agent_id.clone()).or_default();
            let pending = take_pending(agent, &address);
            match action {
                PendingAction::Approve => approve(agent, &entry, &address, pending),
                PendingAction::Deny => deny(agent, &entry, &address, pending.as_ref()),
                PendingAction::Dismiss => {}
            }
            inbox_read_targets.push((agent_id, address.clone()));
            count += 1;
        }
    }
    persist(&server, &data)?;
    drop(mail_access_control_guard);
    for (agent_id, address) in inbox_read_targets {
        super::desktop_inbox::mark_read_by_acl_sender(&server, &agent_id, &address).await?;
    }
    Ok(action_response(count))
}

fn approve(
    agent: &mut AgentMailAccessControl,
    entry: &ActionEntry,
    address: &str,
    pending: Option<MailPendingEntry>,
) {
    let remark = preserve_if_empty(&entry.remark, pending.as_ref().map(|item| &item.remark));
    let display_name = pending
        .as_ref()
        .map_or_else(String::new, |item| item.display_name.clone());
    if let Some(pending) = pending.filter(has_replayable_message) {
        merge_replay(&mut agent.approved_replay, pending);
    }
    agent.whitelist.insert(
        String::from(address),
        MailUserInfo {
            remark,
            display_name,
        },
    );
    agent.blacklist.remove(address);
}

fn deny(
    agent: &mut AgentMailAccessControl,
    entry: &ActionEntry,
    address: &str,
    pending: Option<&MailPendingEntry>,
) {
    let remark = preserve_if_empty(&entry.remark, pending.as_ref().map(|item| &item.remark));
    let display_name = pending
        .as_ref()
        .map_or_else(String::new, |item| item.display_name.clone());
    agent.blacklist.insert(
        String::from(address),
        MailUserInfo {
            remark,
            display_name,
        },
    );
    agent.whitelist.remove(address);
    agent
        .approved_replay
        .retain(|item| item.sender_address != address);
}

fn take_pending(agent: &mut AgentMailAccessControl, address: &str) -> Option<MailPendingEntry> {
    let index = agent
        .pending
        .iter()
        .position(|item| item.sender_address == address)?;
    Some(agent.pending.remove(index))
}

fn has_replayable_message(entry: &MailPendingEntry) -> bool {
    entry.messages.iter().any(|message| {
        message
            .get("uid")
            .and_then(Value::as_i64)
            .is_some_and(|uid| uid != 0)
    }) || entry.uid != 0
}

fn merge_replay(replay: &mut Vec<MailPendingEntry>, mut pending: MailPendingEntry) {
    let Some(existing) = replay
        .iter_mut()
        .find(|item| item.sender_address == pending.sender_address)
    else {
        replay.push(pending);
        return;
    };
    let existing_uids = existing
        .messages
        .iter()
        .filter_map(|message| message.get("uid").and_then(Value::as_i64))
        .collect::<Vec<_>>();
    pending.messages.retain(|message| {
        message
            .get("uid")
            .and_then(Value::as_i64)
            .is_none_or(|uid| uid == 0 || !existing_uids.contains(&uid))
    });
    existing.messages.extend(pending.messages);
}

async fn update_pending_remark(
    State(server): State<AppServer>,
    Json(body): Json<RemarkBody>,
) -> Result<Json<Value>, ApiError> {
    validate_remark_body(&body)?;
    let _guard = server.inner.desktop_mail_access_control_lock.lock().await;
    let mut data = read_data(&server)?;
    let address = normalize_address(&body.address);
    let Some(agent) = data.agents.get_mut(&body.agent_id) else {
        return Err(not_found("Pending entry not found"));
    };
    let pending = agent
        .pending
        .iter_mut()
        .find(|item| item.sender_address == address)
        .ok_or_else(|| not_found("Pending entry not found"))?;
    pending.remark = body.remark;
    persist(&server, &data)?;
    Ok(ok_response())
}

async fn update_remark(
    State(server): State<AppServer>,
    Json(body): Json<RemarkBody>,
) -> Result<Json<Value>, ApiError> {
    validate_remark_body(&body)?;
    let _guard = server.inner.desktop_mail_access_control_lock.lock().await;
    let mut data = read_data(&server)?;
    let address = normalize_address(&body.address);
    let Some(agent) = data.agents.get_mut(&body.agent_id) else {
        return Err(not_found("Address not found in any list"));
    };
    let user = agent
        .whitelist
        .get_mut(&address)
        .or_else(|| agent.blacklist.get_mut(&address))
        .ok_or_else(|| not_found("Address not found in any list"))?;
    user.remark = body.remark;
    persist(&server, &data)?;
    Ok(ok_response())
}

fn target_agents(agent_id: &str, broadcast: bool) -> Vec<String> {
    if agent_id == DEFAULT_AGENT_ID || (broadcast && agent_id.is_empty()) {
        vec![String::from(DEFAULT_AGENT_ID)]
    } else {
        Vec::new()
    }
}

fn validate_action_body(body: &ActionBody, require_valid_address: bool) -> Result<(), ApiError> {
    if body.entries.len() > MAX_BATCH_ENTRIES {
        return Err(bad_request("too many mail access-control entries"));
    }
    for entry in &body.entries {
        validate_agent_id(&entry.agent_id)?;
        validate_address_shape(&entry.address)?;
        validate_metadata("remark", &entry.remark)?;
        validate_metadata("display_name", &entry.display_name)?;
        if require_valid_address {
            validate_mail_address(&entry.address)?;
        }
    }
    Ok(())
}

fn validate_remark_body(body: &RemarkBody) -> Result<(), ApiError> {
    validate_agent_id(&body.agent_id)?;
    validate_address_shape(&body.address)?;
    validate_metadata("remark", &body.remark)
}

fn validate_agent_id(agent_id: &str) -> Result<(), ApiError> {
    if agent_id.len() > MAX_ADDRESS_BYTES || agent_id.chars().any(char::is_control) {
        return Err(bad_request("agent_id is invalid"));
    }
    Ok(())
}

fn validate_address_shape(address: &str) -> Result<(), ApiError> {
    if address.len() > MAX_ADDRESS_BYTES || address.contains('\0') {
        return Err(bad_request("mail address is invalid"));
    }
    Ok(())
}

fn validate_metadata(label: &str, value: &str) -> Result<(), ApiError> {
    if value.len() > MAX_METADATA_BYTES || value.contains('\0') {
        return Err(bad_request(&format!("{label} is invalid")));
    }
    Ok(())
}

fn validate_mail_address(address: &str) -> Result<(), ApiError> {
    let normalized = normalize_address(address);
    let valid = if let Some(domain) = normalized.strip_prefix("*@") {
        valid_domain(domain)
    } else {
        let mut pieces = normalized.split('@');
        let local = pieces.next().unwrap_or_default();
        let domain = pieces.next().unwrap_or_default();
        !local.is_empty()
            && !local.chars().any(char::is_whitespace)
            && pieces.next().is_none()
            && valid_domain(domain)
    };
    if valid {
        Ok(())
    } else {
        Err(bad_request(
            "Invalid email address: expected 'user@domain' or a '*@domain' wildcard.",
        ))
    }
}

fn valid_domain(domain: &str) -> bool {
    domain.contains('.')
        && domain.split('.').all(|label| {
            !label.is_empty()
                && label
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
                && !label.starts_with('-')
                && !label.ends_with('-')
        })
}

fn normalize_address(address: &str) -> String {
    address.trim().to_ascii_lowercase()
}

fn preserve_if_empty(value: &str, previous: Option<&String>) -> String {
    if value.is_empty() {
        previous.cloned().unwrap_or_default()
    } else {
        String::from(value)
    }
}

fn read_data(server: &AppServer) -> Result<MailAccessControlData, ApiError> {
    let Some(serialized) = server
        .inner
        .core
        .read_mail_access_control_data()
        .map_err(internal)?
    else {
        return Ok(MailAccessControlData::default());
    };
    if serialized.len() > MAX_DATA_BYTES {
        return Err(internal(
            "stored mail access-control data exceeds its size limit",
        ));
    }
    let data = serde_json::from_str::<MailAccessControlData>(&serialized)
        .map_err(|_| internal("stored mail access-control data is invalid"))?;
    validate_stored_data(&data)?;
    Ok(data)
}

fn persist(server: &AppServer, data: &MailAccessControlData) -> Result<(), ApiError> {
    validate_stored_data(data)?;
    let serialized =
        serde_json::to_string(data).map_err(|_| internal("mail access-control data is invalid"))?;
    if serialized.len() > MAX_DATA_BYTES {
        return Err(bad_request(
            "mail access-control data exceeds its size limit",
        ));
    }
    server
        .inner
        .core
        .write_mail_access_control_data(&serialized)
        .map_err(internal)
}

fn validate_stored_data(data: &MailAccessControlData) -> Result<(), ApiError> {
    if data.version != 1 || data.agents.len() > MAX_AGENTS {
        return Err(internal(
            "stored mail access-control data has an unsupported shape",
        ));
    }
    let entries = data
        .agents
        .values()
        .map(|agent| {
            agent.whitelist.len()
                + agent.blacklist.len()
                + agent.pending.len()
                + agent.approved_replay.len()
        })
        .sum::<usize>();
    if entries > MAX_ENTRIES {
        return Err(internal(
            "stored mail access-control data has too many entries",
        ));
    }
    Ok(())
}

fn action_response(count: usize) -> Json<Value> {
    Json(json!({"status": "ok", "count": count}))
}

fn ok_response() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

fn json_value<T: Serialize>(value: T) -> Result<Value, ApiError> {
    serde_json::to_value(value)
        .map_err(|_| internal("mail access-control response could not be serialized"))
}

fn bad_request(detail: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, Json(json!({"detail": detail})))
}

fn not_found(detail: &str) -> ApiError {
    (StatusCode::NOT_FOUND, Json(json!({"detail": detail})))
}

fn internal(error_value: impl std::fmt::Display) -> ApiError {
    tracing::warn!(error = %error_value, "Desktop mail access-control persistence failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({
            "detail": "Desktop mail access-control data could not be persisted"
        })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_plain_and_wildcard_addresses() {
        for address in ["User@Example.com", "*@example.com"] {
            assert!(validate_mail_address(address).is_ok(), "{address}");
        }
        for address in [
            "",
            "user@localhost",
            "two@@example.com",
            "*@*",
            "*@-bad.com",
        ] {
            assert!(validate_mail_address(address).is_err(), "{address}");
        }
    }

    #[test]
    fn resolves_only_the_current_rust_agent() {
        assert_eq!(target_agents("default", false), vec!["default"]);
        assert_eq!(target_agents("", true), vec!["default"]);
        assert!(target_agents("missing", true).is_empty());
        assert!(target_agents("", false).is_empty());
    }
}
