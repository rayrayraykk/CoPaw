//! Directory generations must not inherit data solely through path reuse.

use super::*;
use crate::desktop_agents::identity::MARKER_NAME;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn runtime_snapshot_preserves_base_identity_when_project_and_config_change() {
    use crate::desktop_agents;

    let fixture = Fixture::new().await;
    let created = fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Writer"}),
        )
        .await;
    let root = std::path::PathBuf::from(created["workspace_dir"].as_str().unwrap());
    let project = fixture.directory.path().join("project-without-core-marker");
    std::fs::create_dir(&project).unwrap();
    let project = project.canonicalize().unwrap();
    let before = desktop_agents::context_for_agent(&fixture.server, "writer")
        .await
        .unwrap();
    assert_eq!(
        before,
        desktop_agents::AgentContext {
            agent_id: String::from("writer"),
            data_key: serde_json::from_value(binding(&fixture, "writer")).unwrap(),
            workspace: root.clone(),
            config: fixture
                .request("GET", "/api/agents/writer", Value::Null)
                .await,
        }
    );
    assert_eq!(
        registry(&fixture)["agents"]["writer"]["config"]["channels"],
        json!({})
    );
    let next_config = desktop_agents::replace_config_field(
        &fixture.server,
        "writer",
        "project_dir",
        json!(project),
    )
    .await
    .unwrap();
    let after = desktop_agents::context_for_agent(&fixture.server, "writer")
        .await
        .unwrap();
    assert_eq!(
        after,
        desktop_agents::AgentContext {
            config: next_config,
            ..before.clone()
        }
    );
    assert_eq!(
        desktop_agents::project_for_agent(&fixture.server, "writer")
            .await
            .unwrap(),
        project
    );
    assert_eq!(
        desktop_agents::workspace_for_agent(&fixture.server, "writer")
            .await
            .unwrap(),
        root
    );
    assert!(!project.join(MARKER_NAME).exists());
    assert!(before.config.get("project_dir").is_none());

    fixture
        .request(
            "PATCH",
            "/api/agents/writer/toggle",
            json!({"enabled":false}),
        )
        .await;
    for (id, status, detail) in [
        (
            "writer",
            StatusCode::FORBIDDEN,
            "Agent 'writer' is disabled",
        ),
        (
            "missing",
            StatusCode::NOT_FOUND,
            "Agent 'missing' not found",
        ),
    ] {
        let error = desktop_agents::context_for_agent(&fixture.server, id)
            .await
            .unwrap_err();
        assert_eq!((error.0, error.1.0), (status, json!({"detail":detail})));
    }
}

#[tokio::test]
async fn invalid_live_marker_is_not_repaired_by_runtime_reads() {
    let fixture = Fixture::new().await;
    let root = fixture.directory.path().join("workspace");
    let marker = root.join(MARKER_NAME);
    let before = registry(&fixture);
    for bytes in [
        b"{broken".to_vec(),
        vec![b' '; 513],
        b"{\"kind\":\"workspace\",\"id\":\"00000000-0000-0000-0000-000000000000\"}".to_vec(),
    ] {
        std::fs::write(&marker, &bytes).unwrap();
        let error = crate::desktop_agents::context_for_agent(&fixture.server, "default")
            .await
            .unwrap_err();
        assert_eq!(error.0, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(std::fs::read(&marker).unwrap(), bytes);
        assert_eq!(registry(&fixture), before);
    }
}

#[cfg(unix)]
#[tokio::test]
async fn live_symlink_redirect_with_copied_marker_does_not_match_registered_path() {
    let fixture = Fixture::new().await;
    let created = fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Writer"}),
        )
        .await;
    let root = std::path::PathBuf::from(created["workspace_dir"].as_str().unwrap());
    let redirected = root.with_extension("redirected");
    let marker = std::fs::read(root.join(MARKER_NAME)).unwrap();
    let before = registry(&fixture);
    std::fs::rename(&root, root.with_extension("retained")).unwrap();
    std::fs::create_dir(&redirected).unwrap();
    std::fs::write(redirected.join(MARKER_NAME), &marker).unwrap();
    std::os::unix::fs::symlink(&redirected, &root).unwrap();
    assert_eq!(
        scoped(
            &fixture,
            "writer",
            "GET",
            "/api/workspace/tree",
            Value::Null
        )
        .await,
        (
            StatusCode::CONFLICT,
            json!({"detail":"Agent 'writer' Workspace binding has changed"})
        )
    );
    assert_eq!(registry(&fixture), before);
    assert_eq!(std::fs::read(redirected.join(MARKER_NAME)).unwrap(), marker);
    assert_eq!(std::fs::read_link(root).unwrap(), redirected);
}

