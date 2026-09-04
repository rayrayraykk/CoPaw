//! Persistent Inbox event and trace contracts for the unchanged Console.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use axum::Json;
use axum::Router;
use axum::extract::Path;
use axum::extract::RawQuery;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::delete;
use axum::routing::get;
use axum::routing::post;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use url::form_urlencoded;
use uuid::Uuid;

use super::AppServer;

const MAX_DATA_BYTES: usize = 16_777_216;
const MAX_EVENTS: usize = 5_000;
const MAX_TRACES: usize = 5_000;
const MAX_TRACE_EVENTS: usize = 10_000;
const MAX_EVENT_BYTES: usize = 1_048_576;
const MAX_QUERY_VALUE_BYTES: usize = 4_096;
const MAX_MARK_IDS: usize = 5_000;
const DEFAULT_PAGE_LIMIT: usize = 50;
const MAX_PAGE_LIMIT: usize = 500;

pub(super) type InboxApiError = (StatusCode, Json<Value>);

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route("/api/console/inbox/events", get(list_events))
        .route("/api/console/inbox/read", post(mark_read))
        .route("/api/console/inbox/events/{event_id}", delete(delete_event))
        .route("/api/console/inbox/traces/{run_id}", get(get_trace))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct InboxEvent {
    id: String,
    agent_id: String,
    source_type: String,
    source_id: String,
    event_type: String,
    status: String,
    severity: String,
    title: String,
    body: String,
    #[serde(default = "empty_object")]
    payload: Value,
    #[serde(default)]
    read: bool,
    created_at: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct InboxTraceEvent {
    at: f64,
    event: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct InboxTrace {
    run_id: String,
    created_at: f64,
    completed_at: Option<f64>,
    status: String,
    #[serde(default = "empty_object")]
    meta: Value,
    #[serde(default)]
    events: Vec<InboxTraceEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct InboxData {
    version: u32,
    events: Vec<InboxEvent>,
    traces: BTreeMap<String, InboxTrace>,
}

impl Default for InboxData {
    fn default() -> Self {
        Self {
            version: 1,
            events: Vec::new(),
            traces: BTreeMap::new(),
        }
    }
}

pub(super) struct NewInboxEvent {
    pub(super) agent_id: String,
    pub(super) source_type: String,
    pub(super) source_id: String,
    pub(super) event_type: String,
    pub(super) status: String,
    pub(super) severity: String,
    pub(super) title: String,
    pub(super) body: String,
    pub(super) payload: Value,
}

pub(super) struct NewInboxTrace {
    pub(super) run_id: String,
    pub(super) status: String,
    pub(super) meta: Value,
    pub(super) events: Vec<Value>,
    pub(super) error: Option<String>,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct InboxQuery {
    limit: usize,
    offset: usize,
    source_types: BTreeSet<String>,
    status: Option<String>,
    agent_id: Option<String>,
    unread_only: bool,
}

#[derive(Debug, Default, Deserialize)]
struct MarkReadBody {
    #[serde(default)]
    event_ids: Vec<String>,
    #[serde(default)]
    all: bool,
}

async fn list_events(
    State(server): State<AppServer>,
    RawQuery(raw_query): RawQuery,
) -> Result<Json<Value>, InboxApiError> {
    let query = parse_query(raw_query.as_deref().unwrap_or_default())?;
    let _guard = server.inner.desktop_inbox_lock.lock().await;
    let data = read_data(&server)?;
    let mut filtered = data
        .events
        .into_iter()
        .filter(|event| {
            (query.source_types.is_empty() || query.source_types.contains(&event.source_type))
                && query
                    .status
                    .as_ref()
                    .is_none_or(|status| event.status == *status)
                && query
                    .agent_id
                    .as_ref()
                    .is_none_or(|agent_id| event.agent_id == *agent_id)
        })
        .collect::<Vec<_>>();
    let unread_count = filtered.iter().filter(|event| !event.read).count();
    if query.unread_only {
        filtered.retain(|event| !event.read);
    }
    let total = filtered.len();
    let events = filtered
        .into_iter()
        .skip(query.offset)
        .take(query.limit)
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "events": events,
        "total": total,
        "unread_count": unread_count
    })))
}

