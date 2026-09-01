use futures_util::SinkExt;
use futures_util::StreamExt;
use qwenpaw_app_server::AppServer;
use qwenpaw_core::Core;
use qwenpaw_core::ModelConfig;
use serde_json::Value;
use serde_json::json;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio_tungstenite::MaybeTlsStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;

type ClientSocket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

#[tokio::test]
async fn serves_health_and_independent_websocket_sessions() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener should bind");
    let address = listener
        .local_addr()
        .expect("test listener should have an address");
    let server = AppServer::new(Core::new(ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("qwen-test"),
    }));
    let task = tokio::spawn(server.run_http(listener));

    let mut health = tokio::net::TcpStream::connect(address)
        .await
        .expect("health client should connect");
    health
        .write_all(b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .expect("health request should write");
    let mut health_response = Vec::new();
    health
        .read_to_end(&mut health_response)
        .await
        .expect("health response should read");
    let health_response =
        String::from_utf8(health_response).expect("health response should be UTF-8");
    assert!(health_response.starts_with("HTTP/1.1 200 OK"));
    assert!(health_response.contains("{\"status\":\"ok\"}"));

    let url = format!("ws://{address}/app-protocol");
    let (mut first, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("first WebSocket should connect");
    send_json(
        &mut first,
        json!({
            "id": 1,
            "method": "initialize",
            "params": {"clientInfo": {"name": "test", "version": "0.1.0"}}
        }),
    )
    .await;
    let initialized = receive_json(&mut first).await;
    assert_eq!(initialized["result"]["protocolVersion"], json!(2));

    let (mut second, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("second WebSocket should connect");
    send_json(
        &mut second,
        json!({"id": 2, "method": "thread/list", "params": {}}),
    )
    .await;
    assert_eq!(
        receive_json(&mut second).await,
        json!({
            "id": 2,
            "error": {"code": -32000, "message": "server is not initialized"}
        })
    );

    first.close(None).await.expect("first socket should close");
    second
        .close(None)
        .await
        .expect("second socket should close");
    task.abort();
}

#[tokio::test]
async fn rejects_a_non_loopback_http_listener() {
    let listener = tokio::net::TcpListener::bind("0.0.0.0:0")
        .await
        .expect("test listener should bind");
    let server = AppServer::new(Core::new(ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("qwen-test"),
    }));

    let error = server
        .run_http(listener)
        .await
        .expect_err("non-loopback listener should fail closed");
    assert_eq!(
        error.to_string(),
        "HTTP App Protocol requires a loopback listener"
    );
}

async fn send_json(socket: &mut ClientSocket, value: Value) {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .expect("WebSocket request should send");
}

async fn receive_json(socket: &mut ClientSocket) -> Value {
    let message = socket
        .next()
        .await
        .expect("server should send a response")
        .expect("WebSocket response should be valid");
    serde_json::from_str(message.to_text().expect("response should be text"))
        .expect("response should be JSON")
}
