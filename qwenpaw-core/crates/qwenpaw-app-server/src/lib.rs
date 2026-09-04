use std::collections::HashMap;
use std::net::IpAddr;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::extract::WebSocketUpgrade;
use axum::extract::ws::Message;
use axum::extract::ws::WebSocket;
use axum::http::HeaderMap;
use axum::http::HeaderValue;
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use axum::http::header::CACHE_CONTROL;
use axum::http::header::HOST;
use axum::http::header::ORIGIN;
use axum::http::header::WWW_AUTHENTICATE;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::any;
use axum::routing::get;
use axum::routing::post;
use futures_util::SinkExt;
use futures_util::StreamExt;
use qwenpaw_core::Core;
use qwenpaw_core::CoreError;
use qwenpaw_core::McpOAuthStartOptions;
use qwenpaw_core::TurnEventStream;
use qwenpaw_protocol::ClientMessage;
use qwenpaw_protocol::ConfigReadParams;
use qwenpaw_protocol::ConfigWriteParams;
use qwenpaw_protocol::InitializeParams;
use qwenpaw_protocol::InitializeResponse;
use qwenpaw_protocol::McpClientInfo;
use qwenpaw_protocol::McpListParams;
use qwenpaw_protocol::McpListResponse;
use qwenpaw_protocol::McpOAuthRevokeParams;
use qwenpaw_protocol::McpOAuthRevokeResponse;
use qwenpaw_protocol::McpOAuthStartParams;
use qwenpaw_protocol::McpOAuthStartResponse;
use qwenpaw_protocol::McpOAuthStatus;
use qwenpaw_protocol::McpOAuthStatusParams;
use qwenpaw_protocol::McpOAuthStatusResponse;
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
use sha2::Digest as _;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tower_http::services::ServeDir;
use tower_http::services::ServeFile;
use tower_http::set_header::SetResponseHeaderLayer;
use tracing::warn;

mod desktop_access_control;
mod desktop_api;
mod desktop_channels;
mod desktop_chats;
mod desktop_credentials;
mod desktop_cron;
mod desktop_environment;
mod desktop_files;
mod desktop_git;
mod desktop_inbox;
mod desktop_mail_access_control;
mod desktop_navigation;

pub use desktop_credentials::DesktopCredentialStore;
pub use desktop_credentials::SystemDesktopCredentialStore;

const OUTBOUND_CHANNEL_CAPACITY: usize = 128;
const MAX_WEBSOCKET_MESSAGE_BYTES: usize = 1_048_576;
const MIN_REMOTE_TOKEN_BYTES: usize = 32;
const MAX_REMOTE_TOKEN_BYTES: usize = 4096;
const MAX_REMOTE_TLS_PRIVATE_KEY_BYTES: u64 = 1_048_576;
const DESKTOP_SHUTDOWN_TOKEN_HEADER: &str = "x-qwenpaw-desktop-shutdown-token";
const MIN_DESKTOP_SHUTDOWN_TOKEN_BYTES: usize = 16;
const MAX_DESKTOP_SHUTDOWN_TOKEN_BYTES: usize = 4096;
const DESKTOP_DEFAULT_WORKSPACE_ENV: &str = "QWENPAW_DEFAULT_WORKSPACE";

#[derive(Clone)]
pub struct AppServer {
    inner: Arc<AppServerInner>,
}

struct AppServerInner {
    core: Core,
    desktop_session_aliases: tokio::sync::RwLock<DesktopSessionAliases>,
    desktop_pending_approvals: tokio::sync::RwLock<HashMap<String, DesktopPendingApproval>>,
    desktop_push_messages: tokio::sync::RwLock<Vec<DesktopPushMessage>>,
    desktop_access_control_lock: tokio::sync::Mutex<()>,
    desktop_mail_access_control_lock: tokio::sync::Mutex<()>,
    desktop_inbox_lock: tokio::sync::Mutex<()>,
    desktop_channel_config_lock: tokio::sync::Mutex<()>,
    desktop_chat_catalog_lock: tokio::sync::Mutex<()>,
    desktop_cron_lock: tokio::sync::Mutex<()>,
    desktop_environment_lock: tokio::sync::Mutex<()>,
    desktop_git_lock: tokio::sync::Mutex<()>,
    desktop_credentials: Option<Arc<dyn DesktopCredentialStore>>,
    desktop_workspace: Option<DesktopWorkspace>,
    allowed_origins: Vec<String>,
    console_static_dir: Option<PathBuf>,
    desktop_shutdown_token: Option<String>,
    remote_auth_token_file: Option<PathBuf>,
    shutdown: CancellationToken,
}

