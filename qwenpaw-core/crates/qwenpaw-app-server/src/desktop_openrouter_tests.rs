use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use pretty_assertions::assert_eq;
use qwenpaw_core::ModelConfig;

use super::*;

fn rows() -> Value {
    json!({"data": [
        {"id": "alpha/vision-free", "name": "Ignored title", "architecture": {
            "input_modalities": ["text", "image"], "output_modalities": ["text"]},
            "pricing": {"prompt": "0", "completion": 0, "unused": null}},
        {"id": "beta/video-paid", "architecture": {"input_modalities": ["text", "video"],
            "output_modalities": ["text", "image"]}, "pricing": {"prompt": "0.000002", "completion": "0.000004"}},
        {"id": "alpha/vision-free", "pricing": {"prompt": "999"}},
        {"id": " standalone ", "name": " Standalone Model ", "pricing": {}},
        {"id": ""}
    ]})
}

fn first_model() -> Value {
    json!({"id": "alpha/vision-free", "name": "vision-free", "supports_multimodal": true,
        "supports_image": true, "supports_video": false, "probe_source": "documentation",
        "is_free": true, "provider": "alpha", "input_modalities": ["text", "image"],
        "output_modalities": ["text"], "pricing": {"prompt": "0", "completion": "0"}})
}

#[test]
fn normalizes_extended_metadata_and_does_not_classify_unknown_or_tiny_prices_as_free() {
    assert_eq!(
        serde_json::to_value(normalize(&rows()["data"][0]).unwrap().1).unwrap(),
        first_model()
    );
    for (pricing, free) in [
        (json!({}), false),
        (json!({"prompt": "unavailable"}), false),
        (json!({"prompt": "-0.000e-12", "completion": 0}), true),
        (json!({"prompt": "1e-9999", "completion": "0"}), false),
        (json!({"prompt": "0", "request": "0.01"}), false),
        (json!({"prompt": "0", "completion": "NaN"}), false),
    ] {
        let model = normalize(&json!({"id": "test/price", "pricing": pricing}))
            .unwrap()
            .1;
        assert_eq!(model.is_free, free, "{pricing}");
    }
    assert!(normalize(&json!({"id": " "})).is_none());
    assert_eq!(
        normalize(&json!({"id": "one/two/three"})).unwrap().1.name,
        "three"
    );
}

#[test]
fn filtering_preserves_legacy_any_modality_and_false_free_semantics() {
    let paid = normalize(&rows()["data"][1]).unwrap().1;
    let free = normalize(&rows()["data"][0]).unwrap().1;
    for (body, expected) in [
        (json!({}), vec![true, true]),
        (json!({"providers": ["ALPHA"]}), vec![true, false]),
        (
            json!({"input_modalities": ["image", "video"]}),
            vec![true, true],
        ),
        (
            json!({"output_modalities": ["image", "audio"]}),
            vec![false, true],
        ),
        (json!({"is_free": false}), vec![true, true]),
        (json!({"is_free": true}), vec![true, false]),
        (json!({"max_prompt_price": 0.000_001}), vec![true, false]),
        (json!({"max_prompt_price": 0.000_002}), vec![true, true]),
        (json!({"max_prompt_price": -1}), vec![false, false]),
    ] {
        let filter: FilterRequest = serde_json::from_value(body.clone()).unwrap();
        assert_eq!(
            [&free, &paid]
                .iter()
                .map(|model| matches_filter(model, &filter).unwrap())
                .collect::<Vec<_>>(),
            expected,
            "{body}"
        );
    }
}

#[derive(Default)]
struct Credentials(Mutex<BTreeMap<String, String>>);

impl DesktopCredentialStore for Credentials {
    fn load_api_key(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
    fn save_api_key(&self, _: Option<&str>) -> anyhow::Result<()> {
        anyhow::bail!("unused default credential")
    }
    fn load_agent_setting_secret(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.0.lock().unwrap().get(key).cloned())
    }
    fn save_agent_setting_secret(&self, key: &str, value: Option<&str>) -> anyhow::Result<()> {
        let mut values = self.0.lock().unwrap();
        if let Some(value) = value {
            values.insert(key.to_owned(), value.to_owned());
        } else {
            values.remove(key);
        }
        Ok(())
    }
}

