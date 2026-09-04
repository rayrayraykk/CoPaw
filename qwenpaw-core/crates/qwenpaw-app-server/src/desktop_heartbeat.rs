//! Persistent Heartbeat configuration and execution for the unchanged Console.

use std::io::ErrorKind;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::routing::post;
use chrono::NaiveTime;
use chrono::Utc;
use chrono_tz::Tz;
use qwenpaw_protocol::CoreEvent;
use qwenpaw_protocol::Item;
use qwenpaw_protocol::ThreadListParams;
use qwenpaw_protocol::Turn;
use qwenpaw_protocol::TurnInterruptParams;
use qwenpaw_protocol::TurnStartParams;
use qwenpaw_protocol::TurnStatus;
use qwenpaw_protocol::UserInput;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::AppServer;
use super::desktop_inbox::NewInboxEvent;
use super::desktop_inbox::NewInboxTrace;

const HEARTBEAT_DATA_VERSION: u32 = 1;
const MAX_HEARTBEAT_DATA_BYTES: usize = 65_536;
const MAX_HEARTBEAT_FILE_BYTES: u64 = 1_048_576;
const MAX_INTERVAL_SECONDS: u64 = 365 * 24 * 60 * 60;
const MAX_TIMEOUT_SECONDS: u64 = 3_600;
const MAIN_SESSION_ID: &str = "main";
const HEARTBEAT_SOURCE_ID: &str = "_heartbeat";

type ApiError = (StatusCode, Json<Value>);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ActiveHours {
    start: String,
    end: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct HeartbeatConfig {
    #[serde(default)]
    enabled: bool,
    #[serde(default = "default_every")]
    every: String,
    #[serde(default = "default_target")]
    target: String,
    #[serde(default = "default_timeout_seconds")]
    timeout_seconds: u64,
    #[serde(default)]
    active_hours: Option<ActiveHours>,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            every: default_every(),
            target: default_target(),
            timeout_seconds: default_timeout_seconds(),
            active_hours: None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct HeartbeatData {
    version: u32,
    config: HeartbeatConfig,
}

struct HeartbeatRun {
    turn: Turn,
    target: String,
    query_path: String,
}

enum HeartbeatExecution {
    Skipped,
    Finished(HeartbeatRun),
    TimedOut {
        thread_id: String,
        turn_id: String,
        target: String,
        query_path: String,
        timeout_seconds: u64,
    },
}

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route(
            "/api/config/heartbeat",
            get(get_heartbeat).put(put_heartbeat),
        )
        .route("/api/config/heartbeat/run", post(run_heartbeat))
}

pub(super) fn spawn_scheduler(server: &AppServer) -> JoinHandle<()> {
    let weak = Arc::downgrade(&server.inner);
    let mut revision = server.inner.desktop_heartbeat_revision.subscribe();
    let shutdown = server.inner.shutdown.clone();
    tokio::spawn(async move {
        loop {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let server = AppServer { inner };
            let config = {
                let _guard = server.inner.desktop_heartbeat_lock.lock().await;
                match read_config(&server) {
                    Ok(config) => config,
                    Err((_, body)) => {
                        tracing::warn!(
                            detail = ?body.0,
                            "stored Heartbeat configuration could not be scheduled"
                        );
                        HeartbeatConfig::default()
                    }
                }
            };
            let interval = config
                .enabled
                .then(|| parse_interval(&config.every))
                .transpose()
                .ok()
                .flatten();
            drop(server);
            match interval {
                Some(interval) => {
                    tokio::select! {
                        () = tokio::time::sleep(interval) => {
                            if let Some(inner) = weak.upgrade() {
                                let _ = try_spawn_heartbeat(AppServer { inner });
                            } else {
                                return;
                            }
                        }
                        changed = revision.changed() => {
                            if changed.is_err() {
                                return;
                            }
                        }
                        () = shutdown.cancelled() => return,
                    }
                }
                None => {
                    tokio::select! {
                        changed = revision.changed() => {
                            if changed.is_err() {
                                return;
                            }
                        }
                        () = shutdown.cancelled() => return,
                    }
                }
            }
        }
    })
}

async fn get_heartbeat(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_heartbeat_lock.lock().await;
    json_value(read_config(&server)?).map(Json)
}

async fn put_heartbeat(
    State(server): State<AppServer>,
    Json(config): Json<HeartbeatConfig>,
) -> Result<Json<Value>, ApiError> {
    let config = normalize_config(config)?;
    {
        let _guard = server.inner.desktop_heartbeat_lock.lock().await;
        write_config(&server, &config)?;
    }
    server
        .inner
        .desktop_heartbeat_revision
        .send_modify(|revision| *revision = revision.wrapping_add(1));
    json_value(config).map(Json)
}

async fn run_heartbeat(State(server): State<AppServer>) -> Json<Value> {
    Json(json!({"started": try_spawn_heartbeat(server)}))
}

fn try_spawn_heartbeat(server: AppServer) -> bool {
    if server
        .inner
        .desktop_heartbeat_running
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return false;
    }
    tokio::spawn(async move {
        if let Err(error) = execute_and_record(&server).await {
            tracing::warn!(error, "Heartbeat execution failed");
        }
        server
            .inner
            .desktop_heartbeat_running
            .store(false, Ordering::Release);
    });
    true
}

