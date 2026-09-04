//! Persistent Console-compatible cron job contracts.

use std::collections::BTreeMap;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use axum::Json;
use axum::Router;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use chrono::DateTime;
use chrono::NaiveDateTime;
use chrono::SecondsFormat;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Map;
use serde_json::Value;
use serde_json::json;
use uuid::Uuid;

use super::AppServer;
use super::DesktopPushMessage;

const MAX_CRON_JOBS: usize = 256;
const MAX_CRON_DATA_BYTES: usize = 1_048_576;
const MAX_CRON_HISTORY: usize = 100;
const MAX_JOB_INPUT_BYTES: usize = 262_144;

type ApiError = (StatusCode, Json<Value>);

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route("/api/cron/jobs", get(list_jobs).post(create_job))
        .route(
            "/api/cron/jobs/{job_id}",
            get(get_job).put(replace_job).delete(delete_job),
        )
        .route(
            "/api/cron/jobs/{job_id}/pause",
            axum::routing::post(pause_job),
        )
        .route(
            "/api/cron/jobs/{job_id}/resume",
            axum::routing::post(resume_job),
        )
        .route("/api/cron/jobs/{job_id}/run", axum::routing::post(run_job))
        .route("/api/cron/jobs/{job_id}/state", get(get_job_state))
        .route("/api/cron/jobs/{job_id}/history", get(get_job_history))
        .route("/api/cron/dispatch-targets", get(dispatch_targets))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ScheduleSpec {
    #[serde(rename = "type", default = "default_cron_schedule")]
    kind: String,
    #[serde(default)]
    cron: Option<String>,
    #[serde(default)]
    run_at: Option<String>,
    #[serde(default = "default_timezone")]
    timezone: String,
    #[serde(default)]
    repeat_every_days: Option<u32>,
    #[serde(default)]
    repeat_end_type: Option<String>,
    #[serde(default)]
    repeat_until: Option<String>,
    #[serde(default)]
    repeat_count: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct DispatchTarget {
    user_id: String,
    session_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct DispatchSpec {
    #[serde(rename = "type", default = "default_channel_dispatch")]
    kind: String,
    #[serde(default = "default_console_channel")]
    channel: String,
    target: DispatchTarget,
    #[serde(default = "default_stream_mode")]
    mode: String,
    #[serde(default)]
    silent: bool,
    #[serde(default)]
    meta: Map<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RuntimeSpec {
    #[serde(default = "default_one")]
    max_concurrency: u32,
    #[serde(default = "default_timeout")]
    timeout_seconds: u32,
    #[serde(default = "default_misfire_grace")]
    misfire_grace_seconds: u32,
    #[serde(default = "default_true")]
    share_session: bool,
    #[serde(default)]
    tool_safety: bool,
}

impl Default for RuntimeSpec {
    fn default() -> Self {
        Self {
            max_concurrency: default_one(),
            timeout_seconds: default_timeout(),
            misfire_grace_seconds: default_misfire_grace(),
            share_session: true,
            tool_safety: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CronJobSpec {
    #[serde(default)]
    id: Option<String>,
    name: String,
    #[serde(default = "default_true")]
    enabled: bool,
    schedule: ScheduleSpec,
    #[serde(default = "default_agent_task")]
    task_type: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    request: Option<Value>,
    dispatch: DispatchSpec,
    #[serde(default)]
    save_result_to_inbox: Option<bool>,
    #[serde(default)]
    runtime: RuntimeSpec,
    #[serde(default)]
    meta: Map<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct CronJobState {
    next_run_at: Option<String>,
    last_run_at: Option<String>,
    last_status: Option<String>,
    last_error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CronExecutionRecord {
    run_at: String,
    status: String,
    error: Option<String>,
    trigger: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct CronData {
    version: u32,
    jobs: Vec<CronJobSpec>,
    states: BTreeMap<String, CronJobState>,
    history: BTreeMap<String, Vec<CronExecutionRecord>>,
}

impl Default for CronData {
    fn default() -> Self {
        Self {
            version: 1,
            jobs: Vec::new(),
            states: BTreeMap::new(),
            history: BTreeMap::new(),
        }
    }
}

async fn list_jobs(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_cron_lock.lock().await;
    let data = read_data(&server)?;
    json_value(data.jobs).map(Json)
}

async fn create_job(
    State(server): State<AppServer>,
    Json(mut spec): Json<CronJobSpec>,
) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_cron_lock.lock().await;
    let mut data = read_data(&server)?;
    if data.jobs.len() >= MAX_CRON_JOBS {
        return Err(unprocessable("cron job limit reached"));
    }
    spec.id = Some(Uuid::now_v7().to_string());
    validate_and_normalize(&mut spec)?;
    data.jobs.push(spec.clone());
    write_data(&server, &data)?;
    json_value(spec).map(Json)
}

async fn get_job(
    State(server): State<AppServer>,
    Path(job_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_cron_lock.lock().await;
    let data = read_data(&server)?;
    let spec = find_job(&data, &job_id)?.clone();
    let state = data.states.get(&job_id).cloned().unwrap_or_default();
    Ok(Json(json!({"spec": spec, "state": state})))
}

async fn replace_job(
    State(server): State<AppServer>,
    Path(job_id): Path<String>,
    Json(mut spec): Json<CronJobSpec>,
) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_cron_lock.lock().await;
    if spec.id.as_deref().is_some_and(|id| id != job_id) {
        return Err(error(StatusCode::BAD_REQUEST, "job_id mismatch"));
    }
    spec.id = Some(job_id.clone());
    validate_and_normalize(&mut spec)?;
    let mut data = read_data(&server)?;
    let index = find_job_index(&data, &job_id)?;
    data.jobs[index] = spec.clone();
    write_data(&server, &data)?;
    json_value(spec).map(Json)
}

async fn delete_job(
    State(server): State<AppServer>,
    Path(job_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_cron_lock.lock().await;
    let mut data = read_data(&server)?;
    let index = find_job_index(&data, &job_id)?;
    data.jobs.remove(index);
    data.states.remove(&job_id);
    data.history.remove(&job_id);
    write_data(&server, &data)?;
    Ok(Json(json!({"deleted": true})))
}

async fn pause_job(
    State(server): State<AppServer>,
    Path(job_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    set_job_enabled(&server, &job_id, false).await?;
    Ok(Json(json!({"paused": true})))
}

async fn resume_job(
    State(server): State<AppServer>,
    Path(job_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    set_job_enabled(&server, &job_id, true).await?;
    Ok(Json(json!({"resumed": true})))
}

async fn set_job_enabled(server: &AppServer, job_id: &str, enabled: bool) -> Result<(), ApiError> {
    let _guard = server.inner.desktop_cron_lock.lock().await;
    let mut data = read_data(server)?;
    let index = find_job_index(&data, job_id)?;
    data.jobs[index].enabled = enabled;
    if !enabled {
        data.states
            .entry(job_id.to_owned())
            .or_default()
            .next_run_at = None;
    }
    write_data(server, &data)
}

async fn run_job(
    State(server): State<AppServer>,
    Path(job_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let cron_guard = server.inner.desktop_cron_lock.lock().await;
    let mut data = read_data(&server)?;
    let spec = find_job(&data, &job_id)?.clone();
    if spec.task_type != "text" || spec.dispatch.channel != "console" {
        return Err(error(
            StatusCode::NOT_IMPLEMENTED,
            "Rust Core cron execution currently supports console text jobs only",
        ));
    }
    let now = now_rfc3339();
    let text = spec.text.unwrap_or_default();
    let job_name = spec.name;
    let save_result_to_inbox = spec.save_result_to_inbox.unwrap_or(false);
    let push_message = DesktopPushMessage {
        id: Uuid::now_v7().to_string(),
        text: text.clone(),
        sticky: false,
        session_id: spec.dispatch.target.session_id,
        created_at: now_epoch_seconds(),
    };
    let state = data.states.entry(job_id.clone()).or_default();
    state.last_run_at = Some(now.clone());
    state.last_status = Some(String::from("success"));
    state.last_error = None;
    let history = data.history.entry(job_id.clone()).or_default();
    history.push(CronExecutionRecord {
        run_at: now,
        status: String::from("success"),
        error: None,
        trigger: String::from("manual"),
    });
    if history.len() > MAX_CRON_HISTORY {
        history.drain(..history.len() - MAX_CRON_HISTORY);
    }
    write_data(&server, &data)?;
    drop(cron_guard);

    let mut messages = server.inner.desktop_push_messages.write().await;
    messages.push(push_message);
    if messages.len() > 500 {
        let excess = messages.len() - 500;
        messages.drain(..excess);
    }
    drop(messages);
    if save_result_to_inbox {
        let inbox_result = super::desktop_inbox::append_event(
            &server,
            super::desktop_inbox::NewInboxEvent {
                agent_id: String::from("default"),
                source_type: String::from("cron"),
                source_id: job_id.clone(),
                event_type: String::from("cron_result"),
                status: String::from("success"),
                severity: String::from("info"),
                title: format!("Cron result: {job_name}"),
                body: text,
                payload: json!({
                    "job_id": job_id,
                    "job_name": job_name,
                    "task_type": "text",
                    "trigger": "manual",
                    "run_id": null,
                    "save_result_to_inbox": true
                }),
            },
        )
        .await;
        if inbox_result.is_err() {
            tracing::warn!("failed to save a completed Cron result to the Inbox");
        }
    }
    Ok(Json(json!({"started": true})))
}

async fn get_job_state(
    State(server): State<AppServer>,
    Path(job_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_cron_lock.lock().await;
    let data = read_data(&server)?;
    find_job(&data, &job_id)?;
    json_value(data.states.get(&job_id).cloned().unwrap_or_default()).map(Json)
}

async fn get_job_history(
    State(server): State<AppServer>,
    Path(job_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_cron_lock.lock().await;
    let data = read_data(&server)?;
    find_job(&data, &job_id)?;
    json_value(data.history.get(&job_id).cloned().unwrap_or_default()).map(Json)
}

#[derive(Debug, Default, Deserialize)]
struct DispatchTargetsQuery {
    channel: Option<String>,
    keyword: Option<String>,
    limit: Option<usize>,
}

async fn dispatch_targets(
    State(server): State<AppServer>,
    Query(query): Query<DispatchTargetsQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(500);
    if !(1..=2_000).contains(&limit) {
        return Err(unprocessable("limit must be between 1 and 2000"));
    }
    let aliases = server.inner.desktop_session_aliases.read().await;
    let keyword = query.keyword.unwrap_or_default().trim().to_lowercase();
    let mut session_ids = aliases.client_to_thread.keys().cloned().collect::<Vec<_>>();
    session_ids.sort();
    session_ids.dedup();
    let items = session_ids
        .into_iter()
        .filter(|_session_id| {
            query
                .channel
                .as_deref()
                .is_none_or(|value| value == "console")
        })
        .filter(|session_id| {
            keyword.is_empty()
                || format!("console admin {session_id}")
                    .to_lowercase()
                    .contains(&keyword)
        })
        .take(limit)
        .map(|session_id| {
            json!({
                "channel": "console",
                "user_id": "admin",
                "session_id": session_id
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({"channels": ["console"], "items": items})))
}

fn read_data(server: &AppServer) -> Result<CronData, ApiError> {
    let Some(serialized) = server.inner.core.read_cron_data().map_err(internal)? else {
        return Ok(CronData::default());
    };
    if serialized.len() > MAX_CRON_DATA_BYTES {
        return Err(internal("stored cron data exceeds its size limit"));
    }
    let data = serde_json::from_str::<CronData>(&serialized)
        .map_err(|_| internal("stored cron data is invalid"))?;
    if data.version != 1 || data.jobs.len() > MAX_CRON_JOBS {
        return Err(internal("stored cron data has an unsupported shape"));
    }
    Ok(data)
}

fn write_data(server: &AppServer, data: &CronData) -> Result<(), ApiError> {
    let serialized = serde_json::to_string(data).map_err(|_| internal("cron data is invalid"))?;
    if serialized.len() > MAX_CRON_DATA_BYTES {
        return Err(unprocessable("cron data exceeds its size limit"));
    }
    server
        .inner
        .core
        .write_cron_data(&serialized)
        .map_err(internal)
}

fn find_job<'a>(data: &'a CronData, job_id: &str) -> Result<&'a CronJobSpec, ApiError> {
    data.jobs
        .iter()
        .find(|job| job.id.as_deref() == Some(job_id))
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "job not found"))
}

fn find_job_index(data: &CronData, job_id: &str) -> Result<usize, ApiError> {
    data.jobs
        .iter()
        .position(|job| job.id.as_deref() == Some(job_id))
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "job not found"))
}

fn validate_and_normalize(spec: &mut CronJobSpec) -> Result<(), ApiError> {
    let bytes = serde_json::to_vec(spec).map_err(|_| unprocessable("cron job is invalid"))?;
    if bytes.len() > MAX_JOB_INPUT_BYTES {
        return Err(unprocessable("cron job exceeds its size limit"));
    }
    if spec.id.as_deref().is_none_or(str::is_empty) || spec.name.len() > 1_024 {
        return Err(unprocessable("cron job id or name is invalid"));
    }
    validate_schedule(&mut spec.schedule)?;
    validate_dispatch(&spec.dispatch)?;
    if spec.runtime.max_concurrency == 0 || spec.runtime.timeout_seconds == 0 {
        return Err(unprocessable("cron runtime values must be positive"));
    }
    match spec.task_type.as_str() {
        "text" => {
            if spec
                .text
                .as_deref()
                .is_none_or(|text| text.trim().is_empty())
            {
                return Err(unprocessable("task_type is text but text is empty"));
            }
            if spec.dispatch.silent {
                return Err(unprocessable(
                    "silent delivery is only supported for agent tasks",
                ));
            }
            spec.request = None;
        }
        "agent" => {
            let request = spec
                .request
                .as_mut()
                .and_then(Value::as_object_mut)
                .ok_or_else(|| unprocessable("task_type is agent but request is missing"))?;
            request.insert(
                String::from("user_id"),
                Value::String(spec.dispatch.target.user_id.clone()),
            );
            request.insert(
                String::from("session_id"),
                Value::String(spec.dispatch.target.session_id.clone()),
            );
        }
        _ => return Err(unprocessable("task_type must be text or agent")),
    }
    if spec.save_result_to_inbox.is_none() {
        spec.save_result_to_inbox =
            Some(!(spec.task_type == "text" && spec.schedule.kind == "cron"));
    }
    Ok(())
}

fn validate_schedule(schedule: &mut ScheduleSpec) -> Result<(), ApiError> {
    if schedule.timezone.trim().is_empty() || schedule.timezone.len() > 128 {
        return Err(unprocessable("schedule timezone is invalid"));
    }
    match schedule.kind.as_str() {
        "cron" => {
            let cron = schedule
                .cron
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| unprocessable("schedule.type is cron but cron is empty"))?;
            schedule.cron = Some(normalize_cron(cron)?);
            schedule.run_at = None;
            schedule.repeat_every_days = None;
            schedule.repeat_end_type = None;
            schedule.repeat_until = None;
            schedule.repeat_count = None;
        }
        "once" => {
            let run_at = schedule
                .run_at
                .as_deref()
                .ok_or_else(|| unprocessable("schedule.type is once but run_at is missing"))?;
            let run_timestamp = parse_datetime(run_at)?;
            schedule.cron = None;
            if schedule.repeat_every_days.is_none() {
                schedule.repeat_end_type = None;
                schedule.repeat_until = None;
                schedule.repeat_count = None;
                return Ok(());
            }
            if schedule.repeat_every_days == Some(0) {
                return Err(unprocessable("repeat_every_days must be at least 1"));
            }
            let end_type = schedule
                .repeat_end_type
                .get_or_insert_with(|| String::from("never"));
            match end_type.as_str() {
                "never" => {
                    schedule.repeat_until = None;
                    schedule.repeat_count = None;
                }
                "until" => {
                    let repeat_until = schedule.repeat_until.as_deref().ok_or_else(|| {
                        unprocessable("repeat_end_type is until but repeat_until is missing")
                    })?;
                    if parse_datetime(repeat_until)? <= run_timestamp {
                        return Err(unprocessable("repeat_until must be later than run_at"));
                    }
                    schedule.repeat_count = None;
                }
                "count" => {
                    if schedule.repeat_count.is_none_or(|count| count == 0) {
                        return Err(unprocessable(
                            "repeat_end_type is count but repeat_count is missing",
                        ));
                    }
                    schedule.repeat_until = None;
                }
                _ => return Err(unprocessable("repeat_end_type is invalid")),
            }
        }
        _ => return Err(unprocessable("schedule.type must be cron or once")),
    }
    Ok(())
}

fn validate_dispatch(dispatch: &DispatchSpec) -> Result<(), ApiError> {
    if dispatch.kind != "channel" || !matches!(dispatch.mode.as_str(), "stream" | "final") {
        return Err(unprocessable("cron dispatch configuration is invalid"));
    }
    if dispatch.channel.trim().is_empty()
        || dispatch.target.user_id.len() > 4_096
        || dispatch.target.session_id.len() > 4_096
    {
        return Err(unprocessable("cron dispatch target is invalid"));
    }
    Ok(())
}

fn normalize_cron(cron: &str) -> Result<String, ApiError> {
    let mut fields = cron
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    fields = match fields.len() {
        5 => fields,
        4 => vec![
            String::from("0"),
            fields[0].clone(),
            fields[1].clone(),
            fields[2].clone(),
            fields[3].clone(),
        ],
        3 => vec![
            String::from("0"),
            String::from("0"),
            fields[0].clone(),
            fields[1].clone(),
            fields[2].clone(),
        ],
        _ => return Err(unprocessable("cron must have 5 fields")),
    };
    fields[4] = normalize_weekdays(&fields[4]);
    Ok(fields.join(" "))
}

fn normalize_weekdays(value: &str) -> String {
    value
        .split(',')
        .map(|part| {
            let (base, step) = part.split_once('/').unwrap_or((part, ""));
            let normalized = base.split_once('-').map_or_else(
                || weekday(base),
                |(left, right)| format!("{}-{}", weekday(left), weekday(right)),
            );
            if step.is_empty() {
                normalized
            } else {
                format!("{normalized}/{step}")
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn weekday(value: &str) -> String {
    match value {
        "0" | "7" => String::from("sun"),
        "1" => String::from("mon"),
        "2" => String::from("tue"),
        "3" => String::from("wed"),
        "4" => String::from("thu"),
        "5" => String::from("fri"),
        "6" => String::from("sat"),
        _ => value.to_owned(),
    }
}

fn parse_datetime(value: &str) -> Result<i64, ApiError> {
    if let Ok(value) = DateTime::parse_from_rfc3339(value) {
        return Ok(value.timestamp());
    }
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S")
        .map(|value| value.and_utc().timestamp())
        .map_err(|_| unprocessable("scheduled datetime is invalid"))
}

fn now_rfc3339() -> String {
    DateTime::<Utc>::from(SystemTime::now()).to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn now_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn json_value<T: Serialize>(value: T) -> Result<Value, ApiError> {
    serde_json::to_value(value).map_err(|_| internal("cron response could not be serialized"))
}

fn default_true() -> bool {
    true
}

fn default_one() -> u32 {
    1
}

fn default_timeout() -> u32 {
    120
}

fn default_misfire_grace() -> u32 {
    600
}

fn default_cron_schedule() -> String {
    String::from("cron")
}

fn default_timezone() -> String {
    String::from("UTC")
}

fn default_channel_dispatch() -> String {
    String::from("channel")
}

fn default_console_channel() -> String {
    String::from("console")
}

fn default_stream_mode() -> String {
    String::from("stream")
}

fn default_agent_task() -> String {
    String::from("agent")
}

fn unprocessable(detail: &str) -> ApiError {
    error(StatusCode::UNPROCESSABLE_ENTITY, detail)
}

fn internal(error_value: impl std::fmt::Display) -> ApiError {
    tracing::warn!(error = %error_value, "Desktop cron persistence failed");
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Desktop cron data could not be persisted",
    )
}

fn error(status: StatusCode, detail: &str) -> ApiError {
    (status, Json(json!({"detail": detail})))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_supported_cron_shapes_and_weekdays() {
        assert_eq!(normalize_cron("9 * * 0").unwrap(), "0 9 * * sun");
        assert_eq!(normalize_cron("1 2 1-5").unwrap(), "0 0 1 2 mon-fri");
        assert!(normalize_cron("0 0 0 1 1 1").is_err());
    }
}