async fn mark_read(
    State(server): State<AppServer>,
    Json(body): Json<MarkReadBody>,
) -> Result<Json<Value>, InboxApiError> {
    validate_mark_read_body(&body)?;
    let _guard = server.inner.desktop_inbox_lock.lock().await;
    let mut data = read_data(&server)?;
    let ids = body.event_ids.into_iter().collect::<BTreeSet<_>>();
    let mut updated = 0;
    for event in &mut data.events {
        if !event.read && (body.all || ids.contains(&event.id)) {
            event.read = true;
            updated += 1;
        }
    }
    if updated > 0 {
        write_data(&server, &data)?;
    }
    Ok(Json(json!({"updated": updated})))
}

async fn delete_event(
    State(server): State<AppServer>,
    Path(event_id): Path<String>,
) -> Result<Json<Value>, InboxApiError> {
    validate_identifier("event id", &event_id)?;
    let _guard = server.inner.desktop_inbox_lock.lock().await;
    let mut data = read_data(&server)?;
    let index = data
        .events
        .iter()
        .position(|event| event.id == event_id)
        .ok_or_else(|| not_found("event not found"))?;
    let event = data.events.remove(index);
    let run_id = trace_run_id(&event);
    let mut trace_deleted = false;
    if let Some(run_id) = run_id.as_deref() {
        let still_referenced = data
            .events
            .iter()
            .any(|candidate| trace_run_id(candidate).as_deref() == Some(run_id));
        if !still_referenced {
            trace_deleted = data.traces.remove(run_id).is_some();
        }
    }
    write_data(&server, &data)?;
    Ok(Json(json!({
        "deleted": true,
        "trace_deleted": trace_deleted,
        "run_id": run_id
    })))
}

async fn get_trace(
    State(server): State<AppServer>,
    Path(run_id): Path<String>,
) -> Result<Json<Value>, InboxApiError> {
    validate_identifier("run id", &run_id)?;
    let _guard = server.inner.desktop_inbox_lock.lock().await;
    let data = read_data(&server)?;
    let trace = data
        .traces
        .get(&run_id)
        .cloned()
        .ok_or_else(|| not_found("trace not found"))?;
    serde_json::to_value(trace)
        .map(Json)
        .map_err(|_| internal("Inbox trace could not be serialized"))
}

pub(super) async fn append_event(
    server: &AppServer,
    event: NewInboxEvent,
) -> Result<Value, InboxApiError> {
    let event = new_event(event);
    validate_event(&event).map_err(bad_stored_data)?;
    let serialized = serde_json::to_value(&event)
        .map_err(|_| internal("Inbox event could not be serialized"))?;
    let _guard = server.inner.desktop_inbox_lock.lock().await;
    let mut data = read_data(server)?;
    data.events.insert(0, event);
    data.events.truncate(MAX_EVENTS);
    write_data(server, &data)?;
    Ok(serialized)
}

pub(super) async fn append_event_with_trace(
    server: &AppServer,
    event: NewInboxEvent,
    trace: NewInboxTrace,
) -> Result<Value, InboxApiError> {
    validate_identifier("run id", &trace.run_id)?;
    if trace.events.len() > MAX_TRACE_EVENTS {
        return Err(unprocessable("Inbox trace has too many events"));
    }
    let now = now_epoch_seconds();
    let trace = InboxTrace {
        run_id: trace.run_id,
        created_at: now,
        completed_at: Some(now),
        status: trace.status,
        meta: trace.meta,
        events: trace
            .events
            .into_iter()
            .map(|event| InboxTraceEvent { at: now, event })
            .collect(),
        error: trace.error,
    };
    let event = new_event(event);
    if trace_run_id(&event).as_deref() != Some(trace.run_id.as_str()) {
        return Err(unprocessable("Inbox event and trace run ids do not match"));
    }
    validate_event(&event).map_err(bad_stored_data)?;
    let serialized = serde_json::to_value(&event)
        .map_err(|_| internal("Inbox event could not be serialized"))?;
    let _guard = server.inner.desktop_inbox_lock.lock().await;
    let mut data = read_data(server)?;
    data.traces.insert(trace.run_id.clone(), trace);
    data.events.insert(0, event);
    data.events.truncate(MAX_EVENTS);
    let referenced_runs = data
        .events
        .iter()
        .filter_map(trace_run_id)
        .collect::<BTreeSet<_>>();
    data.traces
        .retain(|run_id, _| referenced_runs.contains(run_id));
    write_data(server, &data)?;
    Ok(serialized)
}

fn new_event(event: NewInboxEvent) -> InboxEvent {
    InboxEvent {
        id: Uuid::now_v7().to_string(),
        agent_id: if event.agent_id.is_empty() {
            String::from("default")
        } else {
            event.agent_id
        },
        source_type: event.source_type,
        source_id: event.source_id,
        event_type: event.event_type,
        status: event.status,
        severity: event.severity,
        title: event.title,
        body: event.body,
        payload: event.payload,
        read: false,
        created_at: now_epoch_seconds(),
    }
}

