use std::net::IpAddr;
use std::sync::Arc;

use anyhow::Context;
use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::extract::WebSocketUpgrade;
use axum::extract::ws::Message;
use axum::extract::ws::WebSocket;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::http::header::HOST;
use axum::http::header::ORIGIN;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::get;
use futures_util::SinkExt;
use futures_util::StreamExt;
use qwenpaw_core::Core;
use qwenpaw_core::CoreError;
use qwenpaw_core::TurnEventStream;
use qwenpaw_protocol::ClientMessage;
use qwenpaw_protocol::ConfigReadParams;
use qwenpaw_protocol::ConfigWriteParams;
use qwenpaw_protocol::InitializeParams;
use qwenpaw_protocol::InitializeResponse;
use qwenpaw_protocol::PROTOCOL_VERSION;
use qwenpaw_protocol::ServerInfo;
use qwenpaw_protocol::ServerNotification;
use qwenpaw_protocol::ServerResponse;
use qwenpaw_protocol::ThreadArchiveParams;
use qwenpaw_protocol::ThreadListParams;
use qwenpaw_protocol::ThreadReadParams;
use qwenpaw_protocol::ThreadResumeParams;
use qwenpaw_protocol::ThreadStartParams;
use qwenpaw_protocol::ThreadStartedNotification;
use qwenpaw_protocol::ToolApprovalRespondParams;
use qwenpaw_protocol::TurnInterruptParams;
use qwenpaw_protocol::TurnStartParams;
use qwenpaw_protocol::WorkspaceListParams;
use qwenpaw_protocol::WorkspaceReadParams;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::sync::mpsc;
use tracing::warn;

const OUTBOUND_CHANNEL_CAPACITY: usize = 128;
const MAX_WEBSOCKET_MESSAGE_BYTES: usize = 1_048_576;

#[derive(Clone)]
pub struct AppServer {
    inner: Arc<AppServerInner>,
}

struct AppServerInner {
    core: Core,
    allowed_origins: Vec<String>,
}

#[derive(Default)]
struct ConnectionSession {
    initialized: bool,
}

impl AppServer {
    #[must_use]
    pub fn new(core: Core) -> Self {
        Self {
            inner: Arc::new(AppServerInner {
                core,
                allowed_origins: allowed_origins_from_env(),
            }),
        }
    }

    /// Runs the app server over newline-delimited JSON on stdin and stdout.
    ///
    /// # Errors
    ///
    /// Returns an error when stdin or stdout fails, or when the writer task
    /// cannot complete cleanly.
    pub async fn run_stdio(self) -> anyhow::Result<()> {
        let (outbound_tx, mut outbound_rx) = mpsc::channel::<String>(OUTBOUND_CHANNEL_CAPACITY);
        let writer = tokio::spawn(async move {
            let mut stdout = tokio::io::stdout();
            while let Some(message) = outbound_rx.recv().await {
                stdout
                    .write_all(message.as_bytes())
                    .await
                    .context("failed to write app-server message")?;
                stdout
                    .write_all(b"\n")
                    .await
                    .context("failed to terminate app-server message")?;
                stdout
                    .flush()
                    .await
                    .context("failed to flush app-server message")?;
            }
            Ok::<(), anyhow::Error>(())
        });

        let stdin = tokio::io::stdin();
        let mut lines = BufReader::new(stdin).lines();
        let mut session = ConnectionSession::default();
        while let Some(line) = lines
            .next_line()
            .await
            .context("failed to read app-server input")?
        {
            if line.trim().is_empty() {
                continue;
            }
            self.process_line(&mut session, &line, &outbound_tx).await;
        }
        drop(outbound_tx);
        writer.await.context("app-server writer task failed")??;
        Ok(())
    }

    /// Runs the App Protocol and health endpoints on an existing TCP listener.
    ///
    /// Each WebSocket connection has an independent initialization lifecycle
    /// while sharing the same Core runtime and persistent storage.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP server cannot accept or serve traffic.
    pub async fn run_http(self, listener: tokio::net::TcpListener) -> anyhow::Result<()> {
        let address = listener
            .local_addr()
            .context("HTTP listener address is unavailable")?;
        anyhow::ensure!(
            address.ip().is_loopback(),
            "HTTP App Protocol requires a loopback listener"
        );
        axum::serve(listener, self.router())
            .await
            .context("QwenPaw HTTP app server failed")
    }

