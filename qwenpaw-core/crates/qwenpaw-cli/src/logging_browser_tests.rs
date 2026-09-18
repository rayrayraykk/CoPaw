//! Original Debug page against the production formatter, file sink, and API.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::routing::post;
use qwenpaw_app_server::{AppServer, DesktopCredentialStore};
use qwenpaw_core::{Core, ModelConfig};
use serde_json::{Value, json};
use tokio::time::timeout;

struct NoCredentials;

impl DesktopCredentialStore for NoCredentials {
    fn load_api_key(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }

    fn save_api_key(&self, _: Option<&str>) -> anyhow::Result<()> {
        panic!("Debug acceptance must not write credentials")
    }
}

fn prepare_logs(
    root: &std::path::Path,
    directory: &std::path::Path,
) -> (AppServer, tracing::Dispatch) {
    let workspace = directory.join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let core = Core::new(ModelConfig {
        api_key: None,
        base_url: "http://127.0.0.1:1".into(),
        default_model: "no-model-needed".into(),
    });
    core.write_ui_language("en").unwrap();
    let server = AppServer::new_desktop_with_stores_and_workspace(
        core,
        &root.join("../console/dist"),
        "isolated-debug-token".into(),
        Arc::new(NoCredentials),
        &directory.join("data"),
        &workspace,
    )
    .unwrap();
    let log = server.open_backend_log().unwrap().unwrap();
    let dispatcher = tracing::Dispatch::new(
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_ansi(false)
            .event_format(super::ConsoleLogFormat)
            .with_writer(move || log.clone())
            .finish(),
    );
    tracing::dispatcher::with_default(&dispatcher, || {
        tracing::debug!("debug fixture");
        tracing::info!("info fixture");
        tracing::warn!("warning fixture");
        tracing::error!("error fixture");
    });
    (server, dispatcher)
}

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_debug_browser_reads_filters_sorts_refreshes_and_copies_real_logs() {
    let directory = tempfile::tempdir().unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (server, dispatcher) = prepare_logs(&root, directory.path());

    // A separate loopback control server generates real events. It never
    // replaces Console requests or exposes fixture routes in the product.
    let control = Router::new().route(
        "/emit/{stage}",
        post(move |Path(stage): Path<String>| {
            let dispatcher = dispatcher.clone();
            async move {
                if !matches!(stage.as_str(), "automatic" | "manual") {
                    return StatusCode::BAD_REQUEST;
                }
                tracing::dispatcher::with_default(&dispatcher, || {
                    tracing::info!("{stage} fixture");
                });
                StatusCode::NO_CONTENT
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let http = tokio::spawn(server.run_http(listener));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_base = format!("http://{}", listener.local_addr().unwrap());
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let control_http = tokio::spawn(async move {
        axum::serve(listener, control)
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
    });
    let output = timeout(
        Duration::from_secs(90),
        tokio::process::Command::new("node")
            .arg(root.join("scripts/console_browser_smoke.mjs"))
            .args([&base, "/debug", "--debug-logs"])
            .env("QWENPAW_DEBUG_FIXTURE_URL", control_base)
            .kill_on_drop(true)
            .output(),
    )
    .await;
    let shutdown = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
        .post(format!("{base}/api/desktop/shutdown"))
        .header("X-QwenPaw-Desktop-Shutdown-Token", "isolated-debug-token")
        .send()
        .await;
    stop.send(()).unwrap();
    timeout(Duration::from_secs(5), control_http)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(shutdown.unwrap().status(), StatusCode::OK);
    timeout(Duration::from_secs(5), http)
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
        report["pages"][0]["debugLogs"],
        json!({
            "file":true, "levels":true, "search":true, "sort":true,
            "autoRefresh":true, "pause":true, "manualRefresh":true,
            "copyPayload":true
        })
    );
    let text = std::fs::read_to_string(directory.path().join("data/qwenpaw.log")).unwrap();
    assert!(text.contains(" WARNING "));
    assert!(text.contains("automatic fixture"));
    assert!(text.contains("manual fixture"));
}
