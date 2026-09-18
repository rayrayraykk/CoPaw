//! Workspace admission and passive producer draining during checkpoint restore.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use axum::Json;
use axum::http::{HeaderMap, StatusCode};
use qwenpaw_core::CoreThreadQuiescenceGuard;
use qwenpaw_storage::WorkspaceDataKey;
use serde_json::{Value, json};
use tokio::sync::MutexGuard;
use tokio_util::sync::CancellationToken;

use super::{AgentContext, AppServer};

type ApiError = (StatusCode, Json<Value>);

#[derive(Default)]
pub(crate) struct State {
    paused: Mutex<BTreeMap<WorkspaceDataKey, CancellationToken>>,
    heartbeat: Mutex<Option<CancellationToken>>,
}

pub(crate) struct HeartbeatLease {
    server: AppServer,
    completed: CancellationToken,
    _operation: qwenpaw_core::CoreOperationGuard,
}

impl Drop for HeartbeatLease {
    fn drop(&mut self) {
        let _paused = self
            .server
            .inner
            .desktop_checkpoint_quiescence
            .paused
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.server
            .inner
            .desktop_checkpoint_quiescence
            .heartbeat
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        self.server
            .inner
            .desktop_heartbeat_running
            .store(false, std::sync::atomic::Ordering::Release);
        self.completed.cancel();
    }
}

/// Register before spawning, atomically with the Workspace pause decision.
pub(crate) fn begin_heartbeat(server: &AppServer) -> Option<HeartbeatLease> {
    let operation = server.inner.core.operation_guard().ok()?;
    let key = super::super::desktop_agents::default_data_key(server).ok()?;
    let paused = server
        .inner
        .desktop_checkpoint_quiescence
        .paused
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if server.inner.shutdown.is_cancelled()
        || paused.contains_key(&key)
        || server
            .inner
            .desktop_heartbeat_running
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_err()
    {
        return None;
    }
    let completed = CancellationToken::new();
    *server
        .inner
        .desktop_checkpoint_quiescence
        .heartbeat
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(completed.clone());
    Some(HeartbeatLease {
        server: server.clone(),
        completed,
        _operation: operation,
    })
}

/// Snapshot admission under its existing lock, then wait without blocking completion.
pub(crate) async fn drain_heartbeat(server: &AppServer) {
    let completed = {
        let _paused = server
            .inner
            .desktop_checkpoint_quiescence
            .paused
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        server
            .inner
            .desktop_checkpoint_quiescence
            .heartbeat
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    };
    if let Some(completed) = completed {
        completed.cancelled().await;
    }
}

pub(super) struct RestoreGuard {
    server: AppServer,
    key: WorkspaceDataKey,
    completed: CancellationToken,
    producers: Vec<CancellationToken>,
    core: Option<CoreThreadQuiescenceGuard>,
}

impl Drop for RestoreGuard {
    fn drop(&mut self) {
        // Release native gates before admitting any new Workspace query.
        drop(self.core.take());
        self.server
            .inner
            .desktop_checkpoint_quiescence
            .paused
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.key);
        self.completed.cancel();
    }
}

pub(crate) fn paused(server: &AppServer, key: &WorkspaceDataKey) -> Option<CancellationToken> {
    server
        .inner
        .desktop_checkpoint_quiescence
        .paused
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(key)
        .cloned()
}

pub(crate) fn is_paused(server: &AppServer, key: &WorkspaceDataKey) -> bool {
    paused(server, key).is_some()
}

pub(crate) fn ensure_available(server: &AppServer, key: &WorkspaceDataKey) -> Result<(), ApiError> {
    if is_paused(server, key) {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({"detail":"Workspace checkpoint restoration is in progress"})),
        ));
    }
    Ok(())
}

/// Return with lifecycle admission held, but never retain it while waiting.
pub(crate) async fn admit_agent<'a>(
    server: &'a AppServer,
    headers: &HeaderMap,
) -> Result<(MutexGuard<'a, ()>, AgentContext), ApiError> {
    loop {
        let lifecycle = server.inner.desktop_agent_lifecycle_lock.lock().await;
        let id = super::super::desktop_agents::requested_agent_id(headers)?;
        let agent = super::super::desktop_agents::context_for_agent(server, &id).await?;
        let Some(completed) = paused(server, &agent.data_key) else {
            return Ok((lifecycle, agent));
        };
        drop(lifecycle);
        wait_for_resume(server, completed).await?;
    }
}

pub(crate) async fn wait_for_resume(
    server: &AppServer,
    completed: CancellationToken,
) -> Result<(), ApiError> {
    tokio::select! {
        () = completed.cancelled() => Ok(()),
        () = server.inner.shutdown.cancelled() => Err((StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"detail":"App Server is stopping"})))),
    }
}

