//! A connected SDK owns its connection, never the shared server lifecycle.

use std::sync::Arc;
use std::time::Duration;

use axum::routing::post;
use pretty_assertions::assert_eq;
use qwenpaw_app_server_client::{
    ClientError, ClientIdentity, WebSocketConnection, WebSocketOptions,
};
use qwenpaw_core::{Core, ModelConfig};
use qwenpaw_protocol::{
    ThreadReadResponse, ThreadStartParams, ThreadStartResponse, TurnStartParams, TurnStartResponse,
    TurnStatus, UserInput,
};
use serde_json::{Value, json};

use super::AppServer;

const TOKEN: &str = "sdk-fixture-token-01234567890123456789";

#[tokio::test]
async fn websocket_close_replies_before_dropping_the_transport() {
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::protocol::{CloseFrame, frame::coding::CloseCode};
    let close = Message::Close(Some(CloseFrame {
        code: CloseCode::Normal,
        reason: "client detach".into(),
    }));
    assert_control_reply(close.clone(), close).await;
}

#[tokio::test]
async fn websocket_idle_ping_flushes_pong_without_an_application_response() {
    use tokio_tungstenite::tungstenite::Message;
    assert_control_reply(
        Message::Ping(vec![1, 2, 3].into()),
        Message::Pong(vec![1, 2, 3].into()),
    )
    .await;
}

async fn assert_control_reply(
    sent: tokio_tungstenite::tungstenite::Message,
    expected: tokio_tungstenite::tungstenite::Message,
) {
    use futures_util::{SinkExt, StreamExt};
    let server = AppServer::new(Core::new(model("http://127.0.0.1:1/v1".to_owned())));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("ws://{}/app-protocol", listener.local_addr().unwrap());
    let serving = tokio::spawn(server.clone().run_http(listener));
    let (mut socket, _) = tokio_tungstenite::connect_async(endpoint).await.unwrap();
    socket.send(sent).await.unwrap();
    let reply = tokio::time::timeout(Duration::from_secs(1), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(reply, expected);
    drop(socket);
    stop_server(server, serving).await;
}

fn model(base_url: String) -> ModelConfig {
    ModelConfig {
        api_key: Some("fixture-model-key".to_owned()),
        base_url,
        default_model: "fixture-model".to_owned(),
    }
}

#[tokio::test]
async fn rust_sdk_wss_checks_trust_auth_and_independent_initialization() {
    let directory = tempfile::tempdir().unwrap();
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
    let certificate = directory.path().join("certificate.pem");
    let private_key = directory.path().join("key.pem");
    let token = directory.path().join("token");
    std::fs::write(&certificate, cert.cert.pem()).unwrap();
    private_file(&private_key, cert.signing_key.serialize_pem().as_bytes());
    private_file(&token, TOKEN.as_bytes());
    let mut roots = rustls::RootCertStore::empty();
    roots.add(cert.cert.der().clone()).unwrap();
    let core = Core::new(model("http://127.0.0.1:1/v1".to_owned()));
    let before = core.read_config();
    let server = AppServer::new(core)
        .with_remote_auth_token_file(&token)
        .unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!(
        "wss://localhost:{}/app-protocol",
        listener.local_addr().unwrap().port()
    );
    let owner = server.clone();
    let serving =
        tokio::spawn(async move { owner.run_wss(listener, &certificate, &private_key).await });
    assert!(
        WebSocketConnection::connect(
            &endpoint,
            WebSocketOptions {
                bearer_token: Some(TOKEN.to_owned()),
                tls_roots: None
            }
        )
        .await
        .is_err()
    );
    let error = WebSocketConnection::connect(
        &endpoint,
        WebSocketOptions {
            bearer_token: None,
            tls_roots: Some(roots.clone()),
        },
    )
    .await
    .err()
    .unwrap();
    assert!(matches!(error, ClientError::WebSocketHandshake(401)));
    let options = || WebSocketOptions {
        bearer_token: Some(TOKEN.to_owned()),
        tls_roots: Some(roots.clone()),
    };
    // Trusting the CA must not disable the certificate's hostname check.
    assert!(
        WebSocketConnection::connect(&endpoint.replace("localhost", "127.0.0.1"), options())
            .await
            .is_err()
    );
    let wrong_token = WebSocketConnection::connect(
        &endpoint,
        WebSocketOptions {
            bearer_token: Some("wrong-fixture-token-01234567890123456789".to_owned()),
            tls_roots: Some(roots.clone()),
        },
    )
    .await
    .err()
    .unwrap();
    assert!(matches!(wrong_token, ClientError::WebSocketHandshake(401)));
    let first = initialized_connection(&endpoint, "first", options()).await;
    let second = WebSocketConnection::connect(&endpoint, options())
        .await
        .unwrap();
    let uninitialized = second
        .client()
        .request::<_, Value>("config/read", json!({}))
        .await
        .unwrap_err();
    assert!(matches!(
        uninitialized,
        ClientError::Protocol { code: -32000, .. }
    ));
    second
        .client()
        .initialize(ClientIdentity::new("second", "1"))
        .await
        .unwrap();
    first.disconnect().await.unwrap();
    assert_eq!(
        second
            .client()
            .request::<_, qwenpaw_protocol::ConfigReadResponse>("config/read", json!({}))
            .await
            .unwrap(),
        before
    );
    assert!(!server.inner.shutdown.is_cancelled());
    second.disconnect().await.unwrap();
    assert!(!server.inner.shutdown.is_cancelled());
    stop_server(server, serving).await;
}

async fn stop_server(server: AppServer, serving: tokio::task::JoinHandle<anyhow::Result<()>>) {
    server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), serving)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