struct Remote {
    payload: Value,
    status: StatusCode,
    requests: Vec<BTreeMap<String, String>>,
    queries: Vec<String>,
}

async fn mock_catalog(
    State(remote): State<Arc<Mutex<Remote>>>,
    axum::extract::RawQuery(query): axum::extract::RawQuery,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    let mut remote = remote.lock().unwrap();
    remote.requests.push(
        headers
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_str().unwrap().to_owned()))
            .collect(),
    );
    let query = query.unwrap_or_default();
    remote.queries.push(query.clone());
    let payload = remote.payload.get("pages").map_or_else(
        || remote.payload.clone(),
        |pages| {
            pages
                .get(&query)
                .cloned()
                .unwrap_or_else(|| json!({"error": "unexpected cursor"}))
        },
    );
    (remote.status, Json(payload))
}

struct Fixture {
    _directory: tempfile::TempDir,
    server: AppServer,
    base: String,
    http: tokio::task::JoinHandle<anyhow::Result<()>>,
    remote: Arc<Mutex<Remote>>,
    remote_task: tokio::task::JoinHandle<()>,
    credentials: Arc<Credentials>,
}

impl Fixture {
    async fn new(browser: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let remote = Arc::new(Mutex::new(Remote {
            payload: rows(),
            status: StatusCode::OK,
            requests: Vec::new(),
            queries: Vec::new(),
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let remote_base = format!("http://{}/v1", listener.local_addr().unwrap());
        let router = Router::new()
            .route("/v1/models", get(mock_catalog))
            .with_state(remote.clone());
        let remote_task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let console = if browser {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../console/dist")
                .canonicalize()
                .unwrap()
        } else {
            let console = directory.path().join("console");
            fs::create_dir_all(&console).unwrap();
            fs::write(console.join("index.html"), "fixture").unwrap();
            console
        };
        let workspace = directory.path().join("workspace");
        let data = directory.path().join("data");
        fs::create_dir_all(&workspace).unwrap();
        let core = Core::persistent(
            ModelConfig {
                api_key: None,
                base_url: String::from("http://127.0.0.1:1/v1"),
                default_model: String::from("fixture-model"),
            },
            &data.join("threads.sqlite3"),
        )
        .unwrap();
        let credentials = Arc::new(Credentials::default());
        core.write_ui_language("en").unwrap();
        let server = AppServer::new_desktop_with_stores_and_workspace(
            core,
            &console,
            String::from("openrouter-fixture-shutdown"),
            credentials.clone(),
            &data,
            &workspace,
        )
        .unwrap();
        let _ = configure_provider(State(server.clone()), Path(String::from("openrouter")), Json(json!({
            "base_url": remote_base, "api_key": "fixture-original-key", "custom_headers": {"x-openrouter-title": "Fixture Override", "x-fixture": "value"}
        }))).await.unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let http = tokio::spawn(server.clone().run_http(listener));
        Self {
            _directory: directory,
            server,
            base,
            http,
            remote,
            remote_task,
            credentials,
        }
    }

    async fn shutdown(self) {
        self.server.inner.shutdown.cancel();
        tokio::time::timeout(Duration::from_secs(5), self.http)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        self.remote_task.abort();
        let _ = self.remote_task.await;
    }
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn original_http_catalog_filters_credentials_and_add_survive_restart() {
    let fixture = Fixture::new(false).await;
    let client = reqwest::Client::new();
    let url = format!("{}/api/models/openrouter", fixture.base);
    let before = read_registry(&fixture.server).unwrap();
    let series: Value = client
        .get(format!("{url}/series"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(series, json!({"series": ["alpha", "beta"]}));
    let extended: Value = client
        .post(format!("{url}/discover-extended"))
        .json(&json!({"api_key": "fixture-replacement-key"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let expected = json!([first_model(),
        {"id": "beta/video-paid", "name": "video-paid", "provider": "beta",
            "supports_multimodal": true, "supports_image": false, "supports_video": true,
            "probe_source": "documentation", "is_free": false, "input_modalities": ["text", "video"],
            "output_modalities": ["text", "image"], "pricing": {"prompt": "0.000002", "completion": "0.000004"}},
        {"id": "standalone", "name": "Standalone Model", "provider": "",
            "supports_multimodal": false, "supports_image": false, "supports_video": false,
            "probe_source": "documentation", "is_free": false, "input_modalities": [],
            "output_modalities": [], "pricing": {}}
    ]);
    assert_eq!(
        extended,
        json!({"success": true, "models": expected, "providers": ["alpha", "beta"], "total_count": 3})
    );
    let filtered: Value = client
        .post(format!("{url}/models/filter"))
        .json(&json!({"input_modalities": ["image", "video"], "is_free": true}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        filtered,
        json!({"success": true, "models": [first_model()], "total_count": 1})
    );
    assert_eq!(
        serde_json::to_value(
            &read_registry(&fixture.server).unwrap().providers["openrouter"].extra_models
        )
        .unwrap(),
        serde_json::to_value(&before.providers["openrouter"].extra_models).unwrap()
    );
    assert_eq!(
        load_provider_secret(&fixture.server, "openrouter")
            .await
            .unwrap(),
        Some(String::from("fixture-replacement-key"))
    );
    for request in &fixture.remote.lock().unwrap().requests {
        assert_eq!(request["x-openrouter-title"], "Fixture Override");
        assert_eq!(request["http-referer"], "https://qwenpaw.agentscope.io/");
        assert_eq!(
            request["x-openrouter-categories"],
            "personal-agent,cli-agent"
        );
        assert_eq!(request["x-fixture"], "value");
        assert!(matches!(
            request["authorization"].as_str(),
            "Bearer fixture-original-key" | "Bearer fixture-replacement-key"
        ));
    }
    assert_eq!(
        fixture
            .remote
            .lock()
            .unwrap()
            .requests
            .iter()
            .map(|request| request["authorization"].clone())
            .collect::<Vec<_>>(),
        [
            "Bearer fixture-original-key",
            "Bearer fixture-replacement-key",
            "Bearer fixture-replacement-key"
        ]
    );
    let response = client
        .post(format!("{url}/models"))
        .json(&first_model())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let model =
        read_registry(&fixture.server).unwrap().providers["openrouter"].extra_models[0].clone();
    assert_eq!(model.id, "alpha/vision-free");
    assert_eq!(
        (
            model.is_free,
            model.supports_image,
            model.probe_source.as_deref()
        ),
        (true, Some(true), Some("documentation"))
    );
    let workspace = desktop_workspace(&fixture.server).unwrap();
    let reopened = Core::persistent(
        fixture.server.inner.core.backup_model_config(),
        &workspace.data_dir.join("threads.sqlite3"),
    )
    .unwrap();
    initialize(&reopened, fixture.credentials.as_ref(), workspace).unwrap();
    assert_eq!(
        serde_json::to_value(
            &read_registry_from(workspace).unwrap().providers["openrouter"].extra_models
        )
        .unwrap(),
        serde_json::to_value(vec![model]).unwrap()
    );
    assert!(
        !fs::read_to_string(workspace.data_dir.join("models/registry.json"))
            .unwrap()
            .contains("fixture-replacement-key")
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn openrouter_probe_uses_catalog_metadata_and_preserves_known_capabilities_on_failure() {
    let fixture = Fixture::new(false).await;
    let client = reqwest::Client::new();
    let url = format!("{}/api/models/openrouter/models", fixture.base);
    let _ = client
        .post(&url)
        .json(&first_model())
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let probe_url = format!("{url}/alpha%2Fvision-free/probe-multimodal");
    let response: Value = client
        .post(&probe_url)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        response,
        json!({"supports_multimodal": true, "supports_image": true, "supports_video": false,
        "image_message": "Image capability reported by OpenRouter model metadata: true",
        "video_message": "Video capability reported by OpenRouter model metadata: false"})
    );
    assert_eq!(fixture.remote.lock().unwrap().requests.len(), 1);
    let registry = serde_json::to_value(read_registry(&fixture.server).unwrap()).unwrap();
    assert_eq!(
        registry["providers"]["openrouter"]["extra_models"][0]["probe_source"],
        "documentation"
    );
    for (status, payload, expected) in [
        (StatusCode::OK, json!({"data": []}), StatusCode::BAD_REQUEST),
        (
            StatusCode::UNAUTHORIZED,
            json!({"error": "fixture-original-key"}),
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
    ] {
        {
            let mut remote = fixture.remote.lock().unwrap();
            remote.status = status;
            remote.payload = payload;
        }
        let response = client.post(&probe_url).send().await.unwrap();
        assert_eq!(response.status(), expected);
        assert!(
            !response
                .text()
                .await
                .unwrap()
                .contains("fixture-original-key")
        );
        assert_eq!(
            serde_json::to_value(read_registry(&fixture.server).unwrap()).unwrap(),
            registry
        );
    }
    fixture.shutdown().await;
}

#[tokio::test]
async fn catalog_pagination_replaces_the_previous_cursor_for_both_discovery_contracts() {
    let fixture = Fixture::new(false).await;
    fixture.remote.lock().unwrap().payload = json!({"pages": {
        "": {"data": [{"id": "one"}], "has_more": true, "last_id": "one"},
        "after=one": {"data": [{"id": "two"}], "has_more": true, "last_id": "two"},
        "after=two": {"data": [{"id": "three"}], "has_more": false}
    }});
    let client = reqwest::Client::new();
    for suffix in ["discover-extended", "discover?save=false"] {
        let response = client
            .post(format!("{}/api/models/openrouter/{suffix}", fixture.base))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        assert_eq!(response["success"], true, "{response}");
        assert_eq!(
            response["models"]
                .as_array()
                .unwrap()
                .iter()
                .map(|model| model["id"].clone())
                .collect::<Vec<_>>(),
            vec![json!("one"), json!("two"), json!("three")]
        );
    }
    assert_eq!(
        fixture.remote.lock().unwrap().queries,
        ["", "after=one", "after=two", "", "after=one", "after=two"]
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn failed_catalog_is_not_successful_filtering_or_secret_leakage() {
    let fixture = Fixture::new(false).await;
    let client = reqwest::Client::new();
    let before = read_registry(&fixture.server).unwrap();
    for status in [
        StatusCode::UNAUTHORIZED,
        StatusCode::TEMPORARY_REDIRECT,
        StatusCode::OK,
    ] {
        {
            let mut remote = fixture.remote.lock().unwrap();
            remote.status = status;
            remote.payload = json!({"error": "private fixture-original-key"});
        }
        for (method, suffix, body, expected) in [
            (
                reqwest::Method::GET,
                "series",
                None,
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
            (
                reqwest::Method::POST,
                "models/filter",
                Some(json!({})),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
            (
                reqwest::Method::POST,
                "discover-extended",
                None,
                StatusCode::OK,
            ),
        ] {
            let mut request = client.request(
                method,
                format!("{}/api/models/openrouter/{suffix}", fixture.base),
            );
            if let Some(body) = body {
                request = request.json(&body);
            }
            let response = request.send().await.unwrap();
            assert_eq!(response.status(), expected);
            let value = response.json::<Value>().await.unwrap();
            assert_eq!(
                value,
                if suffix == "discover-extended" {
                    json!({"success": false, "models": [], "providers": [], "total_count": 0})
                } else {
                    json!({"detail": "OpenRouter model catalog could not be fetched"})
                }
            );
        }
    }
    assert_eq!(
        serde_json::to_value(read_registry(&fixture.server).unwrap()).unwrap(),
        serde_json::to_value(before).unwrap()
    );
    fixture.shutdown().await;
}

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_openrouter_browser_filters_and_adds_models() {
    let fixture = Fixture::new(true).await;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let result = tokio::time::timeout(
        Duration::from_secs(90),
        tokio::process::Command::new("node")
            .arg(root.join("scripts/console_browser_smoke.mjs"))
            .args([&fixture.base, "/models", "--openrouter-crud"])
            .kill_on_drop(true)
            .output(),
    )
    .await;
    let output = result.unwrap().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let report: Value =
        serde_json::from_str(&stdout).unwrap_or_else(|_| panic!("{stdout}\n{stderr}"));
    assert!(output.status.success(), "{report:#}\n{stderr}");
    assert_eq!(report["ok"], true);
    let expected = json!({"series": true, "filtered": true, "added": true, "reload": true, "metadataProbe": true});
    assert_eq!(report["pages"][0]["openrouterCrud"], expected);
    let registry = read_registry(&fixture.server).unwrap();
    assert_eq!(registry.providers["openrouter"].extra_models.len(), 1);
    assert_eq!(
        registry.providers["openrouter"].extra_models[0].id,
        "alpha/vision-free"
    );
    println!("{expected:#}");
    fixture.shutdown().await;
}
