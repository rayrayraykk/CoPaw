use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore as _;
use reqwest::header::WWW_AUTHENTICATE;
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::Digest as _;
use sha2::Sha256;
use tokio::io::AsyncReadExt as _;
use tokio::io::AsyncWriteExt as _;
use url::Url;

use super::McpClientConfig;
use super::McpError;
use super::McpManager;
use super::expand_environment;
use super::validate_bearer_token;
use super::validate_http_url;

const OAUTH_TIMEOUT: Duration = Duration::from_secs(15);
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(600);
const MAX_OAUTH_RESPONSE_BYTES: usize = 65_536;
const MAX_CALLBACK_REQUEST_BYTES: usize = 16_384;
const MAX_CALLBACK_ATTEMPTS: usize = 16;
const CREDENTIAL_SERVICE: &str = "io.qwenpaw.mcp.oauth";

#[derive(Debug, Clone, Default)]
pub struct McpOAuthStartOptions {
    pub url: String,
    pub scope: String,
    pub client_id: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpOAuthStartResponse {
    pub authorization_url: String,
    pub session_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct McpOAuthStatus {
    pub authorized: bool,
    pub expires_at: f64,
    pub scope: String,
    pub client_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpOAuthCredentials {
    issuer: String,
    resource: String,
    client_id: String,
    authorization_endpoint: String,
    token_endpoint: String,
    scope: String,
    access_token: String,
    refresh_token: String,
    expires_at: f64,
}

pub trait McpOAuthCredentialStore: Send + Sync {
    /// Loads a credential JSON document by its opaque account identifier.
    ///
    /// # Errors
    ///
    /// Returns an error when the platform credential store cannot be read.
    fn load(&self, account: &str) -> Result<Option<McpOAuthCredentials>, String>;

    /// Replaces a credential JSON document.
    ///
    /// # Errors
    ///
    /// Returns an error when the platform credential store cannot be written.
    fn save(&self, account: &str, credentials: &McpOAuthCredentials) -> Result<(), String>;

    /// Deletes a credential JSON document.
    ///
    /// # Errors
    ///
    /// Returns an error when the platform credential store cannot be changed.
    fn delete(&self, account: &str) -> Result<(), String>;
}

#[derive(Debug, Default)]
pub struct SystemMcpOAuthCredentialStore;

impl McpOAuthCredentialStore for SystemMcpOAuthCredentialStore {
    fn load(&self, account: &str) -> Result<Option<McpOAuthCredentials>, String> {
        let entry = credential_entry(account)?;
        match entry.get_password() {
            Ok(value) => serde_json::from_str(&value)
                .map(Some)
                .map_err(|_| String::from("stored MCP OAuth credential is invalid")),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(String::from("system credential storage could not be read")),
        }
    }

    fn save(&self, account: &str, credentials: &McpOAuthCredentials) -> Result<(), String> {
        let value = serde_json::to_string(credentials)
            .map_err(|_| String::from("MCP OAuth credential could not be encoded"))?;
        credential_entry(account)?
            .set_password(&value)
            .map_err(|_| String::from("system credential storage could not be written"))
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        match credential_entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(String::from(
                "system credential storage could not be changed",
            )),
        }
    }
}

fn credential_entry(account: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(CREDENTIAL_SERVICE, account)
        .map_err(|_| String::from("system credential storage is unavailable"))
}

#[derive(Debug, Clone)]
struct OAuthDiscovery {
    issuer: String,
    resource: String,
    authorization_endpoint: String,
    token_endpoint: String,
    registration_endpoint: Option<String>,
    scope: String,
    authorization_response_issuer_required: bool,
}

#[derive(Debug, Deserialize)]
struct ProtectedResourceMetadata {
    resource: String,
    authorization_servers: Vec<String>,
    #[serde(default)]
    scopes_supported: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct AuthorizationServerMetadata {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    #[serde(default)]
    registration_endpoint: Option<String>,
    #[serde(default)]
    code_challenge_methods_supported: Vec<String>,
    #[serde(default)]
    authorization_response_iss_parameter_supported: bool,
}

#[derive(Debug, Deserialize)]
struct RegistrationResponse {
    client_id: String,
    #[serde(default)]
    token_endpoint_auth_method: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    token_type: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    scope: Option<String>,
}

#[derive(Debug)]
struct OAuthSession {
    server_id: String,
    account: String,
    state: String,
    verifier: String,
    redirect_uri: String,
    client_id: String,
    discovery: OAuthDiscovery,
}

impl McpManager {
    /// Starts an interactive OAuth authorization-code flow for one remote MCP client.
    ///
    /// The returned URL is intended to be opened by the caller in the system
    /// browser. Core owns a temporary random loopback callback listener.
    ///
    /// # Errors
    ///
    /// Returns an error for unknown/non-HTTP clients, unsafe discovery
    /// metadata, missing client registration, or callback listener failure.
    pub async fn start_oauth(
        &self,
        server_id: &str,
        options: McpOAuthStartOptions,
    ) -> Result<McpOAuthStartResponse, McpError> {
        let config = self.oauth_client_config(server_id)?;
        let configured_url = expand_environment(&config.url)?;
        let resource = if options.url.trim().is_empty() {
            configured_url.clone()
        } else {
            if options.url != configured_url {
                return Err(McpError::OAuth(String::from(
                    "OAuth URL must match the configured MCP client URL",
                )));
            }
            options.url.clone()
        };
        validate_secure_oauth_url(server_id, &resource)?;

        let client = oauth_http_client()?;
        let mut discovery = discover_oauth(&client, &resource).await?;
        let configured_oauth = config.oauth.as_ref();
        let authorization_override = first_non_empty([
            options.authorization_endpoint.as_str(),
            configured_oauth.map_or("", |value| value.authorization_endpoint.as_str()),
        ]);
        let token_override = first_non_empty([
            options.token_endpoint.as_str(),
            configured_oauth.map_or("", |value| value.token_endpoint.as_str()),
        ]);
        validate_endpoint_override(
            "authorization",
            authorization_override,
            &discovery.authorization_endpoint,
        )?;
        validate_endpoint_override("token", token_override, &discovery.token_endpoint)?;

        let stored = self.load_oauth_credentials(server_id, &config).await?;
        let mut client_id = first_non_empty([
            options.client_id.as_str(),
            stored.as_ref().map_or("", |value| value.client_id.as_str()),
            configured_oauth.map_or("", |value| value.client_id.as_str()),
        ])
        .to_owned();
        let requested_scope = first_non_empty([
            options.scope.as_str(),
            configured_oauth.map_or("", |value| value.scope.as_str()),
            discovery.scope.as_str(),
        ]);
        discovery.scope = requested_scope.to_owned();

        let (listener, redirect_uri) = bind_callback_listener().await?;
        if client_id.is_empty() {
            let registration_endpoint = discovery.registration_endpoint.as_deref().ok_or_else(|| {
                McpError::OAuth(String::from(
                    "OAuth requires a pre-registered client ID; this server does not offer legacy dynamic registration",
                ))
            })?;
            client_id = dynamic_register(&client, registration_endpoint, &redirect_uri).await?;
        }

        let verifier = random_url_token(32);
        let state = random_url_token(32);
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let mut authorization_url = Url::parse(&discovery.authorization_endpoint)
            .map_err(|_| McpError::OAuth(String::from("authorization endpoint URL is invalid")))?;
        {
            let mut query = authorization_url.query_pairs_mut();
            query
                .append_pair("response_type", "code")
                .append_pair("client_id", &client_id)
                .append_pair("redirect_uri", &redirect_uri)
                .append_pair("state", &state)
                .append_pair("code_challenge", &challenge)
                .append_pair("code_challenge_method", "S256")
                .append_pair("resource", &discovery.resource);
            if !discovery.scope.is_empty() {
                query.append_pair("scope", &discovery.scope);
            }
        }

        let session = OAuthSession {
            server_id: server_id.to_owned(),
            account: oauth_account(server_id, &config),
            state: state.clone(),
            verifier,
            redirect_uri,
            client_id,
            discovery,
        };
        let manager = self.clone();
        tokio::spawn(async move {
            if let Err(error) =
                run_callback_listener(listener, &manager, session, CALLBACK_TIMEOUT).await
            {
                tracing::warn!(%error, "MCP OAuth callback did not complete");
            }
        });

        Ok(McpOAuthStartResponse {
            authorization_url: authorization_url.into(),
            session_id: state,
        })
    }

    /// Returns the current secure-store OAuth status for an MCP client.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown client or credential-store failure.
    pub async fn oauth_status(&self, server_id: &str) -> Result<McpOAuthStatus, McpError> {
        let config = self.oauth_client_config(server_id)?;
        let stored = self.load_oauth_credentials(server_id, &config).await?;
        let Some(credentials) = stored else {
            return Ok(McpOAuthStatus {
                authorized: false,
                expires_at: 0.0,
                scope: String::new(),
                client_id: String::new(),
            });
        };
        Ok(McpOAuthStatus {
            authorized: !credentials.access_token.is_empty() && !credentials.is_expired(),
            expires_at: credentials.expires_at,
            scope: credentials.scope,
            client_id: credentials.client_id,
        })
    }

    /// Removes OAuth credentials and closes any cached connection.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown client or credential-store failure.
    pub async fn revoke_oauth(&self, server_id: &str) -> Result<(), McpError> {
        let config = self.oauth_client_config(server_id)?;
        let account = oauth_account(server_id, &config);
        let store = Arc::clone(&self.inner.oauth_store);
        tokio::task::spawn_blocking(move || store.delete(&account))
            .await
            .map_err(|_| McpError::OAuth(String::from("OAuth credential deletion failed")))?
            .map_err(McpError::OAuth)?;
        self.disconnect_server(server_id).await;
        Ok(())
    }

    pub(crate) async fn stored_oauth_bearer(
        &self,
        server_id: &str,
        config: &McpClientConfig,
        client: &reqwest::Client,
    ) -> Result<Option<String>, McpError> {
        let Some(mut credentials) = self.load_oauth_credentials(server_id, config).await? else {
            return Ok(None);
        };
        if !credentials.access_token.is_empty() && !credentials.is_expired() {
            validate_bearer_token(&credentials.access_token)?;
            return Ok(Some(credentials.access_token));
        }
        if credentials.refresh_token.is_empty() {
            return Err(McpError::OAuthRefresh(format!(
                "MCP client {server_id} requires a new interactive authorization"
            )));
        }
        let response = request_token(
            client,
            &credentials.token_endpoint,
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", &credentials.refresh_token),
                ("client_id", &credentials.client_id),
                ("resource", &credentials.resource),
                ("scope", &credentials.scope),
            ],
        )
        .await?;
        apply_token_response(&mut credentials, response)?;
        self.save_oauth_credentials(oauth_account(server_id, config), credentials.clone())
            .await?;
        Ok(Some(credentials.access_token))
    }

    fn oauth_client_config(&self, server_id: &str) -> Result<McpClientConfig, McpError> {
        let config = self
            .inner
            .clients
            .get(server_id)
            .cloned()
            .ok_or_else(|| McpError::UnknownServer(server_id.to_owned()))?;
        if !matches!(config.transport.as_str(), "streamable_http" | "sse") {
            return Err(McpError::OAuth(String::from(
                "interactive OAuth is available only for remote HTTP MCP clients",
            )));
        }
        if config.oauth.is_none() {
            return Err(McpError::OAuth(String::from(
                "interactive OAuth is not enabled for this MCP client",
            )));
        }
        Ok(config)
    }

    async fn load_oauth_credentials(
        &self,
        server_id: &str,
        config: &McpClientConfig,
    ) -> Result<Option<McpOAuthCredentials>, McpError> {
        let account = oauth_account(server_id, config);
        let store = Arc::clone(&self.inner.oauth_store);
        tokio::task::spawn_blocking(move || store.load(&account))
            .await
            .map_err(|_| McpError::OAuth(String::from("OAuth credential read failed")))?
            .map_err(McpError::OAuth)
    }

    async fn save_oauth_credentials(
        &self,
        account: String,
        credentials: McpOAuthCredentials,
    ) -> Result<(), McpError> {
        let store = Arc::clone(&self.inner.oauth_store);
        tokio::task::spawn_blocking(move || store.save(&account, &credentials))
            .await
            .map_err(|_| McpError::OAuth(String::from("OAuth credential save failed")))?
            .map_err(McpError::OAuth)
    }

    async fn disconnect_server(&self, server_id: &str) {
        let connection = self.inner.connections.lock().await.remove(server_id);
        if let Some(connection) = connection {
            connection.cancellation.cancel();
        }
    }
}

impl McpOAuthCredentials {
    fn is_expired(&self) -> bool {
        self.expires_at > 0.0 && unix_timestamp() >= self.expires_at
    }
}

async fn run_callback_listener(
    listener: tokio::net::TcpListener,
    manager: &McpManager,
    session: OAuthSession,
    callback_timeout: Duration,
) -> Result<(), McpError> {
    let deadline = tokio::time::Instant::now() + callback_timeout;
    for _ in 0..MAX_CALLBACK_ATTEMPTS {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        let accepted = tokio::time::timeout(remaining, listener.accept())
            .await
            .map_err(|_| McpError::OAuth(String::from("OAuth callback timed out")))?
            .map_err(|_| McpError::OAuth(String::from("OAuth callback could not be accepted")))?;
        let (mut stream, _) = accepted;
        match read_callback(&mut stream, &session).await {
            Ok(Some(code)) => {
                let client = oauth_http_client()?;
                let response = request_token(
                    &client,
                    &session.discovery.token_endpoint,
                    &[
                        ("grant_type", "authorization_code"),
                        ("code", &code),
                        ("client_id", &session.client_id),
                        ("redirect_uri", &session.redirect_uri),
                        ("code_verifier", &session.verifier),
                        ("resource", &session.discovery.resource),
                    ],
                )
                .await;
                match response.and_then(|response| credentials_from_token(&session, response)) {
                    Ok(credentials) => {
                        manager
                            .save_oauth_credentials(session.account.clone(), credentials)
                            .await?;
                        manager.disconnect_server(&session.server_id).await;
                        write_callback_response(&mut stream, 200, true).await;
                        return Ok(());
                    }
                    Err(error) => {
                        write_callback_response(&mut stream, 400, false).await;
                        return Err(error);
                    }
                }
            }
            Ok(None) => {
                write_callback_response(&mut stream, 400, false).await;
            }
            Err(error) => {
                write_callback_response(&mut stream, 400, false).await;
                return Err(error);
            }
        }
    }
    Err(McpError::OAuth(String::from("OAuth callback timed out")))
}

async fn bind_callback_listener() -> Result<(tokio::net::TcpListener, String), McpError> {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|_| McpError::OAuth(String::from("OAuth callback listener could not bind")))?;
    let callback_address = listener.local_addr().map_err(|_| {
        McpError::OAuth(String::from(
            "OAuth callback listener address is unavailable",
        ))
    })?;
    let redirect_uri = format!(
        "http://127.0.0.1:{}/oauth/callback",
        callback_address.port()
    );
    Ok((listener, redirect_uri))
}

async fn read_callback(
    stream: &mut tokio::net::TcpStream,
    session: &OAuthSession,
) -> Result<Option<String>, McpError> {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 2_048];
    loop {
        let count = tokio::time::timeout(OAUTH_TIMEOUT, stream.read(&mut chunk))
            .await
            .map_err(|_| McpError::OAuth(String::from("OAuth callback request timed out")))?
            .map_err(|_| {
                McpError::OAuth(String::from("OAuth callback request could not be read"))
            })?;
        if count == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..count]);
        if request.len() > MAX_CALLBACK_REQUEST_BYTES {
            return Err(McpError::OAuth(String::from(
                "OAuth callback request is too large",
            )));
        }
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    let request = std::str::from_utf8(&request)
        .map_err(|_| McpError::OAuth(String::from("OAuth callback request is invalid")))?;
    let first_line = request
        .lines()
        .next()
        .ok_or_else(|| McpError::OAuth(String::from("OAuth callback request is empty")))?;
    let mut parts = first_line.split_whitespace();
    if parts.next() != Some("GET") {
        return Ok(None);
    }
    let target = parts
        .next()
        .ok_or_else(|| McpError::OAuth(String::from("OAuth callback target is missing")))?;
    let url = Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|_| McpError::OAuth(String::from("OAuth callback URL is invalid")))?;
    if url.path() != "/oauth/callback" {
        return Ok(None);
    }
    let values = url
        .query_pairs()
        .collect::<std::collections::HashMap<_, _>>();
    if values.get("state").map(std::convert::AsRef::as_ref) != Some(session.state.as_str()) {
        return Ok(None);
    }
    let issuer = values.get("iss").map(std::convert::AsRef::as_ref);
    if session.discovery.authorization_response_issuer_required && issuer.is_none() {
        return Err(McpError::OAuth(String::from(
            "OAuth authorization response omitted the required issuer",
        )));
    }
    if issuer.is_some_and(|issuer| issuer != session.discovery.issuer) {
        return Err(McpError::OAuth(String::from(
            "OAuth authorization response issuer did not match discovery metadata",
        )));
    }
    if values.contains_key("error") {
        return Err(McpError::OAuth(String::from(
            "OAuth authorization server rejected the request",
        )));
    }
    Ok(values.get("code").map(ToString::to_string))
}

