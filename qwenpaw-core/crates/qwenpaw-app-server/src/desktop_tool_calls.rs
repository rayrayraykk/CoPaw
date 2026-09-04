//! Unchanged Console compatibility routes for live Rust tool calls.

use std::convert::Infallible;

use axum::Json;
use axum::Router;
use axum::body::Bytes;
use axum::extract::Path;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Sse;
use axum::response::sse::Event;
use axum::response::sse::KeepAlive;
use axum::routing::get;
use axum::routing::post;
use futures_util::stream;
use qwenpaw_core::CoreError;
use qwenpaw_core::ToolCallControlError;
use qwenpaw_core::ToolCallSnapshot;
use qwenpaw_core::ToolCallStreamEvent;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;
use tokio::sync::mpsc;

use super::AppServer;

type ApiError = (StatusCode, Json<Value>);

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route(
            "/api/settings/offload-policy",
            get(get_offload_policy).put(set_offload_policy),
        )
        .route("/api/tool-calls/{session_id}", get(list_calls))
        .route("/api/tool-calls/{session_id}/{tool_call_id}", get(get_call))
        .route(
            "/api/tool-calls/{session_id}/{tool_call_id}/output",
            get(get_output),
        )
        .route(
            "/api/tool-calls/{session_id}/{tool_call_id}/stream",
            get(stream_output),
        )
        .route(
            "/api/tool-calls/{session_id}/{tool_call_id}/offload",
            post(offload_call),
        )
        .route(
            "/api/tool-calls/{session_id}/{tool_call_id}/cancel",
            post(cancel_call),
        )
        .route(
            "/api/tool-calls/{session_id}/{tool_call_id}/extend-deadline",
            post(extend_deadline),
        )
}

async fn get_offload_policy(State(server): State<AppServer>) -> Json<Value> {
    Json(json!({"default_action": server.inner.core.tool_offload_policy()}))
}

#[derive(Debug, Deserialize)]
struct OffloadPolicyRequest {
    default_action: String,
}

async fn set_offload_policy(
    State(server): State<AppServer>,
    Json(request): Json<OffloadPolicyRequest>,
) -> Result<Json<Value>, ApiError> {
    let policy = server
        .inner
        .core
        .set_tool_offload_policy(&request.default_action)
        .map_err(core_error)?;
    Ok(Json(json!({"default_action": policy})))
}

async fn list_calls(
    State(server): State<AppServer>,
    Path(session_id): Path<String>,
) -> Json<Value> {
    let Some(thread_id) = existing_thread_id(&server, &session_id).await else {
        return Json(json!({"items": [], "total": 0}));
    };
    let calls = server.inner.core.list_tool_calls(&thread_id).await;
    let items = calls
        .iter()
        .map(|call| info_value(call, &session_id))
        .collect::<Vec<_>>();
    Json(json!({"total": items.len(), "items": items}))
}

async fn get_call(
    State(server): State<AppServer>,
    Path((session_id, tool_call_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let call = scoped_call(&server, &session_id, &tool_call_id).await?;
    Ok(Json(info_value(&call, &session_id)))
}

async fn get_output(
    State(server): State<AppServer>,
    Path((session_id, tool_call_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let call = scoped_call(&server, &session_id, &tool_call_id).await?;
    Ok(Json(output_value(&call)))
}

async fn stream_output(
    State(server): State<AppServer>,
    Path((session_id, tool_call_id)): Path<(String, String)>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let thread_id = existing_thread_id(&server, &session_id)
        .await
        .ok_or_else(not_found)?;
    let mut subscription = server
        .inner
        .core
        .subscribe_tool_call(&thread_id, &tool_call_id)
        .await
        .map_err(control_error)?;
    let (sender, receiver) = mpsc::channel(4);
    tokio::spawn(async move {
        if subscription.snapshot.is_closed {
            for block in subscription.snapshot.content {
                if send_stream_event(&sender, ToolCallStreamEvent::Chunk(block))
                    .await
                    .is_err()
                {
                    return;
                }
            }
            let _ = send_stream_event(&sender, ToolCallStreamEvent::Done).await;
            return;
        }
        while let Some(event) = subscription.events.recv().await {
            let done = event == ToolCallStreamEvent::Done;
            if send_stream_event(&sender, event).await.is_err() || done {
                return;
            }
        }
    });
    let events = stream::unfold(receiver, |mut receiver| async move {
        receiver.recv().await.map(|event| (Ok(event), receiver))
    });
    Ok(Sse::new(events).keep_alive(KeepAlive::default()))
}

async fn send_stream_event(
    sender: &mpsc::Sender<Event>,
    event: ToolCallStreamEvent,
) -> Result<(), mpsc::error::SendError<Event>> {
    let payload = match event {
        ToolCallStreamEvent::Chunk(data) => json!({"type": "chunk", "data": data}),
        ToolCallStreamEvent::Done => json!({"type": "done"}),
    };
    sender
        .send(Event::default().data(payload.to_string()))
        .await
}

async fn offload_call(
    State(server): State<AppServer>,
    Path((session_id, tool_call_id)): Path<(String, String)>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let thread_id = existing_thread_id(&server, &session_id)
        .await
        .ok_or_else(not_found)?;
    server
        .inner
        .core
        .offload_tool_call(&thread_id, &tool_call_id)
        .await
        .map_err(|error| action_error(error, Action::Offload))?;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"status": "accepted", "tool_call_id": tool_call_id})),
    ))
}

