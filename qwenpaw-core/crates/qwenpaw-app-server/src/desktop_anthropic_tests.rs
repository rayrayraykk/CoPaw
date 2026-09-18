use std::convert::Infallible;
use std::fmt::Write as _;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use axum::body::Body;
use axum::http::Request;
use axum::response::IntoResponse as _;
use pretty_assertions::assert_eq;
use qwenpaw_protocol::CoreEvent;
use qwenpaw_protocol::ThreadStartParams;
use qwenpaw_protocol::TurnStartParams;
use qwenpaw_protocol::TurnStatus;
use qwenpaw_protocol::UserInput;
use tower::ServiceExt as _;

use super::*;

#[path = "desktop_usage_workspace_tests.rs"]
mod usage_workspace;

#[derive(Default)]
pub(super) struct Credentials(Mutex<BTreeMap<String, String>>);

impl DesktopCredentialStore for Credentials {
    fn load_api_key(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
    fn save_api_key(&self, _: Option<&str>) -> anyhow::Result<()> {
        panic!("unused default key")
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

#[derive(Default)]
struct Gate {
    started: tokio::sync::Notify,
    release: tokio::sync::Notify,
}

#[derive(Default)]
struct Remote {
    requests: Vec<(HeaderMap, Value)>,
    probes: Vec<(HeaderMap, Value)>,
    hang: bool,
    fail: bool,
    gate: Option<Arc<Gate>>,
}

fn start_event() -> Value {
    json!({"type": "message_start", "message": {"role": "assistant", "content": [], "usage": {
        "input_tokens": 10, "cache_read_input_tokens": 20, "cache_creation_input_tokens": 5, "output_tokens": 1}}})
}

fn reply(tool: bool, id: &str) -> Vec<Value> {
    let mut events = vec![start_event()];
    if tool {
        events.extend([
            json!({"type": "content_block_start", "index": 0, "content_block": {"type": "thinking", "thinking": "", "signature": ""}}),
            json!({"type": "content_block_delta", "index": 0, "delta": {"type": "thinking_delta", "thinking": "Read the fixture."}}),
            json!({"type": "content_block_delta", "index": 0, "delta": {"type": "signature_delta", "signature": "fixture-signature"}}),
            json!({"type": "content_block_stop", "index": 0}),
            json!({"type": "content_block_start", "index": 1, "content_block": {"type": "tool_use", "id": id, "name": "read_file", "input": {}}}),
            json!({"type": "content_block_delta", "index": 1, "delta": {"type": "input_json_delta", "partial_json": "{\"path\":"}}),
            json!({"type": "ping"}),
            json!({"type": "content_block_delta", "index": 1, "delta": {"type": "input_json_delta", "partial_json": "\"fixture.txt\"}"}}),
            json!({"type": "content_block_stop", "index": 1}),
        ]);
    } else {
        events.extend([
            json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}),
            json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "原生 Anthropic 回复"}}),
            json!({"type": "content_block_stop", "index": 0}),
        ]);
    }
    events.extend([
        json!({"type": "message_delta", "delta": {"stop_reason": if tool { "tool_use" } else { "end_turn" }}, "usage": {"output_tokens": 3}}),
        json!({"type": "message_delta", "usage": {"output_tokens": 7}}),
        json!({"type": "message_stop"}),
    ]);
    events
}

async fn messages(
    State(remote): State<Arc<Mutex<Remote>>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> axum::response::Response {
    if body["stream"] != true {
        remote.lock().unwrap().probes.push((headers, body));
        return Json(
            json!({"id": "probe", "type": "message", "role": "assistant",
            "content": [{"type": "text", "text": "OK"}], "stop_reason": "end_turn",
            "usage": {"input_tokens": 1, "output_tokens": 1}}),
        )
        .into_response();
    }
    let (events, hang, gate) = {
        let mut remote = remote.lock().unwrap();
        let index = remote.requests.len();
        remote.requests.push((headers, body.clone()));
        let events = if remote.fail {
            vec![
                start_event(),
                json!({"type": "error", "error": {"message": "fixture-private-key"}}),
            ]
        } else if remote.hang {
            vec![
                start_event(),
                json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}),
                json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "pending"}}),
            ]
        } else {
            let content = body["messages"].as_array().unwrap().last().unwrap()["content"]
                .as_array()
                .unwrap();
            let tool_result = content.iter().find(|block| block["type"] == "tool_result");
            if let Some(result) = tool_result {
                assert_eq!(result["content"], "fixture content");
                let previous =
                    &body["messages"][body["messages"].as_array().unwrap().len() - 2]["content"];
                assert_eq!(
                    previous[0],
                    json!({"type": "thinking", "thinking": "Read the fixture.", "signature": "fixture-signature"})
                );
                assert_eq!(previous[1]["id"], result["tool_use_id"]);
            }
            reply(tool_result.is_none(), &format!("fixture-call-{index}"))
        };
        (events, remote.hang, remote.gate.take())
    };
    if let Some(gate) = gate {
        gate.started.notify_one();
        gate.release.notified().await;
    }
    let mut payload = String::new();
    for event in events {
        writeln!(
            payload,
            "event: {}\ndata: {event}\n",
            event["type"].as_str().unwrap()
        )
        .unwrap();
    }
    let chunks = payload
        .as_bytes()
        .chunks(11)
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

