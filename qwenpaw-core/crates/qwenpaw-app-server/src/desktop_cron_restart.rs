//! Re-register one Agent's schedules without discarding its execution history.

use super::change::CronChange;
use super::{ApiError, AppServer, CronJobState, DateTime, StatusCode, Utc};
use super::{
    WorkspaceDataKey, backup, error, internal, owner_key, reset_schedule, unprocessable,
    upgrade_owners, validate_schedule,
};

// Caller holds Lifecycle, Cron and Agent locks through publication or rollback.
pub(crate) fn prepare_restart(
    server: &AppServer,
    agent: &WorkspaceDataKey,
    now: DateTime<Utc>,
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
        .active_triggers
        .keys()
        .any(|id| &owner_key(&data, id) == agent)
        || data
            .active_runs
            .values()
            .any(|claim| &owner_key(&data, &claim.job_id) == agent)
    {
        return Err(error(
            StatusCode::CONFLICT,
            "Agent Cron runs have not finished stopping",
        ));
    }
    let original = backup::encode(&data).map_err(unprocessable)?;
    let selected = data
        .jobs
        .iter()
        .enumerate()
        .filter(|(_, job)| &owner_key(&data, job.id.as_deref().unwrap_or_default()) == agent)
        .map(|(index, job)| (index, job.clone()))
        .collect::<Vec<_>>();
    for (index, mut job) in selected {
        let id = job.id.as_deref().unwrap_or_default();
        data.states.insert(id.to_owned(), CronJobState::default());
        if validate_schedule(&mut job.schedule)
            .and_then(|()| reset_schedule(&mut data, &job, now))
            .is_err()
        {
            // Original manager startup disables an invalid trigger, not a run.
            data.jobs[index].enabled = false;
            data.scheduled.insert(id.to_owned());
        }
    }
    let after = backup::encode(&data).map_err(unprocessable)?;
    Ok((after != original).then(|| CronChange::new(before, after)))
}
