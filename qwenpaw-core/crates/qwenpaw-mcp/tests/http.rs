use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::response::sse::Event;
use axum::response::sse::Sse;
use axum::routing::get;
use axum::routing::post;
use futures_util::StreamExt;
use futures_util::stream;
use pretty_assertions::assert_eq;
use qwenpaw_mcp::McpManager;
use serde_json::Value;
use serde_json::json;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

#[derive(Clone)]
struct TestState {
    authenticated_requests: Arc<AtomicUsize>,
    token_requests: Arc<AtomicUsize>,
    expected_authorization: Arc<str>,
}

impl TestState {
    fn new(expected_authorization: &str) -> Self {
        Self {
            authenticated_requests: Arc::new(AtomicUsize::new(0)),
            token_requests: Arc::new(AtomicUsize::new(0)),
            expected_authorization: Arc::from(expected_authorization),
        }
    }
}

#[derive(Clone, Default)]
struct LegacyState {
    messages: Arc<Mutex<Option<mpsc::UnboundedSender<Value>>>>,
}

#[tokio::test]
async fn discovers_calls_and_cancels_streamable_http_tools() {
    let state = TestState::new("Bearer secret-token");
    let router = Router::new()
        .route("/mcp", post(handle_mcp))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener should bind");
    let address = listener
        .local_addr()
        .expect("test listener should have an address");
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("test MCP server should run");
    });
    let directory = tempfile::tempdir().expect("temporary directory");
    let config_path = directory.path().join("mcp.json");
    std::fs::write(
        &config_path,
        serde_json::to_vec(&json!({
            "clients": {
                "remote": {
                    "name": "remote",
                    "enabled": true,
                    "type": "http",
                    "baseUrl": format!("http://{address}/mcp"),
                    "headers": {"X-QwenPaw-Test": "network"},
                    "oauth": {"accessToken": "secret-token"},
                    "tools": ["echo"]
                }
            }
        }))
        .expect("config should serialize"),
    )
    .expect("config should write");
    let manager = McpManager::from_path(&config_path).expect("config should load");

    let tools = manager.tools("remote").await.expect("tools should list");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "echo");
    assert_eq!(tools[0].description, "Echo over HTTP");
    assert!(tools[0].enabled);
    assert_eq!(
        tools[0].input_schema,
        json!({
            "type": "object",
            "properties": {"text": {"type": "string"}},
            "required": ["text"]
        })
    );

    assert_eq!(
        manager.definitions().await,
        vec![json!({
            "type": "function",
            "function": {
                "name": "mcp__remote__echo",
                "description": "Echo over HTTP",
                "parameters": {
                    "type": "object",
                    "properties": {"text": {"type": "string"}},
                    "required": ["text"]
                }
            }
        })]
    );
    assert_eq!(
        manager
            .call_tool("mcp__remote__echo", r#"{"text":"hello"}"#)
            .await
            .expect("HTTP tool should run"),
        qwenpaw_mcp::McpToolOutput {
            content: String::from("{\"echo\":\"hello\"}"),
            is_error: false,
        }
    );

    let running_manager = manager.clone();
    let running = tokio::spawn(async move {
        running_manager
            .call_tool("mcp__remote__echo", r#"{"text":"slow"}"#)
            .await
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    manager.cancel_tool("mcp__remote__echo").await;
    let result = tokio::time::timeout(Duration::from_secs(2), running)
        .await
        .expect("cancelled HTTP call should return promptly")
        .expect("call task should join");
    assert!(result.is_err());
    assert!(state.authenticated_requests.load(Ordering::Relaxed) >= 4);
    server.abort();
}

#[tokio::test]
async fn refreshes_an_expired_oauth_token_before_connecting() {
    let state = TestState::new("Bearer refreshed-token");
    let router = Router::new()
        .route("/mcp", post(handle_mcp))
        .route("/token", post(handle_token))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener should bind");
    let address = listener
        .local_addr()
        .expect("test listener should have an address");
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("test OAuth MCP server should run");
    });
    let directory = tempfile::tempdir().expect("temporary directory");
    let config_path = directory.path().join("oauth-mcp.json");
    std::fs::write(
        &config_path,
        serde_json::to_vec(&json!({
            "clients": {
                "oauth": {
                    "enabled": true,
                    "transport": "streamable_http",
                    "url": format!("http://{address}/mcp"),
                    "headers": {"X-QwenPaw-Test": "network"},
                    "oauth": {
                        "clientId": "qwenpaw-test",
                        "scope": "tools.read",
                        "accessToken": "expired-token",
                        "refreshToken": "refresh-secret",
                        "expiresAt": 1,
                        "tokenEndpoint": format!("http://{address}/token")
                    }
                }
            }
        }))
        .expect("config should serialize"),
    )
    .expect("config should write");
    let manager = McpManager::from_path(&config_path).expect("config should load");

    assert_eq!(manager.definitions().await.len(), 1);
    assert_eq!(state.token_requests.load(Ordering::Relaxed), 1);
    assert!(state.authenticated_requests.load(Ordering::Relaxed) >= 3);
    server.abort();
}

#[tokio::test]
async fn discovers_and_calls_legacy_sse_tools() {
    let state = LegacyState::default();
    let router = Router::new()
        .route("/sse", get(legacy_sse))
        .route("/messages", post(legacy_post))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener should bind");
    let address = listener
        .local_addr()
        .expect("test listener should have an address");
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("test SSE MCP server should run");
    });
    let directory = tempfile::tempdir().expect("temporary directory");
    let config_path = directory.path().join("mcp.json");
    std::fs::write(
        &config_path,
        serde_json::to_vec(&json!({
            "clients": {
                "legacy": {
                    "enabled": true,
                    "transport": "sse",
                    "url": format!("http://{address}/sse"),
                    "headers": {
                        "Authorization": "Bearer legacy-token",
                        "X-QwenPaw-Test": "legacy"
                    }
                }
            }
        }))
        .expect("config should serialize"),
    )
    .expect("config should write");
    let manager = McpManager::from_path(&config_path).expect("config should load");

    assert_eq!(
        manager.definitions().await,
        vec![json!({
            "type": "function",
            "function": {
                "name": "mcp__legacy__echo",
                "description": "Echo over legacy SSE",
                "parameters": {
                    "type": "object",
                    "properties": {"text": {"type": "string"}},
                    "required": ["text"]
                }
            }
        })]
    );
    assert_eq!(
        manager
            .call_tool("mcp__legacy__echo", r#"{"text":"legacy"}"#)
            .await
            .expect("legacy SSE tool should run"),
        qwenpaw_mcp::McpToolOutput {
            content: String::from("{\"echo\":\"legacy\"}"),
            is_error: false,
        }
    );
    manager.cancel_tool("mcp__legacy__echo").await;
    server.abort();
}

