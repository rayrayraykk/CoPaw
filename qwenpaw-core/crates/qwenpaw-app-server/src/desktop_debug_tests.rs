use std::fmt::Write as _;
use std::fs;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use pretty_assertions::assert_eq;
use qwenpaw_core::{Core, ModelConfig};
use serde_json::{Value, json};
use tower::ServiceExt as _;

use super::{AppServer, DesktopWorkspace};

struct Fixture {
    directory: tempfile::TempDir,
    server: AppServer,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let mut server = AppServer::new(Core::new(ModelConfig {
            api_key: None,
            base_url: "http://127.0.0.1:1".into(),
            default_model: "no-model-needed".into(),
        }));
        Arc::get_mut(&mut server.inner).unwrap().desktop_workspace = Some(DesktopWorkspace {
            data_dir: root.clone(),
            initial: root.clone(),
            selected: tokio::sync::RwLock::new(root),
        });
        Self { directory, server }
    }

    fn path(&self) -> std::path::PathBuf {
        self.directory
            .path()
            .canonicalize()
            .unwrap()
            .join("qwenpaw.log")
    }

    async fn request(&self, query: &str) -> (StatusCode, Value) {
        let response = super::desktop_debug::router()
            .with_state(self.server.clone())
            .oneshot(
                Request::builder()
                    .uri(format!("/api/console/debug/backend-logs{query}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }
}

#[tokio::test]
async fn debug_log_missing_file_reports_real_path_and_requested_lines_without_creating_it() {
    let fixture = Fixture::new();
    for (query, lines) in [("", 200), ("?lines=20", 20), ("?lines=1000", 1000)] {
        assert_eq!(
            fixture.request(query).await,
            (
                StatusCode::OK,
                json!({"path":fixture.path(), "exists":false,"lines":lines,
                "updated_at":null,"size":0,"content":""})
            )
        );
    }
    assert!(!fixture.path().exists());
}

#[tokio::test]
async fn debug_log_reads_actual_tail_and_descriptor_metadata() {
    let fixture = Fixture::new();
    let mut text = String::new();
    for index in 0..25 {
        writeln!(text, "line {index}").unwrap();
    }
    fs::write(fixture.path(), &text).unwrap();
    // Use an exactly representable timestamp for the complete wire assertion.
    fs::File::options()
        .write(true)
        .open(fixture.path())
        .unwrap()
        .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000))
        .unwrap();
    let metadata = fs::metadata(fixture.path()).unwrap();
    let expected = (5..25)
        .map(|index| format!("line {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        fixture.request("?lines=20").await,
        (
            StatusCode::OK,
            json!({"path":fixture.path(),"exists":true,"lines":20,
            "updated_at":metadata.modified().unwrap().duration_since(UNIX_EPOCH).unwrap().as_secs_f64(),
            "size":metadata.len(),"content":expected})
        )
    );
    fs::write(
        fixture.path(),
        "replacement\r\nsecond\rthird\u{000b}fourth\n",
    )
    .unwrap();
    assert_eq!(
        fixture.request("").await.1["content"],
        "replacement\nsecond\nthird\nfourth"
    );
}

#[tokio::test]
async fn debug_log_bounds_bytes_and_replaces_invalid_utf8_like_the_original() {
    let fixture = Fixture::new();
    let mut bytes = vec![b'x'; 600 * 1024];
    bytes.extend_from_slice(b"\ninvalid \xff\nlast\n");
    fs::write(fixture.path(), &bytes).unwrap();
    let (status, value) = fixture.request("?lines=1000").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["size"], bytes.len());
    let tail = String::from_utf8_lossy(&bytes[bytes.len() - 512 * 1024..]);
    assert_eq!(value["content"], tail.trim_end_matches('\n'));
}

#[tokio::test]
async fn debug_log_validates_line_count_instead_of_returning_fake_success() {
    let fixture = Fixture::new();
    for query in [
        "?lines=19",
        "?lines=1001",
        "?lines=-1",
        "?lines=invalid",
        "?lines=",
    ] {
        let (status, value) = fixture.request(query).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{query}: {value}");
        assert_eq!(value["detail"][0]["loc"], json!(["query", "lines"]));
    }
}

#[cfg(unix)]
#[tokio::test]
async fn debug_log_rejects_symlinks_without_reading_their_targets() {
    let fixture = Fixture::new();
    let outside = tempfile::tempdir().unwrap();
    let secret = outside.path().join("private.txt");
    fs::write(&secret, "private fixture").unwrap();
    std::os::unix::fs::symlink(&secret, fixture.path()).unwrap();
    let (status, value) = fixture.request("").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(!value.to_string().contains("private fixture"));
    assert_eq!(fs::read_to_string(secret).unwrap(), "private fixture");
}

#[tokio::test]
#[ignore = "requires the qwenpaw conda environment; run explicitly for original Python parity"]
async fn original_python_debug_log_http_contract_matches_rust() {
    let fixture = Fixture::new();
    fs::write(fixture.path(), "first\r\nsecond\rthird\u{0085}fourth\n").unwrap();
    fs::File::options()
        .write(true)
        .open(fixture.path())
        .unwrap()
        .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000))
        .unwrap();
    let queries = [
        "",
        "?lines=20",
        "?lines=1000",
        "?lines=19",
        "?lines=1001",
        "?lines=-1",
        "?lines=invalid",
        "?lines=",
        "?lines=20.0",
        "?lines=20.1",
        "?lines=2e1",
        "?lines=1_000",
        "?lines=99999999999999999999999",
        "?lines=19&lines=20",
        "?lines=20&lines=19",
        "?lines=20.",
        "?lines=.0",
        "?lines=20.00",
        "?lines=2_0.0",
        "?lines=2_0.0_0",
        "?lines=_20",
        "?lines=20_",
        "?lines=2__0",
        "?lines=%2B20",
        "?lines=-0",
        "?lines=%2020%20",
        "?lines=00020",
        "?lines=-99999999999999999999999",
    ];
    let mut actual = Vec::new();
    for query in queries {
        let (status, body) = fixture.request(query).await;
        actual.push(json!([status.as_u16(), body]));
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::process::Command::new("python")
            .arg(root.join("scripts/debug_logs_reference.py"))
            .arg(fixture.path())
            .arg(json!(queries).to_string())
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
