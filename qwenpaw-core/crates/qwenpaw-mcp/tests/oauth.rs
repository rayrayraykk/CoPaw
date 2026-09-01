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
use qwenpaw_mcp::McpManager;
use qwenpaw_mcp::McpOAuthCredentialStore;
use qwenpaw_mcp::McpOAuthCredentials;
use qwenpaw_mcp::McpOAuthStartOptions;
use serde_json::Value;
use serde_json::json;
use tokio::sync::Mutex;
use url::Url;

#[derive(Default)]
struct MemoryCredentialStore {
    values: std::sync::Mutex<HashMap<String, McpOAuthCredentials>>,
}

impl McpOAuthCredentialStore for MemoryCredentialStore {
    fn load(&self, account: &str) -> Result<Option<McpOAuthCredentials>, String> {
        Ok(self
            .values
            .lock()
            .map_err(|_| String::from("credential lock failed"))?
            .get(account)
            .cloned())
    }

    fn save(&self, account: &str, credentials: &McpOAuthCredentials) -> Result<(), String> {
        self.values
            .lock()
            .map_err(|_| String::from("credential lock failed"))?
            .insert(account.to_owned(), credentials.clone());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        self.values
            .lock()
            .map_err(|_| String::from("credential lock failed"))?
            .remove(account);
        Ok(())
    }
}

#[derive(Clone)]
struct OAuthServerState {
    origin: String,
    registration: Arc<Mutex<Option<Value>>>,
    token_forms: Arc<Mutex<Vec<HashMap<String, String>>>>,
}

#[tokio::test]
async fn completes_discovers_persists_and_revokes_interactive_oauth() {
    let (origin, server_state, server_task) = start_oauth_server().await;
    let directory = tempfile::tempdir().expect("temporary config directory should be created");
    let config_path = write_oauth_config(directory.path(), &origin);
    let credentials = Arc::new(MemoryCredentialStore::default());
    let manager = McpManager::from_path_with_oauth_store(&config_path, credentials)
        .expect("OAuth MCP config should load");

    let started = manager
        .start_oauth("remote", McpOAuthStartOptions::default())
        .await
        .expect("interactive OAuth should start");
    let authorization_url =
        Url::parse(&started.authorization_url).expect("authorization URL should be valid");
    assert_eq!(authorization_url.path(), "/authorize");
    let query = authorization_url.query_pairs().collect::<HashMap<_, _>>();
    assert_eq!(
        query.get("client_id").map(std::convert::AsRef::as_ref),
        Some("dynamic-client")
    );
    assert_eq!(
        query.get("resource").map(std::convert::AsRef::as_ref),
        Some(format!("{origin}/mcp").as_str())
    );
    assert_eq!(
        query
            .get("code_challenge_method")
            .map(std::convert::AsRef::as_ref),
        Some("S256")
    );
    assert_eq!(
        query.get("state").map(std::convert::AsRef::as_ref),
        Some(started.session_id.as_str())
    );

    let redirect_uri = query
        .get("redirect_uri")
        .expect("authorization URL should contain a redirect URI");
    let wrong_state_callback =
        callback_url(redirect_uri, "wrong-state", "must-not-be-redeemed", &origin);
    let wrong_state_response = reqwest::get(wrong_state_callback)
        .await
        .expect("wrong-state OAuth callback should send");
    assert_eq!(
        wrong_state_response.status(),
        reqwest::StatusCode::BAD_REQUEST
    );
    assert!(server_state.token_forms.lock().await.is_empty());
    let callback = callback_url(
        redirect_uri,
        &started.session_id,
        "authorization-code",
        &origin,
    );
    let response = reqwest::get(callback)
        .await
        .expect("OAuth callback should send");
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let status = manager
        .oauth_status("remote")
        .await
        .expect("OAuth status should load");
    assert!(status.authorized);
    assert_eq!(status.client_id, "dynamic-client");
    assert_eq!(status.scope, "files:read");

    let registration = server_state
        .registration
        .lock()
        .await
        .clone()
        .expect("dynamic registration request should be captured");
    assert_eq!(registration["application_type"], json!("native"));
    assert_eq!(registration["token_endpoint_auth_method"], json!("none"));
    let token_forms = server_state.token_forms.lock().await;
    assert_eq!(token_forms.len(), 1);
    assert_eq!(token_forms[0]["grant_type"], "authorization_code");
    assert_eq!(token_forms[0]["resource"], format!("{origin}/mcp"));
    assert_eq!(token_forms[0]["client_id"], "dynamic-client");
    assert!(!token_forms[0]["code_verifier"].is_empty());
    drop(token_forms);

    manager
        .revoke_oauth("remote")
        .await
        .expect("OAuth credential should revoke");
    assert!(
        !manager
            .oauth_status("remote")
            .await
            .expect("revoked OAuth status should load")
            .authorized
    );
    server_task.abort();
}

