//! Bidirectional Codex app-server JSONL transport, independent of `QwenPaw` RPC.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::FutureExt;
use futures_util::future::BoxFuture;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::{JoinHandle, JoinSet};
use tokio_util::sync::CancellationToken;

const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
const MAX_PENDING: usize = 256;
const MAX_APPROVALS: usize = 64;
const WRITE_TIMEOUT: Duration = Duration::from_secs(15);
const STOP_TIMEOUT: Duration = Duration::from_secs(5);

mod control;
pub use control::{AccountStatus, HarnessDiscoveredSkill, HarnessModel};
pub mod discovery;
pub mod lifecycle;
mod mcp_discovery;
pub use mcp_discovery::HarnessDiscoveredMcpServer;
pub mod projection;
pub mod provider;
pub mod runtime;
pub mod sessions;
pub mod turn;

/// Transport failures; protocol errors retain the server's structured details.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("Codex did not return a turn id")]
    MissingTurnId,
    #[error("Codex turn subscriber lost {0} notifications")]
    NotificationLagged(u64),
    #[error("Codex did not return a thread id")]
    MissingThreadId,
    #[error("Invalid Codex session state")]
    InvalidSessionState,
    #[error(transparent)]
    Capability(#[from] crate::capabilities::CapabilityError),
    #[error("{0}")]
    McpDiscovery(String),
    #[error("Codex MCP discovery output exceeds the size limit")]
    McpOutputLimit,
    #[error("Codex MCP discovery cleanup failed: {0}")]
    McpCleanup(Box<Error>),
    #[error("Codex CLI not found")]
    NotInstalled,
    #[error("Codex app-server transport closed")]
    Closed,
    #[error("Codex app-server request timed out")]
    Timeout,
    #[error("Codex app-server I/O failed: {0:?}")]
    Io(std::io::ErrorKind),
    #[error("Codex app-server emitted an invalid protocol frame")]
    InvalidFrame,
    #[error("Codex app-server frame exceeds the size limit")]
    FrameTooLarge,
    #[error("Codex app-server pending request limit reached")]
    Capacity,
    #[error("Codex app-server returned error {code}: {message}")]
    Protocol {
        code: i64,
        message: String,
        data: Value,
    },
    #[error("Codex app-server exited unsuccessfully: {0:?}")]
    ProcessExit(Option<i32>),
    #[error("Codex app-server did not stop gracefully")]
    StopTimeout,
    #[error("Codex app-server worker failed")]
    Worker,
    #[error("Codex app-server startup cleanup failed")]
    StartupCleanup(Box<Error>),
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.kind())
    }
}

/// Callback for server-originated requests; no callback means decline.
pub type RequestHandler =
    Arc<dyn Fn(Value) -> BoxFuture<'static, Result<Value, Error>> + Send + Sync>;
type Reply = oneshot::Sender<Result<Value, Error>>;

struct State {
    next_id: u64,
    pending: HashMap<u64, Reply>,
    closed: Option<Error>,
    notifications: Option<broadcast::Sender<Value>>,
    handler: Option<RequestHandler>,
}

struct Shared {
    state: Mutex<State>,
    stop: CancellationToken,
}

impl Shared {
    fn close(&self, error: Error) {
        let mut state = self.state.lock().expect("transport state lock poisoned");
        if state.closed.is_none() {
            state.notifications.take();
            for (_, reply) in state.pending.drain() {
                let _ = reply.send(Err(error.clone()));
            }
            state.closed = Some(error);
        }
        let handler = state.handler.take();
        drop(state);
        self.stop.cancel();
        drop(handler);
    }
}

struct Frame {
    value: Value,
    request_id: Option<u64>,
    written: oneshot::Sender<Result<(), Error>>,
}

/// Cloneable request endpoint for one process generation.
#[derive(Clone)]
pub struct CodexClient {
    shared: Arc<Shared>,
    outgoing: mpsc::Sender<Frame>,
}

struct Pending {
    id: u64,
    shared: Arc<Shared>,
}

impl Drop for Pending {
    fn drop(&mut self) {
        self.shared
            .state
            .lock()
            .expect("transport state lock poisoned")
            .pending
            .remove(&self.id);
    }
}

