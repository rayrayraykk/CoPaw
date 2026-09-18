//! Asynchronous native Agent jobs, with durable claims and turn-local policy.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use futures_util::FutureExt as _;
use qwenpaw_core::CoreOperationGuard;
use qwenpaw_core::ToolApprovalLevel;
use qwenpaw_core::TurnEventStream;
use qwenpaw_protocol::CoreEvent;
use qwenpaw_protocol::Turn;
use qwenpaw_protocol::TurnInterruptParams;
use qwenpaw_protocol::TurnStartParams;
use qwenpaw_protocol::TurnStatus;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use super::super::desktop_inbox::{NewInboxEvent, NewInboxTrace};
use super::{
    AgentRunClaim, ApiError, AppServer, CronData, CronJobSpec, Json, StatusCode, Utc, Uuid, Value,
    error, find_job, internal, json, read_data, record_result, unprocessable, write_data,
};

const MAX_ACTIVE_RUNS: usize = 256;

#[derive(Default)]
pub(crate) struct RunState {
    live: Mutex<BTreeMap<String, LiveRun>>,
    tasks: TaskTracker,
}

struct LiveRun {
    job_id: String,
    agent_id: String,
    data_key: super::WorkspaceDataKey,
    gate: Arc<Semaphore>,
    cancellation: CancellationToken,
    completed: CancellationToken,
}

struct LiveLease {
    server: AppServer,
    agent: super::super::desktop_agents::AgentContext,
    run_id: String,
    agent_id: String,
    data_key: super::WorkspaceDataKey,
    completed: CancellationToken,
}

impl Drop for LiveLease {
    fn drop(&mut self) {
        self.server
            .inner
            .desktop_cron_runs
            .live
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.run_id);
        self.completed.cancel();
    }
}

pub(super) fn active_ids(server: &AppServer) -> BTreeSet<String> {
    server
        .inner
        .desktop_cron_runs
        .live
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .keys()
        .cloned()
        .collect()
}

pub(super) fn cancel_job(server: &AppServer, job_id: &str) {
    for run in server
        .inner
        .desktop_cron_runs
        .live
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .values()
    {
        if run.job_id == job_id {
            run.cancellation.cancel();
        }
    }
}

/// Capture a cancellation fence while the caller holds the Cron lock.
pub(crate) fn cancel_agent(server: &AppServer, agent_id: &str) -> Vec<CancellationToken> {
    server
        .inner
        .desktop_cron_runs
        .live
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .values()
        .filter(|run| run.agent_id == agent_id)
        .map(|run| {
            run.cancellation.cancel();
            run.completed.clone()
        })
        .collect()
}

/// Wait outside Cron/Agent locks; finish needs those locks to publish state.
pub(crate) async fn drain_runs(completed: Vec<CancellationToken>) -> Result<(), ApiError> {
    tokio::time::timeout(Duration::from_secs(10), async {
        for completion in completed {
            completion.cancelled().await;
        }
    })
    .await
    .map_err(|_| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Cron Agent runs did not finish stopping",
        )
    })
}

pub(crate) async fn shutdown(server: &AppServer) {
    let state = &server.inner.desktop_cron_runs;
    state.tasks.close();
    state.tasks.wait().await;
}

/// Preserve the immediate manual-run acknowledgement while restore owns admission.
pub(super) fn defer_manual(
    server: &AppServer,
    id: String,
    owner: super::WorkspaceDataKey,
    spec: CronJobSpec,
    operation: CoreOperationGuard,
) {
    let source = server.clone();
    server.inner.desktop_cron_runs.tasks.spawn(async move {
        let admitted = tokio::select! {
            biased;
            () = source.inner.shutdown.cancelled() => return,
            () = restoring(&source) => return,
            admitted = super::manual_job_admission(&source, &id, &owner) => admitted,
        };
        let result = match admitted {
            Ok((_guard, mut data, key))
                if super::owner_key(&data, &key) == owner
                    && spec.id.as_deref() == Some(key.as_str()) =>
            {
                super::execute_manual(&source, &mut data, spec, operation).await
            }
            Ok(_) => Err(error(
                StatusCode::CONFLICT,
                "Queued Cron job identity changed",
            )),
            Err(error) => Err(error),
        };
        if let Err((_, body)) = result {
            tracing::warn!(detail = ?body.0, "Deferred manual Cron did not start");
        }
    });
}

pub(crate) fn workspace_completions(
    server: &AppServer,
    key: &super::WorkspaceDataKey,
) -> Vec<CancellationToken> {
    server
        .inner
        .desktop_cron_runs
        .live
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .values()
        .filter(|run| &run.data_key == key)
        .map(|run| run.completed.clone())
        .collect()
}

