//! Configuration publication must validate the registered Workspace generation.

use super::*;
use crate::desktop_agents;
use crate::desktop_agents::identity::MARKER_NAME;
use pretty_assertions::assert_eq;

async fn writer(fixture: &Fixture) -> std::path::PathBuf {
    let created = fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Writer"}),
        )
        .await;
    std::path::PathBuf::from(created["workspace_dir"].as_str().unwrap())
}

fn assert_untouched(fixture: &Fixture, before: &Value, root: &std::path::Path) {
    assert_eq!(registry(fixture), *before);
    assert_eq!(
        std::fs::read(root.join("agent.json")).unwrap(),
        b"replacement directory owner".to_vec()
    );
    assert!(!root.join(MARKER_NAME).exists());
}

#[tokio::test]
async fn field_publication_rejects_a_replaced_directory_without_writing() {
    let fixture = Fixture::new().await;
    let root = writer(&fixture).await;
    let before = registry(&fixture);
    std::fs::rename(&root, root.with_extension("retained")).unwrap();
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("agent.json"), "replacement directory owner").unwrap();
    let result = desktop_agents::replace_config_field(
        &fixture.server,
        "writer",
        "running",
        json!({"approval_level":"AUTO"}),
    )
    .await;
    let error = result.expect_err("replaced Workspace must reject configuration publication");
    assert_eq!(
        (error.0, error.1.0),
        (
            StatusCode::CONFLICT,
            json!({"detail":"Agent 'writer' Workspace binding has changed"})
        )
    );
    assert_untouched(&fixture, &before, &root);
}

#[tokio::test]
async fn public_settings_reject_a_replaced_directory_without_writing() {
    for (method, path, body) in [
        ("PUT", "/api/agents/writer", json!({"name":"Changed"})),
        (
            "PATCH",
            "/api/agents/writer/model-settings",
            json!({"thinking_level":"high"}),
        ),
        (
            "PUT",
            "/api/models/active",
            json!({"scope":"agent","agent_id":"writer","provider_id":"openai-compatible","model":"cron-fixture"}),
        ),
    ] {
        let fixture = Fixture::new().await;
        let root = writer(&fixture).await;
        let before = registry(&fixture);
        std::fs::rename(&root, root.with_extension("retained")).unwrap();
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("agent.json"), "replacement directory owner").unwrap();
        assert_eq!(
            scoped(&fixture, "writer", method, path, body).await,
            (
                StatusCode::CONFLICT,
                json!({"detail":"Agent 'writer' Workspace binding has changed"})
            ),
            "{method} {path}"
        );
        assert_untouched(&fixture, &before, &root);
    }
}

