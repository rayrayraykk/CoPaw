use std::fs;
use std::sync::Arc;

use super::{AppServer, DesktopCredentialStore};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use pretty_assertions::assert_eq;
use qwenpaw_core::{Core, ModelConfig};
use serde_json::{Value, json};
use tower::ServiceExt as _;

#[path = "desktop_frontend_plugin_tests.rs"]
mod frontend;

struct NoCredentials;

impl DesktopCredentialStore for NoCredentials {
    fn load_api_key(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }

    fn save_api_key(&self, _: Option<&str>) -> anyhow::Result<()> {
        panic!("PawApps must not write credentials")
    }
}

struct Fixture {
    directory: tempfile::TempDir,
    server: AppServer,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.inner.shutdown.cancel();
    }
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("index.html"), "fixture").unwrap();
        let server = Self::open(directory.path());
        Self { directory, server }
    }

    fn open(root: &std::path::Path) -> AppServer {
        fs::create_dir_all(root.join("workspace")).unwrap();
        AppServer::new_desktop_with_stores_and_workspace(
            Core::new(ModelConfig {
                api_key: None,
                base_url: "http://127.0.0.1:1".into(),
                default_model: "no-model-needed".into(),
            }),
            root,
            "isolated-pawapps-token".into(),
            Arc::new(NoCredentials),
            &root.join("data"),
            &root.join("workspace"),
        )
        .unwrap()
    }

    fn plugins(&self) -> std::path::PathBuf {
        self.directory.path().join("data/plugins")
    }

    fn install(&self, id: &str, manifest: &Value) {
        let directory = self.plugins().join(id);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("plugin.json"), manifest.to_string()).unwrap();
    }

    async fn request(&self, method: &str, path: &str) -> (StatusCode, Value) {
        let response = self
            .server
            .clone()
            .router()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }
}

fn manifest() -> Value {
    json!({"id":"demo", "name":"Demo", "version":"1.2.3",
        "description":"Fixture app", "description_i18n":{"zh-CN":"演示"},
        "author":"Fixture", "meta":{"pawapp":{"category":"productivity",
        "icon":"demo", "icon_url":"/icon.svg", "entry_page":"/apps/demo",
        "launch_scope":"page"}, "settings":[{"key":"theme", "type":"string"}]}})
}

fn info() -> Value {
    json!({"id":"demo", "name":"Demo", "version":"1.2.3",
        "description":"Fixture app", "description_i18n":{"zh-CN":"演示"},
        "author":"Fixture", "category":"productivity", "icon":"demo",
        "icon_url":"/icon.svg", "entry_page":"/apps/demo", "launch_scope":"page",
        "status":"installed", "settings":[{"key":"theme", "type":"string"}]})
}

#[tokio::test]
async fn pawapps_discovers_refreshes_and_reopens_original_manifest_contract() {
    let mut fixture = Fixture::new();
    assert_eq!(
        fixture.request("GET", "/api/pawapps").await,
        (StatusCode::OK, json!({"apps":[],"total":0}))
    );
    assert!(
        !fixture.plugins().exists(),
        "Reads must not create plugins directories"
    );
    fixture.install("demo", &manifest());
    fixture.install("ordinary", &json!({"id":"ordinary", "meta":{}}));
    fixture.install("empty", &json!({"meta":{"pawapp":{}}}));
    fixture.install("broken", &Value::Null);
    fs::write(fixture.plugins().join("broken/plugin.json"), "{invalid").unwrap();
    for reopen in [false, true] {
        if reopen {
            fixture.server = Fixture::open(fixture.directory.path());
        }
        assert_eq!(
            fixture.request("GET", "/api/pawapps").await,
            (StatusCode::OK, json!({"apps":[info()],"total":1}))
        );
        assert_eq!(
            fixture.request("GET", "/api/pawapps/demo").await,
            (StatusCode::OK, info())
        );
        assert_eq!(
            fixture.request("GET", "/api/pawapps/demo/settings").await,
            (
                StatusCode::OK,
                json!({"app_id":"demo", "settings":info()["settings"]})
            )
        );
    }
}

#[tokio::test]
async fn pawapps_preserves_defaults_and_manifest_id_independent_of_directory() {
    let fixture = Fixture::new();
    fixture.install(
        "folder",
        &json!({"id":"manifest-id", "meta":{"pawapp":{"icon":"x"}}}),
    );
    assert_eq!(
        fixture.request("GET", "/api/pawapps/manifest-id").await,
        (
            StatusCode::OK,
            json!({"id":"manifest-id", "name":"folder", "version":"0.0.0",
            "description":"", "description_i18n":{}, "author":"", "category":"",
            "icon":"x", "icon_url":"", "entry_page":"", "launch_scope":"page",
            "status":"installed", "settings":[]})
        )
    );
}

