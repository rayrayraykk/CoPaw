use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use axum::Form;
use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::HeaderValue;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::routing::post;
use futures_util::SinkExt;
use futures_util::StreamExt;
use qwenpaw_app_server::AppServer;
use qwenpaw_app_server::DesktopCredentialStore;
use qwenpaw_core::Core;
use qwenpaw_core::McpManager;
use qwenpaw_core::McpOAuthCredentialStore;
use qwenpaw_core::McpOAuthCredentials;
use qwenpaw_core::ModelConfig;
use serde_json::Value;
use serde_json::json;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;
use url::Url;

#[derive(Default)]
struct MemoryOAuthStore {
    values: std::sync::Mutex<HashMap<String, McpOAuthCredentials>>,
}

impl McpOAuthCredentialStore for MemoryOAuthStore {
    fn load(&self, account: &str) -> Result<Option<McpOAuthCredentials>, String> {
        Ok(self
            .values
            .lock()
            .map_err(|_| String::from("OAuth credential lock failed"))?
            .get(account)
            .cloned())
    }

    fn save(&self, account: &str, credentials: &McpOAuthCredentials) -> Result<(), String> {
        self.values
            .lock()
            .map_err(|_| String::from("OAuth credential lock failed"))?
            .insert(account.to_owned(), credentials.clone());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        self.values
            .lock()
            .map_err(|_| String::from("OAuth credential lock failed"))?
            .remove(account);
        Ok(())
    }
}

struct EmptyModelCredentialStore;

fn new_isolated_desktop(
    core: Core,
    console_static_dir: &Path,
    desktop_shutdown_token: String,
    desktop_credentials: Arc<dyn DesktopCredentialStore>,
    desktop_data_dir: &Path,
) -> anyhow::Result<AppServer> {
    AppServer::new_desktop_with_stores_and_workspace(
        core,
        console_static_dir,
        desktop_shutdown_token,
        desktop_credentials,
        desktop_data_dir,
        desktop_data_dir,
    )
}

impl DesktopCredentialStore for EmptyModelCredentialStore {
    fn load_api_key(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }

    fn save_api_key(&self, _api_key: Option<&str>) -> anyhow::Result<()> {
        Ok(())
    }
}

