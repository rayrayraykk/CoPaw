//! Owned turn protocol stream. Harness/Chat event conversion is a separate layer.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::sessions::{PreparedSession, SessionRequest};
use super::{CodexClient, Error};
use crate::capabilities::path_text;

#[derive(Clone)]
pub enum Attachment {
    Image(PathBuf),
    File { path: PathBuf, name: String },
}

#[derive(Clone, Default)]
pub struct TurnInput {
    pub prompt: String,
    pub attachments: Vec<Attachment>,
    pub reasoning_effort: Option<String>,
    pub reasoning_summary: Option<String>,
}

/// Remote completion retains its full notification, including failed status.
#[derive(Debug, Clone, PartialEq)]
pub enum TurnEnd {
    Remote(Value),
    InterruptAcknowledged,
}

/// Dropping requests interruption; only finish verifies the worker's outcome.
/// Notifications remain raw and must not be exposed as normalized Chat events.
pub struct CodexTurn {
    incoming: mpsc::Receiver<Value>,
    cancel: CancellationToken,
    worker: Option<JoinHandle<Result<TurnEnd, Error>>>,
}

impl CodexTurn {
    pub(super) fn start(
        session: PreparedSession,
        mut params: Value,
        timeout: Duration,
    ) -> Result<Self, Error> {
        let notifications = session.runtime.client.subscribe()?;
        params["threadId"] = json!(session.thread_id);
        let (sender, incoming) = mpsc::channel(64);
        let cancel = CancellationToken::new();
        let cancellation = cancel.clone();
        let worker = tokio::spawn(async move {
            run(
                session,
                params,
                timeout,
                notifications,
                sender,
                cancellation,
            )
            .await
        });
        Ok(Self {
            incoming,
            cancel,
            worker: Some(worker),
        })
    }

    /// Receives a complete filtered notification. Call finish after EOF to observe
    /// transport/lag/start/interrupt errors; EOF by itself does not mean success.
    pub async fn next(&mut self) -> Option<Value> {
        self.incoming.recv().await
    }

    /// Requests interruption, including when the start reply is still pending.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Drains remaining notifications and joins the owner. Without cancellation,
    /// waits for remote completion; this is not a remote execution time limit.
    ///
    /// # Errors
    /// Returns RPC, EOF, lag or worker failure. Failed remote turns remain Remote
    /// with their original status/error, never converted to a successful turn.
    pub async fn finish(mut self) -> Result<TurnEnd, Error> {
        while self.incoming.recv().await.is_some() {}
        self.worker
            .take()
            .ok_or(Error::Closed)?
            .await
            .map_err(|_| Error::Worker)?
    }
}

impl Drop for CodexTurn {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

pub(super) fn parameters(request: &SessionRequest, input: TurnInput) -> Result<Value, Error> {
    let mut blocks = Vec::new();
    if !input.prompt.is_empty() {
        blocks.push(json!({"type":"text","text":input.prompt}));
    }
    for attachment in input.attachments {
        blocks.push(match attachment {
            Attachment::Image(path) => json!({"type":"localImage","path":path_text(&path)?}),
            Attachment::File { path, name } => {
                let name = if name.is_empty() {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .ok_or(crate::capabilities::CapabilityError::InvalidPath)?
                } else {
                    &name
                };
                json!({"type":"text","text":format!("Attached file {name}: {}", path_text(&path)?)})
            }
        });
    }
    let options = &request.options;
    let mut params = json!({"cwd":path_text(&request.cwd)?,"input":blocks,
        "summary":input.reasoning_summary.as_deref().filter(|v| !v.is_empty()).unwrap_or("auto")});
    for (key, value) in [
        ("model", &options.model),
        ("effort", &input.reasoning_effort),
        ("approvalPolicy", &options.approval_policy),
    ] {
        if let Some(value) = value.as_ref().filter(|v| !v.is_empty()) {
            params[key] = json!(value);
        }
    }
    let policy = match options.sandbox.as_deref() {
        Some("read-only") => Some("readOnly"),
        Some("workspace-write") => Some("workspaceWrite"),
        Some("danger-full-access") => Some("dangerFullAccess"),
        _ => None,
    };
    if let Some(policy) = policy {
        params["sandboxPolicy"] = json!({"type":policy});
    }
    Ok(params)
}

async fn run(
    session: PreparedSession,
    params: Value,
    timeout: Duration,
    mut notifications: broadcast::Receiver<Value>,
    sender: mpsc::Sender<Value>,
    cancel: CancellationToken,
) -> Result<TurnEnd, Error> {
    let client = &session.runtime.client;
    // Do not cancel this waiter: an accepted remote start can still return its ID.
    let response = client.request("turn/start", params, timeout).await?;
    let turn = response
        .get("turn")
        .and_then(|v| v.get("id"))
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .ok_or(Error::MissingTurnId)?;
    let thread = &session.thread_id;
    loop {
        let message = tokio::select! {
            biased;
            () = cancel.cancelled() => return interrupt(client, thread, turn, timeout).await,
            () = sender.closed() => return interrupt(client, thread, turn, timeout).await,
            message = notifications.recv() => match message {
                Ok(message) => message,
                Err(broadcast::error::RecvError::Closed) => return Err(Error::Closed),
                Err(broadcast::error::RecvError::Lagged(count)) => {
                    interrupt(client, thread, turn, timeout).await?;
                    return Err(Error::NotificationLagged(count));
                }
            },
        };
        if !matches_turn(&message, thread, turn) {
            continue;
        }
        let completed = message["method"] == "turn/completed";
        tokio::select! {
            biased;
            () = cancel.cancelled() => return interrupt(client, thread, turn, timeout).await,
            result = sender.send(message.clone()) => if result.is_err() {
                return interrupt(client, thread, turn, timeout).await;
            },
        }
        if completed {
            return Ok(TurnEnd::Remote(message));
        }
    }
}

fn matches_turn(message: &Value, thread: &str, turn: &str) -> bool {
    let params = &message["params"];
    if params["threadId"].as_str() != Some(thread) {
        return false;
    }
    let id = params
        .get("turnId")
        .filter(|value| super::control::truthy(value))
        .or_else(|| {
            params["turn"]
                .get("id")
                .filter(|value| super::control::truthy(value))
        });
    match id {
        None => true,
        Some(Value::String(id)) => id == turn,
        Some(Value::Number(id)) if id.is_i64() || id.is_u64() => id.to_string() == turn,
        Some(Value::Bool(true)) => turn == "True",
        _ => false,
    }
}

async fn interrupt(
    client: &CodexClient,
    thread: &str,
    turn: &str,
    timeout: Duration,
) -> Result<TurnEnd, Error> {
    client
        .request(
            "turn/interrupt",
            json!({"threadId":thread,"turnId":turn}),
            timeout,
        )
        .await?;
    Ok(TurnEnd::InterruptAcknowledged)
}

#[cfg(test)]
mod tests;
