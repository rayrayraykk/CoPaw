use super::*;
use crate::ClientIdentity;
use pretty_assertions::assert_eq;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[test]
fn validates_endpoints_and_redacts_credentials() {
    for endpoint in [
        "https://localhost/app-protocol",
        "ws://localhost/app-protocol",
        "ws://example.test/app-protocol",
        "ws://192.0.2.1/app-protocol",
        "ws://[::]/app-protocol",
        "wss://user:secret@example.test/app-protocol",
        "wss://example.test/app-protocol?token=secret",
        "wss://example.test/#secret",
    ] {
        let error = request(endpoint, None).unwrap_err().to_string();
        assert!(!error.contains("secret"));
        assert!(error.contains("invalid endpoint"));
    }
    for endpoint in [
        "ws://127.0.0.1/app-protocol",
        "ws://[::1]/app-protocol",
        "wss://example.test/app-protocol",
    ] {
        let request = request(endpoint, Some("fixture-token-01234567890123456789")).unwrap();
        assert_eq!(
            request.headers()[AUTHORIZATION],
            "Bearer fixture-token-01234567890123456789"
        );
        assert!(request.headers()[AUTHORIZATION].is_sensitive());
    }
    for token in [
        "short",
        "fixture-token-01234567890123456789\r\n",
        "a b",
        &"x".repeat(4097),
    ] {
        assert_eq!(
            request("wss://example.test/app-protocol", Some(token))
                .unwrap_err()
                .to_string(),
            "app-server WebSocket connection failed: invalid bearer token"
        );
    }
}

async fn listener() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("ws://{}/app-protocol", listener.local_addr().unwrap());
    (listener, endpoint)
}

#[tokio::test]
async fn correlates_requests_notifications_pretty_json_and_ping() {
    let (listener, endpoint) = listener().await;
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        let initialize: Value =
            serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(initialize["method"], "initialize");
        let response = json!({"id":initialize["id"],"result":{
            "protocolVersion":3,"serverInfo":{"name":"fixture","version":"1"}}});
        socket
            .send(Message::Text(
                serde_json::to_string_pretty(&response).unwrap().into(),
            ))
            .await
            .unwrap();
        let initialized: Value =
            serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(initialized, json!({"method":"initialized","params":{}}));
        socket
            .send(Message::Ping(vec![1, 2, 3].into()))
            .await
            .unwrap();
        let mut requests = Vec::new();
        let mut pong = false;
        while requests.len() < 2 || !pong {
            match socket.next().await.unwrap().unwrap() {
                Message::Text(text) => requests.push(serde_json::from_str::<Value>(&text).unwrap()),
                Message::Pong(bytes) => {
                    assert_eq!(&bytes[..], &[1, 2, 3]);
                    pong = true;
                }
                message => panic!("unexpected frame: {message:?}"),
            }
        }
        socket
            .send(Message::Text(
                json!({"method":"fixture/event","params":{"n":1}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        for request in requests.iter().rev() {
            socket
                .send(Message::Text(
                    json!({"id":request["id"],"result":request["params"]})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
        }
        assert!(matches!(
            socket.next().await.unwrap().unwrap(),
            Message::Close(_)
        ));
    });
    let connection = WebSocketConnection::connect(&endpoint, WebSocketOptions::default())
        .await
        .unwrap();
    let client = connection.client();
    let initialized = client
        .initialize(ClientIdentity::new("sdk", "1"))
        .await
        .unwrap();
    assert_eq!(initialized.server_info.name, "fixture");
    let mut events = client.subscribe();
    let (first, second) = tokio::join!(
        client.request::<_, Value>("fixture/echo", json!({"n":1})),
        client.request::<_, Value>("fixture/echo", json!({"n":2})),
    );
    assert_eq!(first.unwrap(), json!({"n":1}));
    assert_eq!(second.unwrap(), json!({"n":2}));
    assert_eq!(
        events.recv().await.unwrap(),
        ServerNotification {
            method: "fixture/event".to_owned(),
            params: json!({"n":1})
        }
    );
    let clone = client.clone();
    connection.disconnect().await.unwrap();
    assert!(matches!(
        clone.request::<_, Value>("fixture/echo", ()).await,
        Err(ClientError::TransportClosed)
    ));
    server.await.unwrap();
}

#[tokio::test]
async fn rejects_invalid_frames_and_wakes_pending_requests() {
    for (frame, expected) in [
        (Message::Binary(vec![1].into()), "expected a text message"),
        (
            Message::Text("not JSON secret-value".into()),
            "invalid JSON message",
        ),
        (
            Message::Text("x".repeat(MAX_MESSAGE_BYTES + 1).into()),
            "message or write buffer limit exceeded",
        ),
    ] {
        let (listener, endpoint) = listener().await;
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            let _ = socket.next().await;
            let _ = socket.send(frame).await;
            let _ = socket.next().await;
        });
        let connection = WebSocketConnection::connect(&endpoint, WebSocketOptions::default())
            .await
            .unwrap();
        assert!(matches!(
            connection
                .client()
                .request::<_, Value>("fixture/echo", ())
                .await,
            Err(ClientError::TransportClosed)
        ));
        assert_eq!(
            connection.disconnect().await.unwrap_err().to_string(),
            format!("app-server WebSocket connection failed: {expected}")
        );
        server.await.unwrap();
    }
}

#[tokio::test]
async fn drop_closes_socket_and_pending_requests_without_touching_server() {
    let (listener, endpoint) = listener().await;
    let (received, ready) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        let _ = socket.next().await;
        received.send(()).unwrap();
        assert!(socket.next().await.is_none_or(|result| result.is_err()));
    });
    let connection = WebSocketConnection::connect(&endpoint, WebSocketOptions::default())
        .await
        .unwrap();
    let client = connection.client().clone();
    let pending = tokio::spawn(async move { client.request::<_, Value>("fixture/echo", ()).await });
    ready.await.unwrap();
    drop(connection);
    assert!(matches!(
        pending.await.unwrap(),
        Err(ClientError::TransportClosed)
    ));
    server.await.unwrap();
}

