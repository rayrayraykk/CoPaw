//! Creation rollback must not delete a Workspace retained by Agent deletion.

use super::scope::scoped;
use super::*;
use pretty_assertions::assert_eq;

#[path = "desktop_workspace_generation_tests.rs"]
mod generation;

#[path = "desktop_agent_config_binding_tests.rs"]
mod config_binding;

#[path = "desktop_channel_scope_tests.rs"]
mod channel_scope;

#[path = "desktop_channel_profile_tests.rs"]
mod channel_profile;

fn registry(fixture: &Fixture) -> Value {
    serde_json::from_slice(
        &std::fs::read(fixture.directory.path().join("data/agents/catalog.json")).unwrap(),
    )
    .unwrap()
}

fn binding(fixture: &Fixture, id: &str) -> Value {
    registry(fixture)["agents"][id]["data_key"].clone()
}

#[tokio::test]
async fn malformed_native_bindings_fail_closed_without_rewriting_the_catalog() {
    let fixture = Fixture::new().await;
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Writer"}),
        )
        .await;
    let original = registry(&fixture);
    let path = fixture.directory.path().join("data/agents/catalog.json");
    let mut variants = Vec::new();
    let mut missing = original.clone();
    missing["agents"]["writer"]
        .as_object_mut()
        .unwrap()
        .remove("data_key");
    variants.push(missing);
    let mut reused = original.clone();
    reused["agents"]["writer"]["data_key"] = original["agents"]["default"]["data_key"].clone();
    variants.push(reused);
    let mut unindexed = original.clone();
    unindexed["workspace_keys"] = json!({});
    variants.push(unindexed);
    let mut invalid = original.clone();
    invalid["agents"]["writer"]["data_key"]["id"] = json!("not-a-uuid");
    variants.push(invalid);
    for version in [1, 4] {
        let mut altered = original.clone();
        altered["schema_version"] = json!(version);
        variants.push(altered);
    }
    for variant in variants {
        let bytes = serde_json::to_vec(&variant).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(
            scoped(&fixture, "default", "GET", "/api/agents", Value::Null)
                .await
                .0,
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
    std::fs::write(path, serde_json::to_vec(&original).unwrap()).unwrap();
}

#[tokio::test]
async fn registration_identity_follows_the_retained_root_not_the_public_id() {
    let mut fixture = Fixture::new().await;
    let response = fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Writer"}),
        )
        .await;
    let root = response["workspace_dir"].as_str().unwrap().to_owned();
    assert_eq!(
        response,
        json!({"id":"writer","workspace_dir":root,"enabled":true,"pinned":false})
    );
    let original = binding(&fixture, "writer");
    assert_eq!(original["kind"], "workspace");
    fixture
        .request("DELETE", "/api/agents/writer", Value::Null)
        .await;
    assert_eq!(registry(&fixture)["workspace_keys"][&root], original);
    fixture.reopen().await;
    fixture
        .request("POST", "/api/agents", json!({"id":"writer","name":"Again"}))
        .await;
    assert_eq!(binding(&fixture, "writer"), original);
    fixture
        .request("DELETE", "/api/agents/writer", Value::Null)
        .await;
    let other = fixture.directory.path().join("other-root");
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"New","workspace_dir":other}),
        )
        .await;
    assert_ne!(binding(&fixture, "writer"), original);
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"editor","name":"Recovered","workspace_dir":root}),
        )
        .await;
    assert_eq!(binding(&fixture, "editor"), original);
    let before = registry(&fixture);
    fixture.reopen().await;
    assert_eq!(registry(&fixture), before);
    let copy = fixture
        .request(
            "POST",
            "/api/agents/editor/copy",
            json!({"copy_agent_json":true}),
        )
        .await;
    assert_ne!(binding(&fixture, copy["id"].as_str().unwrap()), original);
}

#[tokio::test]
async fn a_recreated_path_receives_a_new_identity_for_auto_and_custom_roots() {
    let fixture = Fixture::new().await;
    for (id, custom) in [("automatic", false), ("custom", true)] {
        let mut body = json!({"id":id,"name":id});
        if custom {
            body["workspace_dir"] = json!(fixture.directory.path().join("custom-root"));
        }
        let response = fixture.request("POST", "/api/agents", body.clone()).await;
        let original = binding(&fixture, id);
        let root = std::path::Path::new(response["workspace_dir"].as_str().unwrap());
        fixture
            .request("DELETE", &format!("/api/agents/{id}"), Value::Null)
            .await;
        std::fs::rename(root, root.with_extension("retained")).unwrap();
        fixture.request("POST", "/api/agents", body).await;
        assert_ne!(binding(&fixture, id), original);
        assert_eq!(
            registry(&fixture)["workspace_keys"][root.to_str().unwrap()],
            binding(&fixture, id)
        );
    }
}

#[tokio::test]
async fn legacy_binding_is_hydrated_before_deletion_and_is_not_reassigned_by_id() {
    let mut fixture = Fixture::new().await;
    let created = fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Writer"}),
        )
        .await;
    let mut old = registry(&fixture);
    old["schema_version"] = json!(1);
    old.as_object_mut().unwrap().remove("workspace_keys");
    for agent in old["agents"].as_object_mut().unwrap().values_mut() {
        agent.as_object_mut().unwrap().remove("data_key");
        std::fs::remove_file(
            std::path::Path::new(agent["workspace_dir"].as_str().unwrap())
                .join(crate::desktop_agents::identity::MARKER_NAME),
        )
        .unwrap();
    }
    std::fs::write(
        fixture.directory.path().join("data/agents/catalog.json"),
        serde_json::to_vec(&old).unwrap(),
    )
    .unwrap();
    fixture.reopen().await;
    fixture
        .request("DELETE", "/api/agents/writer", Value::Null)
        .await;
    let expected = json!({"kind":"legacy_agent","id":"writer"});
    assert_eq!(registry(&fixture)["schema_version"], 3);
    assert_eq!(
        registry(&fixture)["workspace_keys"][created["workspace_dir"].as_str().unwrap()],
        expected
    );
    fixture.request("POST", "/api/agents", json!({"id":"writer","name":"Fresh","workspace_dir":fixture.directory.path().join("fresh")})).await;
    assert_eq!(binding(&fixture, "writer")["kind"], "workspace");
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"editor","name":"Recovered","workspace_dir":created["workspace_dir"]}),
        )
        .await;
    assert_eq!(binding(&fixture, "editor"), expected);
}

