use std::process::Stdio;
use std::time::Duration;

use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use tokio::io::AsyncReadExt as _;
use tokio::process::Command;

use super::{AppServerClient, ClientError, ClientIdentity, StdioAppServer};

#[path = "stdio_shutdown_runtime_tests.rs"]
mod runtimes;

const FIXTURE: &str = r"
const fs = require('node:fs');
const phase = value => fs.writeFileSync(process.argv[1] + '.phase', value);
const lines = require('node:readline').createInterface({ input: process.stdin });
const methods = [];
lines.on('line', line => {
  const request = JSON.parse(line);
  phase(request.method);
  methods.push(request.method);
  if (request.method === 'initialize') {
    process.stdout.write(JSON.stringify({ id: request.id, result: {
      protocolVersion: 3, serverInfo: { name: 'fixture', version: '1' }
    } }) + '\n');
  } else if (request.method === 'fixture/wait') {
    process.stdout.write(JSON.stringify({ method: 'fixture/received', params: {} }) + '\n');
  }
});
lines.on('close', () => {
  phase('eof');
  if (process.argv[2] === 'hold') {
    setInterval(() => {}, 1000);
    return;
  }
  process.stdout.write('x'.repeat(2 * 1024 * 1024), () => {
    phase('stdout-drained');
    process.stderr.write('y'.repeat(2 * 1024 * 1024), () => {
      phase('stderr-drained');
      setTimeout(() => {
        fs.writeFileSync(process.argv[1], JSON.stringify({ methods }));
        process.exitCode = Number(process.argv[2]);
        phase('saved');
      }, 30);
    });
  });
});
";

async fn start(directory: &std::path::Path, mode: &str) -> StdioAppServer {
    let mut command = Command::new("node");
    command
        .args(["-e", FIXTURE])
        .arg(directory.join("finished.json"))
        .arg(mode)
        .stderr(Stdio::piped());
    let server = StdioAppServer::spawn_command(&mut command).unwrap();
    server
        .client()
        .initialize(ClientIdentity::new("shutdown-test", "1"))
        .await
        .unwrap();
    server
}

#[tokio::test]
async fn stdio_shutdown_waits_for_eof_drains_both_pipes_and_closes_clones() {
    let directory = tempfile::tempdir().unwrap();
    let server = start(directory.path(), "0").await;
    let client = server.client().clone();
    let mut notifications = client.subscribe();
    let pending_client = client.clone();
    let pending = tokio::spawn(async move {
        pending_client
            .request::<_, Value>("fixture/wait", json!({}))
            .await
    });
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), notifications.recv())
            .await
            .unwrap()
            .unwrap()
            .method,
        "fixture/received"
    );
    tokio::time::timeout(Duration::from_secs(5), server.shutdown())
        .await
        .unwrap_or_else(|error| {
            panic!(
                "{error}; child phase: {:?}; closing: {}; worker finished: {}; pending finished: {}",
                std::fs::read_to_string(directory.path().join("finished.json.phase")),
                client.closing.load(std::sync::atomic::Ordering::Acquire),
                client.worker_abort.is_finished(),
                pending.is_finished(),
            );
        })
        .unwrap();
    assert!(matches!(
        pending.await.unwrap(),
        Err(ClientError::TransportClosed)
    ));
    assert!(matches!(
        client.request::<_, Value>("thread/list", json!({})).await,
        Err(ClientError::TransportClosed)
    ));
    let saved: Value =
        serde_json::from_slice(&std::fs::read(directory.path().join("finished.json")).unwrap())
            .unwrap();
    assert_eq!(
        saved,
        json!({"methods":["initialize","initialized","fixture/wait"]})
    );
}

