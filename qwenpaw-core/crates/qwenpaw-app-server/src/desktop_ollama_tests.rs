use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use axum::body::Body;
use axum::http::Request;
use pretty_assertions::assert_eq;
use qwenpaw_core::ModelConfig;
use qwenpaw_protocol::ConfigWriteParams;
use qwenpaw_protocol::CoreEvent;
use qwenpaw_protocol::ThreadStartParams;
use qwenpaw_protocol::TurnStartParams;
use qwenpaw_protocol::TurnStatus;
use qwenpaw_protocol::UserInput;
use tower::ServiceExt as _;

use super::*;

#[derive(Default)]
struct Credentials;

impl DesktopCredentialStore for Credentials {
    fn load_api_key(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }

    fn save_api_key(&self, _: Option<&str>) -> anyhow::Result<()> {
        panic!("Ollama tests must not persist a placeholder credential")
    }

    fn load_agent_setting_secret(&self, _: &str) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
}

#[test]
fn ollama_addresses_preserve_proxy_prefix_and_do_not_change_other_providers() {
    for root in [
        "http://127.0.0.1:11434",
        "https://example.test/proxy/ollama",
    ] {
        for suffix in ["", "/", "///", "/v1", "/v1/"] {
            let input = format!(" {root}{suffix} ");
            assert_eq!(provider_stored_base_url("ollama", &input), root);
            assert_eq!(
                provider_api_base_url("ollama", &input),
                format!("{root}/v1")
            );
            assert_eq!(provider_api_base_url("openai", &input), input.trim());
        }
    }
}

#[derive(Default)]
struct Remote {
    paths: Vec<String>,
    chats: Vec<Value>,
    chat_headers: Vec<HeaderMap>,
    fail: bool,
}