impl CodexClient {
    /// Sends a request. Dropping it removes the local waiter, not the remote turn.
    ///
    /// # Errors
    /// Returns transport, capacity, timeout, or structured remote errors.
    ///
    /// # Panics
    /// Panics if the internal state mutex was poisoned.
    pub async fn request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, Error> {
        let (tx, rx) = oneshot::channel();
        let id = {
            let mut state = self
                .shared
                .state
                .lock()
                .expect("transport state lock poisoned");
            if let Some(error) = &state.closed {
                return Err(error.clone());
            }
            if state.pending.len() >= MAX_PENDING {
                return Err(Error::Capacity);
            }
            let id = state.next_id;
            if id > i64::MAX as u64 {
                return Err(Error::Capacity);
            }
            state.next_id = id.checked_add(1).ok_or(Error::Capacity)?;
            state.pending.insert(id, tx);
            id
        };
        let _pending = Pending {
            id,
            shared: self.shared.clone(),
        };
        tokio::time::timeout(timeout, async {
            self.send(
                json!({"id": id, "method": method, "params": params}),
                Some(id),
            )
            .await?;
            rx.await.map_err(|_| Error::Closed)?
        })
        .await
        .map_err(|_| Error::Timeout)?
    }

    /// Sends and flushes a notification, with a bounded write wait.
    ///
    /// # Errors
    /// Returns a transport error or timeout.
    pub async fn notify(&self, method: &str, params: Value) -> Result<(), Error> {
        self.send(json!({"method": method, "params": params}), None)
            .await
    }

    /// Subscribes to notifications; lag is reported by the receiver, never hidden.
    ///
    /// # Errors
    /// Returns the terminal transport error after this generation closes.
    ///
    /// # Panics
    /// Panics if the internal state mutex was poisoned.
    pub fn subscribe(&self) -> Result<broadcast::Receiver<Value>, Error> {
        let state = self
            .shared
            .state
            .lock()
            .expect("transport state lock poisoned");
        match &state.notifications {
            Some(sender) => Ok(sender.subscribe()),
            None => Err(state.closed.clone().unwrap_or(Error::Closed)),
        }
    }

    /// Sets the handler for future server requests without blocking the reader.
    ///
    /// # Panics
    /// Panics if the internal state mutex was poisoned.
    pub fn set_request_handler(&self, handler: Option<RequestHandler>) {
        let mut state = self
            .shared
            .state
            .lock()
            .expect("transport state lock poisoned");
        if state.closed.is_some() {
            return;
        }
        let previous = std::mem::replace(&mut state.handler, handler);
        drop(state);
        drop(previous);
    }

    async fn send(&self, value: Value, request_id: Option<u64>) -> Result<(), Error> {
        let (written, ack) = oneshot::channel();
        tokio::time::timeout(WRITE_TIMEOUT, async {
            self.outgoing
                .send(Frame {
                    value,
                    request_id,
                    written,
                })
                .await
                .map_err(|_| Error::Closed)?;
            ack.await.map_err(|_| Error::Closed)?
        })
        .await
        .map_err(|_| Error::Timeout)?
    }
}

/// Owns the child and all transport workers. Only explicit shutdown proves reaping.
pub struct CodexProcess {
    client: CodexClient,
    supervisor: Option<JoinHandle<Result<(), Error>>>,
}

impl CodexProcess {
    /// Launches a configured command and completes the original initialization.
    /// The caller supplies executable, arguments, workspace and environment.
    ///
    /// # Errors
    /// Returns spawn, handshake, protocol, or timeout failures.
    pub async fn spawn(command: Command, timeout: Duration) -> Result<Self, Error> {
        Self::spawn_with_handler(command, timeout, None).await
    }

