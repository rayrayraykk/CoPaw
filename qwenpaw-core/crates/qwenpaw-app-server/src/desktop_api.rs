use std::convert::Infallible;
use std::path::PathBuf;

use axum::Json;
use axum::Router;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Sse;
use axum::response::sse::Event;
use axum::response::sse::KeepAlive;
use axum::routing::get;
use axum::routing::post;
use axum::routing::put;
use chrono::DateTime;
use chrono::SecondsFormat;
use futures_util::stream;
use qwenpaw_core::CoreError;
use qwenpaw_protocol::ApprovalDecision;
use qwenpaw_protocol::ConfigWriteParams;
use qwenpaw_protocol::CoreEvent;
use qwenpaw_protocol::Item;
use qwenpaw_protocol::Thread;
use qwenpaw_protocol::ThreadArchiveParams;
use qwenpaw_protocol::ThreadListParams;
use qwenpaw_protocol::ThreadResumeParams;
use qwenpaw_protocol::ThreadStartParams;
use qwenpaw_protocol::ThreadStatus;
use qwenpaw_protocol::TurnInterruptParams;
use qwenpaw_protocol::TurnStartParams;
use qwenpaw_protocol::TurnStatus;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;

use super::AppServer;
use super::DesktopPendingApproval;

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route("/api/auth/status", get(auth_status))
        .route("/api/auth/verify", get(auth_verify))
        .route("/api/settings/language", get(language))
        .route("/api/settings/upload-limit", get(upload_limit))
        .route("/api/agents", get(agents))
        .route("/api/models", get(models))
        .route(
            "/api/models/active",
            get(active_models).put(set_active_model),
        )
        .route("/api/models/{provider_id}/config", put(configure_provider))
        .route("/api/models/{provider_id}/models", post(add_model))
        .route("/api/chats", get(chats))
        .route("/api/chats/groups", get(chat_groups))
        .route("/api/chats/{chat_id}", get(chat_history))
        .route(
            "/api/chats/{chat_id}/project-dir",
            get(chat_project_directory),
        )
        .route(
            "/api/chats/{chat_id}/project-dirs",
            get(chat_project_directories)
                .put(set_chat_project_directories)
                .delete(clear_chat_project_directories),
        )
        .route("/api/chats/{chat_id}/archive", post(archive_chat))
        .route("/api/chats/{chat_id}/unarchive", post(unarchive_chat))
        .route("/api/console/chat", post(console_chat))
        .route("/api/console/chat/stop", post(stop_console_chat))
        .route("/api/approval/approve", post(approve_tool))
        .route("/api/approval/deny", post(deny_tool))
        .route("/api/coding-mode", get(coding_mode))
        .route("/api/loops", get(loop_modes))
        .route("/api/loops/status", get(loop_status))
        .route("/api/skills", get(skills))
        .route("/api/workspace/running-config", get(running_config))
        .route(
            "/api/workspace/transcription-provider-type",
            get(transcription_provider_type),
        )
        .route(
            "/api/workspace/project-directory",
            get(project_directory).put(set_project_directory),
        )
        .route(
            "/api/workspace/project-directory/list",
            get(project_directory_list),
        )
        .route(
            "/api/workspace/project-directory/browse-dirs",
            get(browse_directories),
        )
        .route(
            "/api/workspace/project-directory/browse-dirs/create",
            post(create_directory),
        )
        .route("/api/console/push-messages", get(push_messages))
        .route("/api/console/inbox/events", get(inbox_events))
        .route("/api/frontend_plugin", get(frontend_plugins))
}

async fn auth_status() -> Json<Value> {
    Json(json!({"enabled": false, "has_users": false}))
}

async fn auth_verify() -> Json<Value> {
    Json(json!({"valid": true, "username": ""}))
}

async fn language() -> Json<Value> {
    Json(json!({"language": "en"}))
}

async fn upload_limit() -> Json<Value> {
    Json(json!({"upload_max_size_mb": 32}))
}

async fn loop_modes() -> Json<Value> {
    Json(json!([{
        "id": "default",
        "name": "default",
        "slash_command": "",
        "description": "The standard guarded agent loop.",
        "source": "builtin",
        "name_i18n": null,
        "description_i18n": null
    }]))
}

async fn loop_status() -> Json<Value> {
    Json(json!({"state": "idle", "mode": null}))
}

async fn skills() -> Json<Value> {
    Json(json!([]))
}

async fn running_config() -> Json<Value> {
    Json(json!({"approval_level": "AUTO"}))
}

async fn transcription_provider_type() -> Json<Value> {
    Json(json!({"transcription_provider_type": "disabled"}))
}

async fn agents(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let workspace = selected_desktop_workspace(&server).await?;
    Ok(Json(json!({
        "agents": [{
            "id": "default",
            "name": "QwenPaw",
            "description": "Rust Core",
            "workspace_dir": workspace.to_string_lossy(),
            "enabled": true,
            "pinned": true,
            "startup_status": "running",
            "backend": "qwenpaw",
            "backend_capabilities": {"workspace_ui": true},
            "available_in_chat": true
        }]
    })))
}