async fn handshake_request(stream: &mut TcpStream) {
    let mut request = Vec::new();
    while !request.ends_with(b"\r\n\r\n") {
        request.push(stream.read_u8().await.unwrap());
    }
}

#[tokio::test]
async fn handshake_timeout_drops_the_socket() {
    let (listener, endpoint) = listener().await;
    let connect = tokio::spawn(async move {
        WebSocketConnection::connect(&endpoint, WebSocketOptions::default()).await
    });
    let (mut stream, _) = listener.accept().await.unwrap();
    handshake_request(&mut stream).await;
    // Real I/O must finish before paused time can auto-advance to the deadline.
    tokio::time::pause();
    tokio::time::advance(CONNECT_TIMEOUT).await;
    assert_eq!(
        connect.await.unwrap().err().unwrap().to_string(),
        "app-server WebSocket connection failed: handshake timed out"
    );
    assert_eq!(stream.read(&mut [0_u8; 1]).await.unwrap(), 0);
}

#[tokio::test]
async fn cancellation_during_handshake_drops_the_socket() {
    let (listener, endpoint) = listener().await;
    let connect = tokio::spawn(async move {
        WebSocketConnection::connect(&endpoint, WebSocketOptions::default()).await
    });
    let (mut stream, _) = listener.accept().await.unwrap();
    handshake_request(&mut stream).await;
    connect.abort();
    assert!(connect.await.err().unwrap().is_cancelled());
    assert_eq!(stream.read(&mut [0_u8; 1]).await.unwrap(), 0);
}

#[tokio::test]
async fn does_not_follow_redirects_or_expose_handshake_response_secrets() {
    let (listener, endpoint) = listener().await;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        handshake_request(&mut stream).await;
        stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: ws://127.0.0.1:1/secret\r\nX-Token: secret\r\nContent-Length: 6\r\n\r\nsecret").await.unwrap();
    });
    let error = WebSocketConnection::connect(&endpoint, WebSocketOptions::default())
        .await
        .err()
        .unwrap();
    assert_eq!(
        error.to_string(),
        "app-server WebSocket handshake returned HTTP 302"
    );
    server.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn bounded_disconnect_aborts_a_stalled_worker() {
    let client = AppServerClient::start_worker(|_commands, _notifications| async {
        std::future::pending::<Result<(), ClientError>>().await
    });
    let abort = client.worker_abort.clone();
    let connection = WebSocketConnection { client };
    assert_eq!(
        connection.disconnect().await.unwrap_err().to_string(),
        "app-server WebSocket connection failed: disconnect timed out"
    );
    tokio::task::yield_now().await;
    assert!(abort.is_finished());
}

