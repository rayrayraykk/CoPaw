//! Explicit archive-to-local Workspace mappings; Agent labels are not owners.

#[cfg(test)]
#[path = "desktop_cron_workspace_backup_tests.rs"]
mod tests;

use std::collections::{BTreeMap, BTreeSet};

use super::super::{WorkspaceDataKey, owner, owner_key};
use super::{CronData, encode, identity, parse_data};

pub(crate) type Bindings = BTreeMap<String, WorkspaceDataKey>;

fn effective_key(data: &CronData, id: &str, bindings: &Bindings) -> WorkspaceDataKey {
    if data.version < 4
        && owner(data, id) == "default"
        && let Some(key) = bindings.get("default")
    {
        return key.clone();
    }
    owner_key(data, id)
}

fn materialize(data: &mut CronData, bindings: &Bindings) {
    data.workspace_owners = data
        .jobs
        .iter()
        .filter_map(|job| {
            let id = job.id.as_ref()?;
            Some((id.clone(), effective_key(data, id, bindings)))
        })
        .collect();
    if data.version < 4 {
        for claim in data.active_runs.values_mut() {
            claim.agent_id = Some(
                data.owners
                    .get(&claim.job_id)
                    .map_or("default", String::as_str)
                    .to_owned(),
            );
            claim.data_key = data.workspace_owners.get(&claim.job_id).cloned();
        }
    }
    data.version = 4;
}

fn retain(data: &mut CronData, keys: &BTreeSet<WorkspaceDataKey>, include: bool) {
    let retained = data
        .jobs
        .iter()
        .filter_map(|job| job.id.as_ref())
        .filter(|id| keys.contains(&owner_key(data, id)) == include)
        .cloned()
        .collect::<BTreeSet<_>>();
    data.jobs
        .retain(|job| job.id.as_ref().is_some_and(|id| retained.contains(id)));
    data.workspace_owners.retain(|id, _| retained.contains(id));
    data.owners.retain(|id, _| retained.contains(id));
    data.public_ids.retain(|id, _| retained.contains(id));
    data.states.retain(|id, _| retained.contains(id));
    data.history.retain(|id, _| retained.contains(id));
    data.scheduled.retain(|id| retained.contains(id));
    data.active_triggers.retain(|id, _| retained.contains(id));
    data.active_runs
        .retain(|_, claim| retained.contains(&claim.job_id));
}

pub(crate) fn filter_for_bindings(
    serialized: &str,
    selected: &Bindings,
) -> Result<Option<String>, &'static str> {
    let mut data = parse_data(serialized)?;
    materialize(&mut data, selected);
    retain(&mut data, &selected.values().cloned().collect(), true);
    if data.jobs.is_empty() && !selected.contains_key("default") {
        return Ok(None);
    }
    encode(&data).map(Some)
}

pub(crate) fn merge_for_bindings(
    current: Option<&str>,
    archived: Option<&str>,
    selected: &BTreeSet<&str>,
    current_bindings: &Bindings,
    source_bindings: &Bindings,
    target_bindings: &Bindings,
    protected_run_ids: &BTreeSet<String>,
) -> Result<Option<String>, &'static str> {
    if selected.is_empty() {
        return Ok(current.map(str::to_owned));
    }
    let mut local = current.map(parse_data).transpose()?.unwrap_or_default();
    let mut incoming = archived.map(parse_data).transpose()?.unwrap_or_default();
    let mapping = selected
        .iter()
        .map(|id| {
            let source = source_bindings
                .get(*id)
                .ok_or("Archived Cron Workspace binding is missing")?;
            let target = target_bindings
                .get(*id)
                .ok_or("Restored Cron Workspace binding is missing")?;
            Ok((source.clone(), target.clone()))
        })
        .collect::<Result<BTreeMap<_, _>, &'static str>>()?;
    materialize(&mut local, current_bindings);
    materialize(&mut incoming, source_bindings);
    let previous_len = local.jobs.len();
    let local_keys = selected
        .iter()
        .filter_map(|id| current_bindings.get(*id).cloned())
        .collect();
    retain(&mut local, &local_keys, false);
    retain(&mut incoming, &mapping.keys().cloned().collect(), true);
    if previous_len == local.jobs.len() && incoming.jobs.is_empty() && !selected.contains("default")
    {
        return Ok(current.map(str::to_owned));
    }
    for key in incoming.workspace_owners.values_mut() {
        *key = mapping
            .get(key)
            .ok_or("Restored Cron Workspace mapping is incomplete")?
            .clone();
    }
    for claim in incoming.active_runs.values_mut() {
        claim.data_key = incoming.workspace_owners.get(&claim.job_id).cloned();
    }
    if incoming
        .active_runs
        .keys()
        .any(|id| protected_run_ids.contains(id))
    {
        return Err("Restored Cron run ID conflicts with an unselected Inbox record");
    }
    if incoming
        .active_runs
        .keys()
        .any(|id| local.active_runs.contains_key(id))
    {
        return Err("Restored Cron run ID conflicts with an unselected Agent");
    }
    identity::rekey_collisions(&local, &mut incoming);
    local.jobs.extend(incoming.jobs);
    local.workspace_owners.extend(incoming.workspace_owners);
    local.owners.extend(incoming.owners);
    local.public_ids.extend(incoming.public_ids);
    local.states.extend(incoming.states);
    local.history.extend(incoming.history);
    local.scheduled.extend(incoming.scheduled);
    local.active_triggers.extend(incoming.active_triggers);
    local.active_runs.extend(incoming.active_runs);
    if local.jobs.is_empty() && archived.is_none() {
        return Ok(None);
    }
    encode(&local).map(Some)
}