#[derive(Clone)]
struct OAuthFixtureState {
    origin: String,
    token_forms: Arc<Mutex<Vec<HashMap<String, String>>>>,
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn serves_console_and_app_protocol_mcp_oauth_contracts() {
    let (oauth_origin, fixture_state, oauth_task) = start_oauth_fixture().await;
    let directory = tempfile::tempdir().expect("temporary test directory should be created");
    let config_path = write_mcp_config(directory.path(), &oauth_origin);
    let mcp =
        McpManager::from_path_with_oauth_store(&config_path, Arc::new(MemoryOAuthStore::default()))
            .expect("MCP config should load");
    let core = Core::new_with_mcp(
        ModelConfig {
            api_key: None,
            base_url: String::from("http://127.0.0.1:1"),
            default_model: String::from("qwen-test"),
        },
        mcp,
    );
    let console = directory.path().join("console");
    let desktop_data = directory.path().join("desktop-data");
    std::fs::create_dir_all(&console).expect("Console directory should be created");
    std::fs::write(console.join("index.html"), "<html>console</html>")
        .expect("Console index should be written");
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("Desktop listener should bind");
    let desktop_address = listener
        .local_addr()
        .expect("Desktop listener address should resolve");
    let server = new_isolated_desktop(
        core,
        &console,
        String::from("desktop-oauth-token"),
        Arc::new(EmptyModelCredentialStore),
        &desktop_data,
    )
    .expect("Desktop server should configure");
    let desktop_task = tokio::spawn(server.run_http(listener));
    let client = reqwest::Client::new();

    let clients = client
        .get(format!("http://{desktop_address}/api/mcp"))
        .send()
        .await
        .expect("MCP client list should send")
        .json::<Value>()
        .await
        .expect("MCP client list should be JSON");
    assert_eq!(clients[0]["key"], json!("remote"));
    assert_eq!(clients[0]["name"], json!("Remote fixture"));
    assert_eq!(clients[0]["oauth_status"]["authorized"], json!(false));

    let started = client
        .post(format!(
            "http://{desktop_address}/api/mcp/oauth/start/remote"
        ))
        .json(&json!({
            "url": format!("{oauth_origin}/mcp"),
            "scope": "files:read",
            "client_id": "desktop-client",
            "auth_endpoint": "",
            "token_endpoint": ""
        }))
        .send()
        .await
        .expect("MCP OAuth start should send");
    assert_eq!(started.status(), reqwest::StatusCode::OK);
    let started = started
        .json::<Value>()
        .await
        .expect("MCP OAuth start should be JSON");
    let authorization_url = Url::parse(
        started["auth_url"]
            .as_str()
            .expect("OAuth start should return auth_url"),
    )
    .expect("authorization URL should parse");
    let query = authorization_url.query_pairs().collect::<HashMap<_, _>>();
    assert_eq!(query["client_id"], "desktop-client");
    assert_eq!(query["resource"], format!("{oauth_origin}/mcp"));
    let mut callback = Url::parse(&query["redirect_uri"]).expect("OAuth redirect URI should parse");
    callback
        .query_pairs_mut()
        .append_pair("code", "desktop-code")
        .append_pair(
            "state",
            started["session_id"]
                .as_str()
                .expect("OAuth start should return session_id"),
        )
        .append_pair("iss", &oauth_origin);
    let callback = client
        .get(callback)
        .send()
        .await
        .expect("OAuth callback should send");
    assert_eq!(callback.status(), reqwest::StatusCode::OK);

    let status = client
        .get(format!(
            "http://{desktop_address}/api/mcp/oauth/status/remote"
        ))
        .send()
        .await
        .expect("MCP OAuth status should send")
        .json::<Value>()
        .await
        .expect("MCP OAuth status should be JSON");
    assert_eq!(
        status,
        json!({
            "authorized": true,
            "expires_at": status["expires_at"],
            "scope": "files:read"
        })
    );
    assert!(
        status["expires_at"]
            .as_f64()
            .is_some_and(|value| value > 0.0)
    );
    assert_eq!(
        fixture_state.token_forms.lock().await[0]["resource"],
        format!("{oauth_origin}/mcp")
    );

    let revoked = client
        .delete(format!("http://{desktop_address}/api/mcp/oauth/remote"))
        .send()
        .await
        .expect("MCP OAuth revoke should send");
    assert_eq!(revoked.status(), reqwest::StatusCode::OK);
    let status = client
        .get(format!(
            "http://{desktop_address}/api/mcp/oauth/status/remote"
        ))
        .send()
        .await
        .expect("revoked MCP OAuth status should send")
        .json::<Value>()
        .await
        .expect("revoked MCP OAuth status should be JSON");
    assert_eq!(status["authorized"], json!(false));

    let (mut socket, _) =
        tokio_tungstenite::connect_async(format!("ws://{desktop_address}/app-protocol"))
            .await
            .expect("App Protocol WebSocket should connect");
    send_json(
        &mut socket,
        json!({
            "id": 1,
            "method": "initialize",
            "params": {"clientInfo": {"name": "oauth_test", "version": "0.1.0"}}
        }),
    )
    .await;
    assert_eq!(
        receive_json(&mut socket).await["result"]["protocolVersion"],
        3
    );
    send_json(
        &mut socket,
        json!({"id": 2, "method": "mcp/list", "params": {}}),
    )
    .await;
    let listed = receive_json(&mut socket).await;
    assert_eq!(
        listed["result"]["data"][0],
        json!({
            "serverId": "remote",
            "name": "Remote fixture",
            "description": "",
            "enabled": true,
            "transport": "streamable_http",
            "url": format!("{oauth_origin}/mcp"),
            "oauthStatus": {
                "authorized": false,
                "expiresAt": 0.0,
                "scope": "",
                "clientId": ""
            }
        })
    );
    send_json(
        &mut socket,
        json!({
            "id": 3,
            "method": "mcp/oauth/start",
            "params": {
                "serverId": "remote",
                "scope": "files:read",
                "clientId": "vscode-client"
            }
        }),
    )
    .await;
    let started = receive_json(&mut socket).await;
    let authorization_url = Url::parse(
        started["result"]["authorizationUrl"]
            .as_str()
            .expect("App Protocol should return an authorization URL"),
    )
    .expect("App Protocol authorization URL should parse");
    let query = authorization_url.query_pairs().collect::<HashMap<_, _>>();
    assert_eq!(query["client_id"], "vscode-client");
    let mut callback =
        Url::parse(&query["redirect_uri"]).expect("App Protocol redirect URI should parse");
    callback
        .query_pairs_mut()
        .append_pair("code", "vscode-code")
        .append_pair(
            "state",
            started["result"]["sessionId"]
                .as_str()
                .expect("App Protocol should return a session ID"),
        )
        .append_pair("iss", &oauth_origin);
    assert_eq!(
        client
            .get(callback)
            .send()
            .await
            .expect("App Protocol OAuth callback should send")
            .status(),
        reqwest::StatusCode::OK
    );
    send_json(
        &mut socket,
        json!({
            "id": 4,
            "method": "mcp/oauth/status",
            "params": {"serverId": "remote"}
        }),
    )
    .await;
    let status = receive_json(&mut socket).await;
    assert_eq!(status["result"]["status"]["authorized"], true);
    assert_eq!(status["result"]["status"]["clientId"], "vscode-client");
    send_json(
        &mut socket,
        json!({
            "id": 5,
            "method": "mcp/oauth/revoke",
            "params": {"serverId": "remote"}
        }),
    )
    .await;
    assert_eq!(
        receive_json(&mut socket).await,
        json!({"id": 5, "result": {"revoked": true}})
    );
    socket.close(None).await.expect("App Protocol should close");

    desktop_task.abort();
    oauth_task.abort();
}

async fn send_json<S>(socket: &mut tokio_tungstenite::WebSocketStream<S>, value: Value)
where
    tokio_tungstenite::WebSocketStream<S>: futures_util::Sink<Message> + Unpin,
    <tokio_tungstenite::WebSocketStream<S> as futures_util::Sink<Message>>::Error: std::fmt::Debug,
{
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .expect("App Protocol request should send");
}

async fn receive_json<S>(socket: &mut tokio_tungstenite::WebSocketStream<S>) -> Value
where
    tokio_tungstenite::WebSocketStream<S>:
        futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let message = socket
        .next()
        .await
        .expect("App Protocol should send a response")
        .expect("App Protocol response should be valid");
    serde_json::from_str(
        message
            .to_text()
            .expect("App Protocol response should be text"),
    )
    .expect("App Protocol response should be JSON")
}