async fn models(State(server): State<AppServer>) -> Json<Value> {
    let config = server.inner.core.read_config().config;
    Json(json!([provider_info(&config)]))
}

fn provider_info(config: &qwenpaw_protocol::CoreConfig) -> Value {
    let api_key = if config.api_key_configured {
        "********"
    } else {
        ""
    };
    json!({
        "id": "openai-compatible",
        "name": "OpenAI Compatible",
        "api_key_prefix": "",
        "chat_model": "OpenAIChatModel",
        "models": [model_info(&config.default_model)],
        "extra_models": [],
        "discovered_models": [],
        "models_last_synced_at": null,
        "models_last_sync_error": null,
        "models_syncing": false,
        "hidden_model_ids": [],
        "is_custom": false,
        "is_local": false,
        "support_model_discovery": false,
        "support_connection_check": false,
        "freeze_url": false,
        "require_api_key": true,
        "api_key": api_key,
        "base_url": config.base_url,
        "generate_kwargs": {},
        "supports_oauth": false,
        "oauth_connected": false
    })
}

fn model_info(model: &str) -> Value {
    json!({
        "id": model,
        "name": model,
        "supports_multimodal": null,
        "supports_image": null,
        "supports_video": null,
        "max_input_length": 128_000,
        "generate_kwargs": {},
        "relay_reasoning": false,
        "thinking_enabled": null,
        "thinking_budget": null,
        "reasoning_effort": null
    })
}

async fn active_models(State(server): State<AppServer>) -> Json<Value> {
    let config = server.inner.core.read_config().config;
    Json(active_model_info(&config.default_model))
}

fn active_model_info(model: &str) -> Value {
    json!({
        "active_llm": {
            "provider_id": "openai-compatible",
            "model": model
        },
        "effective_max_input_length": 128_000
    })
}

#[derive(Debug, Deserialize)]
struct ProviderConfigRequest {
    #[serde(default)]
    api_key: Option<String>,
    #[serde(default)]
    base_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ActiveModelRequest {
    provider_id: String,
    model: String,
    scope: String,
    #[serde(default)]
    agent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AddModelRequest {
    id: String,
}

async fn configure_provider(
    State(server): State<AppServer>,
    Path(provider_id): Path<String>,
    Json(request): Json<ProviderConfigRequest>,
) -> Result<Json<Value>, ApiError> {
    require_default_provider(&provider_id)?;
    let api_key = request
        .api_key
        .as_deref()
        .map(normalize_api_key)
        .transpose()?;
    if request.base_url.is_some() {
        server
            .inner
            .core
            .write_config(ConfigWriteParams {
                base_url: request.base_url,
                default_model: None,
            })
            .map_err(|error| api_error(&error))?;
    }
    if let Some(api_key) = api_key {
        save_desktop_api_key(&server, api_key.clone()).await?;
        server
            .inner
            .core
            .set_runtime_api_key(api_key)
            .map_err(|error| api_error(&error))?;
    }
    let config = server.inner.core.read_config().config;
    Ok(Json(provider_info(&config)))
}

async fn set_active_model(
    State(server): State<AppServer>,
    Json(request): Json<ActiveModelRequest>,
) -> Result<Json<Value>, ApiError> {
    require_default_provider(&request.provider_id)?;
    if !matches!(request.scope.as_str(), "global" | "agent")
        || request
            .agent_id
            .as_deref()
            .is_some_and(|agent_id| agent_id != "default")
    {
        return Err(bad_request(
            "only the default local agent scope is supported",
        ));
    }
    let response = server
        .inner
        .core
        .write_config(ConfigWriteParams {
            base_url: None,
            default_model: Some(request.model),
        })
        .map_err(|error| api_error(&error))?;
    Ok(Json(active_model_info(&response.config.default_model)))
}

async fn add_model(
    State(server): State<AppServer>,
    Path(provider_id): Path<String>,
    Json(request): Json<AddModelRequest>,
) -> Result<Json<Value>, ApiError> {
    require_default_provider(&provider_id)?;
    let response = server
        .inner
        .core
        .write_config(ConfigWriteParams {
            base_url: None,
            default_model: Some(request.id),
        })
        .map_err(|error| api_error(&error))?;
    Ok(Json(provider_info(&response.config)))
}

fn require_default_provider(provider_id: &str) -> Result<(), ApiError> {
    if provider_id == "openai-compatible" {
        Ok(())
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(json!({"detail": "Model provider not found"})),
        ))
    }
}

fn normalize_api_key(api_key: &str) -> Result<Option<String>, ApiError> {
    let api_key = api_key.trim().to_owned();
    if api_key.is_empty() {
        return Ok(None);
    }
    if api_key.len() > 8_192 || api_key.chars().any(char::is_control) {
        return Err(bad_request(
            "API key must contain at most 8192 bytes without control characters",
        ));
    }
    Ok(Some(api_key))
}