fn private_file(path: &std::path::Path, bytes: &[u8]) {
    use std::io::Write as _;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    options.open(path).unwrap().write_all(bytes).unwrap();
}

async fn initialized_connection(
    endpoint: &str,
    name: &str,
    options: WebSocketOptions,
) -> WebSocketConnection {
    let connection = WebSocketConnection::connect(endpoint, options)
        .await
        .unwrap();
    connection
        .client()
        .initialize(ClientIdentity::new(name, "1"))
        .await
        .unwrap();
    connection
}

#[tokio::test]
async fn rust_sdk_disconnect_keeps_another_client_and_accepted_turn_alive() {
    let directory = tempfile::tempdir().unwrap();
    let requested = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
    let announced = requested.clone();
    let released = release.clone();
    let router = axum::Router::new().route("/v1/chat/completions", post(move || {
        let announced = announced.clone();
        let released = released.clone();
        async move {
            announced.notify_one();
            released.notified().await;
            ([("content-type", "text/event-stream")], "data: {\"choices\":[{\"delta\":{\"content\":\"reply after disconnect\"},\"finish_reason\":null}]}\n\ndata: [DONE]\n\n")
        }
    }));
    let (stop_model, stopped) = tokio::sync::oneshot::channel();
    let model_task = tokio::spawn(async {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
    });
    let core = Core::persistent(model(base_url), &directory.path().join("core.sqlite")).unwrap();
    let config = core.read_config();
    let server = AppServer::new(core.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("ws://{}/app-protocol", listener.local_addr().unwrap());
    let serving = tokio::spawn(server.clone().run_http(listener));
    let first = initialized_connection(&endpoint, "first", WebSocketOptions::default()).await;
    let second = initialized_connection(&endpoint, "second", WebSocketOptions::default()).await;
    let started: ThreadStartResponse = first
        .client()
        .request(
            "thread/start",
            ThreadStartParams {
                model: None,
                workspace_root: Some(directory.path().to_str().unwrap().to_owned()),
            },
        )
        .await
        .unwrap();
    let thread_id = started.thread.id;
    let _: TurnStartResponse = first
        .client()
        .request(
            "turn/start",
            TurnStartParams {
                thread_id: thread_id.clone(),
                input: vec![UserInput::Text {
                    text: "hold".to_owned(),
                }],
            },
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), requested.notified())
        .await
        .unwrap();
    let before = core.read_thread(&thread_id).await.unwrap();
    assert_eq!(before.turns[0].status, TurnStatus::InProgress);
    first.disconnect().await.unwrap();
    let shared: ThreadReadResponse = second
        .client()
        .request("thread/read", json!({"threadId":thread_id}))
        .await
        .unwrap();
    assert_eq!(shared, before);
    assert!(!server.inner.shutdown.is_cancelled());
    release.notify_one();
    let after = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let state: ThreadReadResponse = second
                .client()
                .request("thread/read", json!({"threadId":thread_id}))
                .await
                .unwrap();
            if state.turns[0].status == TurnStatus::Completed {
                break state;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(after, core.read_thread(&thread_id).await.unwrap());
    assert!(
        serde_json::to_string(&after)
            .unwrap()
            .contains("reply after disconnect")
    );
    assert_eq!(core.read_config(), config);
    second.disconnect().await.unwrap();
    assert!(!server.inner.shutdown.is_cancelled());
    stop_server(server, serving).await;
    drop(core);
    stop_model.send(()).unwrap();
    model_task.await.unwrap().unwrap();
}
