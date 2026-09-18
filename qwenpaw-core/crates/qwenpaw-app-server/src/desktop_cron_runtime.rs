//! Persistent scheduling, independent of incoming HTTP traffic.

use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;

use super::ApiError;
use super::AppServer;
use super::CronData;
use super::execute_text;
use super::find_job;
use super::find_job_index;
use super::format_datetime;
use super::internal;
use super::read_data;
use super::record_result;
use super::reset_schedule;
use super::schedule;
use super::validate_and_normalize;
use super::write_data;
use chrono::DateTime;
use chrono::Utc;

pub(crate) fn spawn_scheduler(server: &AppServer) -> JoinHandle<()> {
    let weak = Arc::downgrade(&server.inner);
    let shutdown = server.inner.shutdown.clone();
    tokio::spawn(async move {
        loop {
            if shutdown.is_cancelled() {
                return;
            }
            let Some(inner) = weak.upgrade() else { return };
            if let Err((_, body)) = tick(&AppServer { inner }, Utc::now()).await {
                tracing::warn!(detail = ?body.0, "Cron scheduler could not advance persisted jobs");
            }
            tokio::select! {
                () = shutdown.cancelled() => return,
                () = tokio::time::sleep(Duration::from_millis(250)) => {}
            }
        }
    })
}

pub(super) async fn tick(server: &AppServer, now: DateTime<Utc>) -> Result<(), ApiError> {
    if server.inner.shutdown.is_cancelled() || super::super::desktop_backups::is_restoring(server) {
        return Ok(());
    }
    // Never overlap restore. The guard spans claim, delivery and final state.
    let Ok(operation) = server.inner.core.operation_guard() else {
        return Ok(());
    };
    let _guard = server.inner.desktop_cron_lock.lock().await;
    let mut data = read_data(server)?;
    let mut changed = recover_interrupted(server, &mut data, now).await?;
    for mut job in data.jobs.clone() {
        if server.inner.shutdown.is_cancelled() {
            break;
        }
        let Some(id) = job.id.clone() else {
            return Err(internal("stored cron job has no id"));
        };
        if !job.enabled
            || super::super::desktop_checkpoints::quiescence::is_paused(
                server,
                &super::owner_key(&data, &id),
            )
        {
            continue;
        }
        let owner = super::owner_key(&data, &id);
        if server.inner.desktop_workspace.is_some() {
            if super::super::desktop_agents::context_for_data_key(server, &owner)
                .await
                .is_err()
            {
                continue;
            }
        } else if owner != super::super::desktop_agents::default_data_key(server)? {
            // Lightweight embedded hosts have no non-default registration.
            continue;
        }
        let next = next_slot(&data, &id)?;
        if data.scheduled.contains(&id) && next.is_none_or(|value| value > now) {
            continue;
        }
        if validate_and_normalize(&mut job).is_err() {
            let index = find_job_index(&data, &id)?;
            data.jobs[index].enabled = false;
            data.states.entry(id.clone()).or_default().next_run_at = None;
            record_result(
                &mut data,
                &id,
                "scheduled",
                "error",
                Some(String::from(
                    "Stored cron schedule or task is invalid; job disabled",
                )),
                now,
            );
            changed = true;
            continue;
        }
        let compiled = schedule::Schedule::parse(&job.schedule)?;
        if !data.scheduled.contains(&id) {
            reset_schedule(&mut data, &job, now)?;
            changed = true;
        }
        let first = next_slot(&data, &id)?;
        let Some(first) = first.filter(|value| *value <= now) else {
            continue;
        };
        let slot = compiled
            .latest_due(first, now)
            .ok_or_else(|| internal("stored cron cursor does not match its schedule"))?;
        data.states.entry(id.clone()).or_default().next_run_at =
            compiled.next(now, false).map(format_datetime);
        let late = now.signed_duration_since(slot);
        if late > chrono::Duration::seconds(i64::from(job.runtime.misfire_grace_seconds)) {
            record_result(
                &mut data,
                &id,
                "scheduled",
                "skipped",
                Some(format!(
                    "missed scheduled run at {}: late by {}s, grace={}s",
                    format_datetime(slot),
                    late.num_seconds(),
                    job.runtime.misfire_grace_seconds
                )),
                now,
            );
        } else {
            dispatch_due(server, &mut data, job, operation.clone(), now).await?;
        }
        changed = true;
    }
    if changed {
        write_data(server, &data)?;
    }
    Ok(())
}

async fn dispatch_due(
    server: &AppServer,
    data: &mut CronData,
    job: super::CronJobSpec,
    operation: qwenpaw_core::CoreOperationGuard,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    let id = job
        .id
        .clone()
        .ok_or_else(|| internal("stored cron job has no id"))?;
    let result = if job.dispatch.channel != "console" {
        Err(super::error(
            axum::http::StatusCode::NOT_IMPLEMENTED,
            "Rust Core cron delivery currently supports the Console channel only",
        ))
    } else if job.task_type == "agent" {
        super::agent::enqueue(server, data, job, "scheduled", operation).await
    } else {
        // Persist the advanced cursor and claim before producing side effects.
        execute_text(server, data, &job, "scheduled").await?;
        return Ok(());
    };
    match result {
        Ok(true) => {}
        Ok(false) => record_result(
            data,
            &id,
            "scheduled",
            "skipped",
            Some(String::from("Cron maximum concurrent runs reached")),
            now,
        ),
        Err((_, body)) => record_result(
            data,
            &id,
            "scheduled",
            "error",
            Some(
                body.0["detail"]
                    .as_str()
                    .unwrap_or("Cron dispatch failed")
                    .to_owned(),
            ),
            now,
        ),
    }
    Ok(())
}

async fn recover_interrupted(
    server: &AppServer,
    data: &mut CronData,
    now: DateTime<Utc>,
) -> Result<bool, ApiError> {
    let mut changed = !data.active_triggers.is_empty();
    for (id, trigger) in std::mem::take(&mut data.active_triggers) {
        if find_job(data, &id).is_ok() {
            record_result(
                data,
                &id,
                &trigger,
                "cancelled",
                Some(String::from(
                    "Core stopped during cron execution; delivery was not replayed",
                )),
                now,
            );
        }
    }
    let live = super::agent::active_ids(server);
    for (run_id, claim) in data.active_runs.clone() {
        if live.contains(&run_id) {
            continue;
        }
        super::super::desktop_inbox::interrupt_cron_trace(
            server,
            &run_id,
            claim
                .agent_id
                .as_deref()
                .unwrap_or_else(|| super::owner(data, &claim.job_id)),
        )
        .await?;
        if find_job(data, &claim.job_id).is_ok() {
            record_result(
                data,
                &claim.job_id,
                &claim.trigger,
                "cancelled",
                Some(String::from(
                    "Core stopped during cron execution; delivery was not replayed",
                )),
                now,
            );
        }
        data.active_runs.remove(&run_id);
        changed = true;
    }
    Ok(changed)
}

fn next_slot(data: &CronData, id: &str) -> Result<Option<DateTime<Utc>>, ApiError> {
    data.states
        .get(id)
        .and_then(|state| state.next_run_at.as_deref())
        .map(|value| schedule::datetime(value, chrono_tz::UTC))
        .transpose()
}

#[cfg(test)]
#[path = "desktop_cron_runtime_tests.rs"]
mod tests;