    fn router(self) -> Router {
        Router::new()
            .route("/healthz", get(healthz))
            .route("/readyz", get(readyz))
            .route("/app-protocol", get(websocket_upgrade))
            .with_state(self)
    }

    async fn process_line(
        &self,
        session: &mut ConnectionSession,
        line: &str,
        outbound_tx: &mpsc::Sender<String>,
    ) {
        let message = match serde_json::from_str::<ClientMessage>(line) {
            Ok(message) => message,
            Err(error) => {
                send_response(
                    outbound_tx,
                    ServerResponse::error(Value::Null, -32700, format!("invalid JSON: {error}")),
                )
                .await;
                return;
            }
        };

        let Some(id) = message.id else {
            if message.method != "initialized" {
                warn!(
                    method = message.method,
                    "ignoring unknown client notification"
                );
            }
            return;
        };
        if message.method != "initialize" && !session.initialized {
            send_response(
                outbound_tx,
                ServerResponse::error(id, -32000, "server is not initialized"),
            )
            .await;
            return;
        }
        if message.method == "initialize" && session.initialized {
            send_response(
                outbound_tx,
                ServerResponse::error(id, -32000, "server is already initialized"),
            )
            .await;
            return;
        }

        match self.dispatch(&message.method, message.params).await {
            Ok(output) => {
                if message.method == "initialize" {
                    session.initialized = true;
                }
                send_response(outbound_tx, ServerResponse::success(id, output.result)).await;
                if let Some(post_response) = output.post_response {
                    dispatch_post_response(outbound_tx, post_response).await;
                }
            }
            Err(error) => {
                send_response(
                    outbound_tx,
                    ServerResponse::error(id, error.code, error.message),
                )
                .await;
            }
        }
    }

    async fn dispatch(&self, method: &str, params: Value) -> Result<DispatchOutput, DispatchError> {
        match method {
            "initialize" => {
                let _: InitializeParams = decode_params(params)?;
                DispatchOutput::result(InitializeResponse {
                    protocol_version: PROTOCOL_VERSION,
                    server_info: ServerInfo {
                        name: String::from("qwenpaw-core"),
                        version: String::from(env!("CARGO_PKG_VERSION")),
                    },
                })
            }
            "thread/start" => {
                let response = self
                    .inner
                    .core
                    .start_thread(decode_params::<ThreadStartParams>(params)?)
                    .await
                    .map_err(|error| DispatchError::core(&error))?;
                let notification = ServerNotification {
                    method: "thread/started",
                    params: serde_json::to_value(ThreadStartedNotification {
                        thread: response.thread.clone(),
                    })
                    .map_err(|error| DispatchError::internal(&error))?,
                };
                DispatchOutput::with_post_response(
                    response,
                    PostResponse::Notification(notification),
                )
            }
            "thread/list" => {
                let params = if params.is_null() {
                    ThreadListParams::default()
                } else {
                    decode_params(params)?
                };
                DispatchOutput::result(self.inner.core.list_threads(params).await)
            }
            "thread/resume" => {
                let params: ThreadResumeParams = decode_params(params)?;
                DispatchOutput::result(
                    self.inner
                        .core
                        .resume_thread(&params)
                        .await
                        .map_err(|error| DispatchError::core(&error))?,
                )
            }
            "thread/archive" => {
                let params: ThreadArchiveParams = decode_params(params)?;
                DispatchOutput::result(
                    self.inner
                        .core
                        .archive_thread(&params)
                        .await
                        .map_err(|error| DispatchError::core(&error))?,
                )
            }
            "thread/read" => {
                let params: ThreadReadParams = decode_params(params)?;
                DispatchOutput::result(
                    self.inner
                        .core
                        .read_thread(&params.thread_id)
                        .await
                        .map_err(|error| DispatchError::core(&error))?,
                )
            }
            "turn/start" => {
                let params: TurnStartParams = decode_params(params)?;
                let (response, events) = self
                    .inner
                    .core
                    .start_turn(params)
                    .await
                    .map_err(|error| DispatchError::core(&error))?;
                DispatchOutput::with_post_response(response, PostResponse::TurnEvents(events))
            }
            "turn/interrupt" => {
                let params: TurnInterruptParams = decode_params(params)?;
                DispatchOutput::result(
                    self.inner
                        .core
                        .interrupt_turn(&params)
                        .await
                        .map_err(|error| DispatchError::core(&error))?,
                )
            }
            "tool/approval/respond" => {
                let params: ToolApprovalRespondParams = decode_params(params)?;
                DispatchOutput::result(self.inner.core.respond_tool_approval(params).await)
            }
            "model/list" => DispatchOutput::result(self.inner.core.list_models()),
            "config/read" | "config/write" | "workspace/list" | "workspace/read" => {
                self.dispatch_resource(method, params).await
            }
            _ => Err(DispatchError::method_not_found(method)),
        }
    }