#[derive(Debug, Default, Deserialize)]
struct CancelRequest {
    #[serde(default)]
    force: bool,
}

async fn cancel_call(
    State(server): State<AppServer>,
    Path((session_id, tool_call_id)): Path<(String, String)>,
    body: Bytes,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let request = if body.iter().all(u8::is_ascii_whitespace) {
        CancelRequest::default()
    } else {
        serde_json::from_slice::<CancelRequest>(&body)
            .map_err(|error| bad_request(&format!("invalid cancel request: {error}")))?
    };
    let thread_id = existing_thread_id(&server, &session_id)
        .await
        .ok_or_else(not_found)?;
    server
        .inner
        .core
        .cancel_tool_call(&thread_id, &tool_call_id, request.force)
        .await
        .map_err(|error| action_error(error, Action::Cancel))?;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"status": "accepted", "tool_call_id": tool_call_id})),
    ))
}

#[derive(Debug, Deserialize)]
struct ExtendRequest {
    #[serde(default)]
    seconds: Option<f64>,
    #[serde(default)]
    no_deadline: bool,
    #[serde(default = "default_deadline_target")]
    target: String,
}

fn default_deadline_target() -> String {
    String::from("offload")
}

async fn extend_deadline(
    State(server): State<AppServer>,
    Path((session_id, tool_call_id)): Path<(String, String)>,
    Json(request): Json<ExtendRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let thread_id = existing_thread_id(&server, &session_id)
        .await
        .ok_or_else(not_found)?;
    let call = server
        .inner
        .core
        .extend_tool_call_deadline(
            &thread_id,
            &tool_call_id,
            &request.target,
            request.seconds,
            request.no_deadline,
        )
        .await
        .map_err(|error| action_error(error, Action::Extend))?;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "status": "accepted",
            "tool_call_id": tool_call_id,
            "offload_remaining": call.offload_remaining,
            "kill_remaining": call.kill_remaining
        })),
    ))
}

async fn scoped_call(
    server: &AppServer,
    session_id: &str,
    tool_call_id: &str,
) -> Result<ToolCallSnapshot, ApiError> {
    let thread_id = existing_thread_id(server, session_id)
        .await
        .ok_or_else(not_found)?;
    server
        .inner
        .core
        .tool_call(&thread_id, tool_call_id)
        .await
        .map_err(control_error)
}

async fn existing_thread_id(server: &AppServer, session_id: &str) -> Option<String> {
    if let Some(thread_id) = server
        .inner
        .desktop_session_aliases
        .read()
        .await
        .client_to_thread
        .get(session_id)
        .cloned()
    {
        return Some(thread_id);
    }
    server
        .inner
        .core
        .read_thread(session_id)
        .await
        .ok()
        .map(|response| response.thread.id)
}

fn info_value(call: &ToolCallSnapshot, session_id: &str) -> Value {
    json!({
        "tool_call_id": call.tool_call_id,
        "tool_name": call.tool_name,
        "session_id": session_id,
        "agent_id": "default",
        "status": call.status,
        "started_at": call.started_at,
        "elapsed": call.elapsed,
        "offload_remaining": call.offload_remaining,
        "kill_remaining": call.kill_remaining,
        "end_state": call.end_state,
        "force_cancelled": call.force_cancelled,
        "extra": {},
        "max_internal_timeout_secs": call.max_internal_timeout_secs,
        "offload_reason": call.offload_reason
    })
}

fn output_value(call: &ToolCallSnapshot) -> Value {
    json!({
        "tool_call_id": call.tool_call_id,
        "is_closed": call.is_closed,
        "final_state": call.end_state,
        "content": call.content
    })
}

#[derive(Clone, Copy)]
enum Action {
    Offload,
    Cancel,
    Extend,
}

fn action_error(error: ToolCallControlError, action: Action) -> ApiError {
    if error == ToolCallControlError::NotFound {
        return not_found();
    }
    let detail = match action {
        Action::Offload => {
            "Cannot offload (not running, or kill window too short; extend timeout first)"
        }
        Action::Cancel => "Cannot cancel",
        Action::Extend => "Cannot extend deadline (capped or invalid)",
    };
    (StatusCode::CONFLICT, Json(json!({"detail": detail})))
}

fn control_error(error: ToolCallControlError) -> ApiError {
    if error == ToolCallControlError::NotFound {
        not_found()
    } else {
        (
            StatusCode::CONFLICT,
            Json(json!({"detail": error.to_string()})),
        )
    }
}

fn core_error(error: CoreError) -> ApiError {
    match error {
        CoreError::Config(message) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"detail": message})),
        ),
        other => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"detail": other.to_string()})),
        ),
    }
}

fn bad_request(detail: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, Json(json!({"detail": detail})))
}

fn not_found() -> ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(json!({"detail": "Tool call not found"})),
    )
}