#[tokio::test]
async fn stdio_shutdown_reports_a_nonzero_exit_after_draining() {
    let directory = tempfile::tempdir().unwrap();
    let server = start(directory.path(), "7").await;
    let error = tokio::time::timeout(Duration::from_secs(5), server.shutdown())
        .await
        .unwrap_or_else(|error| {
            panic!(
                "{error}; child phase: {:?}",
                std::fs::read_to_string(directory.path().join("finished.json.phase"))
            );
        })
        .unwrap_err();
    assert!(matches!(error, ClientError::ProcessExit(status) if status.code() == Some(7)));
    let saved: Value =
        serde_json::from_slice(&std::fs::read(directory.path().join("finished.json")).unwrap())
            .unwrap();
    assert_eq!(saved, json!({"methods":["initialize","initialized"]}));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_owned_processes_each_receive_eof_and_report_their_exit() {
    for _ in 0..4 {
        let mut starting = Vec::new();
        for _ in 0..4 {
            starting.push(tokio::spawn(async {
                let directory = tempfile::tempdir().unwrap();
                let server = start(directory.path(), "7").await;
                (directory, server)
            }));
        }
        let mut started = Vec::new();
        for task in starting {
            started.push(task.await.unwrap());
        }
        // Startup is independently bounded by initialize; only ready peers
        // enter the concurrent shutdown barrier and its unchanged deadline.
        let ready = std::sync::Arc::new(tokio::sync::Barrier::new(4));
        let mut tasks = Vec::new();
        for (directory, server) in started {
            let ready = ready.clone();
            tasks.push(tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(5), ready.wait())
                    .await
                    .unwrap();
                let result = tokio::time::timeout(Duration::from_secs(5), server.shutdown()).await;
                let phase = std::fs::read_to_string(directory.path().join("finished.json.phase"));
                (result, phase)
            }));
        }
        let mut results = Vec::new();
        for task in tasks {
            results.push(task.await.unwrap());
        }
        for (result, phase) in results {
            assert!(
                matches!(result, Ok(Err(ClientError::ProcessExit(status)))
                if status.code() == Some(7)),
                "shutdown: {result:?}; phase: {phase:?}"
            );
            assert_eq!(phase.unwrap(), "saved");
        }
    }
}

#[tokio::test]
async fn stdio_shutdown_timeout_reaps_the_child_but_reports_failure() {
    let directory = tempfile::tempdir().unwrap();
    let server = start(directory.path(), "hold").await;
    let client = server.client().clone();
    let started = std::time::Instant::now();
    let error = tokio::time::timeout(Duration::from_secs(40), server.shutdown())
        .await
        .unwrap()
        .unwrap_err();
    assert!(matches!(error, ClientError::ShutdownTimeout));
    assert!(started.elapsed() >= super::SHUTDOWN_TIMEOUT);
    assert!(!directory.path().join("finished.json").exists());
    assert!(client.worker.lock().await.is_none());
    assert!(client.worker_abort.is_finished());
}

#[tokio::test]
async fn ordinary_transport_shutdown_does_not_wait_for_a_live_peer() {
    let (_peer_input, reader) = tokio::io::duplex(64);
    let (writer, mut peer_output) = tokio::io::duplex(64);
    let client = AppServerClient::connect(reader, writer);
    tokio::time::timeout(Duration::from_secs(1), client.shutdown())
        .await
        .unwrap();
    let mut bytes = Vec::new();
    peer_output.read_to_end(&mut bytes).await.unwrap();
    assert_eq!(bytes, Vec::<u8>::new());
    assert!(matches!(
        client.notify("initialized", json!({})).await,
        Err(ClientError::TransportClosed)
    ));
    client.shutdown().await;
}

#[tokio::test]
async fn cancelled_drain_wait_retains_the_worker_for_the_next_waiter() {
    let (peer_input, reader) = tokio::io::duplex(64);
    let (writer, mut peer_output) = tokio::io::duplex(64);
    let client = AppServerClient::connect(reader, writer);
    let waiter = client.clone();
    let task = tokio::spawn(async move { waiter.shutdown_transport(true).await });
    let mut bytes = Vec::new();
    tokio::time::timeout(Duration::from_secs(1), peer_output.read_to_end(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(bytes, Vec::<u8>::new());
    assert!(!task.is_finished());
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(client.worker.lock().await.is_some());
    assert!(!client.worker_abort.is_finished());
    drop(peer_input);
    tokio::time::timeout(Duration::from_secs(1), client.join_worker())
        .await
        .unwrap()
        .unwrap();
    assert!(client.worker.lock().await.is_none());
}
