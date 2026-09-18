//! Scope-aware logical state merging before any file or live Core changes.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use qwenpaw_storage::StoreBackup;

use super::super::desktop_chats;
use super::super::desktop_inbox;
use super::super::desktop_mail_access_control;

const CHAT_DATA: &str = "desktop_chat_catalog_data";
const INBOX_DATA: &str = "desktop_inbox_data";
const MAIL_DATA: &str = "desktop_mail_access_control_data";
const CHANNEL_DATA: &str = "desktop_channel_config_data";
const CRON_DATA: &str = "desktop_cron_data";
const HEARTBEAT_DATA: &str = "desktop_heartbeat_data";
const AGENT_KEYS: [&str; 6] = [
    CHAT_DATA,
    INBOX_DATA,
    MAIL_DATA,
    CHANNEL_DATA,
    CRON_DATA,
    HEARTBEAT_DATA,
];
const PROTECTED_KEYS: [(&str, &str); 2] = [
    ("desktop_security_data", "security"),
    ("desktop_mcp_data", "mcp"),
];

#[derive(Debug)]
pub(super) struct RestoredState {
    pub(super) snapshot: StoreBackup,
    pub(super) preserved_local_keys: Vec<String>,
}

pub(super) fn merge_for_agents(
    current_runtime: &qwenpaw_core::Core,
    current: &StoreBackup,
    archived: &StoreBackup,
    plan: &super::super::desktop_agents::restore::AgentRestorePlan,
    include_globals: bool,
    preserve_local_protected: bool,
) -> Result<RestoredState, &'static str> {
    // The caller captures this runtime and logical snapshot under one restore
    // lease. Defaults/bootstrap settings are real local protection too.
    let mut current = current.clone();
    if include_globals && preserve_local_protected {
        current.settings.insert(
            String::from("desktop_mcp_data"),
            super::super::desktop_mcp::backup_data(current_runtime)?,
        );
        current.settings.insert(
            String::from("desktop_security_data"),
            current_runtime
                .backup_security_data()
                .map_err(|_| "Local security configuration could not be read")?,
        );
    }
    let mut archived = archived.clone();
    desktop_chats::remap_restore_paths(&mut archived, &plan.workspaces, &plan.archived_bindings)?;
    let selected = plan
        .workspaces
        .iter()
        .map(|workspace| workspace.id.as_str())
        .collect();
    let current_cron = current.settings.remove(CRON_DATA);
    let archived_cron = archived.settings.remove(CRON_DATA);
    let protected_runs = desktop_inbox::retained_backup_run_ids(
        current.settings.get(INBOX_DATA).map(String::as_str),
        &selected,
    )?;
    let targets = plan
        .agents
        .agents
        .iter()
        .filter_map(|agent| agent.data_key.clone().map(|key| (agent.id.clone(), key)))
        .collect();
    let mut result = merge_bound(
        &current,
        &archived,
        &selected,
        include_globals,
        preserve_local_protected,
        &desktop_chats::RestoreBindings {
            current: &plan.current_bindings,
            source: &plan.archived_bindings,
            target: &targets,
        },
    )?;
    if let Some(cron) = super::super::desktop_cron::merge_for_bindings(
        current_cron.as_deref(),
        archived_cron.as_deref(),
        &selected,
        &plan.current_bindings,
        &plan.archived_bindings,
        &targets,
        &protected_runs,
    )? {
        result.snapshot.settings.insert(CRON_DATA.to_owned(), cron);
    }
    if serde_json::to_vec(&result.snapshot)
        .map_err(|_| "Restored Core data is invalid")?
        .len() as u64
        > super::MAX_FILE_BYTES
    {
        return Err("Restored Core data exceeds its size limit");
    }
    Ok(result)
}

/// The caller intersects requested IDs with actual archived Workspace payloads.
/// Full/custom registry replacement does not grant access to unselected data.
#[cfg(test)]
pub(super) fn merge(
    current: &StoreBackup,
    archived: &StoreBackup,
    selected_ids: &BTreeSet<&str>,
    include_globals: bool,
    preserve_local_protected: bool,
) -> Result<RestoredState, &'static str> {
    let bindings = desktop_chats::legacy_bindings(selected_ids);
    merge_bound(
        current,
        archived,
        selected_ids,
        include_globals,
        preserve_local_protected,
        &desktop_chats::RestoreBindings {
            current: &bindings,
            source: &bindings,
            target: &bindings,
        },
    )
}