    async fn dispatch_resource(
        &self,
        method: &str,
        params: Value,
    ) -> Result<DispatchOutput, DispatchError> {
        match method {
            "config/read" => {
                let _: ConfigReadParams = decode_default_params(params)?;
                DispatchOutput::result(self.inner.core.read_config())
            }
            "config/write" => {
                let params: ConfigWriteParams = decode_params(params)?;
                DispatchOutput::result(
                    self.inner
                        .core
                        .write_config(params)
                        .map_err(|error| DispatchError::core(&error))?,
                )
            }
            "workspace/list" => {
                let _: WorkspaceListParams = decode_default_params(params)?;
                DispatchOutput::result(self.inner.core.list_workspaces().await)
            }
            "workspace/read" => {
                let params: WorkspaceReadParams = decode_params(params)?;
                DispatchOutput::result(
                    self.inner
                        .core
                        .read_workspace(&params.root)
                        .await
                        .map_err(|error| DispatchError::core(&error))?,
                )
            }
            _ => unreachable!("resource dispatch receives only resource methods"),
        }
    }
}

async fn healthz() -> Json<Value> {
    Json(serde_json::json!({"status": "ok"}))
}

async fn readyz() -> Json<Value> {
    Json(serde_json::json!({"status": "ready"}))
}

