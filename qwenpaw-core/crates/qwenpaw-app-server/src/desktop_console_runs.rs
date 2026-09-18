//! Console turn ownership and cancellation, independent of SSE backpressure.

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::sync::Mutex;
use std::time::Duration;

use axum::Json;
use axum::http::StatusCode;
use axum::response::sse::Event;
use qwenpaw_core::{CoreOperationGuard, TurnEventStream};
use qwenpaw_protocol::{CoreEvent, Turn, TurnInterruptParams};
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use super::AppServer;
use super::desktop_api::{clear_turn_approvals, console_event, track_run_approval};
use super::desktop_chats::ApprovalSessionInfo;

type ConsoleEvent = Result<Event, Infallible>;
type ApiError = (StatusCode, Json<serde_json::Value>);

#[cfg(test)]
#[path = "desktop_console_runs_tests.rs"]
mod tests;

#[derive(Default)]
pub(super) struct RunState {
    live: Mutex<BTreeMap<String, LiveRun>>,
    tasks: TaskTracker,
}

struct LiveRun {
    agent_id: String,
    data_key: qwenpaw_storage::WorkspaceDataKey,
    cancellation: CancellationToken,
    completed: CancellationToken,
}

struct Lease {
    server: AppServer,
    turn: Turn,
    identity: ApprovalSessionInfo,
    agent: super::desktop_agents::AgentContext,
    query: Option<String>,
    cancellation: CancellationToken,
    completed: CancellationToken,
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.server
            .inner
            .desktop_console_runs
            .live
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.turn.id);
        self.completed.cancel();
    }
}

/// Admission and live registration occur under the Agent lifecycle lock.
pub(super) fn spawn(
    server: &AppServer,
    turn: Turn,
    events: TurnEventStream,
    identity: ApprovalSessionInfo,
    agent: super::desktop_agents::AgentContext,
    query: Option<String>,
    operation: CoreOperationGuard,
) -> mpsc::Receiver<ConsoleEvent> {
    let cancellation = server.inner.shutdown.child_token();
    let completed = CancellationToken::new();
    server
        .inner
        .desktop_console_runs
        .live
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(
            turn.id.clone(),
            LiveRun {
                agent_id: identity.agent.clone(),
                data_key: agent.data_key.clone(),
                cancellation: cancellation.clone(),
                completed: completed.clone(),
            },
        );
    let lease = Lease {
        server: server.clone(),
        turn,
        identity,
        agent,
        query,
        cancellation,
        completed,
    };
    let (sender, receiver) = mpsc::channel(64);
    server.inner.desktop_console_runs.tasks.spawn(async move {
        let _operation = operation;
        consume(&lease, events, sender).await;
        clear_turn_approvals(&lease.server, &lease.turn.id).await;
    });
    receiver
}

/// Capture the fence before releasing the lifecycle lock or admitting reuse.
pub(super) fn cancel_agent(server: &AppServer, agent_id: &str) -> Vec<CancellationToken> {
    server
        .inner
        .desktop_console_runs
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

pub(super) async fn drain_runs(completed: Vec<CancellationToken>) -> Result<(), ApiError> {
    tokio::time::timeout(Duration::from_secs(10), async {
        for completion in completed {
            completion.cancelled().await;
        }
    })
    .await
    .map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"detail":"Console Agent runs did not finish stopping"})),
        )
    })
}

pub(super) fn workspace_completions(
    server: &AppServer,
    key: &qwenpaw_storage::WorkspaceDataKey,
) -> Vec<CancellationToken> {
    server
        .inner
        .desktop_console_runs
        .live
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .values()
        .filter(|run| &run.data_key == key)
        .map(|run| run.completed.clone())
        .collect()
}

pub(super) async fn shutdown(server: &AppServer) {
    server.inner.desktop_console_runs.tasks.close();
    server.inner.desktop_console_runs.tasks.wait().await;
}

async fn interrupt(lease: &Lease) {
    let _ = lease
        .server
        .inner
        .core
        .interrupt_turn(&TurnInterruptParams {
            thread_id: lease.turn.thread_id.clone(),
            turn_id: lease.turn.id.clone(),
        })
        .await;
}

async fn consume(lease: &Lease, mut events: TurnEventStream, sender: mpsc::Sender<ConsoleEvent>) {
    let mut interrupted = false;
    loop {
        let event = tokio::select! {
            biased;
            () = lease.cancellation.cancelled(), if !interrupted => {
                interrupt(lease).await;
                interrupted = true;
                continue;
            }
            () = sender.closed(), if !interrupted => {
                interrupt(lease).await;
                interrupted = true;
                continue;
            }
            event = events.recv() => match event {
                Some(event) => event,
                None => return,
            }
        };
        let terminal = matches!(&event, CoreEvent::TurnCompleted(_));
        let completed = matches!(&event, CoreEvent::TurnCompleted(event)
            if super::desktop_checkpoints::auto_snapshot_eligible(&event.turn, lease.query.as_deref()));
        track_run_approval(&lease.server, &event, Some(&lease.identity)).await;
        if let Some(payload) = console_event(event) {
            let output = Ok(Event::default().data(payload.to_string()));
            if interrupted {
                // Drain Core even when the peer has stopped reading. Preserve
                // the terminal event when there is room, without waiting on it.
                let _ = sender.try_send(output);
            } else {
                let permit = tokio::select! {
                    biased;
                    () = lease.cancellation.cancelled() => None,
                    result = sender.reserve() => result.ok(),
                };
                if let Some(permit) = permit {
                    permit.send(output);
                } else {
                    let _ = sender.try_send(output);
                    interrupt(lease).await;
                    interrupted = true;
                }
            }
        }
        if terminal {
            clear_turn_approvals(&lease.server, &lease.turn.id).await;
            if completed && !interrupted && !lease.cancellation.is_cancelled() {
                super::desktop_checkpoints::schedule_auto_checkpoint(
                    &lease.server,
                    &lease.turn.thread_id,
                    &lease.turn.id,
                    &lease.agent,
                    lease.query.clone(),
                    &lease.cancellation,
                )
                .await;
            }
            return;
        }
    }
}
