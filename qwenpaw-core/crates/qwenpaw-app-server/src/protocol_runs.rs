//! Own protocol completion independently of response and notification delivery.

use std::collections::BTreeMap;
use std::sync::Mutex;

use qwenpaw_core::{CoreOperationGuard, TurnEventStream};
use qwenpaw_protocol::{
    CoreEvent, Turn, TurnInterruptParams, TurnStartParams, TurnStartResponse, UserInput,
};
use qwenpaw_storage::WorkspaceDataKey;
use tokio::sync::mpsc;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use super::desktop_agents::AgentContext;
use super::desktop_chats::ApprovalSessionInfo;
use super::{AppServer, DispatchError};

#[derive(Default)]
pub(super) struct RunState {
    live: Mutex<BTreeMap<String, LiveRun>>,
    tasks: TaskTracker,
}

struct LiveRun {
    agent_id: Option<String>,
    data_key: Option<WorkspaceDataKey>,
    cancellation: CancellationToken,
    completed: CancellationToken,
}

pub(super) struct Context {
    agent: Option<AgentContext>,
    identity: Option<ApprovalSessionInfo>,
    query: Option<String>,
}

pub(super) async fn context(
    server: &AppServer,
    thread: &str,
    input: &[UserInput],
) -> Result<Context, DispatchError> {
    let mut context = Context {
        agent: None,
        identity: None,
        query: None,
    };
    let query = input
        .iter()
        .filter_map(|part| match part {
            UserInput::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    context.query = (!query.is_empty()).then_some(query);
    if server.inner.desktop_workspace.is_some() {
        let key = super::desktop_chats::protocol_workspace_key(server, Some(thread))
            .await
            .map_err(api_error)?;
        let agent = super::desktop_agents::context_for_data_key(server, &key)
            .await
            .map_err(api_error)?;
        context.identity = Some(
            super::desktop_chats::bound_approval_session_info(server, thread, &agent)
                .await
                .map_err(api_error)?,
        );
        context.agent = Some(agent);
    }
    Ok(context)
}

fn api_error((_, body): (axum::http::StatusCode, axum::Json<serde_json::Value>)) -> DispatchError {
    DispatchError {
        code: -32000,
        message: body.0["detail"]
            .as_str()
            .unwrap_or("Protocol admission failed")
            .to_owned(),
    }
}

pub(super) async fn start(
    server: &AppServer,
    context: &Context,
    params: TurnStartParams,
) -> Result<(TurnStartResponse, TurnEventStream), DispatchError> {
    let Some(agent) = &context.agent else {
        return server
            .inner
            .core
            .start_turn(params)
            .await
            .map_err(|error| DispatchError::core(&error));
    };
    let runtime = match agent.config.get("running") {
        Some(running) => {
            super::desktop_agent_settings::runtime_config(running).map_err(api_error)?
        }
        None => server
            .inner
            .core
            .agent_runtime_config()
            .map_err(|error| DispatchError::core(&error))?,
    };
    let selection = super::desktop_models::runtime_for_agent_config(server, &agent.config)
        .await
        .map_err(api_error)?;
    server
        .inner
        .core
        .start_turn_with_thread_model(params, selection, runtime, Some(agent.usage_owner()))
        .await
        .map_err(|error| DispatchError::core(&error))
}

struct Lease {
    server: AppServer,
    turn: Turn,
    context: Context,
    cancellation: CancellationToken,
    completed: CancellationToken,
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.server
            .inner
            .protocol_runs
            .live
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.turn.id);
        self.completed.cancel();
    }
}

/// The caller retains protocol admission through registration, before its response.
pub(super) fn spawn(
    server: &AppServer,
    turn: Turn,
    events: TurnEventStream,
    context: Context,
    operation: CoreOperationGuard,
) -> TurnEventStream {
    let cancellation = server.inner.shutdown.child_token();
    let completed = CancellationToken::new();
    server
        .inner
        .protocol_runs
        .live
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(
            turn.id.clone(),
            LiveRun {
                agent_id: context.agent.as_ref().map(|agent| agent.agent_id.clone()),
                data_key: context.agent.as_ref().map(|agent| agent.data_key.clone()),
                cancellation: cancellation.clone(),
                completed: completed.clone(),
            },
        );
    let lease = Lease {
        server: server.clone(),
        turn,
        context,
        cancellation,
        completed,
    };
    let (sender, receiver) = mpsc::channel(64);
    let shutdown = server.inner.shutdown.clone();
    server.inner.protocol_runs.tasks.spawn(async move {
        let terminal = consume(&lease, events, &sender).await;
        if lease.context.identity.is_some() {
            super::desktop_api::clear_turn_approvals(&lease.server, &lease.turn.id).await;
        }
        // No state writes remain. Terminal delivery may still be backpressured
        // but must not keep Workspace or Backup restoration waiting.
        drop(operation);
        drop(lease);
        if let Some(event) = terminal {
            tokio::select! {
                result = sender.send(event) => { let _ = result; }
                () = shutdown.cancelled() => {}
            }
        }
    });
    receiver
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

async fn consume(
    lease: &Lease,
    mut events: TurnEventStream,
    sender: &mpsc::Sender<CoreEvent>,
) -> Option<CoreEvent> {
    let mut interrupted = false;
    let mut forwarding = true;
    loop {
        let event = tokio::select! {
            biased;
            () = lease.cancellation.cancelled(), if !interrupted => {
                interrupt(lease).await;
                interrupted = true;
                forwarding = false;
                continue;
            }
            event = events.recv() => event?,
        };
        if let Some(identity) = &lease.context.identity {
            super::desktop_api::track_run_approval(&lease.server, &event, Some(identity)).await;
        }
        if let CoreEvent::TurnCompleted(completed) = &event {
            if !lease.cancellation.is_cancelled()
                && super::desktop_checkpoints::auto_snapshot_eligible(
                    &completed.turn,
                    lease.context.query.as_deref(),
                )
                && let Some(agent) = &lease.context.agent
            {
                super::desktop_checkpoints::schedule_auto_checkpoint(
                    &lease.server,
                    &lease.turn.thread_id,
                    &lease.turn.id,
                    agent,
                    lease.context.query.clone(),
                    &lease.cancellation,
                )
                .await;
            }
            return Some(event);
        }
        if forwarding {
            tokio::select! {
                biased;
                () = lease.cancellation.cancelled(), if !interrupted => {
                    interrupt(lease).await;
                    interrupted = true;
                    forwarding = false;
                }
                result = sender.send(event) => {
                    // A disconnected SDK previously allowed the Core to finish.
                    // Keep that behavior while retaining completion ownership.
                    forwarding = result.is_ok();
                }
            }
        }
    }
}

pub(super) fn cancel_agent(server: &AppServer, agent_id: &str) -> Vec<CancellationToken> {
    server
        .inner
        .protocol_runs
        .live
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .values()
        .filter(|run| run.agent_id.as_deref() == Some(agent_id))
        .map(|run| {
            run.cancellation.cancel();
            run.completed.clone()
        })
        .collect()
}

pub(super) fn workspace_completions(
    server: &AppServer,
    key: &WorkspaceDataKey,
) -> Vec<CancellationToken> {
    server
        .inner
        .protocol_runs
        .live
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .values()
        .filter(|run| run.data_key.as_ref() == Some(key))
        .map(|run| run.completed.clone())
        .collect()
}

pub(super) async fn shutdown(server: &AppServer) {
    server.inner.protocol_runs.tasks.close();
    server.inner.protocol_runs.tasks.wait().await;
}
