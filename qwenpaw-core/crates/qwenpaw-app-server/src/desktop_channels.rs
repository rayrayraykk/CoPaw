//! Built-in channel configuration contracts for the unchanged Console.

use std::collections::BTreeMap;

use axum::Json;
use axum::Router;
use axum::extract::Path;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::routing::get;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;

use super::AppServer;
use super::desktop_agents;

#[path = "desktop_channel_identity.rs"]
mod identity;
pub(super) use identity::{filter_backup_data, merge_restore_data};

const MAX_CHANNEL_CONFIG_BYTES: usize = 262_144;
const CHANNEL_TYPES: [&str; 18] = [
    "imessage",
    "discord",
    "dingtalk",
    "feishu",
    "qq",
    "telegram",
    "mattermost",
    "mqtt",
    "console",
    "matrix",
    "slack",
    "voice",
    "sip",
    "wecom",
    "xiaoyi",
    "yuanbao",
    "wechat",
    "onebot",
];

type ApiError = (StatusCode, Json<Value>);

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route(
            "/api/config/channels",
            get(list_channels).put(update_channels),
        )
        .route("/api/config/channels/types", get(list_channel_types))
        .route("/api/config/channels/schemas", get(list_channel_schemas))
        .route(
            "/api/config/channels/{channel}/conflict-check",
            axum::routing::post(check_channel_conflict),
        )
        .route("/api/config/channels/{channel}/qrcode", get(channel_qrcode))
        .route(
            "/api/config/channels/{channel}/qrcode/status",
            get(channel_qrcode_status),
        )
        .route(
            "/api/config/channels/{channel}",
            get(get_channel).put(update_channel),
        )
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredChannelData {
    version: u32,
    workspaces: Vec<ChannelWorkspace>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ChannelWorkspace {
    data_key: qwenpaw_storage::WorkspaceDataKey,
    console: Option<ConsoleChannelConfig>,
}

impl Default for StoredChannelData {
    fn default() -> Self {
        Self {
            version: 2,
            workspaces: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools)]
struct ConsoleChannelConfig {
    enabled: bool,
    bot_prefix: String,
    show_tool_calls: bool,
    show_tool_results: bool,
    tool_call_max_length: u32,
    tool_result_max_length: u32,
    show_thinking: bool,
    dm_policy: AccessPolicy,
    group_policy: AccessPolicy,
    allow_from: Vec<String>,
    deny_message: String,
    require_mention: bool,
    no_text_debounce: bool,
    access_control_dm: bool,
    access_control_group: bool,
    dm_disabled: bool,
    group_disabled: bool,
    media_dir: Option<String>,
}

impl Default for ConsoleChannelConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bot_prefix: String::new(),
            show_tool_calls: true,
            show_tool_results: true,
            tool_call_max_length: 200,
            tool_result_max_length: 500,
            show_thinking: true,
            dm_policy: AccessPolicy::Open,
            group_policy: AccessPolicy::Open,
            allow_from: Vec::new(),
            deny_message: String::new(),
            require_mention: false,
            no_text_debounce: true,
            access_control_dm: false,
            access_control_group: false,
            dm_disabled: false,
            group_disabled: false,
            media_dir: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum AccessPolicy {
    #[default]
    Open,
    Allowlist,
}

async fn list_channels(
    State(server): State<AppServer>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let _lifecycle = server.inner.desktop_agent_lifecycle_lock.lock().await;
    let agent = desktop_agents::requested_agent_id(&headers)?;
    let context = desktop_agents::context_for_agent(&server, &agent).await?;
    let _guard = server.inner.desktop_channel_config_lock.lock().await;
    let stored = read_data(&server)?;
    if stored.console(&context.data_key).is_none() {
        return Ok(Json(json!(
            CHANNEL_TYPES
                .into_iter()
                .map(|name| (
                    name,
                    json!({"enabled":false,"bot_prefix":"","isBuiltin":true})
                ))
                .collect::<BTreeMap<_, _>>()
        )));
    }
    let mut result = default_channels();
    result.insert(
        String::from("console"),
        json_value(
            &stored
                .console(&context.data_key)
                .expect("configured channel"),
        )?,
    );
    for value in result.values_mut() {
        if let Some(object) = value.as_object_mut() {
            object.insert(String::from("isBuiltin"), Value::Bool(true));
        }
    }
    Ok(Json(Value::Object(result.into_iter().collect())))
}

async fn list_channel_types() -> Json<Value> {
    Json(json!(CHANNEL_TYPES))
}

async fn list_channel_schemas() -> Json<Value> {
    Json(json!({}))
}

async fn get_channel(
    State(server): State<AppServer>,
    Path(channel): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    ensure_known_channel(&channel)?;
    let _lifecycle = server.inner.desktop_agent_lifecycle_lock.lock().await;
    let agent = desktop_agents::requested_agent_id(&headers)?;
    let context = desktop_agents::context_for_agent(&server, &agent).await?;
    let _guard = server.inner.desktop_channel_config_lock.lock().await;
    let console = read_data(&server)?
        .console(&context.data_key)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"detail":format!("Channel '{channel}' not configured")})),
            )
        })?;
    if channel == "console" {
        return json_value(&console).map(Json);
    }
    Ok(Json(
        default_channel(&channel).expect("channel was validated"),
    ))
}

