//! Usage scope follows immutable Workspace keys, not reusable Agent labels.

use std::collections::{BTreeMap, BTreeSet};

use qwenpaw_storage::{StoreBackup, StoredUsageRecord, WorkspaceDataKey};

use super::desktop_chats::RestoreBindings;

type Bindings = BTreeMap<String, WorkspaceDataKey>;

#[cfg(test)]
#[path = "desktop_usage_backup_tests.rs"]
mod tests;

fn record_key(record: &StoredUsageRecord, bindings: &Bindings) -> WorkspaceDataKey {
    record.data_key.clone().unwrap_or_else(|| {
        if record.agent_id == "default" {
            bindings
                .get("default")
                .cloned()
                .unwrap_or_else(|| WorkspaceDataKey::LegacyAgent(String::from("default")))
        } else {
            WorkspaceDataKey::LegacyAgent(record.agent_id.clone())
        }
    })
}

pub(super) fn filter_backup(
    snapshot: &mut StoreBackup,
    selected: &BTreeSet<&str>,
    bindings: &Bindings,
) -> Result<(), &'static str> {
    snapshot
        .validate_usage()
        .map_err(|_| "Invalid Core usage ownership")?;
    let keys = selected
        .iter()
        .filter_map(|id| bindings.get(*id))
        .collect::<BTreeSet<_>>();
    snapshot
        .usage
        .retain(|record| keys.contains(&record_key(record, bindings)));
    Ok(())
}

pub(super) fn merge(
    current: &StoreBackup,
    source: &StoreBackup,
    selected: &BTreeSet<&str>,
    bindings: &RestoreBindings<'_>,
) -> Result<Vec<StoredUsageRecord>, &'static str> {
    current
        .validate_usage()
        .map_err(|_| "Invalid Core usage ownership")?;
    source
        .validate_usage()
        .map_err(|_| "Invalid Core usage ownership")?;
    let removed = selected
        .iter()
        .filter_map(|id| bindings.current.get(*id))
        .collect::<BTreeSet<_>>();
    let mapped = selected
        .iter()
        .filter_map(|id| bindings.source.get(*id).zip(bindings.target.get(*id)))
        .collect::<BTreeMap<_, _>>();
    let mut result = current
        .usage
        .iter()
        .filter(|record| !removed.contains(&record_key(record, bindings.current)))
        .cloned()
        .collect::<Vec<_>>();
    let mut ids = result
        .iter()
        .map(|record| record.id.clone())
        .collect::<BTreeSet<_>>();
    for record in &source.usage {
        let key = record_key(record, bindings.source);
        let Some(target) = mapped.get(&key) else {
            continue;
        };
        if !target.is_valid() {
            return Err("Invalid Core usage ownership");
        }
        if !ids.insert(record.id.clone()) {
            return Err("Restored usage ID conflicts with existing data");
        }
        let mut restored = record.clone();
        restored.data_key = Some((*target).clone());
        result.push(restored);
    }
    Ok(result)
}