#[derive(Default)]
struct DesktopSessionAliases {
    client_to_thread: HashMap<String, String>,
    thread_to_client: HashMap<String, String>,
}

#[derive(Clone)]
struct DesktopPendingApproval {
    thread_id: String,
    turn_id: String,
    call_id: String,
    tool_name: String,
    arguments: String,
    workspace_root: String,
    session_id: String,
    created_at: i64,
}

#[derive(Clone)]
struct DesktopPushMessage {
    id: String,
    text: String,
    sticky: bool,
    session_id: String,
    created_at: u64,
}

struct DesktopWorkspace {
    data_dir: PathBuf,
    initial: PathBuf,
    selected: tokio::sync::RwLock<PathBuf>,
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
                desktop_session_aliases: tokio::sync::RwLock::new(DesktopSessionAliases::default()),
                desktop_pending_approvals: tokio::sync::RwLock::new(HashMap::new()),
                desktop_push_messages: tokio::sync::RwLock::new(Vec::new()),
                desktop_access_control_lock: tokio::sync::Mutex::new(()),
                desktop_mail_access_control_lock: tokio::sync::Mutex::new(()),
                desktop_inbox_lock: tokio::sync::Mutex::new(()),
                desktop_channel_config_lock: tokio::sync::Mutex::new(()),
                desktop_chat_catalog_lock: tokio::sync::Mutex::new(()),
                desktop_cron_lock: tokio::sync::Mutex::new(()),
                desktop_environment_lock: tokio::sync::Mutex::new(()),
                desktop_git_lock: tokio::sync::Mutex::new(()),
                desktop_credentials: None,
                desktop_workspace: None,
                allowed_origins: allowed_origins_from_env(),
                console_static_dir: None,
                desktop_shutdown_token: None,
                remote_auth_token_file: None,
                shutdown: CancellationToken::new(),
            }),
        }
    }

    /// Creates a loopback HTTP server suitable for the Tauri Desktop sidecar.
    ///
    /// The Console directory must contain `index.html`. The shutdown token is
    /// kept in memory and is required by the Desktop-only shutdown endpoint.
    ///
    /// # Errors
    ///
    /// Returns an error when the Console directory or shutdown token is
    /// invalid.
    pub fn new_desktop(
        core: Core,
        console_static_dir: &Path,
        desktop_shutdown_token: String,
    ) -> anyhow::Result<Self> {
        Self::new_desktop_with_credential_store(
            core,
            console_static_dir,
            desktop_shutdown_token,
            Arc::new(SystemDesktopCredentialStore),
        )
    }

    /// Creates a Desktop server with an explicit secure credential store.
    ///
    /// This constructor exists so platform credential behavior can be tested
    /// without reading or writing the developer's real system keyring.
    ///
    /// # Errors
    ///
    /// Returns an error when the Console directory or shutdown token is
    /// invalid.
    pub fn new_desktop_with_credential_store(
        core: Core,
        console_static_dir: &Path,
        desktop_shutdown_token: String,
        desktop_credentials: Arc<dyn DesktopCredentialStore>,
    ) -> anyhow::Result<Self> {
        Self::new_desktop_with_options(
            core,
            console_static_dir,
            desktop_shutdown_token,
            desktop_credentials,
            None,
        )
    }

    /// Creates a Desktop server with explicit credential and data stores.
    ///
    /// This is primarily useful for embedders and tests that must isolate all
    /// Desktop-owned files from the process environment.
    ///
    /// # Errors
    ///
    /// Returns an error when the Console directory, data directory, or
    /// shutdown token is invalid.
    pub fn new_desktop_with_credential_store_and_data_dir(
        core: Core,
        console_static_dir: &Path,
        desktop_shutdown_token: String,
        desktop_credentials: Arc<dyn DesktopCredentialStore>,
        desktop_data_dir: &Path,
    ) -> anyhow::Result<Self> {
        std::fs::create_dir_all(desktop_data_dir).with_context(|| {
            format!(
                "failed to create Desktop Rust Core data directory {}",
                desktop_data_dir.display()
            )
        })?;
        let desktop_data_dir = desktop_data_dir.canonicalize().with_context(|| {
            format!(
                "failed to resolve Desktop Rust Core data directory {}",
                desktop_data_dir.display()
            )
        })?;
        Self::new_desktop_with_options(
            core,
            console_static_dir,
            desktop_shutdown_token,
            desktop_credentials,
            Some(desktop_data_dir),
        )
    }

    fn new_desktop_with_options(
        core: Core,
        console_static_dir: &Path,
        desktop_shutdown_token: String,
        desktop_credentials: Arc<dyn DesktopCredentialStore>,
        desktop_data_dir: Option<PathBuf>,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            (MIN_DESKTOP_SHUTDOWN_TOKEN_BYTES..=MAX_DESKTOP_SHUTDOWN_TOKEN_BYTES)
                .contains(&desktop_shutdown_token.len())
                && !desktop_shutdown_token.chars().any(char::is_control),
            "Desktop shutdown token must contain 16 through 4096 bytes without control characters"
        );
        let console_static_dir = console_static_dir.canonicalize().with_context(|| {
            format!(
                "failed to resolve Console static directory {}",
                console_static_dir.display()
            )
        })?;
        anyhow::ensure!(
            console_static_dir.is_dir() && console_static_dir.join("index.html").is_file(),
            "Console static directory must contain index.html: {}",
            console_static_dir.display()
        );
        let desktop_workspace = desktop_workspace_from_env(&core, desktop_data_dir)?;
        desktop_environment::initialize(&core, desktop_credentials.as_ref())?;
        Ok(Self {
            inner: Arc::new(AppServerInner {
                core,
                desktop_session_aliases: tokio::sync::RwLock::new(DesktopSessionAliases::default()),
                desktop_pending_approvals: tokio::sync::RwLock::new(HashMap::new()),
                desktop_push_messages: tokio::sync::RwLock::new(Vec::new()),
                desktop_access_control_lock: tokio::sync::Mutex::new(()),
                desktop_mail_access_control_lock: tokio::sync::Mutex::new(()),
                desktop_inbox_lock: tokio::sync::Mutex::new(()),
                desktop_channel_config_lock: tokio::sync::Mutex::new(()),
                desktop_chat_catalog_lock: tokio::sync::Mutex::new(()),
                desktop_cron_lock: tokio::sync::Mutex::new(()),
                desktop_environment_lock: tokio::sync::Mutex::new(()),
                desktop_git_lock: tokio::sync::Mutex::new(()),
                desktop_credentials: Some(desktop_credentials),
                desktop_workspace: Some(desktop_workspace),
                allowed_origins: allowed_origins_from_env(),
                console_static_dir: Some(console_static_dir),
                desktop_shutdown_token: Some(desktop_shutdown_token),
                remote_auth_token_file: None,
                shutdown: CancellationToken::new(),
            }),
        })
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
        let shutdown = self.inner.shutdown.clone();
        axum::serve(listener, self.router())
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
            .context("QwenPaw HTTP app server failed")
    }

    /// Enables bearer authentication for remote WSS handshakes.
    ///
    /// The token file is re-read for every handshake so operators can rotate
    /// it with an atomic file replacement without restarting Core.
    ///
    /// # Errors
    ///
    /// Returns an error when the file is missing, insecure, or contains an
    /// invalid token, or when Desktop mode is active.
    pub fn with_remote_auth_token_file(mut self, path: &Path) -> anyhow::Result<Self> {
        anyhow::ensure!(
            self.inner.console_static_dir.is_none(),
            "remote WSS cannot serve Desktop compatibility routes"
        );
        let path = validate_remote_token_file(path)?;
        let inner = Arc::get_mut(&mut self.inner)
            .context("remote authentication must be configured before cloning AppServer")?;
        inner.remote_auth_token_file = Some(path);
        Ok(self)
    }

    /// Runs authenticated App Protocol `WebSocket` connections over TLS.
    ///
    /// # Errors
    ///
    /// Returns an error when authentication was not configured, TLS material
    /// is invalid, or the server cannot accept traffic.
    pub async fn run_wss(
        self,
        listener: std::net::TcpListener,
        certificate_path: &Path,
        private_key_path: &Path,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.inner.remote_auth_token_file.is_some(),
            "remote WSS requires bearer authentication"
        );
        let private_key_path = validate_remote_tls_private_key(private_key_path)?;
        let tls = axum_server::tls_rustls::RustlsConfig::from_pem_file(
            certificate_path,
            &private_key_path,
        )
        .await
        .context("remote WSS TLS certificate or private key is invalid")?;
        listener
            .set_nonblocking(true)
            .context("remote WSS listener could not enter nonblocking mode")?;
        let handle = axum_server::Handle::new();
        let shutdown_handle = handle.clone();
        let shutdown = self.inner.shutdown.clone();
        tokio::spawn(async move {
            shutdown.cancelled().await;
            shutdown_handle.graceful_shutdown(Some(Duration::from_secs(10)));
        });
        axum_server::from_tcp_rustls(listener, tls)
            .context("remote WSS listener is invalid")?
            .handle(handle)
            .serve(self.remote_router().into_make_service())
            .await
            .context("QwenPaw remote WSS app server failed")
    }

    fn router(self) -> Router {
        let console_static_dir = self.inner.console_static_dir.clone();
        let mut router = Router::new()
            .route("/healthz", get(healthz))
            .route("/readyz", get(readyz))
            .route("/api/version", get(version))
            .route("/api/healthz", get(healthz))
            .route("/api/desktop/shutdown", post(desktop_shutdown))
            .route("/app-protocol", get(websocket_upgrade));
        if let Some(directory) = console_static_dir {
            let index = directory.join("index.html");
            router = router
                .merge(desktop_access_control::router())
                .merge(desktop_channels::router())
                .merge(desktop_api::router())
                .merge(desktop_cron::router())
                .merge(desktop_environment::router())
                .merge(desktop_files::router())
                .merge(desktop_git::router())
                .merge(desktop_inbox::router())
                .merge(desktop_mail_access_control::router())
                .merge(desktop_navigation::router())
                .route("/api", any(api_not_found))
                .route("/api/{*path}", any(api_not_found))
                .fallback_service(ServeDir::new(directory).fallback(ServeFile::new(index)))
                .layer(SetResponseHeaderLayer::overriding(
                    CACHE_CONTROL,
                    HeaderValue::from_static("no-cache, no-store, must-revalidate"),
                ));
        }
        router.with_state(self)
    }

    fn remote_router(self) -> Router {
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
            "initialize" => initialize_response(params),
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
            "config/read" | "config/write" | "workspace/list" | "workspace/read" | "mcp/list"
            | "mcp/oauth/start" | "mcp/oauth/status" | "mcp/oauth/revoke" => {
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
            "mcp/list" => {
                let _: McpListParams = decode_default_params(params)?;
                let clients = self
                    .inner
                    .core
                    .list_mcp_clients()
                    .await
                    .map_err(|error| DispatchError::core(&error))?;
                DispatchOutput::result(McpListResponse {
                    data: clients.into_iter().map(protocol_mcp_client).collect(),
                })
            }
            "mcp/oauth/start" => {
                let params: McpOAuthStartParams = decode_params(params)?;
                let response = self
                    .inner
                    .core
                    .start_mcp_oauth(
                        &params.server_id,
                        McpOAuthStartOptions {
                            url: params.url.unwrap_or_default(),
                            scope: params.scope.unwrap_or_default(),
                            client_id: params.client_id.unwrap_or_default(),
                            authorization_endpoint: params
                                .authorization_endpoint
                                .unwrap_or_default(),
                            token_endpoint: params.token_endpoint.unwrap_or_default(),
                        },
                    )
                    .await
                    .map_err(|error| DispatchError::core(&error))?;
                DispatchOutput::result(McpOAuthStartResponse {
                    authorization_url: response.authorization_url,
                    session_id: response.session_id,
                })
            }
            "mcp/oauth/status" => {
                let params: McpOAuthStatusParams = decode_params(params)?;
                let status = self
                    .inner
                    .core
                    .mcp_oauth_status(&params.server_id)
                    .await
                    .map_err(|error| DispatchError::core(&error))?;
                DispatchOutput::result(McpOAuthStatusResponse {
                    status: protocol_oauth_status(status),
                })
            }
            "mcp/oauth/revoke" => {
                let params: McpOAuthRevokeParams = decode_params(params)?;
                self.inner
                    .core
                    .revoke_mcp_oauth(&params.server_id)
                    .await
                    .map_err(|error| DispatchError::core(&error))?;
                DispatchOutput::result(McpOAuthRevokeResponse { revoked: true })
            }
            _ => unreachable!("resource dispatch receives only resource methods"),
        }
    }
}

