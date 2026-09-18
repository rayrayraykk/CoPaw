//! Workspace ownership shared by Console chat readers and writers.

use super::{
    ApiError, AppServer, ChatCatalog, WorkspaceDataKey, alias_key, default_agent_id, not_found,
    read_catalog,
};

pub(super) fn record_key(key: Option<&WorkspaceDataKey>, actor: &str) -> WorkspaceDataKey {
    key.cloned()
        .unwrap_or_else(|| WorkspaceDataKey::LegacyAgent(actor.to_owned()))
}

pub(super) fn upgrade_catalog(catalog: &mut ChatCatalog, default: &WorkspaceDataKey) {
    if catalog.version != 1 {
        return;
    }
    let key = |actor: &str| {
        if actor == "default" {
            default.clone()
        } else {
            WorkspaceDataKey::LegacyAgent(actor.to_owned())
        }
    };
    for chat in catalog.chats.values_mut() {
        chat.data_key = Some(key(&chat.agent_id));
    }
    for group in &mut catalog.groups {
        group.data_key = Some(key(&group.agent_id));
    }
    catalog.version = 2;
}

pub(super) async fn data_key_for_agent(
    server: &AppServer,
    actor: &str,
) -> Result<WorkspaceDataKey, ApiError> {
    if server.inner.desktop_workspace.is_none() && actor == "default" {
        return Ok(WorkspaceDataKey::LegacyAgent(default_agent_id()));
    }
    Ok(
        super::super::desktop_agents::context_for_agent(server, actor)
            .await?
            .data_key,
    )
}

/// A cache never authorizes ownership, and project paths never assign it.
pub(crate) async fn resolve_existing_thread(
    server: &AppServer,
    actor: &str,
    requested: &str,
) -> Result<Option<String>, ApiError> {
    let key = data_key_for_agent(server, actor).await?;
    resolve_bound_thread(server, actor, &key, requested).await
}

/// The caller retains the validated registration and lifecycle admission lock.
pub(crate) async fn resolve_bound_thread(
    server: &AppServer,
    actor: &str,
    key: &WorkspaceDataKey,
    requested: &str,
) -> Result<Option<String>, ApiError> {
    let cached = server
        .inner
        .desktop_session_aliases
        .read()
        .await
        .client_to_thread
        .get(&alias_key(key, requested))
        .cloned();
    let catalog = {
        let _guard = server.inner.desktop_chat_catalog_lock.lock().await;
        read_catalog(server)?
    };
    let owned = |id: &str| {
        catalog.chats.get(id).map_or(actor == "default", |chat| {
            chat.data_key.as_ref() == Some(key)
        })
    };
    // A direct internal ID cannot be reinterpreted as somebody else's alias.
    if server.inner.core.read_thread(requested).await.is_ok() {
        return if owned(requested) {
            Ok(Some(requested.to_owned()))
        } else {
            Err(not_found(&format!("thread not found: {requested}")))
        };
    }
    // Match the original ChatManager: the most recently updated match wins.
    let latest = catalog
        .chats
        .iter()
        .filter(|(_, chat)| {
            chat.data_key.as_ref() == Some(key)
                && chat.channel == "console"
                && chat.session_id == requested
        })
        .max_by_key(|(_, chat)| chat.updated_at);
    if let Some((id, _)) = latest {
        return Ok(Some(id.clone()));
    }
    if let Some(id) = cached
        && owned(&id)
        && server.inner.core.read_thread(&id).await.is_ok()
    {
        return Ok(Some(id));
    }
    Ok(None)
}