async fn write_callback_response(stream: &mut tokio::net::TcpStream, status: u16, success: bool) {
    let (reason, title, message) = if success {
        (
            "OK",
            "Authorization complete",
            "You can close this window and return to QwenPaw.",
        )
    } else {
        (
            "Bad Request",
            "Authorization failed",
            "Return to QwenPaw and start authorization again.",
        )
    };
    let body = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>{title}</title><style>body{{font-family:system-ui,sans-serif;display:grid;place-items:center;min-height:100vh;margin:0;background:#f5f5f2;color:#191919}}main{{max-width:34rem;padding:2.5rem;border:1px solid #d8d8d2;border-radius:1rem;background:white}}h1{{font-size:1.4rem}}p{{color:#5f5f59}}</style></head><body><main><h1>{title}</h1><p>{message}</p></main></body></html>"
    );
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Security-Policy: default-src 'none'; style-src 'unsafe-inline'\r\nReferrer-Policy: no-referrer\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

async fn discover_oauth(
    client: &reqwest::Client,
    resource: &str,
) -> Result<OAuthDiscovery, McpError> {
    let resource_url = Url::parse(resource)
        .map_err(|_| McpError::OAuth(String::from("MCP resource URL is invalid")))?;
    let challenge = probe_challenge(client, resource).await;
    let metadata_urls = protected_resource_metadata_urls(
        &resource_url,
        challenge.as_ref().and_then(|value| value.0.as_deref()),
    )?;
    let mut resource_metadata = None;
    for metadata_url in metadata_urls {
        if let Ok(metadata) = fetch_json::<ProtectedResourceMetadata>(client, &metadata_url).await {
            if metadata.resource != resource {
                continue;
            }
            resource_metadata = Some(metadata);
            break;
        }
    }
    let resource_metadata = resource_metadata.ok_or_else(|| {
        McpError::OAuth(String::from(
            "OAuth protected-resource metadata could not be discovered",
        ))
    })?;
    let issuer = resource_metadata
        .authorization_servers
        .first()
        .ok_or_else(|| {
            McpError::OAuth(String::from(
                "OAuth protected-resource metadata has no authorization server",
            ))
        })?
        .clone();
    validate_secure_oauth_url("authorization server", &issuer)?;
    let metadata = discover_authorization_server(client, &issuer).await?;
    if metadata.issuer != issuer {
        return Err(McpError::OAuth(String::from(
            "authorization-server metadata issuer did not match the discovered issuer",
        )));
    }
    if !metadata.code_challenge_methods_supported.is_empty()
        && !metadata
            .code_challenge_methods_supported
            .iter()
            .any(|method| method == "S256")
    {
        return Err(McpError::OAuth(String::from(
            "authorization server does not advertise PKCE S256",
        )));
    }
    validate_secure_oauth_url("authorization endpoint", &metadata.authorization_endpoint)?;
    validate_secure_oauth_url("token endpoint", &metadata.token_endpoint)?;
    if let Some(endpoint) = &metadata.registration_endpoint {
        validate_secure_oauth_url("registration endpoint", endpoint)?;
    }
    let challenge_scope = challenge.and_then(|value| value.1).unwrap_or_default();
    let scope = if challenge_scope.is_empty() {
        resource_metadata.scopes_supported.join(" ")
    } else {
        challenge_scope
    };
    Ok(OAuthDiscovery {
        issuer,
        resource: resource.to_owned(),
        authorization_endpoint: metadata.authorization_endpoint,
        token_endpoint: metadata.token_endpoint,
        registration_endpoint: metadata.registration_endpoint,
        scope,
        authorization_response_issuer_required: metadata
            .authorization_response_iss_parameter_supported,
    })
}

async fn probe_challenge(
    client: &reqwest::Client,
    resource: &str,
) -> Option<(Option<String>, Option<String>)> {
    let response = client.get(resource).send().await.ok()?;
    if response.status() != reqwest::StatusCode::UNAUTHORIZED {
        return None;
    }
    for value in response.headers().get_all(WWW_AUTHENTICATE) {
        let Ok(value) = value.to_str() else {
            continue;
        };
        if !value
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("bearer")
        {
            continue;
        }
        return Some((
            challenge_parameter(value, "resource_metadata"),
            challenge_parameter(value, "scope"),
        ));
    }
    None
}

fn protected_resource_metadata_urls(
    resource: &Url,
    challenged: Option<&str>,
) -> Result<Vec<String>, McpError> {
    let mut urls = Vec::new();
    if let Some(challenged) = challenged {
        validate_secure_oauth_url("resource metadata", challenged)?;
        urls.push(challenged.to_owned());
    }
    let origin = resource.origin().ascii_serialization();
    let suffix = resource.path().trim_start_matches('/');
    if !suffix.is_empty() {
        urls.push(format!(
            "{origin}/.well-known/oauth-protected-resource/{suffix}"
        ));
    }
    urls.push(format!("{origin}/.well-known/oauth-protected-resource"));
    urls.dedup();
    Ok(urls)
}

async fn discover_authorization_server(
    client: &reqwest::Client,
    issuer: &str,
) -> Result<AuthorizationServerMetadata, McpError> {
    let issuer_url = Url::parse(issuer)
        .map_err(|_| McpError::OAuth(String::from("authorization issuer URL is invalid")))?;
    let origin = issuer_url.origin().ascii_serialization();
    let suffix = issuer_url.path().trim_matches('/');
    let urls = if suffix.is_empty() {
        vec![
            format!("{origin}/.well-known/oauth-authorization-server"),
            format!("{origin}/.well-known/openid-configuration"),
        ]
    } else {
        vec![
            format!("{origin}/.well-known/oauth-authorization-server/{suffix}"),
            format!("{origin}/.well-known/openid-configuration/{suffix}"),
            format!("{origin}/{suffix}/.well-known/openid-configuration"),
        ]
    };
    for url in urls {
        if let Ok(metadata) = fetch_json::<AuthorizationServerMetadata>(client, &url).await {
            return Ok(metadata);
        }
    }
    Err(McpError::OAuth(String::from(
        "authorization-server metadata could not be discovered",
    )))
}

async fn dynamic_register(
    client: &reqwest::Client,
    endpoint: &str,
    redirect_uri: &str,
) -> Result<String, McpError> {
    let response = client
        .post(endpoint)
        .json(&serde_json::json!({
            "client_name": "QwenPaw MCP Client",
            "application_type": "native",
            "redirect_uris": [redirect_uri],
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "token_endpoint_auth_method": "none"
        }))
        .send()
        .await
        .map_err(|_| McpError::OAuth(String::from("dynamic client registration failed")))?;
    let status = response.status();
    if !matches!(status.as_u16(), 200 | 201) {
        return Err(McpError::OAuth(format!(
            "dynamic client registration returned HTTP {}",
            status.as_u16()
        )));
    }
    let registration: RegistrationResponse = decode_json_response(response).await?;
    if registration.client_id.trim().is_empty()
        || registration
            .token_endpoint_auth_method
            .as_deref()
            .is_some_and(|method| method != "none")
    {
        return Err(McpError::OAuth(String::from(
            "dynamic registration did not create a public native client",
        )));
    }
    Ok(registration.client_id)
}

async fn request_token(
    client: &reqwest::Client,
    endpoint: &str,
    values: &[(&str, &str)],
) -> Result<TokenResponse, McpError> {
    validate_secure_oauth_url("token endpoint", endpoint)?;
    let form = values
        .iter()
        .filter(|(_, value)| !value.is_empty())
        .copied()
        .collect::<Vec<_>>();
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
    let token: TokenResponse = decode_json_response(response).await?;
    if token
        .token_type
        .as_deref()
        .is_some_and(|value| !value.eq_ignore_ascii_case("bearer"))
    {
        return Err(McpError::OAuthRefresh(String::from(
            "token endpoint returned a non-Bearer token",
        )));
    }
    validate_bearer_token(&token.access_token)?;
    Ok(token)
}

fn credentials_from_token(
    session: &OAuthSession,
    response: TokenResponse,
) -> Result<McpOAuthCredentials, McpError> {
    let mut credentials = McpOAuthCredentials {
        issuer: session.discovery.issuer.clone(),
        resource: session.discovery.resource.clone(),
        client_id: session.client_id.clone(),
        authorization_endpoint: session.discovery.authorization_endpoint.clone(),
        token_endpoint: session.discovery.token_endpoint.clone(),
        scope: session.discovery.scope.clone(),
        access_token: String::new(),
        refresh_token: String::new(),
        expires_at: 0.0,
    };
    apply_token_response(&mut credentials, response)?;
    Ok(credentials)
}

fn apply_token_response(
    credentials: &mut McpOAuthCredentials,
    response: TokenResponse,
) -> Result<(), McpError> {
    validate_bearer_token(&response.access_token)?;
    credentials.access_token = response.access_token;
    if !response.refresh_token.is_empty() {
        validate_refresh_token(&response.refresh_token)?;
        credentials.refresh_token = response.refresh_token;
    }
    if let Some(scope) = response.scope {
        credentials.scope = scope;
    }
    credentials.expires_at = match response.expires_in.filter(|value| *value > 0) {
        Some(value) => {
            let value = u32::try_from(value).map_err(|_| {
                McpError::OAuthRefresh(String::from(
                    "token endpoint returned an invalid expiration",
                ))
            })?;
            unix_timestamp() + f64::from(value)
        }
        None => 0.0,
    };
    Ok(())
}

fn validate_refresh_token(value: &str) -> Result<(), McpError> {
    if value.is_empty()
        || value.len() > MAX_OAUTH_RESPONSE_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(McpError::OAuthRefresh(String::from(
            "token endpoint returned an invalid refresh token",
        )));
    }
    Ok(())
}

async fn fetch_json<T: DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
) -> Result<T, McpError> {
    validate_secure_oauth_url("OAuth metadata", url)?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|_| McpError::OAuth(String::from("OAuth metadata request failed")))?;
    if !response.status().is_success() {
        return Err(McpError::OAuth(format!(
            "OAuth metadata endpoint returned HTTP {}",
            response.status().as_u16()
        )));
    }
    decode_json_response(response).await
}