async fn handle_mcp(
    State(state): State<TestState>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> Response {
    assert_eq!(
        headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some(state.expected_authorization.as_ref())
    );
    assert_eq!(
        headers
            .get("x-qwenpaw-test")
            .and_then(|value| value.to_str().ok()),
        Some("network")
    );
    state.authenticated_requests.fetch_add(1, Ordering::Relaxed);
    let Some(id) = request.get("id").cloned() else {
        return StatusCode::ACCEPTED.into_response();
    };
    let result = match request.get("method").and_then(Value::as_str) {
        Some("initialize") => json!({
            "protocolVersion": "2025-03-26",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "http-test", "version": "0.1.0"}
        }),
        Some("tools/list") => json!({
            "tools": [{
                "name": "echo",
                "description": "Echo over HTTP",
                "inputSchema": {
                    "type": "object",
                    "properties": {"text": {"type": "string"}},
                    "required": ["text"]
                }
            }]
        }),
        Some("tools/call") => {
            let text = request["params"]["arguments"]["text"]
                .as_str()
                .unwrap_or_default();
            if text == "slow" {
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
            json!({
                "content": [{"type": "text", "text": text}],
                "structuredContent": {"echo": text},
                "isError": false
            })
        }
        method => panic!("unexpected MCP method: {method:?}"),
    };
    Json(json!({"jsonrpc": "2.0", "id": id, "result": result})).into_response()
}

async fn handle_token(State(state): State<TestState>, body: axum::body::Bytes) -> Response {
    let body = String::from_utf8(body.to_vec()).expect("token form should be UTF-8");
    assert!(body.contains("grant_type=refresh_token"));
    assert!(body.contains("client_id=qwenpaw-test"));
    assert!(body.contains("refresh_token=refresh-secret"));
    assert!(body.contains("scope=tools.read"));
    state.token_requests.fetch_add(1, Ordering::Relaxed);
    Json(json!({
        "access_token": "refreshed-token",
        "token_type": "Bearer",
        "expires_in": 3600
    }))
    .into_response()
}

async fn legacy_sse(State(state): State<LegacyState>, headers: HeaderMap) -> Response {
    assert_legacy_headers(&headers);
    let (message_tx, message_rx) = mpsc::unbounded_channel::<Value>();
    *state.messages.lock().await = Some(message_tx);
    let endpoint = stream::once(async {
        Ok::<_, Infallible>(Event::default().event("endpoint").data("/messages"))
    });
    let messages = stream::unfold(message_rx, |mut receiver| async move {
        receiver.recv().await.map(|message| {
            (
                Ok::<_, Infallible>(Event::default().event("message").data(message.to_string())),
                receiver,
            )
        })
    });
    Sse::new(endpoint.chain(messages)).into_response()
}

async fn legacy_post(
    State(state): State<LegacyState>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> Response {
    assert_legacy_headers(&headers);
    if let Some(id) = request.get("id").cloned() {
        let result = mcp_result(&request);
        state
            .messages
            .lock()
            .await
            .as_ref()
            .expect("SSE connection should be registered")
            .send(json!({"jsonrpc": "2.0", "id": id, "result": result}))
            .expect("SSE response should send");
    }
    StatusCode::ACCEPTED.into_response()
}

fn assert_legacy_headers(headers: &HeaderMap) {
    assert_eq!(
        headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer legacy-token")
    );
    assert_eq!(
        headers
            .get("x-qwenpaw-test")
            .and_then(|value| value.to_str().ok()),
        Some("legacy")
    );
}

fn mcp_result(request: &Value) -> Value {
    match request.get("method").and_then(Value::as_str) {
        Some("initialize") => json!({
            "protocolVersion": "2025-03-26",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "sse-test", "version": "0.1.0"}
        }),
        Some("tools/list") => json!({
            "tools": [{
                "name": "echo",
                "description": "Echo over legacy SSE",
                "inputSchema": {
                    "type": "object",
                    "properties": {"text": {"type": "string"}},
                    "required": ["text"]
                }
            }]
        }),
        Some("tools/call") => {
            let text = request["params"]["arguments"]["text"]
                .as_str()
                .unwrap_or_default();
            json!({
                "content": [{"type": "text", "text": text}],
                "structuredContent": {"echo": text},
                "isError": false
            })
        }
        method => panic!("unexpected legacy MCP method: {method:?}"),
    }
}
