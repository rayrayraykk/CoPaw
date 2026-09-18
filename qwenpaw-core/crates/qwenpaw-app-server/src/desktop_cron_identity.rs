//! Public Agent/job identities are independent from unique runtime storage keys.

use super::CronData;
use std::collections::BTreeSet;
use uuid::Uuid;

pub(super) fn validate_and_hydrate(data: &mut CronData) -> Result<(), &'static str> {
    let keys = data
        .jobs
        .iter()
        .filter_map(|job| job.id.as_deref())
        .collect::<BTreeSet<_>>();
    for (key, id) in &data.public_ids {
        if !keys.contains(key.as_str())
            || id.is_empty()
            || id.len() > 1024
            || id.chars().any(char::is_control)
        {
            return Err("Stored Cron public identity is invalid");
        }
    }
    let mut identities = BTreeSet::new();
    for job in &mut data.jobs {
        let key = job.id.as_ref().ok_or("Stored Cron job has no ID")?;
        job.public_id = data.public_ids.get(key).cloned();
        let agent = data.workspace_owners.get(key).cloned().unwrap_or_else(|| {
            super::super::WorkspaceDataKey::LegacyAgent(
                data.owners
                    .get(key)
                    .map_or("default", String::as_str)
                    .to_owned(),
            )
        });
        if !identities.insert((agent, job.public_id().unwrap_or_default().to_owned())) {
            return Err("Stored Cron public ID is duplicated within an Agent");
        }
    }
    Ok(())
}

pub(super) fn rekey_collisions(local: &CronData, incoming: &mut CronData) {
    let mut occupied = local
        .jobs
        .iter()
        .chain(&incoming.jobs)
        .filter_map(|job| job.id.clone())
        .collect::<BTreeSet<_>>();
    let retained = local
        .jobs
        .iter()
        .filter_map(|job| job.id.as_deref())
        .collect::<BTreeSet<_>>();
    let collisions = incoming
        .jobs
        .iter()
        .filter_map(|job| job.id.as_ref())
        .filter(|key| retained.contains(key.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    for old in collisions {
        let key = loop {
            let candidate = Uuid::now_v7().to_string();
            if occupied.insert(candidate.clone()) {
                break candidate;
            }
        };
        rekey(incoming, &old, &key);
    }
}

fn rekey(data: &mut CronData, old: &str, key: &str) {
    let public_id = data
        .public_ids
        .remove(old)
        .unwrap_or_else(|| old.to_owned());
    for job in &mut data.jobs {
        if job.id.as_deref() == Some(old) {
            job.id = Some(key.to_owned());
            job.public_id = Some(public_id.clone());
        }
    }
    data.public_ids.insert(key.to_owned(), public_id);
    if let Some(owner) = data.owners.remove(old) {
        data.owners.insert(key.to_owned(), owner);
    }
    if let Some(owner) = data.workspace_owners.remove(old) {
        data.workspace_owners.insert(key.to_owned(), owner);
    }
    if let Some(state) = data.states.remove(old) {
        data.states.insert(key.to_owned(), state);
    }
    if let Some(history) = data.history.remove(old) {
        data.history.insert(key.to_owned(), history);
    }
    if let Some(trigger) = data.active_triggers.remove(old) {
        data.active_triggers.insert(key.to_owned(), trigger);
    }
    if data.scheduled.remove(old) {
        data.scheduled.insert(key.to_owned());
    }
    for claim in data.active_runs.values_mut() {
        if claim.job_id == old {
            key.clone_into(&mut claim.job_id);
        }
    }
    data.version = data.version.max(3);
}
