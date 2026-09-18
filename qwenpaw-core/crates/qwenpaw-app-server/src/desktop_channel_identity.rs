//! Native channel configuration belongs to a Workspace, never a reusable ID.

use std::collections::{BTreeMap, BTreeSet};

use qwenpaw_storage::WorkspaceDataKey;
use serde::Deserialize;

use super::{ChannelWorkspace, ConsoleChannelConfig, StoredChannelData, validate_console};
use crate::desktop_chats::RestoreBindings;

type Bindings = BTreeMap<String, WorkspaceDataKey>;
const MAX_DATA_BYTES: usize = 2_097_152;
const MAX_WORKSPACES: usize = 128;
const INVALID: &str = "Stored channel configuration is invalid";

#[cfg(test)]
#[path = "desktop_channel_identity_tests.rs"]
mod tests;

impl StoredChannelData {
    pub(super) fn console(&self, key: &WorkspaceDataKey) -> Option<ConsoleChannelConfig> {
        self.workspaces
            .iter()
            .find(|entry| &entry.data_key == key)
            .map_or_else(
                || Some(ConsoleChannelConfig::default()),
                |entry| entry.console.clone(),
            )
    }

    pub(super) fn set_console(
        &mut self,
        key: WorkspaceDataKey,
        config: Option<ConsoleChannelConfig>,
    ) {
        if let Some(entry) = self
            .workspaces
            .iter_mut()
            .find(|entry| entry.data_key == key)
        {
            entry.console = config;
        } else {
            self.workspaces.push(ChannelWorkspace {
                data_key: key,
                console: config,
            });
        }
    }
}

pub(super) fn decode(
    serialized: Option<&str>,
    default_key: Option<&WorkspaceDataKey>,
) -> Result<StoredChannelData, &'static str> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Legacy {
        version: u32,
        console: ConsoleChannelConfig,
    }
    let Some(serialized) = serialized else {
        return Ok(StoredChannelData::default());
    };
    if serialized.len() > MAX_DATA_BYTES {
        return Err("Stored channel configuration is too large");
    }
    let value: serde_json::Value = serde_json::from_str(serialized).map_err(|_| INVALID)?;
    let data = if value["version"] == 1 {
        if !value
            .get("console")
            .is_some_and(serde_json::Value::is_object)
        {
            return Err(INVALID);
        }
        let old: Legacy = serde_json::from_value(value).map_err(|_| INVALID)?;
        debug_assert_eq!(old.version, 1);
        StoredChannelData {
            version: 2,
            workspaces: vec![ChannelWorkspace {
                data_key: default_key
                    .cloned()
                    .unwrap_or_else(|| WorkspaceDataKey::LegacyAgent(String::from("default"))),
                console: Some(old.console),
            }],
        }
    } else {
        let entries = value
            .get("workspaces")
            .and_then(serde_json::Value::as_array)
            .ok_or(INVALID)?;
        if entries.iter().any(|entry| {
            !entry
                .get("console")
                .is_some_and(|config| config.is_object() || config.is_null())
        }) {
            return Err(INVALID);
        }
        serde_json::from_value(value).map_err(|_| INVALID)?
    };
    validate(&data)?;
    Ok(data)
}

pub(super) fn encode(data: &StoredChannelData) -> Result<String, &'static str> {
    validate(data)?;
    let mut data = data.clone();
    data.workspaces.sort_by(|a, b| a.data_key.cmp(&b.data_key));
    let serialized = serde_json::to_string(&data).map_err(|_| INVALID)?;
    if serialized.len() > MAX_DATA_BYTES {
        return Err("Stored channel configuration is too large");
    }
    Ok(serialized)
}

fn validate(data: &StoredChannelData) -> Result<(), &'static str> {
    if data.version != 2 || data.workspaces.len() > MAX_WORKSPACES {
        return Err(INVALID);
    }
    let mut keys = BTreeSet::new();
    for entry in &data.workspaces {
        if !entry.data_key.is_valid() || !keys.insert(&entry.data_key) {
            return Err(INVALID);
        }
        if let Some(console) = &entry.console {
            validate_console(console).map_err(|_| INVALID)?;
        }
    }
    Ok(())
}

pub(crate) fn filter_backup_data(
    serialized: &str,
    selected: &BTreeSet<&str>,
    bindings: &Bindings,
) -> Result<String, &'static str> {
    let mut data = decode(Some(serialized), bindings.get("default"))?;
    let keys = selected
        .iter()
        .filter_map(|id| bindings.get(*id))
        .collect::<BTreeSet<_>>();
    data.workspaces
        .retain(|entry| keys.contains(&entry.data_key));
    encode(&data)
}

pub(crate) fn merge_restore_data(
    current: Option<&str>,
    archived: Option<&str>,
    selected: &BTreeSet<&str>,
    bindings: &RestoreBindings<'_>,
) -> Result<String, &'static str> {
    let mut current = decode(current, bindings.current.get("default"))?;
    let incoming = decode(archived, bindings.source.get("default"))?;
    let removed = selected
        .iter()
        .filter_map(|id| bindings.current.get(*id))
        .collect::<BTreeSet<_>>();
    let mapped = selected
        .iter()
        .filter_map(|id| bindings.source.get(*id).zip(bindings.target.get(*id)))
        .collect::<BTreeMap<_, _>>();
    current
        .workspaces
        .retain(|entry| !removed.contains(&entry.data_key));
    for mut entry in incoming.workspaces {
        let Some(target) = mapped.get(&entry.data_key) else {
            continue;
        };
        if current
            .workspaces
            .iter()
            .any(|entry| &entry.data_key == *target)
        {
            return Err("Restored channel Workspace conflicts with unselected data");
        }
        entry.data_key = (*target).clone();
        current.workspaces.push(entry);
    }
    encode(&current)
}
