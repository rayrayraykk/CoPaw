use super::*;
use pretty_assertions::assert_eq;
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_chat_keeps_final_save_error_across_reload_and_allows_recovery() {
    let fixture = browser_fixture().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let host = tokio::spawn(fixture.server.clone().run_http(listener));
    let browser =
        tokio::time::timeout(Duration::from_secs(90), run_browser(&fixture, &origin)).await;
    settle(&fixture).await;
    fixture.server.inner.shutdown.cancel();
    let closed = tokio::time::timeout(Duration::from_secs(5), host)
        .await
        .unwrap()
        .unwrap();
    let output = browser.unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: Value = serde_json::from_str(&stdout).unwrap_or_else(|_| panic!("{stdout}"));
    assert!(output.status.success(), "{report:#}");
    assert_eq!(report["ok"], true);
    assert_eq!(
        report["pages"][0]["finalPersistence"],
        json!({"replyRetained":true,"failureVisible":true,"reload":true,
            "journalUnchanged":true,"nextTurn":true,"recoveredReload":true})
    );
    assert_eq!(
        closed.unwrap_err().to_string(),
        "thread storage failed: one or more final turn writes failed in this Core instance"
    );
    assert_recovery(&fixture).await;
}

async fn browser_fixture() -> Fixture {
    let mut fixture = Fixture::new().await;
    enabled(&fixture, "default").await;
    fixture
        .request(
            "PUT",
            "/api/workspace/running-config",
            json!({"approval_level":"OFF"}),
        )
        .await;
    fixture.server.inner.core.write_ui_language("en").unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    fixture.server.inner.shutdown.cancel();
    shutdown(&fixture.server).await;
    fixture.server = AppServer::new_desktop_with_stores_and_workspace(
        fixture.server.inner.core.clone(),
        &root.join("../console/dist"),
        String::from("final-persistence-browser"),
        Arc::new(Credentials),
        &fixture.directory.path().join("data"),
        &fixture.directory.path().join("workspace"),
    )
    .unwrap();
    rusqlite::Connection::open(fixture.directory.path().join("core.sqlite"))
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER reject_completion BEFORE INSERT ON threads
             WHEN json_extract(NEW.snapshot, '$.turns[#-1].status') != 'inProgress'
             BEGIN SELECT RAISE(FAIL, 'fixture completion write failure'); END;",
        )
        .unwrap();
    fixture
}

async fn run_browser(fixture: &Fixture, origin: &str) -> std::process::Output {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut child = tokio::process::Command::new("node")
        .arg(root.join("scripts/console_browser_smoke.mjs"))
        .args([origin, "/chat", "--final-persistence"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut lines = BufReader::new(child.stderr.take().unwrap()).lines();
    let inspect = async {
        let mut first: Option<(String, Value)> = None;
        while let Some(line) = lines.next_line().await.unwrap() {
            eprintln!("{line}");
            if line == "QWENPAW_FINAL_PERSISTENCE_VISIBLE" {
                assert!(first.is_none());
                first = Some(assert_failed_journal(fixture).await);
                input.write_all(b"continue\n").await.unwrap();
            } else if line == "QWENPAW_FINAL_PERSISTENCE_RELOADED" {
                let (thread, before) = first.as_ref().unwrap();
                assert_eq!(&read_journal(fixture, thread), before);
                rusqlite::Connection::open(fixture.directory.path().join("core.sqlite"))
                    .unwrap()
                    .execute_batch("DROP TRIGGER reject_completion")
                    .unwrap();
                input.write_all(b"continue\n").await.unwrap();
            }
        }
    };
    let (output, ()) = tokio::join!(child.wait_with_output(), inspect);
    output.unwrap()
}

fn read_journal(fixture: &Fixture, thread: &str) -> Value {
    let database = rusqlite::Connection::open_with_flags(
        fixture.directory.path().join("core.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let row: String = database
        .query_row(
            "SELECT snapshot FROM threads WHERE id = ?",
            [thread],
            |row| row.get(0),
        )
        .unwrap();
    serde_json::from_str(&row).unwrap()
}

async fn assert_failed_journal(fixture: &Fixture) -> (String, Value) {
    let snapshots = fixture.server.inner.core.statistics_snapshots().await;
    assert_eq!(snapshots.len(), 1);
    let thread = snapshots[0].thread.id.clone();
    let live = fixture
        .server
        .inner
        .core
        .read_thread(&thread)
        .await
        .unwrap();
    assert_eq!(live.turns.len(), 1);
    assert_eq!(live.turns[0].status, TurnStatus::Failed);
    assert_eq!(
        live.turns[0].error,
        Some(qwenpaw_protocol::ErrorInfo {
            message: String::from(
                "Failed to persist the final turn; the latest state may not survive restart."
            )
        })
    );
    let saved = read_journal(fixture, &thread);
    let mut admitted = live.turns[0].clone();
    admitted.status = TurnStatus::InProgress;
    admitted.error = None;
    // Model/tool steps are journaled before finalization; their presence does
    // not acknowledge the final status or its persistence failure.
    assert_eq!(saved["turns"], json!([admitted]));
    assert_eq!(saved["thread"]["status"], "active");
    settle(fixture).await;
    assert_eq!(summary(fixture, "default").await, no_auto());
    (thread, saved)
}

async fn assert_recovery(fixture: &Fixture) {
    let snapshots = fixture.server.inner.core.statistics_snapshots().await;
    assert_eq!(snapshots.len(), 1);
    let live = fixture
        .server
        .inner
        .core
        .read_thread(&snapshots[0].thread.id)
        .await
        .unwrap();
    assert_eq!(
        live.turns
            .iter()
            .map(|turn| turn.status)
            .collect::<Vec<_>>(),
        vec![TurnStatus::Failed, TurnStatus::Completed]
    );
    assert_eq!(live.turns[1].error, None);
    let saved = read_journal(fixture, &live.thread.id);
    assert_eq!(
        json!({"thread":saved["thread"],"turns":saved["turns"]}),
        json!(live)
    );
    settle(fixture).await;
    assert_eq!(summary(fixture, "default").await, one_auto());
    let requests = fixture.remote.requests.lock().unwrap();
    assert_eq!(requests.len(), 4);
    for (index, text) in [
        (0, "write fixture with final persistence failure"),
        (2, "write fixture after storage recovery"),
    ] {
        assert_eq!(
            requests[index]["messages"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()["content"],
            text
        );
    }
}
