use std::collections::BTreeMap;
use std::collections::HashSet;

use axum::Json;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::DateTime;
use chrono::SecondsFormat;
use qwenpaw_protocol::Thread;
use qwenpaw_protocol::ThreadArchiveParams;
use qwenpaw_protocol::ThreadListParams;
use qwenpaw_protocol::ThreadResumeParams;
use qwenpaw_protocol::ThreadStartParams;
use qwenpaw_protocol::ThreadStatus;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Map;
use serde_json::Value;
use serde_json::json;
use uuid::Uuid;

use super::AppServer;

const MAX_CATALOG_BYTES: usize = 8 * 1_024 * 1_024;
const MAX_CHATS: usize = 5_000;
const MAX_GROUPS: usize = 256;
const MAX_BATCH_SIZE: usize = 500;
const MAX_ID_BYTES: usize = 1_024;
const MAX_CHAT_NAME_BYTES: usize = 4_096;
const MAX_GROUP_NAME_CHARS: usize = 64;

type ApiError = (StatusCode, Json<Value>);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct ChatCatalog {
    version: u32,
    chats: BTreeMap<String, ChatMetadata>,
    groups: Vec<ChatGroup>,
}

impl Default for ChatCatalog {
    fn default() -> Self {
        Self {
            version: 1,
            chats: BTreeMap::new(),
            groups: default_groups(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ChatMetadata {
    name: String,
    session_id: String,
    user_id: String,
    channel: String,
    meta: Map<String, Value>,
    pinned: bool,
    source: String,
    group_id: String,
    parent_session_id: Option<String>,
    root_session_id: Option<String>,
    updated_at: i64,
    last_finished_at: Option<i64>,
}

#[derive(Clone, Debug)]
pub(super) struct CheckpointSessionInfo {
    pub(super) thread_id: String,
    pub(super) session_id: String,
    pub(super) user_id: String,
    pub(super) channel: String,
    pub(super) title: String,
    pub(super) archived: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct ChatGroup {
    id: String,
    name: String,
    order: usize,
    kind: String,
    source: Option<String>,
    pinned: bool,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct ChatListQuery {
    user_id: Option<String>,
    channel: Option<String>,
    archived: Option<bool>,
    #[allow(dead_code)]
    include_app_owned: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateChatRequest {
    #[serde(default = "default_chat_name")]
    name: String,
    session_id: String,
    user_id: String,
    #[serde(default = "default_channel")]
    channel: String,
    #[serde(default)]
    meta: Map<String, Value>,
    #[serde(default = "default_source")]
    source: String,
    #[serde(default)]
    group_id: Option<String>,
    #[serde(default)]
    parent_session_id: Option<String>,
    #[serde(default)]
    root_session_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UpdateChatRequest {
    name: Option<String>,
    pinned: Option<bool>,
    group_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CreateGroupRequest {
    name: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UpdateGroupRequest {
    name: Option<String>,
    pinned: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReorderGroupsRequest {
    group_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BatchChatIds {
    chat_ids: Vec<String>,
}

pub(super) async fn list_chats(
    State(server): State<AppServer>,
    Query(query): Query<ChatListQuery>,
) -> Result<Json<Vec<Value>>, ApiError> {
    let threads = list_all_threads(&server).await;
    let aliases = server.inner.desktop_session_aliases.read().await;
    let _guard = server.inner.desktop_chat_catalog_lock.lock().await;
    let mut catalog = read_catalog(&server)?;
    let thread_ids = threads
        .iter()
        .map(|thread| thread.id.as_str())
        .collect::<HashSet<_>>();
    let before = catalog.chats.len();
    catalog
        .chats
        .retain(|thread_id, _| thread_ids.contains(thread_id.as_str()));
    let mut changed = before != catalog.chats.len();
    let mut chats = Vec::new();
    for thread in threads {
        let session_id = aliases
            .thread_to_client
            .get(&thread.id)
            .map_or(thread.id.as_str(), String::as_str);
        if !catalog.chats.contains_key(&thread.id) {
            catalog
                .chats
                .insert(thread.id.clone(), default_metadata(&thread, session_id));
            changed = true;
        }
        let metadata = catalog
            .chats
            .get(&thread.id)
            .expect("metadata should be present");
        if query
            .archived
            .is_some_and(|archived| archived != thread.archived)
            || query
                .user_id
                .as_deref()
                .is_some_and(|user_id| !user_id.is_empty() && user_id != metadata.user_id)
            || query
                .channel
                .as_deref()
                .is_some_and(|channel| channel != metadata.channel)
        {
            continue;
        }
        chats.push(chat_spec(&thread, metadata));
    }
    if changed {
        write_catalog(&server, &catalog)?;
    }
    Ok(Json(chats))
}

pub(super) async fn create_chat(
    State(server): State<AppServer>,
    Json(request): Json<CreateChatRequest>,
) -> Result<Json<Value>, ApiError> {
    validate_chat_name(&request.name)?;
    validate_identifier("session_id", &request.session_id)?;
    validate_identifier("user_id", &request.user_id)?;
    validate_identifier("channel", &request.channel)?;
    validate_source(&request.source)?;
    validate_optional_identifier("parent_session_id", request.parent_session_id.as_deref())?;
    validate_optional_identifier("root_session_id", request.root_session_id.as_deref())?;
    let workspace = workspace_from_meta(&server, &request.meta).await?;
    let group_id = request
        .group_id
        .clone()
        .unwrap_or_else(|| default_group_for_source(&request.source).to_owned());
    let guard = server.inner.desktop_chat_catalog_lock.lock().await;
    let mut catalog = read_catalog(&server)?;
    require_group(&catalog, &group_id)?;
    if catalog.chats.len() >= MAX_CHATS {
        return Err(unprocessable("too many chats"));
    }
    let thread = server
        .inner
        .core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(workspace),
        })
        .await
        .map_err(core_error)?
        .thread;
    let metadata = ChatMetadata {
        name: request.name,
        session_id: request.session_id.clone(),
        user_id: request.user_id,
        channel: request.channel,
        meta: request.meta,
        pinned: false,
        source: request.source,
        group_id,
        parent_session_id: request.parent_session_id,
        root_session_id: request.root_session_id,
        updated_at: thread.updated_at,
        last_finished_at: None,
    };
    catalog.chats.insert(thread.id.clone(), metadata.clone());
    if let Err(error) = write_catalog(&server, &catalog) {
        drop(guard);
        let _ = server.inner.core.delete_thread(&thread.id).await;
        return Err(error);
    }
    drop(guard);
    let mut aliases = server.inner.desktop_session_aliases.write().await;
    aliases
        .client_to_thread
        .insert(request.session_id.clone(), thread.id.clone());
    aliases
        .thread_to_client
        .insert(thread.id.clone(), request.session_id);
    Ok(Json(chat_spec(&thread, &metadata)))
}

pub(super) async fn update_chat(
    State(server): State<AppServer>,
    Path(chat_id): Path<String>,
    Json(request): Json<UpdateChatRequest>,
) -> Result<Json<Value>, ApiError> {
    validate_identifier("chat id", &chat_id)?;
    if let Some(name) = request.name.as_deref() {
        validate_chat_name(name)?;
    }
    let thread = server
        .inner
        .core
        .read_thread(&chat_id)
        .await
        .map_err(core_error)?
        .thread;
    let aliases = server.inner.desktop_session_aliases.read().await;
    let session_id = aliases
        .thread_to_client
        .get(&chat_id)
        .map_or(chat_id.as_str(), String::as_str);
    let _guard = server.inner.desktop_chat_catalog_lock.lock().await;
    let mut catalog = read_catalog(&server)?;
    if let Some(group_id) = request.group_id.as_deref() {
        require_group(&catalog, group_id)?;
    }
    let metadata = catalog
        .chats
        .entry(chat_id.clone())
        .or_insert_with(|| default_metadata(&thread, session_id));
    let only_group_update =
        request.group_id.is_some() && request.name.is_none() && request.pinned.is_none();
    if let Some(name) = request.name {
        metadata.name = name;
    }
    if let Some(pinned) = request.pinned {
        metadata.pinned = pinned;
    }
    if let Some(group_id) = request.group_id {
        metadata.group_id = group_id;
    }
    if !only_group_update {
        metadata.updated_at = now();
    }
    let metadata = metadata.clone();
    write_catalog(&server, &catalog)?;
    Ok(Json(chat_spec(&thread, &metadata)))
}

pub(super) async fn delete_chat(
    State(server): State<AppServer>,
    Path(chat_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_identifier("chat id", &chat_id)?;
    server
        .inner
        .core
        .delete_thread(&chat_id)
        .await
        .map_err(core_error)?;
    remove_chat_state(&server, &[chat_id]).await?;
    Ok(Json(json!({"deleted": true})))
}

pub(super) async fn batch_delete_chats(
    State(server): State<AppServer>,
    Json(chat_ids): Json<Vec<String>>,
) -> Result<Json<Value>, ApiError> {
    validate_batch(&chat_ids)?;
    let mut deleted = Vec::new();
    for chat_id in &chat_ids {
        match server.inner.core.delete_thread(chat_id).await {
            Ok(_) => deleted.push(chat_id.clone()),
            Err(qwenpaw_core::CoreError::ThreadNotFound(_)) => {}
            Err(error) => return Err(core_error(error)),
        }
    }
    remove_chat_state(&server, &deleted).await?;
    Ok(Json(json!({"deleted": !deleted.is_empty()})))
}

pub(super) async fn archive_chat(
    State(server): State<AppServer>,
    Path(chat_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let thread = server
        .inner
        .core
        .archive_thread(&ThreadArchiveParams {
            thread_id: chat_id.clone(),
        })
        .await
        .map_err(core_error)?
        .thread;
    let metadata = metadata_for_thread(&server, &thread).await?;
    Ok(Json(chat_spec(&thread, &metadata)))
}

pub(super) async fn unarchive_chat(
    State(server): State<AppServer>,
    Path(chat_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let thread = server
        .inner
        .core
        .resume_thread(&ThreadResumeParams {
            thread_id: chat_id.clone(),
        })
        .await
        .map_err(core_error)?
        .thread;
    let metadata = metadata_for_thread(&server, &thread).await?;
    Ok(Json(chat_spec(&thread, &metadata)))
}

pub(super) async fn batch_archive_chats(
    State(server): State<AppServer>,
    Json(request): Json<BatchChatIds>,
) -> Result<Json<Value>, ApiError> {
    validate_batch(&request.chat_ids)?;
    let mut succeeded = Vec::new();
    let mut failed = Vec::new();
    for chat_id in request.chat_ids {
        match server
            .inner
            .core
            .archive_thread(&ThreadArchiveParams {
                thread_id: chat_id.clone(),
            })
            .await
        {
            Ok(_) => succeeded.push(chat_id),
            Err(qwenpaw_core::CoreError::ThreadNotFound(_)) => failed.push(json!({
                "chat_id": chat_id,
                "reason": "not_found",
                "message": format!("Chat not found: {chat_id}")
            })),
            Err(qwenpaw_core::CoreError::ThreadBusy(_)) => failed.push(json!({
                "chat_id": chat_id,
                "reason": "in_progress",
                "message": "Chat is running"
            })),
            Err(error) => return Err(core_error(error)),
        }
    }
    Ok(Json(json!({"succeeded": succeeded, "failed": failed})))
}

pub(super) async fn batch_unarchive_chats(
    State(server): State<AppServer>,
    Json(request): Json<BatchChatIds>,
) -> Result<Json<Value>, ApiError> {
    validate_batch(&request.chat_ids)?;
    let mut succeeded = Vec::new();
    let mut failed = Vec::new();
    for chat_id in request.chat_ids {
        match server
            .inner
            .core
            .resume_thread(&ThreadResumeParams {
                thread_id: chat_id.clone(),
            })
            .await
        {
            Ok(_) => succeeded.push(chat_id),
            Err(qwenpaw_core::CoreError::ThreadNotFound(_)) => failed.push(json!({
                "chat_id": chat_id,
                "reason": "not_found",
                "message": format!("Chat not found: {chat_id}")
            })),
            Err(error) => return Err(core_error(error)),
        }
    }
    Ok(Json(json!({"succeeded": succeeded, "failed": failed})))
}

pub(super) async fn list_groups(
    State(server): State<AppServer>,
) -> Result<Json<Vec<ChatGroup>>, ApiError> {
    let _guard = server.inner.desktop_chat_catalog_lock.lock().await;
    let catalog = read_catalog(&server)?;
    Ok(Json(ordered_groups(catalog.groups)))
}

pub(super) async fn create_group(
    State(server): State<AppServer>,
    Json(request): Json<CreateGroupRequest>,
) -> Result<Json<ChatGroup>, ApiError> {
    let name = normalize_group_name(&request.name)?;
    let _guard = server.inner.desktop_chat_catalog_lock.lock().await;
    let mut catalog = read_catalog(&server)?;
    if catalog.groups.len() >= MAX_GROUPS {
        return Err(unprocessable("too many chat groups"));
    }
    let group = ChatGroup {
        id: Uuid::now_v7().to_string(),
        name,
        order: catalog
            .groups
            .iter()
            .map(|group| group.order)
            .max()
            .unwrap_or_default()
            .saturating_add(1),
        kind: String::from("custom"),
        source: None,
        pinned: false,
    };
    catalog.groups.push(group.clone());
    write_catalog(&server, &catalog)?;
    Ok(Json(group))
}

pub(super) async fn update_group(
    State(server): State<AppServer>,
    Path(group_id): Path<String>,
    Json(request): Json<UpdateGroupRequest>,
) -> Result<Json<ChatGroup>, ApiError> {
    validate_identifier("group id", &group_id)?;
    if request.name.is_none() && request.pinned.is_none() {
        return Err(unprocessable("At least one group field must be provided"));
    }
    let name = request
        .name
        .as_deref()
        .map(normalize_group_name)
        .transpose()?;
    let _guard = server.inner.desktop_chat_catalog_lock.lock().await;
    let mut catalog = read_catalog(&server)?;
    let group = catalog
        .groups
        .iter_mut()
        .find(|group| group.id == group_id)
        .ok_or_else(|| not_found("Chat group not found"))?;
    if is_fixed_source_group(group) {
        return Err(bad_request("Source groups cannot be changed"));
    }
    if let Some(name) = name {
        group.name = name;
    }
    if let Some(pinned) = request.pinned {
        group.pinned = pinned;
    }
    let group = group.clone();
    write_catalog(&server, &catalog)?;
    Ok(Json(group))
}

pub(super) async fn reorder_groups(
    State(server): State<AppServer>,
    Json(request): Json<ReorderGroupsRequest>,
) -> Result<Json<Vec<ChatGroup>>, ApiError> {
    if request.group_ids.len() < 2 {
        return Err(unprocessable("group_ids must contain at least two IDs"));
    }
    for group_id in &request.group_ids {
        validate_identifier("group id", group_id)?;
    }
    let _guard = server.inner.desktop_chat_catalog_lock.lock().await;
    let mut catalog = read_catalog(&server)?;
    let current = catalog
        .groups
        .iter()
        .map(|group| (group.id.clone(), group.clone()))
        .collect::<BTreeMap<_, _>>();
    let unique = request.group_ids.iter().collect::<HashSet<_>>();
    if unique.len() != request.group_ids.len() {
        return Err(bad_request("Group order contains duplicate IDs"));
    }
    if request.group_ids.len() != current.len()
        || request
            .group_ids
            .iter()
            .any(|group_id| !current.contains_key(group_id))
    {
        return Err(bad_request("Group order must contain every group ID"));
    }
    let fixed = ordered_groups(catalog.groups.clone())
        .into_iter()
        .filter(is_fixed_source_group)
        .map(|group| group.id)
        .collect::<Vec<_>>();
    if !request.group_ids.ends_with(&fixed) {
        return Err(bad_request("Source groups must remain at the end"));
    }
    catalog.groups = request
        .group_ids
        .iter()
        .enumerate()
        .map(|(order, group_id)| {
            let mut group = current
                .get(group_id)
                .expect("validated group should exist")
                .clone();
            group.order = order;
            group
        })
        .collect();
    let groups = ordered_groups(catalog.groups.clone());
    write_catalog(&server, &catalog)?;
    Ok(Json(groups))
}

pub(super) async fn delete_group(
    State(server): State<AppServer>,
    Path(group_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_identifier("group id", &group_id)?;
    let _guard = server.inner.desktop_chat_catalog_lock.lock().await;
    let mut catalog = read_catalog(&server)?;
    let index = catalog
        .groups
        .iter()
        .position(|group| group.id == group_id)
        .ok_or_else(|| not_found("Chat group not found"))?;
    if catalog.groups[index].kind != "custom" {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({"detail": "Built-in chat groups cannot be deleted"})),
        ));
    }
    catalog.groups.remove(index);
    for metadata in catalog.chats.values_mut() {
        if metadata.group_id == group_id {
            metadata.group_id = default_group_for_source(&metadata.source).to_owned();
        }
    }
    let mut sorted_indices = (0..catalog.groups.len()).collect::<Vec<_>>();
    sorted_indices.sort_by_key(|index| catalog.groups[*index].order);
    for (order, index) in sorted_indices.into_iter().enumerate() {
        catalog.groups[index].order = order;
    }
    write_catalog(&server, &catalog)?;
    Ok(Json(json!({"success": true, "group_id": group_id})))
}

pub(super) async fn ensure_thread_metadata(
    server: &AppServer,
    thread: &Thread,
    session_id: &str,
) -> Result<(), ApiError> {
    let _guard = server.inner.desktop_chat_catalog_lock.lock().await;
    let mut catalog = read_catalog(server)?;
    if let Some(metadata) = catalog.chats.get_mut(&thread.id) {
        session_id.clone_into(&mut metadata.session_id);
        metadata.updated_at = metadata.updated_at.max(thread.updated_at);
    } else {
        catalog
            .chats
            .insert(thread.id.clone(), default_metadata(thread, session_id));
    }
    write_catalog(server, &catalog)
}

pub(super) async fn checkpoint_sessions(
    server: &AppServer,
    workspace_root: &str,
) -> Result<Vec<CheckpointSessionInfo>, ApiError> {
    let threads = list_all_threads(server).await;
    let aliases = server.inner.desktop_session_aliases.read().await;
    let _guard = server.inner.desktop_chat_catalog_lock.lock().await;
    let mut catalog = read_catalog(server)?;
    let mut changed = false;
    let mut sessions = Vec::new();
    for thread in threads {
        if thread.workspace_root.as_deref() != Some(workspace_root) {
            continue;
        }
        let session_id = aliases
            .thread_to_client
            .get(&thread.id)
            .map_or(thread.id.as_str(), String::as_str);
        if !catalog.chats.contains_key(&thread.id) {
            catalog
                .chats
                .insert(thread.id.clone(), default_metadata(&thread, session_id));
            changed = true;
        }
        let metadata = catalog
            .chats
            .get(&thread.id)
            .expect("checkpoint session metadata should exist");
        sessions.push(CheckpointSessionInfo {
            thread_id: thread.id,
            session_id: metadata.session_id.clone(),
            user_id: metadata.user_id.clone(),
            channel: metadata.channel.clone(),
            title: metadata.name.clone(),
            archived: thread.archived,
        });
    }
    if changed {
        write_catalog(server, &catalog)?;
    }
    sessions.sort_by(|left, right| {
        left.title
            .cmp(&right.title)
            .then_with(|| left.session_id.cmp(&right.session_id))
    });
    Ok(sessions)
}

async fn metadata_for_thread(
    server: &AppServer,
    thread: &Thread,
) -> Result<ChatMetadata, ApiError> {
    let aliases = server.inner.desktop_session_aliases.read().await;
    let session_id = aliases
        .thread_to_client
        .get(&thread.id)
        .map_or(thread.id.as_str(), String::as_str);
    let _guard = server.inner.desktop_chat_catalog_lock.lock().await;
    let mut catalog = read_catalog(server)?;
    let metadata = catalog
        .chats
        .entry(thread.id.clone())
        .or_insert_with(|| default_metadata(thread, session_id))
        .clone();
    write_catalog(server, &catalog)?;
    Ok(metadata)
}

async fn remove_chat_state(server: &AppServer, chat_ids: &[String]) -> Result<(), ApiError> {
    if chat_ids.is_empty() {
        return Ok(());
    }
    let chat_ids = chat_ids.iter().collect::<HashSet<_>>();
    {
        let mut aliases = server.inner.desktop_session_aliases.write().await;
        aliases
            .client_to_thread
            .retain(|_, thread_id| !chat_ids.contains(thread_id));
        aliases
            .thread_to_client
            .retain(|thread_id, _| !chat_ids.contains(thread_id));
    }
    let _guard = server.inner.desktop_chat_catalog_lock.lock().await;
    let mut catalog = read_catalog(server)?;
    catalog
        .chats
        .retain(|thread_id, _| !chat_ids.contains(thread_id));
    write_catalog(server, &catalog)
}

async fn list_all_threads(server: &AppServer) -> Vec<Thread> {
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
            return threads;
        };
        cursor = Some(next_cursor);
    }
}

async fn workspace_from_meta(
    server: &AppServer,
    meta: &Map<String, Value>,
) -> Result<String, ApiError> {
    let selected = server
        .inner
        .desktop_workspace
        .as_ref()
        .ok_or_else(|| internal("Desktop Workspace is unavailable"))?
        .selected
        .read()
        .await
        .clone();
    let requested = meta
        .get("runtime_context")
        .and_then(Value::as_object)
        .and_then(|context| {
            context
                .get("project_dirs")
                .and_then(Value::as_array)
                .and_then(|dirs| dirs.first())
                .and_then(Value::as_object)
                .and_then(|entry| entry.get("path"))
                .and_then(Value::as_str)
                .or_else(|| context.get("project_dir").and_then(Value::as_str))
        });
    let path = requested.map_or(selected, std::path::PathBuf::from);
    let canonical = path
        .canonicalize()
        .map_err(|_| bad_request("Project directory is unavailable"))?;
    if !canonical.is_dir() {
        return Err(bad_request("Project directory is unavailable"));
    }
    Ok(canonical.to_string_lossy().into_owned())
}

fn chat_spec(thread: &Thread, metadata: &ChatMetadata) -> Value {
    let mut meta = metadata.meta.clone();
    meta.insert("model".to_owned(), json!(thread.model));
    meta.insert("workspace_root".to_owned(), json!(thread.workspace_root));
    let updated_at = metadata.updated_at.max(thread.updated_at);
    json!({
        "id": thread.id,
        "session_id": metadata.session_id,
        "user_id": metadata.user_id,
        "channel": metadata.channel,
        "name": metadata.name,
        "created_at": timestamp(thread.created_at),
        "updated_at": timestamp(updated_at),
        "last_finished_at": metadata.last_finished_at.and_then(timestamp),
        "meta": meta,
        "status": if thread.status == ThreadStatus::Active { "running" } else { "idle" },
        "pinned": metadata.pinned,
        "archived_at": thread.archived.then(|| timestamp(thread.updated_at)).flatten(),
        "archived": thread.archived,
        "source": metadata.source,
        "group_id": metadata.group_id,
        "parent_session_id": metadata.parent_session_id,
        "root_session_id": metadata.root_session_id,
    })
}

fn default_metadata(thread: &Thread, session_id: &str) -> ChatMetadata {
    ChatMetadata {
        name: String::from("New Chat"),
        session_id: session_id.to_owned(),
        user_id: String::from("desktop"),
        channel: String::from("console"),
        meta: Map::new(),
        pinned: false,
        source: String::from("chat"),
        group_id: String::from("default"),
        parent_session_id: None,
        root_session_id: None,
        updated_at: thread.updated_at,
        last_finished_at: None,
    }
}

fn default_groups() -> Vec<ChatGroup> {
    vec![
        ChatGroup {
            id: String::from("default"),
            name: String::from("Uncategorized"),
            order: 0,
            kind: String::from("default"),
            source: Some(String::from("chat")),
            pinned: false,
        },
        ChatGroup {
            id: String::from("cron"),
            name: String::from("Scheduled tasks"),
            order: 1,
            kind: String::from("cron"),
            source: Some(String::from("cron")),
            pinned: false,
        },
        ChatGroup {
            id: String::from("subagents"),
            name: String::from("Subagents"),
            order: 2,
            kind: String::from("subagents"),
            source: Some(String::from("subagent")),
            pinned: false,
        },
    ]
}

fn ordered_groups(mut groups: Vec<ChatGroup>) -> Vec<ChatGroup> {
    groups.sort_by_key(|group| {
        if is_fixed_source_group(group) {
            (1_u8, false, fixed_source_order(group))
        } else {
            (0_u8, !group.pinned, group.order)
        }
    });
    groups
}

fn fixed_source_order(group: &ChatGroup) -> usize {
    match group.kind.as_str() {
        "cron" => 0,
        "subagents" => 1,
        _ => usize::MAX,
    }
}

fn is_fixed_source_group(group: &ChatGroup) -> bool {
    matches!(group.kind.as_str(), "cron" | "subagents")
}

fn default_group_for_source(source: &str) -> &'static str {
    match source {
        "cron" => "cron",
        "subagent" => "subagents",
        _ => "default",
    }
}

fn require_group(catalog: &ChatCatalog, group_id: &str) -> Result<(), ApiError> {
    if catalog.groups.iter().any(|group| group.id == group_id) {
        Ok(())
    } else {
        Err(bad_request(&format!("Unknown chat group: {group_id}")))
    }
}

fn read_catalog(server: &AppServer) -> Result<ChatCatalog, ApiError> {
    let Some(serialized) = server
        .inner
        .core
        .read_chat_catalog_data()
        .map_err(internal_error)?
    else {
        return Ok(ChatCatalog::default());
    };
    if serialized.len() > MAX_CATALOG_BYTES {
        return Err(internal("stored chat catalog exceeds its size limit"));
    }
    let catalog = serde_json::from_str::<ChatCatalog>(&serialized)
        .map_err(|_| internal("stored chat catalog is invalid"))?;
    validate_catalog(&catalog).map_err(internal)?;
    Ok(catalog)
}

fn write_catalog(server: &AppServer, catalog: &ChatCatalog) -> Result<(), ApiError> {
    validate_catalog(catalog).map_err(unprocessable)?;
    let serialized = serde_json::to_string(catalog)
        .map_err(|_| internal("chat catalog could not be serialized"))?;
    if serialized.len() > MAX_CATALOG_BYTES {
        return Err(unprocessable("chat catalog exceeds its size limit"));
    }
    server
        .inner
        .core
        .write_chat_catalog_data(&serialized)
        .map_err(internal_error)
}

fn validate_catalog(catalog: &ChatCatalog) -> Result<(), &'static str> {
    if catalog.version != 1 {
        return Err("chat catalog version is unsupported");
    }
    if catalog.chats.len() > MAX_CHATS || catalog.groups.len() > MAX_GROUPS {
        return Err("chat catalog exceeds its item limit");
    }
    let mut group_ids = HashSet::new();
    for group in &catalog.groups {
        validate_identifier_value(&group.id)?;
        validate_group_name_value(&group.name)?;
        if !group_ids.insert(group.id.as_str()) {
            return Err("chat catalog contains duplicate group IDs");
        }
        if !matches!(
            group.kind.as_str(),
            "default" | "cron" | "subagents" | "custom"
        ) {
            return Err("chat catalog contains an invalid group kind");
        }
    }
    for required in ["default", "cron", "subagents"] {
        if !group_ids.contains(required) {
            return Err("chat catalog is missing a built-in group");
        }
    }
    for (thread_id, metadata) in &catalog.chats {
        validate_identifier_value(thread_id)?;
        validate_identifier_value(&metadata.session_id)?;
        validate_identifier_value(&metadata.user_id)?;
        validate_identifier_value(&metadata.channel)?;
        validate_chat_name_value(&metadata.name)?;
        validate_source_value(&metadata.source)?;
        if !group_ids.contains(metadata.group_id.as_str()) {
            return Err("chat catalog references an unknown group");
        }
        if serde_json::to_vec(&metadata.meta).map_or(true, |value| value.len() > 1_048_576) {
            return Err("chat metadata exceeds its size limit");
        }
    }
    Ok(())
}

fn validate_batch(chat_ids: &[String]) -> Result<(), ApiError> {
    if chat_ids.len() > MAX_BATCH_SIZE {
        return Err(unprocessable("too many chat IDs"));
    }
    for chat_id in chat_ids {
        validate_identifier("chat id", chat_id)?;
    }
    Ok(())
}

fn validate_chat_name(name: &str) -> Result<(), ApiError> {
    validate_chat_name_value(name).map_err(unprocessable)
}

fn validate_chat_name_value(name: &str) -> Result<(), &'static str> {
    if name.len() > MAX_CHAT_NAME_BYTES || name.chars().any(char::is_control) {
        return Err("chat name is invalid");
    }
    Ok(())
}

fn normalize_group_name(name: &str) -> Result<String, ApiError> {
    let name = name.trim();
    validate_group_name_value(name).map_err(unprocessable)?;
    Ok(name.to_owned())
}

fn validate_group_name_value(name: &str) -> Result<(), &'static str> {
    if name.is_empty()
        || name.chars().count() > MAX_GROUP_NAME_CHARS
        || name.chars().any(char::is_control)
    {
        return Err("group name is invalid");
    }
    Ok(())
}

fn validate_source(source: &str) -> Result<(), ApiError> {
    validate_source_value(source).map_err(unprocessable)
}

fn validate_source_value(source: &str) -> Result<(), &'static str> {
    if matches!(source, "chat" | "cron" | "subagent") {
        Ok(())
    } else {
        Err("chat source is invalid")
    }
}

fn validate_optional_identifier(label: &str, value: Option<&str>) -> Result<(), ApiError> {
    value.map_or(Ok(()), |value| validate_identifier(label, value))
}

fn validate_identifier(label: &str, value: &str) -> Result<(), ApiError> {
    validate_identifier_value(value).map_err(|_| unprocessable(&format!("{label} is invalid")))
}

fn validate_identifier_value(value: &str) -> Result<(), &'static str> {
    if value.is_empty() || value.len() > MAX_ID_BYTES || value.chars().any(char::is_control) {
        Err("identifier is invalid")
    } else {
        Ok(())
    }
}

fn timestamp(seconds: i64) -> Option<String> {
    DateTime::from_timestamp(seconds, 0)
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Secs, true))
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
        })
}