async fn save_desktop_api_key(server: &AppServer, api_key: Option<String>) -> Result<(), ApiError> {
    let credentials = server.inner.desktop_credentials.clone().ok_or_else(|| {
        (
            StatusCode::NOT_IMPLEMENTED,
            Json(json!({"detail": "Desktop credential storage is unavailable"})),
        )
    })?;
    let result = tokio::task::spawn_blocking(move || credentials.save_api_key(api_key.as_deref()))
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, "Desktop credential task failed");
            credential_store_error()
        })?;
    result.map_err(|error| {
        tracing::warn!(error = %error, "Desktop credential storage failed");
        credential_store_error()
    })
}

fn credential_store_error() -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"detail": "System credential storage is unavailable"})),
    )
}

#[derive(Debug, Default, Deserialize)]
struct ChatListQuery {
    archived: Option<bool>,
}

async fn chats(
    State(server): State<AppServer>,
    Query(query): Query<ChatListQuery>,
) -> Json<Vec<Value>> {
    let mut cursor = None;
    let mut threads = Vec::new();
    loop {
        let page = server
            .inner
            .core
            .list_threads(ThreadListParams {
                cursor,
                limit: Some(200),
                include_archived: true,
            })
            .await;
        threads.extend(page.data);
        let Some(next_cursor) = page.next_cursor else {
            break;
        };
        cursor = Some(next_cursor);
    }
    let aliases = server.inner.desktop_session_aliases.read().await;
    Json(
        threads
            .into_iter()
            .filter(|thread| {
                query
                    .archived
                    .is_none_or(|archived| thread.archived == archived)
            })
            .map(|thread| {
                let session_id = aliases.thread_to_client.get(&thread.id).map(String::as_str);
                chat_from_thread(&thread, session_id)
            })
            .collect(),
    )
}

fn chat_from_thread(thread: &Thread, session_id: Option<&str>) -> Value {
    let status = match thread.status {
        ThreadStatus::Active => "running",
        ThreadStatus::Idle | ThreadStatus::Error => "idle",
    };
    let created_at = timestamp(thread.created_at);
    let updated_at = timestamp(thread.updated_at);
    let archived_at = thread.archived.then(|| updated_at.clone()).flatten();
    json!({
        "id": thread.id,
        "session_id": session_id.unwrap_or(&thread.id),
        "user_id": "desktop",
        "channel": "console",
        "name": null,
        "created_at": created_at,
        "updated_at": updated_at,
        "last_finished_at": null,
        "meta": {
            "model": thread.model,
            "workspace_root": thread.workspace_root,
        },
        "status": status,
        "pinned": false,
        "archived_at": archived_at,
        "archived": thread.archived,
        "source": "chat",
        "group_id": null,
        "parent_session_id": null,
        "root_session_id": null,
    })
}

fn timestamp(seconds: i64) -> Option<String> {
    DateTime::from_timestamp(seconds, 0)
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Secs, true))
}

type ApiError = (StatusCode, Json<Value>);