#[tokio::test]
async fn invalid_markers_reject_every_configuration_publication_without_repair() {
    let fixture = Fixture::new().await;
    let root = writer(&fixture).await;
    let before = registry(&fixture);
    let config = std::fs::read(root.join("agent.json")).unwrap();
    let marker = root.join(MARKER_NAME);
    for (bytes, status, detail) in [
        (
            b"{broken".to_vec(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "Workspace identity file is invalid",
        ),
        (
            vec![b' '; 513],
            StatusCode::INTERNAL_SERVER_ERROR,
            "Workspace identity file is invalid",
        ),
        (
            serde_json::to_vec(&binding(&fixture, "default")).unwrap(),
            StatusCode::CONFLICT,
            "Agent 'writer' Workspace binding has changed",
        ),
    ] {
        std::fs::write(&marker, &bytes).unwrap();
        let expected = (status, json!({"detail":detail}));
        let error = desktop_agents::replace_config_field(
            &fixture.server,
            "writer",
            "active_model",
            Value::Null,
        )
        .await
        .unwrap_err();
        assert_eq!((error.0, error.1.0), expected);
        for (method, path, body) in [
            ("PUT", "/api/agents/writer", json!({"name":"Changed"})),
            (
                "PATCH",
                "/api/agents/writer/model-settings",
                json!({"thinking_level":"high"}),
            ),
            (
                "PUT",
                "/api/models/active",
                json!({"scope":"agent","agent_id":"writer","provider_id":"openai-compatible","model":"cron-fixture"}),
            ),
        ] {
            assert_eq!(
                scoped(&fixture, "writer", method, path, body).await,
                expected,
                "{path}"
            );
        }
        assert_eq!(std::fs::read(&marker).unwrap(), bytes);
        assert_eq!(std::fs::read(root.join("agent.json")).unwrap(), config);
        assert_eq!(registry(&fixture), before);
    }
}

#[tokio::test]
async fn valid_settings_preserve_other_agents_and_reopen_including_disabled_edits() {
    let mut fixture = Fixture::new().await;
    let root = writer(&fixture).await;
    let before = registry(&fixture);
    let marker = std::fs::read(root.join(MARKER_NAME)).unwrap();
    let mut expected = before["agents"]["writer"]["config"].clone();
    let channels = fixture
        .request("GET", "/api/agents/writer", Value::Null)
        .await["channels"]
        .clone();
    expected["name"] = json!("Renamed");
    fixture
        .request("PUT", "/api/agents/writer", json!({"name":"Renamed"}))
        .await;
    expected["active_model"] = json!({"provider_id":"openai-compatible","model":"cron-fixture"});
    assert_eq!(
        desktop_agents::replace_config_field(
            &fixture.server,
            "writer",
            "active_model",
            expected["active_model"].clone()
        )
        .await
        .unwrap(),
        {
            let mut view = expected.clone();
            view["channels"] = channels;
            view
        }
    );
    fixture
        .request(
            "PATCH",
            "/api/agents/writer/toggle",
            json!({"enabled":false}),
        )
        .await;
    expected["thinking_level"] = json!("high");
    fixture
        .request(
            "PATCH",
            "/api/agents/writer/model-settings",
            json!({"thinking_level":"high"}),
        )
        .await;
    let error = desktop_agents::replace_config_field(
        &fixture.server,
        "writer",
        "active_model",
        Value::Null,
    )
    .await
    .unwrap_err();
    assert_eq!(
        (error.0, error.1.0),
        (
            StatusCode::FORBIDDEN,
            json!({"detail":"Agent 'writer' is disabled"})
        )
    );
    for reopened in [false, true] {
        if reopened {
            fixture.reopen().await;
        }
        let current = registry(&fixture);
        assert_eq!(current["agents"]["writer"]["config"], expected);
        assert_eq!(current["agents"]["writer"]["enabled"], false);
        assert_eq!(current["agents"]["default"], before["agents"]["default"]);
        assert_eq!(current["workspace_keys"], before["workspace_keys"]);
        assert_eq!(std::fs::read(root.join(MARKER_NAME)).unwrap(), marker);
        let persisted: Value =
            serde_json::from_slice(&std::fs::read(root.join("agent.json")).unwrap()).unwrap();
        assert_eq!(persisted, expected);
    }
}

#[cfg(unix)]
#[tokio::test]
async fn redirected_directory_with_copied_marker_cannot_receive_configuration() {
    let fixture = Fixture::new().await;
    let root = writer(&fixture).await;
    let before = registry(&fixture);
    let retained = root.with_extension("retained");
    let redirected = root.with_extension("redirected");
    let marker = std::fs::read(root.join(MARKER_NAME)).unwrap();
    std::fs::rename(&root, &retained).unwrap();
    std::fs::create_dir(&redirected).unwrap();
    std::fs::write(redirected.join(MARKER_NAME), &marker).unwrap();
    std::os::unix::fs::symlink(&redirected, &root).unwrap();
    let expected = (
        StatusCode::CONFLICT,
        json!({"detail":"Agent 'writer' Workspace binding has changed"}),
    );
    assert_eq!(
        scoped(
            &fixture,
            "writer",
            "PUT",
            "/api/agents/writer",
            json!({"name":"Changed"})
        )
        .await,
        expected
    );
    let error = desktop_agents::replace_config_field(
        &fixture.server,
        "writer",
        "active_model",
        Value::Null,
    )
    .await
    .unwrap_err();
    assert_eq!((error.0, error.1.0), expected);
    assert!(!redirected.join("agent.json").exists());
    assert_eq!(std::fs::read(redirected.join(MARKER_NAME)).unwrap(), marker);
    assert_eq!(std::fs::read_link(root).unwrap(), redirected);
    assert_eq!(registry(&fixture), before);
}

#[tokio::test]
async fn publication_rechecks_identity_after_waiting_for_the_registration_lock() {
    let fixture = Fixture::new().await;
    let root = writer(&fixture).await;
    let before = registry(&fixture);
    let guard = fixture.server.inner.desktop_agents_lock.lock().await;
    let publication = desktop_agents::replace_config_field(
        &fixture.server,
        "writer",
        "active_model",
        Value::Null,
    );
    tokio::pin!(publication);
    assert!(matches!(
        futures_util::poll!(&mut publication),
        std::task::Poll::Pending
    ));
    std::fs::rename(&root, root.with_extension("retained")).unwrap();
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("agent.json"), "replacement directory owner").unwrap();
    drop(guard);
    let error = publication.await.unwrap_err();
    assert_eq!(
        (error.0, error.1.0),
        (
            StatusCode::CONFLICT,
            json!({"detail":"Agent 'writer' Workspace binding has changed"})
        )
    );
    assert_untouched(&fixture, &before, &root);
}

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_agents_page_preserves_settings_crud_and_reopen_with_binding_checks() {
    let mut fixture = Fixture::new().await;
    let before = registry(&fixture)["agents"]["default"].clone();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    fixture.server.inner.shutdown.cancel();
    fixture.server = AppServer::new_desktop_with_stores_and_workspace(
        fixture.server.inner.core.clone(),
        &root.join("../console/dist"),
        String::from("config-binding-browser-shutdown"),
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
            .args([&origin, "/agents", "--agents-crud"])
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
        report["pages"][0]["agentsCrud"],
        json!({
            "createdThroughOriginalModal":true,"editedThroughOriginalModal":true,
            "copiedThroughOriginalModal":true,"pinnedThroughOriginalTable":true,
            "toggledThroughOriginalTable":true,"deletedThroughOriginalTable":true,
            "selectedThroughOriginalSidebar":true,"selectedAgent":"browser-agent",
            "workspaceFilesIsolated":true
        })
    );
    let after = registry(&fixture);
    assert_eq!(after["agents"]["default"], before);
    assert_eq!(after["agents"].as_object().unwrap().len(), 2);
    assert_eq!(
        after["agents"]["browser-agent"]["config"]["name"],
        "Browser Agent Updated"
    );
    fixture.reopen().await;
    assert_eq!(registry(&fixture), after);
    assert!(fixture.remote.requests.lock().unwrap().is_empty());
}
