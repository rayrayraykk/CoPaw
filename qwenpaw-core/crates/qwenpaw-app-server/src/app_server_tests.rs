use axum::http::HeaderMap;
use axum::http::HeaderValue;
use axum::http::header::HOST;
use axum::http::header::ORIGIN;
use pretty_assertions::assert_eq;
use qwenpaw_core::ModelConfig;
use serde_json::json;
use tokio::sync::mpsc;
use tower::ServiceExt as _;

use super::*;

#[test]
fn workspace_initialization_resolves_relative_data_without_changing_the_base() {
    let current = std::env::current_dir().unwrap();
    let build = current.join("target");
    std::fs::create_dir_all(&build).unwrap();
    let directory = tempfile::tempdir_in(&build).unwrap();
    let initial = directory.path().join("workspace");
    std::fs::create_dir(&initial).unwrap();
    let relative = directory
        .path()
        .strip_prefix(&current)
        .unwrap()
        .join("state");
    let server = test_server();
    let workspace = desktop_workspace_from_env(
        &server.inner.core,
        Some(relative.clone()),
        Some(initial.clone()),
    )
    .unwrap();
    assert_eq!(
        workspace.data_dir,
        current.join(relative).canonicalize().unwrap()
    );
    assert_eq!(workspace.initial, initial.canonicalize().unwrap());
    assert_eq!(*workspace.selected.try_read().unwrap(), workspace.initial);
}

#[tokio::test]
async fn disconnected_http_request_keeps_restore_lease_until_blocking_write_finishes() {
    let server = test_server();
    let core = server.inner.core.clone();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = tokio::sync::oneshot::channel();
    let handler_core = core.clone();
    let write = Arc::new(std::sync::Mutex::new(Some((
        started_tx,
        release_rx,
        finished_tx,
        handler_core,
    ))));
    let router = Router::new()
        .route(
            "/write",
            post(move || {
                let (started_tx, release_rx, finished_tx, handler_core) =
                    write.lock().unwrap().take().unwrap();
                async move {
                    tokio::task::spawn_blocking(move || {
                        started_tx.send(()).unwrap();
                        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                        handler_core.write_ui_language("zh").unwrap();
                    })
                    .await
                    .unwrap();
                    finished_tx.send(()).unwrap();
                    StatusCode::NO_CONTENT
                }
            }),
        )
        .layer(from_fn_with_state(server, restore_operation));
    let request = tokio::spawn(
        router.oneshot(
            Request::builder()
                .method("POST")
                .uri("/write")
                .body(axum::body::Body::empty())
                .unwrap(),
        ),
    );
    tokio::time::timeout(Duration::from_secs(5), started_rx)
        .await
        .unwrap()
        .unwrap();
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    assert!(matches!(
        core.begin_restore(Duration::from_millis(30)).await,
        Err(qwenpaw_core::CoreError::RestoreTimeout)
    ));
    release_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), finished_rx)
        .await
        .unwrap()
        .unwrap();
    let guard = core.begin_restore(Duration::from_secs(5)).await.unwrap();
    assert_eq!(core.read_ui_language().unwrap(), "zh");
    drop(guard);
}

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
    assert_eq!(initialize["result"]["protocolVersion"], json!(3));

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
