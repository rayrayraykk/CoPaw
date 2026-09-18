//! Public checkpoint operations must use the requested base Workspace.

use super::super::scope::scoped;
use super::*;
use pretty_assertions::assert_eq;

#[path = "desktop_checkpoint_browser_tests.rs"]
mod browser;

#[path = "desktop_checkpoint_identity_tests.rs"]
mod identity;

#[path = "desktop_checkpoint_runtime_tests.rs"]
mod runtime;

#[path = "desktop_checkpoint_quiescence_tests.rs"]
mod quiescence;

async fn api(fixture: &Fixture, actor: &str, method: &str, suffix: &str, body: Value) -> Value {
    let (status, value) = scoped(
        fixture,
        actor,
        method,
        &format!("/api/workspace/checkpoints{suffix}"),
        body,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    value
}

#[tokio::test]
async fn checkpoint_workspace_manual_write_finishes_before_agent_deletion_and_reuse() {
    let fixture = Fixture::new().await;
    actor(&fixture, "writer").await;
    {
        let guard = fixture.server.inner.desktop_checkpoint_lock.lock().await;
        let mutation = api(
            &fixture,
            "writer",
            "PATCH",
            "/auto",
            json!({"enabled":true}),
        );
        tokio::pin!(mutation);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut mutation)
                .await
                .is_err()
        );
        assert!(
            fixture
                .server
                .inner
                .desktop_agent_lifecycle_lock
                .try_lock()
                .is_err()
        );
        let deletion = fixture.request("DELETE", "/api/agents/writer", Value::Null);
        tokio::pin!(deletion);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut deletion)
                .await
                .is_err()
        );
        drop(guard);
        assert_eq!(mutation.await, json!({"auto_enabled":true}));
        deletion.await;
    }
    fixture.request("POST", "/api/agents", json!({"id":"editor","name":"Editor","workspace_dir":fixture.directory.path().join("data/workspaces/writer")})).await;
    fixture.request("POST", "/api/agents", json!({"id":"writer","name":"New writer","workspace_dir":fixture.directory.path().join("new-writer")})).await;
    for (id, path, enabled) in [
        ("editor", "data/workspaces/writer", true),
        ("writer", "new-writer", false),
    ] {
        assert_eq!(
            api(&fixture, id, "GET", "/status", Value::Null).await,
            json!({"auto_enabled":enabled,"has_checkpoints":false,"workspace_dir":fixture.directory.path().join(path).canonicalize().unwrap()})
        );
    }
}

#[tokio::test]
async fn checkpoint_workspace_settings_use_the_requested_base_not_shared_project() {
    let mut fixture = Fixture::new().await;
    actor(&fixture, "writer").await;
    chat(&fixture, "default", "same-session", "shared-project").await;
    chat(&fixture, "writer", "same-session", "shared-project").await;
    for id in ["default", "writer"] {
        fixture
            .request(
                "PUT",
                &format!("/api/agents/{id}"),
                json!({"project_dir":fixture.directory.path().join("shared-project")}),
            )
            .await;
    }
    assert_eq!(
        api(
            &fixture,
            "writer",
            "PATCH",
            "/auto",
            json!({"enabled":true})
        )
        .await,
        json!({"auto_enabled":true})
    );
    for (id, path, enabled) in [
        ("default", "workspace", false),
        ("writer", "data/workspaces/writer", true),
    ] {
        assert_eq!(
            api(&fixture, id, "GET", "/status", Value::Null).await,
            json!({"auto_enabled":enabled,"has_checkpoints":false,"workspace_dir":fixture.directory.path().join(path).canonicalize().unwrap()})
        );
    }
    let settings = json!({"gc_keep_count":4,"gc_keep_days":2,"pre_restore_retention_days":1});
    assert_eq!(
        api(
            &fixture,
            "writer",
            "PATCH",
            "/gc/settings",
            settings.clone()
        )
        .await,
        settings
    );
    fixture.reopen().await;
    fixture
        .request(
            "PUT",
            "/api/agents/writer",
            json!({"project_dir":fixture.directory.path().join("workspace")}),
        )
        .await;
    assert_eq!(
        api(&fixture, "writer", "GET", "/gc/settings", Value::Null).await,
        settings
    );
    api(&fixture, "default", "DELETE", "", Value::Null).await;
    assert_eq!(
        api(&fixture, "writer", "GET", "/gc/settings", Value::Null).await,
        settings
    );
}