#[tokio::test]
async fn live_workspace_replacement_is_rejected_without_rebinding_or_touching_files() {
    let fixture = Fixture::new().await;
    let created = fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Writer"}),
        )
        .await;
    let root = std::path::PathBuf::from(created["workspace_dir"].as_str().unwrap());
    let retained = root.with_extension("retained");
    let before = registry(&fixture);
    std::fs::rename(&root, &retained).unwrap();
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("replacement.txt"), "not the registered workspace").unwrap();
    for (method, path, body) in [
        ("GET", "/api/workspace/tree", Value::Null),
        (
            "GET",
            "/api/workspace/file-content?path=replacement.txt",
            Value::Null,
        ),
        (
            "PUT",
            "/api/workspace/file-content?path=replacement.txt",
            json!({"content":"overwritten"}),
        ),
        (
            "POST",
            "/api/chats",
            json!({"name":"Wrong directory","session_id":"replacement","user_id":"fixture"}),
        ),
    ] {
        assert_eq!(
            scoped(&fixture, "writer", method, path, body).await,
            (
                StatusCode::CONFLICT,
                json!({"detail":"Agent 'writer' Workspace binding has changed"})
            ),
            "{method} {path}"
        );
    }
    assert_eq!(registry(&fixture), before);
    assert!(!root.join(MARKER_NAME).exists());
    assert_eq!(
        std::fs::read_to_string(root.join("replacement.txt")).unwrap(),
        "not the registered workspace"
    );
    assert_eq!(
        scoped(
            &fixture,
            "default",
            "GET",
            "/api/workspace/tree",
            Value::Null
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        scoped(&fixture, "writer", "GET", "/api/agents", Value::Null)
            .await
            .0,
        StatusCode::OK
    );
    std::fs::rename(&root, root.with_extension("replacement")).unwrap();
    std::fs::rename(&retained, &root).unwrap();
    assert_eq!(
        scoped(
            &fixture,
            "writer",
            "GET",
            "/api/workspace/tree",
            Value::Null
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(registry(&fixture), before);
}

#[tokio::test]
async fn missing_or_different_live_marker_cannot_supply_runtime_config_or_workspace() {
    let fixture = Fixture::new().await;
    let created = fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Writer"}),
        )
        .await;
    let marker = std::path::Path::new(created["workspace_dir"].as_str().unwrap()).join(MARKER_NAME);
    let original = std::fs::read(&marker).unwrap();
    let before = registry(&fixture);
    let different = serde_json::to_vec(&binding(&fixture, "default")).unwrap();
    std::fs::rename(&marker, marker.with_extension("retained")).unwrap();
    for bytes in [None, Some(different)] {
        if let Some(bytes) = &bytes {
            std::fs::write(&marker, bytes).unwrap();
        }
        let expected = (
            StatusCode::CONFLICT,
            json!({"detail":"Agent 'writer' Workspace binding has changed"}),
        );
        assert_eq!(
            scoped(
                &fixture,
                "writer",
                "GET",
                "/api/workspace/tree",
                Value::Null
            )
            .await,
            expected
        );
        for error in [
            crate::desktop_agents::config_for_agent(&fixture.server, "writer")
                .await
                .unwrap_err(),
            crate::desktop_agents::context_for_agent(&fixture.server, "writer")
                .await
                .map(|context| context.model())
                .unwrap_err(),
            crate::desktop_agents::project_for_agent(&fixture.server, "writer")
                .await
                .unwrap_err(),
        ] {
            assert_eq!((error.0, error.1.0), expected);
        }
        assert_eq!(std::fs::read(&marker).ok(), bytes);
        assert_eq!(registry(&fixture), before);
    }
    std::fs::write(&marker, original).unwrap();
    assert!(
        crate::desktop_agents::config_for_agent(&fixture.server, "writer")
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn original_file_tree_hides_metadata_and_direct_paths_cannot_modify_it() {
    let fixture = Fixture::new().await;
    let root = fixture.directory.path().join("workspace");
    let before = std::fs::read(root.join(MARKER_NAME)).unwrap();
    let tree = fixture
        .request("GET", "/api/workspace/tree", Value::Null)
        .await;
    assert!(!tree.to_string().contains(MARKER_NAME));
    for name in [MARKER_NAME.to_owned(), MARKER_NAME.to_uppercase()] {
        let path = format!("/api/workspace/file-content?path={name}");
        assert_eq!(
            scoped(&fixture, "default", "GET", &path, Value::Null)
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            scoped(
                &fixture,
                "default",
                "PUT",
                &path,
                json!({"content":"replacement"})
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(std::fs::read(root.join(MARKER_NAME)).unwrap(), before);
}

#[tokio::test]
async fn replacing_a_retained_directory_before_registration_does_not_reuse_its_data() {
    let mut fixture = Fixture::new().await;
    let created = fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Writer"}),
        )
        .await;
    let root = std::path::PathBuf::from(created["workspace_dir"].as_str().unwrap());
    let original = binding(&fixture, "writer");
    fixture
        .request("DELETE", "/api/agents/writer", Value::Null)
        .await;
    std::fs::rename(&root, root.with_extension("retained")).unwrap();
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("new-user-file"), "new contents").unwrap();
    fixture.reopen().await;
    assert!(!root.join(MARKER_NAME).exists());
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Replacement"}),
        )
        .await;
    let replacement = binding(&fixture, "writer");
    assert_ne!(replacement, original);
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(root.join(MARKER_NAME)).unwrap()).unwrap(),
        replacement
    );
    assert_eq!(
        std::fs::read_to_string(root.join("new-user-file")).unwrap(),
        "new contents"
    );
    assert_eq!(
        serde_json::from_slice::<Value>(
            &std::fs::read(root.with_extension("retained").join(MARKER_NAME)).unwrap()
        )
        .unwrap(),
        original
    );
}

#[tokio::test]
async fn failed_custom_replacement_can_retry_without_reattaching_the_old_index() {
    let mut fixture = Fixture::new().await;
    let root = fixture.directory.path().join("custom-generation");
    let body = json!({"id":"writer","name":"Writer","workspace_dir":root});
    fixture.request("POST", "/api/agents", body.clone()).await;
    let original = binding(&fixture, "writer");
    fixture
        .request("DELETE", "/api/agents/writer", Value::Null)
        .await;
    std::fs::rename(&root, root.with_extension("retained")).unwrap();
    let before = registry(&fixture);
    assert_eq!(
        fail_next_creation(&mut fixture, body.clone()).await.0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(registry(&fixture), before);
    assert!(root.is_dir());
    let pending: Value =
        serde_json::from_slice(&std::fs::read(root.join(MARKER_NAME)).unwrap()).unwrap();
    assert_ne!(pending, original);
    fixture.reopen().await;
    fixture.request("POST", "/api/agents", body).await;
    assert_ne!(binding(&fixture, "writer"), original);
    let published = binding(&fixture, "writer");
    std::fs::write(root.join("later-edit"), "ordinary editing").unwrap();
    fixture
        .request("DELETE", "/api/agents/writer", Value::Null)
        .await;
    fixture.reopen().await;
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"editor","name":"Editor","workspace_dir":root}),
        )
        .await;
    assert_eq!(binding(&fixture, "editor"), published);
}