#[tokio::test]
async fn incompatible_version_does_not_send_initialized() {
    let (listener, endpoint) = listener().await;
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        let initialize: Value =
            serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        socket
            .send(Message::Text(
                json!({"id":initialize["id"],"result":{
            "protocolVersion":0,"serverInfo":{"name":"fixture","version":"1"}}})
                .to_string()
                .into(),
            ))
            .await
            .unwrap();
        assert!(matches!(
            socket.next().await.unwrap().unwrap(),
            Message::Close(_)
        ));
    });
    let connection = WebSocketConnection::connect(&endpoint, WebSocketOptions::default())
        .await
        .unwrap();
    assert!(matches!(
        connection
            .client()
            .initialize(ClientIdentity::new("sdk", "1"))
            .await,
        Err(ClientError::ProtocolVersion {
            expected: 3,
            actual: 0
        })
    ));
    connection.disconnect().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn outgoing_limit_closes_transport_without_sending_payload() {
    let (listener, endpoint) = listener().await;
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        assert!(socket.next().await.is_none_or(|result| result.is_err()));
    });
    let connection = WebSocketConnection::connect(&endpoint, WebSocketOptions::default())
        .await
        .unwrap();
    assert!(matches!(
        connection
            .client()
            .request::<_, Value>("fixture/echo", "x".repeat(MAX_MESSAGE_BYTES))
            .await,
        Err(ClientError::TransportClosed)
    ));
    assert_eq!(
        connection.disconnect().await.unwrap_err().to_string(),
        "app-server WebSocket connection failed: outgoing message exceeds 1 MiB"
    );
    server.await.unwrap();
}

// The real WebSocket sink stalls on a bounded byte stream, without relying on
// platform-specific TCP buffer sizes or flooding an external server.
async fn backpressured_connection() -> (
    WebSocketConnection,
    tokio::io::DuplexStream,
    tokio::task::JoinHandle<Result<Value, ClientError>>,
) {
    let (stream, mut peer) = tokio::io::duplex(64);
    let socket = WebSocketStream::from_raw_socket(
        stream,
        tokio_tungstenite::tungstenite::protocol::Role::Client,
        None,
    )
    .await;
    let client = AppServerClient::start_worker(move |commands, notifications| {
        run(socket, commands, notifications)
    });
    let requester = client.clone();
    let pending =
        tokio::spawn(async move { requester.request("fixture/echo", "x".repeat(4096)).await });
    peer.read_u8().await.unwrap();
    (WebSocketConnection { client }, peer, pending)
}

#[tokio::test]
async fn bounded_disconnect_aborts_real_websocket_write_backpressure() {
    let (connection, mut peer, pending) = backpressured_connection().await;
    let abort = connection.client.worker_abort.clone();
    tokio::time::pause();
    assert_eq!(
        connection.disconnect().await.unwrap_err().to_string(),
        "app-server WebSocket connection failed: disconnect timed out"
    );
    assert!(matches!(
        pending.await.unwrap(),
        Err(ClientError::TransportClosed)
    ));
    assert!(abort.is_finished());
    let mut rest = Vec::new();
    peer.read_to_end(&mut rest).await.unwrap();
    assert!(rest.len() <= 64);
}

#[tokio::test]
async fn cancelled_disconnect_aborts_worker_and_wakes_pending() {
    let (connection, mut peer, pending) = backpressured_connection().await;
    let clone = connection.client.clone();
    let abort = clone.worker_abort.clone();
    let disconnect = tokio::spawn(connection.disconnect());
    while !clone.closing.load(std::sync::atomic::Ordering::Acquire) {
        tokio::task::yield_now().await;
    }
    disconnect.abort();
    assert!(disconnect.await.unwrap_err().is_cancelled());
    assert!(matches!(
        pending.await.unwrap(),
        Err(ClientError::TransportClosed)
    ));
    assert!(abort.is_finished());
    assert!(matches!(
        clone.request::<_, Value>("fixture/echo", ()).await,
        Err(ClientError::TransportClosed)
    ));
    let mut rest = Vec::new();
    peer.read_to_end(&mut rest).await.unwrap();
    assert!(rest.len() <= 64);
}