pub(super) async fn mark_read_by_acl_sender(
    server: &AppServer,
    agent_id: &str,
    sender_address: &str,
) -> Result<usize, InboxApiError> {
    let sender_address = sender_address.trim().to_ascii_lowercase();
    if agent_id.is_empty() || sender_address.is_empty() {
        return Ok(0);
    }
    let _guard = server.inner.desktop_inbox_lock.lock().await;
    let mut data = read_data(server)?;
    let mut updated = 0;
    for event in &mut data.events {
        let sender = event
            .payload
            .get("acl_sender_address")
            .and_then(Value::as_str)
            .map(str::trim)
            .map(str::to_ascii_lowercase);
        if !event.read
            && event.agent_id == agent_id
            && event.payload.get("acl_status").and_then(Value::as_str) == Some("pending")
            && sender.as_deref() == Some(sender_address.as_str())
        {
            event.read = true;
            updated += 1;
        }
    }
    if updated > 0 {
        write_data(server, &data)?;
    }
    Ok(updated)
}

fn parse_query(raw_query: &str) -> Result<InboxQuery, InboxApiError> {
    let mut query = InboxQuery {
        limit: DEFAULT_PAGE_LIMIT,
        ..InboxQuery::default()
    };
    for (key, value) in form_urlencoded::parse(raw_query.as_bytes()) {
        if value.len() > MAX_QUERY_VALUE_BYTES {
            return Err(unprocessable("Inbox query value is too long"));
        }
        match key.as_ref() {
            "limit" => {
                query.limit = value
                    .parse::<usize>()
                    .map_err(|_| unprocessable("limit must be an integer"))?;
            }
            "offset" => {
                query.offset = value
                    .parse::<usize>()
                    .map_err(|_| unprocessable("offset must be an integer"))?;
            }
            "source_type" | "source_types" if !value.is_empty() => {
                query.source_types.insert(value.into_owned());
            }
            "status" if !value.is_empty() => query.status = Some(value.into_owned()),
            "agent_id" if !value.is_empty() => query.agent_id = Some(value.into_owned()),
            "unread_only" => query.unread_only = parse_bool(&value)?,
            _ => {}
        }
    }
    if !(1..=MAX_PAGE_LIMIT).contains(&query.limit) {
        return Err(unprocessable("limit must be between 1 and 500"));
    }
    Ok(query)
}

fn parse_bool(value: &str) -> Result<bool, InboxApiError> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(unprocessable("unread_only must be a boolean")),
    }
}

fn validate_mark_read_body(body: &MarkReadBody) -> Result<(), InboxApiError> {
    if body.event_ids.len() > MAX_MARK_IDS {
        return Err(unprocessable("too many Inbox event ids"));
    }
    for event_id in &body.event_ids {
        if event_id.len() > MAX_QUERY_VALUE_BYTES || event_id.chars().any(char::is_control) {
            return Err(unprocessable("event id is invalid"));
        }
    }
    Ok(())
}

fn validate_identifier(label: &str, value: &str) -> Result<(), InboxApiError> {
    if value.is_empty()
        || value.len() > MAX_QUERY_VALUE_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(unprocessable(&format!("{label} is invalid")));
    }
    Ok(())
}

fn read_data(server: &AppServer) -> Result<InboxData, InboxApiError> {
    let Some(serialized) = server.inner.core.read_inbox_data().map_err(internal)? else {
        return Ok(InboxData::default());
    };
    if serialized.len() > MAX_DATA_BYTES {
        return Err(internal("stored Inbox data exceeds its size limit"));
    }
    let data = serde_json::from_str::<InboxData>(&serialized)
        .map_err(|_| internal("stored Inbox data is invalid"))?;
    validate_data(&data).map_err(bad_stored_data)?;
    Ok(data)
}

fn write_data(server: &AppServer, data: &InboxData) -> Result<(), InboxApiError> {
    validate_data(data).map_err(bad_stored_data)?;
    let serialized = serde_json::to_string(data).map_err(|_| internal("Inbox data is invalid"))?;
    if serialized.len() > MAX_DATA_BYTES {
        return Err(unprocessable("Inbox data exceeds its size limit"));
    }
    server
        .inner
        .core
        .write_inbox_data(&serialized)
        .map_err(internal)
}

