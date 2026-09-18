#![allow(clippy::too_many_lines, clippy::needless_pass_by_value)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use axum::response::Redirect;
use pretty_assertions::assert_eq;
use qwenpaw_core::ModelConfig;

use super::*;

#[derive(Default)]
struct Credentials {
    values: Mutex<BTreeMap<String, String>>,
    fail_write: AtomicBool,
}

impl DesktopCredentialStore for Credentials {
    fn load_api_key(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
    fn save_api_key(&self, _: Option<&str>) -> anyhow::Result<()> {
        anyhow::bail!("unexpected default credential write")
    }
    fn load_agent_setting_secret(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.values.lock().unwrap().get(key).cloned())
    }
    fn save_agent_setting_secret(&self, key: &str, value: Option<&str>) -> anyhow::Result<()> {
        let mut values = self.values.lock().unwrap();
        if let Some(value) = value {
            values.insert(key.to_owned(), value.to_owned());
        } else {
            values.remove(key);
        }
        // A failed secure-store write may already have changed the value.
        if self.fail_write.swap(false, Ordering::SeqCst) {
            anyhow::bail!("fixture secret store diagnostic must not leak")
        }
        Ok(())
    }
}

struct Remote {
    codes: BTreeMap<String, String>,
    starts: Vec<BTreeMap<String, String>>,
    exchanges: Vec<Value>,
    discovery_auth: Vec<String>,
    exchange_status: StatusCode,
    exchange_body: Value,
    discovery_status: StatusCode,
    gate: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
}

impl Default for Remote {
    fn default() -> Self {
        Self {
            codes: BTreeMap::new(),
            starts: Vec::new(),
            exchanges: Vec::new(),
            discovery_auth: Vec::new(),
            exchange_status: StatusCode::OK,
            exchange_body: json!({"key":"fixture-new-key"}),
            discovery_status: StatusCode::OK,
            gate: None,
        }
    }
}

async fn authorize(
    State(remote): State<Arc<Mutex<Remote>>>,
    Query(params): Query<BTreeMap<String, String>>,
) -> Redirect {
    assert_eq!(params["code_challenge_method"], "S256");
    let mut callback = url::Url::parse(&params["callback_url"]).unwrap();
    let state = callback
        .query_pairs()
        .find(|(key, _)| key == "state")
        .unwrap()
        .1
        .into_owned();
    let code = format!("fixture-code-{state}");
    remote
        .lock()
        .unwrap()
        .codes
        .insert(code.clone(), params["code_challenge"].clone());
    remote.lock().unwrap().starts.push(params);
    callback.query_pairs_mut().append_pair("code", &code);
    Redirect::to(callback.as_str())
}

async fn keys(
    State(remote): State<Arc<Mutex<Remote>>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
    assert!(!headers.contains_key("authorization"));
    let (status, payload, gate) = {
        let mut remote = remote.lock().unwrap();
        remote.exchanges.push(body.clone());
        let challenge = remote.codes.remove(body["code"].as_str().unwrap());
        let verifier = body["code_verifier"].as_str().unwrap();
        assert_eq!(body["code_challenge_method"], "S256");
        assert_eq!(
            challenge,
            Some(URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())))
        );
        (
            remote.exchange_status,
            remote.exchange_body.clone(),
            remote.gate.clone(),
        )
    };
    if let Some((started, release)) = gate {
        started.notify_one();
        release.notified().await;
    }
    (status, Json(payload))
}

async fn models(
    State(remote): State<Arc<Mutex<Remote>>>,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    let mut remote = remote.lock().unwrap();
    remote.discovery_auth.push(
        headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned(),
    );
    (
        remote.discovery_status,
        Json(
            json!({"data":[{"id":"fixture/model","name":"Fixture Model","context_length":32000}]}),
        ),
    )
}

