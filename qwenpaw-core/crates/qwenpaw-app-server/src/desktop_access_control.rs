//! Persistent channel access-control contracts for the unchanged Console.

use std::collections::BTreeMap;

use axum::Json;
use axum::Router;
use axum::extract::Path;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::routing::post;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;

use super::AppServer;

const MAX_ACCESS_CONTROL_BYTES: usize = 1_048_576;
const MAX_CHANNELS: usize = 128;
const MAX_USERS: usize = 10_000;
const MAX_BATCH_ENTRIES: usize = 1_000;
const MAX_IDENTIFIER_BYTES: usize = 4_096;
const MAX_METADATA_BYTES: usize = 16_384;

type ApiError = (StatusCode, Json<Value>);

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route("/api/access-control", get(list_access_control))
        .route("/api/access-control/pending/all", get(list_all_pending))
        .route("/api/access-control/pending/approve", post(approve_pending))
        .route("/api/access-control/pending/deny", post(deny_pending))
        .route("/api/access-control/pending/dismiss", post(dismiss_pending))
        .route(
            "/api/access-control/pending/remark",
            post(update_pending_remark),
        )
        .route("/api/access-control/whitelist/add", post(add_to_whitelist))
        .route(
            "/api/access-control/whitelist/remove",
            post(remove_from_whitelist),
        )
        .route("/api/access-control/blacklist/add", post(add_to_blacklist))
        .route(
            "/api/access-control/blacklist/remove",
            post(remove_from_blacklist),
        )
        .route("/api/access-control/remark", post(update_remark))
        .route("/api/access-control/username", post(update_username))
        .route("/api/access-control/{channel}", get(get_channel_acl))
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct UserInfo {
    #[serde(default)]
    remark: String,
    #[serde(default)]
    username: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PendingEntry {
    user_id: String,
    channel: String,
    timestamp: f64,
    #[serde(default)]
    first_message: String,
    #[serde(default)]
    remark: String,
    #[serde(default)]
    username: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct ChannelAccessControl {
    #[serde(default)]
    whitelist: BTreeMap<String, UserInfo>,
    #[serde(default)]
    blacklist: BTreeMap<String, UserInfo>,
    #[serde(default)]
    pending: Vec<PendingEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
struct AccessControlData {
    version: u32,
    channels: BTreeMap<String, ChannelAccessControl>,
}

impl Default for AccessControlData {
    fn default() -> Self {
        Self {
            version: 1,
            channels: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ActionEntry {
    channel: String,
    user_id: String,
    #[serde(default)]
    remark: String,
    #[serde(default)]
    username: String,
}

#[derive(Debug, Deserialize)]
struct ActionBody {
    entries: Vec<ActionEntry>,
}

#[derive(Debug, Deserialize)]
struct RemarkBody {
    channel: String,
    user_id: String,
    remark: String,
}

#[derive(Debug, Deserialize)]
struct UsernameBody {
    channel: String,
    user_id: String,
    username: String,
}

async fn list_access_control(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_access_control_lock.lock().await;
    let mut channels = read_data(&server)?.channels;
    channels.retain(|channel, data| channel == "console" || !is_empty(data));
    channels.entry(String::from("console")).or_default();
    json_value(channels).map(Json)
}

async fn get_channel_acl(
    State(server): State<AppServer>,
    Path(channel): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_identifier("channel", &channel)?;
    let _guard = server.inner.desktop_access_control_lock.lock().await;
    let data = read_data(&server)?;
    json_value(data.channels.get(&channel).cloned().unwrap_or_default()).map(Json)
}

async fn list_all_pending(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_access_control_lock.lock().await;
    let data = read_data(&server)?;
    let mut pending = data
        .channels
        .into_values()
        .flat_map(|channel| channel.pending)
        .collect::<Vec<_>>();
    pending.sort_by(|left, right| right.timestamp.total_cmp(&left.timestamp));
    json_value(pending).map(Json)
}

async fn add_to_whitelist(
    State(server): State<AppServer>,
    Json(body): Json<ActionBody>,
) -> Result<Json<Value>, ApiError> {
    validate_action_body(&body)?;
    let count = body.entries.len();
    let _guard = server.inner.desktop_access_control_lock.lock().await;
    let mut data = read_data(&server)?;
    for entry in body.entries {
        let channel = data.channels.entry(entry.channel.clone()).or_default();
        let existing = channel.whitelist.get(&entry.user_id);
        channel.whitelist.insert(
            entry.user_id.clone(),
            UserInfo {
                remark: preserve_if_empty(&entry.remark, existing.map(|item| &item.remark)),
                username: preserve_if_empty(&entry.username, existing.map(|item| &item.username)),
            },
        );
        channel.blacklist.remove(&entry.user_id);
        remove_pending(channel, &entry.user_id, &entry.channel);
    }
    persist(&server, &data)?;
    Ok(action_response(count))
}

async fn remove_from_whitelist(
    State(server): State<AppServer>,
    Json(body): Json<ActionBody>,
) -> Result<Json<Value>, ApiError> {
    validate_action_body(&body)?;
    let count = body.entries.len();
    let _guard = server.inner.desktop_access_control_lock.lock().await;
    let mut data = read_data(&server)?;
    for entry in body.entries {
        data.channels
            .entry(entry.channel)
            .or_default()
            .whitelist
            .remove(&entry.user_id);
    }
    persist(&server, &data)?;
    Ok(action_response(count))
}

async fn add_to_blacklist(
    State(server): State<AppServer>,
    Json(body): Json<ActionBody>,
) -> Result<Json<Value>, ApiError> {
    validate_action_body(&body)?;
    let count = body.entries.len();
    let _guard = server.inner.desktop_access_control_lock.lock().await;
    let mut data = read_data(&server)?;
    for entry in body.entries {
        let channel = data.channels.entry(entry.channel.clone()).or_default();
        let existing = channel.blacklist.get(&entry.user_id);
        channel.blacklist.insert(
            entry.user_id.clone(),
            UserInfo {
                remark: preserve_if_empty(&entry.remark, existing.map(|item| &item.remark)),
                username: preserve_if_empty(&entry.username, existing.map(|item| &item.username)),
            },
        );
        channel.whitelist.remove(&entry.user_id);
        remove_pending(channel, &entry.user_id, &entry.channel);
    }
    persist(&server, &data)?;
    Ok(action_response(count))
}

async fn remove_from_blacklist(
    State(server): State<AppServer>,
    Json(body): Json<ActionBody>,
) -> Result<Json<Value>, ApiError> {
    validate_action_body(&body)?;
    let count = body.entries.len();
    let _guard = server.inner.desktop_access_control_lock.lock().await;
    let mut data = read_data(&server)?;
    for entry in body.entries {
        data.channels
            .entry(entry.channel)
            .or_default()
            .blacklist
            .remove(&entry.user_id);
    }
    persist(&server, &data)?;
    Ok(action_response(count))
}

async fn approve_pending(
    State(server): State<AppServer>,
    Json(body): Json<ActionBody>,
) -> Result<Json<Value>, ApiError> {
    move_pending(server, body, true).await
}

async fn deny_pending(
    State(server): State<AppServer>,
    Json(body): Json<ActionBody>,
) -> Result<Json<Value>, ApiError> {
    move_pending(server, body, false).await
}

async fn move_pending(
    server: AppServer,
    body: ActionBody,
    approve: bool,
) -> Result<Json<Value>, ApiError> {
    validate_action_body(&body)?;
    let count = body.entries.len();
    let _guard = server.inner.desktop_access_control_lock.lock().await;
    let mut data = read_data(&server)?;
    for entry in body.entries {
        let channel = data.channels.entry(entry.channel.clone()).or_default();
        let pending = channel
            .pending
            .iter()
            .find(|item| item.user_id == entry.user_id && item.channel == entry.channel)
            .cloned();
        remove_pending(channel, &entry.user_id, &entry.channel);
        let user = UserInfo {
            remark: preserve_if_empty(&entry.remark, pending.as_ref().map(|item| &item.remark)),
            username: pending.map_or_else(String::new, |item| item.username),
        };
        if approve {
            channel.whitelist.insert(entry.user_id.clone(), user);
            channel.blacklist.remove(&entry.user_id);
        } else {
            channel.blacklist.insert(entry.user_id.clone(), user);
            channel.whitelist.remove(&entry.user_id);
        }
    }
    persist(&server, &data)?;
    Ok(action_response(count))
}

async fn dismiss_pending(
    State(server): State<AppServer>,
    Json(body): Json<ActionBody>,
) -> Result<Json<Value>, ApiError> {
    validate_action_body(&body)?;
    let count = body.entries.len();
    let _guard = server.inner.desktop_access_control_lock.lock().await;
    let mut data = read_data(&server)?;
    for entry in body.entries {
        let channel = data.channels.entry(entry.channel.clone()).or_default();
        remove_pending(channel, &entry.user_id, &entry.channel);
    }
    persist(&server, &data)?;
    Ok(action_response(count))
}

async fn update_remark(
    State(server): State<AppServer>,
    Json(body): Json<RemarkBody>,
) -> Result<Json<Value>, ApiError> {
    validate_update(&body.channel, &body.user_id, &body.remark)?;
    let _guard = server.inner.desktop_access_control_lock.lock().await;
    let mut data = read_data(&server)?;
    let channel = data.channels.entry(body.channel).or_default();
    let found = channel
        .whitelist
        .get_mut(&body.user_id)
        .or_else(|| channel.blacklist.get_mut(&body.user_id));
    let Some(user) = found else {
        return Err(not_found("User not found in any list"));
    };
    user.remark = body.remark;
    persist(&server, &data)?;
    Ok(ok_response())
}

async fn update_username(
    State(server): State<AppServer>,
    Json(body): Json<UsernameBody>,
) -> Result<Json<Value>, ApiError> {
    validate_update(&body.channel, &body.user_id, &body.username)?;
    let _guard = server.inner.desktop_access_control_lock.lock().await;
    let mut data = read_data(&server)?;
    let channel = data.channels.entry(body.channel.clone()).or_default();
    let mut found = false;
    if let Some(user) = channel.whitelist.get_mut(&body.user_id) {
        user.username.clone_from(&body.username);
        found = true;
    }
    if let Some(user) = channel.blacklist.get_mut(&body.user_id) {
        user.username.clone_from(&body.username);
        found = true;
    }
    for pending in &mut channel.pending {
        if pending.user_id == body.user_id && pending.channel == body.channel {
            pending.username.clone_from(&body.username);
            found = true;
        }
    }
    if !found {
        return Err(not_found("User not found in any list"));
    }
    persist(&server, &data)?;
    Ok(ok_response())
}

async fn update_pending_remark(
    State(server): State<AppServer>,
    Json(body): Json<RemarkBody>,
) -> Result<Json<Value>, ApiError> {
    validate_update(&body.channel, &body.user_id, &body.remark)?;
    let _guard = server.inner.desktop_access_control_lock.lock().await;
    let mut data = read_data(&server)?;
    let channel = data.channels.entry(body.channel.clone()).or_default();
    let pending = channel
        .pending
        .iter_mut()
        .find(|item| item.user_id == body.user_id && item.channel == body.channel)
        .ok_or_else(|| not_found("Pending entry not found"))?;
    pending.remark = body.remark;
    persist(&server, &data)?;
    Ok(ok_response())
}

fn read_data(server: &AppServer) -> Result<AccessControlData, ApiError> {
    let Some(serialized) = server
        .inner
        .core
        .read_access_control_data()
        .map_err(internal)?
    else {
        return Ok(AccessControlData::default());
    };
    if serialized.len() > MAX_ACCESS_CONTROL_BYTES {
        return Err(internal(
            "stored access-control data exceeds its size limit",
        ));
    }
    let data = serde_json::from_str::<AccessControlData>(&serialized)
        .map_err(|_| internal("stored access-control data is invalid"))?;
    validate_stored_data(&data)?;
    Ok(data)
}

fn persist(server: &AppServer, data: &AccessControlData) -> Result<(), ApiError> {
    validate_stored_data(data)?;
    let serialized =
        serde_json::to_string(data).map_err(|_| internal("access-control data is invalid"))?;
    if serialized.len() > MAX_ACCESS_CONTROL_BYTES {
        return Err(unprocessable("access-control data exceeds its size limit"));
    }
    server
        .inner
        .core
        .write_access_control_data(&serialized)
        .map_err(internal)
}

fn validate_stored_data(data: &AccessControlData) -> Result<(), ApiError> {
    if data.version != 1 || data.channels.len() > MAX_CHANNELS {
        return Err(internal(
            "stored access-control data has an unsupported shape",
        ));
    }
    let users = data
        .channels
        .values()
        .map(|channel| channel.whitelist.len() + channel.blacklist.len() + channel.pending.len())
        .sum::<usize>();
    if users > MAX_USERS {
        return Err(internal("stored access-control data has too many users"));
    }
    Ok(())
}

fn validate_action_body(body: &ActionBody) -> Result<(), ApiError> {
    if body.entries.len() > MAX_BATCH_ENTRIES {
        return Err(unprocessable("too many access-control entries"));
    }
    for entry in &body.entries {
        validate_identifier("channel", &entry.channel)?;
        validate_identifier("user_id", &entry.user_id)?;
        validate_metadata("remark", &entry.remark)?;
        validate_metadata("username", &entry.username)?;
    }
    Ok(())
}

fn validate_update(channel: &str, user_id: &str, value: &str) -> Result<(), ApiError> {
    validate_identifier("channel", channel)?;
    validate_identifier("user_id", user_id)?;
    validate_metadata("metadata", value)
}

fn validate_identifier(label: &str, value: &str) -> Result<(), ApiError> {
    if value.trim().is_empty()
        || value.len() > MAX_IDENTIFIER_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(unprocessable(&format!("{label} is invalid")));
    }
    Ok(())
}

fn validate_metadata(label: &str, value: &str) -> Result<(), ApiError> {
    if value.len() > MAX_METADATA_BYTES || value.contains('\0') {
        return Err(unprocessable(&format!("{label} is invalid")));
    }
    Ok(())
}

fn remove_pending(channel: &mut ChannelAccessControl, user_id: &str, channel_name: &str) {
    channel
        .pending
        .retain(|item| item.user_id != user_id || item.channel != channel_name);
}

fn preserve_if_empty(value: &str, previous: Option<&String>) -> String {
    if value.is_empty() {
        previous.cloned().unwrap_or_default()
    } else {
        value.to_owned()
    }
}

fn is_empty(channel: &ChannelAccessControl) -> bool {
    channel.whitelist.is_empty() && channel.blacklist.is_empty() && channel.pending.is_empty()
}

fn action_response(count: usize) -> Json<Value> {
    Json(json!({"status": "ok", "count": count}))
}

fn ok_response() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

fn json_value<T: Serialize>(value: T) -> Result<Value, ApiError> {
    serde_json::to_value(value)
        .map_err(|_| internal("access-control response could not be serialized"))
}

fn unprocessable(detail: &str) -> ApiError {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(json!({"detail": detail})),
    )
}

fn not_found(detail: &str) -> ApiError {
    (StatusCode::NOT_FOUND, Json(json!({"detail": detail})))
}

fn internal(error_value: impl std::fmt::Display) -> ApiError {
    tracing::warn!(error = %error_value, "Desktop access-control persistence failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"detail": "Desktop access-control data could not be persisted"})),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_existing_metadata_when_an_add_omits_it() {
        let previous = String::from("existing");
        assert_eq!(preserve_if_empty("", Some(&previous)), "existing");
        assert_eq!(
            preserve_if_empty("replacement", Some(&previous)),
            "replacement"
        );
    }
}
