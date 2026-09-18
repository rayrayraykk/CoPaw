use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
#[ignore = "requires the qwenpaw conda environment; run explicitly for original Python parity"]
async fn original_plugin_management_python_list_status_and_assets_match_rust() {
    let fixture = Fixture::new();
    fixture.install("demo", &frontend_manifest());
    fs::create_dir(fixture.plugins().join("demo/ui")).unwrap();
    let (status, list) = fixture.request("GET", "/api/plugins").await;
    let mut actual = vec![json!([status.as_u16(), list])];
    for name in ["main.js", "chunk-abcdefgh.js", "style.css"] {
        fs::write(fixture.plugins().join("demo/ui").join(name), "hello").unwrap();
        for (method, range, conditional) in [
            ("GET", false, false),
            ("GET", true, false),
            ("GET", false, true),
        ] {
            let mut request = Request::builder()
                .method(method)
                .uri(format!("/api/plugins/demo/files/ui/{name}"));
            if range {
                request = request.header("range", "bytes=1-2");
            }
            if conditional {
                request = request.header("if-modified-since", "Sat, 01 Jan 2050 00:00:00 GMT");
            }
            let response = fixture
                .server
                .clone()
                .router()
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            let status = response.status().as_u16();
            let mime = response.headers()["content-type"]
                .to_str()
                .unwrap()
                .to_owned();
            let cache = response.headers()["cache-control"]
                .to_str()
                .unwrap()
                .to_owned();
            let bytes = axum::body::to_bytes(response.into_body(), 1024)
                .await
                .unwrap();
            actual.push(json!({"status":status,"mime":mime,"cache":cache,
                "body":String::from_utf8(bytes.to_vec()).unwrap()}));
        }
    }
    for id in ["demo", "missing", "broken"] {
        if id == "broken" {
            fixture.install("broken", &Value::Null);
        }
        let (status, body) = fixture
            .request("GET", &format!("/api/plugins/{id}/status"))
            .await;
        actual.push(json!([status.as_u16(), body]));
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = tokio::process::Command::new("python")
        .current_dir(root)
        .args(["-m", "scripts.plugin_management_reference"])
        .arg(frontend_manifest().to_string())
        .kill_on_drop(true)
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        json!(actual),
        serde_json::from_slice::<Value>(&output.stdout).unwrap()
    );
}

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_plugin_management_page_lists_filters_switches_views_and_reloads() {
    let mut fixture = Fixture::new();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    Arc::get_mut(&mut fixture.server.inner)
        .unwrap()
        .console_static_dir = Some(root.join("../console/dist").canonicalize().unwrap());
    fixture.server.inner.core.write_ui_language("en").unwrap();
    for (id, name) in [("notes", "Fixture Notes"), ("tasks", "Fixture Tasks")] {
        fixture.install(
            id,
            &json!({"id":id,"name":name,"version":"1.2.3","type":"general"}),
        );
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let http = tokio::spawn(fixture.server.clone().run_http(listener));
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        tokio::process::Command::new("node")
            .arg(root.join("scripts/console_plugin_manager_smoke.mjs"))
            .arg(base)
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
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    for key in ["ok", "list", "search", "views", "refresh", "reload"] {
        assert_eq!(report[key], true);
    }
}

#[tokio::test]
async fn plugin_management_reads_discover_and_reopen_original_disk_records() {
    let mut fixture = Fixture::new();
    assert_eq!(
        fixture.request("GET", "/api/plugins").await,
        (StatusCode::OK, json!([]))
    );
    assert!(!fixture.plugins().exists());
    fixture.install("demo", &frontend_manifest());
    fixture.install("broken", &Value::Null);
    fixture.install("old.disabled", &frontend_manifest());
    for reopen in [false, true] {
        if reopen {
            fixture.server = Fixture::open(fixture.directory.path());
        }
        assert_eq!(
            fixture.request("GET", "/api/plugins").await,
            (StatusCode::OK, json!([frontend_info()]))
        );
        for id in ["demo", "broken", "old.disabled"] {
            assert_eq!(
                fixture
                    .request("GET", &format!("/api/plugins/{id}/status"))
                    .await,
                (
                    StatusCode::OK,
                    json!({"id":id,"loaded":false,"enabled":false})
                )
            );
        }
        assert_eq!(
            fixture.request("GET", "/api/plugins/missing/status").await,
            (
                StatusCode::NOT_FOUND,
                json!({"detail":"Plugin 'missing' not found."})
            )
        );
    }
}

#[tokio::test]
async fn plugin_management_files_match_public_assets_and_reject_path_escape() {
    let fixture = Fixture::new();
    fixture.install("demo", &frontend_manifest());
    fs::write(fixture.plugins().join("demo/main.js"), "fixture").unwrap();
    for prefix in ["plugins", "frontend_plugin"] {
        let response = fixture
            .server
            .clone()
            .router()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/{prefix}/demo/files/main.js"))
                    .header("range", "bytes=1-3")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.headers()["content-type"], "application/javascript");
        assert_eq!(response.headers()["cache-control"], "no-cache");
        assert_eq!(
            axum::body::to_bytes(response.into_body(), 1024)
                .await
                .unwrap()
                .as_ref(),
            b"ixt"
        );
    }
    for url in [
        "/api/plugins/demo/files/%2e%2e/secret",
        "/api/plugins/C:%5csecret/status",
    ] {
        assert_eq!(
            fixture.request("GET", url).await,
            (StatusCode::FORBIDDEN, json!({"detail":"Access denied"}))
        );
    }
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("plugin.json"), "outside").unwrap();
        std::os::unix::fs::symlink(outside.path(), fixture.plugins().join("linked")).unwrap();
        assert_eq!(
            fixture.request("GET", "/api/plugins/linked/status").await,
            (
                StatusCode::NOT_FOUND,
                json!({"detail":"Plugin 'linked' not found."})
            )
        );
        let directory = fixture.plugins().join("manifest-link");
        fs::create_dir(&directory).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("plugin.json"),
            directory.join("plugin.json"),
        )
        .unwrap();
        assert_eq!(
            fixture
                .request("GET", "/api/plugins/manifest-link/status")
                .await,
            (StatusCode::FORBIDDEN, json!({"detail":"Access denied"}))
        );
        fs::create_dir(fixture.plugins().join("no-manifest")).unwrap();
        assert_eq!(
            fixture
                .request("GET", "/api/plugins/no-manifest/status")
                .await,
            (
                StatusCode::NOT_FOUND,
                json!({"detail":"Plugin 'no-manifest' not found."})
            )
        );
    }
}