fn initialize_response(params: Value) -> Result<DispatchOutput, DispatchError> {
    let _: InitializeParams = decode_params(params)?;
    DispatchOutput::result(InitializeResponse {
        protocol_version: PROTOCOL_VERSION,
        server_info: ServerInfo {
            name: String::from("qwenpaw-core"),
            version: String::from(env!("CARGO_PKG_VERSION")),
        },
    })
}

fn protocol_mcp_client(client: qwenpaw_core::McpClientInfo) -> McpClientInfo {
    McpClientInfo {
        server_id: client.key,
        name: client.name,
        description: client.description,
        enabled: client.enabled,
        transport: client.transport,
        url: client.url,
        oauth_status: client.oauth_status.map(protocol_oauth_status),
    }
}

fn protocol_oauth_status(status: qwenpaw_core::McpOAuthStatus) -> McpOAuthStatus {
    McpOAuthStatus {
        authorized: status.authorized,
        expires_at: status.expires_at,
        scope: status.scope,
        client_id: status.client_id,
    }
}

fn desktop_workspace_from_env(
    core: &Core,
    desktop_data_dir: Option<PathBuf>,
) -> anyhow::Result<DesktopWorkspace> {
    let configured = std::env::var_os(DESKTOP_DEFAULT_WORKSPACE_ENV)
        .map(PathBuf::from)
        .map_or_else(std::env::current_dir, Ok)?;
    let initial = configured.canonicalize().with_context(|| {
        format!(
            "failed to resolve Desktop default workspace {}",
            configured.display()
        )
    })?;
    anyhow::ensure!(
        initial.is_dir(),
        "Desktop default workspace is not a directory: {}",
        initial.display()
    );
    let selected = core
        .read_preferred_workspace()?
        .map(PathBuf::from)
        .and_then(|path| path.canonicalize().ok())
        .filter(|path| path.is_dir())
        .unwrap_or_else(|| initial.clone());
    let data_dir = desktop_data_dir
        .or_else(|| std::env::var_os("QWENPAW_HOME").map(PathBuf::from))
        .unwrap_or_else(|| initial.join(".qwenpaw-core"));
    Ok(DesktopWorkspace {
        data_dir,
        selected: tokio::sync::RwLock::new(selected),
        initial,
    })
}