/// Caller retains the Cron lock through persistence and live registration.
pub(super) async fn enqueue(
    server: &AppServer,
    data: &mut CronData,
    job: CronJobSpec,
    trigger: &str,
    operation: CoreOperationGuard,
) -> Result<bool, ApiError> {
    if server.inner.shutdown.is_cancelled() || super::super::desktop_backups::is_restoring(server) {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Cron execution is stopping",
        ));
    }
    let id = job
        .id
        .as_deref()
        .ok_or_else(|| internal("cron job id is missing"))?;
    let data_key = super::owner_key(data, id);
    super::super::desktop_checkpoints::quiescence::ensure_available(server, &data_key)?;
    let context = super::super::desktop_agents::context_for_data_key(server, &data_key).await?;
    let agent_id = context.agent_id.clone();
    let state = &server.inner.desktop_cron_runs;
    let mut live = state
        .live
        .lock()
        .map_err(|_| internal("Cron run registry is unavailable"))?;
    let matching = live
        .values()
        .filter(|run| run.job_id == id)
        .collect::<Vec<_>>();
    if trigger == "scheduled" && matching.len() >= job.runtime.max_concurrency as usize {
        return Ok(false);
    }
    if live.len() >= MAX_ACTIVE_RUNS {
        return Err(error(
            StatusCode::TOO_MANY_REQUESTS,
            "Cron active and queued run limit reached",
        ));
    }
    let gate = matching.first().map_or_else(
        || {
            Arc::new(Semaphore::new(
                (job.runtime.max_concurrency as usize).min(MAX_ACTIVE_RUNS),
            ))
        },
        |run| Arc::clone(&run.gate),
    );
    let run_id = Uuid::now_v7().to_string();
    let cancellation = server.inner.shutdown.child_token();
    let completed = CancellationToken::new();
    let claim = AgentRunClaim {
        job_id: id.to_owned(),
        trigger: trigger.to_owned(),
        agent_id: Some(agent_id.clone()),
        data_key: Some(data_key.clone()),
    };
    data.active_runs.insert(run_id.clone(), claim.clone());
    if let Err(error) = write_data(server, data) {
        data.active_runs.remove(&run_id);
        return Err(error);
    }
    live.insert(
        run_id.clone(),
        LiveRun {
            job_id: id.to_owned(),
            agent_id: agent_id.clone(),
            data_key: data_key.clone(),
            gate: Arc::clone(&gate),
            cancellation: cancellation.clone(),
            completed: completed.clone(),
        },
    );
    drop(live);
    let lease = LiveLease {
        server: server.clone(),
        agent: context,
        run_id,
        agent_id,
        data_key,
        completed,
    };
    state.tasks.spawn(async move {
        let _operation = operation;
        // Retain the permit through trace, Inbox and final state publication.
        let permit = tokio::select! {
            biased;
            () = cancellation.cancelled() => None,
            () = restoring(&lease.server) => None,
            permit = gate.acquire_owned() => permit.ok(),
        };
        let outcome = if permit.is_some() {
            AssertUnwindSafe(run(&lease, &job, &cancellation))
                .catch_unwind()
                .await
                .unwrap_or_else(|_| Outcome::error("Cron executor panicked"))
        } else {
            Outcome::cancelled()
        };
        if let Err((_, body)) = finish(&lease, &job, &claim, outcome).await {
            tracing::warn!(detail = ?body.0, "Cron could not persist execution result");
        }
        // Drop removes the live entry even if an unexpected failure unwinds.
    });
    Ok(true)
}

struct Outcome {
    status: &'static str,
    error: Option<String>,
    turn: Option<Turn>,
}

impl Outcome {
    fn error(message: impl Into<String>) -> Self {
        Self {
            status: "error",
            error: Some(message.into()),
            turn: None,
        }
    }

    fn cancelled() -> Self {
        Self {
            status: "cancelled",
            error: Some(String::from("Job was cancelled")),
            turn: None,
        }
    }
}

