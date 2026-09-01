use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;

use futures_util::StreamExt;
use http::HeaderMap;
use http::HeaderName;
use http::HeaderValue;
use http::header::ACCEPT;
use http::header::AUTHORIZATION;
use http::header::CONTENT_TYPE;
use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::model::ServerJsonRpcMessage;
use rmcp::service::RoleClient;
use rmcp::service::RunningService;
use rmcp::service::RxJsonRpcMessage;
use rmcp::service::TxJsonRpcMessage;
use rmcp::transport::TokioChildProcess;
use rmcp::transport::Transport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tracing::info;
use tracing::warn;

mod oauth;

pub use oauth::McpOAuthCredentialStore;
pub use oauth::McpOAuthCredentials;
pub use oauth::McpOAuthStartOptions;
pub use oauth::McpOAuthStartResponse;
pub use oauth::McpOAuthStatus;
pub use oauth::SystemMcpOAuthCredentialStore;

const CONFIG_ENV: &str = "QWENPAW_MCP_CONFIG";
const MAX_CONFIG_BYTES: u64 = 1_048_576;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
const CALL_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_CLIENTS: usize = 32;
const MAX_TOOLS_PER_SERVER: usize = 64;
const MAX_DEFINITION_BYTES: usize = 65_536;
const MAX_TOTAL_DEFINITION_BYTES: usize = 1_048_576;
const MAX_RESULT_BYTES: usize = 1_048_576;
const MAX_HTTP_HEADERS: usize = 64;
const MAX_HTTP_HEADER_BYTES: usize = 16_384;