async fn healthz() -> Json<Value> {
    Json(serde_json::json!({"status": "ok"}))
}

async fn readyz() -> Json<Value> {
    Json(serde_json::json!({"status": "ready"}))
}

async fn version() -> Json<Value> {
    Json(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "backend": "rust-core",
        "protocolVersion": PROTOCOL_VERSION,
    }))
}

async fn desktop_shutdown(State(server): State<AppServer>, headers: HeaderMap) -> Response {
    let Some(expected) = server.inner.desktop_shutdown_token.as_deref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(provided) = headers
        .get(DESKTOP_SHUTDOWN_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if provided != expected {
        return StatusCode::NOT_FOUND.into_response();
    }
    server.inner.shutdown.cancel();
    Json(serde_json::json!({"ok": true})).into_response()
}

async fn api_not_found() -> StatusCode {
    StatusCode::NOT_FOUND
}

async fn websocket_upgrade(
    State(server): State<AppServer>,
    headers: HeaderMap,
    websocket: WebSocketUpgrade,
) -> Response {
    match server.remote_request_authorized(&headers).await {
        Ok(true) => {}
        Ok(false) => return remote_unauthorized(),
        Err(()) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
    if !server.origin_allowed(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    websocket
        .max_message_size(MAX_WEBSOCKET_MESSAGE_BYTES)
        .on_upgrade(move |socket| server.serve_websocket(socket))
}

impl AppServer {
    async fn remote_request_authorized(&self, headers: &HeaderMap) -> Result<bool, ()> {
        let Some(path) = self.inner.remote_auth_token_file.as_ref() else {
            return Ok(true);
        };
        let metadata = tokio::fs::metadata(path).await.map_err(|_| {
            warn!("remote authentication token file metadata could not be read");
        })?;
        if !remote_token_metadata_is_secure(&metadata) {
            warn!("remote authentication token file permissions or size are invalid");
            return Err(());
        }
        let bytes = tokio::fs::read(path).await.map_err(|_| {
            warn!("remote authentication token file could not be read");
        })?;
        let expected = parse_remote_token(&bytes).map_err(|_| {
            warn!("remote authentication token file is invalid");
        })?;
        let Some(provided) = headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(bearer_token)
        else {
            return Ok(false);
        };
        Ok(constant_time_token_eq(expected, provided))
    }

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

fn validate_remote_token_file(path: &Path) -> anyhow::Result<PathBuf> {
    let path = path.canonicalize().with_context(|| {
        format!(
            "remote authentication token file is unavailable: {}",
            path.display()
        )
    })?;
    let metadata = std::fs::metadata(&path)
        .context("remote authentication token file metadata is unavailable")?;
    anyhow::ensure!(
        remote_token_metadata_is_secure(&metadata),
        "remote authentication token file must be a small regular file and must not be accessible by group or other users"
    );
    let bytes =
        std::fs::read(&path).context("remote authentication token file could not be read")?;
    parse_remote_token(&bytes).map_err(anyhow::Error::msg)?;
    Ok(path)
}

fn validate_remote_tls_private_key(path: &Path) -> anyhow::Result<PathBuf> {
    let path = path.canonicalize().with_context(|| {
        format!(
            "remote WSS TLS private key is unavailable: {}",
            path.display()
        )
    })?;
    let metadata =
        std::fs::metadata(&path).context("remote WSS TLS private key metadata is unavailable")?;
    anyhow::ensure!(
        metadata.is_file()
            && metadata.len() > 0
            && metadata.len() <= MAX_REMOTE_TLS_PRIVATE_KEY_BYTES,
        "remote WSS TLS private key must be a non-empty regular file no larger than 1 MiB"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;

        anyhow::ensure!(
            metadata.permissions().mode().trailing_zeros() >= 6,
            "remote WSS TLS private key must not be accessible by group or other users"
        );
    }
    Ok(path)
}

fn remote_token_metadata_is_secure(metadata: &std::fs::Metadata) -> bool {
    if !metadata.is_file() || metadata.len() > (MAX_REMOTE_TOKEN_BYTES + 2) as u64 {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;

        if metadata.mode().trailing_zeros() < 6 {
            return false;
        }
    }
    true
}

fn parse_remote_token(bytes: &[u8]) -> Result<&str, &'static str> {
    let value =
        std::str::from_utf8(bytes).map_err(|_| "remote authentication token must be UTF-8")?;
    let token = value.trim_end_matches(['\r', '\n']);
    if !value[token.len()..]
        .bytes()
        .all(|byte| matches!(byte, b'\r' | b'\n'))
    {
        return Err("remote authentication token has invalid trailing data");
    }
    if !(MIN_REMOTE_TOKEN_BYTES..=MAX_REMOTE_TOKEN_BYTES).contains(&token.len())
        || !token.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(
            "remote authentication token must contain 32 through 4096 printable ASCII bytes",
        );
    }
    Ok(token)
}

fn bearer_token(value: &str) -> Option<&str> {
    let (scheme, token) = value.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("bearer") && !token.contains(char::is_whitespace)).then_some(token)
}

fn constant_time_token_eq(expected: &str, provided: &str) -> bool {
    let expected = sha2::Sha256::digest(expected.as_bytes());
    let provided = sha2::Sha256::digest(provided.as_bytes());
    expected
        .iter()
        .zip(provided.iter())
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn remote_unauthorized() -> Response {
    let mut response = StatusCode::UNAUTHORIZED.into_response();
    response.headers_mut().insert(
        WWW_AUTHENTICATE,
        HeaderValue::from_static("Bearer realm=\"qwenpaw-app-protocol\""),
    );
    response
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