#[cfg(unix)]
#[tokio::test]
async fn canonical_symlink_registration_preserves_the_workspace_identity() {
    let fixture = Fixture::new().await;
    let original = fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Writer"}),
        )
        .await;
    let key = binding(&fixture, "writer");
    fixture
        .request("DELETE", "/api/agents/writer", Value::Null)
        .await;
    let link = fixture.directory.path().join("linked");
    std::os::unix::fs::symlink(original["workspace_dir"].as_str().unwrap(), &link).unwrap();
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"editor","name":"Editor","workspace_dir":link}),
        )
        .await;
    assert_eq!(binding(&fixture, "editor"), key);
}

struct FailingCredentials {
    armed: std::sync::atomic::AtomicBool,
}

impl crate::DesktopCredentialStore for FailingCredentials {
    fn load_api_key(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }

    fn save_api_key(&self, _: Option<&str>) -> anyhow::Result<()> {
        panic!("fixture must not save model credentials")
    }

    fn save_agent_setting_secret(&self, _: &str, value: Option<&str>) -> anyhow::Result<()> {
        assert_eq!(value, None);
        if self.armed.load(std::sync::atomic::Ordering::SeqCst) {
            anyhow::bail!("fixture credential failure");
        }
        Ok(())
    }
}

async fn fail_next_creation(fixture: &mut Fixture, body: Value) -> (StatusCode, Value) {
    fixture.server.inner.shutdown.cancel();
    shutdown(&fixture.server).await;
    let credentials = Arc::new(FailingCredentials {
        armed: std::sync::atomic::AtomicBool::new(false),
    });
    fixture.server = AppServer::new_desktop_with_stores_and_workspace(
        fixture.server.inner.core.clone(),
        fixture.directory.path(),
        String::from("workspace-fault-shutdown"),
        credentials.clone(),
        &fixture.directory.path().join("data"),
        &fixture.directory.path().join("workspace"),
    )
    .unwrap();
    credentials
        .armed
        .store(true, std::sync::atomic::Ordering::SeqCst);
    scoped(fixture, "default", "POST", "/api/agents", body).await
}

#[tokio::test]
async fn failed_recreation_does_not_delete_the_retained_automatic_workspace() {
    let mut fixture = Fixture::new().await;
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Writer"}),
        )
        .await;
    let workspace = fixture.directory.path().join("data/workspaces/writer");
    std::fs::write(workspace.join("KEEP.txt"), "retained user data").unwrap();
    let profile = std::fs::read(workspace.join("agent.json")).unwrap();
    fixture
        .request("DELETE", "/api/agents/writer", Value::Null)
        .await;
    let catalog = std::fs::read(fixture.directory.path().join("data/agents/catalog.json")).unwrap();
    assert_eq!(
        fail_next_creation(&mut fixture, json!({"id":"writer","name":"Replacement"}))
            .await
            .0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        std::fs::read_to_string(workspace.join("KEEP.txt")).unwrap(),
        "retained user data"
    );
    assert_eq!(
        std::fs::read(workspace.join("agent.json")).unwrap(),
        profile
    );
    assert_eq!(
        std::fs::read(fixture.directory.path().join("data/agents/catalog.json")).unwrap(),
        catalog
    );
}

#[tokio::test]
async fn failed_new_creation_cleans_only_its_new_automatic_workspace() {
    let mut fixture = Fixture::new().await;
    assert_eq!(
        fail_next_creation(&mut fixture, json!({"id":"writer","name":"Writer"}))
            .await
            .0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert!(
        !fixture
            .directory
            .path()
            .join("data/workspaces/writer")
            .exists()
    );
    assert!(fixture.directory.path().join("workspace").is_dir());
}

#[tokio::test]
async fn failed_creation_retains_the_custom_workspace() {
    let mut fixture = Fixture::new().await;
    let workspace = fixture.directory.path().join("custom");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(workspace.join("KEEP.txt"), "custom user data").unwrap();
    assert_eq!(
        fail_next_creation(
            &mut fixture,
            json!({"id":"writer","name":"Writer","workspace_dir":workspace})
        )
        .await
        .0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        std::fs::read_to_string(workspace.join("KEEP.txt")).unwrap(),
        "custom user data"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn failed_creation_through_an_existing_auto_symlink_does_not_delete_its_target() {
    let mut fixture = Fixture::new().await;
    let target = fixture.directory.path().join("retained");
    let candidate = fixture.directory.path().join("data/workspaces/writer");
    std::fs::create_dir(&target).unwrap();
    std::fs::create_dir_all(candidate.parent().unwrap()).unwrap();
    std::fs::write(target.join("KEEP.txt"), "linked user data").unwrap();
    std::os::unix::fs::symlink(&target, &candidate).unwrap();
    assert_eq!(
        fail_next_creation(&mut fixture, json!({"id":"writer","name":"Writer"}))
            .await
            .0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        std::fs::read_to_string(target.join("KEEP.txt")).unwrap(),
        "linked user data"
    );
    assert_eq!(std::fs::read_link(candidate).unwrap(), target);
}
