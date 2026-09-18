use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_agents_page_stops_ordinary_chats_without_touching_default() {
    let mut fixture = Fixture::new().await;
    writer(&fixture).await;
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"editor","name":"Editor"}),
        )
        .await;
    let root = prepare_browser(&mut fixture).await;
    let default = agent_chat(&fixture, "default").await;
    let writer_id = agent_chat(&fixture, "writer").await;
    let editor = agent_chat(&fixture, "editor").await;
    let default_body = start_chat(&fixture, "default", &default).await;
    let writer_body = start_chat(&fixture, "writer", &writer_id).await;
    let editor_body = start_chat(&fixture, "editor", &editor).await;
    let pending = wait_pending(&fixture, 3).await;
    let original = fixture
        .server
        .inner
        .core
        .read_thread(&default)
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(fixture.server.clone().run_http(listener));
    // Reuse the original Agent-row controls driver; it only observes approvals,
    // not Cron internals. All three pending runs here are ordinary Console chats.
    let result = tokio::time::timeout(
        Duration::from_secs(90),
        tokio::process::Command::new("node")
            .arg(root.join("scripts/console_browser_smoke.mjs"))
            .args([&origin, "/agents", "--cron-agent-stop"])
            .kill_on_drop(true)
            .output(),
    )
    .await;
    let after = fixture
        .server
        .inner
        .core
        .read_thread(&default)
        .await
        .unwrap();
    let approvals = fixture
        .request("GET", "/api/console/push-messages", Value::Null)
        .await;
    let writer_turn = fixture
        .server
        .inner
        .core
        .read_thread(&writer_id)
        .await
        .unwrap();
    let editor_turn = fixture
        .server
        .inner
        .core
        .read_thread(&editor)
        .await
        .unwrap();
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_browser_report(&result.unwrap().unwrap());
    assert_eq!(after, original);
    let expected = pending["pending_approvals"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|approval| approval["agent_id"] == "default")
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        approvals,
        json!({"messages":[],"pending_approvals":expected})
    );
    assert_eq!(
        (writer_turn.turns[0].status, editor_turn.turns[0].status),
        (TurnStatus::Interrupted, TurnStatus::Interrupted)
    );
    for body in [writer_body, editor_body, default_body] {
        terminal(body, "canceled").await;
    }
    for path in [
        "workspace/cron-output.txt",
        "data/workspaces/writer/cron-output.txt",
        "data/workspaces/editor/cron-output.txt",
    ] {
        assert!(!fixture.directory.path().join(path).exists());
    }
}

async fn prepare_browser(fixture: &mut Fixture) -> std::path::PathBuf {
    fixture.server.inner.core.write_ui_language("en").unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    fixture.server.inner.shutdown.cancel();
    shutdown(&fixture.server).await;
    fixture.server = AppServer::new_desktop_with_stores_and_workspace(
        fixture.server.inner.core.clone(),
        &root.join("../console/dist"),
        String::from("console-lifecycle-fixture"),
        Arc::new(Credentials),
        &fixture.directory.path().join("data"),
        &fixture.directory.path().join("workspace"),
    )
    .unwrap();
    root
}

fn assert_browser_report(output: &std::process::Output) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let report: Value =
        serde_json::from_str(&stdout).unwrap_or_else(|_| panic!("{stdout}\n{stderr}"));
    assert!(output.status.success(), "{report:#}\n{stderr}");
    assert_eq!(
        report["pages"][0]["cronAgentStop"],
        json!({
            "disabled":true,"reenabled":true,"deleted":true,"selectedFallback":true,"defaultPending":true,"reload":true
        })
    );
}
