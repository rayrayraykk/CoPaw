use std::convert::Infallible;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::response::IntoResponse as _;
use pretty_assertions::assert_eq;
use qwenpaw_protocol::{CoreEvent, ThreadStartParams, TurnStartParams, TurnStatus, UserInput};

use super::anthropic_tests::{Credentials, api, config};
use super::*;

#[derive(Default)]
struct Remote {
    requests: Vec<(HeaderMap, Value)>,
    probes: Vec<(HeaderMap, Value)>,
    catalogs: Vec<(HeaderMap, BTreeMap<String, String>)>,
    media_replies: BTreeMap<String, (StatusCode, Value)>,
    fail: bool,
    hang: bool,
    repeat_cursor: bool,
}

async fn catalog(
    State(remote): State<Arc<Mutex<Remote>>>,
    Query(query): Query<BTreeMap<String, String>>,
    headers: HeaderMap,
) -> Json<Value> {
    let mut remote = remote.lock().unwrap();
    remote.catalogs.push((headers, query.clone()));
    if query.contains_key("pageToken") && !remote.repeat_cursor {
        Json(
            json!({"models": [{"name": "models/fixture-gemini", "displayName": "Duplicate"},
            {"name": "models/fixture-other", "displayName": "Fixture Other", "inputTokenLimit": 4096, "outputTokenLimit": 1024}]}),
        )
    } else {
        Json(
            json!({"models": [{"name": "models/fixture-gemini", "displayName": "Fixture Gemini", "inputTokenLimit": 8192, "outputTokenLimit": 2048}], "nextPageToken": "fixture cursor+/="}),
        )
    }
}

