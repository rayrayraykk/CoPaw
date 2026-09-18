//! Channels must use the selected, validated Workspace, including after reopen.

use super::*;
use pretty_assertions::assert_eq;

const SINGLE: &str = "/api/config/channels/console";
const ALL: &str = "/api/config/channels";

async fn read(fixture: &Fixture, agent: &str) -> Value {
    let (status, value) = scoped(fixture, agent, "GET", SINGLE, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    value
}

async fn save(fixture: &Fixture, agent: &str, prefix: &str) -> Value {
    let mut expected = read(fixture, agent).await;
    expected["bot_prefix"] = json!(prefix);
    assert_eq!(
        scoped(fixture, agent, "PUT", SINGLE, expected.clone()).await,
        (StatusCode::OK, expected.clone())
    );
    expected
}

async fn create(fixture: &Fixture, id: &str) -> std::path::PathBuf {
    let value = fixture
        .request("POST", "/api/agents", json!({"id":id,"name":id}))
        .await;
    value["workspace_dir"].as_str().unwrap().into()
}

#[tokio::test]
async fn channel_scope_single_bulk_and_reopen_preserve_other_workspaces() {
    let mut fixture = Fixture::new().await;
    create(&fixture, "writer").await;
    create(&fixture, "editor").await;
    let default = read(&fixture, "default").await;
    let writer = save(&fixture, "writer", "writer only").await;
    assert_eq!(read(&fixture, "default").await, default);
    assert_eq!(read(&fixture, "editor").await, default);
    let mut editor = default.clone();
    editor["bot_prefix"] = json!("editor only");
    let (status, all) = scoped(&fixture, "editor", "PUT", ALL, json!({"console":editor})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(all["console"], editor);
    assert_eq!(all.as_object().unwrap().len(), 18);
    for reopened in [false, true] {
        if reopened {
            fixture.reopen().await;
        }
        for (id, expected) in [
            ("default", &default),
            ("writer", &writer),
            ("editor", &editor),
        ] {
            assert_eq!(read(&fixture, id).await, *expected);
            let (status, list) = scoped(&fixture, id, "GET", ALL, Value::Null).await;
            let mut builtin = expected.clone();
            builtin["isBuiltin"] = json!(true);
            assert_eq!((status, list["console"].clone()), (StatusCode::OK, builtin));
        }
    }
}

#[tokio::test]
async fn channel_scope_invalid_registrations_cannot_read_or_write() {
    let fixture = Fixture::new().await;
    let root = create(&fixture, "writer").await;
    save(&fixture, "default", "keep").await;
    let before = fixture
        .server
        .inner
        .core
        .read_channel_config_data()
        .unwrap();
    let catalog = registry(&fixture);
    for (id, expected) in [
        ("missing", StatusCode::NOT_FOUND),
        ("writer", StatusCode::CONFLICT),
    ] {
        if id == "writer" {
            std::fs::rename(&root, root.with_extension("retained")).unwrap();
            std::fs::create_dir(&root).unwrap();
        }
        assert_rejected(&fixture, id, expected).await;
        assert_eq!(
            fixture
                .server
                .inner
                .core
                .read_channel_config_data()
                .unwrap(),
            before
        );
        assert_eq!(registry(&fixture), catalog);
    }
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    create(&fixture, "disabled").await;
    fixture
        .request(
            "PATCH",
            "/api/agents/disabled/toggle",
            json!({"enabled":false}),
        )
        .await;
    assert_rejected(&fixture, "disabled", StatusCode::FORBIDDEN).await;
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .read_channel_config_data()
            .unwrap(),
        before
    );
}

async fn assert_rejected(fixture: &Fixture, id: &str, expected: StatusCode) {
    for (method, path, body) in [
        ("GET", SINGLE, Value::Null),
        ("GET", ALL, Value::Null),
        ("PUT", SINGLE, json!({"bot_prefix":"must not save"})),
        (
            "PUT",
            ALL,
            json!({"console":{"bot_prefix":"must not save"}}),
        ),
        (
            "POST",
            "/api/config/channels/console/conflict-check",
            json!({}),
        ),
    ] {
        let (status, response) = scoped(fixture, id, method, path, body).await;
        assert_eq!(status, expected, "{method} {path}: {response}");
    }
}

#[tokio::test]
async fn channel_scope_revalidates_after_waiting_for_lifecycle_admission() {
    let fixture = Fixture::new().await;
    let root = create(&fixture, "writer").await;
    let before = fixture
        .server
        .inner
        .core
        .read_channel_config_data()
        .unwrap();
    let guard = fixture
        .server
        .inner
        .desktop_agent_lifecycle_lock
        .lock()
        .await;
    let request = scoped(
        &fixture,
        "writer",
        "PUT",
        SINGLE,
        json!({"bot_prefix":"queued"}),
    );
    tokio::pin!(request);
    assert!(matches!(
        futures_util::poll!(&mut request),
        std::task::Poll::Pending
    ));
    std::fs::rename(&root, root.with_extension("retained")).unwrap();
    std::fs::create_dir(&root).unwrap();
    drop(guard);
    assert_eq!(
        request.await,
        (
            StatusCode::CONFLICT,
            json!({"detail":"Agent 'writer' Workspace binding has changed"})
        )
    );
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .read_channel_config_data()
            .unwrap(),
        before
    );
    assert_eq!(std::fs::read_dir(root).unwrap().count(), 0);
}