async fn execute_and_record(server: &AppServer) -> Result<(), String> {
    let execution = execute_heartbeat(server).await?;
    match execution {
        HeartbeatExecution::Finished(run) if run.target == "inbox" => {
            record_finished_run(server, run).await
        }
        HeartbeatExecution::TimedOut {
            thread_id,
            turn_id,
            target,
            query_path,
            timeout_seconds,
        } if target == "inbox" => {
            record_timeout(
                server,
                &thread_id,
                &turn_id,
                &target,
                &query_path,
                timeout_seconds,
            )
            .await
        }
        HeartbeatExecution::Skipped
        | HeartbeatExecution::Finished(_)
        | HeartbeatExecution::TimedOut { .. } => Ok(()),
    }
}

async fn execute_heartbeat(server: &AppServer) -> Result<HeartbeatExecution, String> {
    let config = {
        let _guard = server.inner.desktop_heartbeat_lock.lock().await;
        read_config(server).map_err(api_error_detail)?
    };
    if !in_active_hours(server, config.active_hours.as_ref())? {
        return Ok(HeartbeatExecution::Skipped);
    }
    let workspace = selected_workspace(server).await?;
    let query_path = workspace.join("HEARTBEAT.md");
    let metadata = match tokio::fs::symlink_metadata(&query_path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(HeartbeatExecution::Skipped);
        }
        Err(error) => return Err(format!("failed to inspect HEARTBEAT.md: {error}")),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(String::from("HEARTBEAT.md must be a regular file"));
    }
    if metadata.len() > MAX_HEARTBEAT_FILE_BYTES {
        return Err(String::from("HEARTBEAT.md exceeds the 1 MiB limit"));
    }
    let query = tokio::fs::read_to_string(&query_path)
        .await
        .map_err(|error| format!("failed to read HEARTBEAT.md: {error}"))?;
    let query = query.trim();
    if query.is_empty() {
        return Ok(HeartbeatExecution::Skipped);
    }
    let target_session = if config.target == "last" {
        last_console_session(server)
            .await
            .unwrap_or_else(|| String::from(MAIN_SESSION_ID))
    } else {
        String::from(MAIN_SESSION_ID)
    };
    let workspace_string = workspace.to_string_lossy().into_owned();
    let thread_id = super::desktop_api::resolve_console_thread(
        server,
        Some(&target_session),
        Some(&workspace_string),
    )
    .await
    .map_err(api_error_detail)?;
    let (started, events) = server
        .inner
        .core
        .start_turn(TurnStartParams {
            thread_id: thread_id.clone(),
            input: vec![UserInput::Text {
                text: query.to_owned(),
            }],
        })
        .await
        .map_err(|error| error.to_string())?;
    let turn_id = started.turn.id;
    let consume = consume_turn(server, events, &turn_id);
    let completed = tokio::select! {
        result = tokio::time::timeout(Duration::from_secs(config.timeout_seconds), consume) => {
            match result {
                Ok(result) => Some(result?),
                Err(_) => None,
            }
        }
        () = server.inner.shutdown.cancelled() => None,
    };
    let query_path = query_path.to_string_lossy().into_owned();
    if let Some(turn) = completed {
        return Ok(HeartbeatExecution::Finished(HeartbeatRun {
            turn,
            target: config.target,
            query_path,
        }));
    }
    let _ = server
        .inner
        .core
        .interrupt_turn(&TurnInterruptParams {
            thread_id: thread_id.clone(),
            turn_id: turn_id.clone(),
        })
        .await;
    super::desktop_api::clear_turn_approvals(server, &turn_id).await;
    Ok(HeartbeatExecution::TimedOut {
        thread_id,
        turn_id,
        target: config.target,
        query_path,
        timeout_seconds: config.timeout_seconds,
    })
}

