//! Job-owned logical backup selection; no file writes or live runtime changes.

use std::collections::BTreeSet;

use super::{CronData, MAX_CRON_DATA_BYTES, MAX_CRON_JOBS, owner};

#[path = "desktop_cron_identity.rs"]
mod identity;

#[path = "desktop_cron_workspace_backup.rs"]
mod workspace;
pub(crate) use workspace::{filter_for_bindings, merge_for_bindings};

pub(super) fn parse_data(serialized: &str) -> Result<CronData, &'static str> {
    if serialized.len() > MAX_CRON_DATA_BYTES {
        return Err("Stored Cron data exceeds its size limit");
    }
    let mut data: CronData =
        serde_json::from_str(serialized).map_err(|_| "Stored Cron data is invalid")?;
    if !matches!(data.version, 1..=4)
        || (data.version == 1 && !data.owners.is_empty())
        || (data.version < 3 && !data.public_ids.is_empty())
        || (data.version < 4 && !data.workspace_owners.is_empty())
        || (data.version == 4 && data.workspace_owners.len() != data.jobs.len())
        || data.jobs.len() > MAX_CRON_JOBS
    {
        return Err("Stored Cron data has an unsupported shape");
    }
    let mut ids = BTreeSet::new();
    for job in &data.jobs {
        let id = job.id.as_deref().ok_or("Stored Cron job has no ID")?;
        if id.is_empty() || id.len() > 1024 || id.chars().any(char::is_control) || !ids.insert(id) {
            return Err("Stored Cron job IDs are invalid or duplicated");
        }
    }
    for (id, agent) in &data.owners {
        if !ids.contains(id.as_str())
            || super::super::desktop_agents::validate_agent_id(agent, true).is_err()
        {
            return Err("Stored Cron ownership is invalid");
        }
    }
    for (id, key) in &data.workspace_owners {
        if !ids.contains(id.as_str()) || !key.is_valid() {
            return Err("Stored Cron Workspace ownership is invalid");
        }
    }
    if data.version == 4 {
        for claim in data.active_runs.values() {
            if claim.data_key.is_none()
                || claim.data_key.as_ref() != data.workspace_owners.get(&claim.job_id)
                || claim.agent_id.as_deref().is_none_or(|id| {
                    super::super::desktop_agents::validate_agent_id(id, true).is_err()
                })
            {
                return Err("Stored Cron run Workspace ownership is invalid");
            }
        }
    }
    identity::validate_and_hydrate(&mut data)?;
    Ok(data)
}

fn retain(data: &mut CronData, selected: &BTreeSet<&str>, include: bool) {
    let ids = data
        .jobs
        .iter()
        .filter_map(|job| job.id.as_deref())
        .filter(|id| selected.contains(owner(data, id)) == include)
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    data.jobs
        .retain(|job| job.id.as_ref().is_some_and(|id| ids.contains(id)));
    data.owners.retain(|id, _| ids.contains(id));
    data.workspace_owners.retain(|id, _| ids.contains(id));
    data.public_ids.retain(|id, _| ids.contains(id));
    data.states.retain(|id, _| ids.contains(id));
    data.history.retain(|id, _| ids.contains(id));
    data.scheduled.retain(|id| ids.contains(id));
    data.active_triggers.retain(|id, _| ids.contains(id));
    data.active_runs
        .retain(|_, claim| ids.contains(&claim.job_id));
}

pub(super) fn encode(data: &CronData) -> Result<String, &'static str> {
    let mut value = serde_json::to_value(data).map_err(|_| "Cron data could not be encoded")?;
    if !data.workspace_owners.is_empty() || data.version == 4 {
        value["version"] = serde_json::json!(4);
    } else if !data.public_ids.is_empty() || data.version == 3 {
        value["version"] = serde_json::json!(3);
    } else if !data.owners.is_empty() {
        // Version 1 readers ignore unknown fields and would execute all jobs as
        // default. A scoped store must be rejected by those older runtimes.
        value["version"] = serde_json::json!(2);
    }
    let serialized = serde_json::to_string(&value).map_err(|_| "Cron data could not be encoded")?;
    if serialized.len() > MAX_CRON_DATA_BYTES || data.jobs.len() > MAX_CRON_JOBS {
        return Err("Restored Cron data exceeds its size limit");
    }
    parse_data(&serialized)?;
    Ok(serialized)
}

#[cfg(test)]
fn filter_backup_data(
    serialized: &str,
    selected: &BTreeSet<&str>,
) -> Result<Option<String>, &'static str> {
    let mut data = parse_data(serialized)?;
    if data.version == 4 {
        return Err("Cron Workspace bindings are required for backup");
    }
    let before = encode(&data)?;
    retain(&mut data, selected, true);
    if data.jobs.is_empty() && !selected.contains("default") {
        return Ok(None);
    }
    let after = encode(&data)?;
    Ok(Some(if before == after {
        serialized.to_owned()
    } else {
        after
    }))
}

pub(crate) fn merge_restore_data(
    current: Option<&str>,
    archived: Option<&str>,
    selected: &BTreeSet<&str>,
) -> Result<Option<String>, &'static str> {
    if selected.is_empty() {
        return Ok(current.map(str::to_owned));
    }
    let mut local = current.map(parse_data).transpose()?.unwrap_or_default();
    let mut incoming = archived.map(parse_data).transpose()?.unwrap_or_default();
    if local.version == 4 || incoming.version == 4 {
        return Err("Cron Workspace bindings are required for restore");
    }
    let before = encode(&local)?;
    retain(&mut local, selected, false);
    retain(&mut incoming, selected, true);
    // Preserve the exact unselected serialization, including an absent value.
    if !selected.contains("default") && incoming.jobs.is_empty() && encode(&local)? == before {
        return Ok(current.map(str::to_owned));
    }
    if incoming
        .active_runs
        .keys()
        .any(|id| local.active_runs.contains_key(id))
    {
        return Err("Restored Cron run ID conflicts with an unselected Agent");
    }
    identity::rekey_collisions(&local, &mut incoming);
    local.version = local.version.max(incoming.version);
    local.jobs.extend(incoming.jobs);
    local.owners.extend(incoming.owners);
    local.workspace_owners.extend(incoming.workspace_owners);
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

#[cfg(test)]
#[path = "desktop_cron_backup_tests.rs"]
mod tests;