type ClientService = RunningService<RoleClient, ()>;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpClientConfig {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_true", alias = "isActive")]
    enabled: bool,
    #[serde(default, alias = "type")]
    transport: String,
    #[serde(default, alias = "baseUrl")]
    url: String,
    #[serde(default)]
    headers: HashMap<String, String>,
    #[serde(default)]
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    cwd: String,
    #[serde(default)]
    tools: Option<Vec<String>>,
    #[serde(default)]
    oauth: Option<McpOAuthConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpOAuthConfig {
    #[serde(default)]
    client_id: String,
    #[serde(default)]
    scope: String,
    #[serde(default)]
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    expires_at: f64,
    #[serde(default)]
    token_endpoint: String,
    #[serde(default, alias = "authEndpoint")]
    authorization_endpoint: String,
}

#[derive(Debug, Default, Deserialize)]
struct McpConfig {
    #[serde(default)]
    clients: BTreeMap<String, McpClientConfig>,
}

#[derive(Debug, Deserialize)]
struct AgentConfig {
    mcp: McpConfig,
}

#[derive(Debug, Clone)]
struct ToolRoute {
    server_id: String,
    remote_name: String,
}

struct Connection {
    client: Mutex<ClientService>,
    cancellation: CancellationToken,
}

#[derive(Clone)]
pub struct McpManager {
    inner: Arc<McpManagerInner>,
}

struct McpManagerInner {
    clients: BTreeMap<String, McpClientConfig>,
    connections: Mutex<HashMap<String, Arc<Connection>>>,
    routes: RwLock<HashMap<String, ToolRoute>>,
    oauth_store: Arc<dyn McpOAuthCredentialStore>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpToolOutput {
    pub content: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct McpClientInfo {
    pub key: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub transport: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub cwd: String,
    pub tools: Option<Vec<String>>,
    pub oauth_status: Option<McpOAuthStatus>,
}

impl McpManager {
    /// Loads MCP client configuration from `QWENPAW_MCP_CONFIG`.
    ///
    /// # Errors
    ///
    /// Returns an error when the configured file is missing, oversized, or
    /// malformed. An unset variable creates a manager with no MCP clients.
    pub fn from_env() -> Result<Self, McpError> {
        let Some(path) = std::env::var_os(CONFIG_ENV) else {
            return Ok(Self::empty());
        };
        Self::from_path(Path::new(&path))
    }

    #[must_use]
    pub fn empty() -> Self {
        Self::new(BTreeMap::new(), Arc::new(SystemMcpOAuthCredentialStore))
    }

    /// Loads the legacy-compatible MCP client map from a JSON file.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid file metadata, JSON, or enabled clients.
    pub fn from_path(path: &Path) -> Result<Self, McpError> {
        Self::from_path_with_oauth_store(path, Arc::new(SystemMcpOAuthCredentialStore))
    }

    /// Loads MCP clients with an explicitly supplied OAuth credential store.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid configuration or enabled clients.
    pub fn from_path_with_oauth_store(
        path: &Path,
        oauth_store: Arc<dyn McpOAuthCredentialStore>,
    ) -> Result<Self, McpError> {
        let metadata = std::fs::metadata(path).map_err(|source| McpError::ConfigRead {
            path: path.to_path_buf(),
            source,
        })?;
        if metadata.len() > MAX_CONFIG_BYTES {
            return Err(McpError::ConfigTooLarge(metadata.len()));
        }
        let bytes = std::fs::read(path).map_err(|source| McpError::ConfigRead {
            path: path.to_path_buf(),
            source,
        })?;
        let value: Value = serde_json::from_slice(&bytes)?;
        let mut config = if value.get("mcp").is_some() {
            serde_json::from_value::<AgentConfig>(value)?.mcp
        } else {
            serde_json::from_value::<McpConfig>(value)?
        };
        if config.clients.len() > MAX_CLIENTS {
            return Err(McpError::TooManyClients(config.clients.len()));
        }
        for client in config.clients.values_mut() {
            normalize_transport(client);
        }
        for (id, client) in &config.clients {
            validate_client(id, client)?;
        }
        Ok(Self::new(config.clients, oauth_store))
    }

    fn new(
        clients: BTreeMap<String, McpClientConfig>,
        oauth_store: Arc<dyn McpOAuthCredentialStore>,
    ) -> Self {
        Self {
            inner: Arc::new(McpManagerInner {
                clients,
                connections: Mutex::new(HashMap::new()),
                routes: RwLock::new(HashMap::new()),
                oauth_store,
            }),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner
            .clients
            .values()
            .all(|client| !client.enabled || !transport_is_supported(&client.transport))
    }

    /// Returns a redacted snapshot of configured MCP clients.
    ///
    /// # Errors
    ///
    /// Returns an error when OAuth status cannot be read from the secure store.
    pub async fn clients(&self) -> Result<Vec<McpClientInfo>, McpError> {
        let mut clients = Vec::with_capacity(self.inner.clients.len());
        for (key, config) in &self.inner.clients {
            let remote = matches!(config.transport.as_str(), "streamable_http" | "sse");
            let oauth_status = if remote {
                Some(self.oauth_status(key).await?)
            } else {
                None
            };
            clients.push(McpClientInfo {
                key: key.clone(),
                name: if config.name.is_empty() {
                    key.clone()
                } else {
                    config.name.clone()
                },
                description: config.description.clone(),
                enabled: config.enabled,
                transport: config.transport.clone(),
                url: config.url.clone(),
                headers: redact_values(&config.headers),
                command: config.command.clone(),
                args: config.args.clone(),
                env: redact_values(&config.env),
                cwd: config.cwd.clone(),
                tools: config.tools.clone(),
                oauth_status,
            });
        }
        Ok(clients)
    }

    /// Discovers enabled MCP tools and returns OpenAI-compatible definitions.
    pub async fn definitions(&self) -> Vec<Value> {
        let mut definitions = Vec::new();
        let mut routes = HashMap::new();
        let mut total_bytes = 0_usize;
        for (server_id, config) in &self.inner.clients {
            if !config.enabled {
                continue;
            }
            if !transport_is_supported(&config.transport) {
                warn!(
                    %server_id,
                    transport = %config.transport,
                    "MCP transport is not supported"
                );
                continue;
            }
            match self.discover_server(server_id, config).await {
                Ok(discovered) => {
                    for (definition, route) in discovered {
                        let name = route_name(&definition);
                        let definition_bytes = definition.to_string().len();
                        match routes.entry(name) {
                            Entry::Occupied(_) => {
                                warn!(%server_id, "duplicate MCP model tool name was skipped");
                            }
                            Entry::Vacant(entry)
                                if definition_bytes > MAX_DEFINITION_BYTES
                                    || total_bytes.saturating_add(definition_bytes)
                                        > MAX_TOTAL_DEFINITION_BYTES =>
                            {
                                warn!(
                                    %server_id,
                                    name = %entry.key(),
                                    "oversized MCP tool definition was skipped"
                                );
                            }
                            Entry::Vacant(entry) => {
                                total_bytes += definition_bytes;
                                entry.insert(route);
                                definitions.push(definition);
                            }
                        }
                    }
                }
                Err(error) => warn!(%server_id, %error, "MCP tool discovery failed"),
            }
        }
        *self.inner.routes.write().await = routes;
        definitions
    }

    #[must_use]
    pub async fn contains_tool(&self, name: &str) -> bool {
        self.inner.routes.read().await.contains_key(name)
    }

    /// Calls a previously discovered namespaced MCP tool.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown tool, malformed arguments, a closed
    /// server, or a timed-out MCP request.
    pub async fn call_tool(&self, name: &str, arguments: &str) -> Result<McpToolOutput, McpError> {
        let route = self
            .inner
            .routes
            .read()
            .await
            .get(name)
            .cloned()
            .ok_or_else(|| McpError::UnknownTool(name.to_owned()))?;
        let arguments: Value = serde_json::from_str(arguments)?;
        let arguments = arguments
            .as_object()
            .cloned()
            .ok_or_else(|| McpError::ArgumentsNotObject(name.to_owned()))?;
        let connection = self.connection(&route.server_id).await?;
        let transport = self
            .inner
            .clients
            .get(&route.server_id)
            .map(|config| config.transport.as_str())
            .unwrap_or_default();
        let client = connection.client.lock().await;
        let request = CallToolRequestParams::new(route.remote_name).with_arguments(arguments);
        let result = tokio::select! {
            () = connection.cancellation.cancelled() => {
                return Err(McpError::CallCancelled(name.to_owned()));
            }
            result = tokio::time::timeout(CALL_TIMEOUT, client.call_tool(request)) => {
                result
                    .map_err(|_| McpError::CallTimedOut(name.to_owned()))?
                    .map_err(|error| request_error(transport, &error.to_string()))?
            }
        };
        let bytes = serde_json::to_vec(&result)?;
        if bytes.len() > MAX_RESULT_BYTES {
            return Err(McpError::ResultTooLarge {
                name: name.to_owned(),
                bytes: bytes.len(),
            });
        }
        let value: Value = serde_json::from_slice(&bytes)?;
        Ok(output_from_result(&value))
    }

    pub async fn cancel_tool(&self, name: &str) {
        let route = self.inner.routes.read().await.get(name).cloned();
        let Some(route) = route else {
            return;
        };
        let connection = self.inner.connections.lock().await.remove(&route.server_id);
        if let Some(connection) = connection {
            connection.cancellation.cancel();
        }
    }

    async fn discover_server(
        &self,
        server_id: &str,
        config: &McpClientConfig,
    ) -> Result<Vec<(Value, ToolRoute)>, McpError> {
        let connection = self.connection(server_id).await?;
        let client = connection.client.lock().await;
        let tools = tokio::time::timeout(STARTUP_TIMEOUT, client.list_all_tools())
            .await
            .map_err(|_| McpError::DiscoveryTimedOut(server_id.to_owned()))?
            .map_err(|error| request_error(&config.transport, &error.to_string()))?;
        let mut result = Vec::new();
        for tool in tools.into_iter().take(MAX_TOOLS_PER_SERVER) {
            let remote_name = tool.name.to_string();
            if config
                .tools
                .as_ref()
                .is_some_and(|allowed| !allowed.contains(&remote_name))
            {
                continue;
            }
            let model_name = model_tool_name(server_id, &remote_name);
            let description = tool.description.as_deref().unwrap_or_else(|| {
                if config.description.is_empty() {
                    "MCP tool"
                } else {
                    &config.description
                }
            });
            let definition = json!({
                "type": "function",
                "function": {
                    "name": model_name,
                    "description": description,
                    "parameters": tool.input_schema,
                }
            });
            result.push((
                definition,
                ToolRoute {
                    server_id: server_id.to_owned(),
                    remote_name,
                },
            ));
        }
        Ok(result)
    }

    async fn connection(&self, server_id: &str) -> Result<Arc<Connection>, McpError> {
        let existing = { self.inner.connections.lock().await.get(server_id).cloned() };
        if let Some(connection) = existing
            && !connection.client.lock().await.is_closed()
        {
            return Ok(connection);
        }
        let config = self
            .inner
            .clients
            .get(server_id)
            .ok_or_else(|| McpError::UnknownServer(server_id.to_owned()))?;
        let cancellation = CancellationToken::new();
        let client = match config.transport.as_str() {
            "stdio" => start_stdio_client(server_id, config, cancellation.clone()).await?,
            "streamable_http" => {
                start_http_client(self, server_id, config, cancellation.clone()).await?
            }
            "sse" => start_sse_client(self, server_id, config, cancellation.clone()).await?,
            _ => {
                return Err(McpError::UnsupportedTransport {
                    id: server_id.to_owned(),
                    transport: config.transport.clone(),
                });
            }
        };
        let connection = Arc::new(Connection {
            client: Mutex::new(client),
            cancellation,
        });
        self.inner
            .connections
            .lock()
            .await
            .insert(server_id.to_owned(), Arc::clone(&connection));
        Ok(connection)
    }
}

async fn start_stdio_client(
    server_id: &str,
    config: &McpClientConfig,
    cancellation: CancellationToken,
) -> Result<ClientService, McpError> {
    let mut command = Command::new(&config.command);
    command
        .args(&config.args)
        .envs(resolve_environment(&config.env)?)
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    if !config.cwd.is_empty() {
        command.current_dir(&config.cwd);
    }
    let (transport, stderr) = TokioChildProcess::builder(command)
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(stderr) = stderr {
        let server_id = server_id.to_owned();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                info!(%server_id, %line, "MCP server stderr");
            }
        });
    }
    tokio::time::timeout(STARTUP_TIMEOUT, ().serve_with_ct(transport, cancellation))
        .await
        .map_err(|_| McpError::StartupTimedOut(server_id.to_owned()))?
        .map_err(|error| McpError::Startup(error.to_string()))
}

async fn start_http_client(
    manager: &McpManager,
    server_id: &str,
    config: &McpClientConfig,
    cancellation: CancellationToken,
) -> Result<ClientService, McpError> {
    let url = expand_environment(&config.url)?;
    validate_http_url(server_id, &url)?;
    let http_client = reqwest::Client::builder()
        .connect_timeout(STARTUP_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| McpError::HttpClient(error.to_string()))?;
    let (headers, configured_token) = resolve_http_headers(config)?;
    let bearer_token =
        resolve_manager_bearer(manager, server_id, config, configured_token, &http_client).await?;
    let mut transport_config = StreamableHttpClientTransportConfig::with_uri(url)
        .custom_headers(headers)
        .max_sse_event_size(MAX_RESULT_BYTES);
    if let Some(token) = bearer_token {
        transport_config = transport_config.auth_header(token);
    }
    let transport = StreamableHttpClientTransport::with_client(http_client, transport_config);
    tokio::time::timeout(STARTUP_TIMEOUT, ().serve_with_ct(transport, cancellation))
        .await
        .map_err(|_| McpError::StartupTimedOut(server_id.to_owned()))?
        .map_err(|_| McpError::Startup(String::from("streamable HTTP handshake failed")))
}

async fn start_sse_client(
    manager: &McpManager,
    server_id: &str,
    config: &McpClientConfig,
    cancellation: CancellationToken,
) -> Result<ClientService, McpError> {
    let result = tokio::time::timeout(STARTUP_TIMEOUT, async {
        let transport =
            LegacySseTransport::connect(manager, server_id, config, cancellation.clone()).await?;
        ().serve_with_ct(transport, cancellation)
            .await
            .map_err(|error| LegacySseError::Protocol(error.to_string()))
    })
    .await
    .map_err(|_| McpError::StartupTimedOut(server_id.to_owned()))?;
    result.map_err(|error| McpError::Startup(error.to_string()))
}

#[derive(Clone)]
struct LegacySseSender {
    client: reqwest::Client,
    endpoint: reqwest::Url,
    headers: HeaderMap,
}

impl LegacySseSender {
    async fn send(&self, message: TxJsonRpcMessage<RoleClient>) -> Result<(), LegacySseError> {
        let response = self
            .client
            .post(self.endpoint.clone())
            .headers(self.headers.clone())
            .header(CONTENT_TYPE, "application/json")
            .json(&message)
            .send()
            .await
            .map_err(|_| LegacySseError::Http("POST request"))?;
        if !response.status().is_success() {
            return Err(LegacySseError::HttpStatus(response.status().as_u16()));
        }
        Ok(())
    }
}

struct LegacySseTransport {
    sender: LegacySseSender,
    receiver: mpsc::Receiver<ServerJsonRpcMessage>,
    cancellation: CancellationToken,
}

impl LegacySseTransport {
    async fn connect(
        manager: &McpManager,
        server_id: &str,
        config: &McpClientConfig,
        cancellation: CancellationToken,
    ) -> Result<Self, LegacySseError> {
        let raw_url = expand_environment(&config.url)
            .map_err(|error| LegacySseError::Configuration(error.to_string()))?;
        validate_http_url(server_id, &raw_url)
            .map_err(|error| LegacySseError::Configuration(error.to_string()))?;
        let base_url = reqwest::Url::parse(&raw_url)
            .map_err(|_| LegacySseError::Configuration(String::from("invalid SSE URL")))?;
        let client = reqwest::Client::builder()
            .connect_timeout(STARTUP_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| LegacySseError::Http("client setup"))?;
        let (custom_headers, configured_token) = resolve_http_headers(config)
            .map_err(|error| LegacySseError::Configuration(error.to_string()))?;
        let bearer_token =
            resolve_manager_bearer(manager, server_id, config, configured_token, &client)
                .await
                .map_err(|error| LegacySseError::Configuration(error.to_string()))?;
        let headers = legacy_header_map(custom_headers, bearer_token)?;
        let response = client
            .get(base_url.clone())
            .headers(headers.clone())
            .header(ACCEPT, "text/event-stream")
            .send()
            .await
            .map_err(|_| LegacySseError::Http("SSE connection"))?;
        if !response.status().is_success() {
            return Err(LegacySseError::HttpStatus(response.status().as_u16()));
        }
        let is_event_stream = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value
                    .split(';')
                    .next()
                    .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("text/event-stream"))
            });
        if !is_event_stream {
            return Err(LegacySseError::UnexpectedContentType);
        }
        let (endpoint_tx, endpoint_rx) = oneshot::channel();
        let (message_tx, message_rx) = mpsc::channel(128);
        let reader_cancellation = cancellation.clone();
        let server_id = server_id.to_owned();
        tokio::spawn(async move {
            let result = read_legacy_sse(
                response,
                &base_url,
                message_tx,
                endpoint_tx,
                reader_cancellation,
            )
            .await;
            if let Err(error) = result {
                warn!(%server_id, %error, "legacy MCP SSE reader stopped");
            }
        });
        let endpoint = endpoint_rx
            .await
            .map_err(|_| LegacySseError::MissingEndpoint)??;
        Ok(Self {
            sender: LegacySseSender {
                client,
                endpoint,
                headers,
            },
            receiver: message_rx,
            cancellation,
        })
    }
}