struct Fixture {
    directory: tempfile::TempDir,
    server: AppServer,
    base: String,
    credentials: Arc<Credentials>,
    remote: Arc<Mutex<Remote>>,
    remote_task: tokio::task::JoinHandle<()>,
    http: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl Fixture {
    async fn new(browser: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let workspace = directory.path().join("workspace");
        let console = directory.path().join("console");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&console).unwrap();
        fs::write(console.join("index.html"), "fixture").unwrap();
        let console = if browser {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../console/dist")
                .canonicalize()
                .unwrap()
        } else {
            console
        };
        let core = Core::persistent(
            ModelConfig {
                api_key: None,
                base_url: String::from("http://127.0.0.1:1/v1"),
                default_model: String::from("fixture"),
            },
            &directory.path().join("data/threads.sqlite3"),
        )
        .unwrap();
        core.write_ui_language("en").unwrap();
        let credentials = Arc::new(Credentials::default());
        let mut server = AppServer::new_desktop_with_stores_and_workspace(
            core,
            &console,
            String::from("oauth-fixture-shutdown"),
            credentials.clone(),
            &directory.path().join("data"),
            &workspace,
        )
        .unwrap();
        let remote = Arc::new(Mutex::new(Remote::default()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let remote_base = format!("http://{}", listener.local_addr().unwrap());
        let inner = Arc::get_mut(&mut server.inner).unwrap();
        inner
            .allowed_origins
            .push(String::from("https://ui.example.test"));
        inner.desktop_provider_oauth.authorize_url = format!("{remote_base}/auth");
        inner.desktop_provider_oauth.exchange_url = format!("{remote_base}/api/v1/auth/keys");
        let router = Router::new()
            .route("/auth", get(authorize))
            .route("/api/v1/auth/keys", post(keys))
            .route("/api/v1/models", get(models))
            .with_state(remote.clone());
        let remote_task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let _ = configure_provider(
            State(server.clone()),
            Path(String::from(PROVIDER)),
            Json(json!({"base_url":format!("{remote_base}/api/v1")})),
        )
        .await
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let http = tokio::spawn(server.clone().run_http(listener));
        Self {
            directory,
            server,
            base,
            credentials,
            remote,
            remote_task,
            http,
        }
    }

    async fn start(&self) -> Value {
        let response = reqwest::Client::new()
            .post(format!(
                "{}/api/providers/openrouter/oauth/start",
                self.base
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        response.json().await.unwrap()
    }

    async fn redirect(&self, start: &Value) -> String {
        let response = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap()
            .get(start["authorize_url"].as_str().unwrap())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        response.headers()["location"].to_str().unwrap().to_owned()
    }

    async fn status(&self, state: &str) -> Value {
        reqwest::get(format!(
            "{}/api/providers/openrouter/oauth/status?state={state}",
            self.base
        ))
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
    }

    async fn configure(&self, body: Value) {
        let _ = configure_provider(
            State(self.server.clone()),
            Path(String::from(PROVIDER)),
            Json(body),
        )
        .await
        .unwrap();
    }

    async fn shutdown(self) {
        self.server.inner.shutdown.cancel();
        tokio::time::timeout(Duration::from_secs(5), self.http)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        self.remote_task.abort();
    }
}

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_provider_oauth_browser_connects_discovers_and_reloads() {
    let fixture = Fixture::new(true).await;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = tokio::time::timeout(
        Duration::from_secs(90),
        tokio::process::Command::new("node")
            .arg(root.join("scripts/console_browser_smoke.mjs"))
            .args([&fixture.base, "/chat", "--provider-oauth"])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let report: Value =
        serde_json::from_str(&stdout).unwrap_or_else(|_| panic!("{stdout}\n{stderr}"));
    assert!(output.status.success(), "{report:#}\n{stderr}");
    assert_eq!(report["ok"], true);
    let expected = json!({"confirmation":true,"externalRedirect":true,"polling":true,"modelDiscovery":true,"modelAdded":true,"reload":true});
    assert_eq!(report["pages"][0]["providerOAuth"], expected);
    assert_eq!(fixture.remote.lock().unwrap().starts.len(), 1);
    assert_eq!(fixture.remote.lock().unwrap().exchanges.len(), 1);
    assert_eq!(
        fixture.credentials.values.lock().unwrap()["model-provider-api-key:openrouter"],
        "fixture-new-key"
    );
    assert_eq!(
        read_registry(&fixture.server).unwrap().providers[PROVIDER].extra_models[0].id,
        "fixture/model"
    );
    println!("{expected:#}");
    fixture.shutdown().await;
}

#[tokio::test]
async fn provider_oauth_pkce_callback_persists_key_discovers_models_and_survives_restart() {
    let fixture = Fixture::new(false).await;
    let provider = read_registry(&fixture.server).unwrap().providers[PROVIDER].clone();
    assert_eq!(
        provider_response(&provider).unwrap()["supports_oauth"],
        true
    );
    assert_eq!(
        provider_response(&provider).unwrap()["oauth_connected"],
        false
    );
    let begin = fixture.start().await;
    assert_eq!(begin.as_object().unwrap().len(), 3);
    assert_eq!(begin["flow_type"], "browser_redirect");
    let state = begin["state"].as_str().unwrap();
    assert_eq!(URL_SAFE_NO_PAD.decode(state).unwrap().len(), 32);
    assert_eq!(
        fixture.status(state).await,
        json!({"status":"pending","error":null})
    );
    let redirect = fixture.redirect(&begin).await;
    assert_eq!(
        url::Url::parse(&redirect)
            .unwrap()
            .query_pairs()
            .find(|(key, _)| key == "state")
            .unwrap()
            .1,
        state
    );
    let response = reqwest::get(&redirect).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["referrer-policy"], "no-referrer");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert!(
        response.headers()["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("script-src 'nonce-")
    );
    let html = response.text().await.unwrap();
    assert!(html.contains("oauth_complete"));
    assert!(!html.contains("fixture-new-key"));
    assert!(!html.contains(state));
    assert_eq!(
        fixture.status(state).await,
        json!({"status":"completed","error":null})
    );
    assert_eq!(
        *fixture.credentials.values.lock().unwrap(),
        BTreeMap::from([(
            String::from("model-provider-api-key:openrouter"),
            String::from("fixture-new-key")
        )])
    );
    let registry = read_registry(&fixture.server).unwrap();
    let provider = &registry.providers[PROVIDER];
    assert_eq!(
        provider_response(provider).unwrap()["oauth_connected"],
        true
    );
    assert_eq!(provider_response(provider).unwrap()["api_key"], "********");
    assert_eq!(
        provider
            .discovered_models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["fixture/model"]
    );
    let disk = fs::read_to_string(registry_path(
        fixture.server.inner.desktop_workspace.as_ref().unwrap(),
    ))
    .unwrap();
    assert!(!disk.contains("fixture-new-key"));
    let replay = reqwest::get(&redirect).await.unwrap();
    assert_eq!(replay.status(), StatusCode::BAD_REQUEST);
    assert_eq!(fixture.remote.lock().unwrap().exchanges.len(), 1);
    assert_eq!(
        fixture.remote.lock().unwrap().discovery_auth,
        vec![String::from("Bearer fixture-new-key")]
    );
    // Selecting the OAuth provider feeds the same key into the existing Core.
    let selected = reqwest::Client::new()
        .put(format!("{}/api/models/active", fixture.base))
        .json(&json!({"provider_id":"openrouter","model":"fixture/model","scope":"global"}))
        .send()
        .await
        .unwrap();
    assert_eq!(selected.status(), StatusCode::OK);
    assert_eq!(
        fixture.server.inner.core.backup_model_config().api_key,
        Some(String::from("fixture-new-key"))
    );
    fixture.remote.lock().unwrap().exchange_body = json!({"key":"fixture-reconnected-key"});
    let reconnect = fixture.start().await;
    assert_eq!(
        reqwest::get(fixture.redirect(&reconnect).await)
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        fixture.server.inner.core.backup_model_config().api_key,
        Some(String::from("fixture-reconnected-key"))
    );
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .read_config()
            .config
            .api_key_configured,
        true
    );
    let reopened = Core::persistent(
        ModelConfig {
            api_key: None,
            base_url: String::from("http://127.0.0.1:1/v1"),
            default_model: String::from("fixture"),
        },
        &fixture.directory.path().join("data/threads.sqlite3"),
    )
    .unwrap();
    initialize(
        &reopened,
        fixture.credentials.as_ref(),
        fixture.server.inner.desktop_workspace.as_ref().unwrap(),
    )
    .unwrap();
    assert_eq!(
        reopened.read_config(),
        fixture.server.inner.core.read_config()
    );
    assert_eq!(
        reopened.backup_model_config().api_key,
        Some(String::from("fixture-reconnected-key"))
    );
    fixture.configure(json!({"api_key":""})).await;
    assert_eq!(
        provider_response(&read_registry(&fixture.server).unwrap().providers[PROVIDER]).unwrap()["oauth_connected"],
        false
    );
    assert!(fixture.credentials.values.lock().unwrap().is_empty());
    fixture.shutdown().await;
}

#[tokio::test]
async fn provider_oauth_rejects_missing_wrong_expired_and_superseded_sessions() {
    let fixture = Fixture::new(false).await;
    let first = fixture.start().await;
    let first_url = fixture.redirect(&first).await;
    let second = fixture.start().await;
    assert_ne!(first["state"], second["state"]);
    assert_eq!(
        fixture.status(first["state"].as_str().unwrap()).await,
        json!({"status":"failed","error":"Superseded by a newer authorization"})
    );
    assert_eq!(
        reqwest::get(&first_url).await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
    for suffix in ["?code=unbound", "?state=wrong&code=unbound"] {
        assert_eq!(
            reqwest::get(format!(
                "{}/api/providers/openrouter/oauth/callback{suffix}",
                fixture.base
            ))
            .await
            .unwrap()
            .status(),
            StatusCode::BAD_REQUEST
        );
    }
    let state = second["state"].as_str().unwrap();
    let mismatch: Value = reqwest::get(format!(
        "{}/api/providers/dashscope/oauth/status?state={state}",
        fixture.base
    ))
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(
        mismatch,
        json!({"status":"failed","error":"Provider mismatch"})
    );
    fixture
        .server
        .inner
        .desktop_provider_oauth
        .sessions
        .lock()
        .await
        .get_mut(state)
        .unwrap()
        .created = Instant::now().checked_sub(TTL).unwrap();
    assert_eq!(
        fixture.status(state).await,
        json!({"status":"failed","error":"Session expired"})
    );
    assert!(fixture.remote.lock().unwrap().exchanges.is_empty());
    assert!(fixture.credentials.values.lock().unwrap().is_empty());
    fixture.shutdown().await;
}

#[tokio::test]
async fn provider_oauth_validates_origins_and_does_not_trust_forwarded_callback_headers() {
    let fixture = Fixture::new(false).await;
    for (host, origin, expected) in [
        (
            "ui.example.test",
            Some("https://ui.example.test"),
            StatusCode::OK,
        ),
        (
            "ui.example.test:443",
            Some("https://ui.example.test"),
            StatusCode::OK,
        ),
        ("evil.example.test", None, StatusCode::BAD_REQUEST),
        (
            "localhost",
            Some("https://evil.example.test"),
            StatusCode::FORBIDDEN,
        ),
        ("evil.example.test@localhost", None, StatusCode::BAD_REQUEST),
    ] {
        let mut request = reqwest::Client::new()
            .post(format!(
                "{}/api/providers/openrouter/oauth/start",
                fixture.base
            ))
            .header("host", host)
            .header("x-forwarded-proto", "https")
            .header(
                "x-qwenpaw-hub-oauth-callback-url",
                "https://attacker.example/callback",
            );
        if let Some(origin) = origin {
            request = request.header("origin", origin);
        }
        let response = request.send().await.unwrap();
        assert_eq!(response.status(), expected, "{host}");
        if expected == StatusCode::OK {
            let body: Value = response.json().await.unwrap();
            let authorize = url::Url::parse(body["authorize_url"].as_str().unwrap()).unwrap();
            let callback = authorize
                .query_pairs()
                .find(|(key, _)| key == "callback_url")
                .unwrap()
                .1
                .into_owned();
            assert!(callback.starts_with(
                "https://ui.example.test/api/providers/openrouter/oauth/callback?state="
            ));
        }
    }
    let unknown = reqwest::Client::new()
        .post(format!(
            "{}/api/providers/dashscope/oauth/start",
            fixture.base
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(unknown.status(), StatusCode::NOT_FOUND);
    fixture.shutdown().await;
}

#[tokio::test]
async fn provider_oauth_exchange_errors_and_failed_writes_preserve_existing_credentials() {
    let fixture = Fixture::new(false).await;
    fixture
        .configure(json!({"api_key":"fixture-old-key"}))
        .await;
    let before = serde_json::to_value(read_registry(&fixture.server).unwrap()).unwrap();
    for (status, body, expected) in [
        (
            StatusCode::FORBIDDEN,
            json!({"error":"fixture-diagnostic-secret"}),
            "OAuth exchange was rejected",
        ),
        (
            StatusCode::TEMPORARY_REDIRECT,
            json!({}),
            "OAuth exchange was rejected",
        ),
        (
            StatusCode::OK,
            json!({}),
            "OAuth exchange returned no credentials",
        ),
        (
            StatusCode::OK,
            json!({"key":"bad\nkey"}),
            "OAuth exchange returned invalid credentials",
        ),
        (
            StatusCode::OK,
            json!({"key":"x".repeat(EXCHANGE_LIMIT)}),
            "OAuth exchange response is too large",
        ),
    ] {
        {
            let mut remote = fixture.remote.lock().unwrap();
            remote.exchange_status = status;
            remote.exchange_body = body;
        }
        let begin = fixture.start().await;
        let response = reqwest::get(fixture.redirect(&begin).await).await.unwrap();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(
            !response
                .text()
                .await
                .unwrap()
                .contains("fixture-diagnostic-secret")
        );
        assert_eq!(
            fixture.status(begin["state"].as_str().unwrap()).await,
            json!({"status":"failed","error":expected})
        );
        assert_eq!(
            serde_json::to_value(read_registry(&fixture.server).unwrap()).unwrap(),
            before
        );
        assert_eq!(
            fixture.credentials.values.lock().unwrap()["model-provider-api-key:openrouter"],
            "fixture-old-key"
        );
    }
    {
        let mut remote = fixture.remote.lock().unwrap();
        remote.exchange_body = json!({"key":"fixture-new-key"});
    }
    let begin = fixture.start().await;
    fixture.credentials.fail_write.store(true, Ordering::SeqCst);
    assert_eq!(
        reqwest::get(fixture.redirect(&begin).await)
            .await
            .unwrap()
            .status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        fixture.status(begin["state"].as_str().unwrap()).await,
        json!({"status":"failed","error":"Provider credentials could not be saved"})
    );
    assert_eq!(
        serde_json::to_value(read_registry(&fixture.server).unwrap()).unwrap(),
        before
    );
    assert_eq!(
        fixture.credentials.values.lock().unwrap()["model-provider-api-key:openrouter"],
        "fixture-old-key"
    );
    assert!(fixture.remote.lock().unwrap().discovery_auth.is_empty());
    fixture.shutdown().await;
}

#[tokio::test]
async fn provider_oauth_late_exchange_cannot_override_manual_configuration_or_new_session() {
    for supersede in [false, true] {
        let fixture = Fixture::new(false).await;
        let started = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        fixture.remote.lock().unwrap().gate = Some((started.clone(), release.clone()));
        let begin = fixture.start().await;
        let url = fixture.redirect(&begin).await;
        let pending = tokio::spawn(async move { reqwest::get(url).await.unwrap() });
        tokio::time::timeout(Duration::from_secs(2), started.notified())
            .await
            .unwrap();
        if supersede {
            let _ = fixture.start().await;
        } else {
            fixture
                .configure(json!({"api_key":"fixture-manual-key"}))
                .await;
        }
        release.notify_one();
        let response = tokio::time::timeout(Duration::from_secs(2), pending)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            response.status(),
            if supersede {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::CONFLICT
            }
        );
        assert_eq!(
            fixture.status(begin["state"].as_str().unwrap()).await,
            json!({"status":"failed","error":if supersede {"Superseded by a newer authorization"} else {"Provider configuration changed during authorization"}})
        );
        assert_eq!(
            fixture
                .credentials
                .values
                .lock()
                .unwrap()
                .get("model-provider-api-key:openrouter")
                .cloned(),
            if supersede {
                None
            } else {
                Some(String::from("fixture-manual-key"))
            }
        );
        assert!(fixture.remote.lock().unwrap().discovery_auth.is_empty());
        fixture.shutdown().await;
    }
}

#[tokio::test]
async fn provider_oauth_discovery_failure_keeps_success_and_sessions_are_bounded() {
    let fixture = Fixture::new(false).await;
    fixture.remote.lock().unwrap().discovery_status = StatusCode::SERVICE_UNAVAILABLE;
    let begin = fixture.start().await;
    assert_eq!(
        reqwest::get(fixture.redirect(&begin).await)
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        fixture.status(begin["state"].as_str().unwrap()).await,
        json!({"status":"completed","error":null})
    );
    assert_eq!(
        fixture.credentials.values.lock().unwrap()["model-provider-api-key:openrouter"],
        "fixture-new-key"
    );
    for _ in 0..MAX_SESSIONS + 2 {
        let _ = fixture.start().await;
    }
    assert_eq!(
        fixture
            .server
            .inner
            .desktop_provider_oauth
            .sessions
            .lock()
            .await
            .len(),
        MAX_SESSIONS
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn provider_oauth_external_secret_change_rejects_an_old_callback() {
    let fixture = Fixture::new(false).await;
    let begin = fixture.start().await;
    let before = serde_json::to_value(read_registry(&fixture.server).unwrap()).unwrap();
    let credentials = BTreeMap::from([(
        String::from("model-provider-api-key:openrouter"),
        String::from("fixture-external-key"),
    )]);
    *fixture.credentials.values.lock().unwrap() = credentials.clone();
    let response = reqwest::get(fixture.redirect(&begin).await).await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        fixture.status(begin["state"].as_str().unwrap()).await,
        json!({"status":"failed","error":"Provider configuration changed during authorization"})
    );
    assert_eq!(
        serde_json::to_value(read_registry(&fixture.server).unwrap()).unwrap(),
        before
    );
    assert_eq!(*fixture.credentials.values.lock().unwrap(), credentials);
    assert!(fixture.remote.lock().unwrap().discovery_auth.is_empty());
    fixture.shutdown().await;
}

#[tokio::test]
async fn provider_oauth_wrong_origin_and_denial_never_exchange_or_save_credentials() {
    let fixture = Fixture::new(false).await;
    let begin = fixture.start().await;
    let state = begin["state"].as_str().unwrap();
    let redirect = fixture.redirect(&begin).await;
    let response = reqwest::Client::new()
        .get(&redirect)
        .header("host", "ui.example.test")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        fixture.status(state).await,
        json!({"status":"pending","error":null})
    );
    let mut denied = url::Url::parse(&redirect).unwrap();
    denied
        .query_pairs_mut()
        .append_pair("error", "<script>fixture-diagnostic-secret</script>");
    let response = reqwest::get(denied).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        !response
            .text()
            .await
            .unwrap()
            .contains("fixture-diagnostic-secret")
    );
    assert_eq!(
        fixture.status(state).await,
        json!({"status":"failed","error":"Authorization was denied"})
    );
    assert_eq!(
        reqwest::get(redirect).await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
    assert!(fixture.remote.lock().unwrap().exchanges.is_empty());
    assert!(fixture.credentials.values.lock().unwrap().is_empty());
    fixture.shutdown().await;
}