fn validate_data(data: &InboxData) -> Result<(), &'static str> {
    if data.version != 1 || data.events.len() > MAX_EVENTS || data.traces.len() > MAX_TRACES {
        return Err("stored Inbox data has an unsupported shape");
    }
    for event in &data.events {
        validate_event(event)?;
    }
    for (run_id, trace) in &data.traces {
        if run_id != &trace.run_id
            || trace.events.len() > MAX_TRACE_EVENTS
            || !valid_timestamp(trace.created_at)
            || trace
                .completed_at
                .is_some_and(|value| !valid_timestamp(value))
            || trace.status.len() > MAX_QUERY_VALUE_BYTES
        {
            return Err("stored Inbox trace is invalid");
        }
        validate_identifier_value(&trace.run_id)?;
        if trace.events.iter().any(|event| !valid_timestamp(event.at)) {
            return Err("stored Inbox trace event is invalid");
        }
    }
    Ok(())
}

fn validate_event(event: &InboxEvent) -> Result<(), &'static str> {
    validate_identifier_value(&event.id)?;
    for value in [
        &event.agent_id,
        &event.source_type,
        &event.source_id,
        &event.event_type,
        &event.status,
        &event.severity,
    ] {
        if value.len() > MAX_QUERY_VALUE_BYTES || value.contains('\0') {
            return Err("stored Inbox event metadata is invalid");
        }
    }
    if !valid_timestamp(event.created_at)
        || serde_json::to_vec(event).map_or(true, |serialized| serialized.len() > MAX_EVENT_BYTES)
    {
        return Err("stored Inbox event is invalid");
    }
    Ok(())
}

fn validate_identifier_value(value: &str) -> Result<(), &'static str> {
    if value.is_empty()
        || value.len() > MAX_QUERY_VALUE_BYTES
        || value.chars().any(char::is_control)
    {
        return Err("stored Inbox identifier is invalid");
    }
    Ok(())
}

fn valid_timestamp(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}

fn trace_run_id(event: &InboxEvent) -> Option<String> {
    event
        .payload
        .get("run_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn now_epoch_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

fn empty_object() -> Value {
    json!({})
}

fn bad_stored_data(detail: &'static str) -> InboxApiError {
    internal(detail)
}

fn internal(error_value: impl std::fmt::Display) -> InboxApiError {
    tracing::warn!(error = %error_value, "Desktop Inbox persistence failed");
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Desktop Inbox data could not be persisted",
    )
}

fn not_found(detail: &str) -> InboxApiError {
    error(StatusCode::NOT_FOUND, detail)
}

fn unprocessable(detail: &str) -> InboxApiError {
    error(StatusCode::UNPROCESSABLE_ENTITY, detail)
}

fn error(status: StatusCode, detail: &str) -> InboxApiError {
    (status, Json(json!({"detail": detail})))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_repeated_sources_and_python_style_booleans() {
        assert_eq!(
            parse_query(
                "limit=5&offset=2&source_type=mail&source_types=cron&source_types=heartbeat&unread_only=yes"
            )
            .unwrap(),
            InboxQuery {
                limit: 5,
                offset: 2,
                source_types: BTreeSet::from([
                    String::from("cron"),
                    String::from("heartbeat"),
                    String::from("mail"),
                ]),
                status: None,
                agent_id: None,
                unread_only: true,
            }
        );
        assert!(parse_query("limit=0").is_err());
        assert!(parse_query("unread_only=maybe").is_err());
    }

    #[test]
    fn validates_trace_keys_and_finite_timestamps() {
        let mut data = InboxData::default();
        data.traces.insert(
            String::from("key"),
            InboxTrace {
                run_id: String::from("different"),
                created_at: 1.0,
                completed_at: None,
                status: String::from("running"),
                meta: json!({}),
                events: Vec::new(),
                error: None,
            },
        );
        assert_eq!(validate_data(&data), Err("stored Inbox trace is invalid"));
    }

    #[test]
    fn rejects_event_counts_above_the_persistence_limit() {
        let event = InboxEvent {
            id: String::from("event"),
            agent_id: String::from("default"),
            source_type: String::from("cron"),
            source_id: String::from("job"),
            event_type: String::from("cron_result"),
            status: String::from("success"),
            severity: String::from("info"),
            title: String::from("title"),
            body: String::from("body"),
            payload: json!({}),
            read: false,
            created_at: 1.0,
        };
        let data = InboxData {
            version: 1,
            events: vec![event; MAX_EVENTS + 1],
            traces: BTreeMap::new(),
        };
        assert_eq!(
            validate_data(&data),
            Err("stored Inbox data has an unsupported shape")
        );
    }
}
