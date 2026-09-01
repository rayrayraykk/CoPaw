use std::process::Stdio;
use std::time::Duration;

use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::process::Command;
use tokio::time::timeout;

#[tokio::test]
async fn serves_initialize_and_thread_start_over_stdio() {
    let core_home = tempfile::tempdir().expect("temporary core home should be created");
    let mut child = Command::new(env!("CARGO_BIN_EXE_qwenpaw-core"))
        .args(["app-server", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("QWENPAW_HOME", core_home.path())
        .spawn()
        .expect("app server should start");
    let mut stdin = child.stdin.take().expect("stdin should be piped");
    let stdout = child.stdout.take().expect("stdout should be piped");
    let mut lines = BufReader::new(stdout).lines();

    send(
        &mut stdin,
        json!({
            "id": 1,
            "method": "initialize",
            "params": {
                "clientInfo": {"name": "integration_test", "version": "0.1.0"}
            }
        }),
    )
    .await;
    let initialized = receive(&mut lines).await;
    assert_eq!(
        initialized,
        json!({
            "id": 1,
            "result": {
                "protocolVersion": 3,
                "serverInfo": {"name": "qwenpaw-core", "version": "0.2.0"}
            }
        })
    );

    send(
        &mut stdin,
        json!({"id": 2, "method": "thread/start", "params": {}}),
    )
    .await;
    let response = receive(&mut lines).await;
    let notification = receive(&mut lines).await;
    assert_eq!(response["id"], json!(2));
    assert_eq!(notification["method"], json!("thread/started"));
    assert_eq!(
        response["result"]["thread"],
        notification["params"]["thread"]
    );

    child.kill().await.expect("app server should stop");
}

#[tokio::test]
async fn starts_with_an_empty_store_without_touching_legacy_data() {
    let app_data = tempfile::tempdir().expect("temporary app data should be created");
    let legacy_home = app_data.path().join("python-product");
    let core_home = app_data.path().join("rust-core-v1");
    std::fs::create_dir_all(&legacy_home).expect("legacy fixture directory should be created");
    let legacy_file = legacy_home.join("chats.json");
    let legacy_contents = br#"{"chats":[{"id":"legacy-thread"}]}"#;
    std::fs::write(&legacy_file, legacy_contents).expect("legacy fixture should be written");

    let mut child = Command::new(env!("CARGO_BIN_EXE_qwenpaw-core"))
        .args(["app-server", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("QWENPAW_HOME", &core_home)
        .spawn()
        .expect("app server should start");
    let mut stdin = child.stdin.take().expect("stdin should be piped");
    let stdout = child.stdout.take().expect("stdout should be piped");
    let mut lines = BufReader::new(stdout).lines();

    send(
        &mut stdin,
        json!({
            "id": 1,
            "method": "initialize",
            "params": {
                "clientInfo": {"name": "fresh_start_test", "version": "0.1.0"}
            }
        }),
    )
    .await;
    let initialized = receive(&mut lines).await;
    assert_eq!(initialized["result"]["protocolVersion"], json!(3));

    send(
        &mut stdin,
        json!({"id": 2, "method": "thread/list", "params": {}}),
    )
    .await;
    assert_eq!(
        receive(&mut lines).await,
        json!({"id": 2, "result": {"data": [], "nextCursor": null}})
    );

    child.kill().await.expect("app server should stop");
    assert_eq!(
        std::fs::read(&legacy_file).expect("legacy fixture should remain readable"),
        legacy_contents
    );
    assert!(core_home.join("threads.sqlite3").is_file());
}

#[tokio::test]
async fn desktop_mode_reports_its_port_and_stops_with_the_authenticated_endpoint() {
    let core_home = tempfile::tempdir().expect("temporary core home should be created");
    let console = tempfile::tempdir().expect("temporary Console should be created");
    std::fs::write(console.path().join("index.html"), "<html>desktop</html>")
        .expect("Console index should be written");
    let mut child = Command::new(env!("CARGO_BIN_EXE_qwenpaw-core"))
        .args([
            "app-server",
            "--listen",
            "127.0.0.1:0",
            "--desktop",
            "--console-static-dir",
        ])
        .arg(console.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("QWENPAW_HOME", core_home.path())
        .env("QWENPAW_API_KEY", "desktop-test-key")
        .env(
            "QWENPAW_DESKTOP_SHUTDOWN_TOKEN",
            "desktop-integration-token",
        )
        .spawn()
        .expect("Desktop app server should start");
    let stdout = child.stdout.take().expect("stdout should be piped");
    let mut lines = BufReader::new(stdout).lines();
    let ready = timeout(Duration::from_secs(5), lines.next_line())
        .await
        .expect("Desktop ready marker should not time out")
        .expect("Desktop ready marker should be readable")
        .expect("Desktop ready marker should exist");
    let payload = ready
        .strip_prefix("QWENPAW_BACKEND_READY ")
        .expect("Desktop ready marker should use the compatibility prefix");
    let port = serde_json::from_str::<Value>(payload)
        .expect("Desktop ready marker should contain JSON")["port"]
        .as_u64()
        .and_then(|port| u16::try_from(port).ok())
        .expect("Desktop ready marker should contain a TCP port");

    let mut version = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("Desktop HTTP client should connect");
    version
        .write_all(b"GET /api/version HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .expect("version request should write");
    let mut version_response = Vec::new();
    version
        .read_to_end(&mut version_response)
        .await
        .expect("version response should read");
    assert!(String::from_utf8_lossy(&version_response).contains("\"backend\":\"rust-core\""));

    let mut shutdown = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("Desktop shutdown client should connect");
    shutdown
        .write_all(
            b"POST /api/desktop/shutdown HTTP/1.1\r\nHost: localhost\r\nX-QwenPaw-Desktop-Shutdown-Token: desktop-integration-token\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )
        .await
        .expect("Desktop shutdown request should write");
    let mut shutdown_response = Vec::new();
    shutdown
        .read_to_end(&mut shutdown_response)
        .await
        .expect("Desktop shutdown response should read");
    assert!(String::from_utf8_lossy(&shutdown_response).starts_with("HTTP/1.1 200 OK"));
    let status = timeout(Duration::from_secs(5), child.wait())
        .await
        .expect("Desktop app server should exit before timeout")
        .expect("Desktop app server should be waitable");
    assert!(status.success());
}

async fn send(stdin: &mut tokio::process::ChildStdin, message: Value) {
    stdin
        .write_all(format!("{message}\n").as_bytes())
        .await
        .expect("request should be written");
    stdin.flush().await.expect("request should be flushed");
}

async fn receive(lines: &mut tokio::io::Lines<BufReader<tokio::process::ChildStdout>>) -> Value {
    let line = timeout(Duration::from_secs(5), lines.next_line())
        .await
        .expect("app server response should not time out")
        .expect("app server response should be readable")
        .expect("app server should return a line");
    serde_json::from_str(&line).expect("app server response should be JSON")
}