fn default_channel() -> String {
    String::from("console")
}

fn default_chat_name() -> String {
    String::from("New Chat")
}

fn default_source() -> String {
    String::from("chat")
}

fn core_error(error: qwenpaw_core::CoreError) -> ApiError {
    let detail = error.to_string();
    let status = match &error {
        qwenpaw_core::CoreError::ThreadNotFound(_) => StatusCode::NOT_FOUND,
        qwenpaw_core::CoreError::ThreadBusy(_) | qwenpaw_core::CoreError::ThreadArchived(_) => {
            StatusCode::CONFLICT
        }
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    drop(error);
    (status, Json(json!({"detail": detail})))
}

fn bad_request(message: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, Json(json!({"detail": message})))
}

fn unprocessable(message: &str) -> ApiError {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(json!({"detail": message})),
    )
}

fn not_found(message: &str) -> ApiError {
    (StatusCode::NOT_FOUND, Json(json!({"detail": message})))
}

fn internal(message: &str) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"detail": message})),
    )
}

fn internal_error(error: qwenpaw_core::CoreError) -> ApiError {
    let message = error.to_string();
    drop(error);
    internal(&message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_pinned_custom_and_fixed_source_groups() {
        let mut groups = default_groups();
        groups.push(ChatGroup {
            id: String::from("regular"),
            name: String::from("Regular"),
            order: 4,
            kind: String::from("custom"),
            source: None,
            pinned: false,
        });
        groups.push(ChatGroup {
            id: String::from("pinned"),
            name: String::from("Pinned"),
            order: 5,
            kind: String::from("custom"),
            source: None,
            pinned: true,
        });

        assert_eq!(
            ordered_groups(groups)
                .into_iter()
                .map(|group| group.id)
                .collect::<Vec<_>>(),
            vec!["pinned", "default", "regular", "cron", "subagents"]
        );
    }

    #[test]
    fn rejects_catalogs_that_reference_unknown_groups() {
        let thread = Thread {
            id: String::from("thread"),
            model: String::from("model"),
            workspace_root: Some(String::from("/workspace")),
            status: ThreadStatus::Idle,
            archived: false,
            created_at: 1,
            updated_at: 1,
        };
        let mut catalog = ChatCatalog::default();
        let mut metadata = default_metadata(&thread, "session");
        metadata.group_id = String::from("missing");
        catalog.chats.insert(thread.id.clone(), metadata);

        assert_eq!(
            validate_catalog(&catalog),
            Err("chat catalog references an unknown group")
        );
    }
}