#[tokio::test]
async fn pawapps_uninstall_changes_disk_and_listing_but_not_neighbors() {
    let fixture = Fixture::new();
    fixture.install("demo", &manifest());
    fixture.install("neighbor", &json!({"id":"neighbor"}));
    assert_eq!(
        fixture.request("DELETE", "/api/pawapps/demo").await,
        (
            StatusCode::OK,
            json!({"id":"demo", "message":"PawApp 'demo' uninstalled."})
        )
    );
    assert!(!fixture.plugins().join("demo").exists());
    assert_eq!(
        fs::read_to_string(fixture.plugins().join("neighbor/plugin.json")).unwrap(),
        json!({"id":"neighbor"}).to_string()
    );
    assert_eq!(
        fixture.request("GET", "/api/pawapps").await,
        (StatusCode::OK, json!({"apps":[], "total":0}))
    );
    for (method, path) in [
        ("DELETE", "/api/pawapps/demo"),
        ("GET", "/api/pawapps/demo"),
        ("GET", "/api/pawapps/demo/settings"),
        ("GET", "/api/pawapps/demo/static/index.html"),
    ] {
        assert_eq!(
            fixture.request(method, path).await,
            (
                StatusCode::NOT_FOUND,
                json!({"detail":"PawApp 'demo' not found"})
            )
        );
    }
}

#[tokio::test]
async fn pawapps_static_supports_binary_head_range_and_missing_files() {
    let fixture = Fixture::new();
    fixture.install("demo", &manifest());
    let content = b"\x00\x01\x02\x03\xff\xfe";
    fs::write(fixture.plugins().join("demo/asset.bin"), content).unwrap();
    for (method, range, status, expected) in [
        ("GET", None, StatusCode::OK, content.as_slice()),
        ("HEAD", None, StatusCode::OK, &b""[..]),
        (
            "GET",
            Some("bytes=2-4"),
            StatusCode::PARTIAL_CONTENT,
            &content[2..5],
        ),
    ] {
        let mut request = Request::builder()
            .method(method)
            .uri("/api/pawapps/demo/static/asset.bin");
        if let Some(range) = range {
            request = request.header("range", range);
        }
        let response = fixture
            .server
            .clone()
            .router()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        assert_eq!(
            response.headers()["content-type"],
            "application/octet-stream"
        );
        if range.is_some() {
            assert_eq!(response.headers()["content-range"], "bytes 2-4/6");
        }
        assert_eq!(
            axum::body::to_bytes(response.into_body(), 1024)
                .await
                .unwrap()
                .as_ref(),
            expected
        );
    }
    assert_eq!(
        fixture
            .request("GET", "/api/pawapps/demo/static/missing.txt")
            .await,
        (
            StatusCode::NOT_FOUND,
            json!({"detail":"File not found: missing.txt"})
        )
    );
}