fn merge_bound(
    current: &StoreBackup,
    archived: &StoreBackup,
    selected_ids: &BTreeSet<&str>,
    include_globals: bool,
    preserve_local_protected: bool,
    chat_bindings: &desktop_chats::RestoreBindings<'_>,
) -> Result<RestoredState, &'static str> {
    if !matches!(current.version, 1 | 2) || !matches!(archived.version, 1 | 2) {
        return Err("Unsupported Core backup version");
    }
    let mut result = current.clone();
    let mut incoming = archived.threads.clone();
    desktop_chats::filter_backup_threads(
        archived.settings.get(CHAT_DATA).map(String::as_str),
        selected_ids,
        &mut incoming,
        chat_bindings.source,
    )?;
    let mut replaced = current.threads.clone();
    desktop_chats::filter_backup_threads(
        current.settings.get(CHAT_DATA).map(String::as_str),
        selected_ids,
        &mut replaced,
        chat_bindings.current,
    )?;
    let replaced_ids = replaced
        .iter()
        .map(|stored| stored.thread.id.as_str())
        .collect::<BTreeSet<_>>();
    result
        .threads
        .retain(|stored| !replaced_ids.contains(stored.thread.id.as_str()));
    let mut thread_ids = result
        .threads
        .iter()
        .map(|stored| stored.thread.id.as_str())
        .collect::<BTreeSet<_>>();
    for stored in &incoming {
        if !thread_ids.insert(stored.thread.id.as_str()) {
            return Err("Restored Thread ID conflicts with existing data");
        }
    }
    let incoming_ids = incoming
        .iter()
        .map(|stored| stored.thread.id.as_str())
        .collect::<BTreeSet<_>>();
    let (settings, preserved_local_keys) = merge_settings(
        &current.settings,
        &archived.settings,
        selected_ids,
        &incoming_ids,
        include_globals,
        preserve_local_protected,
        chat_bindings,
    )?;
    result.settings = settings;
    result.threads.extend(incoming);
    result.usage =
        super::super::desktop_usage::merge(current, archived, selected_ids, chat_bindings)?;
    if result.usage.iter().any(|record| record.data_key.is_some()) {
        result.version = 2;
    }
    let bytes = serde_json::to_vec(&result).map_err(|_| "Restored Core data is invalid")?;
    if bytes.len() as u64 > super::MAX_FILE_BYTES {
        return Err("Restored Core data exceeds its size limit");
    }
    Ok(RestoredState {
        snapshot: result,
        preserved_local_keys,
    })
}

fn merge_settings(
    current: &BTreeMap<String, String>,
    archived: &BTreeMap<String, String>,
    selected_ids: &BTreeSet<&str>,
    incoming_ids: &BTreeSet<&str>,
    include_globals: bool,
    preserve_local_protected: bool,
    chat_bindings: &desktop_chats::RestoreBindings<'_>,
) -> Result<(BTreeMap<String, String>, Vec<String>), &'static str> {
    let mut settings = if include_globals {
        archived.clone()
    } else {
        current.clone()
    };
    let mut preserved = Vec::new();
    if include_globals && preserve_local_protected {
        for (key, label) in PROTECTED_KEYS {
            if let Some(value) = current.get(key) {
                settings.insert(key.to_owned(), value.clone());
                preserved.push(label.to_owned());
            }
        }
    }
    // Global replacement never imports archived Agent data incidentally.
    for key in AGENT_KEYS {
        settings.remove(key);
        if let Some(value) = current.get(key) {
            settings.insert(key.to_owned(), value.clone());
        }
    }
    if selected_ids.is_empty() {
        return Ok((settings, preserved));
    }
    for key in [CHAT_DATA, INBOX_DATA, MAIL_DATA, CHANNEL_DATA] {
        let local = current.get(key).map(String::as_str);
        let backup = archived.get(key).map(String::as_str);
        if local.is_none() && backup.is_none() {
            continue;
        }
        let merged = match key {
            CHAT_DATA => desktop_chats::merge_restore_data(
                local,
                backup,
                selected_ids,
                incoming_ids,
                chat_bindings.current,
                chat_bindings.source,
                chat_bindings.target,
            )?,
            INBOX_DATA => desktop_inbox::merge_restore_data(local, backup, selected_ids)?,
            CHANNEL_DATA => super::super::desktop_channels::merge_restore_data(
                local,
                backup,
                selected_ids,
                chat_bindings,
            )?,
            _ => desktop_mail_access_control::merge_restore_data(
                local,
                backup,
                selected_ids,
                chat_bindings,
            )?,
        };
        settings.insert(key.to_owned(), merged);
    }
    let cron = super::super::desktop_cron::merge_restore_data(
        current.get(CRON_DATA).map(String::as_str),
        archived.get(CRON_DATA).map(String::as_str),
        selected_ids,
    )?;
    settings.remove(CRON_DATA);
    if let Some(value) = cron {
        settings.insert(CRON_DATA.to_owned(), value);
    }
    if selected_ids.contains("default") {
        settings.remove(HEARTBEAT_DATA);
        if let Some(value) = archived.get(HEARTBEAT_DATA) {
            settings.insert(HEARTBEAT_DATA.to_owned(), value.clone());
        }
    }
    Ok((settings, preserved))
}

#[cfg(test)]
#[path = "desktop_backup_restore_state_tests.rs"]
mod tests;