#[derive(Debug, Deserialize)]
struct ConsoleChatRequest {
    #[serde(default)]
    input: Vec<Value>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    request_context: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct StopChatQuery {
    chat_id: String,
}

#[derive(Debug, Deserialize)]
struct ApprovalActionRequest {
    request_id: String,
    session_id: String,
    #[serde(default)]
    scope: Option<String>,
}

async fn console_chat(
    State(server): State<AppServer>,
    Json(request): Json<ConsoleChatRequest>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let workspace_root = console_workspace_root(request.request_context.as_ref())?;
    let thread_id = resolve_console_thread(
        &server,
        request.session_id.as_deref(),
        workspace_root.as_deref(),
    )
    .await?;
    let thread = server
        .inner
        .core
        .read_thread(&thread_id)
        .await
        .map_err(|error| api_error(&error))?
        .thread;
    let workspace_root = thread
        .workspace_root
        .as_deref()
        .ok_or_else(|| bad_request("Console chat requires a Workspace directory"))?;
    let input = super::desktop_files::console_user_input(
        &server,
        &request.input,
        std::path::Path::new(workspace_root),
    )
    .await?;
    let (started, mut core_events) = server
        .inner
        .core
        .start_turn(TurnStartParams {
            thread_id: thread_id.clone(),
            input,
        })
        .await
        .map_err(|error| api_error(&error))?;
    let turn_id = started.turn.id;
    let core = server.inner.core.clone();
    let stream_server = server.clone();
    let (event_tx, event_rx) = tokio::sync::mpsc::channel(64);
    tokio::spawn(async move {
        while let Some(core_event) = core_events.recv().await {
            let terminal = matches!(&core_event, CoreEvent::TurnCompleted(_));
            track_pending_approval(&stream_server, &core_event).await;
            if let Some(payload) = console_event(core_event) {
                let event = Ok(Event::default().data(payload.to_string()));
                if event_tx.send(event).await.is_err() {
                    let _ = core
                        .interrupt_turn(&TurnInterruptParams {
                            thread_id: thread_id.clone(),
                            turn_id: turn_id.clone(),
                        })
                        .await;
                    clear_turn_approvals(&stream_server, &turn_id).await;
                    return;
                }
            }
            if terminal {
                clear_turn_approvals(&stream_server, &turn_id).await;
                return;
            }
        }
        clear_turn_approvals(&stream_server, &turn_id).await;
    });
    let stream = stream::unfold(event_rx, |mut receiver| async move {
        receiver.recv().await.map(|event| (event, receiver))
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

async fn track_pending_approval(server: &AppServer, event: &CoreEvent) {
    match event {
        CoreEvent::ToolApprovalRequested(approval) => {
            let session_id = server
                .inner
                .desktop_session_aliases
                .read()
                .await
                .thread_to_client
                .get(&approval.thread_id)
                .cloned()
                .unwrap_or_else(|| approval.thread_id.clone());
            server.inner.desktop_pending_approvals.write().await.insert(
                approval.approval_id.clone(),
                DesktopPendingApproval {
                    thread_id: approval.thread_id.clone(),
                    turn_id: approval.turn_id.clone(),
                    call_id: approval.call_id.clone(),
                    tool_name: approval.tool_name.clone(),
                    arguments: approval.arguments.clone(),
                    workspace_root: approval.workspace_root.clone(),
                    session_id,
                    created_at: unix_timestamp(),
                },
            );
        }
        CoreEvent::ToolApprovalResolved(approval) => {
            server
                .inner
                .desktop_pending_approvals
                .write()
                .await
                .remove(&approval.approval_id);
        }
        _ => {}
    }
}

async fn clear_turn_approvals(server: &AppServer, turn_id: &str) {
    server
        .inner
        .desktop_pending_approvals
        .write()
        .await
        .retain(|_, approval| approval.turn_id != turn_id);
}

async fn approve_tool(
    State(server): State<AppServer>,
    Json(request): Json<ApprovalActionRequest>,
) -> Result<Json<Value>, ApiError> {
    if request.scope.as_deref() == Some("similar") {
        return Err((
            StatusCode::NOT_IMPLEMENTED,
            Json(json!({
                "detail": "Rust Core currently supports one-time approval only"
            })),
        ));
    }
    resolve_tool_approval(&server, request, ApprovalDecision::Approved).await
}

async fn deny_tool(
    State(server): State<AppServer>,
    Json(request): Json<ApprovalActionRequest>,
) -> Result<Json<Value>, ApiError> {
    resolve_tool_approval(&server, request, ApprovalDecision::Denied).await
}

async fn resolve_tool_approval(
    server: &AppServer,
    request: ApprovalActionRequest,
    decision: ApprovalDecision,
) -> Result<Json<Value>, ApiError> {
    let pending = server
        .inner
        .desktop_pending_approvals
        .read()
        .await
        .get(&request.request_id)
        .cloned()
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"detail": "Approval request not found"})),
            )
        })?;
    if pending.session_id != request.session_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({"detail": "Root session mismatch"})),
        ));
    }
    let response = server
        .inner
        .core
        .respond_tool_approval(qwenpaw_protocol::ToolApprovalRespondParams {
            approval_id: request.request_id.clone(),
            decision,
        })
        .await;
    server
        .inner
        .desktop_pending_approvals
        .write()
        .await
        .remove(&request.request_id);
    if !response.accepted {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({"detail": "Approval request expired"})),
        ));
    }
    let action = match decision {
        ApprovalDecision::Approved => "approved",
        ApprovalDecision::Denied => "denied",
    };
    Ok(Json(json!({
        "success": true,
        "message": format!("Tool '{}' {action}", pending.tool_name),
        "tool_name": pending.tool_name,
        "request_id": request.request_id
    })))
}

async fn stop_console_chat(
    State(server): State<AppServer>,
    Query(query): Query<StopChatQuery>,
) -> Json<Value> {
    let aliased = server
        .inner
        .desktop_session_aliases
        .read()
        .await
        .client_to_thread
        .get(&query.chat_id)
        .cloned();
    let thread_id = aliased.as_deref().unwrap_or(&query.chat_id);
    let Ok(thread) = server.inner.core.read_thread(thread_id).await else {
        return Json(json!({"stopped": false}));
    };
    let Some(turn_id) = thread
        .turns
        .iter()
        .rev()
        .find(|turn| turn.status == TurnStatus::InProgress)
        .map(|turn| turn.id.clone())
    else {
        return Json(json!({"stopped": false}));
    };
    let stopped = server
        .inner
        .core
        .interrupt_turn(&TurnInterruptParams {
            thread_id: thread_id.to_owned(),
            turn_id,
        })
        .await
        .is_ok_and(|response| response.accepted);
    Json(json!({"stopped": stopped}))
}