async fn consume_turn(
    server: &AppServer,
    mut events: qwenpaw_core::TurnEventStream,
    turn_id: &str,
) -> Result<Turn, String> {
    while let Some(event) = events.recv().await {
        super::desktop_api::track_pending_approval(server, &event).await;
        if let CoreEvent::TurnCompleted(notification) = event {
            super::desktop_api::clear_turn_approvals(server, turn_id).await;
            return Ok(notification.turn);
        }
    }
    super::desktop_api::clear_turn_approvals(server, turn_id).await;
    Err(String::from("Heartbeat Agent event stream ended early"))
}

async fn record_finished_run(server: &AppServer, run: HeartbeatRun) -> Result<(), String> {
    let success = run.turn.status == TurnStatus::Completed;
    let body = last_agent_message(&run.turn).unwrap_or_else(|| {
        run.turn.error.as_ref().map_or_else(
            || String::from("Heartbeat task finished."),
            |error| error.message.clone(),
        )
    });
    let run_id = Uuid::now_v7().to_string();
    let status = if success { "success" } else { "error" };
    let event_type = if success {
        "heartbeat_result"
    } else {
        "heartbeat_error"
    };
    let error = (!success).then(|| body.clone());
    let thread_id = run.turn.thread_id.clone();
    let turn_id = run.turn.id.clone();
    super::desktop_inbox::append_event_with_trace(
        server,
        NewInboxEvent {
            agent_id: String::from("default"),
            source_type: String::from("heartbeat"),
            source_id: String::from(HEARTBEAT_SOURCE_ID),
            event_type: String::from(event_type),
            status: String::from(status),
            severity: String::from(if success { "info" } else { "error" }),
            title: String::from(if success {
                "Heartbeat result"
            } else {
                "Heartbeat execution failed"
            }),
            body,
            payload: json!({
                "run_id": run_id,
                "target": run.target,
                "query_file": run.query_path,
                "thread_id": thread_id,
                "turn_id": turn_id
            }),
        },
        NewInboxTrace {
            run_id,
            status: String::from(status),
            meta: json!({
                "source": "heartbeat",
                "task_type": "agent",
                "target": run.target,
                "query_file": run.query_path,
                "agent_id": "default",
                "session_id": MAIN_SESSION_ID,
                "channel": "console",
                "thread_id": thread_id,
                "turn_id": turn_id
            }),
            events: trace_events(&run.turn),
            error,
        },
    )
    .await
    .map(|_| ())
    .map_err(api_error_detail)
}

