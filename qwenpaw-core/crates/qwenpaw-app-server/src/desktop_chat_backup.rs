//! Explicit source and destination identities for scoped chat backups.

use super::{
    BTreeMap, BTreeSet, ChatCatalog, ChatMetadata, MAX_CATALOG_BYTES, Value, WorkspaceDataKey,
    default_agent_id, json, upgrade_catalog, validate_catalog,
};

type Bindings = BTreeMap<String, WorkspaceDataKey>;

pub(crate) struct RestoreBindings<'a> {
    pub current: &'a Bindings,
    pub source: &'a Bindings,
    pub target: &'a Bindings,
}

fn decode(serialized: Option<&str>, bindings: &Bindings) -> Result<ChatCatalog, &'static str> {
    let mut catalog = if let Some(serialized) = serialized {
        if serialized.len() > MAX_CATALOG_BYTES {
            return Err("Backup chat catalog exceeds its size limit");
        }
        serde_json::from_str(serialized).map_err(|_| "Backup chat catalog is invalid")?
    } else {
        ChatCatalog::default()
    };
    validate_catalog(&catalog)?;
    upgrade_catalog(
        &mut catalog,
        &bindings
            .get("default")
            .cloned()
            .unwrap_or_else(|| WorkspaceDataKey::LegacyAgent(default_agent_id())),
    );
    Ok(catalog)
}

fn encode(catalog: &ChatCatalog) -> Result<String, &'static str> {
    validate_catalog(catalog)?;
    let serialized =
        serde_json::to_string(catalog).map_err(|_| "Backup chat catalog is invalid")?;
    if serialized.len() > MAX_CATALOG_BYTES {
        return Err("Backup chat catalog exceeds its size limit");
    }
    Ok(serialized)
}

#[cfg(test)]
#[path = "desktop_chat_backup_tests.rs"]
mod tests;

fn selected_keys<'a>(
    ids: &BTreeSet<&str>,
    bindings: &'a Bindings,
) -> BTreeSet<&'a WorkspaceDataKey> {
    ids.iter().filter_map(|id| bindings.get(*id)).collect()
}

#[cfg(test)]
pub(crate) fn legacy_bindings(ids: &BTreeSet<&str>) -> Bindings {
    ids.iter()
        .chain(std::iter::once(&"default"))
        .map(|id| {
            (
                (*id).to_owned(),
                WorkspaceDataKey::LegacyAgent((*id).to_owned()),
            )
        })
        .collect()
}

pub(crate) fn filter_backup_threads(
    serialized: Option<&str>,
    ids: &BTreeSet<&str>,
    threads: &mut Vec<qwenpaw_storage::StoredThread>,
    bindings: &Bindings,
) -> Result<(), &'static str> {
    let catalog = decode(serialized, bindings)?;
    let keys = selected_keys(ids, bindings);
    threads.retain(|stored| {
        catalog.chats.get(&stored.thread.id).map_or_else(
            // App Protocol has no Agent selector. Project paths do not assign
            // an uncatalogued Thread to a non-default Agent.
            || ids.contains("default"),
            |chat| chat.data_key.as_ref().is_some_and(|key| keys.contains(key)),
        )
    });
    Ok(())
}

pub(crate) fn filter_backup_data(
    serialized: &str,
    ids: &BTreeSet<&str>,
    threads: &BTreeSet<&str>,
    bindings: &Bindings,
) -> Result<String, &'static str> {
    let mut catalog = decode(Some(serialized), bindings)?;
    let keys = selected_keys(ids, bindings);
    catalog.chats.retain(|id, chat| {
        threads.contains(id.as_str())
            && chat.data_key.as_ref().is_some_and(|key| keys.contains(key))
    });
    catalog.groups.retain(|group| {
        group
            .data_key
            .as_ref()
            .is_some_and(|key| keys.contains(key))
    });
    encode(&catalog)
}

pub(crate) fn backup_thread_owners(
    snapshot: &qwenpaw_storage::StoreBackup,
    bindings: &Bindings,
) -> Result<BTreeMap<String, BTreeSet<String>>, &'static str> {
    let catalog = decode(
        snapshot
            .settings
            .get("desktop_chat_catalog_data")
            .map(String::as_str),
        bindings,
    )?;
    let mut owners = BTreeMap::<String, BTreeSet<String>>::new();
    for stored in &snapshot.threads {
        let actor = catalog
            .chats
            .get(&stored.thread.id)
            .map_or(Some("default"), |chat| {
                bindings.iter().find_map(|(actor, key)| {
                    (chat.data_key.as_ref() == Some(key)).then_some(actor.as_str())
                })
            });
        if let Some(actor) = actor {
            owners
                .entry(actor.to_owned())
                .or_default()
                .insert(stored.thread.id.clone());
        }
    }
    Ok(owners)
}