    async fn spawn_with_handler(
        mut command: Command,
        timeout: Duration,
        handler: Option<RequestHandler>,
    ) -> Result<Self, Error> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let input = child.stdin.take().ok_or(Error::Closed)?;
        let output = child.stdout.take().ok_or(Error::Closed)?;
        let mut stderr = child.stderr.take().ok_or(Error::Closed)?;
        let (client, reader, writer) = connect(output, input, handler);
        let shared = client.shared.clone();
        let stderr_worker = tokio::spawn(async move {
            // Drain bounded chunks without retaining or logging credential-bearing text.
            tokio::io::copy(&mut stderr, &mut tokio::io::sink()).await
        });
        let supervisor = tokio::spawn(async move {
            let result = tokio::select! {
                status = child.wait() => status.map_err(Error::from).and_then(|status| {
                    if status.success() { Ok(()) } else { Err(Error::ProcessExit(status.code())) }
                }),
                () = shared.stop.cancelled() => {
                    writer.abort();
                    stop_child(&mut child).await
                }
            };
            shared.close(result.clone().err().unwrap_or(Error::Closed));
            reader.abort();
            writer.abort();
            stderr_worker.abort();
            let _ = reader.await;
            let _ = writer.await;
            let _ = stderr_worker.await;
            result
        });
        let process = Self {
            client,
            supervisor: Some(supervisor),
        };
        let handshake = async {
            process
                .client
                .request(
                    "initialize",
                    json!({"clientInfo": {
                        "name": "qwenpaw", "title": "QwenPaw", "version": "1"
                    }}),
                    timeout,
                )
                .await?;
            process.client.notify("initialized", json!({})).await
        }
        .await;
        if let Err(error) = handshake {
            match process.shutdown().await {
                Ok(()) | Err(Error::ProcessExit(_)) => {}
                Err(cleanup) => return Err(Error::StartupCleanup(Box::new(cleanup))),
            }
            return Err(error);
        }
        Ok(process)
    }

    /// Returns a client bound to this process, never to a later replacement.
    #[must_use]
    pub fn client(&self) -> CodexClient {
        self.client.clone()
    }

    /// Closes stdin, rejects waiters and reaps the child; forced exit is an error.
    ///
    /// # Errors
    /// Returns abnormal exit, forced stop, I/O or worker failures.
    pub async fn shutdown(mut self) -> Result<(), Error> {
        self.client.shared.close(Error::Closed);
        self.supervisor
            .take()
            .ok_or(Error::Closed)?
            .await
            .map_err(|_| Error::Worker)?
    }
}

impl Drop for CodexProcess {
    fn drop(&mut self) {
        // The supervisor retains ownership until the child is reaped.
        self.client.shared.close(Error::Closed);
    }
}

async fn stop_child(child: &mut Child) -> Result<(), Error> {
    if let Ok(status) = tokio::time::timeout(STOP_TIMEOUT, child.wait()).await {
        let status = status?;
        if status.success() {
            Ok(())
        } else {
            Err(Error::ProcessExit(status.code()))
        }
    } else {
        child.start_kill()?;
        tokio::time::timeout(STOP_TIMEOUT, child.wait())
            .await
            .map_err(|_| Error::StopTimeout)??;
        Err(Error::StopTimeout)
    }
}

fn connect<R, W>(
    reader: R,
    writer: W,
    handler: Option<RequestHandler>,
) -> (CodexClient, JoinHandle<()>, JoinHandle<()>)
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (notifications, _) = broadcast::channel(1000);
    let shared = Arc::new(Shared {
        state: Mutex::new(State {
            next_id: 1,
            pending: HashMap::new(),
            closed: None,
            notifications: Some(notifications),
            handler,
        }),
        stop: CancellationToken::new(),
    });
    let (outgoing, incoming) = mpsc::channel(64);
    let client = CodexClient { shared, outgoing };
    let read_client = client.clone();
    let reader = tokio::spawn(async move {
        if let Err(error) = read_messages(reader, &read_client).await {
            read_client.shared.close(error);
        }
    });
    let write_shared = client.shared.clone();
    let writer = tokio::spawn(async move {
        if let Err(error) = write_messages(writer, incoming, &write_shared).await {
            write_shared.close(error);
        }
    });
    (client, reader, writer)
}