async fn record_timeout(
    server: &AppServer,
    thread_id: &str,
    turn_id: &str,
    target: &str,
    query_path: &str,
    timeout_seconds: u64,
) -> Result<(), String> {
    let run_id = Uuid::now_v7().to_string();
    let body = format!("Heartbeat run timed out after {timeout_seconds}s.");
    super::desktop_inbox::append_event_with_trace(
        server,
        NewInboxEvent {
            agent_id: String::from("default"),
            source_type: String::from("heartbeat"),
            source_id: String::from(HEARTBEAT_SOURCE_ID),
            event_type: String::from("heartbeat_timeout"),
            status: String::from("error"),
            severity: String::from("error"),
            title: String::from("Heartbeat timed out"),
            body: body.clone(),
            payload: json!({
                "run_id": run_id,
                "target": target,
                "query_file": query_path,
                "thread_id": thread_id,
                "turn_id": turn_id
            }),
        },
        NewInboxTrace {
            run_id,
            status: String::from("timeout"),
            meta: json!({
                "source": "heartbeat",
                "task_type": "agent",
                "target": target,
                "query_file": query_path,
                "agent_id": "default",
                "session_id": MAIN_SESSION_ID,
                "channel": "console",
                "thread_id": thread_id,
                "turn_id": turn_id
            }),
            events: vec![text_trace_event("assistant", &body)],
            error: Some(body),
        },
    )
    .await
    .map(|_| ())
    .map_err(api_error_detail)
}

fn trace_events(turn: &Turn) -> Vec<Value> {
    turn.items
        .iter()
        .map(|item| match item {
            Item::UserMessage { text, .. } => text_trace_event("user", text),
            Item::AgentMessage { text, .. } => text_trace_event("assistant", text),
            Item::ToolCall {
                call_id,
                name,
                arguments,
                ..
            } => json!({
                "role": "assistant",
                "tool_name": name,
                "content": [{
                    "type": "tool_call",
                    "id": call_id,
                    "name": name,
                    "raw_input": arguments,
                    "input": serde_json::from_str::<Value>(arguments)
                        .unwrap_or_else(|_| Value::String(arguments.clone()))
                }]
            }),
            Item::ToolResult {
                call_id,
                content,
                is_error,
                ..
            } => json!({
                "role": "tool",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": call_id,
                    "is_error": is_error,
                    "output": [{"type": "text", "text": content}]
                }]
            }),
        })
        .collect()
}

fn text_trace_event(role: &str, text: &str) -> Value {
    json!({
        "role": role,
        "content": [{"type": "text", "text": text}]
    })
}

fn last_agent_message(turn: &Turn) -> Option<String> {
    turn.items.iter().rev().find_map(|item| match item {
        Item::AgentMessage { text, .. } if !text.trim().is_empty() => Some(text.clone()),
        _ => None,
    })
}

async fn selected_workspace(server: &AppServer) -> Result<std::path::PathBuf, String> {
    let workspace = server
        .inner
        .desktop_workspace
        .as_ref()
        .ok_or_else(|| String::from("Heartbeat requires a Desktop Workspace"))?;
    Ok(workspace.selected.read().await.clone())
}

async fn last_console_session(server: &AppServer) -> Option<String> {
    let threads = server
        .inner
        .core
        .list_threads(ThreadListParams {
            cursor: None,
            limit: Some(500),
            include_archived: false,
        })
        .await;
    let aliases = server.inner.desktop_session_aliases.read().await;
    threads.data.into_iter().find_map(|thread| {
        aliases
            .thread_to_client
            .get(&thread.id)
            .filter(|session_id| session_id.as_str() != MAIN_SESSION_ID)
            .cloned()
    })
}

