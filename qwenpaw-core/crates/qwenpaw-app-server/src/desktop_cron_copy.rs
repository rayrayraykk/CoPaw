//! Copy specifications only; runtime identities and execution state are private.

use super::change::CronChange;
use super::{
    ApiError, AppServer, BTreeSet, Uuid, WorkspaceDataKey, backup, internal, owner_key,
    unprocessable, upgrade_owners,
};

// Caller holds the Cron lock until catalog publication or rollback completes.
pub(crate) fn prepare_copy(
    server: &AppServer,
    source: &WorkspaceDataKey,
    target: &str,
    target_key: &WorkspaceDataKey,
) -> Result<Option<CronChange>, ApiError> {
    let Some(before) = server.inner.core.read_cron_data().map_err(internal)? else {
        return Ok(None);
    };
    let mut data = backup::parse_data(&before).map_err(unprocessable)?;
    upgrade_owners(
        &mut data,
        &super::super::desktop_agents::default_data_key(server)?,
    );
    if data
        .jobs
        .iter()
        .any(|job| &owner_key(&data, job.id.as_deref().unwrap_or_default()) == target_key)
    {
        return Err(unprocessable("Agent copy target already owns Cron jobs"));
    }
    let jobs = data
        .jobs
        .iter()
        .filter(|job| &owner_key(&data, job.id.as_deref().unwrap_or_default()) == source)
        .cloned()
        .collect::<Vec<_>>();
    if jobs.is_empty() {
        return Ok(None);
    }
    let mut occupied = data
        .jobs
        .iter()
        .filter_map(|job| job.id.clone())
        .collect::<BTreeSet<_>>();
    for mut job in jobs {
        let key = loop {
            let candidate = Uuid::now_v7().to_string();
            if occupied.insert(candidate.clone()) {
                break candidate;
            }
        };
        let public_id = job.public_id().unwrap_or_default().to_owned();
        job.id = Some(key.clone());
        job.public_id = Some(public_id.clone());
        data.owners.insert(key.clone(), target.to_owned());
        data.workspace_owners
            .insert(key.clone(), target_key.clone());
        data.public_ids.insert(key, public_id);
        data.jobs.push(job);
    }
    let after = backup::encode(&data).map_err(unprocessable)?;
    Ok(Some(CronChange::new(before, after)))
}