#[tokio::test]
async fn channel_scope_retained_root_reconnects_but_new_root_and_copy_are_empty() {
    let mut fixture = Fixture::new().await;
    let root = create(&fixture, "writer").await;
    let default = read(&fixture, "default").await;
    let original = save(&fixture, "writer", "retained workspace").await;
    fixture
        .request("DELETE", "/api/agents/writer", Value::Null)
        .await;
    let fresh = fixture.directory.path().join("fresh");
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"New writer","workspace_dir":fresh}),
        )
        .await;
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"editor","name":"Retained editor","workspace_dir":root}),
        )
        .await;
    assert_eq!(read(&fixture, "writer").await, default);
    assert_eq!(read(&fixture, "editor").await, original);
    let copied = fixture
        .request(
            "POST",
            "/api/agents/editor/copy",
            json!({"copy_agent_json":true}),
        )
        .await;
    let copy_id = copied["id"].as_str().unwrap();
    assert_eq!(read(&fixture, copy_id).await, default);
    save(&fixture, copy_id, "copy only").await;
    fixture.reopen().await;
    assert_eq!(read(&fixture, "writer").await, default);
    assert_eq!(read(&fixture, "editor").await, original);
    assert_eq!(read(&fixture, "default").await, default);
}

#[tokio::test]
async fn channel_scope_native_v1_is_default_only_and_corruption_is_not_overwritten() {
    let fixture = Fixture::new().await;
    create(&fixture, "writer").await;
    let empty = read(&fixture, "writer").await;
    let mut legacy = empty.clone();
    legacy["bot_prefix"] = json!("native legacy");
    let serialized = json!({"version":1,"console":legacy}).to_string();
    fixture
        .server
        .inner
        .core
        .write_channel_config_data(&serialized)
        .unwrap();
    assert_eq!(read(&fixture, "default").await, legacy);
    assert_eq!(read(&fixture, "writer").await, empty);
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .read_channel_config_data()
            .unwrap(),
        Some(serialized)
    );
    save(&fixture, "writer", "new writer").await;
    assert_eq!(read(&fixture, "default").await, legacy);
    let data: Value = serde_json::from_str(
        &fixture
            .server
            .inner
            .core
            .read_channel_config_data()
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(data["version"], 2);
    assert_eq!(data["workspaces"].as_array().unwrap().len(), 2);
    fixture
        .server
        .inner
        .core
        .write_channel_config_data("broken")
        .unwrap();
    for (method, body) in [
        ("GET", Value::Null),
        ("PUT", json!({"bot_prefix":"do not repair"})),
    ] {
        let (status, _) = scoped(&fixture, "writer", method, SINGLE, body).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            fixture
                .server
                .inner
                .core
                .read_channel_config_data()
                .unwrap(),
            Some(String::from("broken"))
        );
    }
}

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_channels_page_scopes_drawer_saves_and_reopens_without_frontend_changes() {
    let mut fixture = Fixture::new().await;
    create(&fixture, "writer").await;
    create(&fixture, "editor").await;
    let default = save(&fixture, "default", "default baseline").await;
    let mut writer = save(&fixture, "writer", "writer baseline").await;
    let mut editor = save(&fixture, "editor", "editor baseline").await;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    fixture.server.inner.shutdown.cancel();
    fixture.server = AppServer::new_desktop_with_stores_and_workspace(
        fixture.server.inner.core.clone(),
        &root.join("../console/dist"),
        String::from("channel-browser-shutdown"),
        Arc::new(Credentials),
        &fixture.directory.path().join("data"),
        &fixture.directory.path().join("workspace"),
    )
    .unwrap();
    fixture.server.inner.core.write_ui_language("en").unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(fixture.server.clone().run_http(listener));
    let result = tokio::time::timeout(
        Duration::from_secs(100),
        tokio::process::Command::new("node")
            .arg(root.join("scripts/console_browser_smoke.mjs"))
            .args([&origin, "/channels", "--channel-scope"])
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
    let stderr = String::from_utf8_lossy(&output.stderr);
    let report: Value =
        serde_json::from_str(&stdout).unwrap_or_else(|_| panic!("{stdout}\n{stderr}"));
    assert!(output.status.success(), "{report:#}\n{stderr}");
    assert_eq!(report["ok"], true);
    assert_eq!(
        report["pages"][0]["channelScope"],
        json!({
            "savedThroughOriginalDrawer":["writer","editor"], "switchedThroughOriginalSidebar":true,
            "defaultUnchanged":true,"wholeConfigurationsIsolated":true,"reload":true
        })
    );
    writer["bot_prefix"] = json!("writer saved through original drawer");
    editor["bot_prefix"] = json!("editor saved through original drawer");
    fixture.reopen().await;
    for (id, expected) in [("default", default), ("writer", writer), ("editor", editor)] {
        assert_eq!(read(&fixture, id).await, expected);
    }
    assert!(fixture.remote.requests.lock().unwrap().is_empty());
}