fn in_active_hours(server: &AppServer, active_hours: Option<&ActiveHours>) -> Result<bool, String> {
    let Some(active_hours) = active_hours else {
        return Ok(true);
    };
    let start = parse_clock(&active_hours.start).map_err(api_error_detail)?;
    let end = parse_clock(&active_hours.end).map_err(api_error_detail)?;
    let timezone = super::desktop_agent_settings::user_timezone(&server.inner.core)
        .map_err(api_error_detail)?;
    let timezone = timezone
        .parse::<Tz>()
        .map_err(|_| format!("stored user timezone is invalid: {timezone}"))?;
    Ok(time_is_active(
        Utc::now().with_timezone(&timezone).time(),
        start,
        end,
    ))
}

fn time_is_active(now: NaiveTime, start: NaiveTime, end: NaiveTime) -> bool {
    if start <= end {
        start <= now && now <= end
    } else {
        now >= start || now <= end
    }
}

fn normalize_config(mut config: HeartbeatConfig) -> Result<HeartbeatConfig, ApiError> {
    config.every = config.every.trim().to_ascii_lowercase();
    parse_interval(&config.every)?;
    config.target = config.target.trim().to_ascii_lowercase();
    if !matches!(config.target.as_str(), "main" | "last" | "inbox") {
        return Err(unprocessable(
            "Heartbeat target must be main, last, or inbox",
        ));
    }
    if !(1..=MAX_TIMEOUT_SECONDS).contains(&config.timeout_seconds) {
        return Err(unprocessable(
            "Heartbeat timeoutSeconds must be between 1 and 3600",
        ));
    }
    if let Some(active_hours) = &mut config.active_hours {
        active_hours.start = active_hours.start.trim().to_owned();
        active_hours.end = active_hours.end.trim().to_owned();
        parse_clock(&active_hours.start)?;
        parse_clock(&active_hours.end)?;
    }
    let bytes = serde_json::to_vec(&config)
        .map_err(|_| unprocessable("Heartbeat configuration is invalid"))?;
    if bytes.len() > MAX_HEARTBEAT_DATA_BYTES {
        return Err(unprocessable("Heartbeat configuration is too large"));
    }
    Ok(config)
}

fn parse_interval(value: &str) -> Result<Duration, ApiError> {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 {
        return Err(unprocessable("Heartbeat every is invalid"));
    }
    let mut index = 0;
    let mut previous_rank = 4;
    let mut total = 0_u64;
    while index < bytes.len() {
        let start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if start == index || index == bytes.len() {
            return Err(unprocessable("Heartbeat every is invalid"));
        }
        let number = value[start..index]
            .parse::<u64>()
            .map_err(|_| unprocessable("Heartbeat every is invalid"))?;
        let (rank, multiplier) = match bytes[index] {
            b'h' => (3, 3_600_u64),
            b'm' => (2, 60_u64),
            b's' => (1, 1_u64),
            _ => return Err(unprocessable("Heartbeat every is invalid")),
        };
        if rank >= previous_rank {
            return Err(unprocessable("Heartbeat every is invalid"));
        }
        previous_rank = rank;
        total = total
            .checked_add(
                number
                    .checked_mul(multiplier)
                    .ok_or_else(|| unprocessable("Heartbeat every is too large"))?,
            )
            .ok_or_else(|| unprocessable("Heartbeat every is too large"))?;
        index += 1;
    }
    if total == 0 || total > MAX_INTERVAL_SECONDS {
        return Err(unprocessable(
            "Heartbeat every must be between 1 second and 365 days",
        ));
    }
    Ok(Duration::from_secs(total))
}

fn parse_clock(value: &str) -> Result<NaiveTime, ApiError> {
    NaiveTime::parse_from_str(value, "%H:%M")
        .map_err(|_| unprocessable("Heartbeat active hours must use HH:mm"))
}

