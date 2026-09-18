use super::*;
use pretty_assertions::assert_eq;

#[path = "desktop_checkpoint_running_browser_tests.rs"]
mod running;

async fn browser_fixture() -> (Fixture, String, std::path::PathBuf, Value) {
    let mut fixture = Fixture::new().await;
    actor(&fixture, "writer").await;
    chat(&fixture, "default", "default-session", "workspace").await;
    let writer_thread = chat(&fixture, "writer", "writer-session", "shared-project").await;
    let shared = fixture.directory.path().join("shared-project");
    for id in ["default", "writer"] {
        fixture
            .request(
                "PUT",
                &format!("/api/agents/{id}"),
                json!({"project_dir":shared}),
            )
            .await;
    }
    api(
        &fixture,
        "default",
        "POST",
        "/snapshot",
        json!({"session_id":"default-session","user_id":"desktop","name":"Default baseline"}),
    )
    .await;
    seed_writer_restore(&fixture, &shared).await;
    let before = api(&fixture, "default", "GET", "/graph", Value::Null).await;
    fixture.server.inner.core.write_ui_language("en").unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    fixture.server.inner.shutdown.cancel();
    shutdown(&fixture.server).await;
    fixture.server = AppServer::new_desktop_with_stores_and_workspace(
        fixture.server.inner.core.clone(),
        &root.join("../console/dist"),
        String::from("checkpoint-browser-shutdown"),
        Arc::new(Credentials),
        &fixture.directory.path().join("data"),
        &fixture.directory.path().join("workspace"),
    )
    .unwrap();
    (fixture, writer_thread, shared, before)
}

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_checkpoint_page_scopes_controls_to_the_selected_agent() {
    let (mut fixture, writer_thread, shared, before) = browser_fixture().await;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(fixture.server.clone().run_http(listener));
    let result = tokio::time::timeout(
        Duration::from_secs(90),
        tokio::process::Command::new("node")
            .arg(root.join("scripts/console_browser_smoke.mjs"))
            .args([&origin, "/checkpoints", "--checkpoints-crud"])
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            .output(),
    )
    .await;
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let output = result.unwrap().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: Value = serde_json::from_str(&stdout).unwrap_or_else(|_| panic!("{stdout}"));
    assert!(output.status.success(), "{report:#}");
    assert_eq!(report["ok"], true);
    assert_eq!(
        report["pages"][0]["checkpointsCrud"],
        json!({
            "autoIsolated":true,"snapshot":true,"gcSettings":true,"switched":true,
            "reload":true,"restorePreview":true,"selectiveRestore":true,
            "gc":true,"resetOnlyWriter":true
        })
    );
    fixture.reopen().await;
    assert_eq!(
        api(&fixture, "default", "GET", "/graph", Value::Null).await,
        before
    );
    assert_eq!(
        api(&fixture, "writer", "GET", "/status", Value::Null).await,
        json!({"auto_enabled":false,"has_checkpoints":false,"workspace_dir":fixture.directory.path().join("data/workspaces/writer").canonicalize().unwrap()})
    );
    assert_restored_files(&fixture, &shared, &writer_thread).await;
    assert!(fixture.remote.requests.lock().unwrap().is_empty());
}

async fn assert_restored_files(fixture: &Fixture, shared: &std::path::Path, writer_thread: &str) {
    let base = fixture.directory.path().join("data/workspaces/writer");
    assert_eq!(
        std::fs::read_to_string(base.join("notes.txt")).unwrap(),
        "base original"
    );
    assert_eq!(
        std::fs::read_to_string(base.join("unselected.txt")).unwrap(),
        "unselected edited"
    );
    assert_eq!(
        std::fs::read_to_string(shared.join("notes.txt")).unwrap(),
        "external edited"
    );
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .read_thread(writer_thread)
            .await
            .unwrap()
            .thread
            .workspace_root,
        Some(
            shared
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        )
    );
}

async fn seed_writer_restore(fixture: &Fixture, shared: &std::path::Path) {
    let base = fixture.directory.path().join("data/workspaces/writer");
    std::fs::write(base.join("notes.txt"), "base original").unwrap();
    std::fs::write(base.join("unselected.txt"), "unselected original").unwrap();
    api(
        fixture,
        "writer",
        "POST",
        "/snapshot",
        json!({"session_id":"writer-session","user_id":"desktop","name":"Writer baseline"}),
    )
    .await;
    std::fs::write(base.join("notes.txt"), "base edited").unwrap();
    std::fs::write(base.join("unselected.txt"), "unselected edited").unwrap();
    std::fs::write(shared.join("notes.txt"), "external edited").unwrap();
}