async fn decode_json_response<T: DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, McpError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_OAUTH_RESPONSE_BYTES as u64)
    {
        return Err(McpError::OAuth(String::from("OAuth response is too large")));
    }
    let mut bytes = Vec::new();
    let mut response = response;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| McpError::OAuth(String::from("OAuth response could not be read")))?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_OAUTH_RESPONSE_BYTES {
            return Err(McpError::OAuth(String::from("OAuth response is too large")));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| McpError::OAuth(String::from("OAuth response is invalid JSON")))
}

fn oauth_http_client() -> Result<reqwest::Client, McpError> {
    reqwest::Client::builder()
        .connect_timeout(OAUTH_TIMEOUT)
        .timeout(OAUTH_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| McpError::OAuth(String::from("OAuth HTTP client could not be created")))
}

fn validate_secure_oauth_url(label: &str, value: &str) -> Result<(), McpError> {
    validate_http_url(label, value)?;
    let url = Url::parse(value).map_err(|_| McpError::OAuth(format!("{label} URL is invalid")))?;
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        return Err(McpError::OAuth(format!(
            "{label} must use HTTPS unless it is a literal loopback URL"
        )));
    }
    Ok(())
}

fn validate_endpoint_override(
    label: &str,
    provided: &str,
    discovered: &str,
) -> Result<(), McpError> {
    if provided.is_empty() || provided == discovered {
        return Ok(());
    }
    Err(McpError::OAuth(format!(
        "{label} endpoint override did not match validated discovery metadata"
    )))
}

