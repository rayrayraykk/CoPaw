//! Typed client for the `QwenPaw` App Server protocol.
//!
//! This crate owns client-side transport lifecycle and request correlation. It
//! intentionally contains no agent, tool, storage, or presentation logic.

use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use qwenpaw_protocol::ClientInfo;
use qwenpaw_protocol::InitializeParams;
use qwenpaw_protocol::InitializeResponse;
use qwenpaw_protocol::PROTOCOL_VERSION;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use serde_json::json;
use thiserror::Error;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::process::Child;
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

const COMMAND_CAPACITY: usize = 64;
const NOTIFICATION_CAPACITY: usize = 256;
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// One untyped server notification received from App Protocol.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerNotification {
    pub method: String,
    pub params: Value,
}

/// Identity sent during the App Protocol initialize handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientIdentity {
    pub name: String,
    pub version: String,
    pub title: Option<String>,
}

impl ClientIdentity {
    /// Creates a client identity with no display title.
    #[must_use]
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            title: None,
        }
    }

    /// Adds a human-readable client title.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

/// Client-side App Protocol and transport errors.
#[derive(Debug, Error)]
pub enum ClientError {
    #[error("app-server transport is closed")]
    TransportClosed,
    #[error("app-server request timed out")]
    RequestTimeout,
    #[error("app-server returned protocol error {code}: {message}")]
    Protocol { code: i32, message: String },
    #[error("app-server returned invalid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("app-server I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("app-server protocol version {actual} does not match SDK version {expected}")]
    ProtocolVersion { expected: u32, actual: u32 },
}

enum ClientCommand {
    Request {
        method: String,
        params: Value,
        response: oneshot::Sender<Result<Value, ClientError>>,
    },
    Notify {
        method: String,
        params: Value,
    },
    Shutdown,
}

/// A connected App Protocol client over an arbitrary asynchronous byte stream.
#[derive(Clone)]
pub struct AppServerClient {
    commands: mpsc::Sender<ClientCommand>,
    notifications: broadcast::Sender<ServerNotification>,
    request_timeout: Duration,
    worker: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl AppServerClient {
    /// Connects a client to newline-delimited App Protocol input and output.
    pub fn connect<R, W>(reader: R, writer: W) -> Self
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let (command_tx, command_rx) = mpsc::channel(COMMAND_CAPACITY);
        let (notification_tx, _) = broadcast::channel(NOTIFICATION_CAPACITY);
        let worker_notifications = notification_tx.clone();
        let worker = tokio::spawn(run_transport(
            reader,
            writer,
            command_rx,
            worker_notifications,
        ));
        Self {
            commands: command_tx,
            notifications: notification_tx,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            worker: Arc::new(Mutex::new(Some(worker))),
        }
    }

    /// Changes the timeout used by subsequent requests.
    #[must_use]
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// Completes the required initialize handshake and version check.
    ///
    /// # Errors
    ///
    /// Returns a transport, protocol, decoding, timeout, or version error.
    pub async fn initialize(
        &self,
        identity: ClientIdentity,
    ) -> Result<InitializeResponse, ClientError> {
        let response = self
            .request::<_, InitializeResponse>(
                "initialize",
                InitializeParams {
                    client_info: ClientInfo {
                        name: identity.name,
                        version: identity.version,
                        title: identity.title,
                    },
                },
            )
            .await?;
        if response.protocol_version != PROTOCOL_VERSION {
            return Err(ClientError::ProtocolVersion {
                expected: PROTOCOL_VERSION,
                actual: response.protocol_version,
            });
        }
        self.notify("initialized", json!({})).await?;
        Ok(response)
    }

    /// Sends a typed App Protocol request and decodes its typed result.
    ///
    /// # Errors
    ///
    /// Returns a transport, protocol, serialization, decoding, or timeout
    /// error.
    pub async fn request<P, R>(&self, method: &str, params: P) -> Result<R, ClientError>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        let params = serde_json::to_value(params)?;
        let (response_tx, response_rx) = oneshot::channel();
        self.commands
            .send(ClientCommand::Request {
                method: method.to_owned(),
                params,
                response: response_tx,
            })
            .await
            .map_err(|_| ClientError::TransportClosed)?;
        let response = tokio::time::timeout(self.request_timeout, response_rx)
            .await
            .map_err(|_| ClientError::RequestTimeout)?
            .map_err(|_| ClientError::TransportClosed)??;
        Ok(serde_json::from_value(response)?)
    }

    /// Sends a client notification without waiting for a response.
    ///
    /// # Errors
    ///
    /// Returns an error when serialization fails or the transport is closed.
    pub async fn notify<P>(&self, method: &str, params: P) -> Result<(), ClientError>
    where
        P: Serialize,
    {
        self.commands
            .send(ClientCommand::Notify {
                method: method.to_owned(),
                params: serde_json::to_value(params)?,
            })
            .await
            .map_err(|_| ClientError::TransportClosed)
    }

    /// Subscribes to raw server notifications in wire order.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<ServerNotification> {
        self.notifications.subscribe()
    }

    /// Stops the transport worker and waits for it to finish.
    pub async fn shutdown(&self) {
        let _ = self.commands.send(ClientCommand::Shutdown).await;
        if let Some(worker) = self.worker.lock().await.take() {
            let _ = worker.await;
        }
    }
}

