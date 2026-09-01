use std::process::Stdio;
use std::time::Duration;

use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use tokio::io::AsyncBufReadExt;
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
                "protocolVersion": 2,
                "serverInfo": {"name": "qwenpaw-core", "version": "0.1.0"}
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