async fn websocket_upgrade(
    State(server): State<AppServer>,
    headers: HeaderMap,
    websocket: WebSocketUpgrade,
) -> Response {
    if !server.origin_allowed(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    websocket
        .max_message_size(MAX_WEBSOCKET_MESSAGE_BYTES)
        .on_upgrade(move |socket| server.serve_websocket(socket))
}

impl AppServer {
    fn origin_allowed(&self, headers: &HeaderMap) -> bool {
        let Some(origin) = headers.get(ORIGIN) else {
            return true;
        };
        let Ok(origin) = origin.to_str() else {
            return false;
        };
        let normalized = normalize_origin(origin);
        if self
            .inner
            .allowed_origins
            .iter()
            .any(|allowed| allowed == &normalized)
        {
            return true;
        }
        let Some(host) = headers.get(HOST).and_then(|value| value.to_str().ok()) else {
            return false;
        };
        if !host_is_loopback(host) {
            return false;
        }
        normalized == format!("http://{host}") || normalized == format!("https://{host}")
    }

    async fn serve_websocket(self, socket: WebSocket) {
        let (mut sender, mut receiver) = socket.split();
        let (outbound_tx, mut outbound_rx) = mpsc::channel::<String>(OUTBOUND_CHANNEL_CAPACITY);
        let writer = tokio::spawn(async move {
            while let Some(payload) = outbound_rx.recv().await {
                if sender.send(Message::Text(payload.into())).await.is_err() {
                    break;
                }
            }
        });
        let mut session = ConnectionSession::default();
        while let Some(message) = receiver.next().await {
            match message {
                Ok(Message::Text(line)) => {
                    self.process_line(&mut session, &line, &outbound_tx).await;
                }
                Ok(Message::Close(_)) | Err(_) => break,
                Ok(Message::Binary(_)) => {
                    send_response(
                        &outbound_tx,
                        ServerResponse::error(Value::Null, -32700, "expected a text message"),
                    )
                    .await;
                }
                Ok(Message::Ping(_) | Message::Pong(_)) => {}
            }
        }
        drop(outbound_tx);
        let _ = writer.await;
    }
}

fn allowed_origins_from_env() -> Vec<String> {
    std::env::var("QWENPAW_ALLOWED_ORIGINS")
        .ok()
        .into_iter()
        .flat_map(|value| {
            value
                .split(',')
                .map(normalize_origin)
                .filter(|origin| !origin.is_empty())
                .collect::<Vec<_>>()
        })
        .collect()
}

fn normalize_origin(origin: &str) -> String {
    origin.trim().trim_end_matches('/').to_owned()
}

fn host_is_loopback(host: &str) -> bool {
    let hostname = if let Some(ipv6) = host.strip_prefix('[') {
        let Some((address, _)) = ipv6.split_once(']') else {
            return false;
        };
        address
    } else if let Some((hostname, port)) = host.rsplit_once(':') {
        if port.parse::<u16>().is_ok() {
            hostname
        } else {
            host
        }
    } else {
        host
    };
    hostname.eq_ignore_ascii_case("localhost")
        || hostname
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

async fn send_response(outbound_tx: &mpsc::Sender<String>, response: ServerResponse) {
    send_serialized(outbound_tx, &response).await;
}

async fn send_notification(outbound_tx: &mpsc::Sender<String>, notification: ServerNotification) {
    send_serialized(outbound_tx, &notification).await;
}

async fn send_serialized<T: serde::Serialize>(outbound_tx: &mpsc::Sender<String>, message: &T) {
    match serde_json::to_string(message) {
        Ok(message) => {
            if outbound_tx.send(message).await.is_err() {
                warn!("app-server output receiver disconnected");
            }
        }
        Err(error) => warn!(%error, "failed to serialize app-server message"),
    }
}

async fn dispatch_post_response(outbound_tx: &mpsc::Sender<String>, post_response: PostResponse) {
    match post_response {
        PostResponse::Notification(notification) => {
            send_notification(outbound_tx, notification).await;
        }
        PostResponse::TurnEvents(mut events) => {
            let event_tx = outbound_tx.clone();
            tokio::spawn(async move {
                while let Some(event) = events.recv().await {
                    match event.into_notification() {
                        Ok(notification) => {
                            send_notification(&event_tx, notification).await;
                        }
                        Err(error) => {
                            warn!(%error, "failed to serialize core event");
                        }
                    }
                }
            });
        }
    }
}

fn decode_params<T: DeserializeOwned>(params: Value) -> Result<T, DispatchError> {
    serde_json::from_value(params).map_err(|error| DispatchError {
        code: -32602,
        message: format!("invalid params: {error}"),
    })
}

fn decode_default_params<T: Default + DeserializeOwned>(params: Value) -> Result<T, DispatchError> {
    if params.is_null() {
        Ok(T::default())
    } else {
        decode_params(params)
    }
}

struct DispatchOutput {
    result: Value,
    post_response: Option<PostResponse>,
}

impl DispatchOutput {
    fn result<T: serde::Serialize>(result: T) -> Result<Self, DispatchError> {
        Ok(Self {
            result: serde_json::to_value(result)
                .map_err(|error| DispatchError::internal(&error))?,
            post_response: None,
        })
    }

    fn with_post_response<T: serde::Serialize>(
        result: T,
        post_response: PostResponse,
    ) -> Result<Self, DispatchError> {
        Ok(Self {
            result: serde_json::to_value(result)
                .map_err(|error| DispatchError::internal(&error))?,
            post_response: Some(post_response),
        })
    }
}

enum PostResponse {
    Notification(ServerNotification),
    TurnEvents(TurnEventStream),
}

struct DispatchError {
    code: i32,
    message: String,
}

impl DispatchError {
    fn method_not_found(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("method not found: {method}"),
        }
    }

    fn state(message: impl Into<String>) -> Self {
        Self {
            code: -32000,
            message: message.into(),
        }
    }

    fn core(error: &CoreError) -> Self {
        Self::state(error.to_string())
    }

    fn internal(error: &serde_json::Error) -> Self {
        Self {
            code: -32603,
            message: format!("internal serialization error: {error}"),
        }
    }
}

#[cfg(test)]
#[path = "app_server_tests.rs"]
mod tests;
