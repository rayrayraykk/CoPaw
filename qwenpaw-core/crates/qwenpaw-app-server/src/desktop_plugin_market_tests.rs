use super::*;
use pretty_assertions::assert_eq;

const REMOTE: &str = "/qwenpaw/openapi/v1/plugins";

#[tokio::test]
async fn plugin_market_timeout_returns_gateway_error_without_an_empty_catalog() {
    let fixture = Fixture::new().await;
    fixture.respond(REMOTE, result());
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    fixture.remote.lock().unwrap().gate = Some((REMOTE.into(), started.clone(), release.clone()));
    let (response, ready) = tokio::join!(
        tokio::time::timeout(Duration::from_secs(20), query(&fixture, "")),
        tokio::time::timeout(Duration::from_secs(5), started.notified())
    );
    ready.unwrap();
    assert_eq!(
        response.unwrap(),
        (
            StatusCode::BAD_GATEWAY,
            json!({"detail":"Failed to fetch from plugin market: Market provider request failed"})
        )
    );
    release.notify_one();
    fixture.shutdown().await;
}

#[tokio::test]
async fn plugin_market_transport_does_not_follow_a_real_redirect() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let visited = Arc::new(AtomicBool::new(false));
    let target = visited.clone();
    let router = Router::new()
        .route(
            "/redirect",
            get(|| async { axum::response::Redirect::temporary("/destination") }),
        )
        .route(
            "/destination",
            get(move || {
                let target = target.clone();
                async move {
                    target.store(true, Ordering::SeqCst);
                    Json(json!({"unexpected":true}))
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/redirect", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    assert_eq!(
        super::super::fetch(url, HeaderMap::new()).await,
        Err(String::from("Market provider returned HTTP 307"))
    );
    assert!(!visited.load(Ordering::SeqCst));
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
}

#[tokio::test]
async fn plugin_market_invalid_queries_return_all_errors_without_contacting_remote() {
    let fixture = Fixture::new().await;
    assert_eq!(
        query(
            &fixture,
            "?page_number=no&page_size=1.5&is_featured=bad&is_trending="
        )
        .await,
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            json!({"detail":[
                {"type":"int_parsing","loc":["query","page_number"],"msg":"Input should be a valid integer, unable to parse string as an integer","input":"no"},
                {"type":"int_parsing","loc":["query","page_size"],"msg":"Input should be a valid integer, unable to parse string as an integer","input":"1.5"},
                {"type":"bool_parsing","loc":["query","is_featured"],"msg":"Input should be a valid boolean, unable to interpret input","input":"bad"},
                {"type":"bool_parsing","loc":["query","is_trending"],"msg":"Input should be a valid boolean, unable to interpret input","input":""}
            ]})
        )
    );
    assert_eq!(fixture.remote.lock().unwrap().requests.len(), 0);
    fixture.shutdown().await;
}

#[tokio::test]
#[ignore = "requires qwenpaw Python environment; execute original plugin search handler"]
async fn plugin_market_queries_and_full_responses_match_original_python() {
    let fixture = Fixture::new().await;
    fixture.respond(REMOTE, result());
    let queries = [
        "",
        "?page_number=2&page_size=7&search=rust%20%26%20%2F&category=agent-tool&sort_by=fauvarate&is_featured=false&is_trending=YES&token=not-forwarded",
        "?page_number=%2B0003.00&page_size=-2&search=&category=&sort_by=",
        "?page_number=no&page_size=1.5&is_featured=bad&is_trending=",
        "?page_number=.0",
        "?page_number=1.&is_featured=%20true%20",
        "?page_number=1_0&page_size=-000",
        "?page_number=9223372036854775808&is_featured=YES&is_trending=off",
        "?page_number=1&page_number=2",
        "?is_featured=on&is_trending=N",
        "?is_featured=1&is_trending=0",
        "?is_featured=t&is_trending=f",
        "?is_featured=y&is_trending=no",
        "?page_number=1e2",
        "?page_number=_1",
        "?page_number=1__0",
        "?page_size=1_",
    ];
    let mut actual = Vec::new();
    for suffix in queries {
        let before = fixture.remote.lock().unwrap().requests.len();
        let (status, body) = query(&fixture, suffix).await;
        let remote = fixture.remote.lock().unwrap();
        let parameters = if remote.requests.len() > before {
            json!(remote.requests.last().unwrap().query)
        } else {
            Value::Null
        };
        actual.push(json!({"status":status.as_u16(),"body":body,"query":parameters}));
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = tokio::process::Command::new("python")
        .current_dir(root)
        .args(["-m", "scripts.plugin_market_reference"])
        .arg(json!(queries).to_string())
        .arg(result().to_string())
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
    fixture.shutdown().await;
}

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_plugin_market_browses_searches_filters_refreshes_and_recovers() {
    let fixture = Fixture::with_browser(true).await;
    let page = |id: &str, name: &str, total: u64| {
        let mut payload = result();
        payload["data"]["total"] = json!(total);
        payload["data"]["plugins"][0]["id"] = json!(id);
        payload["data"]["plugins"][0]["display_name"] = json!(name);
        payload
    };
    fixture.respond(REMOTE, page("@fixture/demo", "Fixture Plugin", 1));
    for (suffix, id, name, total) in [
        (
            "?page_number=1&page_size=20&sort_by=downloads",
            "@fixture/first",
            "First Plugin",
            2,
        ),
        (
            "?page_number=2&page_size=20&sort_by=downloads",
            "@fixture/second",
            "Second Plugin",
            2,
        ),
        (
            "?page_number=1&page_size=20&search=Needle&sort_by=downloads",
            "@fixture/needle",
            "Needle Plugin",
            1,
        ),
    ] {
        fixture.respond(&format!("{REMOTE}{suffix}"), page(id, name, total));
    }
    fixture.remote.lock().unwrap().responses.insert(
        format!("{REMOTE}?page_number=1&page_size=20&search=fail&sort_by=downloads"),
        (
            StatusCode::SERVICE_UNAVAILABLE,
            b"private upstream failure".to_vec(),
        ),
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = tokio::time::timeout(
        Duration::from_secs(120),
        tokio::process::Command::new("node")
            .arg(root.join("scripts/console_plugin_market_smoke.mjs"))
            .arg(&fixture.base)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    for key in [
        "ok",
        "pagination",
        "search",
        "filters",
        "sort",
        "views",
        "refresh",
        "recovery",
        "reload",
    ] {
        assert_eq!(report[key], true);
    }
    {
        let remote = fixture.remote.lock().unwrap();
        for (key, value) in [
            ("page_number", "2"),
            ("search", "Needle"),
            ("search", "fail"),
            ("category", "agent-tool"),
            ("is_featured", "true"),
            ("is_trending", "true"),
            ("sort_by", "updated_time"),
        ] {
            assert!(
                remote
                    .requests
                    .iter()
                    .any(|r| r.path == REMOTE && r.query.get(key).is_some_and(|v| v == value)),
                "{key}={value}"
            );
        }
        assert!(remote.requests.iter().all(|r| r.path == REMOTE));
    }
    fixture.shutdown().await;
}

async fn query(fixture: &Fixture, query: &str) -> (StatusCode, Value) {
    let response = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .get(format!("{}/api/plugins/market/search{query}", fixture.base))
        .header("authorization", "Bearer local-only-fixture")
        .header("cookie", "session=local-only-fixture")
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.bytes().await.unwrap();
    (
        status,
        serde_json::from_slice(&body)
            .unwrap_or_else(|_| json!({"raw":String::from_utf8_lossy(&body)})),
    )
}

fn result() -> Value {
    json!({"success":true,"message":"ok","data":{"total":23,"plugins":[{
        "id":"@fixture/demo","display_name":"Fixture Plugin","version":"1.2.3",
        "developer":"Fixture","owner":"fixture","logo_url":null,"downloads":12,
        "view_count":31,"details_url":null,"locales":{"en":{"description":"Fixture description","category":"agent-tool"}},
        "qwenpaw_compat_labels":["2.x"],"is_featured":true,"is_trending":false
    }]},"extra":{"preserved":true}})
}

#[tokio::test]
async fn plugin_market_search_forwards_only_original_parameters_and_preserves_json() {
    let fixture = Fixture::new().await;
    fixture.respond(REMOTE, result());
    for (suffix, expected) in [
        ("", json!({"page_number":"1","page_size":"20"})),
        (
            "?page_number=2&page_size=7&search=rust%20%26%20%2F&category=agent-tool&sort_by=fauvarate&is_featured=false&is_trending=YES&token=not-forwarded",
            json!({"page_number":"2","page_size":"7","search":"rust & /","category":"agent-tool","sort_by":"fauvarate","is_featured":"false","is_trending":"true"}),
        ),
        (
            "?page_number=%2B0003.00&page_size=-2&search=&category=&sort_by=",
            json!({"page_number":"3","page_size":"-2"}),
        ),
    ] {
        assert_eq!(query(&fixture, suffix).await, (StatusCode::OK, result()));
        let remote = fixture.remote.lock().unwrap();
        let request = remote.requests.last().unwrap();
        assert_eq!(request.path, REMOTE);
        assert_eq!(json!(request.query), expected);
        for name in ["authorization", "cookie", "x-agent-id", "x-api-key"] {
            assert!(!request.headers.contains_key(name), "{name}");
        }
    }
    let failed = json!({"success":false,"message":"fixture unavailable","data":null});
    fixture.respond(REMOTE, failed.clone());
    assert_eq!(query(&fixture, "").await, (StatusCode::OK, failed));
    fixture.shutdown().await;
}

#[tokio::test]
async fn plugin_market_errors_are_not_hidden_as_empty_successful_catalogs() {
    let fixture = Fixture::new().await;
    for (status, bytes, message) in [
        (
            StatusCode::SERVICE_UNAVAILABLE,
            b"private upstream body".to_vec(),
            "Market provider returned HTTP 503",
        ),
        (
            StatusCode::FOUND,
            b"redirect".to_vec(),
            "Market provider returned HTTP 302",
        ),
        (
            StatusCode::OK,
            b"not json".to_vec(),
            "Market provider returned invalid JSON",
        ),
        (
            StatusCode::OK,
            vec![b'x'; MAX_BYTES + 1],
            "Market provider response exceeds size limit",
        ),
    ] {
        fixture
            .remote
            .lock()
            .unwrap()
            .responses
            .insert(REMOTE.into(), (status, bytes));
        assert_eq!(
            query(&fixture, "").await,
            (
                StatusCode::BAD_GATEWAY,
                json!({"detail":format!("Failed to fetch from plugin market: {message}")})
            )
        );
    }
    assert_eq!(fixture.remote.lock().unwrap().requests.len(), 4);
    fixture.shutdown().await;
}