fn challenge_parameter(value: &str, expected: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        while index < bytes.len() && (bytes[index].is_ascii_whitespace() || bytes[index] == b',') {
            index += 1;
        }
        let start = index;
        while index < bytes.len()
            && !bytes[index].is_ascii_whitespace()
            && !matches!(bytes[index], b'=' | b',')
        {
            index += 1;
        }
        let name = &value[start..index];
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() || bytes[index] != b'=' {
            while index < bytes.len() && bytes[index] != b',' {
                index += 1;
            }
            continue;
        }
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let parsed = if index < bytes.len() && bytes[index] == b'"' {
            index += 1;
            let mut result = String::new();
            while index < bytes.len() && bytes[index] != b'"' {
                if bytes[index] == b'\\' && index + 1 < bytes.len() {
                    index += 1;
                }
                result.push(char::from(bytes[index]));
                index += 1;
            }
            index = index.saturating_add(1);
            result
        } else {
            let value_start = index;
            while index < bytes.len() && !bytes[index].is_ascii_whitespace() && bytes[index] != b','
            {
                index += 1;
            }
            value[value_start..index].to_owned()
        };
        if name.eq_ignore_ascii_case(expected) {
            return Some(parsed);
        }
    }
    None
}

fn oauth_account(server_id: &str, config: &McpClientConfig) -> String {
    format!(
        "mcp-{:x}",
        Sha256::digest(format!("{server_id}\0{}", config.url).as_bytes())
    )
}

