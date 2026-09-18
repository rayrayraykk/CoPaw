//! Observe the unchanged `RestoreModal` before releasing an actual Core producer.

use super::*;
use crate::desktop_checkpoints::quiescence as gate;
use pretty_assertions::assert_eq;
use tokio::io::AsyncBufReadExt as _;

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_checkpoint_snapshot_and_restore_work_while_a_turn_is_running() {
    let (mut fixture, writer, shared, before) = browser_fixture().await;
    let events = start_model(&fixture, &writer).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server_task = tokio::spawn(fixture.server.clone().run_http(listener));
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut child = tokio::process::Command::new("node")
        .arg(root.join("scripts/console_browser_smoke.mjs"))
        .args([&origin, "/checkpoints", "--checkpoints-running-restore"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let stderr = child.stderr.take().unwrap();
    let result = tokio::time::timeout(Duration::from_secs(90), async {
        tokio::join!(
            child.wait_with_output(),
            release_after_ui_wait(&fixture, stderr)
        )
    })
    .await;
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), server_task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let (output, observed) = result.unwrap();
    let output = output.unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: Value = serde_json::from_str(&stdout).unwrap_or_else(|_| panic!("{stdout}"));
    assert!(output.status.success(), "{report:#}");
    assert!(observed, "browser never observed the pending restore");
    assert_eq!(report["ok"], true);
    assert_eq!(
        report["pages"][0]["checkpointsCrud"],
        json!({
            "autoIsolated":true,"snapshot":true,"gcSettings":true,"switched":true,
            "reload":true,"restorePreview":true,"selectiveRestore":true,
            "gc":true,"resetOnlyWriter":true,"waitingForRun":true
        })
    );
    assert_model_completed(events).await;
    fixture.reopen().await;
    assert_restored_files(&fixture, &shared, &writer).await;
    assert_eq!(
        api(&fixture, "default", "GET", "/graph", Value::Null).await,
        before
    );
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .read_thread(&writer)
            .await
            .unwrap()
            .turns,
        vec![]
    );
    assert_eq!(fixture.remote.requests.lock().unwrap().len(), 2);
}

async fn release_after_ui_wait(fixture: &Fixture, stderr: tokio::process::ChildStderr) -> bool {
    let mut lines = tokio::io::BufReader::new(stderr).lines();
    let mut observed = false;
    while let Some(line) = lines.next_line().await.unwrap() {
        if line != "QWENPAW_CHECKPOINT_RESTORE_WAITING" {
            eprintln!("{line}");
            continue;
        }
        assert!(!observed, "duplicate restore observation");
        let key = fixture.data_key("writer");
        tokio::time::timeout(Duration::from_secs(5), async {
            while !gate::is_paused(&fixture.server, &key) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(
                fixture
                    .directory
                    .path()
                    .join("data/workspaces/writer/notes.txt")
            )
            .unwrap(),
            "base edited"
        );
        assert!(
            fixture
                .server
                .inner
                .desktop_agent_lifecycle_lock
                .try_lock()
                .is_ok()
        );
        fixture
            .remote
            .release_hold
            .store(true, std::sync::atomic::Ordering::Release);
        fixture.remote.hold_released.notify_waiters();
        observed = true;
    }
    observed
}

async fn start_model(fixture: &Fixture, writer: &str) -> qwenpaw_core::TurnEventStream {
    let (_, events) = fixture
        .server
        .inner
        .core
        .start_turn_with_runtime(
            qwenpaw_protocol::TurnStartParams {
                thread_id: writer.to_owned(),
                input: vec![qwenpaw_protocol::UserInput::Text {
                    text: String::from("hold"),
                }],
            },
            None,
            qwenpaw_core::AgentRuntimeConfig {
                approval_level: qwenpaw_core::ToolApprovalLevel::Off,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    fixture.wait_requests(1).await;
    events
}

async fn assert_model_completed(mut events: qwenpaw_core::TurnEventStream) {
    let completed = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(event) = events.recv().await {
            if let CoreEvent::TurnCompleted(event) = event {
                return event.turn.status;
            }
        }
        panic!("active fixture ended without a completion event");
    })
    .await
    .unwrap();
    assert_eq!(completed, TurnStatus::Completed);
}