/// Snapshot writers share the restore lock, never lifecycle admission.
pub(super) async fn admit_auto<'a>(
    server: &'a AppServer,
    key: &WorkspaceDataKey,
    cancellation: &CancellationToken,
) -> Option<MutexGuard<'a, ()>> {
    loop {
        let checkpoint = tokio::select! {
            biased;
            () = cancellation.cancelled() => return None,
            () = server.inner.shutdown.cancelled() => return None,
            guard = server.inner.desktop_checkpoint_lock.lock() => guard,
        };
        let Some(completed) = paused(server, key) else {
            return Some(checkpoint);
        };
        drop(checkpoint);
        tokio::select! {
            biased;
            () = cancellation.cancelled() => return None,
            resumed = wait_for_resume(server, completed) => { resumed.ok()?; }
        }
    }
}

/// Control operations must still allow disabled or invalid registrations to be removed.
pub(crate) async fn admit_agent_control<'a>(
    server: &'a AppServer,
    id: &str,
) -> Result<MutexGuard<'a, ()>, ApiError> {
    loop {
        let lifecycle = server.inner.desktop_agent_lifecycle_lock.lock().await;
        let key = super::super::desktop_agents::registered_data_key(server, id).await?;
        let Some(completed) = key.as_ref().and_then(|key| paused(server, key)) else {
            return Ok(lifecycle);
        };
        drop(lifecycle);
        wait_for_resume(server, completed).await?;
    }
}

/// The caller holds lifecycle, Cron and checkpoint locks through this snapshot.
pub(super) fn freeze(server: &AppServer, agent: &AgentContext) -> Result<RestoreGuard, ApiError> {
    let completed = CancellationToken::new();
    {
        let mut scopes = server
            .inner
            .desktop_checkpoint_quiescence
            .paused
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if scopes.contains_key(&agent.data_key) {
            return Err((
                StatusCode::CONFLICT,
                Json(json!({"detail":"Workspace checkpoint restoration is in progress"})),
            ));
        }
        scopes.insert(agent.data_key.clone(), completed.clone());
    }
    let mut guard = RestoreGuard {
        server: server.clone(),
        key: agent.data_key.clone(),
        completed,
        producers: Vec::new(),
        core: None,
    };
    guard
        .producers
        .extend(super::super::desktop_console_runs::workspace_completions(
            server,
            &agent.data_key,
        ));
    guard
        .producers
        .extend(super::super::protocol_runs::workspace_completions(
            server,
            &agent.data_key,
        ));
    guard
        .producers
        .extend(super::super::desktop_cron::workspace_completions(
            server,
            &agent.data_key,
        ));
    // Holding the checkpoint lock has already drained snapshot writes. Pending
    // or lock-waiting snapshots retain their timers and wait for this restore;
    // waiting for their completion here would deadlock against our own gate.
    if agent.data_key == super::super::desktop_agents::default_data_key(server)? {
        guard.producers.extend(
            server
                .inner
                .desktop_checkpoint_quiescence
                .heartbeat
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone(),
        );
    }
    Ok(guard)
}

impl RestoreGuard {
    /// No application locks may be held: completion publishes catalog state.
    pub(super) async fn drain(
        mut self,
        agent: &AgentContext,
        timeout: Duration,
    ) -> Result<Self, ApiError> {
        let result = tokio::time::timeout(timeout, async {
            for token in &self.producers {
                token.cancelled().await;
            }
            let ids = super::super::desktop_chats::bound_checkpoint_sessions(&self.server, agent)
                .await?
                .into_iter()
                .map(|session| session.thread_id)
                .collect::<Vec<_>>();
            self.core = Some(
                self.server
                    .inner
                    .core
                    .quiesce_threads(&ids, timeout)
                    .await
                    .map_err(|error| match error {
                        qwenpaw_core::CoreError::RestoreTimeout => timeout_error(timeout),
                        _ => super::core_error(error),
                    })?,
            );
            Ok::<(), ApiError>(())
        })
        .await;
        match result {
            Ok(result) => result?,
            Err(_) => return Err(timeout_error(timeout)),
        }
        Ok(self)
    }
}

fn timeout_error(timeout: Duration) -> ApiError {
    (
        StatusCode::BAD_REQUEST,
        Json(
            json!({"detail":format!("Checkpoint restore was cancelled because workspace tasks did not become idle within {:.1}s.", timeout.as_secs_f64())}),
        ),
    )
}

/// SDK-created Threads belong to default; known Threads keep catalog ownership.
pub(crate) async fn admit_protocol<'a>(
    server: &'a AppServer,
    method: &str,
    params: &Value,
) -> Result<Option<MutexGuard<'a, ()>>, ApiError> {
    if server.inner.desktop_workspace.is_none()
        || !matches!(
            method,
            "thread/start" | "thread/resume" | "thread/archive" | "turn/start"
        )
    {
        return Ok(None);
    }
    loop {
        let lifecycle = server.inner.desktop_agent_lifecycle_lock.lock().await;
        let key = super::super::desktop_chats::protocol_workspace_key(
            server,
            params.get("threadId").and_then(Value::as_str),
        )
        .await?;
        let Some(completed) = paused(server, &key) else {
            return Ok(Some(lifecycle));
        };
        drop(lifecycle);
        wait_for_resume(server, completed).await?;
    }
}