async fn update_channel(
    State(server): State<AppServer>,
    Path(channel): Path<String>,
    headers: HeaderMap,
    Json(value): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    ensure_known_channel(&channel)?;
    ensure_payload_size(&value)?;
    let _lifecycle = server.inner.desktop_agent_lifecycle_lock.lock().await;
    let agent = desktop_agents::requested_agent_id(&headers)?;
    let context = desktop_agents::context_for_agent(&server, &agent).await?;
    if channel != "console" {
        return Err(runtime_unavailable(&channel));
    }
    let config = parse_console(value)?;
    let _guard = server.inner.desktop_channel_config_lock.lock().await;
    let mut stored = read_data(&server)?;
    stored.set_console(context.data_key, Some(config.clone()));
    persist(&server, &stored)?;
    json_value(&config).map(Json)
}

async fn update_channels(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Json(value): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    ensure_payload_size(&value)?;
    let object = value
        .as_object()
        .ok_or_else(|| bad_request("Channel configuration must be an object"))?;
    for channel in object.keys() {
        ensure_known_channel(channel)?;
        if channel != "console" {
            return Err(runtime_unavailable(channel));
        }
    }
    let console = object
        .get("console")
        .cloned()
        .ok_or_else(|| bad_request("Channel configuration must include console"))?;
    let updated = update_channel(
        State(server),
        Path(String::from("console")),
        headers,
        Json(console),
    )
    .await?
    .0;
    let mut result = default_channels();
    result.insert(String::from("console"), updated);
    for value in result.values_mut() {
        if let Some(object) = value.as_object_mut() {
            object.remove("isBuiltin");
        }
    }
    Ok(Json(Value::Object(result.into_iter().collect())))
}

async fn check_channel_conflict(
    State(server): State<AppServer>,
    Path(channel): Path<String>,
    headers: HeaderMap,
    Json(value): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    ensure_known_channel(&channel)?;
    ensure_payload_size(&value)?;
    let _lifecycle = server.inner.desktop_agent_lifecycle_lock.lock().await;
    let agent = desktop_agents::requested_agent_id(&headers)?;
    desktop_agents::context_for_agent(&server, &agent).await?;
    Ok(Json(json!({"conflict": false, "agents": []})))
}

async fn channel_qrcode(Path(channel): Path<String>) -> Result<Json<Value>, ApiError> {
    ensure_known_channel(&channel)?;
    Err(runtime_unavailable(&channel))
}

async fn channel_qrcode_status(Path(channel): Path<String>) -> Result<Json<Value>, ApiError> {
    ensure_known_channel(&channel)?;
    Err(runtime_unavailable(&channel))
}

fn read_data(server: &AppServer) -> Result<StoredChannelData, ApiError> {
    server.ensure_agent_publication_available()?;
    let serialized = server
        .inner
        .core
        .read_channel_config_data()
        .map_err(|error| internal_error(&error.to_string()))?;
    let default_key = desktop_agents::default_data_key(server)?;
    identity::decode(serialized.as_deref(), Some(&default_key)).map_err(internal_error)
}

fn persist(server: &AppServer, data: &StoredChannelData) -> Result<(), ApiError> {
    let serialized = identity::encode(data).map_err(bad_request)?;
    server
        .inner
        .core
        .write_channel_config_data(&serialized)
        .map_err(|error| internal_error(&error.to_string()))
}

pub(super) fn profile_view(
    server: &AppServer,
    key: &qwenpaw_storage::WorkspaceDataKey,
    raw: &Value,
) -> Result<Value, ApiError> {
    let mut view = raw.clone();
    view["channels"] = profile_channels(&read_data(server)?, key)?;
    Ok(view)
}

