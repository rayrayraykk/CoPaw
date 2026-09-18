use super::*;
use pretty_assertions::assert_eq;

async fn automatic(fixture: &Fixture, thread: &str, agent: &desktop_agents::AgentContext) {
    crate::desktop_checkpoints::maybe_create_auto_checkpoint(
        &fixture.server,
        thread,
        agent,
        None,
        &fixture.server.inner.shutdown,
    )
    .await;
}

#[tokio::test]
async fn checkpoint_identity_completion_keeps_the_admitted_owner_after_agent_name_reuse() {
    let fixture = Fixture::new().await;
    actor(&fixture, "writer").await;
    let thread = chat(&fixture, "writer", "session", "external-project").await;
    api(
        &fixture,
        "writer",
        "PATCH",
        "/auto",
        json!({"enabled":true}),
    )
    .await;
    let admitted = desktop_agents::context_for_agent(&fixture.server, "writer")
        .await
        .unwrap();
    fixture
        .request("DELETE", "/api/agents/writer", Value::Null)
        .await;
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"editor","name":"Editor","workspace_dir":admitted.workspace}),
        )
        .await;
    fixture.request("POST", "/api/agents", json!({"id":"writer","name":"New writer","workspace_dir":fixture.directory.path().join("new-writer")})).await;
    api(
        &fixture,
        "writer",
        "PATCH",
        "/auto",
        json!({"enabled":true}),
    )
    .await;
    let before = api(&fixture, "writer", "GET", "/graph", Value::Null).await;
    // Deletion drains runs while holding this lock. Completion cannot acquire it.
    let lifecycle = fixture
        .server
        .inner
        .desktop_agent_lifecycle_lock
        .lock()
        .await;
    tokio::time::timeout(
        Duration::from_secs(2),
        automatic(&fixture, &thread, &admitted),
    )
    .await
    .unwrap();
    drop(lifecycle);
    assert_eq!(
        api(&fixture, "writer", "GET", "/graph", Value::Null).await,
        before
    );
    assert_eq!(
        api(&fixture, "editor", "GET", "/graph", Value::Null).await["summary"],
        json!({"total":1,"auto":1,"snapshots":0,"safety":0,"heads":1})
    );
}

#[tokio::test]
async fn checkpoint_identity_cancellation_while_waiting_does_not_write_an_auto_snapshot() {
    let fixture = Fixture::new().await;
    let thread = chat(&fixture, "default", "session", "external-project").await;
    api(
        &fixture,
        "default",
        "PATCH",
        "/auto",
        json!({"enabled":true}),
    )
    .await;
    let admitted = desktop_agents::context_for_agent(&fixture.server, "default")
        .await
        .unwrap();
    let before = api(&fixture, "default", "GET", "/graph", Value::Null).await;
    let cancellation = tokio_util::sync::CancellationToken::new();
    let guard = fixture.server.inner.desktop_checkpoint_lock.lock().await;
    let completion = crate::desktop_checkpoints::maybe_create_auto_checkpoint(
        &fixture.server,
        &thread,
        &admitted,
        None,
        &cancellation,
    );
    tokio::pin!(completion);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut completion)
            .await
            .is_err()
    );
    cancellation.cancel();
    drop(guard);
    tokio::time::timeout(Duration::from_secs(2), completion)
        .await
        .unwrap();
    assert_eq!(
        api(&fixture, "default", "GET", "/graph", Value::Null).await,
        before
    );
}

#[tokio::test]
async fn checkpoint_identity_marker_replacement_while_waiting_cannot_write_old_state() {
    let fixture = Fixture::new().await;
    actor(&fixture, "writer").await;
    let thread = chat(&fixture, "writer", "session", "external-project").await;
    api(
        &fixture,
        "writer",
        "PATCH",
        "/auto",
        json!({"enabled":true}),
    )
    .await;
    let admitted = desktop_agents::context_for_agent(&fixture.server, "writer")
        .await
        .unwrap();
    let state = crate::desktop_checkpoints::state_directory(
        &fixture.directory.path().join("data"),
        &admitted.data_key,
    )
    .join("state.json");
    let before = std::fs::read(&state).unwrap();
    let guard = fixture.server.inner.desktop_checkpoint_lock.lock().await;
    let completion = automatic(&fixture, &thread, &admitted);
    tokio::pin!(completion);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut completion)
            .await
            .is_err()
    );
    let marker = desktop_agents::identity::MARKER_NAME;
    std::fs::copy(
        fixture.directory.path().join("workspace").join(marker),
        admitted.workspace.join(marker),
    )
    .unwrap();
    drop(guard);
    tokio::time::timeout(Duration::from_secs(2), completion)
        .await
        .unwrap();
    assert_eq!(std::fs::read(state).unwrap(), before);
}

