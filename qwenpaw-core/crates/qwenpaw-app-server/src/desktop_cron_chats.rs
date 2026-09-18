//! Durable session registration for application-owned Cron turns.

use super::{
    ApiError, AppServer, MAX_CHATS, Thread, ThreadStartParams, alias_key, core_error,
    default_group_for_source, ensure_agent_groups, metadata_for_key, read_catalog, unprocessable,
    validate_chat_name, validate_identifier, write_catalog,
};

/// Read persisted chat tuples; internal session aliases are not user targets.
pub(crate) async fn cron_dispatch_targets(
    server: &AppServer,
    agent_id: &str,
) -> Result<Vec<(String, String, String)>, ApiError> {
    let key = super::data_key_for_agent(server, agent_id).await?;
    let _guard = server.inner.desktop_chat_catalog_lock.lock().await;
    Ok(read_catalog(server)?
        .chats
        .into_values()
        .filter(|chat| chat.data_key.as_ref() == Some(&key))
        .map(|chat| (chat.channel, chat.user_id, chat.session_id))
        .collect())
}

pub(crate) async fn resolve_cron_chat(
    server: &AppServer,
    agent_id: &str,
    session_id: &str,
    user_id: &str,
    name: &str,
) -> Result<Thread, ApiError> {
    validate_identifier("session_id", session_id)?;
    validate_identifier("user_id", user_id)?;
    validate_chat_name(name)?;
    let context = super::super::desktop_agents::context_for_agent(server, agent_id).await?;
    let key = context.data_key.clone();
    let workspace = context.project()?;
    let model = context.model();
    let guard = server.inner.desktop_chat_catalog_lock.lock().await;
    let mut catalog = read_catalog(server)?;
    let existing = catalog.chats.iter().find(|(_, metadata)| {
        metadata.data_key.as_ref() == Some(&key)
            && metadata.channel == "console"
            && metadata.user_id == user_id
            && metadata.session_id == session_id
    });
    let thread = if let Some((id, _)) = existing {
        // Do not rename, regroup, unarchive, or rebind a shared existing chat.
        server
            .inner
            .core
            .read_thread(id)
            .await
            .map_err(core_error)?
            .thread
    } else {
        if catalog.chats.len() >= MAX_CHATS {
            return Err(unprocessable("too many chats"));
        }
        ensure_agent_groups(&mut catalog, agent_id, &key);
        let thread = server
            .inner
            .core
            .start_thread(ThreadStartParams {
                model,
                workspace_root: Some(workspace.to_string_lossy().into_owned()),
            })
            .await
            .map_err(core_error)?
            .thread;
        let mut metadata = metadata_for_key(&thread, session_id, agent_id, &key);
        metadata.name = name.to_owned();
        metadata.user_id = user_id.to_owned();
        metadata.source = String::from("cron");
        default_group_for_source("cron").clone_into(&mut metadata.group_id);
        catalog.chats.insert(thread.id.clone(), metadata);
        if let Err(error) = write_catalog(server, &catalog) {
            let removed = server.inner.core.delete_thread(&thread.id).await;
            drop(guard);
            if removed.is_err() {
                return Err(super::internal(
                    "Cron chat creation and Thread rollback failed; recovery is required",
                ));
            }
            return Err(error);
        }
        thread
    };
    drop(guard);
    // Match the established alias lock ordering: never acquire it while holding
    // the catalog lock, since UI catalog readers acquire them in reverse order.
    let mut aliases = server.inner.desktop_session_aliases.write().await;
    aliases
        .client_to_thread
        .insert(alias_key(&key, session_id), thread.id.clone());
    if agent_id == "default" {
        aliases
            .client_to_thread
            .insert(session_id.to_owned(), thread.id.clone());
    }
    aliases
        .thread_to_client
        .insert(thread.id.clone(), session_id.to_owned());
    Ok(thread)
}
