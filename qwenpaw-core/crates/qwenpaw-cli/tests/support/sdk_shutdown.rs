use std::sync::Arc;
use std::time::Duration;

use axum::{Router, routing::post};
use pretty_assertions::assert_eq;
use qwenpaw_app_server_client::{ClientError, ClientIdentity, StdioAppServer};
use qwenpaw_protocol::{
    ThreadReadResponse, ThreadStartResponse, ThreadStatus, TurnStartResponse, TurnStatus,
};
use qwenpaw_storage::ThreadStore;
use serde_json::json;
use tokio::{net::TcpListener, process::Command, sync::Notify};

#[tokio::test]
async fn rust_sdk_shutdown_saves_interruption_before_core_restart_recovery() {
    run_shutdown(false).await;
}

#[tokio::test]
async fn rust_sdk_shutdown_rejects_a_real_core_final_persistence_failure() {
    run_shutdown(true).await;
}

async fn run_shutdown(reject_final: bool) {
    let directory = tempfile::tempdir().unwrap();
    let received = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let (address, model) = start_model(received.clone(), release.clone()).await;
    let mut command = Command::new(env!("CARGO_BIN_EXE_qwenpaw-core"));
    command
        .args(["app-server", "--stdio"])
        .current_dir(directory.path())
        .env("QWENPAW_HOME", directory.path())
        .env("QWENPAW_API_KEY", "shutdown-fixture-key")
        .env("QWENPAW_BASE_URL", format!("http://{address}/v1"));
    let server = StdioAppServer::spawn_command(&mut command).unwrap();
    server
        .client()
        .initialize(ClientIdentity::new("rust-shutdown", "1"))
        .await
        .unwrap();
    let started: ThreadStartResponse = server
        .client()
        .request(
            "thread/start",
            json!({
                "workspaceRoot": directory.path(), "model": null
            }),
        )
        .await
        .unwrap();
    let thread_id = started.thread.id;
    let _: TurnStartResponse = server
        .client()
        .request(
            "turn/start",
            json!({
                "threadId": thread_id, "input": [{"type":"text","text":"hold"}]
            }),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), received.notified())
        .await
        .unwrap();
    let expected: ThreadReadResponse = server
        .client()
        .request(
            "thread/read",
            json!({
                "threadId": thread_id
            }),
        )
        .await
        .unwrap();
    assert_eq!(expected.turns.len(), 1);
    assert_eq!(expected.turns[0].status, TurnStatus::InProgress);
    let database = rusqlite::Connection::open(directory.path().join("threads.sqlite3")).unwrap();
    if reject_final {
        database
            .execute_batch(
                "CREATE TRIGGER reject_final BEFORE INSERT ON threads
             WHEN json_extract(NEW.snapshot, '$.turns[#-1].status') != 'inProgress'
             BEGIN SELECT RAISE(FAIL, 'fixture final write failure'); END;",
            )
            .unwrap();
    }
    let closed = tokio::time::timeout(Duration::from_secs(5), server.shutdown())
        .await
        .unwrap();
    let mut expected =
        shutdown_history(directory.path(), closed, expected, reject_final, &database);

    let reopened = StdioAppServer::spawn_command(&mut command).unwrap();
    reopened
        .client()
        .initialize(ClientIdentity::new("rust-reopen", "1"))
        .await
        .unwrap();
    let restored: ThreadReadResponse = reopened
        .client()
        .request(
            "thread/read",
            json!({
                "threadId": thread_id
            }),
        )
        .await
        .unwrap();
    if reject_final {
        assert!(restored.thread.updated_at >= expected.thread.updated_at);
        expected.thread.updated_at = restored.thread.updated_at;
        expected.thread.status = ThreadStatus::Idle;
        expected.turns[0].status = TurnStatus::Interrupted;
    }
    assert_eq!(restored, expected);
    reopened.shutdown().await.unwrap();
    release.notify_waiters();
    model.abort();
    let _ = model.await;
}

async fn start_model(
    received: Arc<Notify>,
    release: Arc<Notify>,
) -> (
    std::net::SocketAddr,
    tokio::task::JoinHandle<std::io::Result<()>>,
) {
    let router = Router::new().route(
        "/v1/chat/completions",
        post(move || {
            let received = received.clone();
            let release = release.clone();
            async move {
                received.notify_one();
                release.notified().await;
                String::new()
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, router).await });
    (address, task)
}

fn shutdown_history(
    directory: &std::path::Path,
    closed: Result<(), ClientError>,
    expected: ThreadReadResponse,
    reject_final: bool,
    database: &rusqlite::Connection,
) -> ThreadReadResponse {
    if !reject_final {
        closed.unwrap();
        return read_saved_history(directory, expected);
    }
    let Err(ClientError::ProcessExit(status)) = closed else {
        panic!("failed final persistence must produce an abnormal Core exit");
    };
    assert_eq!(status.code(), Some(1));
    let snapshots = ThreadStore::open(&directory.join("threads.sqlite3"))
        .unwrap()
        .load_all()
        .unwrap();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(
        ThreadReadResponse {
            thread: snapshots[0].thread.clone(),
            turns: snapshots[0].turns.clone()
        },
        expected
    );
    database
        .execute_batch("DROP TRIGGER reject_final;")
        .unwrap();
    expected
}

fn read_saved_history(
    directory: &std::path::Path,
    mut expected: ThreadReadResponse,
) -> ThreadReadResponse {
    // ThreadStore does not run Core's startup recovery of active turns.
    let store = ThreadStore::open(&directory.join("threads.sqlite3")).unwrap();
    let snapshots = store.load_all().unwrap();
    assert_eq!(snapshots.len(), 1);
    let saved = &snapshots[0];
    assert!(saved.thread.updated_at >= expected.thread.updated_at);
    expected.thread.status = ThreadStatus::Idle;
    expected.thread.updated_at = saved.thread.updated_at;
    expected.turns[0].status = TurnStatus::Interrupted;
    assert_eq!(
        ThreadReadResponse {
            thread: saved.thread.clone(),
            turns: saved.turns.clone()
        },
        expected
    );
    expected
}