async fn generate(
    State(remote): State<Arc<Mutex<Remote>>>,
    Path(resource): Path<String>,
    Query(query): Query<BTreeMap<String, String>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> axum::response::Response {
    assert!(!query.contains_key("key"));
    if resource.ends_with(":generateContent") {
        let kind = if body["contents"][0]["parts"][0].get("inlineData").is_some() {
            "image"
        } else if body["contents"][0]["parts"][0].get("fileData").is_some() {
            "video"
        } else {
            "connection"
        };
        let mut remote = remote.lock().unwrap();
        remote.probes.push((headers, body));
        if let Some((status, reply)) = remote.media_replies.get(kind) {
            return if let Value::String(text) = reply {
                (*status, text.clone()).into_response()
            } else {
                (*status, Json(reply.clone())).into_response()
            };
        }
        let text = match kind {
            "image" => "red",
            "video" => "yes",
            _ => "OK",
        };
        return Json(json!({"candidates": [{"content": {"role": "model", "parts": [{"text": text}]}, "finishReason": "STOP"}]})).into_response();
    }
    assert_eq!(resource, "fixture-gemini:streamGenerateContent");
    assert_eq!(
        query,
        BTreeMap::from([(String::from("alt"), String::from("sse"))])
    );
    let (events, hang) = {
        let mut remote = remote.lock().unwrap();
        remote.requests.push((headers, body.clone()));
        let events = if remote.fail {
            vec![
                json!({"candidates": [{"content": {"parts": [{"functionCall": {"name": "read_file", "args": {"path": "fixture.txt"}}, "thoughtSignature": "incomplete-signature"}]}}]}),
                json!({"error": {"message": "gemini-fixture-private-key"}}),
            ]
        } else if remote.hang {
            vec![json!({"candidates": [{"content": {"parts": [{"text": "pending"}]}}]})]
        } else {
            let contents = body["contents"].as_array().unwrap();
            let result = contents.last().unwrap()["parts"]
                .as_array()
                .unwrap()
                .iter()
                .find_map(|part| part.get("functionResponse"));
            let parts = if let Some(result) = result {
                assert_eq!(
                    result,
                    &json!({"name": "read_file", "response": {"output": "fixture content"}})
                );
                assert_eq!(
                    contents[contents.len() - 2]["parts"],
                    json!([
                    {"text": "Read the fixture.", "thought": true},
                    {"functionCall": {"name": "read_file", "args": {"path": "fixture.txt"}}, "thoughtSignature": "fixture-signed-call"}])
                );
                json!([{"text": "原生 Gemini 回复"}, {"text": "", "thoughtSignature": "fixture-signed-final"}])
            } else {
                json!([{"text": "Read the fixture.", "thought": true},
                    {"functionCall": {"name": "read_file", "args": {"path": "fixture.txt"}}, "thoughtSignature": "fixture-signed-call"}])
            };
            vec![
                json!({"candidates": [{"index": 0, "content": {"role": "model", "parts": parts}, "finishReason": "STOP"}]}),
                json!({"usageMetadata": {"promptTokenCount": 10, "candidatesTokenCount": 3, "thoughtsTokenCount": 5,
                    "cachedContentTokenCount": 4, "totalTokenCount": 18}}),
            ]
        };
        (events, remote.hang)
    };
    let mut payload = String::new();
    for event in events {
        writeln!(payload, "data: {event}\n").unwrap();
    }
    let chunks = payload
        .as_bytes()
        .chunks(7)
        .map(|chunk| Ok::<_, Infallible>(Bytes::copy_from_slice(chunk)))
        .collect::<Vec<_>>();
    let tail = if hang {
        futures_util::stream::pending().boxed()
    } else {
        futures_util::stream::empty().boxed()
    };
    (
        [("content-type", "text/event-stream")],
        Body::from_stream(futures_util::stream::iter(chunks).chain(tail)),
    )
        .into_response()
}

#[path = "desktop_gemini_probe_tests.rs"]
mod probe_tests;

#[path = "desktop_image_tests.rs"]
mod image_tests;

struct Fixture {
    directory: tempfile::TempDir,
    server: AppServer,
    remote: Arc<Mutex<Remote>>,
    credentials: Arc<Credentials>,
    base: String,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Fixture {
    async fn new(browser: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let workspace = directory.path().join("workspace");
        let console = directory.path().join("console");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&console).unwrap();
        fs::write(workspace.join("fixture.txt"), "fixture content").unwrap();
        fs::write(console.join("index.html"), "fixture").unwrap();
        let console = if browser {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../console/dist")
                .canonicalize()
                .unwrap()
        } else {
            console
        };
        let remote = Arc::new(Mutex::new(Remote::default()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/proxy", listener.local_addr().unwrap());
        let router = Router::new()
            .route("/proxy/v1beta/models", get(catalog))
            .route("/proxy/v1beta/models/{resource}", post(generate))
            .with_state(remote.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let credentials = Arc::new(Credentials::default());
        let server = AppServer::new_desktop_with_stores_and_workspace(
            Core::persistent(config(), &directory.path().join("threads.sqlite3")).unwrap(),
            &console,
            String::from("gemini-fixture-shutdown"),
            credentials.clone(),
            &directory.path().join("data"),
            &workspace,
        )
        .unwrap();
        server.inner.core.write_ui_language("en").unwrap();
        // The original built-in Gemini URL is frozen. Only the isolated backend
        // endpoint is seeded; browser mutations still use unchanged UI controls.
        let mut registry = read_registry(&server).unwrap();
        let provider = registry.providers.get_mut("gemini").unwrap();
        assert!(provider.freeze_url);
        provider.base_url.clone_from(&base);
        write_registry(&server, &registry).unwrap();
        Self {
            directory,
            server,
            remote,
            credentials,
            base,
            task,
        }
    }

    async fn configure(&self) {
        api(&self.server, "PUT", "/api/models/gemini/config", json!({"api_key": "gemini-fixture-private-key",
            "custom_headers": {"x-fixture": "native"}, "generate_kwargs": {"max_tokens": 1024, "thinking_enable": true}})).await;
        api(
            &self.server,
            "POST",
            "/api/models/gemini/models",
            json!({"id": "fixture-gemini", "name": "Fixture Gemini"}),
        )
        .await;
        api(
            &self.server,
            "PUT",
            "/api/models/gemini/models/fixture-gemini/config",
            json!({"generate_kwargs": {"max_output_tokens": 2048}}),
        )
        .await;
        api(
            &self.server,
            "PUT",
            "/api/models/active",
            json!({"provider_id": "gemini", "model": "fixture-gemini", "scope": "global"}),
        )
        .await;
    }
}

async fn turn(core: &Core, thread: &str, expected: TurnStatus) {
    let (_, mut events) = core
        .start_turn(TurnStartParams {
            thread_id: thread.to_owned(),
            input: vec![UserInput::Text {
                text: String::from("Read fixture.txt"),
            }],
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut text = String::new();
        let mut tools = 0;
        while let Some(event) = events.recv().await {
            match event {
                CoreEvent::AgentMessageDelta(delta) => text.push_str(&delta.delta),
                CoreEvent::ItemCompleted(item)
                    if matches!(item.item, qwenpaw_protocol::Item::ToolResult { .. }) =>
                {
                    tools += 1;
                }
                CoreEvent::TurnCompleted(completed) => {
                    assert_eq!(
                        completed.turn.status, expected,
                        "{:?}",
                        completed.turn.error
                    );
                    if expected == TurnStatus::Completed {
                        assert_eq!((text.as_str(), tools), ("原生 Gemini 回复", 1));
                    } else {
                        assert!(
                            !format!("{:?}", completed.turn.error)
                                .contains("gemini-fixture-private-key")
                        );
                        assert_eq!(tools, 0);
                    }
                    return;
                }
                _ => {}
            }
        }
        panic!("stream ended without completion");
    })
    .await
    .unwrap();
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn native_selection_tool_roundtrip_restart_restore_parameters_error_and_cancel() {
    let fixture = Fixture::new(false).await;
    fixture.configure().await;
    let core = &fixture.server.inner.core;
    let thread = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(
                fixture
                    .directory
                    .path()
                    .join("workspace")
                    .to_string_lossy()
                    .into_owned(),
            ),
        })
        .await
        .unwrap()
        .thread;
    turn(core, &thread.id, TurnStatus::Completed).await;
    let restarted =
        Core::persistent(config(), &fixture.directory.path().join("threads.sqlite3")).unwrap();
    initialize(
        &restarted,
        fixture.credentials.as_ref(),
        desktop_workspace(&fixture.server).unwrap(),
    )
    .unwrap();
    turn(&restarted, &thread.id, TurnStatus::Completed).await;
    let snapshot = restarted.backup_snapshot(4 * 1024 * 1024).unwrap();
    let restored = restarted.prepare_restore(&snapshot).unwrap();
    hydrate_restore(
        &restored,
        fixture.credentials.as_ref(),
        &serde_json::to_vec(&read_registry(&fixture.server).unwrap()).unwrap(),
    )
    .unwrap();
    turn(&restored, &thread.id, TurnStatus::Completed).await;
    let snapshot = restored.backup_snapshot(4 * 1024 * 1024).unwrap();
    assert_eq!(snapshot.usage.len(), 6);
    for usage in snapshot.usage {
        assert_eq!(
            usage.call,
            qwenpaw_storage::StoredModelCall {
                provider_id: String::from("gemini"),
                model: String::from("fixture-gemini"),
                prompt_tokens: 10,
                completion_tokens: 8,
                cache_read_tokens: 4,
                cache_write_tokens: 0,
                cache_eligible_input_tokens: 10,
                cache_observed: true,
                usage_observed: true
            }
        );
    }
    {
        let remote = fixture.remote.lock().unwrap();
        assert_eq!(remote.requests.len(), 6);
        for (headers, body) in &remote.requests {
            assert_eq!(headers["x-goog-api-key"], "gemini-fixture-private-key");
            assert_eq!(headers["x-fixture"], "native");
            assert!(!headers.contains_key("authorization"));
            assert_eq!(
                body["generationConfig"],
                json!({"maxOutputTokens": 2048, "thinkingConfig": {"includeThoughts": true, "thinkingBudget": 1024}})
            );
            assert!(body["systemInstruction"]["parts"].is_array());
            assert!(
                body["tools"][0]["functionDeclarations"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|tool| tool["name"] == "read_file")
            );
            for key in ["messages", "model", "stream", "stream_options"] {
                assert!(body.get(key).is_none());
            }
        }
    }
    fixture.remote.lock().unwrap().fail = true;
    turn(&restored, &thread.id, TurnStatus::Failed).await;
    {
        let mut remote = fixture.remote.lock().unwrap();
        remote.fail = false;
        remote.hang = true;
    }
    let (started, mut events) = restored
        .start_turn(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![UserInput::Text {
                text: String::from("wait"),
            }],
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(event) = events.recv().await {
            if matches!(event, CoreEvent::AgentMessageDelta(_)) {
                return;
            }
        }
        panic!("missing streamed text");
    })
    .await
    .unwrap();
    restored
        .interrupt_turn(&qwenpaw_protocol::TurnInterruptParams {
            thread_id: thread.id,
            turn_id: started.turn.id,
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(event) = events.recv().await {
            if let CoreEvent::TurnCompleted(completed) = event {
                assert_eq!(completed.turn.status, TurnStatus::Interrupted);
                return;
            }
        }
        panic!("missing interruption completion");
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn native_catalog_pagination_connection_and_model_checks() {
    let fixture = Fixture::new(false).await;
    fixture.configure().await;
    let connected = api(
        &fixture.server,
        "POST",
        "/api/models/gemini/test",
        json!({}),
    )
    .await;
    assert_eq!(connected["success"], true, "{connected:#}");
    let checked = api(
        &fixture.server,
        "POST",
        "/api/models/gemini/models/test",
        json!({"model_id": "models/fixture-gemini"}),
    )
    .await;
    assert_eq!(checked["success"], true, "{checked:#}");
    let remote = desktop_model_remote::RemoteProvider {
        base_url: fixture.base.clone(),
        chat_model: String::from("GeminiChatModel"),
        custom_headers: vec![],
        auth_mode: String::from("api_key"),
        secret: Some(String::from("gemini-fixture-private-key")),
    };
    assert_eq!(
        desktop_model_remote::discover_models(&remote)
            .await
            .unwrap(),
        vec![
            desktop_model_remote::DiscoveredModel {
                id: String::from("fixture-gemini"),
                name: String::from("Fixture Gemini"),
                max_input_length: Some(8192),
                max_output_length: Some(2048)
            },
            desktop_model_remote::DiscoveredModel {
                id: String::from("fixture-other"),
                name: String::from("Fixture Other"),
                max_input_length: Some(4096),
                max_output_length: Some(1024)
            }
        ]
    );
    {
        let mut remote = fixture.remote.lock().unwrap();
        assert_eq!(remote.probes.len(), 1);
        assert_eq!(
            remote.probes[0].1,
            json!({"contents": [{"role": "user", "parts": [{"text": "ping"}]}], "generationConfig": {"maxOutputTokens": 20}})
        );
        for (headers, query) in &remote.catalogs {
            assert_eq!(headers["x-goog-api-key"], "gemini-fixture-private-key");
            assert!(!query.contains_key("key"));
        }
        assert_eq!(
            remote.catalogs.last().unwrap().1,
            BTreeMap::from([(String::from("pageToken"), String::from("fixture cursor+/="))])
        );
        remote.repeat_cursor = true;
    }
    assert!(
        desktop_model_remote::discover_models(&remote)
            .await
            .unwrap_err()
            .message
            .contains("repeated its cursor")
    );
}

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_gemini_browser_configures_selects_chats_and_reloads() {
    use base64::Engine as _;
    let fixture = Fixture::new(true).await;
    let image_data = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";
    let image_path = fixture.directory.path().join("browser-red.png");
    fs::write(
        &image_path,
        base64::engine::general_purpose::STANDARD
            .decode(image_data)
            .unwrap(),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let http = tokio::spawn(fixture.server.clone().run_http(listener));
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = tokio::time::timeout(
        Duration::from_secs(100),
        tokio::process::Command::new("node")
            .arg(root.join("scripts/console_browser_smoke.mjs"))
            .args([&origin, "/models", "--gemini-chat"])
            .env("QWENPAW_GEMINI_FIXTURE_URL", &fixture.base)
            .env("QWENPAW_IMAGE_FIXTURE_PATH", &image_path)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), http)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let report: Value =
        serde_json::from_str(&stdout).unwrap_or_else(|_| panic!("{stdout}\n{stderr}"));
    assert!(output.status.success(), "{report:#}\n{stderr}");
    assert_eq!(report["ok"], true);
    assert_eq!(
        report["pages"][0]["geminiChat"],
        json!({"configured": true, "modelAdded": true, "agentSelected": true, "reply": true, "tool": true, "reload": true, "multimodal": true, "image": true})
    );
    let remote = fixture.remote.lock().unwrap();
    assert!(!remote.probes.is_empty());
    assert_eq!(
        remote
            .probes
            .iter()
            .filter(|(_, body)| body["contents"][0]["parts"][0].get("inlineData").is_some())
            .count(),
        1
    );
    assert_eq!(
        remote
            .probes
            .iter()
            .filter(|(_, body)| body["contents"][0]["parts"][0].get("fileData").is_some())
            .count(),
        1
    );
    assert_eq!(remote.requests.len(), 2);
    for (headers, body) in &remote.requests {
        assert_eq!(headers["x-goog-api-key"], "gemini-fixture-private-key");
        let parts = body["contents"][0]["parts"].as_array().unwrap();
        assert_eq!(
            parts
                .iter()
                .filter_map(|part| part.get("inlineData"))
                .collect::<Vec<_>>(),
            vec![&json!({"mimeType": "image/png", "data": image_data})]
        );
    }
    assert_eq!(
        read_registry(&fixture.server).unwrap().active_provider_id,
        DEFAULT_PROVIDER_ID
    );
}
