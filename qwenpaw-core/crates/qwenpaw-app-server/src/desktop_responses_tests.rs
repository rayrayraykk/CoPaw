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

const IMAGE: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";

#[derive(Default)]
struct Remote {
    requests: Vec<(HeaderMap, Value)>,
    fail: bool,
    hang: bool,
}

async fn responses(
    State(remote): State<Arc<Mutex<Remote>>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> axum::response::Response {
    if body["stream"] != true {
        return Json(json!({"id": "probe", "status": "completed", "output": [{"type": "message",
            "role": "assistant", "status": "completed", "content": [{"type": "output_text", "text": "OK"}]}]})).into_response();
    }
    let (events, hang) = {
        let mut remote = remote.lock().unwrap();
        remote.requests.push((headers, body.clone()));
        let input = body["input"].as_array().unwrap();
        let last = input.last().unwrap();
        let tool = last["type"] != "function_call_output";
        if !tool {
            assert_eq!(last["output"], "fixture content");
            assert_eq!(last["call_id"], "call_1");
            assert_eq!(input[input.len() - 2]["call_id"], "call_1");
            assert_eq!(
                input[input.len() - 3]["encrypted_content"],
                "opaque-fixture-reasoning"
            );
        }
        let call = json!({"type": "function_call", "id": "fc_1", "call_id": "call_1", "name": "read_file",
            "arguments": "{\"path\":\"fixture.txt\"}", "status": "completed"});
        let mut events = vec![json!({"type": "response.created", "response": {"id": "resp_1"}})];
        if remote.hang {
            events.push(json!({"type": "response.output_text.delta", "delta": "pending"}));
        } else if remote.fail {
            events.extend([json!({"type": "response.output_item.done", "output_index": 0, "item": call}),
                json!({"type": "response.incomplete", "response": {"id": "resp_1", "error": {"message": "responses-private-key"}}})]);
        } else {
            let output = if tool {
                vec![
                    json!({"type": "reasoning", "id": "rs_1", "summary": [], "encrypted_content": "opaque-fixture-reasoning"}),
                    call,
                ]
            } else {
                events.push(
                    json!({"type": "response.output_text.delta", "delta": "原生 Responses 回复"}),
                );
                vec![
                    json!({"type": "message", "id": "msg_1", "status": "completed", "role": "assistant",
                    "content": [{"type": "output_text", "text": "原生 Responses 回复", "annotations": []}]}),
                ]
            };
            events.push(json!({"type": "response.completed", "response": {"id": "resp_1", "status": "completed",
                "output": output, "usage": {"input_tokens": 10, "output_tokens": 8,
                    "input_tokens_details": {"cached_tokens": 4}, "output_tokens_details": {"reasoning_tokens": 5}}}}));
        }
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
        use base64::Engine as _;
        let directory = tempfile::tempdir().unwrap();
        let workspace = directory.path().join("workspace");
        let console = directory.path().join("console");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&console).unwrap();
        fs::write(workspace.join("fixture.txt"), "fixture content").unwrap();
        fs::write(
            workspace.join("red.png"),
            base64::engine::general_purpose::STANDARD
                .decode(IMAGE)
                .unwrap(),
        )
        .unwrap();
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
        let base = format!("http://{}/proxy/v1", listener.local_addr().unwrap());
        let router = Router::new()
            .route("/proxy/v1/responses", post(responses))
            .route(
                "/proxy/v1/models",
                get(|| async {
                    Json(json!({"data": [{"id": "fixture-responses", "object": "model"}]}))
                }),
            )
            .with_state(remote.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let credentials = Arc::new(Credentials::default());
        let server = AppServer::new_desktop_with_stores_and_workspace(
            Core::persistent(config(), &directory.path().join("threads.sqlite3")).unwrap(),
            &console,
            "responses-fixture-shutdown".into(),
            credentials.clone(),
            &directory.path().join("data"),
            &workspace,
        )
        .unwrap();
        server.inner.core.write_ui_language("en").unwrap();
        let mut registry = read_registry(&server).unwrap();
        let provider = registry.providers.get_mut("openai-response").unwrap();
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
        api(
            &self.server,
            "PUT",
            "/api/models/openai-response/config",
            json!({"api_key": "responses-private-key",
            "custom_headers": {"x-fixture": "native"}, "generate_kwargs": {"max_tokens": 1024}}),
        )
        .await;
        api(
            &self.server,
            "POST",
            "/api/models/openai-response/models",
            json!({"id": "fixture-responses", "name": "Fixture Responses"}),
        )
        .await;
        api(
            &self.server,
            "PUT",
            "/api/models/openai-response/models/fixture-responses/config",
            json!({"generate_kwargs": {"max_output_tokens": 2048}}),
        )
        .await;
        api(&self.server, "PUT", "/api/models/active", json!({"provider_id": "openai-response", "model": "fixture-responses", "scope": "global"})).await;
    }
}

async fn turn(core: &Core, thread: &str, expected: TurnStatus) {
    let (_, mut events) = core
        .start_turn(TurnStartParams {
            thread_id: thread.into(),
            input: vec![
                UserInput::Text {
                    text: "Read fixture.txt".into(),
                },
                UserInput::Image {
                    path: "red.png".into(),
                },
            ],
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
                CoreEvent::TurnCompleted(done) => {
                    assert_eq!(done.turn.status, expected, "{:?}", done.turn.error);
                    assert!(!format!("{:?}", done.turn.error).contains("responses-private-key"));
                    assert_eq!(tools, usize::from(expected == TurnStatus::Completed));
                    if expected == TurnStatus::Completed {
                        assert_eq!(text, "原生 Responses 回复");
                    }
                    return;
                }
                _ => {}
            }
        }
        panic!("missing terminal event");
    })
    .await
    .unwrap();
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn native_provider_tools_images_restart_failure_and_cancellation() {
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
    {
        let remote = fixture.remote.lock().unwrap();
        assert_eq!(remote.requests.len(), 4);
        for (headers, body) in &remote.requests {
            assert_eq!(headers["authorization"], "Bearer responses-private-key");
            assert_eq!(headers["x-fixture"], "native");
            assert_eq!(body["store"], false);
            assert_eq!(body["model"], "fixture-responses");
            assert_eq!(body["max_output_tokens"], 2048);
            assert!(body.get("messages").is_none());
            let image = body["input"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|item| item["content"].as_array())
                .flatten()
                .find(|part| part["type"] == "input_image")
                .unwrap();
            assert_eq!(image["image_url"], format!("data:image/png;base64,{IMAGE}"));
        }
        assert!(
            remote.requests[2]
                .1
                .to_string()
                .contains("opaque-fixture-reasoning")
        );
    }
    let history = restarted.read_thread(&thread.id).await.unwrap();
    assert!(
        !serde_json::to_string(&history)
            .unwrap()
            .contains("opaque-fixture-reasoning")
    );
    fixture.remote.lock().unwrap().fail = true;
    turn(&restarted, &thread.id, TurnStatus::Failed).await;
    {
        let mut remote = fixture.remote.lock().unwrap();
        remote.fail = false;
        remote.hang = true;
    }
    let (started, mut events) = restarted
        .start_turn(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![UserInput::Text {
                text: "Wait".into(),
            }],
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(event) = events.recv().await {
            if matches!(event, CoreEvent::AgentMessageDelta(_)) {
                break;
            }
        }
    })
    .await
    .unwrap();
    restarted
        .interrupt_turn(&qwenpaw_protocol::TurnInterruptParams {
            thread_id: thread.id,
            turn_id: started.turn.id,
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(event) = events.recv().await {
            if let CoreEvent::TurnCompleted(done) = event {
                assert_eq!(done.turn.status, TurnStatus::Interrupted);
                return;
            }
        }
        panic!("missing interrupted completion");
    })
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_responses_browser_configures_selects_uploads_chats_and_reloads() {
    let fixture = Fixture::new(true).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let http = tokio::spawn(fixture.server.clone().run_http(listener));
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = tokio::time::timeout(
        Duration::from_secs(100),
        tokio::process::Command::new("node")
            .arg(root.join("scripts/console_browser_smoke.mjs"))
            .args([&origin, "/models", "--responses-chat"])
            .env("QWENPAW_RESPONSES_FIXTURE_URL", &fixture.base)
            .env(
                "QWENPAW_IMAGE_FIXTURE_PATH",
                fixture.directory.path().join("workspace/red.png"),
            )
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
        report["pages"][0]["responsesChat"],
        json!({"configured": true, "modelAdded": true,
        "agentSelected": true, "reply": true, "tool": true, "reload": true, "image": true})
    );
    let remote = fixture.remote.lock().unwrap();
    assert_eq!(remote.requests.len(), 2);
    for (headers, body) in &remote.requests {
        assert_eq!(headers["authorization"], "Bearer sk-responses-private-key");
        assert_eq!(body["store"], false);
        let image = body["input"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|item| item["content"].as_array())
            .flatten()
            .find(|part| part["type"] == "input_image")
            .unwrap();
        assert_eq!(image["image_url"], format!("data:image/png;base64,{IMAGE}"));
    }
}
