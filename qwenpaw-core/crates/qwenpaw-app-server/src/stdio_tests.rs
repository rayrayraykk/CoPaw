use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use pretty_assertions::assert_eq;
use qwenpaw_core::{Core, ModelConfig};
use serde_json::json;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, ReadBuf};

use super::{AppServer, run};

fn server() -> AppServer {
    AppServer::new(Core::new(ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1/v1"),
        default_model: String::from("stdio-fixture"),
    }))
}

fn initialize() -> String {
    format!(
        "{}\n",
        json!({"id":1,"method":"initialize","params":{
            "clientInfo":{"name":"stdio-fixture","version":"1"}
        }})
    )
}

async fn finish(mut task: tokio::task::JoinHandle<anyhow::Result<()>>) -> anyhow::Result<()> {
    if let Ok(result) = tokio::time::timeout(Duration::from_secs(1), &mut task).await {
        result.unwrap()
    } else {
        task.abort();
        let _ = task.await;
        panic!("stdio did not finish after its transport stopped");
    }
}

#[tokio::test]
async fn eof_closes_host_admission_without_requiring_a_desktop() {
    let server = server();
    run(server.clone(), tokio::io::empty(), tokio::io::sink())
        .await
        .unwrap();
    assert!(server.inner.shutdown.is_cancelled());
}

#[tokio::test]
async fn explicit_host_shutdown_stops_an_idle_stdio_reader() {
    let server = server();
    let (_client, input) = tokio::io::duplex(4096);
    let task = tokio::spawn(run(server.clone(), input, tokio::io::sink()));
    server.inner.shutdown.cancel();
    finish(task).await.unwrap();
}

#[tokio::test]
async fn output_failure_stops_the_host_even_when_input_remains_open() {
    let server = server();
    let (mut client, input) = tokio::io::duplex(4096);
    let (output, reader) = tokio::io::duplex(1);
    drop(reader);
    let task = tokio::spawn(run(server.clone(), input, output));
    client.write_all(initialize().as_bytes()).await.unwrap();
    let error = finish(task).await.unwrap_err();
    assert_eq!(error.to_string(), "failed to write app-server message");
    assert!(server.inner.shutdown.is_cancelled());
}

struct BrokenInput;

impl AsyncRead for BrokenInput {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Poll::Ready(Err(std::io::Error::other("fixture input failed")))
    }
}

#[tokio::test]
async fn input_failure_closes_host_admission_and_preserves_the_error() {
    let server = server();
    let error = run(server.clone(), BrokenInput, tokio::io::sink())
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "failed to read app-server input");
    assert!(server.inner.shutdown.is_cancelled());
}

#[tokio::test]
async fn eof_flushes_the_admitted_response_as_protocol_json_only() {
    let server = server();
    let input = std::io::Cursor::new(format!("\n{}\n", initialize()).into_bytes());
    let (output, mut client) = tokio::io::duplex(16);
    let reader = tokio::spawn(async move {
        let mut bytes = Vec::new();
        client.read_to_end(&mut bytes).await.unwrap();
        bytes
    });
    run(server, input, output).await.unwrap();
    let bytes = reader.await.unwrap();
    assert_eq!(bytes.last(), Some(&b'\n'));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
        json!({"id":1,"result":{
            "protocolVersion":3,"serverInfo":{"name":"qwenpaw-core","version":"0.2.0"}
        }})
    );
}

#[tokio::test(start_paused = true)]
async fn eof_with_unread_output_has_a_bounded_error_after_host_shutdown() {
    let server = server();
    let input = std::io::Cursor::new(initialize().into_bytes());
    let (output, _unread) = tokio::io::duplex(1);
    let started = tokio::time::Instant::now();
    let error = run(server.clone(), input, output).await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "app-server output did not drain after shutdown"
    );
    assert_eq!(started.elapsed(), super::OUTPUT_DRAIN_TIMEOUT);
    assert!(server.inner.shutdown.is_cancelled());
}