fn random_url_token(bytes: usize) -> String {
    let mut value = vec![0_u8; bytes];
    rand::rng().fill_bytes(&mut value);
    URL_SAFE_NO_PAD.encode(value)
}

fn first_non_empty<const N: usize>(values: [&str; N]) -> &str {
    values
        .into_iter()
        .find(|value| !value.is_empty())
        .unwrap_or("")
}

fn unix_timestamp() -> f64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0.0, |duration| duration.as_secs_f64())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bearer_challenge_parameters() {
        let value = "Bearer realm=\"mcp\", resource_metadata=\"https://mcp.example/.well-known/oauth-protected-resource\", scope=\"files:read files:write\"";
        assert_eq!(
            challenge_parameter(value, "resource_metadata").as_deref(),
            Some("https://mcp.example/.well-known/oauth-protected-resource")
        );
        assert_eq!(
            challenge_parameter(value, "scope").as_deref(),
            Some("files:read files:write")
        );
    }

    #[test]
    fn requires_https_except_for_literal_loopback_urls() {
        assert!(validate_secure_oauth_url("test", "https://example.com/oauth").is_ok());
        assert!(validate_secure_oauth_url("test", "http://127.0.0.1:3000/oauth").is_ok());
        assert!(validate_secure_oauth_url("test", "http://localhost:3000/oauth").is_ok());
        assert!(validate_secure_oauth_url("test", "http://example.com/oauth").is_err());
    }

    #[tokio::test]
    async fn bounds_the_loopback_callback_wait() {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("test callback listener should bind");
        let session = OAuthSession {
            server_id: String::from("remote"),
            account: String::from("account"),
            state: String::from("state"),
            verifier: String::from("verifier"),
            redirect_uri: String::from("http://127.0.0.1/oauth/callback"),
            client_id: String::from("client"),
            discovery: OAuthDiscovery {
                issuer: String::from("https://issuer.example"),
                resource: String::from("https://mcp.example"),
                authorization_endpoint: String::from("https://issuer.example/authorize"),
                token_endpoint: String::from("https://issuer.example/token"),
                registration_endpoint: None,
                scope: String::new(),
                authorization_response_issuer_required: true,
            },
        };
        let error = run_callback_listener(
            listener,
            &McpManager::empty(),
            session,
            Duration::from_millis(1),
        )
        .await
        .expect_err("idle callback listener should time out");
        assert_eq!(
            error.to_string(),
            "MCP OAuth failed: OAuth callback timed out"
        );
    }
}
