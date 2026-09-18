//! Project destinations must belong to the requested Agent's base Workspace.

use std::fs;
use std::io::{Cursor, Write as _};
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use pretty_assertions::assert_eq;
use qwenpaw_core::{Core, ModelConfig};
use serde_json::{Value, json};
use tower::ServiceExt as _;

use super::{AppServer, DesktopCredentialStore, desktop_agents};

#[path = "desktop_project_browser_tests.rs"]
mod browser;

#[path = "desktop_project_contract_tests.rs"]
mod contract;

struct NoSecrets;

impl DesktopCredentialStore for NoSecrets {
    fn load_api_key(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
    fn save_api_key(&self, value: Option<&str>) -> anyhow::Result<()> {
        assert_eq!(value, None);
        Ok(())
    }
    fn save_agent_setting_secret(&self, _: &str, value: Option<&str>) -> anyhow::Result<()> {
        assert_eq!(value, None);
        Ok(())
    }
}

struct Fixture {
    directory: tempfile::TempDir,
    server: AppServer,
    writer: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.inner.shutdown.cancel();
    }
}

impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        fs::write(root.join("index.html"), "fixture").unwrap();
        fs::create_dir(root.join("default")).unwrap();
        let server = AppServer::new_desktop_with_stores_and_workspace(
            Core::new(ModelConfig {
                api_key: None,
                base_url: "http://127.0.0.1:1".into(),
                default_model: "fixture".into(),
            }),
            &root,
            "isolated-project-token".into(),
            Arc::new(NoSecrets),
            &root.join("data"),
            &root.join("default"),
        )
        .unwrap();
        let fixture = Self {
            directory,
            server,
            writer: root.join("writer"),
        };
        let response = fixture
            .json(
                "POST",
                "/api/agents",
                "default",
                json!({"id":"writer","name":"Writer","workspace_dir":fixture.writer}),
            )
            .await;
        assert_eq!(response.0, StatusCode::CREATED, "{}", response.1);
        fs::write(root.join("default/sentinel.txt"), "default untouched").unwrap();
        fixture
    }

    fn default_root(&self) -> PathBuf {
        self.directory
            .path()
            .canonicalize()
            .unwrap()
            .join("default")
    }

    fn reopen(&mut self) {
        self.server.inner.shutdown.cancel();
        let root = self.directory.path().canonicalize().unwrap();
        self.server = AppServer::new_desktop_with_stores_and_workspace(
            Core::new(ModelConfig {
                api_key: None,
                base_url: "http://127.0.0.1:1".into(),
                default_model: "fixture".into(),
            }),
            &root,
            "isolated-project-token".into(),
            Arc::new(NoSecrets),
            &root.join("data"),
            &root.join("default"),
        )
        .unwrap();
    }

    async fn raw(
        &self,
        method: &str,
        path: &str,
        agent: &str,
        content_type: &str,
        body: Vec<u8>,
    ) -> (StatusCode, Vec<u8>) {
        let response = self
            .server
            .clone()
            .router()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("X-Agent-Id", agent)
                    .header("Content-Type", content_type)
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        (status, bytes.to_vec())
    }

    async fn json(
        &self,
        method: &str,
        path: &str,
        agent: &str,
        body: Value,
    ) -> (StatusCode, Value) {
        let (status, body) = self
            .raw(
                method,
                path,
                agent,
                "application/json",
                body.to_string().into_bytes(),
            )
            .await;
        (status, serde_json::from_slice(&body).unwrap())
    }

    async fn assert_writer_project(&self, name: &str, response: (StatusCode, Value)) {
        let target = self.writer.join("coding_projects").join(name);
        assert_eq!(
            response,
            (StatusCode::OK, json!({"path":target,"name":name}))
        );
        assert_eq!(
            desktop_agents::context_for_agent(&self.server, "writer")
                .await
                .unwrap()
                .project()
                .unwrap(),
            target
        );
        assert_eq!(
            fs::read_to_string(self.default_root().join("sentinel.txt")).unwrap(),
            "default untouched"
        );
    }
}