pub(super) async fn api(server: &AppServer, method: &str, path: &str, body: Value) -> Value {
    let response = router()
        .merge(super::super::desktop_agents::router())
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
        "{path}: {}",
        String::from_utf8_lossy(&bytes)
    );
    serde_json::from_slice(&bytes).unwrap()
}

pub(super) fn config() -> ModelConfig {
    ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1/v1"),
        default_model: String::from("initial-model"),
    }
}

async fn turn(core: &Core, thread_id: &str, expected: TurnStatus) {
    let (_, mut events) = core
        .start_turn(TurnStartParams {
            thread_id: thread_id.to_owned(),
            input: vec![UserInput::Text {
                text: String::from("read the fixture"),
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
                        assert_eq!(text, "原生 Anthropic 回复");
                        assert_eq!(tools, 1);
                    } else {
                        assert!(
                            !format!("{:?}", completed.turn.error).contains("fixture-private-key")
                        );
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

struct Fixture {
    directory: tempfile::TempDir,
    server: AppServer,
    remote: Arc<Mutex<Remote>>,
    base: String,
    credentials: Arc<Credentials>,
    task: tokio::task::JoinHandle<()>,
}

impl Fixture {
    async fn new(browser: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let console = directory.path().join("console");
        let workspace = directory.path().join("workspace");
        fs::create_dir_all(&console).unwrap();
        fs::create_dir_all(&workspace).unwrap();
        fs::write(console.join("index.html"), "fixture").unwrap();
        let console = if browser {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../console/dist")
                .canonicalize()
                .unwrap()
        } else {
            console
        };
        fs::write(workspace.join("fixture.txt"), "fixture content").unwrap();
        let database = directory.path().join("threads.sqlite3");
        let remote = Arc::new(Mutex::new(Remote::default()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/proxy", listener.local_addr().unwrap());
        let remote_router = Router::new()
        .route("/proxy/v1/messages", post(messages))
        .route("/proxy/v1/models", get(|| async {
            Json(json!({"data": [{"id": "fixture-claude", "display_name": "Fixture Claude"}]}))
        }))
        .with_state(remote.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, remote_router).await.unwrap();
        });
        let credentials = Arc::new(Credentials::default());
        let server = AppServer::new_desktop_with_stores_and_workspace(
            Core::persistent(config(), &database).unwrap(),
            &console,
            String::from("native-fixture-shutdown"),
            credentials.clone(),
            &directory.path().join("data"),
            &workspace,
        )
        .unwrap();
        server.inner.core.write_ui_language("en").unwrap();
        Self {
            directory,
            server,
            remote,
            base,
            credentials,
            task,
        }
    }
}

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_anthropic_browser_configures_selects_chats_and_reloads() {
    let fixture = Fixture::new(true).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let http = tokio::spawn(fixture.server.clone().run_http(listener));
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = tokio::time::timeout(
        Duration::from_secs(100),
        tokio::process::Command::new("node")
            .arg(root.join("scripts/console_browser_smoke.mjs"))
            .args([&origin, "/models", "--anthropic-chat"])
            .env("QWENPAW_ANTHROPIC_FIXTURE_URL", &fixture.base)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    eprintln!("{stderr}");
    let report: Value =
        serde_json::from_str(&stdout).unwrap_or_else(|_| panic!("{stdout}\n{stderr}"));
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), http)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    fixture.task.abort();
    assert!(output.status.success(), "{report:#}\n{stderr}");
    assert_eq!(report["ok"], true);
    assert_eq!(
        report["pages"][0]["anthropicChat"],
        json!({
            "configured": true, "modelAdded": true, "agentSelected": true,
            "reply": true, "tool": true, "reload": true
        })
    );
    let remote = fixture.remote.lock().unwrap();
    assert!(!remote.probes.is_empty());
    assert_eq!(remote.requests.len(), 2);
    for (headers, body) in &remote.requests {
        assert_eq!(headers["x-api-key"], "sk-ant-fixture-private-key");
        assert_eq!(body["model"], "fixture-claude");
    }
    assert_eq!(
        read_registry(&fixture.server).unwrap().active_provider_id,
        DEFAULT_PROVIDER_ID
    );
}

async fn console_turn(fixture: &Fixture, agent: &str, session: &str) -> Vec<Value> {
    let response = super::super::desktop_api::router().with_state(fixture.server.clone())
        .oneshot(Request::builder().method("POST").uri("/api/console/chat")
            .header("content-type", "application/json").header("x-agent-id", agent)
            .body(Body::from(json!({"session_id": session,
                "request_context": {"session_project_dirs": [{"path": fixture.directory.path().join("workspace")}]},
                "input": [{"role": "user", "content": "Read the fixture"}]}).to_string())).unwrap())
        .await.unwrap();
    let status = response.status();
    let bytes = tokio::time::timeout(
        Duration::from_secs(10),
        axum::body::to_bytes(response.into_body(), 1024 * 1024),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    let events: Vec<Value> = std::str::from_utf8(&bytes)
        .unwrap()
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .map(|data| serde_json::from_str(data).unwrap())
        .collect();
    assert_eq!(events.last().unwrap()["status"], "completed", "{events:#?}");
    assert!(
        events
            .iter()
            .any(|event| event["type"] == "plugin_call_output"
                && event["content"][0]["data"]["output"] == "fixture content")
    );
    events
}

#[tokio::test]
async fn console_agent_model_routes_isolated_turns_and_keeps_selection_after_catalog_removal() {
    let fixture = Fixture::new(false).await;
    api(&fixture.server, "POST", "/api/models/custom-providers", json!({
        "id": "agent-native", "name": "Agent Native", "chat_model": "AnthropicChatModel",
        "default_base_url": fixture.base, "models": [{"id": "agent-claude", "name": "Agent Claude"}]
    })).await;
    for (provider, key) in [("anthropic", "global-key"), ("agent-native", "agent-key")] {
        api(
            &fixture.server,
            "PUT",
            &format!("/api/models/{provider}/config"),
            json!({
                "base_url": fixture.base, "api_key": key,
                "custom_headers": {"x-fixture": key}, "generate_kwargs": {"max_tokens": 2048}
            }),
        )
        .await;
    }
    api(
        &fixture.server,
        "POST",
        "/api/models/anthropic/models",
        json!({"id": "global-claude", "name": "Global Claude"}),
    )
    .await;
    api(
        &fixture.server,
        "PUT",
        "/api/models/active",
        json!({
            "provider_id": "anthropic", "model": "global-claude", "scope": "global"
        }),
    )
    .await;
    api(&fixture.server, "PUT", "/api/models/active", json!({
        "provider_id": "agent-native", "model": "agent-claude", "scope": "agent", "agent_id": "default"
    })).await;
    api(
        &fixture.server,
        "POST",
        "/api/agents",
        json!({"id": "fallback", "name": "Fallback"}),
    )
    .await;
    let before = fixture.server.inner.core.read_config();
    let (selected, fallback) = tokio::join!(
        console_turn(&fixture, "default", "1700000000000-selected"),
        console_turn(&fixture, "fallback", "1700000000000-fallback")
    );
    assert_ne!(selected[0]["id"], fallback[0]["id"]);
    assert_eq!(fixture.server.inner.core.read_config(), before);
    {
        let remote = fixture.remote.lock().unwrap();
        assert_eq!(remote.requests.len(), 4);
        let mut counts = BTreeMap::new();
        for (headers, body) in &remote.requests {
            let model = body["model"].as_str().unwrap();
            let expected_key = match model {
                "agent-claude" => "agent-key",
                "global-claude" => "global-key",
                _ => panic!("unexpected model {model}"),
            };
            assert_eq!(headers["x-api-key"], expected_key);
            assert_eq!(headers["x-fixture"], expected_key);
            *counts.entry(model).or_insert(0) += 1;
        }
        assert_eq!(
            counts,
            BTreeMap::from([("agent-claude", 2), ("global-claude", 2)])
        );
    }
    api(
        &fixture.server,
        "DELETE",
        "/api/models/agent-native/models/agent-claude",
        json!({}),
    )
    .await;
    assert_removed_model_selection(&fixture).await;
    console_turn(&fixture, "default", "1700000000000-selected").await;
    let remote = fixture.remote.lock().unwrap();
    assert_eq!(remote.requests.len(), 6);
    for (headers, body) in &remote.requests[4..] {
        assert_eq!(body["model"], "agent-claude");
        assert_eq!(headers["x-api-key"], "agent-key");
        assert_eq!(headers["x-fixture"], "agent-key");
    }
    assert_eq!(fixture.server.inner.core.read_config(), before);
    let snapshot = fixture
        .server
        .inner
        .core
        .backup_snapshot(4 * 1024 * 1024)
        .unwrap();
    let serialized = serde_json::to_string(&snapshot).unwrap();
    assert!(!serialized.contains("agent-key"));
    assert!(!serialized.contains("global-key"));
    fixture.task.abort();
}

async fn assert_removed_model_selection(fixture: &Fixture) {
    assert_eq!(
        api(
            &fixture.server,
            "GET",
            "/api/models/active?scope=effective&agent_id=default",
            Value::Null
        )
        .await,
        json!({"active_llm":{"provider_id":"agent-native","model":"agent-claude"},"effective_max_input_length":200_000})
    );
}

#[tokio::test]
async fn console_global_fallback_uses_live_core_runtime_not_stale_provider_registry() {
    let fixture = Fixture::new(false).await;
    api(
        &fixture.server,
        "POST",
        "/api/agents",
        json!({"id": "live", "name": "Live"}),
    )
    .await;
    fixture
        .server
        .inner
        .core
        .configure_model_runtime(
            ModelConfig {
                base_url: fixture.base.clone(),
                api_key: Some(String::from("live-key")),
                default_model: String::from("live-claude"),
            },
            ModelRequestOptions {
                provider_id: Some(String::from("live-native")),
                protocol: qwenpaw_core::ModelProtocol::AnthropicMessages,
                ..ModelRequestOptions::default()
            },
        )
        .unwrap();
    console_turn(&fixture, "live", "1700000000000-live").await;
    let remote = fixture.remote.lock().unwrap();
    assert_eq!(remote.requests.len(), 2);
    for (headers, body) in &remote.requests {
        assert_eq!(headers["x-api-key"], "live-key");
        assert_eq!(body["model"], "live-claude");
    }
    assert_eq!(
        read_registry(&fixture.server).unwrap().active_provider_id,
        DEFAULT_PROVIDER_ID
    );
    fixture.task.abort();
}

#[tokio::test]
async fn active_turn_keeps_provider_snapshot_when_global_runtime_changes_between_steps() {
    let fixture = Fixture::new(false).await;
    api(
        &fixture.server,
        "PUT",
        "/api/models/anthropic/config",
        json!({"base_url": fixture.base, "api_key": "turn-key"}),
    )
    .await;
    api(
        &fixture.server,
        "POST",
        "/api/models/anthropic/models",
        json!({"id": "fixture-claude", "name": "Fixture Claude"}),
    )
    .await;
    api(
        &fixture.server,
        "PUT",
        "/api/models/active",
        json!({
            "provider_id": "anthropic", "model": "fixture-claude", "scope": "global"
        }),
    )
    .await;
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
    let gate = Arc::new(Gate::default());
    fixture.remote.lock().unwrap().gate = Some(gate.clone());
    tokio::join!(turn(core, &thread.id, TurnStatus::Completed), async {
        tokio::time::timeout(Duration::from_secs(10), gate.started.notified())
            .await
            .unwrap();
        core.configure_model_runtime(config(), ModelRequestOptions::default())
            .unwrap();
        gate.release.notify_one();
    });
    assert_eq!(core.read_config().config.default_model, "initial-model");
    let remote = fixture.remote.lock().unwrap();
    assert_eq!(remote.requests.len(), 2);
    for (headers, body) in &remote.requests {
        assert_eq!(headers["x-api-key"], "turn-key");
        assert_eq!(body["model"], "fixture-claude");
    }
    fixture.task.abort();
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn anthropic_selection_tool_roundtrip_restart_restore_auth_error_and_cancel() {
    let Fixture {
        directory,
        server,
        remote,
        base,
        credentials,
        task,
    } = Fixture::new(false).await;
    let workspace = directory.path().join("workspace");
    let database = directory.path().join("threads.sqlite3");
    api(
        &server,
        "PUT",
        "/api/models/anthropic/config",
        json!({"base_url": base,
        "api_key": "fixture-private-key", "custom_headers": {"x-fixture": "native"},
        "generate_kwargs": {"max_tokens": 2048, "thinking_enable": true, "thinking_budget": 1024}}),
    )
    .await;
    api(
        &server,
        "POST",
        "/api/models/anthropic/models",
        json!({"id": "fixture-claude", "name": "Fixture Claude"}),
    )
    .await;
    api(
        &server,
        "PUT",
        "/api/models/active",
        json!({"provider_id": "anthropic", "model": "fixture-claude", "scope": "global"}),
    )
    .await;
    let thread = server
        .inner
        .core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(workspace.to_string_lossy().into_owned()),
        })
        .await
        .unwrap()
        .thread;
    turn(&server.inner.core, &thread.id, TurnStatus::Completed).await;
    api(
        &server,
        "PUT",
        "/api/models/anthropic/config",
        json!({"base_url": format!("{base}/v1"), "auth_mode": "auth_token",
        "custom_headers": {"x-fixture": "native", "x-api-key": "must-not-send"}}),
    )
    .await;
    let restarted = Core::persistent(config(), &database).unwrap();
    initialize(
        &restarted,
        credentials.as_ref(),
        desktop_workspace(&server).unwrap(),
    )
    .unwrap();
    turn(&restarted, &thread.id, TurnStatus::Completed).await;
    let snapshot = restarted.backup_snapshot(4 * 1024 * 1024).unwrap();
    let restored = restarted.prepare_restore(&snapshot).unwrap();
    hydrate_restore(
        &restored,
        credentials.as_ref(),
        &serde_json::to_vec(&read_registry(&server).unwrap()).unwrap(),
    )
    .unwrap();
    turn(&restored, &thread.id, TurnStatus::Completed).await;
    let snapshot = restored.backup_snapshot(4 * 1024 * 1024).unwrap();
    assert_eq!(snapshot.usage.len(), 6);
    for usage in &snapshot.usage {
        assert_eq!(
            usage.call,
            qwenpaw_storage::StoredModelCall {
                provider_id: String::from("anthropic"),
                model: String::from("fixture-claude"),
                prompt_tokens: 35,
                completion_tokens: 7,
                cache_read_tokens: 20,
                cache_write_tokens: 5,
                cache_eligible_input_tokens: 35,
                cache_observed: true,
                usage_observed: true
            }
        );
    }
    {
        let remote = remote.lock().unwrap();
        assert_eq!(remote.requests.len(), 6);
        for (index, (headers, body)) in remote.requests.iter().enumerate() {
            assert_eq!(headers["anthropic-version"], "2023-06-01");
            assert_eq!(headers["x-fixture"], "native");
            if index < 2 {
                assert_eq!(headers["x-api-key"], "fixture-private-key");
                assert!(!headers.contains_key("authorization"));
            } else {
                assert_eq!(headers["authorization"], "Bearer fixture-private-key");
                assert!(!headers.contains_key("x-api-key"));
            }
            assert_eq!(body["model"], "fixture-claude");
            assert_eq!(
                body["thinking"],
                json!({"type": "enabled", "budget_tokens": 1024})
            );
            assert_eq!(body["max_tokens"], 2048);
            assert!(body["system"].is_array());
            assert!(
                body["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|tool| tool["name"] == "read_file" && tool["input_schema"].is_object())
            );
            assert!(body.get("stream_options").is_none());
            assert!(
                body["messages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|message| message.get("provider_content").is_none())
            );
        }
    }
    remote.lock().unwrap().fail = true;
    turn(&restored, &thread.id, TurnStatus::Failed).await;
    {
        let mut remote = remote.lock().unwrap();
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
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(event) = events.recv().await {
            if matches!(event, CoreEvent::AgentMessageDelta(_)) {
                break;
            }
        }
    })
    .await
    .unwrap();
    restored
        .interrupt_turn(&qwenpaw_protocol::TurnInterruptParams {
            thread_id: thread.id.clone(),
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
    task.abort();
}