fn profile_channels(
    data: &StoredChannelData,
    key: &qwenpaw_storage::WorkspaceDataKey,
) -> Result<Value, ApiError> {
    let Some(console) = data.console(key) else {
        return Ok(Value::Null);
    };
    let mut result: serde_json::Map<String, Value> = CHANNEL_TYPES
        .iter()
        .map(|name| {
            (
                String::from(*name),
                default_channel(name).expect("static channel"),
            )
        })
        .collect();
    result.insert(String::from("console"), json_value(&console)?);
    Ok(Value::Object(result))
}

pub(super) struct ProfileUpdate {
    serialized: Option<String>,
    pub(super) channels: Value,
}

impl ProfileUpdate {
    /// The caller holds the Channel lock and commits this after file publication.
    pub(super) fn commit(
        &self,
        server: &AppServer,
        publication: uuid::Uuid,
    ) -> Result<(), ApiError> {
        server
            .inner
            .core
            .commit_agent_publication(publication, self.serialized.as_deref())
            .map_err(|error| internal_error(&error.to_string()))
    }
}

pub(super) fn prepare_profile_update(
    server: &AppServer,
    key: &qwenpaw_storage::WorkspaceDataKey,
    raw: &Value,
    submitted: Option<&Value>,
) -> Result<ProfileUpdate, ApiError> {
    let mut data = read_data(server)?;
    let current = profile_channels(&data, key)?;
    let Some(submitted) = submitted else {
        return Ok(ProfileUpdate {
            serialized: None,
            channels: current,
        });
    };
    validate_profile_config(submitted)?;
    if raw.get("channels").is_some_and(|shadow| {
        (shadow.is_null() || shadow.as_object().is_some_and(|value| !value.is_empty()))
            && shadow != &current
    }) {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({"detail":
                "Stored Agent Channels conflict with Workspace configuration; original data was preserved"
            })),
        ));
    }
    let next = if submitted.is_null() {
        None
    } else {
        Some(parse_console(
            submitted
                .get("console")
                .cloned()
                .unwrap_or_else(|| json!({})),
        )?)
    };
    data.set_console(key.clone(), next);
    let channels = profile_channels(&data, key)?;
    let serialized = if channels == current {
        None
    } else {
        Some(identity::encode(&data).map_err(bad_request)?)
    };
    Ok(ProfileUpdate {
        serialized,
        channels,
    })
}

/// Validate a Profile submission before any Agent files or secrets are written.
/// This gate does not publish or resolve the separate Profile channel snapshot.
pub(super) fn validate_profile_config(value: &Value) -> Result<(), ApiError> {
    ensure_payload_size(value)?;
    if value.is_null() {
        return Ok(());
    }
    let object = value
        .as_object()
        .ok_or_else(|| bad_request("Channel configuration must be an object or null"))?;
    for (channel, config) in object {
        ensure_known_channel(channel)?;
        if channel == "console" {
            parse_console(config.clone())?;
        } else {
            let defaults = default_channel(channel).expect("channel was validated");
            // A full Profile round-trip includes disabled external defaults.
            // Only supplied fields identical to those defaults are inert.
            if !config.as_object().is_some_and(|fields| {
                fields.iter().all(|(name, value)| {
                    defaults.get(name).is_some_and(|expected| {
                        // JavaScript re-encodes integral floats as integers.
                        expected == value
                            || (expected.is_number()
                                && value.is_number()
                                && expected.as_f64() == value.as_f64())
                    })
                })
            }) {
                return Err(runtime_unavailable(channel));
            }
        }
    }
    Ok(())
}

fn parse_console(value: Value) -> Result<ConsoleChannelConfig, ApiError> {
    if !value.is_object() {
        return Err(bad_request(
            "Console channel configuration must be an object",
        ));
    }
    let mut config: ConsoleChannelConfig = serde_json::from_value(value)
        .map_err(|error| bad_request(&format!("Invalid Console channel configuration: {error}")))?;
    config.enabled = true;
    validate_console(&config)?;
    Ok(config)
}

fn validate_console(config: &ConsoleChannelConfig) -> Result<(), ApiError> {
    if config.bot_prefix.len() > 4_096
        || config.deny_message.len() > 16_384
        || config
            .media_dir
            .as_ref()
            .is_some_and(|value| value.len() > 16_384)
        || config.allow_from.len() > 10_000
        || config.allow_from.iter().any(|value| value.len() > 4_096)
    {
        return Err(bad_request("Console channel configuration exceeds limits"));
    }
    Ok(())
}

