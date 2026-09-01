use axum::http::HeaderMap;
use axum::http::HeaderValue;
use axum::http::header::HOST;
use axum::http::header::ORIGIN;
use pretty_assertions::assert_eq;
use qwenpaw_core::ModelConfig;
use serde_json::json;
use tokio::sync::mpsc;

use super::*;

#[tokio::test]
async fn requires_initialize_before_other_requests() {
    let server = test_server();
    let (tx, mut rx) = mpsc::channel(8);
    let mut session = ConnectionSession::default();
    server
        .process_line(
            &mut session,
            &json!({"id": 1, "method": "thread/list", "params": {}}).to_string(),
            &tx,
        )
        .await;

    let response: serde_json::Value =
        serde_json::from_str(&rx.recv().await.expect("server should send a response"))
            .expect("response should be JSON");
    assert_eq!(
        response,
        json!({
            "id": 1,
            "error": {"code": -32000, "message": "server is not initialized"}
        })
    );
}

#[tokio::test]
async fn initializes_and_creates_a_thread() {
    let server = test_server();
    let (tx, mut rx) = mpsc::channel(8);
    let mut session = ConnectionSession::default();
    server
        .process_line(
            &mut session,
            &json!({
                "id": 1,
                "method": "initialize",
                "params": {
                    "clientInfo": {"name": "test", "version": "0.1.0"}
                }
            })
            .to_string(),
            &tx,
        )
        .await;
    let initialize: serde_json::Value =
        serde_json::from_str(&rx.recv().await.expect("server should initialize"))
            .expect("response should be JSON");
    assert_eq!(initialize["id"], json!(1));
    assert_eq!(initialize["result"]["protocolVersion"], json!(2));

    server
        .process_line(
            &mut session,
            &json!({
                "id": 99,
                "method": "initialize",
                "params": {
                    "clientInfo": {"name": "test", "version": "0.1.0"}
                }
            })
            .to_string(),
            &tx,
        )
        .await;
    let duplicate: serde_json::Value =
        serde_json::from_str(&rx.recv().await.expect("server should reject reinitialize"))
            .expect("response should be JSON");
    assert_eq!(
        duplicate,
        json!({
            "id": 99,
            "error": {"code": -32000, "message": "server is already initialized"}
        })
    );

    server
        .process_line(
            &mut session,
            &json!({"id": 2, "method": "thread/start", "params": {}}).to_string(),
            &tx,
        )
        .await;
    let response: serde_json::Value = serde_json::from_str(
        &rx.recv()
            .await
            .expect("server should respond to thread start"),
    )
    .expect("response should be JSON");
    let notification: serde_json::Value =
        serde_json::from_str(&rx.recv().await.expect("server should notify thread start"))
            .expect("notification should be JSON");
    assert_eq!(notification["method"], json!("thread/started"));
    assert_eq!(
        notification["params"]["thread"],
        response["result"]["thread"]
    );

    server
        .process_line(
            &mut session,
            &json!({
                "id": 3,
                "method": "tool/approval/respond",
                "params": {"approvalId": "missing", "decision": "denied"}
            })
            .to_string(),
            &tx,
        )
        .await;
    let approval: serde_json::Value =
        serde_json::from_str(&rx.recv().await.expect("server should respond to approval"))
            .expect("response should be JSON");
    assert_eq!(approval, json!({"id": 3, "result": {"accepted": false}}));
}

#[test]
fn websocket_origin_requires_loopback_or_an_explicit_allowlist() {
    let mut server = test_server();
    Arc::get_mut(&mut server.inner)
        .expect("test server should not be cloned")
        .allowed_origins
        .clear();
    let mut same_origin = HeaderMap::new();
    same_origin.insert(HOST, HeaderValue::from_static("127.0.0.1:8765"));
    same_origin.insert(ORIGIN, HeaderValue::from_static("http://127.0.0.1:8765"));
    assert!(server.origin_allowed(&same_origin));

    let mut rebinding = HeaderMap::new();
    rebinding.insert(HOST, HeaderValue::from_static("attacker.example"));
    rebinding.insert(ORIGIN, HeaderValue::from_static("http://attacker.example"));
    assert!(!server.origin_allowed(&rebinding));

    let mut foreign_origin = HeaderMap::new();
    foreign_origin.insert(HOST, HeaderValue::from_static("localhost:8765"));
    foreign_origin.insert(ORIGIN, HeaderValue::from_static("https://attacker.example"));
    assert!(!server.origin_allowed(&foreign_origin));
}

fn test_server() -> AppServer {
    AppServer::new(Core::new(ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("qwen-test"),
    }))
}