async fn resolve_console_thread(
    server: &AppServer,
    requested_session_id: Option<&str>,
    requested_workspace: Option<&str>,
) -> Result<String, ApiError> {
    let requested = requested_session_id.map(str::trim).unwrap_or_default();
    if requested.len() > 1024 || requested.chars().any(char::is_control) {
        return Err(bad_request("session_id is invalid"));
    }
    if let Some(thread_id) = server
        .inner
        .desktop_session_aliases
        .read()
        .await
        .client_to_thread
        .get(requested)
        .cloned()
    {
        if let Some(workspace) = requested_workspace {
            server
                .inner
                .core
                .set_thread_workspace(&thread_id, &PathBuf::from(workspace))
                .await
                .map_err(|error| api_error(&error))?;
        }
        return Ok(thread_id);
    }
    if !requested.is_empty() {
        match server.inner.core.read_thread(requested).await {
            Ok(_) => {
                if let Some(workspace) = requested_workspace {
                    server
                        .inner
                        .core
                        .set_thread_workspace(requested, &PathBuf::from(workspace))
                        .await
                        .map_err(|error| api_error(&error))?;
                }
                return Ok(requested.to_owned());
            }
            Err(CoreError::ThreadNotFound(_)) if is_console_local_session_id(requested) => {}
            Err(error) => return Err(api_error(&error)),
        }
    }
    let workspace_root = match requested_workspace {
        Some(workspace) => canonical_workspace_path(workspace)?,
        None => selected_desktop_workspace(server).await?,
    };
    let thread = server
        .inner
        .core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(workspace_root.to_string_lossy().into_owned()),
        })
        .await
        .map_err(|error| api_error(&error))?
        .thread;
    if !requested.is_empty() {
        let mut aliases = server.inner.desktop_session_aliases.write().await;
        aliases
            .client_to_thread
            .insert(requested.to_owned(), thread.id.clone());
        aliases
            .thread_to_client
            .insert(thread.id.clone(), requested.to_owned());
    }
    Ok(thread.id)
}

fn console_workspace_root(request_context: Option<&Value>) -> Result<Option<String>, ApiError> {
    let Some(project_dirs) =
        request_context.and_then(|context| context.get("session_project_dirs"))
    else {
        return Ok(None);
    };
    let project_dirs = project_dirs
        .as_array()
        .ok_or_else(|| bad_request("session_project_dirs must be an array"))?;
    if project_dirs.len() > 1 {
        return Err((
            StatusCode::NOT_IMPLEMENTED,
            Json(json!({"detail": "Rust Core currently supports one project directory per chat"})),
        ));
    }
    let Some(project) = project_dirs.first() else {
        return Ok(None);
    };
    let path = project
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| bad_request("session project directory path is required"))?;
    canonical_workspace_path(path).map(|path| Some(path.to_string_lossy().into_owned()))
}

fn is_console_local_session_id(value: &str) -> bool {
    let Some((timestamp, suffix)) = value.split_once('-') else {
        return false;
    };
    !timestamp.is_empty()
        && timestamp.bytes().all(|byte| byte.is_ascii_digit())
        && !suffix.is_empty()
        && suffix
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

fn console_event(event: CoreEvent) -> Option<Value> {
    match event {
        CoreEvent::TurnStarted(notification) => Some(json!({
            "object": "response",
            "id": notification.turn.id,
            "status": "in_progress",
            "created_at": unix_timestamp(),
            "output": []
        })),
        CoreEvent::ItemStarted(notification) => {
            console_item_event(&notification.item, "in_progress")
        }
        CoreEvent::AgentMessageDelta(notification) => Some(json!({
            "object": "content",
            "msg_id": notification.item_id,
            "index": 0,
            "type": "text",
            "delta": true,
            "status": "in_progress",
            "text": notification.delta
        })),
        CoreEvent::ItemCompleted(notification) => {
            console_item_event(&notification.item, "completed")
        }
        CoreEvent::TurnCompleted(notification) => {
            let status = match notification.turn.status {
                TurnStatus::InProgress => "in_progress",
                TurnStatus::Completed => "completed",
                TurnStatus::Interrupted => "canceled",
                TurnStatus::Failed => "failed",
            };
            Some(json!({
                "object": "response",
                "id": notification.turn.id,
                "status": status,
                "output": [],
                "error": notification.turn.error
            }))
        }
        CoreEvent::ToolApprovalRequested(_) | CoreEvent::ToolApprovalResolved(_) => None,
    }
}

fn console_item_event(item: &Item, status: &str) -> Option<Value> {
    match item {
        Item::UserMessage { .. } => None,
        Item::AgentMessage { id, .. } => Some(json!({
            "object": "message",
            "id": id,
            "type": "message",
            "role": "assistant",
            "status": status,
            "content": []
        })),
        Item::ToolCall {
            id,
            call_id,
            name,
            arguments,
        } => Some(json!({
            "object": "message",
            "id": id,
            "type": "tool_call",
            "role": "assistant",
            "status": status,
            "call_id": call_id,
            "content": [{
                "type": "data",
                "data": {"name": name, "arguments": arguments}
            }]
        })),
        Item::ToolResult {
            id,
            call_id,
            content,
            is_error,
        } => Some(json!({
            "object": "message",
            "id": id,
            "type": "tool_call_output",
            "role": "tool",
            "status": status,
            "call_id": call_id,
            "content": [{"type": "text", "text": content}],
            "is_error": is_error
        })),
    }
}

fn unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
        })
}

