use super::*;
use pretty_assertions::assert_eq;

async fn catalog(fixture: &Fixture) -> (StatusCode, Value) {
    let response = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .get(format!("{}/api/plugins/catalog", fixture.base))
        .header("authorization", "Bearer fixture-do-not-forward")
        .header("cookie", "fixture=do-not-forward")
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    let value = serde_json::from_str(&body)
        .unwrap_or_else(|error| panic!("catalog returned {status}: {body:?}: {error}"));
    (status, value)
}

const MAIN: &str = "/download/metadata/index.json";
const INDEX: &str = "/download/plugins/index.json";

#[tokio::test]
async fn official_catalog_preserves_falsey_files_as_an_empty_catalog() {
    let fixture = Fixture::new().await;
    for files in [
        Value::Null,
        json!(false),
        json!(0),
        json!(0.0),
        json!(""),
        json!([]),
        json!({}),
    ] {
        configure(&fixture, json!({"files":files,"updated_at":"fixture"}));
        assert_eq!(
            catalog(&fixture).await,
            (
                StatusCode::OK,
                json!({"updated_at":"fixture","plugins":[],"error":null})
            )
        );
    }
    fixture.shutdown().await;
}

#[tokio::test]
async fn official_catalog_request_timeout_preserves_the_original_error_shape() {
    let fixture = Fixture::new().await;
    configure(&fixture, json!({}));
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    fixture.remote.lock().unwrap().gate = Some((MAIN.into(), started.clone(), release.clone()));
    let (response, ready) = tokio::join!(
        tokio::time::timeout(Duration::from_secs(35), catalog(&fixture)),
        tokio::time::timeout(Duration::from_secs(5), started.notified())
    );
    ready.unwrap();
    assert_eq!(
        response.unwrap(),
        (
            StatusCode::OK,
            json!({"updated_at":null,"plugins":[],"error":"Failed to fetch plugin catalog index"})
        )
    );
    release.notify_one();
    fixture.shutdown().await;
}

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; original official catalog acceptance"]
async fn official_catalog_original_frontend_preserves_browsing_and_error_recovery() {
    let fixture = Fixture::with_browser(true).await;
    install(
        &fixture,
        "upgrade",
        &json!({"id":"upgrade","version":"1.0rc1"}),
    );
    install(
        &fixture,
        "installed",
        &json!({"id":"installed","version":"1.0"}),
    );
    configure(
        &fixture,
        json!({"files":{
            "upgrade":{"plugin_id":"upgrade","id":"upgrade-1.0","name":"Upgrade Demo","version":"1.0","platform":"tool","url":"/upgrade.zip"},
            "installed":{"plugin_id":"installed","id":"installed-1.0","name":"Installed Demo","version":"1.0","platform":"tool","url":"/installed.zip"},
            "new":{"id":"new-1.0","name":"New Demo","version":"1.0","platform":"bundle","url":"/new.zip"}
        }}),
    );
    let remote = fixture.remote.clone();
    let control = Router::new().route("/{action}", post(move |axum::extract::Path(action): axum::extract::Path<String>| {
        let remote = remote.clone();
        async move {
            let response = match action.as_str() {
                "fail" => (StatusCode::SERVICE_UNAVAILABLE, Vec::new()),
                "recover" => (StatusCode::OK, serde_json::to_vec(&json!({"products":{"plugins":{"index_url":"/plugins/index.json"}}})).unwrap()),
                _ => return StatusCode::NOT_FOUND,
            };
            remote.lock().unwrap().responses.insert(MAIN.into(), response);
            StatusCode::NO_CONTENT
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_base = format!("http://{}", listener.local_addr().unwrap());
    let control_task = tokio::spawn(async move { axum::serve(listener, control).await.unwrap() });
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/console_official_plugin_smoke.mjs");
    let output = tokio::time::timeout(
        Duration::from_secs(100),
        tokio::process::Command::new("node")
            .arg(script)
            .arg(&fixture.base)
            .arg(control_base)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    control_task.abort();
    assert!(control_task.await.unwrap_err().is_cancelled());
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    for key in [
        "ok", "statuses", "search", "filters", "views", "refresh", "recovery", "reload",
    ] {
        assert_eq!(report[key], true);
    }
    {
        let remote = fixture.remote.lock().unwrap();
        assert!(
            remote
                .requests
                .iter()
                .filter(|request| request.path == MAIN)
                .count()
                >= 5
        );
        assert!(
            remote
                .requests
                .iter()
                .all(|request| request.path == MAIN || request.path == INDEX)
        );
    }
    fixture.shutdown().await;
}

fn configure(fixture: &Fixture, index: Value) {
    fixture.respond(
        MAIN,
        json!({"products":{"plugins":{"index_url":"/plugins/index.json"}}}),
    );
    fixture.respond(INDEX, index);
}

fn install(fixture: &Fixture, name: &str, manifest: &Value) {
    let directory = fixture.directory.path().join("data/plugins").join(name);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("plugin.json"),
        serde_json::to_vec(manifest).unwrap(),
    )
    .unwrap();
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write as _;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

#[tokio::test]
async fn official_catalog_returns_full_metadata_and_raw_installed_versions() {
    let fixture = Fixture::new().await;
    install(
        &fixture,
        "backend.disabled",
        &json!({"id":"demo","version":"1.0rc1"}),
    );
    configure(
        &fixture,
        json!({"updated_at":"2026-09-15","files":{
            "entry":{"id":"demo-1.0-aBcD1234","version":"1.0","name":{"en-US":"Demo","zh-CN":"示例"},
            "description":{"en":"English","zh":"中文","fr":"","de":false},
            "author":"Author","platform":"tool","size":123,"sha256":"fixture-hash",
            "url":"/plugins/demo.zip","min_version":"v2.2.0","max_version":"2.0.0"}
        }}),
    );
    let base = &fixture.server.inner.desktop_market.download;
    assert_eq!(
        catalog(&fixture).await,
        (
            StatusCode::OK,
            json!({
                "updated_at":"2026-09-15","error":null,"plugins":[{
                    "id":"demo-1.0-aBcD1234","plugin_id":"demo","name":"Demo","description":"English",
                    "description_i18n":{"en":"English","zh":"中文"},"author":"Author","kind":"tool",
                    "version":"1.0","size":"123","sha256":"fixture-hash",
                    "install_url":format!("{base}/plugins/demo.zip"),"installed":true,
                    "installed_version":"1.0rc1","upgrade_available":true
                }]
            })
        )
    );
    {
        let remote = fixture.remote.lock().unwrap();
        assert_eq!(
            remote
                .requests
                .iter()
                .map(|request| request.path.as_str())
                .collect::<Vec<_>>(),
            [MAIN, INDEX]
        );
        for request in &remote.requests {
            assert_eq!(request.headers.get("authorization"), None);
            assert_eq!(request.headers.get("cookie"), None);
            assert_eq!(request.headers["accept"], "application/json");
            assert_eq!(request.headers["accept-encoding"], "gzip");
        }
    }
    fixture.shutdown().await;
}

#[tokio::test]
async fn official_catalog_preserves_gzip_and_stable_order_with_duplicate_keys() {
    let fixture = Fixture::new().await;
    configure(&fixture, json!({}));
    let bytes = br#"{"files":{"z":{"id":"first","url":"/first.zip"},"a":{"id":"second","url":"/second.zip"},"z":{"id":"replacement","url":"/replacement.zip"}}}"#;
    let split = bytes.len() / 2;
    let mut compressed = gzip(&bytes[..split]);
    compressed.extend(gzip(&bytes[split..]));
    fixture
        .remote
        .lock()
        .unwrap()
        .responses
        .insert(INDEX.into(), (StatusCode::OK, compressed));
    let (status, body) = catalog(&fixture).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["id"].clone())
            .collect::<Vec<_>>(),
        [json!("replacement"), json!("second")]
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn official_catalog_handles_invalid_paths_json_and_bounded_gzip() {
    let fixture = Fixture::new().await;
    for path in ["", "https://untrusted.invalid/index.json", "relative.json"] {
        fixture.respond(MAIN, json!({"products":{"plugins":{"index_url":path}}}));
        assert_eq!(
            catalog(&fixture).await,
            (
                StatusCode::OK,
                json!({
                    "updated_at":null,"plugins":[],"error":"Invalid plugins index_url in main metadata"
                })
            )
        );
    }
    assert_eq!(fixture.remote.lock().unwrap().requests.len(), 3);
    configure(&fixture, json!({}));
    for bytes in [
        b"not json".to_vec(),
        vec![0x1f, 0x8b],
        vec![b' '; MAX_BYTES + 1],
        gzip(&vec![b' '; MAX_BYTES + 1]),
    ] {
        fixture
            .remote
            .lock()
            .unwrap()
            .responses
            .insert(INDEX.into(), (StatusCode::OK, bytes));
        assert_eq!(
            catalog(&fixture).await,
            (
                StatusCode::OK,
                json!({
                    "updated_at":null,"plugins":[],"error":"Failed to fetch plugins metadata"
                })
            )
        );
    }
    fixture.shutdown().await;
}

#[tokio::test]
async fn official_catalog_reads_fallback_manifests_and_skips_invalid_files() {
    let fixture = Fixture::new().await;
    install(&fixture, ".legacy", &json!({}));
    install(&fixture, "bad", &json!([]));
    install(
        &fixture,
        "huge",
        &json!({"id":"huge","padding":"x".repeat(1024 * 1024)}),
    );
    configure(
        &fixture,
        json!({"files":{
            "legacy":{"plugin_id":".legacy","version":"1.0","url":"/legacy.zip"},
            "bad":{"plugin_id":"bad","url":"/bad.zip"},
            "huge":{"plugin_id":"huge","url":"/huge.zip"},
            "external":{"url":"https://untrusted.invalid/p.zip"},"invalid":false
        }}),
    );
    let (_, body) = catalog(&fixture).await;
    assert_eq!(
        body["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| json!([
                row["plugin_id"],
                row["installed"],
                row["installed_version"],
                row["upgrade_available"]
            ]))
            .collect::<Vec<_>>(),
        [
            json!(["bad", false, null, false]),
            json!(["huge", false, null, false]),
            json!([".legacy", true, "0.0.0", true])
        ]
    );
    fixture.shutdown().await;
}

#[cfg(unix)]
#[tokio::test]
async fn official_catalog_does_not_read_installed_symlinks_outside_plugin_directory() {
    let fixture = Fixture::new().await;
    install(&fixture, "linked", &json!({"id":"unused"}));
    let root = fixture.directory.path().join("data/plugins");
    let outside = fixture.directory.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(
        outside.join("plugin.json"),
        br#"{"id":"external","version":"1.0"}"#,
    )
    .unwrap();
    std::os::unix::fs::symlink(&outside, root.join("external")).unwrap();
    std::fs::create_dir(root.join("manifest-link")).unwrap();
    std::os::unix::fs::symlink(
        outside.join("plugin.json"),
        root.join("manifest-link/plugin.json"),
    )
    .unwrap();
    configure(
        &fixture,
        json!({"files":{"external":{"plugin_id":"external","url":"/external.zip"}}}),
    );
    assert_eq!(catalog(&fixture).await.1["plugins"][0]["installed"], false);
    fixture.shutdown().await;
}

#[tokio::test]
#[ignore = "requires qwenpaw Python environment; execute original catalog normalization"]
async fn official_catalog_full_responses_match_original_python() {
    let fixture = Fixture::new().await;
    let versions = [
        "",
        "1.0",
        "1.0.0",
        "1.0rc1",
        "1.0.post1",
        "1.0.dev1",
        "1!1.0",
        "1.0+linux.1",
        "V1.0",
        "not-version",
    ];
    let constraints = [
        json!({}),
        json!({"min_version":"2.2.0"}),
        json!({"min_version":"2.3.0"}),
        json!({"max_version":"3"}),
        json!({"qwenpaw_version":{}}),
        json!({"qwenpaw_version":{"min":" v2.2.0 ","max":"1"}}),
        json!({"qwenpaw_version":{"min":"1","max":null}}),
        json!({"min_version":"1.0a1"}),
        json!({"min_version":"1.0.0a1"}),
        json!({"min_version":"0!1.0"}),
        json!({"min_version":"0!1.0","max_version":"3"}),
        json!({"min_version":"2.2.0","max_version":"broken"}),
    ];
    let mut files = serde_json::Map::new();
    let mut manifests = serde_json::Map::new();
    for (i, installed) in versions.iter().enumerate() {
        for (j, version) in versions.iter().enumerate() {
            let id = format!("v-{i}-{j}");
            let manifest = json!({"id":id,"version":installed});
            install(&fixture, &id, &manifest);
            manifests.insert(id.clone(), manifest);
            files.insert(
                id.clone(),
                json!({"plugin_id":id,"version":version,"url":"/plugin.zip"}),
            );
        }
    }
    for (index, constraint) in constraints.iter().enumerate() {
        let mut entry = constraint.clone();
        entry["id"] = json!(format!("constraint-{index}-1.0-aBCd1234"));
        entry["version"] = json!("1.0");
        entry["url"] = json!("/constraint.zip");
        entry["name"] = json!({"en-US":"","zh-CN":"兼容"});
        entry["description"] = json!({"en":false,"zh":123});
        files.insert(format!("constraint-{index}"), entry);
    }
    files.insert(
        String::from("fallback"),
        json!({"url":"/fallback.zip","name":true,"description":null,"size":0}),
    );
    let index = json!({"updated_at":123,"files":files});
    configure(&fixture, index.clone());
    let actual = catalog(&fixture).await;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = tokio::time::timeout(Duration::from_secs(30), tokio::process::Command::new("python")
        .current_dir(root).args(["-m", "scripts.official_plugin_reference"])
        .arg(json!({"base":fixture.server.inner.desktop_market.download,"index":index,"manifests":manifests}).to_string())
        .kill_on_drop(true).output()).await.unwrap().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        actual,
        (
            StatusCode::OK,
            serde_json::from_slice::<Value>(&output.stdout).unwrap()
        )
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn official_catalog_preserves_the_original_missing_product_response() {
    let fixture = Fixture::new().await;
    fixture.respond("/download/metadata/index.json", json!({"products":{}}));
    let actual = catalog(&fixture).await;
    fixture.shutdown().await;
    assert_eq!(
        actual,
        (
            StatusCode::OK,
            json!({"updated_at":null,"plugins":[],"error":null})
        )
    );
}

#[tokio::test]
async fn official_catalog_preserves_http_success_with_an_upstream_error() {
    let fixture = Fixture::new().await;
    fixture.remote.lock().unwrap().responses.insert(
        String::from("/download/metadata/index.json"),
        (
            StatusCode::SERVICE_UNAVAILABLE,
            b"private upstream error".to_vec(),
        ),
    );
    let actual = catalog(&fixture).await;
    fixture.shutdown().await;
    assert_eq!(
        actual,
        (
            StatusCode::OK,
            json!({
                "updated_at":null,"plugins":[],"error":"Failed to fetch plugin catalog index"
            })
        )
    );
}