async fn restoring(server: &AppServer) {
    loop {
        if super::super::desktop_backups::is_restoring(server) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn run(lease: &LiveLease, job: &CronJobSpec, cancel: &CancellationToken) -> Outcome {
    let server = &lease.server;
    {
        let _guard = server.inner.desktop_cron_lock.lock().await;
        let mut data = match read_data(server) {
            Ok(data) => data,
            Err(error) => return Outcome::error(detail(error)),
        };
        if !data.active_runs.contains_key(&lease.run_id)
            || super::owner_key(&data, job.id.as_deref().unwrap_or_default()) != lease.data_key
        {
            return Outcome::cancelled();
        }
        data.states
            .entry(job.id.clone().unwrap_or_default())
            .or_default()
            .last_status = Some(String::from("running"));
        if let Err(error) = write_data(server, &data) {
            return Outcome::error(detail(error));
        }
    }
    let pending = Outcome {
        status: "running",
        error: None,
        turn: None,
    };
    if let Err(error) =
        super::super::desktop_inbox::write_cron_trace(server, trace(lease, job, &pending)).await
    {
        return Outcome::error(detail(error));
    }
    let deadline =
        tokio::time::Instant::now() + Duration::from_secs(u64::from(job.runtime.timeout_seconds));
    let prepared = tokio::select! {
        biased;
        () = cancel.cancelled() => return Outcome::cancelled(),
        () = restoring(server) => return Outcome::cancelled(),
        result = tokio::time::timeout_at(deadline, prepare(lease, job)) => match result {
            Ok(Ok(prepared)) => prepared,
            Ok(Err(error)) => return Outcome::error(detail(error)),
            Err(_) => return Outcome::error("Cron Agent execution timed out"),
        }
    };
    let outcome = consume(server, prepared, deadline, cancel).await;
    let query = job
        .request
        .as_ref()
        .and_then(|request| request.get("input"))
        .and_then(Value::as_array)
        .and_then(|input| super::super::desktop_checkpoints::console_query(input));
    if outcome.status == "success"
        && let Some(turn) = &outcome.turn
        && super::super::desktop_checkpoints::auto_snapshot_eligible(turn, query.as_deref())
    {
        super::super::desktop_checkpoints::schedule_auto_checkpoint(
            server,
            &turn.thread_id,
            &turn.id,
            &lease.agent,
            query,
            cancel,
        )
        .await;
    }
    outcome
}

async fn prepare(
    lease: &LiveLease,
    job: &CronJobSpec,
) -> Result<(Turn, TurnEventStream), ApiError> {
    let server = &lease.server;
    let agent_id = &lease.agent_id;
    let context = super::super::desktop_agents::context_for_agent(server, agent_id).await?;
    if context.data_key != lease.data_key {
        return Err(error(
            StatusCode::CONFLICT,
            "Cron Workspace binding has changed",
        ));
    }
    let usage_owner = context.usage_owner();
    let config = context.config;
    let running = config
        .get("running")
        .cloned()
        .unwrap_or_else(super::super::desktop_agent_settings::default_running_config);
    let mut runtime = super::super::desktop_agent_settings::runtime_config(&running)?;
    runtime.approval_level = if job.runtime.tool_safety {
        ToolApprovalLevel::Auto
    } else {
        ToolApprovalLevel::Off
    };
    let model = super::super::desktop_models::runtime_for_agent_config(server, &config).await?;
    let session = session_id(job);
    let user = if job.dispatch.target.user_id.is_empty() {
        "cron"
    } else {
        &job.dispatch.target.user_id
    };
    let thread =
        super::super::desktop_chats::resolve_cron_chat(server, agent_id, &session, user, &job.name)
            .await?;
    let input = job
        .request
        .as_ref()
        .and_then(|request| request.get("input"))
        .and_then(Value::as_array)
        .ok_or_else(|| unprocessable("Cron Agent request.input must be a message array"))?;
    let workspace = thread
        .workspace_root
        .as_deref()
        .ok_or_else(|| internal("Cron chat has no Workspace"))?;
    let input = super::super::desktop_files::console_user_input(
        server,
        input,
        std::path::Path::new(workspace),
    )
    .await?;
    let params = TurnStartParams {
        thread_id: thread.id,
        input,
    };
    loop {
        match server
            .inner
            .core
            .start_turn_with_owner(
                params.clone(),
                model.clone(),
                runtime.clone(),
                Some(usage_owner.clone()),
            )
            .await
        {
            Ok((started, events)) => return Ok((started.turn, events)),
            // Shared sessions serialize turns, including concurrent manual runs.
            // The enclosing preparation deadline/cancellation bounds this wait.
            Err(qwenpaw_core::CoreError::ThreadBusy(_)) => {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(error) => return Err(internal(error)),
        }
    }
}

fn session_id(job: &CronJobSpec) -> String {
    let target = &job.dispatch.target.session_id;
    let id = job.public_id().unwrap_or_default();
    if target.is_empty() {
        format!("cron:{id}")
    } else if job.runtime.share_session {
        target.clone()
    } else {
        format!("{target}:cron:{id}")
    }
}

async fn consume(
    server: &AppServer,
    (started, mut events): (Turn, TurnEventStream),
    deadline: tokio::time::Instant,
    cancel: &CancellationToken,
) -> Outcome {
    let interrupted = tokio::select! {
        biased;
        () = cancel.cancelled() => Outcome::cancelled(),
        () = restoring(server) => Outcome::cancelled(),
        result = tokio::time::timeout_at(deadline, drain(server, &mut events)) => match result {
            Ok(Ok(turn)) => {
                super::super::desktop_api::clear_turn_approvals(server, &started.id).await;
                return Outcome {
                    status: match turn.status { TurnStatus::Completed => "success", TurnStatus::Interrupted => "cancelled", _ => "error" },
                    error: turn.error.as_ref().map(|error| error.message.clone()).or_else(|| (turn.status == TurnStatus::Interrupted).then(|| String::from("Job was cancelled"))), turn: Some(turn),
                };
            }
            Ok(Err(error)) => Outcome::error(error),
            Err(_) => Outcome::error("Cron Agent execution timed out"),
        }
    };
    let _ = server
        .inner
        .core
        .interrupt_turn(&TurnInterruptParams {
            thread_id: started.thread_id,
            turn_id: started.id.clone(),
        })
        .await;
    let turn = tokio::time::timeout(Duration::from_secs(5), drain(server, &mut events)).await;
    super::super::desktop_api::clear_turn_approvals(server, &started.id).await;
    match turn {
        Ok(Ok(turn)) => Outcome {
            turn: Some(turn),
            ..interrupted
        },
        _ => Outcome::error("Cron could not observe the interrupted Turn finishing"),
    }
}

async fn drain(server: &AppServer, events: &mut TurnEventStream) -> Result<Turn, String> {
    while let Some(event) = events.recv().await {
        super::super::desktop_api::track_pending_approval(server, &event).await;
        if let CoreEvent::TurnCompleted(event) = event {
            return Ok(event.turn);
        }
    }
    Err(String::from("Cron Agent event stream ended early"))
}

fn trace(lease: &LiveLease, job: &CronJobSpec, outcome: &Outcome) -> NewInboxTrace {
    NewInboxTrace {
        run_id: lease.run_id.clone(),
        status: outcome.status.to_owned(),
        error: outcome.error.clone(),
        meta: json!({"source":"cron", "agent_id":lease.agent_id, "job_id":job.public_id(), "job_name":job.name,
            "task_type":"agent", "dispatch_channel":job.dispatch.channel, "target_user_id":job.dispatch.target.user_id,
            "target_session_id":job.dispatch.target.session_id, "session_id":session_id(job), "silent":job.dispatch.silent,
            "thread_id":outcome.turn.as_ref().map(|turn| &turn.thread_id), "turn_id":outcome.turn.as_ref().map(|turn| &turn.id)}),
        events: outcome
            .turn
            .as_ref()
            .map_or_else(Vec::new, super::super::desktop_heartbeat::trace_events),
    }
}

async fn finish(
    lease: &LiveLease,
    job: &CronJobSpec,
    claim: &AgentRunClaim,
    mut outcome: Outcome,
) -> Result<(), ApiError> {
    let server = &lease.server;
    let _guard = server.inner.desktop_cron_lock.lock().await;
    let mut data = read_data(server)?;
    // A deleted job cannot recreate its state, history, or Inbox notification.
    let retained = data.active_runs.get(&lease.run_id) == Some(claim)
        && find_job(&data, &claim.job_id).is_ok()
        && super::owner_key(&data, &claim.job_id) == lease.data_key;
    if let Err(error) =
        super::super::desktop_inbox::write_cron_trace(server, trace(lease, job, &outcome)).await
    {
        outcome.status = "error";
        outcome.error = Some(detail(error));
    }
    if !retained {
        return Ok(());
    }
    if outcome.status == "success" && job.save_result_to_inbox.unwrap_or(true)
        && let Err(error) = super::super::desktop_inbox::append_event(server, NewInboxEvent {
            agent_id: lease.agent_id.clone(), source_type: String::from("cron"), source_id: job.public_id().unwrap_or_default().to_owned(),
            event_type: String::from("cron_result"), status: String::from("success"), severity: String::from("info"),
            title: format!("Cron result: {}", job.name), body: String::from("Agent cron task finished successfully."),
            payload: json!({"run_id":lease.run_id,"job_id":job.public_id(),"job_name":job.name,"task_type":"agent","trigger":claim.trigger,"save_result_to_inbox":true}),
        }).await {
            outcome.status = "error";
            outcome.error = Some(detail(error));
    }
    record_result(
        &mut data,
        &claim.job_id,
        &claim.trigger,
        outcome.status,
        outcome.error,
        Utc::now(),
    );
    data.active_runs.remove(&lease.run_id);
    write_data(server, &data)
}

fn detail((_, Json(body)): ApiError) -> String {
    body.get("detail")
        .and_then(Value::as_str)
        .unwrap_or("Cron operation failed")
        .to_owned()
}

#[cfg(test)]
#[path = "desktop_cron_agent_tests.rs"]
mod tests;