fn bad_request(message: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, Json(json!({"detail": message})))
}

async fn chat_history(
    State(server): State<AppServer>,
    Path(chat_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let response = server
        .inner
        .core
        .read_thread(&chat_id)
        .await
        .map_err(|error| api_error(&error))?;
    let timestamp = timestamp(response.thread.created_at).unwrap_or_default();
    let messages = response
        .turns
        .iter()
        .flat_map(|turn| turn.items.iter())
        .map(|item| message_from_item(item, &timestamp))
        .collect::<Vec<_>>();
    let status = match response.thread.status {
        ThreadStatus::Active => "running",
        ThreadStatus::Idle | ThreadStatus::Error => "idle",
    };
    Ok(Json(json!({"messages": messages, "status": status})))
}

fn message_from_item(item: &Item, timestamp: &str) -> Value {
    let metadata = json!({"timestamp": timestamp});
    match item {
        Item::UserMessage { id, text } => json!({
            "id": id,
            "role": "user",
            "content": [{"type": "text", "text": text}],
            "metadata": metadata
        }),
        Item::AgentMessage { id, text } => json!({
            "id": id,
            "role": "assistant",
            "type": "message",
            "content": [{"type": "text", "text": text}],
            "metadata": metadata
        }),
        Item::ToolCall {
            id,
            call_id,
            name,
            arguments,
        } => json!({
            "id": id,
            "role": "assistant",
            "type": "tool_call",
            "call_id": call_id,
            "name": name,
            "arguments": arguments,
            "content": [],
            "metadata": metadata
        }),
        Item::ToolResult {
            id,
            call_id,
            content,
            is_error,
        } => json!({
            "id": id,
            "role": "tool",
            "type": "tool_call_output",
            "call_id": call_id,
            "content": content,
            "is_error": is_error,
            "metadata": metadata
        }),
    }
}

async fn archive_chat(
    State(server): State<AppServer>,
    Path(chat_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let response = server
        .inner
        .core
        .archive_thread(&ThreadArchiveParams { thread_id: chat_id })
        .await
        .map_err(|error| api_error(&error))?;
    let aliases = server.inner.desktop_session_aliases.read().await;
    let session_id = aliases
        .thread_to_client
        .get(&response.thread.id)
        .map(String::as_str);
    Ok(Json(chat_from_thread(&response.thread, session_id)))
}

async fn unarchive_chat(
    State(server): State<AppServer>,
    Path(chat_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let response = server
        .inner
        .core
        .resume_thread(&ThreadResumeParams { thread_id: chat_id })
        .await
        .map_err(|error| api_error(&error))?;
    let aliases = server.inner.desktop_session_aliases.read().await;
    let session_id = aliases
        .thread_to_client
        .get(&response.thread.id)
        .map(String::as_str);
    Ok(Json(chat_from_thread(&response.thread, session_id)))
}

fn api_error(error: &CoreError) -> ApiError {
    let status = match error {
        CoreError::ThreadNotFound(_) => StatusCode::NOT_FOUND,
        CoreError::ThreadBusy(_) | CoreError::ThreadArchived(_) => StatusCode::CONFLICT,
        CoreError::EmptyInput | CoreError::FileReference(_) | CoreError::Workspace(_) => {
            StatusCode::BAD_REQUEST
        }
        CoreError::InputTooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
        CoreError::Model(_) => StatusCode::BAD_GATEWAY,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json(json!({"detail": error.to_string()})))
}

async fn chat_groups() -> Json<Value> {
    Json(json!([
        {
            "id": "default",
            "name": "Uncategorized",
            "order": 0,
            "kind": "default",
            "source": "chat",
            "pinned": true
        },
        {
            "id": "cron",
            "name": "Cron",
            "order": 1,
            "kind": "cron",
            "source": "cron",
            "pinned": false
        },
        {
            "id": "subagents",
            "name": "Subagents",
            "order": 2,
            "kind": "subagents",
            "source": "subagent",
            "pinned": false
        }
    ]))
}

async fn coding_mode() -> Json<Value> {
    Json(json!({"enabled": false, "agent_id": "default"}))
}

#[derive(Debug, Deserialize)]
struct ProjectDirectoryRequest {
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct BrowseDirectoriesQuery {
    path: Option<String>,
    #[serde(default)]
    show_hidden: bool,
}

#[derive(Debug, Deserialize)]
struct CreateDirectoryRequest {
    parent: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct ProjectDirectoryInput {
    path: String,
    #[serde(default)]
    label: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatProjectDirectoriesRequest {
    project_dirs: Vec<ProjectDirectoryInput>,
}

async fn project_directory(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let selected = selected_desktop_workspace(&server).await?;
    Ok(Json(project_directory_info(&server, &selected)?))
}

async fn set_project_directory(
    State(server): State<AppServer>,
    Json(request): Json<ProjectDirectoryRequest>,
) -> Result<Json<Value>, ApiError> {
    let workspace = desktop_workspace(&server)?;
    let selected = match request.path {
        Some(path) => canonical_workspace_path(&path)?,
        None => workspace.initial.clone(),
    };
    let selected = server
        .inner
        .core
        .write_preferred_workspace(&selected)
        .map(PathBuf::from)
        .map_err(|error| api_error(&error))?;
    selected.clone_into(&mut *workspace.selected.write().await);
    Ok(Json(project_directory_info(&server, &selected)?))
}

async fn project_directory_list(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let selected = selected_desktop_workspace(&server).await?;
    let mut paths = vec![selected.clone()];
    for workspace in server.inner.core.list_workspaces().await.data {
        let path = PathBuf::from(workspace.root);
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths.sort();
    let projects = paths
        .into_iter()
        .map(|path| {
            json!({
                "path": path.to_string_lossy(),
                "name": path_name(&path),
                "is_git": path.join(".git").is_dir(),
                "is_active": path == selected
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(Value::Array(projects)))
}

async fn browse_directories(
    State(server): State<AppServer>,
    Query(query): Query<BrowseDirectoriesQuery>,
) -> Result<Json<Value>, ApiError> {
    let requested = match query.path.as_deref().map(str::trim) {
        None | Some("") => selected_desktop_workspace(&server).await?,
        Some("~") => dirs::home_dir().unwrap_or(selected_desktop_workspace(&server).await?),
        Some(path) => PathBuf::from(path),
    };
    let current = canonical_workspace_path(&requested.to_string_lossy())?;
    let mut directories = std::fs::read_dir(&current)
        .map_err(|_| bad_request("directory cannot be read"))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !file_type.is_dir() || (!query.show_hidden && name.starts_with('.')) {
                return None;
            }
            Some(json!({
                "name": name,
                "path": entry.path().to_string_lossy()
            }))
        })
        .take(500)
        .collect::<Vec<_>>();
    directories.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));
    Ok(Json(json!({
        "current": current.to_string_lossy(),
        "parent": current.parent().map(|path| path.to_string_lossy()),
        "dirs": directories,
        "selectable": true
    })))
}

async fn create_directory(
    Json(request): Json<CreateDirectoryRequest>,
) -> Result<Json<Value>, ApiError> {
    let parent = canonical_workspace_path(&request.parent)?;
    validate_directory_name(&request.name)?;
    let created = parent.join(&request.name);
    std::fs::create_dir(&created).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"detail": format!("failed to create directory: {error}")})),
        )
    })?;
    let created = canonical_workspace_path(&created.to_string_lossy())?;
    Ok(Json(json!({
        "path": created.to_string_lossy(),
        "name": path_name(&created)
    })))
}