async fn start_oauth_fixture() -> (
    String,
    OAuthFixtureState,
    tokio::task::JoinHandle<Result<(), std::io::Error>>,
) {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("OAuth listener should bind");
    let origin = format!(
        "http://{}",
        listener.local_addr().expect("OAuth address should resolve")
    );
    let state = OAuthFixtureState {
        origin: origin.clone(),
        token_forms: Arc::new(Mutex::new(Vec::new())),
    };
    let router = Router::new()
        .route("/mcp", get(oauth_challenge))
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(resource_metadata),
        )
        .route(
            "/.well-known/oauth-authorization-server",
            get(server_metadata),
        )
        .route("/token", post(token))
        .with_state(state.clone());
    let task = tokio::spawn(async move { axum::serve(listener, router).await });
    (origin, state, task)
}

async fn oauth_challenge(State(state): State<OAuthFixtureState>) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::WWW_AUTHENTICATE,
        HeaderValue::from_str(&format!(
            "Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource/mcp\", scope=\"files:read\"",
            state.origin
        ))
        .expect("OAuth challenge should be valid"),
    );
    (StatusCode::UNAUTHORIZED, headers)
}

async fn resource_metadata(State(state): State<OAuthFixtureState>) -> Json<Value> {
    Json(json!({
        "resource": format!("{}/mcp", state.origin),
        "authorization_servers": [state.origin],
        "scopes_supported": ["files:read"]
    }))
}

async fn server_metadata(State(state): State<OAuthFixtureState>) -> Json<Value> {
    Json(json!({
        "issuer": state.origin,
        "authorization_endpoint": format!("{}/authorize", state.origin),
        "token_endpoint": format!("{}/token", state.origin),
        "code_challenge_methods_supported": ["S256"],
        "authorization_response_iss_parameter_supported": true
    }))
}

async fn token(
    State(state): State<OAuthFixtureState>,
    Form(form): Form<HashMap<String, String>>,
) -> Json<Value> {
    state.token_forms.lock().await.push(form);
    Json(json!({
        "access_token": "desktop-access-secret",
        "refresh_token": "desktop-refresh-secret",
        "token_type": "Bearer",
        "expires_in": 3600,
        "scope": "files:read"
    }))
}

fn write_mcp_config(directory: &Path, origin: &str) -> std::path::PathBuf {
    let path = directory.join("mcp.json");
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&json!({
            "clients": {
                "remote": {
                    "name": "Remote fixture",
                    "transport": "streamable_http",
                    "url": format!("{origin}/mcp"),
                    "oauth": {"clientId": "desktop-client"}
                }
            }
        }))
        .expect("MCP config should encode"),
    )
    .expect("MCP config should be written");
    path
}