#[tokio::test]
async fn checkpoint_identity_foreign_state_is_rejected_without_resetting_or_changing_it() {
    let fixture = Fixture::new().await;
    actor(&fixture, "writer").await;
    api(
        &fixture,
        "writer",
        "PATCH",
        "/auto",
        json!({"enabled":true}),
    )
    .await;
    let writer = desktop_agents::context_for_agent(&fixture.server, "writer")
        .await
        .unwrap();
    let default = desktop_agents::context_for_agent(&fixture.server, "default")
        .await
        .unwrap();
    let path = crate::desktop_checkpoints::state_directory(
        &fixture.directory.path().join("data"),
        &writer.data_key,
    )
    .join("state.json");
    let mut state: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    state["identity"]["data_key"] = serde_json::to_value(default.data_key).unwrap();
    let bytes = serde_json::to_vec(&state).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    for (method, suffix, body) in [
        ("GET", "/status", Value::Null),
        ("GET", "/graph", Value::Null),
        ("PATCH", "/auto", json!({"enabled":false})),
        ("DELETE", "", Value::Null),
    ] {
        assert_eq!(
            scoped(
                &fixture,
                "writer",
                method,
                &format!("/api/workspace/checkpoints{suffix}"),
                body
            )
            .await,
            (
                StatusCode::CONFLICT,
                json!({"detail":"Checkpoint state Workspace identity does not match"})
            )
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}

#[tokio::test]
async fn checkpoint_identity_new_directory_at_the_same_path_does_not_inherit_settings() {
    let mut fixture = Fixture::new().await;
    actor(&fixture, "writer").await;
    api(
        &fixture,
        "writer",
        "PATCH",
        "/auto",
        json!({"enabled":true}),
    )
    .await;
    let old_key = fixture.data_key("writer");
    let root = fixture.directory.path().join("data/workspaces/writer");
    fixture
        .request("DELETE", "/api/agents/writer", Value::Null)
        .await;
    std::fs::rename(&root, fixture.directory.path().join("retained-writer")).unwrap();
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"New writer","workspace_dir":root}),
        )
        .await;
    assert_ne!(fixture.data_key("writer"), old_key);
    fixture.reopen().await;
    assert_eq!(
        api(&fixture, "writer", "GET", "/status", Value::Null).await,
        json!({"auto_enabled":false,"has_checkpoints":false,"workspace_dir":root.canonicalize().unwrap()})
    );
}

#[tokio::test]
async fn checkpoint_identity_auto_uses_the_admitted_base_not_another_agents_project_root() {
    let fixture = Fixture::new().await;
    actor(&fixture, "writer").await;
    let thread = chat(&fixture, "writer", "writer-session", "workspace").await;
    api(
        &fixture,
        "default",
        "PATCH",
        "/auto",
        json!({"enabled":true}),
    )
    .await;
    api(
        &fixture,
        "writer",
        "PATCH",
        "/auto",
        json!({"enabled":true}),
    )
    .await;
    let before = api(&fixture, "default", "GET", "/graph", Value::Null).await;
    let admitted = desktop_agents::context_for_agent(&fixture.server, "writer")
        .await
        .unwrap();
    crate::desktop_checkpoints::maybe_create_auto_checkpoint(
        &fixture.server,
        &thread,
        &admitted,
        None,
        &fixture.server.inner.shutdown,
    )
    .await;
    assert_eq!(
        api(&fixture, "default", "GET", "/graph", Value::Null).await,
        before
    );
    assert_eq!(
        api(&fixture, "writer", "GET", "/graph", Value::Null).await["summary"],
        json!({"total":1,"auto":1,"snapshots":0,"safety":0,"heads":1})
    );
}

#[tokio::test]
async fn checkpoint_identity_unbound_legacy_data_is_preserved_and_not_silently_adopted() {
    let fixture = Fixture::new().await;
    let root = fixture
        .directory
        .path()
        .join("workspace")
        .canonicalize()
        .unwrap();
    let key = format!(
        "{:x}",
        sha2::Sha256::digest(root.to_string_lossy().as_bytes())
    );
    let legacy = fixture.directory.path().join("data/checkpoints").join(key);
    std::fs::create_dir_all(&legacy).unwrap();
    let bytes =
        serde_json::to_vec(&json!({"version":1,"auto_enabled":true,"heads":{},"entries":[]}))
            .unwrap();
    std::fs::write(legacy.join("state.json"), &bytes).unwrap();
    let result = scoped(
        &fixture,
        "default",
        "GET",
        "/api/workspace/checkpoints/status",
        Value::Null,
    )
    .await;
    assert_eq!(result.0, StatusCode::CONFLICT, "{result:?}");
    assert_eq!(std::fs::read(legacy.join("state.json")).unwrap(), bytes);
}