fn read_config(server: &AppServer) -> Result<HeartbeatConfig, ApiError> {
    let Some(serialized) = server.inner.core.read_heartbeat_data().map_err(internal)? else {
        return Ok(HeartbeatConfig::default());
    };
    if serialized.len() > MAX_HEARTBEAT_DATA_BYTES {
        return Err(internal("stored Heartbeat configuration is too large"));
    }
    let data = serde_json::from_str::<HeartbeatData>(&serialized)
        .map_err(|_| internal("stored Heartbeat configuration is invalid"))?;
    if data.version != HEARTBEAT_DATA_VERSION {
        return Err(internal(
            "stored Heartbeat configuration version is unsupported",
        ));
    }
    normalize_config(data.config).map_err(|_| internal("stored Heartbeat configuration is invalid"))
}

fn write_config(server: &AppServer, config: &HeartbeatConfig) -> Result<(), ApiError> {
    let serialized = serde_json::to_string(&HeartbeatData {
        version: HEARTBEAT_DATA_VERSION,
        config: config.clone(),
    })
    .map_err(|_| internal("Heartbeat configuration could not be serialized"))?;
    if serialized.len() > MAX_HEARTBEAT_DATA_BYTES {
        return Err(unprocessable("Heartbeat configuration is too large"));
    }
    server
        .inner
        .core
        .write_heartbeat_data(&serialized)
        .map_err(internal)
}

fn default_every() -> String {
    String::from("6h")
}

fn default_target() -> String {
    String::from("main")
}

fn default_timeout_seconds() -> u64 {
    300
}

fn json_value(value: impl Serialize) -> Result<Value, ApiError> {
    serde_json::to_value(value).map_err(|_| internal("Heartbeat response is invalid"))
}

fn api_error_detail((_, Json(body)): ApiError) -> String {
    body.get("detail")
        .and_then(Value::as_str)
        .unwrap_or("Heartbeat operation failed")
        .to_owned()
}

fn internal(error_value: impl std::fmt::Display) -> ApiError {
    tracing::warn!(error = %error_value, "Desktop Heartbeat persistence failed");
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Heartbeat configuration could not be persisted",
    )
}

fn unprocessable(detail: &str) -> ApiError {
    error(StatusCode::UNPROCESSABLE_ENTITY, detail)
}

fn error(status: StatusCode, detail: &str) -> ApiError {
    (status, Json(json!({"detail": detail})))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bounded_legacy_intervals() {
        assert_eq!(
            parse_interval("1h30m5s").expect("valid interval"),
            Duration::from_secs(5_405)
        );
        assert_eq!(
            parse_interval("1s").expect("valid interval"),
            Duration::from_secs(1)
        );
        for invalid in ["", "0s", "1m2h", "1h1h", "1", "-1m", "366d"] {
            assert!(parse_interval(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn evaluates_normal_and_cross_midnight_active_hours() {
        let morning = NaiveTime::from_hms_opt(9, 0, 0).expect("valid time");
        let evening = NaiveTime::from_hms_opt(21, 0, 0).expect("valid time");
        let start = NaiveTime::from_hms_opt(8, 0, 0).expect("valid time");
        let end = NaiveTime::from_hms_opt(22, 0, 0).expect("valid time");
        assert!(time_is_active(morning, start, end));
        assert!(time_is_active(evening, start, end));
        assert!(!time_is_active(
            NaiveTime::from_hms_opt(7, 59, 0).expect("valid time"),
            start,
            end
        ));

        let overnight_start = NaiveTime::from_hms_opt(22, 0, 0).expect("valid time");
        let overnight_end = NaiveTime::from_hms_opt(6, 0, 0).expect("valid time");
        assert!(time_is_active(
            NaiveTime::from_hms_opt(23, 0, 0).expect("valid time"),
            overnight_start,
            overnight_end
        ));
        assert!(time_is_active(
            NaiveTime::from_hms_opt(5, 0, 0).expect("valid time"),
            overnight_start,
            overnight_end
        ));
        assert!(!time_is_active(
            NaiveTime::from_hms_opt(12, 0, 0).expect("valid time"),
            overnight_start,
            overnight_end
        ));
    }
}