async fn chat_project_directory(
    State(server): State<AppServer>,
    Path(chat_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let thread = server
        .inner
        .core
        .read_thread(&chat_id)
        .await
        .map_err(|error| api_error(&error))?
        .thread;
    let root = thread
        .workspace_root
        .ok_or_else(|| bad_request("chat has no Workspace directory"))?;
    let selected = selected_desktop_workspace(&server).await?;
    Ok(Json(json!({
        "project_dir": root,
        "source": "session",
        "agent_project_dir": selected.to_string_lossy(),
        "exists": PathBuf::from(&root).is_dir()
    })))
}

async fn chat_project_directories(
    State(server): State<AppServer>,
    Path(chat_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let thread = server
        .inner
        .core
        .read_thread(&chat_id)
        .await
        .map_err(|error| api_error(&error))?
        .thread;
    let selected = selected_desktop_workspace(&server).await?;
    let directories = thread.workspace_root.map_or_else(Vec::new, |root| {
        let path = PathBuf::from(&root);
        vec![json!({
            "path": root,
            "label": null,
            "exists": path.is_dir(),
            "nested_with": null,
            "is_workspace": path == selected
        })]
    });
    Ok(Json(json!({
        "project_dirs": directories,
        "source": "session",
        "agent_project_dir": selected.to_string_lossy()
    })))
}

async fn set_chat_project_directories(
    State(server): State<AppServer>,
    Path(chat_id): Path<String>,
    Json(request): Json<ChatProjectDirectoriesRequest>,
) -> Result<Json<Value>, ApiError> {
    if request.project_dirs.len() > 1 {
        return Err((
            StatusCode::NOT_IMPLEMENTED,
            Json(json!({"detail": "Rust Core currently supports one project directory per chat"})),
        ));
    }
    let path = match request.project_dirs.first() {
        Some(directory) => {
            if directory
                .label
                .as_deref()
                .is_some_and(|label| label.len() > 256 || label.chars().any(char::is_control))
            {
                return Err(bad_request("project directory label is invalid"));
            }
            canonical_workspace_path(&directory.path)?
        }
        None => selected_desktop_workspace(&server).await?,
    };
    server
        .inner
        .core
        .set_thread_workspace(&chat_id, &path)
        .await
        .map_err(|error| api_error(&error))?;
    chat_project_directories(State(server), Path(chat_id)).await
}

async fn clear_chat_project_directories(
    State(server): State<AppServer>,
    Path(chat_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let selected = selected_desktop_workspace(&server).await?;
    server
        .inner
        .core
        .set_thread_workspace(&chat_id, &selected)
        .await
        .map_err(|error| api_error(&error))?;
    chat_project_directories(State(server), Path(chat_id)).await
}

fn desktop_workspace(server: &AppServer) -> Result<&super::DesktopWorkspace, ApiError> {
    server.inner.desktop_workspace.as_ref().ok_or_else(|| {
        (
            StatusCode::NOT_IMPLEMENTED,
            Json(json!({"detail": "Desktop Workspace is unavailable"})),
        )
    })
}

async fn selected_desktop_workspace(server: &AppServer) -> Result<PathBuf, ApiError> {
    Ok(desktop_workspace(server)?.selected.read().await.clone())
}

fn project_directory_info(server: &AppServer, selected: &PathBuf) -> Result<Value, ApiError> {
    let workspace = desktop_workspace(server)?;
    Ok(json!({
        "path": selected.to_string_lossy(),
        "name": path_name(selected),
        "is_workspace_default": selected == &workspace.initial,
        "workspace_dir": workspace.initial.to_string_lossy(),
        "exists": selected.is_dir()
    }))
}

fn canonical_workspace_path(path: &str) -> Result<PathBuf, ApiError> {
    let path = path.trim();
    if path.is_empty() || path.len() > 4_096 || path.chars().any(char::is_control) {
        return Err(bad_request("Workspace path is invalid"));
    }
    let canonical = PathBuf::from(path)
        .canonicalize()
        .map_err(|_| bad_request("Workspace directory does not exist"))?;
    if !canonical.is_dir() {
        return Err(bad_request("Workspace path is not a directory"));
    }
    Ok(canonical)
}

fn validate_directory_name(name: &str) -> Result<(), ApiError> {
    if name.is_empty()
        || name.len() > 255
        || name == "."
        || name == ".."
        || name
            .chars()
            .any(|character| character.is_control() || character == '/' || character == '\\')
    {
        return Err(bad_request("directory name is invalid"));
    }
    Ok(())
}

fn path_name(path: &std::path::Path) -> String {
    path.file_name().map_or_else(
        || path.to_string_lossy().into_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

async fn push_messages(State(server): State<AppServer>) -> Json<Value> {
    let pending = server.inner.desktop_pending_approvals.read().await;
    let mut approvals = pending.iter().collect::<Vec<_>>();
    approvals.sort_by(|(left_id, left), (right_id, right)| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left_id.cmp(right_id))
    });
    let pending_approvals = approvals
        .into_iter()
        .map(|(approval_id, approval)| {
            let arguments = serde_json::from_str::<Value>(&approval.arguments)
                .ok()
                .filter(Value::is_object)
                .unwrap_or_else(|| json!({"raw": approval.arguments}));
            json!({
                "request_id": approval_id,
                "session_id": approval.session_id,
                "root_session_id": approval.session_id,
                "owner_agent_id": "default",
                "agent_id": "default",
                "tool_name": approval.tool_name,
                "tool_display_name": approval.tool_name,
                "tool_source": "rust-core",
                "severity": "medium",
                "findings_count": 0,
                "findings_summary": "",
                "tool_params": arguments,
                "source_type": "tool_guard",
                "driver": "rust-core",
                "reasoning": "",
                "created_at": approval.created_at,
                "timeout_seconds": 120,
                "is_generalized": false,
                "exact_target": approval.workspace_root,
                "similar_target": null,
                "call_id": approval.call_id,
                "thread_id": approval.thread_id
            })
        })
        .collect::<Vec<_>>();
    Json(json!({"messages": [], "pending_approvals": pending_approvals}))
}

async fn inbox_events() -> Json<Value> {
    Json(json!({"events": [], "total": 0, "unread_count": 0}))
}

async fn frontend_plugins() -> Json<Value> {
    Json(json!([]))
}