async fn remote_request(
    State(remote): State<Arc<Mutex<Remote>>>,
    request: Request<Body>,
) -> axum::response::Response {
    use axum::response::IntoResponse as _;

    let path = request.uri().path().to_owned();
    let method = request.method().clone();
    let headers = request.headers().clone();
    let body = axum::body::to_bytes(request.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let mut remote = remote.lock().unwrap();
    remote.paths.push(format!("{method} {path}"));
    if remote.fail {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "fixture unavailable"})),
        )
            .into_response();
    }
    match (method.as_str(), path.as_str()) {
        ("GET", "/proxy/v1/models") => Json(json!({"data": [
            {"id": "fixture:latest", "object": "model", "owned_by": "library"}
        ]}))
        .into_response(),
        ("POST", "/proxy/v1/chat/completions") => {
            remote.chats.push(serde_json::from_slice(&body).unwrap());
            remote.chat_headers.push(headers);
            ([("content-type", "text/event-stream")], concat!(
                "data: {\"choices\":[{\"delta\":{\"content\":\"Ollama fixture reply\"},\"finish_reason\":null}]}\n\n",
                "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
                "data: [DONE]\n\n"
            )).into_response()
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn api(server: &AppServer, method: &str, path: &str, body: Value) -> Value {
    let response = router()
        .with_state(server.clone())
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    assert!(
        status.is_success(),
        "{path}: {status} {}",
        String::from_utf8_lossy(&bytes)
    );
    serde_json::from_slice(&bytes).unwrap()
}

fn new_core() -> Core {
    Core::new(ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1/v1"),
        default_model: String::from("fixture-initial"),
    })
}

async fn chat(core: &Core, workspace: &std::path::Path) {
    let thread = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(workspace.to_string_lossy().into_owned()),
        })
        .await
        .unwrap()
        .thread;
    let (_, mut events) = core
        .start_turn(TurnStartParams {
            thread_id: thread.id,
            input: vec![UserInput::Text {
                text: String::from("hello"),
            }],
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut text = String::new();
        while let Some(event) = events.recv().await {
            match event {
                CoreEvent::AgentMessageDelta(delta) => text.push_str(&delta.delta),
                CoreEvent::TurnCompleted(completed) => {
                    assert_eq!(completed.turn.status, TurnStatus::Completed);
                    assert_eq!(completed.turn.error, None);
                    assert_eq!(text, "Ollama fixture reply");
                    return;
                }
                _ => {}
            }
        }
        panic!("Model stream closed without completing the turn");
    })
    .await
    .unwrap();
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn ollama_api_selection_chat_startup_and_restore_use_the_compatible_endpoint() {
    let directory = tempfile::tempdir().unwrap();
    let console = directory.path().join("console");
    let workspace = directory.path().join("workspace");
    fs::create_dir_all(&console).unwrap();
    fs::create_dir_all(&workspace).unwrap();
    fs::write(console.join("index.html"), "fixture").unwrap();
    let remote = Arc::new(Mutex::new(Remote::default()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}/proxy", listener.local_addr().unwrap());
    let remote_router = Router::new()
        .fallback(remote_request)
        .with_state(remote.clone());
    let task = tokio::spawn(async move {
        axum::serve(listener, remote_router).await.unwrap();
    });
    let credentials = Arc::new(Credentials);
    let server = AppServer::new_desktop_with_stores_and_workspace(
        new_core(),
        &console,
        String::from("ollama-fixture-shutdown"),
        credentials.clone(),
        &directory.path().join("data"),
        &workspace,
    )
    .unwrap();

    let tested = api(
        &server,
        "POST",
        "/api/models/ollama/test",
        json!({"base_url": root}),
    )
    .await;
    assert_eq!(tested["success"], true);
    let configured = api(
        &server,
        "PUT",
        "/api/models/ollama/config",
        json!({"base_url": format!("{root}/v1/"),
            "custom_headers": {"x-fixture-secret": "fixture-private-header"},
            "generate_kwargs": {"temperature": 0.6, "top_p": 0.8, "max_tokens": 64,
                "extra_body": {"fixture_options": {"keep": true, "replace": "provider"}}}}),
    )
    .await;
    assert_eq!(configured["base_url"], root);
    assert_eq!(configured["api_key"], "");
    let discovered = api(
        &server,
        "POST",
        "/api/models/ollama/discover?save=true",
        json!({"base_url": format!("{root}/v1/")}),
    )
    .await;
    assert_eq!(discovered["success"], true);
    let registry = read_registry(&server).unwrap();
    assert_eq!(registry.providers["ollama"].base_url, root);
    assert_eq!(
        registry.providers["ollama"].discovered_models[0].id,
        "fixture:latest"
    );

    for (model, success, status) in [
        ("fixture:latest", true, "available"),
        ("missing:latest", false, "model_not_found"),
    ] {
        let checked = api(
            &server,
            "POST",
            "/api/models/ollama/models/test",
            json!({"model_id": model}),
        )
        .await;
        assert_eq!(
            json!({"success": checked["success"], "status": checked["status"], "verification": checked["verification"]}),
            json!({"success": success, "status": status, "verification": "provider_only"})
        );
    }
    assert_eq!(
        remote.lock().unwrap().paths,
        vec!["GET /proxy/v1/models"; 4]
    );
    api(
        &server,
        "PUT",
        "/api/models/active",
        json!({"provider_id": "ollama", "model": "fixture:latest", "scope": "global"}),
    )
    .await;
    chat(&server.inner.core, &workspace).await;
    api(
        &server,
        "PUT",
        "/api/models/ollama/models/fixture:latest/config",
        json!({"generate_kwargs": {"temperature": 0.2,
            "extra_body": {"fixture_options": {"replace": "model"}}}}),
    )
    .await;
    chat(&server.inner.core, &workspace).await;
    let before = serde_json::to_value(read_registry(&server).unwrap()).unwrap();
    assert!(
        configure_provider(
            State(server.clone()),
            Path(String::from("ollama")),
            Json(json!({"generate_kwargs": {"extra_body": {"stream": false}}}))
        )
        .await
        .is_err()
    );
    assert_eq!(
        serde_json::to_value(read_registry(&server).unwrap()).unwrap(),
        before
    );
    assert_eq!(
        backup_runtime_credential(&server).unwrap(),
        Some((String::from("ollama"), None))
    );
    server
        .inner
        .core
        .set_runtime_api_key(Some(String::from("fixture-private-proxy-key")))
        .unwrap();
    assert_eq!(
        backup_runtime_credential(&server).unwrap(),
        Some((
            String::from("ollama"),
            Some(String::from("fixture-private-proxy-key"))
        ))
    );
    server
        .inner
        .core
        .write_config(ConfigWriteParams {
            base_url: Some(String::from("http://127.0.0.1:1/v1")),
            default_model: None,
        })
        .unwrap();
    assert_eq!(
        backup_runtime_credential(&server),
        Err("Effective model credential cannot be matched to its provider")
    );

    let mut registry = read_registry(&server).unwrap();
    registry.providers.get_mut("ollama").unwrap().base_url = format!("{root}/v1/");
    write_registry(&server, &registry).unwrap();
    let old_bytes = serde_json::to_vec(&registry).unwrap();

    let restarted = new_core();
    initialize(
        &restarted,
        credentials.as_ref(),
        desktop_workspace(&server).unwrap(),
    )
    .unwrap();
    assert_eq!(
        restarted.read_config().config.base_url,
        format!("{root}/v1")
    );
    assert!(!restarted.read_config().config.api_key_configured);
    assert_eq!(
        read_registry(&server).unwrap().providers["ollama"].base_url,
        root
    );
    chat(&restarted, &workspace).await;
    let restored = new_core();
    let normalized = hydrate_restore(&restored, credentials.as_ref(), &old_bytes).unwrap();
    let restored_registry: ProviderRegistry = serde_json::from_slice(&normalized).unwrap();
    assert_eq!(restored_registry.providers["ollama"].base_url, root);
    assert_eq!(restored.read_config(), restarted.read_config());
    chat(&restored, &workspace).await;
    {
        let remote = remote.lock().unwrap();
        assert_eq!(remote.chats.len(), 4);
        for (index, request) in remote.chats.iter().enumerate() {
            assert_eq!(request["model"], "fixture:latest");
            assert_eq!(request["stream"], true);
            assert_eq!(request["temperature"], if index == 0 { 0.6 } else { 0.2 });
            assert_eq!(request["top_p"], 0.8);
            assert_eq!(request["max_tokens"], 64);
            assert_eq!(
                request["fixture_options"],
                json!({"keep": true,
                "replace": if index == 0 { "provider" } else { "model" }})
            );
            assert!(request.get("extra_body").is_none());
            assert_eq!(
                remote.chat_headers[index]["x-fixture-secret"],
                "fixture-private-header"
            );
        }
    }
    api(
        &server,
        "PUT",
        "/api/models/ollama/config",
        json!({"custom_headers": {}, "generate_kwargs": {}}),
    )
    .await;
    api(
        &server,
        "PUT",
        "/api/models/ollama/models/fixture:latest/config",
        json!({"generate_kwargs": {}}),
    )
    .await;
    chat(&server.inner.core, &workspace).await;
    {
        let mut remote = remote.lock().unwrap();
        let request = &remote.chats[4];
        for key in ["temperature", "top_p", "max_tokens", "fixture_options"] {
            assert!(request.get(key).is_none(), "{request}");
        }
        assert!(!remote.chat_headers[4].contains_key("x-fixture-secret"));
        remote.fail = true;
    }
    let checked = api(
        &server,
        "POST",
        "/api/models/ollama/models/test",
        json!({"model_id": "fixture:latest"}),
    )
    .await;
    assert_eq!(
        json!({"success": checked["success"], "http_status": checked["http_status"], "retryable": checked["retryable"], "verification": checked["verification"]}),
        json!({"success": false, "http_status": null, "retryable": false, "verification": "provider_only"})
    );
    assert_eq!(checked["status"], "model_not_found");
    let provider_check = api(&server, "POST", "/api/models/ollama/test", json!({})).await;
    assert_eq!(provider_check["success"], false);
    assert_eq!(provider_check["http_status"], 503);
    assert_eq!(provider_check["retryable"], true);
    assert_eq!(remote.lock().unwrap().chats.len(), 5);
    task.abort();
}