#[tokio::test]
async fn create_same_named_projects_in_distinct_agent_bases_without_changing_default_selection() {
    let fixture = Fixture::new().await;
    let first = fixture
        .json(
            "POST",
            "/api/workspace/project-directory/create",
            "default",
            json!({"name":"shared"}),
        )
        .await;
    assert_eq!(first.0, StatusCode::OK);
    let default_before = desktop_agents::context_for_agent(&fixture.server, "default")
        .await
        .unwrap();
    let preferred = fixture
        .server
        .inner
        .core
        .read_preferred_workspace()
        .unwrap();
    let created = fixture
        .json(
            "POST",
            "/api/workspace/project-directory/create",
            "writer",
            json!({"name":"shared"}),
        )
        .await;
    fixture.assert_writer_project("shared", created).await;
    assert!(fixture.writer.join("coding_projects/shared/.git").is_dir());
    assert_eq!(
        desktop_agents::context_for_agent(&fixture.server, "default")
            .await
            .unwrap(),
        default_before
    );
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .read_preferred_workspace()
            .unwrap(),
        preferred
    );
}

#[tokio::test]
async fn project_list_does_not_show_other_agents_projects() {
    let fixture = Fixture::new().await;
    fs::create_dir_all(
        fixture
            .default_root()
            .join("coding_projects/private-default"),
    )
    .unwrap();
    fs::create_dir_all(fixture.writer.join("coding_projects/writer-only")).unwrap();
    assert_eq!(
        fixture
            .json(
                "GET",
                "/api/workspace/project-directory/list",
                "writer",
                Value::Null
            )
            .await,
        (
            StatusCode::OK,
            json!([{"path":fixture.writer.join("coding_projects/writer-only"),
        "name":"writer-only", "is_git":false,"is_active":false}])
        )
    );
}

#[tokio::test]
async fn local_import_uses_the_requested_agent_base() {
    let fixture = Fixture::new().await;
    let source = tempfile::Builder::new()
        .prefix(".qwenpaw-project-test-")
        .tempdir_in(dirs::home_dir().unwrap())
        .unwrap();
    fs::write(source.path().join("keep.txt"), "import fixture").unwrap();
    let response = fixture
        .json(
            "POST",
            "/api/workspace/project-directory/import-local",
            "writer",
            json!({"path":source.path(), "name":"imported"}),
        )
        .await;
    fixture.assert_writer_project("imported", response).await;
    assert_eq!(
        fs::read_to_string(fixture.writer.join("coding_projects/imported/keep.txt")).unwrap(),
        "import fixture"
    );
    assert!(!fixture.default_root().join("coding_projects").exists());
}