/// A child `qwenpaw-core app-server` process and its connected client.
pub struct StdioAppServer {
    client: AppServerClient,
    child: Child,
}

impl StdioAppServer {
    /// Starts a Core executable with the stdio App Protocol transport.
    ///
    /// # Errors
    ///
    /// Returns an error when the process cannot start or does not expose its
    /// stdin/stdout pipes.
    pub fn spawn(executable: &Path) -> Result<Self, ClientError> {
        let mut command = Command::new(executable);
        command
            .args(["app-server", "--stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        Self::spawn_command(&mut command)
    }

    /// Starts a caller-configured Core command over piped stdin/stdout.
    ///
    /// The caller owns arguments, environment, working directory, and stderr.
    /// The SDK always replaces stdin/stdout with pipes.
    ///
    /// # Errors
    ///
    /// Returns an error when the process cannot start or does not expose its
    /// stdin/stdout pipes.
    pub fn spawn_command(command: &mut Command) -> Result<Self, ClientError> {
        command.stdin(Stdio::piped()).stdout(Stdio::piped());
        let mut child = command.spawn()?;
        let stdout = child.stdout.take().ok_or(ClientError::TransportClosed)?;
        let stdin = child.stdin.take().ok_or(ClientError::TransportClosed)?;
        Ok(Self {
            client: AppServerClient::connect(stdout, stdin),
            child,
        })
    }

    /// Returns the connected protocol client.
    #[must_use]
    pub const fn client(&self) -> &AppServerClient {
        &self.client
    }

    /// Gracefully closes the client transport and terminates the child.
    ///
    /// # Errors
    ///
    /// Returns an error when process termination or waiting fails.
    pub async fn shutdown(mut self) -> Result<(), ClientError> {
        self.client.shutdown().await;
        if self.child.try_wait()?.is_none() {
            self.child.kill().await?;
        }
        let _ = self.child.wait().await?;
        Ok(())
    }
}

impl Drop for StdioAppServer {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

async fn run_transport<R, W>(
    reader: R,
    mut writer: W,
    mut commands: mpsc::Receiver<ClientCommand>,
    notifications: broadcast::Sender<ServerNotification>,
) where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    let mut next_id = 1_u64;
    let mut pending = HashMap::<u64, oneshot::Sender<Result<Value, ClientError>>>::new();

    loop {
        tokio::select! {
            command = commands.recv() => {
                pending.retain(|_, sender| !sender.is_closed());
                match command {
                    Some(ClientCommand::Request { method, params, response }) => {
                        let id = next_id;
                        next_id = next_id.saturating_add(1);
                        let message = json!({"id": id, "method": method, "params": params});
                        if write_message(&mut writer, &message).await.is_err() {
                            let _ = response.send(Err(ClientError::TransportClosed));
                            break;
                        }
                        pending.insert(id, response);
                    }
                    Some(ClientCommand::Notify { method, params }) => {
                        let message = json!({"method": method, "params": params});
                        if write_message(&mut writer, &message).await.is_err() {
                            break;
                        }
                    }
                    Some(ClientCommand::Shutdown) | None => break,
                }
            }
            line = lines.next_line() => {
                match line {
                    Ok(Some(line)) => handle_server_line(&line, &mut pending, &notifications),
                    Ok(None) | Err(_) => break,
                }
            }
        }
    }

    for (_, response) in pending {
        let _ = response.send(Err(ClientError::TransportClosed));
    }
}

async fn write_message<W>(writer: &mut W, message: &Value) -> Result<(), ClientError>
where
    W: AsyncWrite + Unpin,
{
    let mut encoded = serde_json::to_vec(message)?;
    encoded.push(b'\n');
    writer.write_all(&encoded).await?;
    writer.flush().await?;
    Ok(())
}

fn handle_server_line(
    line: &str,
    pending: &mut HashMap<u64, oneshot::Sender<Result<Value, ClientError>>>,
    notifications: &broadcast::Sender<ServerNotification>,
) {
    let Ok(message) = serde_json::from_str::<Value>(line) else {
        return;
    };
    if let Some(id) = message.get("id").and_then(Value::as_u64) {
        let Some(response) = pending.remove(&id) else {
            return;
        };
        let result = match message.get("error") {
            Some(error) => Err(ClientError::Protocol {
                code: error
                    .get("code")
                    .and_then(Value::as_i64)
                    .and_then(|code| i32::try_from(code).ok())
                    .unwrap_or(-32_000),
                message: error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown App Protocol error")
                    .to_owned(),
            }),
            None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
        };
        let _ = response.send(result);
        return;
    }
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        return;
    };
    let _ = notifications.send(ServerNotification {
        method: method.to_owned(),
        params: message.get("params").cloned().unwrap_or(Value::Null),
    });
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use qwenpaw_protocol::REQUEST_METHODS;
    use qwenpaw_protocol::SERVER_NOTIFICATIONS;
    use qwenpaw_protocol::ServerInfo;
    use tokio::io::AsyncBufReadExt;
    use tokio::io::AsyncWriteExt;
    use tokio::io::BufReader;

    use super::*;

    #[test]
    fn matches_shared_protocol_fixture_inventory() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../docs/api-contract/fixtures/app-protocol-v3.json"
        ))
        .expect("shared fixture should be valid JSON");
        let mut fixture_requests = fixture["requests"]
            .as_object()
            .expect("fixture requests should be an object")
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let mut rust_requests = REQUEST_METHODS
            .iter()
            .map(|(method, _, _)| *method)
            .collect::<Vec<_>>();
        fixture_requests.sort_unstable();
        rust_requests.sort_unstable();
        assert_eq!(fixture_requests, rust_requests);

        let mut fixture_notifications = fixture["serverNotifications"]
            .as_object()
            .expect("fixture notifications should be an object")
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let mut rust_notifications = SERVER_NOTIFICATIONS
            .iter()
            .map(|(method, _)| *method)
            .collect::<Vec<_>>();
        fixture_notifications.sort_unstable();
        rust_notifications.sort_unstable();
        assert_eq!(fixture_notifications, rust_notifications);
    }

    #[tokio::test]
    async fn initializes_and_delivers_notifications_over_json_lines() {
        let (client_stream, server_stream) = tokio::io::duplex(8_192);
        let (client_reader, client_writer) = tokio::io::split(client_stream);
        let (server_reader, mut server_writer) = tokio::io::split(server_stream);
        let server = tokio::spawn(async move {
            let mut lines = BufReader::new(server_reader).lines();
            let initialize = lines
                .next_line()
                .await
                .expect("initialize read should succeed")
                .expect("initialize should be present");
            let request: Value =
                serde_json::from_str(&initialize).expect("initialize should be JSON");
            assert_eq!(request["method"], "initialize");
            server_writer
                .write_all(
                    format!(
                        "{{\"id\":1,\"result\":{{\"protocolVersion\":{PROTOCOL_VERSION},\"serverInfo\":{{\"name\":\"qwenpaw-core\",\"version\":\"0.2.0\"}}}}}}\n"
                    )
                    .as_bytes(),
                )
                .await
                .expect("initialize response should be written");
            let initialized = lines
                .next_line()
                .await
                .expect("initialized read should succeed")
                .expect("initialized should be present");
            let notification: Value =
                serde_json::from_str(&initialized).expect("initialized should be JSON");
            assert_eq!(notification["method"], "initialized");
            server_writer
                .write_all(b"{\"method\":\"turn/started\",\"params\":{\"value\":1}}\n")
                .await
                .expect("notification should be written");
        });

        let client = AppServerClient::connect(client_reader, client_writer);
        let mut notifications = client.subscribe();
        let response = client
            .initialize(ClientIdentity::new("test-client", "1.0.0"))
            .await
            .expect("initialize should succeed");
        assert_eq!(
            response,
            InitializeResponse {
                protocol_version: PROTOCOL_VERSION,
                server_info: ServerInfo {
                    name: String::from("qwenpaw-core"),
                    version: String::from("0.2.0"),
                },
            }
        );
        assert_eq!(
            notifications
                .recv()
                .await
                .expect("notification should be delivered"),
            ServerNotification {
                method: String::from("turn/started"),
                params: json!({"value": 1}),
            }
        );
        client.shutdown().await;
        server.await.expect("server task should finish");
    }

    #[tokio::test]
    async fn returns_typed_protocol_errors() {
        let (client_stream, server_stream) = tokio::io::duplex(4_096);
        let (client_reader, client_writer) = tokio::io::split(client_stream);
        let (server_reader, mut server_writer) = tokio::io::split(server_stream);
        let server = tokio::spawn(async move {
            let mut lines = BufReader::new(server_reader).lines();
            let _ = lines.next_line().await;
            server_writer
                .write_all(b"{\"id\":1,\"error\":{\"code\":-32601,\"message\":\"missing\"}}\n")
                .await
                .expect("error response should be written");
        });
        let client = AppServerClient::connect(client_reader, client_writer);
        let error = client
            .request::<_, Value>("missing", json!({}))
            .await
            .expect_err("request should fail");
        assert!(matches!(
            error,
            ClientError::Protocol {
                code: -32601,
                message
            } if message == "missing"
        ));
        client.shutdown().await;
        server.await.expect("server task should finish");
    }
}