pub(crate) fn remap_restore_paths(
    snapshot: &mut qwenpaw_storage::StoreBackup,
    workspaces: &[super::super::desktop_agents::restore::WorkspaceRestore],
    bindings: &Bindings,
) -> Result<(), &'static str> {
    use super::super::desktop_agents::restore::remap_workspace_path;

    let name = "desktop_chat_catalog_data";
    let mut catalog = decode(snapshot.settings.get(name).map(String::as_str), bindings)?;
    let owner = |chat: Option<&ChatMetadata>| {
        chat.map_or(Some("default"), |chat| {
            bindings.iter().find_map(|(actor, key)| {
                (chat.data_key.as_ref() == Some(key)).then_some(actor.as_str())
            })
        })
    };
    for stored in &mut snapshot.threads {
        if let Some(mapping) = workspaces.iter().find(|mapping| {
            Some(mapping.id.as_str()) == owner(catalog.chats.get(&stored.thread.id))
        }) && let Some(root) = &stored.thread.workspace_root
            && let Some(mapped) =
                remap_workspace_path(root, &mapping.source_root, &mapping.destination)
        {
            stored.thread.workspace_root = Some(mapped);
        }
    }
    let mut changed = false;
    for chat in catalog.chats.values_mut() {
        let Some(mapping) = workspaces
            .iter()
            .find(|mapping| Some(mapping.id.as_str()) == owner(Some(chat)))
        else {
            continue;
        };
        let Some(context) = chat
            .meta
            .get_mut("runtime_context")
            .and_then(Value::as_object_mut)
        else {
            continue;
        };
        let remap = |path: &mut Value| {
            if let Some(value) = path.as_str()
                && let Some(mapped) =
                    remap_workspace_path(value, &mapping.source_root, &mapping.destination)
            {
                *path = json!(mapped);
                return true;
            }
            false
        };
        if let Some(path) = context.get_mut("project_dir") {
            changed |= remap(path);
        }
        if let Some(projects) = context
            .get_mut("project_dirs")
            .and_then(Value::as_array_mut)
        {
            for project in projects {
                if let Some(path) = project.get_mut("path") {
                    changed |= remap(path);
                }
            }
        }
    }
    if changed {
        snapshot.settings.insert(name.to_owned(), encode(&catalog)?);
    }
    Ok(())
}

pub(crate) fn merge_restore_data(
    current: Option<&str>,
    archived: Option<&str>,
    ids: &BTreeSet<&str>,
    restored_threads: &BTreeSet<&str>,
    current_bindings: &Bindings,
    source_bindings: &Bindings,
    target_bindings: &Bindings,
) -> Result<String, &'static str> {
    if ids.is_empty()
        && let Some(current) = current
    {
        return Ok(current.to_owned());
    }
    let mut current = decode(current, current_bindings)?;
    let archived = decode(archived, source_bindings)?;
    let removed = selected_keys(ids, current_bindings);
    let mut mapped = BTreeMap::new();
    for id in ids {
        let source = source_bindings
            .get(*id)
            .ok_or("Backup chat Workspace binding is missing")?;
        let target = target_bindings
            .get(*id)
            .ok_or("Restored chat Workspace binding is missing")?;
        mapped.insert(source, target);
    }
    current.chats.retain(|_, chat| {
        !chat
            .data_key
            .as_ref()
            .is_some_and(|key| removed.contains(key))
    });
    current.groups.retain(|group| {
        !group
            .data_key
            .as_ref()
            .is_some_and(|key| removed.contains(key))
    });
    for (id, mut chat) in archived.chats {
        let Some(target) = chat.data_key.as_ref().and_then(|key| mapped.get(key)) else {
            continue;
        };
        if !restored_threads.contains(id.as_str()) {
            continue;
        }
        chat.data_key = Some((*target).clone());
        if current.chats.insert(id, chat).is_some() {
            return Err("Restored chat ID conflicts with an unselected Agent");
        }
    }
    for mut group in archived.groups {
        let Some(target) = group.data_key.as_ref().and_then(|key| mapped.get(key)) else {
            continue;
        };
        group.data_key = Some((*target).clone());
        current.groups.push(group);
    }
    encode(&current)
}