#[tokio::test]
async fn checkpoint_workspace_invalid_requests_never_mutate_default_state() {
    let fixture = Fixture::new().await;
    actor(&fixture, "disabled").await;
    actor(&fixture, "mismatch").await;
    fixture
        .request(
            "PATCH",
            "/api/agents/disabled/toggle",
            json!({"enabled":false}),
        )
        .await;
    let marker = desktop_agents::identity::MARKER_NAME;
    std::fs::copy(
        fixture.directory.path().join("workspace").join(marker),
        fixture
            .directory
            .path()
            .join("data/workspaces/mismatch")
            .join(marker),
    )
    .unwrap();
    api(
        &fixture,
        "default",
        "PATCH",
        "/auto",
        json!({"enabled":true}),
    )
    .await;
    let before = api(&fixture, "default", "GET", "/status", Value::Null).await;
    let state_path = std::fs::read_dir(fixture.directory.path().join("data/checkpoints"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
        .join("state.json");
    let state_before = std::fs::read(&state_path).unwrap();
    let restore = json!({"commit":"f".repeat(64),"session_id":"session","user_id":"desktop","channel":"console"});
    let requests = [
        ("GET", "/status", Value::Null),
        ("GET", "/graph", Value::Null),
        ("PATCH", "/auto", json!({"enabled":false})),
        ("POST", "/snapshot", json!({"session_id":"session"})),
        ("POST", "/restore/preview", restore.clone()),
        ("POST", "/restore", restore),
        ("POST", "/gc/preview", json!({})),
        ("POST", "/gc", json!({})),
        ("GET", "/gc/settings", Value::Null),
        (
            "PATCH",
            "/gc/settings",
            json!({"gc_keep_count":0,"gc_keep_days":0,"pre_restore_retention_days":0}),
        ),
        ("DELETE", "", Value::Null),
    ];
    for (agent, expected_status, detail) in [
        (
            "missing",
            StatusCode::NOT_FOUND,
            "Agent 'missing' not found",
        ),
        (
            "disabled",
            StatusCode::FORBIDDEN,
            "Agent 'disabled' is disabled",
        ),
        (
            "mismatch",
            StatusCode::CONFLICT,
            "Agent 'mismatch' Workspace binding has changed",
        ),
    ] {
        for (method, suffix, body) in &requests {
            let result = scoped(
                &fixture,
                agent,
                method,
                &format!("/api/workspace/checkpoints{suffix}"),
                body.clone(),
            )
            .await;
            assert_eq!(
                result,
                (expected_status, json!({"detail":detail})),
                "{method} {suffix}"
            );
        }
    }
    assert_eq!(std::fs::read(state_path).unwrap(), state_before);
    assert_eq!(
        api(&fixture, "default", "GET", "/status", Value::Null).await,
        before
    );
}

#[tokio::test]
async fn checkpoint_workspace_nondefault_restores_files_memory_and_gc_without_touching_default() {
    let mut fixture = Fixture::new().await;
    actor(&fixture, "writer").await;
    let thread_id = chat(
        &fixture,
        "writer",
        "writer-session",
        "data/workspaces/writer",
    )
    .await;
    chat(&fixture, "default", "default-session", "workspace").await;
    let root = fixture.directory.path().join("data/workspaces/writer");
    let (status, config) = scoped(&fixture, "writer", "PUT", "/api/workspace/running-config",
        json!({"reme_light_memory_config":{"daily_dir":"writer-daily","digest_dir":"writer-digest"}})).await;
    assert_eq!(status, StatusCode::OK, "{config}");
    std::fs::create_dir_all(root.join("writer-daily")).unwrap();
    std::fs::write(root.join("writer-daily/day.md"), "original memory").unwrap();
    std::fs::write(root.join("notes.txt"), "original notes").unwrap();
    std::fs::write(
        fixture.directory.path().join("workspace/notes.txt"),
        "default notes",
    )
    .unwrap();
    let default_graph = api(&fixture, "default", "GET", "/graph", Value::Null).await;
    let snapshot = api(
        &fixture,
        "writer",
        "POST",
        "/snapshot",
        json!({"session_id":"writer-session","user_id":"desktop","name":"Before edit"}),
    )
    .await;
    let request = json!({"commit":snapshot["commit"],"session_id":"writer-session","user_id":"desktop","include_memory":true,"include_files":true,"files":["notes.txt"]});
    for suffix in ["/restore/preview", "/restore"] {
        assert_eq!(
            scoped(
                &fixture,
                "default",
                "POST",
                &format!("/api/workspace/checkpoints{suffix}"),
                request.clone()
            )
            .await,
            (
                StatusCode::NOT_FOUND,
                json!({"detail":"Checkpoint was not found"})
            )
        );
    }
    std::fs::write(root.join("writer-daily/day.md"), "edited memory").unwrap();
    std::fs::write(root.join("notes.txt"), "edited notes").unwrap();
    fixture.reopen().await;
    let expected = json!({"target":snapshot["commit"],"commit":snapshot["commit"],
        "restored_paths":["sessions/writer-session.json","notes.txt","writer-daily/day.md"],
        "deleted_paths":[],"file_paths":["notes.txt"],"pre_restore_ref":null,
        "dry_run":true,"include_memory":true,"include_files":true});
    assert_eq!(
        api(
            &fixture,
            "writer",
            "POST",
            "/restore/preview",
            request.clone()
        )
        .await,
        expected
    );
    let restored = api(&fixture, "writer", "POST", "/restore", request).await;
    let safety = restored["pre_restore_ref"].as_str().unwrap();
    assert!(safety.starts_with("refs/pre-restore/"), "{restored}");
    let mut applied = expected;
    applied["dry_run"] = json!(false);
    applied["pre_restore_ref"] = json!(safety);
    assert_eq!(restored, applied);
    assert_eq!(
        std::fs::read_to_string(root.join("writer-daily/day.md")).unwrap(),
        "original memory"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("notes.txt")).unwrap(),
        "original notes"
    );
    assert_eq!(
        std::fs::read_to_string(fixture.directory.path().join("workspace/notes.txt")).unwrap(),
        "default notes"
    );
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .read_thread(&thread_id)
            .await
            .unwrap()
            .thread
            .workspace_root,
        Some(root.canonicalize().unwrap().to_string_lossy().into_owned())
    );
    assert_gc_and_reset_preserve_default(&fixture, &snapshot, safety, &default_graph).await;
}