fn ensure_payload_size(value: &Value) -> Result<(), ApiError> {
    let size = serde_json::to_vec(value)
        .map_err(|error| bad_request(&format!("Invalid channel configuration: {error}")))?
        .len();
    if size > MAX_CHANNEL_CONFIG_BYTES {
        return Err(bad_request("Channel configuration is too large"));
    }
    Ok(())
}

fn ensure_known_channel(channel: &str) -> Result<(), ApiError> {
    if CHANNEL_TYPES.contains(&channel) {
        Ok(())
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(json!({"detail": format!("Channel '{channel}' not found")})),
        ))
    }
}

fn runtime_unavailable(channel: &str) -> ApiError {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({
            "detail": format!("Rust runtime for channel '{channel}' is not implemented")
        })),
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

fn json_value<T: Serialize>(value: &T) -> Result<Value, ApiError> {
    serde_json::to_value(value)
        .map_err(|error| internal_error(&format!("Channel response failed: {error}")))
}

fn value_with_builtin(mut value: Value) -> Value {
    if let Some(object) = value.as_object_mut() {
        object.insert(String::from("isBuiltin"), Value::Bool(true));
    }
    value
}

fn default_channels() -> BTreeMap<String, Value> {
    CHANNEL_TYPES
        .iter()
        .map(|channel| {
            (
                String::from(*channel),
                value_with_builtin(default_channel(channel).expect("static channel must exist")),
            )
        })
        .collect()
}

#[allow(clippy::too_many_lines)]
fn default_channel(channel: &str) -> Option<Value> {
    let mut value = base_channel(channel == "console");
    let object = value.as_object_mut().expect("base channel is an object");
    let fields = match channel {
        "imessage" => json!({
            "db_path": "~/Library/Messages/chat.db", "poll_sec": 1.0,
            "media_dir": null, "max_decoded_size": 10_485_760
        }),
        "discord" => json!({
            "bot_token": "", "http_proxy": "", "http_proxy_auth": "",
            "accept_bot_messages": false, "streaming_enabled": false,
            "media_dir": null
        }),
        "dingtalk" => json!({
            "client_id": "", "client_secret": "", "message_type": "markdown",
            "cron_message_type": "markdown", "card_template_id": "",
            "card_template_key": "content", "robot_code": "", "media_dir": null,
            "card_auto_layout": false, "at_sender_on_reply": false,
            "streaming_enabled": false, "share_session_in_group": false,
            "endpoint": ""
        }),
        "feishu" => json!({
            "app_id": "", "app_secret": "", "encrypt_key": "",
            "verification_token": "", "media_dir": null, "domain": "feishu",
            "streaming_enabled": false, "share_session_in_group": false
        }),
        "qq" => json!({
            "app_id": "", "client_secret": "", "markdown_enabled": true,
            "max_reconnect_attempts": 100, "ack_message": ""
        }),
        "telegram" => json!({
            "bot_token": "", "base_url": "", "http_proxy": "",
            "http_proxy_auth": "", "show_typing": null,
            "streaming_enabled": false
        }),
        "mattermost" => json!({
            "url": "", "bot_token": "", "media_dir": null,
            "show_typing": null, "thread_follow_without_mention": false
        }),
        "mqtt" => json!({
            "host": "", "port": null, "transport": "", "clean_session": true,
            "qos": 2, "username": null, "password": null,
            "subscribe_topic": "", "publish_topic": "", "tls_enabled": false,
            "tls_ca_certs": null, "tls_certfile": null, "tls_keyfile": null
        }),
        "console" => return serde_json::to_value(ConsoleChannelConfig::default()).ok(),
        "matrix" => json!({
            "homeserver": "", "user_id": "", "access_token": "",
            "group_allow_from": [], "groups": {}, "encryption": false,
            "vision_enabled": true, "history_limit": 50, "password": "",
            "device_name": "qwenpaw-worker", "sync_timeout_ms": 30000,
            "mention_pill_in_body": false, "outbound_structured_mentions": true,
            "streaming_enabled": false, "share_session_in_group": true
        }),
        "slack" => json!({
            "bot_token": "", "app_token": "", "proxy": null,
            "streaming_enabled": false, "media_dir": null,
            "allow_from": null, "require_mention": true
        }),
        "voice" => json!({
            "twilio_account_sid": "", "twilio_auth_token": "",
            "phone_number": "", "phone_number_sid": "", "tts_provider": "google",
            "tts_voice": "en-US-Journey-D", "stt_provider": "deepgram",
            "language": "en-US",
            "welcome_greeting": "Hi! This is QwenPaw. How can I help you?"
        }),
        "sip" => json!({
            "sip_mode": "dev", "sip_host": "0.0.0.0", "sip_port": 5061,
            "sip_username": "", "sip_password": "", "sip_server": "",
            "sip_transport": "UDP", "rtp_port_low": 10000,
            "rtp_port_high": 20000, "dashscope_api_key": "",
            "tts_provider": "aliyun", "tts_voice": "", "stt_provider": "aliyun",
            "language": "zh-CN", "welcome_greeting": "你好，我是QwenPaw",
            "call_timeout": 120.0, "livekit_url": "", "livekit_api_key": "",
            "livekit_api_secret": "", "livekit_sip_trunk_id": "",
            "livekit_room_name": "sip-inbound", "livekit_output_sample_rate": 24000,
            "max_concurrent_calls": 5
        }),
        "wecom" => json!({
            "bot_id": "", "secret": "", "ws_url": "", "media_dir": null,
            "welcome_text": "", "share_session_in_group": true,
            "max_reconnect_attempts": -1, "streaming_enabled": false
        }),
        "xiaoyi" => json!({
            "ak": "", "sk": "", "agent_id": "", "ws_url": "",
            "task_timeout_ms": 3_600_000
        }),
        "yuanbao" => json!({
            "app_id": "", "app_secret": "", "api_domain": "bot.yuanbao.tencent.com",
            "ws_url": "", "media_dir": null, "accept_bot_messages": false
        }),
        "wechat" => json!({
            "bot_token": "", "bot_token_file": "", "base_url": "",
            "media_dir": null, "message_merge_enabled": false,
            "message_merge_delay_ms": 0
        }),
        "onebot" => json!({
            "ws_host": "127.0.0.1", "ws_port": 6199, "access_token": "",
            "share_session_in_group": false, "media_dir": null,
            "media_base64": false, "media_base64_max_mb": 10,
            "media_download_max_mb": 50
        }),
        _ => return None,
    };
    object.extend(
        fields
            .as_object()
            .expect("channel fields are an object")
            .clone(),
    );
    Some(value)
}