async fn write_messages<W: AsyncWrite + Unpin>(
    mut writer: W,
    mut incoming: mpsc::Receiver<Frame>,
    shared: &Shared,
) -> Result<(), Error> {
    while let Some(frame) = incoming.recv().await {
        {
            let state = shared.state.lock().expect("transport state lock poisoned");
            if let Some(error) = &state.closed {
                return Err(error.clone());
            }
            if frame
                .request_id
                .is_some_and(|id| !state.pending.contains_key(&id))
            {
                continue;
            }
        }
        let mut bytes = serde_json::to_vec(&frame.value).map_err(|_| Error::InvalidFrame)?;
        if bytes.len() > MAX_FRAME_BYTES {
            let _ = frame.written.send(Err(Error::FrameTooLarge));
            continue;
        }
        bytes.push(b'\n');
        let result = tokio::time::timeout(WRITE_TIMEOUT, async {
            writer.write_all(&bytes).await?;
            writer.flush().await
        })
        .await
        .map_err(|_| Error::Timeout)?
        .map_err(Error::from);
        let _ = frame.written.send(result.clone());
        result?;
    }
    Ok(())
}

async fn read_messages<R: AsyncRead + Unpin>(reader: R, client: &CodexClient) -> Result<(), Error> {
    let mut reader = BufReader::new(reader);
    let mut handlers = JoinSet::new();
    loop {
        let bytes = read_frame(&mut reader).await?;
        // Match the original tolerance of non-JSON stdout, without logging its contents.
        let Ok(message) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        if !message.is_object() {
            return Err(Error::InvalidFrame);
        }
        while handlers.try_join_next().is_some() {}
        if message.get("id").is_some_and(|id| !id.is_null()) && message.get("method").is_some() {
            if handlers.len() >= MAX_APPROVALS {
                return Err(Error::Capacity);
            }
            let id = message["id"].clone();
            if !(id.is_string() || id.is_i64()) {
                return Err(Error::InvalidFrame);
            }
            let handler = client
                .shared
                .state
                .lock()
                .expect("transport state lock poisoned")
                .handler
                .clone();
            let client = client.clone();
            handlers.spawn(async move {
                let result = match handler {
                    Some(handler) => std::panic::AssertUnwindSafe(async { handler(message).await })
                        .catch_unwind()
                        .await
                        .ok()
                        .and_then(Result::ok)
                        .unwrap_or_else(|| json!({"decision": "decline"})),
                    None => json!({"decision": "decline"}),
                };
                if let Err(error) = client.send(json!({"id": id, "result": result}), None).await {
                    client.shared.close(error);
                }
            });
        } else if let Some(id) = message.get("id").and_then(Value::as_u64) {
            let reply = client
                .shared
                .state
                .lock()
                .expect("transport state lock poisoned")
                .pending
                .remove(&id);
            if let Some(reply) = reply {
                let result = match message.get("error") {
                    Some(error) => Err(Error::Protocol {
                        code: error.get("code").and_then(Value::as_i64).unwrap_or(-32000),
                        message: error
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("Unknown error")
                            .to_owned(),
                        data: error.get("data").cloned().unwrap_or(Value::Null),
                    }),
                    None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
                };
                let _ = reply.send(result);
            }
        } else if message.get("id").is_none_or(Value::is_null) {
            let state = client
                .shared
                .state
                .lock()
                .expect("transport state lock poisoned");
            if let Some(sender) = &state.notifications {
                let _ = sender.send(message);
            }
        }
    }
}

async fn read_frame<R: AsyncRead + Unpin>(reader: &mut BufReader<R>) -> Result<Vec<u8>, Error> {
    let mut result = Vec::new();
    loop {
        let bytes = reader.fill_buf().await?;
        if bytes.is_empty() {
            return Err(Error::Closed);
        }
        let newline = bytes.iter().position(|byte| *byte == b'\n');
        let count = newline.map_or(bytes.len(), |index| index + 1);
        if result.len() + count > MAX_FRAME_BYTES + 1 {
            return Err(Error::FrameTooLarge);
        }
        result.extend_from_slice(&bytes[..count]);
        reader.consume(count);
        if newline.is_some() {
            return Ok(result);
        }
    }
}

#[cfg(test)]
mod tests;
