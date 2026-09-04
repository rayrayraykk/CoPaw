//! Built-in channel configuration contracts for the unchanged Console.

use std::collections::BTreeMap;

use axum::Json;
use axum::Router;
use axum::extract::Path;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;

use super::AppServer;

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

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredChannelData {
    version: u32,
    console: ConsoleChannelConfig,
}

impl Default for StoredChannelData {
    fn default() -> Self {
        Self {
            version: 1,
            console: ConsoleChannelConfig::default(),
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

async fn list_channels(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_channel_config_lock.lock().await;
    let stored = read_data(&server)?;
    let mut result = default_channels();
    result.insert(String::from("console"), json_value(&stored.console)?);
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
) -> Result<Json<Value>, ApiError> {
    ensure_known_channel(&channel)?;
    if channel == "console" {
        let _guard = server.inner.desktop_channel_config_lock.lock().await;
        return json_value(&read_data(&server)?.console).map(Json);
    }
    Ok(Json(
        default_channel(&channel).expect("channel was validated"),
    ))
}

async fn update_channel(
    State(server): State<AppServer>,
    Path(channel): Path<String>,
    Json(value): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    ensure_known_channel(&channel)?;
    ensure_payload_size(&value)?;
    if channel != "console" {
        return Err(runtime_unavailable(&channel));
    }
    let mut config: ConsoleChannelConfig = serde_json::from_value(value)
        .map_err(|error| bad_request(&format!("Invalid Console channel configuration: {error}")))?;
    config.enabled = true;
    validate_console(&config)?;
    let _guard = server.inner.desktop_channel_config_lock.lock().await;
    persist(
        &server,
        &StoredChannelData {
            version: 1,
            console: config.clone(),
        },
    )?;
    json_value(&config).map(Json)
}

async fn update_channels(
    State(server): State<AppServer>,
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
    let updated = update_channel(State(server), Path(String::from("console")), Json(console))
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
    Path(channel): Path<String>,
    Json(value): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    ensure_known_channel(&channel)?;
    ensure_payload_size(&value)?;
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
    let Some(serialized) = server
        .inner
        .core
        .read_channel_config_data()
        .map_err(|error| internal_error(&error.to_string()))?
    else {
        return Ok(StoredChannelData::default());
    };
    if serialized.len() > MAX_CHANNEL_CONFIG_BYTES {
        return Err(internal_error("Stored channel configuration is too large"));
    }
    let data: StoredChannelData = serde_json::from_str(&serialized)
        .map_err(|_| internal_error("Stored channel configuration is invalid"))?;
    if data.version != 1 {
        return Err(internal_error(
            "Stored channel configuration version is unsupported",
        ));
    }
    validate_console(&data.console)
        .map_err(|_| internal_error("Stored Console channel configuration is invalid"))?;
    Ok(data)
}

fn persist(server: &AppServer, data: &StoredChannelData) -> Result<(), ApiError> {
    let serialized = serde_json::to_string(data)
        .map_err(|error| internal_error(&format!("Channel configuration failed: {error}")))?;
    if serialized.len() > MAX_CHANNEL_CONFIG_BYTES {
        return Err(bad_request("Channel configuration is too large"));
    }
    server
        .inner
        .core
        .write_channel_config_data(&serialized)
        .map_err(|error| internal_error(&error.to_string()))
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
            "streaming_enabled": false, "media_dir": null
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