#[tokio::test]
async fn zip_import_and_staging_use_the_requested_agent_base() {
    let fixture = Fixture::new().await;
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    archive
        .start_file("keep.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    archive.write_all(b"zip fixture").unwrap();
    let bytes = archive.finish().unwrap().into_inner();
    let mut body = b"--fixture-boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"project.zip\"\r\nContent-Type: application/zip\r\n\r\n".to_vec();
    body.extend(bytes);
    body.extend(b"\r\n--fixture-boundary--\r\n");
    let (status, bytes) = fixture
        .raw(
            "POST",
            "/api/workspace/project-directory/upload-zip?name=uploaded",
            "writer",
            "multipart/form-data; boundary=fixture-boundary",
            body,
        )
        .await;
    fixture
        .assert_writer_project(
            "uploaded",
            (status, serde_json::from_slice(&bytes).unwrap()),
        )
        .await;
    assert_eq!(
        fs::read_to_string(fixture.writer.join("coding_projects/uploaded/keep.txt")).unwrap(),
        "zip fixture"
    );
    assert!(!fixture.default_root().join("coding_projects").exists());
}

#[tokio::test]
async fn clone_stream_publishes_the_requested_agent_project() {
    let fixture = Fixture::new().await;
    let source = fixture.directory.path().join("local.git");
    let output = tokio::process::Command::new("git")
        .args(["init", "--bare"])
        .arg(&source)
        .output()
        .await
        .unwrap();
    assert!(output.status.success());
    let (status, bytes) = fixture
        .raw(
            "POST",
            "/api/workspace/project-directory/clone",
            "writer",
            "application/json",
            json!({"url":source,"name":"cloned"})
                .to_string()
                .into_bytes(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let text = String::from_utf8(bytes).unwrap();
    let events = text
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        events.last().unwrap(),
        &json!({"type":"done","path":fixture.writer.join("coding_projects/cloned"),"name":"cloned"})
    );
    assert!(fixture.writer.join("coding_projects/cloned/.git").is_dir());
    assert!(!fixture.default_root().join("coding_projects").exists());
}

async fn replace_writer(fixture: &Fixture) -> PathBuf {
    assert_eq!(
        fixture
            .json("DELETE", "/api/agents/writer", "default", Value::Null)
            .await
            .0,
        StatusCode::OK
    );
    let replacement = fixture
        .directory
        .path()
        .canonicalize()
        .unwrap()
        .join("replacement");
    assert_eq!(
        fixture
            .json(
                "POST",
                "/api/agents",
                "default",
                json!({
                    "id":"writer","name":"New Writer","workspace_dir":replacement
                })
            )
            .await
            .0,
        StatusCode::CREATED
    );
    replacement
}

#[tokio::test]
async fn stale_project_completion_cannot_publish_to_a_reused_agent_id() {
    let fixture = Fixture::new().await;
    let captured = desktop_agents::context_for_agent(&fixture.server, "writer")
        .await
        .unwrap();
    let project = fixture.writer.join("completed-project");
    fs::create_dir(&project).unwrap();
    let replacement = replace_writer(&fixture).await;
    let before = fs::read(replacement.join("agent.json")).unwrap();
    let default_before = desktop_agents::context_for_agent(&fixture.server, "default")
        .await
        .unwrap();
    let error = desktop_agents::set_project_for_context(&fixture.server, &captured, Some(&project))
        .await
        .unwrap_err();
    assert_eq!(error.0, StatusCode::CONFLICT);
    assert_eq!(fs::read(replacement.join("agent.json")).unwrap(), before);
    assert_eq!(
        desktop_agents::context_for_agent(&fixture.server, "default")
            .await
            .unwrap(),
        default_before
    );
    assert!(
        project.is_dir(),
        "An already-created project must not be deleted on publication failure"
    );
}

#[tokio::test]
async fn queued_clone_rejects_reused_agent_before_creating_any_project_directory() {
    let fixture = Fixture::new().await;
    let guard = fixture.server.inner.desktop_project_lock.lock().await;
    let response = fixture.server.clone().router().oneshot(Request::builder()
        .method("POST").uri("/api/workspace/project-directory/clone")
        .header("X-Agent-Id", "writer").header("Content-Type", "application/json")
        .body(Body::from(json!({"url":fixture.directory.path().join("never-cloned.git"),"name":"queued"}).to_string())).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let replacement = replace_writer(&fixture).await;
    let before = fs::read(replacement.join("agent.json")).unwrap();
    drop(guard);
    let bytes = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        to_bytes(response.into_body(), 4096),
    )
    .await
    .unwrap()
    .unwrap();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    let events = text
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        events,
        vec![json!({"type":"error","detail":"Agent Workspace binding has changed"})]
    );
    for root in [
        fixture.default_root(),
        fixture.writer.clone(),
        replacement.clone(),
    ] {
        assert!(!root.join("coding_projects").exists());
    }
    assert_eq!(fs::read(replacement.join("agent.json")).unwrap(), before);
}

#[tokio::test]
async fn project_publication_during_global_restore_keeps_files_and_selection_unchanged() {
    let fixture = Fixture::new().await;
    let context = desktop_agents::context_for_agent(&fixture.server, "default")
        .await
        .unwrap();
    let project = fixture.default_root().join("unpublished");
    fs::create_dir(&project).unwrap();
    let config = fs::read(fixture.default_root().join("agent.json")).ok();
    let catalog_path = fixture.directory.path().join("data/agents/catalog.json");
    let catalog = fs::read(&catalog_path).unwrap();
    let preferred = fixture
        .server
        .inner
        .core
        .read_preferred_workspace()
        .unwrap();
    let _restore = fixture
        .server
        .inner
        .core
        .begin_restore(std::time::Duration::from_secs(1))
        .await
        .unwrap();
    assert!(
        desktop_agents::set_project_for_context(&fixture.server, &context, Some(&project))
            .await
            .is_err()
    );
    assert_eq!(
        fs::read(fixture.default_root().join("agent.json")).ok(),
        config
    );
    assert_eq!(fs::read(catalog_path).unwrap(), catalog);
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .read_preferred_workspace()
            .unwrap(),
        preferred
    );
}

#[cfg(unix)]
#[tokio::test]
async fn project_storage_symlink_cannot_redirect_writes_or_list_another_agent() {
    let fixture = Fixture::new().await;
    let target = fixture.default_root().join("coding_projects");
    fs::create_dir(&target).unwrap();
    std::os::unix::fs::symlink(&target, fixture.writer.join("coding_projects")).unwrap();
    for (method, path, body) in [
        (
            "POST",
            "/api/workspace/project-directory/create",
            json!({"name":"escaped"}),
        ),
        ("GET", "/api/workspace/project-directory/list", Value::Null),
    ] {
        assert_eq!(
            fixture.json(method, path, "writer", body).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(fs::read_dir(target).unwrap().count(), 0);
}
