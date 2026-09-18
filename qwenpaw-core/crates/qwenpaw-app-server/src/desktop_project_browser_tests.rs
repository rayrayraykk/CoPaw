use std::fs;
use std::sync::Arc;

use axum::http::StatusCode;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

use super::{Fixture, desktop_agents};

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_project_modal_keeps_create_clone_zip_recent_and_reset_agent_scoped() {
    let mut fixture = Fixture::new().await;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    Arc::get_mut(&mut fixture.server.inner)
        .unwrap()
        .console_static_dir = Some(root.join("../console/dist").canonicalize().unwrap());
    fixture.server.inner.core.write_ui_language("en").unwrap();
    let default_created = fixture
        .json(
            "POST",
            "/api/workspace/project-directory/create",
            "default",
            json!({"name":"shared"}),
        )
        .await;
    assert_eq!(default_created.0, StatusCode::OK);
    fs::create_dir_all(fixture.writer.join("coding_projects/writer-only")).unwrap();
    let before = desktop_agents::context_for_agent(&fixture.server, "default")
        .await
        .unwrap();
    let preferred = fixture
        .server
        .inner
        .core
        .read_preferred_workspace()
        .unwrap();
    let source = fixture.directory.path().join("source.git");
    let status = tokio::process::Command::new("git")
        .args(["init", "--bare"])
        .arg(&source)
        .output()
        .await
        .unwrap()
        .status;
    assert!(status.success());
    let upload = fixture.directory.path().join("browser-upload");
    fs::create_dir(&upload).unwrap();
    fs::write(upload.join("keep.txt"), "browser ZIP fixture").unwrap();
    fs::create_dir(upload.join("node_modules")).unwrap();
    fs::write(upload.join("node_modules/skip.txt"), "skip fixture").unwrap();
    let browse = fixture
        .directory
        .path()
        .canonicalize()
        .unwrap()
        .join("browse-parent");
    fs::create_dir_all(browse.join("visible")).unwrap();
    fs::create_dir(browse.join(".hidden-fixture")).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let http = tokio::spawn(fixture.server.clone().run_http(listener));
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        tokio::process::Command::new("node")
            .arg(root.join("scripts/console_browser_smoke.mjs"))
            .args([&base, "/agent-config", "--project-ownership"])
            .env("QWENPAW_PROJECT_FIXTURE_SOURCE", &source)
            .env("QWENPAW_PROJECT_FIXTURE_UPLOAD", &upload)
            .env("QWENPAW_PROJECT_FIXTURE_BROWSE", &browse)
            .kill_on_drop(true)
            .output(),
    )
    .await;
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(5), http)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let output = output.unwrap().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["ok"], true);
    assert_eq!(
        report["pages"][0]["projectOwnership"],
        json!({
            "agentSwitch":true,"create":true,"recent":true,"reset":true,
            "clone":true,"zip":true,"reload":true,"defaultUnchanged":true,
            "openDirectory":true
        })
    );
    assert_eq!(
        desktop_agents::context_for_agent(&fixture.server, "default")
            .await
            .unwrap(),
        before
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
    assert_project_files(&fixture);
}

fn assert_project_files(fixture: &Fixture) {
    let upload = fixture.directory.path().join("browser-upload");
    let projects = fixture.writer.join("coding_projects");
    assert!(projects.join("shared/.git").is_dir());
    assert!(projects.join("browser-cloned/.git").is_dir());
    // Keep the original ZIP member layout, including its top-level folder.
    assert_eq!(
        fs::read_to_string(projects.join("browser-upload/browser-upload/keep.txt")).unwrap(),
        "browser ZIP fixture"
    );
    assert!(
        !projects
            .join("browser-upload/browser-upload/node_modules")
            .exists()
    );
    assert_eq!(
        fs::read_to_string(upload.join("keep.txt")).unwrap(),
        "browser ZIP fixture"
    );
    assert!(
        !fixture
            .default_root()
            .join("coding_projects/browser-cloned")
            .exists()
    );
    assert!(
        !fixture
            .default_root()
            .join("coding_projects/browser-upload")
            .exists()
    );
}
