use super::*;
use pretty_assertions::assert_eq;

#[path = "desktop_plugin_manager_tests.rs"]
mod manager;

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_frontend_plugin_opens_interacts_reloads_and_returns_to_apps() {
    let mut fixture = Fixture::new();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    Arc::get_mut(&mut fixture.server.inner)
        .unwrap()
        .console_static_dir = Some(root.join("../console/dist").canonicalize().unwrap());
    fixture.server.inner.core.write_ui_language("en").unwrap();
    fixture.install("demo", &frontend_manifest());
    fs::create_dir(fixture.plugins().join("demo/ui")).unwrap();
    fs::write(fixture.plugins().join("demo/ui/main.js"), r"
const {React} = window.QwenPaw.host;
function Demo() {
  const [count,setCount] = React.useState(0);
  return React.createElement('section',null,
    React.createElement('h2',null,'Fixture plugin page'),
    React.createElement('p',{id:'fixture-count'},'Count: '+count),
    React.createElement('button',{id:'fixture-increment',onClick:()=>setCount(count+1)},'Increment'));
}
window.QwenPaw.registerRoutes('demo',[{path:'/apps/demo',component:Demo,label:'Demo'}]);
").unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let http = tokio::spawn(fixture.server.clone().run_http(listener));
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        tokio::process::Command::new("node")
            .arg(root.join("scripts/console_frontend_plugin_smoke.mjs"))
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
    for key in ["ok", "open", "interaction", "reload", "back", "reopen"] {
        assert_eq!(report[key], true);
    }
}

fn frontend_manifest() -> Value {
    json!({"id":"demo", "name":"Demo", "version":"1.2.3",
        "type":"app", "entry":{"frontend":"ui/main.js"},
        "meta":{"pawapp":{"entry_page":"/apps/demo"}}})
}

fn frontend_info() -> Value {
    json!({"id":"demo", "name":"Demo", "version":"1.2.3",
        "description":"", "author":"", "enabled":true,"loaded":false,
        "plugin_type":"app", "frontend_entry":"ui/main.js"})
}

#[tokio::test]
#[ignore = "requires the qwenpaw conda environment; run explicitly for original Python parity"]
async fn original_frontend_plugin_python_list_and_assets_match_rust() {
    let fixture = Fixture::new();
    fixture.install("demo", &frontend_manifest());
    fs::create_dir(fixture.plugins().join("demo/ui")).unwrap();
    let (status, list) = fixture.request("GET", "/api/frontend_plugin").await;
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
                .uri(format!("/api/frontend_plugin/demo/files/ui/{name}"));
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
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = tokio::process::Command::new("python")
        .arg(root.join("scripts/frontend_plugin_reference.py"))
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
async fn frontend_plugins_discover_reopen_and_skip_disabled_without_fake_loaded_state() {
    let mut fixture = Fixture::new();
    assert_eq!(
        fixture.request("GET", "/api/frontend_plugin").await,
        (StatusCode::OK, json!([]))
    );
    assert!(!fixture.plugins().exists());
    fixture.install("demo", &frontend_manifest());
    fixture.install(".hidden", &frontend_manifest());
    fixture.install("old.disabled", &frontend_manifest());
    fixture.install("broken", &Value::Null);
    fixture.install("z-general", &json!({"id":"z-general","version":"0.0.0"}));
    let expected = json!([frontend_info(),{"id":"z-general","name":"z-general",
        "version":"0.0.0","description":"","author":"","enabled":true,
        "loaded":false,"plugin_type":"general","frontend_entry":null}]);
    for reopen in [false, true] {
        if reopen {
            fixture.server = Fixture::open(fixture.directory.path());
        }
        assert_eq!(
            fixture.request("GET", "/api/frontend_plugin").await,
            (StatusCode::OK, expected.clone())
        );
    }
    fs::rename(
        fixture.plugins().join("demo"),
        fixture.plugins().join("demo.disabled"),
    )
    .unwrap();
    assert_eq!(
        fixture.request("GET", "/api/frontend_plugin").await,
        (StatusCode::OK, json!([expected[1]]))
    );
}

#[tokio::test]
async fn frontend_plugins_keep_original_type_inference_order() {
    let fixture = Fixture::new();
    for (id, meta, entry, kind) in [
        (
            "a",
            json!({"tools":["a"],"chat_model":"b"}),
            Value::Null,
            "tool",
        ),
        (
            "b",
            json!({"provider_id":"b","hook_type":"x"}),
            Value::Null,
            "provider",
        ),
        (
            "c",
            json!({"hook_type":"x","commands":["c"]}),
            Value::Null,
            "hook",
        ),
        (
            "d",
            json!({"commands":["c"],"channel":"d"}),
            Value::Null,
            "command",
        ),
        ("e", json!({"channel":"e"}), json!("main.js"), "channel"),
        ("f", json!({}), json!("main.js"), "frontend"),
        ("g", json!({}), Value::Null, "general"),
    ] {
        fixture.install(
            id,
            &json!({"id":id,"version":"0.0.0","meta":meta,"entry":{"frontend":entry}}),
        );
        let (_, list) = fixture.request("GET", "/api/frontend_plugin").await;
        let actual = list
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == id)
            .unwrap();
        assert_eq!(
            actual,
            &json!({"id":id,"name":id,"version":"0.0.0",
            "description":"","author":"","enabled":true,"loaded":false,
            "plugin_type":kind,"frontend_entry":entry})
        );
    }
}

#[tokio::test]
async fn frontend_plugin_files_keep_mime_cache_head_range_and_hot_changes() {
    let fixture = Fixture::new();
    fixture.install("demo", &frontend_manifest());
    fs::create_dir(fixture.plugins().join("demo/ui")).unwrap();
    for (name, mime, cache) in [
        ("main.js", "application/javascript", "no-cache"),
        ("main.mjs", "application/javascript", "no-cache"),
        ("style.css", "text/css; charset=utf-8", "no-cache"),
        (
            "chunk-abcdefgh.js",
            "application/javascript",
            "public, max-age=31536000, immutable",
        ),
        ("chunk-abcdefg.js", "application/javascript", "no-cache"),
    ] {
        let file = fixture.plugins().join("demo/ui").join(name);
        for content in ["first", "later"] {
            fs::write(&file, content).unwrap();
            for (method, range, status, expected) in [
                ("GET", false, StatusCode::OK, content),
                ("HEAD", false, StatusCode::OK, ""),
                ("GET", true, StatusCode::PARTIAL_CONTENT, &content[1..3]),
            ] {
                let mut request = Request::builder()
                    .method(method)
                    .uri(format!("/api/frontend_plugin/demo/files/ui/{name}"));
                if range {
                    request = request.header("range", "bytes=1-2");
                }
                let response = fixture
                    .server
                    .clone()
                    .router()
                    .oneshot(request.body(Body::empty()).unwrap())
                    .await
                    .unwrap();
                assert_eq!(response.status(), status);
                assert_eq!(response.headers()["content-type"], mime);
                assert_eq!(response.headers()["cache-control"], cache);
                let bytes = axum::body::to_bytes(response.into_body(), 1024)
                    .await
                    .unwrap();
                assert_eq!(bytes.as_ref(), expected.as_bytes());
            }
        }
    }
    for (url, status, detail) in [
        (
            "/api/frontend_plugin/missing/files/main.js",
            StatusCode::NOT_FOUND,
            "Plugin 'missing' not found",
        ),
        (
            "/api/frontend_plugin/demo/files/missing.js",
            StatusCode::NOT_FOUND,
            "File not found: missing.js",
        ),
        (
            "/api/frontend_plugin/demo/files/%2e%2e/outside.js",
            StatusCode::FORBIDDEN,
            "Access denied",
        ),
        (
            "/api/frontend_plugin/demo/files/C:%5csecret",
            StatusCode::FORBIDDEN,
            "Access denied",
        ),
    ] {
        assert_eq!(
            fixture.request("GET", url).await,
            (status, json!({"detail":detail}))
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn frontend_plugins_reject_escape_links_and_oversize_manifests() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let outside = tempfile::tempdir().unwrap();
    fixture.install("demo", &frontend_manifest());
    fs::write(outside.path().join("secret.js"), "outside fixture").unwrap();
    symlink(
        outside.path().join("secret.js"),
        fixture.plugins().join("demo/escape.js"),
    )
    .unwrap();
    assert_eq!(
        fixture
            .request("GET", "/api/frontend_plugin/demo/files/escape.js")
            .await,
        (StatusCode::FORBIDDEN, json!({"detail":"Access denied"}))
    );
    symlink(
        fixture.plugins().join("demo"),
        fixture.plugins().join("linked"),
    )
    .unwrap();
    fixture.install(
        "huge",
        &json!({"id":"huge","description":"x".repeat(1024*1024)}),
    );
    assert_eq!(
        fixture.request("GET", "/api/frontend_plugin").await,
        (StatusCode::OK, json!([frontend_info()]))
    );
}