impl Transport<RoleClient> for LegacySseTransport {
    type Error = LegacySseError;

    fn send(
        &mut self,
        item: TxJsonRpcMessage<RoleClient>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
        let sender = self.sender.clone();
        async move { sender.send(item).await }
    }

    async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleClient>> {
        self.receiver.recv().await
    }

    fn close(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.cancellation.cancel();
        self.receiver.close();
        std::future::ready(Ok(()))
    }
}

async fn read_legacy_sse(
    response: reqwest::Response,
    base_url: &reqwest::Url,
    message_tx: mpsc::Sender<ServerJsonRpcMessage>,
    endpoint_tx: oneshot::Sender<Result<reqwest::Url, LegacySseError>>,
    cancellation: CancellationToken,
) -> Result<(), LegacySseError> {
    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    let mut endpoint_tx = Some(endpoint_tx);
    loop {
        let chunk = tokio::select! {
            () = cancellation.cancelled() => return Ok(()),
            chunk = stream.next() => chunk,
        };
        let Some(chunk) = chunk else {
            if let Some(sender) = endpoint_tx.take() {
                let _ = sender.send(Err(LegacySseError::MissingEndpoint));
            }
            return Ok(());
        };
        let chunk = chunk.map_err(|_| LegacySseError::Http("SSE read"))?;
        buffer.extend_from_slice(&chunk);
        if buffer.len() > MAX_RESULT_BYTES {
            return Err(LegacySseError::EventTooLarge);
        }
        while let Some((event_end, delimiter_len)) = sse_event_end(&buffer) {
            let event = buffer.drain(..event_end).collect::<Vec<_>>();
            buffer.drain(..delimiter_len);
            let Some(event) = parse_sse_event(&event)? else {
                continue;
            };
            match event.kind.as_str() {
                "endpoint" => {
                    let endpoint = validate_legacy_endpoint(base_url, &event.data);
                    let Some(sender) = endpoint_tx.take() else {
                        return Err(LegacySseError::DuplicateEndpoint);
                    };
                    sender.send(endpoint).map_err(|_| LegacySseError::Closed)?;
                }
                "message" => {
                    let message = serde_json::from_str::<ServerJsonRpcMessage>(&event.data)
                        .map_err(|error| LegacySseError::Protocol(error.to_string()))?;
                    if message_tx.send(message).await.is_err() {
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
    }
}

struct ParsedSseEvent {
    kind: String,
    data: String,
}

fn parse_sse_event(bytes: &[u8]) -> Result<Option<ParsedSseEvent>, LegacySseError> {
    let text = std::str::from_utf8(bytes).map_err(|_| LegacySseError::InvalidUtf8)?;
    let mut kind = String::from("message");
    let mut data = Vec::new();
    for raw_line in text.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.starts_with(':') {
            continue;
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "event" => value.clone_into(&mut kind),
            "data" => data.push(value),
            _ => {}
        }
    }
    if data.is_empty() {
        return Ok(None);
    }
    Ok(Some(ParsedSseEvent {
        kind,
        data: data.join("\n"),
    }))
}

fn sse_event_end(buffer: &[u8]) -> Option<(usize, usize)> {
    [
        b"\r\n\r\n".as_slice(),
        b"\n\n".as_slice(),
        b"\r\r".as_slice(),
    ]
    .into_iter()
    .filter_map(|delimiter| {
        buffer
            .windows(delimiter.len())
            .position(|window| window == delimiter)
            .map(|position| (position, delimiter.len()))
    })
    .min_by_key(|(position, _)| *position)
}

fn validate_legacy_endpoint(
    base_url: &reqwest::Url,
    value: &str,
) -> Result<reqwest::Url, LegacySseError> {
    let endpoint = base_url
        .join(value.trim())
        .map_err(|_| LegacySseError::InvalidEndpoint)?;
    let same_origin = endpoint.scheme() == base_url.scheme()
        && endpoint.host_str() == base_url.host_str()
        && endpoint.port_or_known_default() == base_url.port_or_known_default();
    if !same_origin
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(LegacySseError::InvalidEndpoint);
    }
    Ok(endpoint)
}

fn legacy_header_map(
    custom_headers: HashMap<HeaderName, HeaderValue>,
    bearer_token: Option<String>,
) -> Result<HeaderMap, LegacySseError> {
    let mut headers = custom_headers.into_iter().collect::<HeaderMap>();
    if let Some(token) = bearer_token {
        let mut value = HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| LegacySseError::Configuration(String::from("invalid Bearer token")))?;
        value.set_sensitive(true);
        headers.insert(AUTHORIZATION, value);
    }
    Ok(headers)
}

#[derive(Debug, thiserror::Error)]
enum LegacySseError {
    #[error("legacy SSE configuration is invalid: {0}")]
    Configuration(String),
    #[error("legacy SSE HTTP operation failed during {0}")]
    Http(&'static str),
    #[error("legacy SSE endpoint returned HTTP {0}")]
    HttpStatus(u16),
    #[error("legacy SSE endpoint did not return text/event-stream")]
    UnexpectedContentType,
    #[error("legacy SSE stream did not provide an endpoint event")]
    MissingEndpoint,
    #[error("legacy SSE stream provided a duplicate endpoint event")]
    DuplicateEndpoint,
    #[error("legacy SSE endpoint event was invalid or changed origin")]
    InvalidEndpoint,
    #[error("legacy SSE event exceeded the 1048576-byte limit")]
    EventTooLarge,
    #[error("legacy SSE event was not UTF-8")]
    InvalidUtf8,
    #[error("legacy SSE protocol failed: {0}")]
    Protocol(String),
    #[error("legacy SSE transport closed")]
    Closed,
}

fn default_true() -> bool {
    true
}

fn redact_values(values: &HashMap<String, String>) -> HashMap<String, String> {
    values
        .keys()
        .map(|key| (key.clone(), String::from("********")))
        .collect()
}

fn normalize_transport(config: &mut McpClientConfig) {
    let transport = config.transport.trim().to_ascii_lowercase();
    config.transport = match transport.as_str() {
        "" if !config.url.trim().is_empty() && config.command.trim().is_empty() => {
            String::from("streamable_http")
        }
        "" | "stdio" => String::from("stdio"),
        "http" | "streamable-http" | "streamablehttp" | "streamable_http" => {
            String::from("streamable_http")
        }
        "sse" => String::from("sse"),
        _ => transport,
    };
}

fn transport_is_supported(transport: &str) -> bool {
    matches!(transport, "stdio" | "streamable_http" | "sse")
}

fn request_error(transport: &str, message: &str) -> McpError {
    if transport == "stdio" {
        McpError::Request(message.to_owned())
    } else {
        McpError::Request(String::from("remote MCP request failed"))
    }
}

fn validate_client(id: &str, config: &McpClientConfig) -> Result<(), McpError> {
    if id.trim().is_empty() {
        return Err(McpError::InvalidConfig(String::from(
            "MCP client id cannot be empty",
        )));
    }
    if !config.enabled {
        return Ok(());
    }
    if !transport_is_supported(&config.transport) {
        return Err(McpError::UnsupportedTransport {
            id: id.to_owned(),
            transport: config.transport.clone(),
        });
    }
    if config.transport == "stdio" && config.command.trim().is_empty() {
        return Err(McpError::InvalidConfig(format!(
            "stdio MCP client {id} requires a command"
        )));
    }
    if matches!(config.transport.as_str(), "streamable_http" | "sse")
        && config.url.trim().is_empty()
    {
        return Err(McpError::InvalidConfig(format!(
            "{} MCP client {id} requires a URL",
            config.transport
        )));
    }
    if matches!(config.transport.as_str(), "streamable_http" | "sse") && !config.url.contains("${")
    {
        validate_http_url(id, &config.url)?;
    }
    if config.headers.len() > MAX_HTTP_HEADERS {
        return Err(McpError::InvalidConfig(format!(
            "MCP client {id} has too many HTTP headers"
        )));
    }
    Ok(())
}

fn validate_http_url(server_id: &str, value: &str) -> Result<(), McpError> {
    let url = reqwest::Url::parse(value).map_err(|_| {
        McpError::InvalidConfig(format!("MCP client {server_id} has an invalid URL"))
    })?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(McpError::InvalidConfig(format!(
            "MCP client {server_id} URL must be HTTP(S), include a host, and omit credentials and fragments"
        )));
    }
    Ok(())
}

fn resolve_http_headers(
    config: &McpClientConfig,
) -> Result<(HashMap<HeaderName, HeaderValue>, Option<String>), McpError> {
    let mut headers = HashMap::new();
    let mut bearer_token = None;
    let mut total_bytes = 0_usize;
    for (raw_name, raw_value) in &config.headers {
        let name = HeaderName::from_bytes(raw_name.as_bytes()).map_err(|_| {
            McpError::InvalidConfig(format!("invalid MCP HTTP header name: {raw_name}"))
        })?;
        let value = expand_environment(raw_value)?;
        total_bytes = total_bytes.saturating_add(raw_name.len() + value.len());
        if total_bytes > MAX_HTTP_HEADER_BYTES {
            return Err(McpError::InvalidConfig(String::from(
                "MCP HTTP headers exceed the 16384-byte limit",
            )));
        }
        if name == AUTHORIZATION {
            let token = bearer_value(&value).ok_or_else(|| {
                McpError::InvalidConfig(String::from(
                    "MCP Authorization header must use a non-empty Bearer token",
                ))
            })?;
            if bearer_token.replace(token).is_some() {
                return Err(McpError::InvalidConfig(String::from(
                    "MCP configuration contains multiple Bearer tokens",
                )));
            }
            continue;
        }
        if matches!(name.as_str(), "accept" | "mcp-session-id" | "last-event-id") {
            return Err(McpError::InvalidConfig(format!(
                "MCP HTTP header {name} is reserved by the transport"
            )));
        }
        let mut value = HeaderValue::from_str(&value).map_err(|_| {
            McpError::InvalidConfig(format!("invalid value for MCP HTTP header {name}"))
        })?;
        value.set_sensitive(true);
        headers.insert(name, value);
    }
    if let Some(token) = &bearer_token {
        validate_bearer_token(token)?;
        total_bytes = total_bytes.saturating_add(token.len());
        if total_bytes > MAX_HTTP_HEADER_BYTES {
            return Err(McpError::InvalidConfig(String::from(
                "MCP HTTP headers exceed the 16384-byte limit",
            )));
        }
    }
    Ok((headers, bearer_token))
}

async fn resolve_http_bearer(
    server_id: &str,
    config: &McpClientConfig,
    configured_token: Option<String>,
    client: &reqwest::Client,
) -> Result<Option<String>, McpError> {
    let Some(oauth) = &config.oauth else {
        return Ok(configured_token);
    };
    if configured_token.is_some()
        && (!oauth.access_token.is_empty() || !oauth.refresh_token.is_empty())
    {
        return Err(McpError::InvalidConfig(String::from(
            "MCP configuration cannot set both Authorization and OAuth credentials",
        )));
    }
    if configured_token.is_some() {
        return Ok(configured_token);
    }
    let expired = oauth.expires_at > 0.0
        && SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .is_ok_and(|now| now.as_secs_f64() >= oauth.expires_at);
    if !oauth.access_token.is_empty() && !expired {
        let token = expand_environment(&oauth.access_token)?;
        validate_bearer_token(&token)?;
        return Ok(Some(token));
    }
    if oauth.refresh_token.is_empty()
        || oauth.token_endpoint.is_empty()
        || oauth.client_id.is_empty()
    {
        return Err(McpError::OAuthRefresh(format!(
            "MCP client {server_id} requires a new interactive authorization"
        )));
    }
    refresh_oauth_token(server_id, oauth, client)
        .await
        .map(Some)
}

async fn resolve_manager_bearer(
    manager: &McpManager,
    server_id: &str,
    config: &McpClientConfig,
    configured_token: Option<String>,
    client: &reqwest::Client,
) -> Result<Option<String>, McpError> {
    if let Some(stored_token) = manager
        .stored_oauth_bearer(server_id, config, client)
        .await?
    {
        if configured_token.is_some() {
            return Err(McpError::InvalidConfig(String::from(
                "MCP configuration cannot combine Authorization with stored OAuth credentials",
            )));
        }
        return Ok(Some(stored_token));
    }
    resolve_http_bearer(server_id, config, configured_token, client).await
}

async fn refresh_oauth_token(
    server_id: &str,
    oauth: &McpOAuthConfig,
    client: &reqwest::Client,
) -> Result<String, McpError> {
    let endpoint = expand_environment(&oauth.token_endpoint)?;
    validate_http_url(server_id, &endpoint)?;
    let refresh_token = expand_environment(&oauth.refresh_token)?;
    let client_id = expand_environment(&oauth.client_id)?;
    let mut form = BTreeMap::from([
        ("client_id", client_id),
        ("grant_type", String::from("refresh_token")),
        ("refresh_token", refresh_token),
    ]);
    if !oauth.scope.is_empty() {
        form.insert("scope", oauth.scope.clone());
    }
    let response = client
        .post(endpoint)
        .form(&form)
        .send()
        .await
        .map_err(|_| McpError::OAuthRefresh(String::from("token request failed")))?;
    if !response.status().is_success() {
        return Err(McpError::OAuthRefresh(format!(
            "token endpoint returned HTTP {}",
            response.status().as_u16()
        )));
    }
    let body = read_bounded_http_body(response, 65_536).await?;
    let token: OAuthTokenResponse = serde_json::from_slice(&body)
        .map_err(|_| McpError::OAuthRefresh(String::from("invalid token response")))?;
    if token
        .token_type
        .as_deref()
        .is_some_and(|token_type| !token_type.eq_ignore_ascii_case("bearer"))
    {
        return Err(McpError::OAuthRefresh(String::from(
            "token endpoint returned a non-Bearer token",
        )));
    }
    validate_bearer_token(&token.access_token)?;
    Ok(token.access_token)
}

async fn read_bounded_http_body(
    response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, McpError> {
    if response
        .content_length()
        .is_some_and(|content_length| content_length > limit as u64)
    {
        return Err(McpError::OAuthRefresh(String::from(
            "token response exceeded 65536 bytes",
        )));
    }
    let mut stream = response.bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk
            .map_err(|_| McpError::OAuthRefresh(String::from("token response read failed")))?;
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(McpError::OAuthRefresh(String::from(
                "token response exceeded 65536 bytes",
            )));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[derive(Debug, Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
    #[serde(default)]
    token_type: Option<String>,
}

fn bearer_value(value: &str) -> Option<String> {
    let (scheme, token) = value.trim().split_once(char::is_whitespace)?;
    let token = token.trim();
    if scheme.eq_ignore_ascii_case("bearer") && !token.is_empty() {
        Some(token.to_owned())
    } else {
        None
    }
}

fn validate_bearer_token(token: &str) -> Result<(), McpError> {
    if token.is_empty() || token.chars().any(char::is_whitespace) {
        return Err(McpError::InvalidConfig(String::from(
            "MCP Bearer token is empty or contains whitespace",
        )));
    }
    HeaderValue::from_str(&format!("Bearer {token}"))
        .map(|_| ())
        .map_err(|_| McpError::InvalidConfig(String::from("MCP Bearer token is invalid")))
}

fn resolve_environment(
    values: &HashMap<String, String>,
) -> Result<HashMap<String, String>, McpError> {
    values
        .iter()
        .map(|(name, value)| Ok((name.clone(), expand_environment(value)?)))
        .collect()
}

fn expand_environment(value: &str) -> Result<String, McpError> {
    let mut expanded = String::new();
    let mut remaining = value;
    while let Some(start) = remaining.find("${") {
        expanded.push_str(&remaining[..start]);
        let after = &remaining[start + 2..];
        let end = after.find('}').ok_or_else(|| {
            McpError::InvalidConfig(format!("unterminated environment reference in {value}"))
        })?;
        let name = &after[..end];
        let replacement =
            std::env::var(name).map_err(|_| McpError::MissingEnvironment(name.to_owned()))?;
        expanded.push_str(&replacement);
        remaining = &after[end + 1..];
    }
    expanded.push_str(remaining);
    Ok(expanded)
}

fn model_tool_name(server_id: &str, remote_name: &str) -> String {
    let mut name = format!(
        "mcp__{}__{}",
        sanitize_name(server_id),
        sanitize_name(remote_name)
    );
    name.truncate(64);
    name
}

fn sanitize_name(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn route_name(definition: &Value) -> String {
    definition["function"]["name"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

fn output_from_result(value: &Value) -> McpToolOutput {
    let is_error = value
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if let Some(structured) = value.get("structuredContent")
        && !structured.is_null()
    {
        return McpToolOutput {
            content: structured.to_string(),
            is_error,
        };
    }
    let texts = value
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|content| content.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>();
    McpToolOutput {
        content: if texts.is_empty() {
            value.to_string()
        } else {
            texts.join("\n")
        },
        is_error,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("failed to read MCP config {path}: {source}")]
    ConfigRead {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("MCP config exceeds 1048576 bytes: {0}")]
    ConfigTooLarge(u64),
    #[error("MCP config has {0} clients, exceeding the 32-client limit")]
    TooManyClients(usize),
    #[error("invalid MCP JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid MCP configuration: {0}")]
    InvalidConfig(String),
    #[error("MCP client {id} uses unsupported transport {transport}")]
    UnsupportedTransport { id: String, transport: String },
    #[error("MCP configuration references missing environment variable {0}")]
    MissingEnvironment(String),
    #[error("unknown MCP server: {0}")]
    UnknownServer(String),
    #[error("unknown MCP tool: {0}")]
    UnknownTool(String),
    #[error("MCP tool arguments must be a JSON object: {0}")]
    ArgumentsNotObject(String),
    #[error("failed to spawn MCP server: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("failed to create MCP HTTP client: {0}")]
    HttpClient(String),
    #[error("MCP OAuth failed: {0}")]
    OAuth(String),
    #[error("MCP OAuth refresh failed: {0}")]
    OAuthRefresh(String),
    #[error("MCP server startup failed: {0}")]
    Startup(String),
    #[error("MCP request failed: {0}")]
    Request(String),
    #[error("MCP server startup timed out: {0}")]
    StartupTimedOut(String),
    #[error("MCP tool discovery timed out: {0}")]
    DiscoveryTimedOut(String),
    #[error("MCP tool call timed out: {0}")]
    CallTimedOut(String),
    #[error("MCP tool call was cancelled: {0}")]
    CallCancelled(String),
    #[error("MCP tool {name} returned {bytes} bytes, exceeding the 1048576-byte limit")]
    ResultTooLarge { name: String, bytes: usize },
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;