async fn assert_gc_and_reset_preserve_default(
    fixture: &Fixture,
    snapshot: &Value,
    safety: &str,
    default_graph: &Value,
) {
    let graph = api(fixture, "writer", "GET", "/graph", Value::Null).await;
    assert_eq!(
        graph["summary"],
        json!({"total":2,"auto":0,"snapshots":1,"safety":1,"heads":1})
    );
    for (suffix, dry_run) in [("/gc/preview", true), ("/gc", false)] {
        assert_eq!(
            api(
                fixture,
                "writer",
                "POST",
                suffix,
                json!({"keep_count":0,"keep_days":0,"pre_restore_days":0})
            )
            .await,
            json!({"deleted_refs":[safety],"kept_refs":[],"dry_run":dry_run})
        );
    }
    let retained = api(fixture, "writer", "GET", "/graph", Value::Null).await;
    assert_eq!(retained["nodes"][0]["commit"], snapshot["commit"]);
    assert_eq!(
        retained["summary"],
        json!({"total":1,"auto":0,"snapshots":1,"safety":0,"heads":1})
    );
    assert_eq!(
        api(fixture, "writer", "DELETE", "", Value::Null).await,
        json!({"reset":true,"auto_enabled":false})
    );
    assert_eq!(
        &api(fixture, "default", "GET", "/graph", Value::Null).await,
        default_graph
    );
}
