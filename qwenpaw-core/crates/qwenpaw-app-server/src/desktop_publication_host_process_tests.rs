//! Exercise real Profile HTTP publication and startup, never packaged binaries.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::Request;
use pretty_assertions::assert_eq;
use serde_json::json;
use tower::ServiceExt as _;

use super::*;

pub(super) const ROOT: &str = "QWENPAW_PUBLICATION_HOST_TEST_ROOT";
pub(super) const BOUNDARY: &str = "QWENPAW_PUBLICATION_HOST_TEST_BOUNDARY";
const CHILD_TEST: &str =
    "desktop_agents::publication::process_tests::publication_host_process_child";

pub(super) fn pause(boundary: &str) {
    if std::env::var(BOUNDARY).ok().as_deref() != Some(boundary) {
        return;
    }
    let root = PathBuf::from(std::env::var_os(ROOT).unwrap());
    fs::write(root.join("ready.pending"), boundary).unwrap();
    fs::rename(root.join("ready.pending"), root.join("ready")).unwrap();
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

struct NoSecrets;

impl DesktopCredentialStore for NoSecrets {
    fn load_api_key(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
    fn save_api_key(&self, _: Option<&str>) -> anyhow::Result<()> {
        panic!("this process matrix must not use credentials")
    }
}

fn server(root: &Path) -> AppServer {
    server_with_credentials(root, Arc::new(NoSecrets))
}

pub(super) fn server_with_credentials(
    root: &Path,
    credentials: Arc<dyn DesktopCredentialStore>,
) -> AppServer {
    AppServer::new_desktop_with_stores_and_workspace(
        Core::persistent(
            qwenpaw_core::ModelConfig {
                api_key: None,
                base_url: "http://127.0.0.1:1".into(),
                default_model: "fixture".into(),
            },
            &root.join("core.sqlite"),
        )
        .unwrap(),
        root,
        "fixture-shutdown".into(),
        credentials,
        &root.join("data"),
        &root.join("workspace"),
    )
    .unwrap()
}

pub(super) async fn profile(server: &AppServer, method: &str, body: Value) -> Value {
    let response = server
        .clone()
        .router()
        .oneshot(
            Request::builder()
                .method(method)
                .uri("/api/agents/default")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(status.is_success(), "{status}: {value}");
    value
}

#[tokio::test]
async fn publication_host_process_child() {
    let Some(root) = std::env::var_os(ROOT) else {
        return;
    };
    let server = server(&PathBuf::from(root));
    profile(
        &server,
        "PUT",
        json!({"name":"Durable profile", "channels":{"console":{"bot_prefix":"durable"}}}),
    )
    .await;
    panic!("child did not pause at its requested boundary");
}

pub(super) struct IsolatedChild(pub(super) Child);

impl Drop for IsolatedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[path = "desktop_publication_credential_process_tests.rs"]
mod credentials;

#[tokio::test]
async fn publication_host_killed_profile_recovers_through_actual_startup_at_nine_boundaries() {
    for (boundary, committed) in [
        ("staging", false),
        ("publishing", false),
        ("secret-published", false),
        ("files-published", false),
        ("committed", true),
        ("cleaning", true),
        ("files-cleaned", true),
        ("secrets-cleaned", true),
        ("finished", true),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        fs::write(
            root.join("index.html"),
            "<!doctype html><title>fixture</title>",
        )
        .unwrap();
        fs::create_dir(root.join("workspace")).unwrap();
        let initial = server(&root);
        let before = profile(&initial, "GET", Value::Null).await;
        let old_channels = initial.inner.core.read_channel_config_data().unwrap();
        let old_catalog = fs::read(root.join("data/agents/catalog.json")).unwrap();
        let old_config = fs::read(root.join("workspace/agent.json")).ok();
        drop(initial);
        let mut child = IsolatedChild(
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", CHILD_TEST, "--nocapture"])
                .env(ROOT, &root)
                .env(BOUNDARY, boundary)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(20);
        while !root.join("ready").exists() {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "child exited before {boundary}"
            );
            assert!(Instant::now() < deadline, "child did not reach {boundary}");
            std::thread::sleep(Duration::from_millis(10));
        }
        child.0.kill().unwrap();
        assert!(!child.0.wait().unwrap().success());
        let recovered = server(&root);
        let mut expected = before;
        if committed {
            expected["name"] = json!("Durable profile");
            expected["channels"]["console"]["bot_prefix"] = json!("durable");
        } else {
            assert_eq!(
                fs::read(root.join("data/agents/catalog.json")).unwrap(),
                old_catalog
            );
            assert_eq!(fs::read(root.join("workspace/agent.json")).ok(), old_config);
            assert_eq!(
                recovered.inner.core.read_channel_config_data().unwrap(),
                old_channels
            );
        }
        assert_eq!(
            profile(&recovered, "GET", Value::Null).await,
            expected,
            "{boundary}"
        );
        assert_eq!(recovered.inner.core.read_agent_publication().unwrap(), None);
        let config = fs::read(root.join("workspace/agent.json")).ok();
        let catalog = fs::read(root.join("data/agents/catalog.json")).unwrap();
        let channels = recovered.inner.core.read_channel_config_data().unwrap();
        drop(recovered);
        let reopened = server(&root);
        assert_eq!(profile(&reopened, "GET", Value::Null).await, expected);
        assert_eq!(fs::read(root.join("workspace/agent.json")).ok(), config);
        assert_eq!(
            fs::read(root.join("data/agents/catalog.json")).unwrap(),
            catalog
        );
        assert_eq!(
            reopened.inner.core.read_channel_config_data().unwrap(),
            channels
        );
    }
}