#[tokio::test]
async fn rejects_an_authorization_response_from_the_wrong_issuer() {
    let (origin, _, server_task) = start_oauth_server().await;
    let directory = tempfile::tempdir().expect("temporary config directory should be created");
    let config_path = write_oauth_config(directory.path(), &origin);
    let manager = McpManager::from_path_with_oauth_store(
        &config_path,
        Arc::new(MemoryCredentialStore::default()),
    )
    .expect("OAuth MCP config should load");
    let started = manager
        .start_oauth("remote", McpOAuthStartOptions::default())
        .await
        .expect("interactive OAuth should start");
    let authorization_url =
        Url::parse(&started.authorization_url).expect("authorization URL should be valid");
    let query = authorization_url.query_pairs().collect::<HashMap<_, _>>();
    let callback = callback_url(
        query
            .get("redirect_uri")
            .expect("authorization URL should contain a redirect URI"),
        &started.session_id,
        "must-not-be-redeemed",
        "https://wrong-issuer.example",
    );
    let response = reqwest::get(callback)
        .await
        .expect("OAuth callback should send");
    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    assert!(
        !manager
            .oauth_status("remote")
            .await
            .expect("OAuth status should load")
            .authorized
    );
    server_task.abort();
}

async fn start_oauth_server() -> (
    String,
    OAuthServerState,
    tokio::task::JoinHandle<Result<(), std::io::Error>>,
) {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("OAuth fixture listener should bind");
    let origin = format!(
        "http://{}",
        listener
            .local_addr()
            .expect("OAuth fixture address should resolve")
    );
    let state = OAuthServerState {
        origin: origin.clone(),
        registration: Arc::new(Mutex::new(None)),
        token_forms: Arc::new(Mutex::new(Vec::new())),
    };
    let router = Router::new()
        .route("/mcp", get(mcp_challenge))
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(protected_resource_metadata),
        )
        .route(
            "/.well-known/oauth-authorization-server",
            get(authorization_server_metadata),
        )
        .route("/register", post(register_client))
        .route("/token", post(exchange_token))
        .with_state(state.clone());
    let task = tokio::spawn(async move { axum::serve(listener, router).await });
    (origin, state, task)
}

async fn mcp_challenge(State(state): State<OAuthServerState>) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::WWW_AUTHENTICATE,
        HeaderValue::from_str(&format!(
            "Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource/mcp\", scope=\"files:read\"",
            state.origin
        ))
        .expect("challenge header should be valid"),
    );
    (StatusCode::UNAUTHORIZED, headers)
}

async fn protected_resource_metadata(State(state): State<OAuthServerState>) -> Json<Value> {
    Json(json!({
        "resource": format!("{}/mcp", state.origin),
        "authorization_servers": [state.origin],
        "scopes_supported": ["files:read"]
    }))
}

async fn authorization_server_metadata(State(state): State<OAuthServerState>) -> Json<Value> {
    Json(json!({
        "issuer": state.origin,
        "authorization_endpoint": format!("{}/authorize", state.origin),
        "token_endpoint": format!("{}/token", state.origin),
        "registration_endpoint": format!("{}/register", state.origin),
        "code_challenge_methods_supported": ["S256"],
        "authorization_response_iss_parameter_supported": true
    }))
}

async fn register_client(
    State(state): State<OAuthServerState>,
    Json(body): Json<Value>,
) -> Json<Value> {
    *state.registration.lock().await = Some(body);
    Json(json!({
        "client_id": "dynamic-client",
        "token_endpoint_auth_method": "none"
    }))
}

async fn exchange_token(
    State(state): State<OAuthServerState>,
    Form(form): Form<HashMap<String, String>>,
) -> Json<Value> {
    state.token_forms.lock().await.push(form);
    Json(json!({
        "access_token": "access-secret",
        "refresh_token": "refresh-secret",
        "token_type": "Bearer",
        "expires_in": 3600,
        "scope": "files:read"
    }))
}

fn write_oauth_config(directory: &Path, origin: &str) -> std::path::PathBuf {
    let path = directory.join("oauth.json");
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&json!({
            "clients": {
                "remote": {
                    "transport": "streamable_http",
                    "url": format!("{origin}/mcp"),
                    "oauth": {}
                }
            }
        }))
        .expect("OAuth config should encode"),
    )
    .expect("OAuth config should be written");
    path
}

fn callback_url(redirect_uri: &str, state: &str, code: &str, issuer: &str) -> Url {
    let mut callback = Url::parse(redirect_uri).expect("redirect URI should be valid");
    callback
        .query_pairs_mut()
        .append_pair("code", code)
        .append_pair("state", state)
        .append_pair("iss", issuer);
    callback
}
