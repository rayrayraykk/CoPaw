//! A second CLI must not recover turns owned by a still-running Core.

use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use axum::{Router, routing::post};
use pretty_assertions::assert_eq;
use qwenpaw_app_server_client::{AppServerClient, ClientIdentity, StdioAppServer};
use serde_json::{Value, json};
use tokio::{net::TcpListener, process::Command, sync::Notify};

fn command(home: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_qwenpaw-core"));
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("QWENPAW_HOME", home)
        .env("QWENPAW_API_KEY", "instance-fixture-key")
        .env("QWENPAW_BASE_URL", "http://127.0.0.1:1/v1")
        .env("QWENPAW_MODEL", "instance-fixture")
        .kill_on_drop(true);
    // Windows process startup needs its system directory, not user credentials.
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", system_root);
    }
    command
}

fn snapshot(home: &std::path::Path, id: &str) -> Value {
    let connection = rusqlite::Connection::open_with_flags(
        home.join("threads.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let value: String = connection
        .query_row("SELECT snapshot FROM threads WHERE id = ?1", [id], |row| {
            row.get(0)
        })
        .unwrap();
    serde_json::from_str(&value).unwrap()
}

async fn held_model() -> (
    std::net::SocketAddr,
    Arc<Notify>,
    tokio::task::JoinHandle<()>,
) {
    let requested = Arc::new(Notify::new());
    let received = requested.clone();
    let router = Router::new().route(
        "/v1/chat/completions",
        post(move || {
            let received = received.clone();
            async move {
                received.notify_one();
                std::future::pending::<String>().await
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let model = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (address, requested, model)
}

async fn active_thread(client: &AppServerClient, workspace: &std::path::Path) -> String {
    let started: Value = client
        .request("thread/start", json!({"workspaceRoot":workspace}))
        .await
        .unwrap();
    let id = started["thread"]["id"].as_str().unwrap().to_owned();
    let _: Value = client
        .request(
            "turn/start",
            json!({"threadId":id,"input":[{"type":"text","text":"hold"}]}),
        )
        .await
        .unwrap();
    id
}

async fn start(home: &std::path::Path) -> StdioAppServer {
    let app = StdioAppServer::spawn_command(command(home).args(["app-server", "--stdio"])).unwrap();
    app.client()
        .initialize(ClientIdentity::new("instance-fixture", "1"))
        .await
        .unwrap();
    app
}

#[tokio::test]
async fn concurrent_cli_is_rejected_before_recovering_an_active_turn() {
    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().join("共享 Core data");
    let (address, requested, model) = held_model().await;
    let mut first_command = command(&home);
    first_command
        .args(["app-server", "--stdio"])
        .current_dir(directory.path())
        .env("QWENPAW_BASE_URL", format!("http://{address}/v1"));
    let first = StdioAppServer::spawn_command(&mut first_command).unwrap();
    first
        .client()
        .initialize(ClientIdentity::new("instance-owner", "1"))
        .await
        .unwrap();
    let id = active_thread(first.client(), directory.path()).await;
    tokio::time::timeout(Duration::from_secs(5), requested.notified())
        .await
        .unwrap();
    let before: Value = first
        .client()
        .request("thread/read", json!({"threadId":id}))
        .await
        .unwrap();
    let disk = snapshot(&home, &id);
    assert_eq!(before["turns"][0]["status"], "inProgress");
    assert_eq!(
        json!({"thread":disk["thread"],"turns":disk["turns"]}),
        before
    );

    let independent = start(&directory.path().join("independent")).await;
    let independent_threads: Value = independent
        .client()
        .request("thread/list", json!({}))
        .await
        .unwrap();
    independent.shutdown().await.unwrap();

    let mut outputs = Vec::new();
    for arguments in [
        vec!["app-server"],
        vec!["app-server", "--stdio"],
        vec!["app-server", "--listen", "127.0.0.1:0"],
        vec!["app-server", "--listen", "127.0.0.1:0", "--desktop"],
        vec![
            "app-server",
            "--listen",
            "127.0.0.1:0",
            "--remote",
            "--tls-cert",
            "unused-cert",
            "--tls-key",
            "unused-key",
            "--auth-token-file",
            "unused-token",
        ],
    ] {
        let output = tokio::time::timeout(
            Duration::from_secs(5),
            command(&home).args(arguments).stdin(Stdio::null()).output(),
        )
        .await
        .unwrap()
        .unwrap();
        outputs.push(output);
    }
    // Close all owned work before assertions so the red test leaves no child.
    let after_disk = snapshot(&home, &id);
    let after: Value = first
        .client()
        .request("thread/read", json!({"threadId":id}))
        .await
        .unwrap();
    first.shutdown().await.unwrap();
    model.abort();
    let _ = model.await;

    assert_eq!(independent_threads, json!({"data":[],"nextCursor":null}));
    for output in outputs {
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(output.stdout, Vec::<u8>::new());
        assert_eq!(
            String::from_utf8(output.stderr).unwrap().trim(),
            "Error: Core data directory is already in use by another process"
        );
    }
    assert_eq!(after, before);
    assert_eq!(after_disk, disk);

    let saved = snapshot(&home, &id);
    assert_eq!(saved["turns"][0]["status"], "interrupted");
    let reopened = start(&home).await;
    let restored: Value = reopened
        .client()
        .request("thread/read", json!({"threadId":id}))
        .await
        .unwrap();
    reopened.shutdown().await.unwrap();
    assert_eq!(
        restored,
        json!({"thread":saved["thread"],"turns":saved["turns"]})
    );
    assert!(home.join(".core-instance.lock").is_file());
}

#[tokio::test]
async fn killed_owner_releases_the_lock_before_real_startup_recovery() {
    let directory = tempfile::tempdir().unwrap();
    let (address, requested, model) = held_model().await;
    let mut owner = command(directory.path())
        .args(["app-server", "--stdio"])
        .env("QWENPAW_BASE_URL", format!("http://{address}/v1"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let client =
        AppServerClient::connect(owner.stdout.take().unwrap(), owner.stdin.take().unwrap());
    client
        .initialize(ClientIdentity::new("killed-owner", "1"))
        .await
        .unwrap();
    let id = active_thread(&client, directory.path()).await;
    tokio::time::timeout(Duration::from_secs(5), requested.notified())
        .await
        .unwrap();
    let before = snapshot(directory.path(), &id);
    assert_eq!(before["turns"][0]["status"], "inProgress");
    owner.kill().await.unwrap();
    assert!(!owner.wait().await.unwrap().success());
    client.shutdown().await;
    model.abort();
    let _ = model.await;
    // Inspect before the replacement process is allowed to recover anything.
    assert_eq!(snapshot(directory.path(), &id), before);
    let reopened = start(directory.path()).await;
    let restored: Value = reopened
        .client()
        .request("thread/read", json!({"threadId":id}))
        .await
        .unwrap();
    let mut expected = before;
    expected["thread"]["status"] = json!("idle");
    assert!(
        restored["thread"]["updatedAt"].as_u64().unwrap()
            >= expected["thread"]["updatedAt"].as_u64().unwrap()
    );
    expected["thread"]["updatedAt"] = restored["thread"]["updatedAt"].clone();
    expected["turns"][0]["status"] = json!("interrupted");
    assert_eq!(snapshot(directory.path(), &id), expected);
    reopened.shutdown().await.unwrap();
    assert_eq!(
        restored,
        json!({"thread":expected["thread"],"turns":expected["turns"]})
    );
}

#[tokio::test]
async fn failed_initialization_does_not_retain_ownership() {
    let directory = tempfile::tempdir().unwrap();
    let output = command(directory.path())
        .args(["app-server", "--stdio"])
        .env("QWENPAW_BASE_URL", "")
        .stdin(Stdio::null())
        .output()
        .await
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, Vec::<u8>::new());
    let reopened = start(directory.path()).await;
    let threads: Value = reopened
        .client()
        .request("thread/list", json!({}))
        .await
        .unwrap();
    reopened.shutdown().await.unwrap();
    assert_eq!(threads, json!({"data":[],"nextCursor":null}));
}