#[tokio::test]
async fn pawapps_rejects_portable_path_escapes_without_deleting_or_reading_neighbors() {
    let fixture = Fixture::new();
    fixture.install("demo", &manifest());
    let sentinel = fixture.plugins().join("sentinel.txt");
    fs::write(&sentinel, "private fixture").unwrap();
    for id in ["%2e%2e", "bad%5cid", "bad%2fid", "C%3A"] {
        for method in ["GET", "DELETE"] {
            let path = if method == "GET" {
                format!("/api/pawapps/{id}/static/sentinel.txt")
            } else {
                format!("/api/pawapps/{id}")
            };
            assert_eq!(
                fixture.request(method, &path).await,
                (StatusCode::BAD_REQUEST, json!({"detail":"Invalid app id"}))
            );
        }
    }
    for path in [
        "%2e%2e/sentinel.txt",
        "%2e%2e%5csentinel.txt",
        "C%3A/sentinel.txt",
    ] {
        assert_eq!(
            fixture
                .request("GET", &format!("/api/pawapps/demo/static/{path}"))
                .await,
            (StatusCode::FORBIDDEN, json!({"detail":"Access denied"}))
        );
    }
    assert_eq!(fs::read_to_string(sentinel).unwrap(), "private fixture");
    assert!(fixture.plugins().join("demo/plugin.json").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn pawapps_rejects_symlink_app_root_and_asset_escapes() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    fixture.install("demo", &manifest());
    let outside = fixture.directory.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("secret.txt"), "private fixture").unwrap();
    symlink(&outside, fixture.plugins().join("escape")).unwrap();
    symlink(
        outside.join("secret.txt"),
        fixture.plugins().join("demo/link.txt"),
    )
    .unwrap();
    assert_eq!(
        fixture
            .request("GET", "/api/pawapps/demo/static/link.txt")
            .await,
        (StatusCode::FORBIDDEN, json!({"detail":"Access denied"}))
    );
    for (method, path) in [
        ("GET", "/api/pawapps/escape/static/secret.txt"),
        ("DELETE", "/api/pawapps/escape"),
    ] {
        assert_eq!(
            fixture.request(method, path).await,
            (
                StatusCode::BAD_REQUEST,
                json!({"detail":"Invalid app path"})
            )
        );
    }
    assert_eq!(
        fs::read_to_string(outside.join("secret.txt")).unwrap(),
        "private fixture"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn pawapps_never_scans_an_external_plugins_root_or_manifest() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let external = tempfile::tempdir().unwrap();
    fs::write(external.path().join("plugin.json"), manifest().to_string()).unwrap();
    fs::create_dir_all(fixture.plugins().join("demo")).unwrap();
    symlink(
        external.path().join("plugin.json"),
        fixture.plugins().join("demo/plugin.json"),
    )
    .unwrap();
    assert_eq!(
        fixture.request("GET", "/api/pawapps").await,
        (StatusCode::OK, json!({"apps":[],"total":0}))
    );
    let second = Fixture::new();
    symlink(external.path(), second.plugins()).unwrap();
    for (method, path) in [("GET", "/api/pawapps"), ("DELETE", "/api/pawapps/demo")] {
        assert_eq!(
            second.request(method, path).await,
            (
                StatusCode::BAD_REQUEST,
                json!({"detail":"Invalid plugins path"})
            )
        );
    }
    assert_eq!(
        fs::read_to_string(external.path().join("plugin.json")).unwrap(),
        manifest().to_string()
    );
}

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_pawapps_browser_lists_filters_cancels_uninstalls_and_reloads() {
    let mut fixture = Fixture::new();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    Arc::get_mut(&mut fixture.server.inner)
        .unwrap()
        .console_static_dir = Some(root.join("../console/dist").canonicalize().unwrap());
    fixture.server.inner.core.write_ui_language("en").unwrap();
    for (id, name, category) in [
        ("notes", "Fixture Notes", "notes"),
        ("tasks", "Fixture Tasks", "tasks"),
    ] {
        let mut app = manifest();
        app["id"] = json!(id);
        app["name"] = json!(name);
        app["meta"]["pawapp"]["category"] = json!(category);
        app["meta"]["pawapp"]["icon_url"] = json!(format!("/api/pawapps/{id}/static/icon.svg"));
        fixture.install(id, &app);
        fs::write(fixture.plugins().join(id).join("icon.svg"),
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="blue"/></svg>"#).unwrap();
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let http = tokio::spawn(fixture.server.clone().run_http(listener));
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(90),
        tokio::process::Command::new("node")
            .arg(root.join("scripts/console_browser_smoke.mjs"))
            .args([&base, "/market", "--pawapps-crud"])
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
        report["pages"][0]["pawappsCrud"],
        json!({"list":true,"icon":true,"search":true,
        "category":true,"refresh":true,"cancel":true,"uninstall":true,"reload":true})
    );
    assert!(!fixture.plugins().join("notes").exists());
    assert!(fixture.plugins().join("tasks/plugin.json").is_file());
}

#[tokio::test]
#[ignore = "requires the qwenpaw conda environment; run explicitly for original Python parity"]
async fn original_python_pawapps_http_contract_matches_rust_directory_lifecycle() {
    let fixture = Fixture::new();
    fixture.install("demo", &manifest());
    let mut actual = Vec::new();
    for (method, path) in [
        ("GET", "/api/pawapps"),
        ("GET", "/api/pawapps/demo"),
        ("GET", "/api/pawapps/demo/settings"),
        ("DELETE", "/api/pawapps/demo"),
        ("GET", "/api/pawapps"),
        ("GET", "/api/pawapps/demo"),
    ] {
        let (status, body) = fixture.request(method, path).await;
        actual.push(json!([status.as_u16(), body]));
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::process::Command::new("python")
            .arg(root.join("scripts/pawapps_reference.py"))
            .arg(manifest().to_string())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json!(actual), expected);
}
