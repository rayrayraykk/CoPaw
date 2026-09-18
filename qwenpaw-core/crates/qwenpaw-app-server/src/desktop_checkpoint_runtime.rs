//! Debounced, owned checkpoint work; never abort an in-progress file write.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use uuid::Uuid;

use super::{AgentContext, AppServer, WorkspaceDataKey};

const AUTO_DELAY: Duration = Duration::from_millis(1_500);
const AUTO_GC_INTERVAL: Duration = Duration::from_secs(15 * 60);

#[derive(Default)]
pub(crate) struct RuntimeState {
    jobs: Mutex<BTreeMap<Uuid, Job>>,
    tasks: TaskTracker,
    last_gc: Mutex<BTreeMap<WorkspaceDataKey, Instant>>,
}

struct Job {
    agent_id: String,
    workspace: WorkspaceDataKey,
    session: String,
    thread: String,
    active: bool,
    cancellation: CancellationToken,
    completed: CancellationToken,
}

struct Lease {
    server: AppServer,
    id: Uuid,
    cancellation: CancellationToken,
    completed: CancellationToken,
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.server
            .inner
            .desktop_checkpoint_runtime
            .jobs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.id);
        self.completed.cancel();
    }
}

impl RuntimeState {
    pub(super) fn touch(&self, key: &WorkspaceDataKey) {
        self.last_gc
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(key.clone())
            .or_insert_with(Instant::now);
    }

    pub(super) fn gc_due(&self, key: &WorkspaceDataKey) -> bool {
        let mut clocks = self
            .last_gc
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let last = clocks.entry(key.clone()).or_insert_with(Instant::now);
        if last.elapsed() < AUTO_GC_INTERVAL {
            return false;
        }
        *last = Instant::now();
        true
    }
}

pub(crate) fn enqueue(
    server: &AppServer,
    agent: &AgentContext,
    thread: &str,
    session: String,
    query: Option<String>,
    cancellation: &CancellationToken,
) -> Option<CancellationToken> {
    let state = &server.inner.desktop_checkpoint_runtime;
    let mut jobs = state
        .jobs
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if cancellation.is_cancelled()
        || server.inner.shutdown.is_cancelled()
        || state.tasks.is_closed()
        || super::super::desktop_backups::is_restoring(server)
    {
        return None;
    }
    for job in jobs
        .values()
        .filter(|job| !job.active && job.workspace == agent.data_key && job.session == session)
    {
        job.cancellation.cancel();
    }
    let id = Uuid::now_v7();
    let cancellation = cancellation.child_token();
    let completed = CancellationToken::new();
    jobs.insert(
        id,
        Job {
            agent_id: agent.agent_id.clone(),
            workspace: agent.data_key.clone(),
            session,
            thread: thread.to_owned(),
            active: false,
            cancellation: cancellation.clone(),
            completed: completed.clone(),
        },
    );
    let lease = Lease {
        server: server.clone(),
        id,
        cancellation,
        completed: completed.clone(),
    };
    let agent = agent.clone();
    let thread = thread.to_owned();
    let deadline = Instant::now() + AUTO_DELAY;
    // Register before releasing the mutex so shutdown cannot miss the task.
    state.tasks.spawn(async move {
        tokio::select! {
            biased;
            () = lease.cancellation.cancelled() => return,
            () = lease.server.inner.shutdown.cancelled() => return,
            () = tokio::time::sleep_until(deadline) => {}
        }
        {
            let mut jobs = lease
                .server
                .inner
                .desktop_checkpoint_runtime
                .jobs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if lease.cancellation.is_cancelled() {
                return;
            }
            jobs.get_mut(&lease.id)
                .expect("registered checkpoint task")
                .active = true;
        }
        if super::super::desktop_backups::is_restoring(&lease.server) {
            return;
        }
        let Ok(_operation) = lease.server.inner.core.operation_guard() else {
            return;
        };
        super::maybe_create_auto_checkpoint(
            &lease.server,
            &thread,
            &agent,
            query,
            &lease.cancellation,
        )
        .await;
    });
    Some(completed)
}

fn cancel_matching(server: &AppServer, matches: impl Fn(&Job) -> bool) -> Vec<CancellationToken> {
    server
        .inner
        .desktop_checkpoint_runtime
        .jobs
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .values()
        .filter(|job| matches(job))
        .map(|job| {
            job.cancellation.cancel();
            job.completed.clone()
        })
        .collect()
}

pub(crate) fn cancel_agent(server: &AppServer, agent: &str) -> Vec<CancellationToken> {
    cancel_matching(server, |job| job.agent_id == agent)
}

pub(super) fn cancel_workspace(server: &AppServer, workspace: &WorkspaceDataKey) {
    cancel_matching(server, |job| &job.workspace == workspace);
}

pub(super) fn cancel_threads(server: &AppServer, workspace: &WorkspaceDataKey, threads: &[String]) {
    cancel_matching(server, |job| {
        &job.workspace == workspace && threads.contains(&job.thread)
    });
}

pub(crate) async fn drain(completed: Vec<CancellationToken>) {
    for completion in completed {
        completion.cancelled().await;
    }
}

pub(crate) async fn cancel_all_and_drain(server: &AppServer) {
    drain(cancel_matching(server, |_| true)).await;
    server
        .inner
        .desktop_checkpoint_runtime
        .last_gc
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clear();
}

pub(crate) async fn shutdown(server: &AppServer) {
    let state = &server.inner.desktop_checkpoint_runtime;
    {
        let jobs = state
            .jobs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.tasks.close();
        for job in jobs.values() {
            job.cancellation.cancel();
        }
    }
    state.tasks.wait().await;
}

#[cfg(test)]
pub(crate) fn task_counts(server: &AppServer) -> (usize, usize) {
    let jobs = server.inner.desktop_checkpoint_runtime.jobs.lock().unwrap();
    (
        jobs.values().filter(|job| !job.active).count(),
        jobs.values().filter(|job| job.active).count(),
    )
}