#[tokio::test]
async fn v2_upgrade_establishes_once_and_does_not_repair_a_later_missing_marker() {
    let mut fixture = Fixture::new().await;
    let created = fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Writer"}),
        )
        .await;
    let root = std::path::Path::new(created["workspace_dir"].as_str().unwrap());
    let original = binding(&fixture, "writer");
    let mut old = registry(&fixture);
    old["schema_version"] = json!(2);
    std::fs::remove_file(root.join(MARKER_NAME)).unwrap();
    std::fs::write(
        fixture.directory.path().join("data/agents/catalog.json"),
        serde_json::to_vec(&old).unwrap(),
    )
    .unwrap();
    fixture.reopen().await;
    assert_eq!(registry(&fixture)["schema_version"], 3);
    assert_eq!(binding(&fixture, "writer"), original);
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(root.join(MARKER_NAME)).unwrap()).unwrap(),
        original
    );
    fixture
        .request("DELETE", "/api/agents/writer", Value::Null)
        .await;
    std::fs::remove_file(root.join(MARKER_NAME)).unwrap();
    fixture.reopen().await;
    assert!(!root.join(MARKER_NAME).exists());
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"New generation"}),
        )
        .await;
    assert_ne!(binding(&fixture, "writer"), original);
}

#[tokio::test]
async fn corrupt_and_oversized_markers_fail_without_changing_registration_or_marker() {
    let fixture = Fixture::new().await;
    let created = fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Writer"}),
        )
        .await;
    let marker = std::path::Path::new(created["workspace_dir"].as_str().unwrap()).join(MARKER_NAME);
    fixture
        .request("DELETE", "/api/agents/writer", Value::Null)
        .await;
    let before = registry(&fixture);
    for bytes in [b"invalid json".to_vec(), vec![b' '; 513]] {
        std::fs::write(&marker, &bytes).unwrap();
        assert_eq!(
            scoped(
                &fixture,
                "default",
                "POST",
                "/api/agents",
                json!({"id":"writer","name":"Writer"})
            )
            .await
            .0,
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(registry(&fixture), before);
        assert_eq!(std::fs::read(&marker).unwrap(), bytes);
    }
}

#[cfg(unix)]
#[tokio::test]
async fn linked_marker_is_rejected_without_reading_or_replacing_its_target() {
    let fixture = Fixture::new().await;
    let root = fixture.directory.path().join("custom-link");
    std::fs::create_dir(&root).unwrap();
    let target = fixture.directory.path().join("private-marker-target");
    std::fs::write(&target, "untouched").unwrap();
    std::os::unix::fs::symlink(&target, root.join(MARKER_NAME)).unwrap();
    let before = registry(&fixture);
    assert_eq!(
        scoped(
            &fixture,
            "default",
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Writer","workspace_dir":root})
        )
        .await
        .0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "untouched");
    assert_eq!(std::fs::read_link(root.join(MARKER_NAME)).unwrap(), target);
    assert_eq!(registry(&fixture), before);
}