fn base_channel(enabled: bool) -> Value {
    json!({
        "enabled": enabled,
        "bot_prefix": "",
        "show_tool_calls": true,
        "show_tool_results": true,
        "tool_call_max_length": 200,
        "tool_result_max_length": 500,
        "show_thinking": true,
        "dm_policy": "open",
        "group_policy": "open",
        "allow_from": [],
        "deny_message": "",
        "require_mention": false,
        "no_text_debounce": true,
        "access_control_dm": false,
        "access_control_group": false,
        "dm_disabled": false,
        "group_disabled": false
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires the qwenpaw conda environment and original Python sources"]
    async fn channel_profile_defaults_match_original_handler_and_allow_round_trip() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture = tempfile::tempdir().unwrap();
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            tokio::process::Command::new("python")
                .arg(root.join("scripts/channel_profile_reference.py"))
                .arg(fixture.path())
                .env("PYTHONPATH", root.join("../src"))
                .env("QWENPAW_WORKING_DIR", fixture.path())
                .env("PYTHONDONTWRITEBYTECODE", "1")
                .kill_on_drop(true)
                .output(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let reference: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(reference["passed"], true);
        assert_eq!(reference["tests"], 8);
        let expected = &reference["observations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["case"] == "test_empty_materializes_defaults")
            .unwrap()["channels"];
        let actual: serde_json::Map<String, Value> = CHANNEL_TYPES
            .iter()
            .map(|name| (String::from(*name), default_channel(name).unwrap()))
            .collect();
        pretty_assertions::assert_eq!(Value::Object(actual), *expected);
        validate_profile_config(expected).unwrap();
    }

    #[test]
    fn exposes_all_python_builtin_channel_defaults() {
        let channels = default_channels();
        assert_eq!(channels.len(), CHANNEL_TYPES.len());
        assert_eq!(channels["console"]["enabled"], true);
        assert_eq!(channels["telegram"]["enabled"], false);
        assert_eq!(channels["onebot"]["ws_host"], "127.0.0.1");
        assert_eq!(channels["sip"]["rtp_port_high"], 20_000);
        assert_eq!(channels["wechat"]["message_merge_delay_ms"], 0);
        assert!(channels.values().all(|value| value["isBuiltin"] == true));
    }
}
