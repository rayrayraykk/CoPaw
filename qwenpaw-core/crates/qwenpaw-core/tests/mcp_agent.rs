use std::fs;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::post;
use pretty_assertions::assert_eq;
use qwenpaw_core::Core;
use qwenpaw_core::McpAccessEffect;
use qwenpaw_core::McpManager;
use qwenpaw_core::ModelConfig;
use qwenpaw_protocol::ApprovalDecision;
use qwenpaw_protocol::CoreEvent;
use qwenpaw_protocol::Item;
use qwenpaw_protocol::ThreadStartParams;
use qwenpaw_protocol::ToolApprovalRespondParams;
use qwenpaw_protocol::TurnInterruptParams;
use qwenpaw_protocol::TurnStartParams;
use qwenpaw_protocol::TurnStatus;
use qwenpaw_protocol::UserInput;
use serde_json::Value;
use serde_json::json;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::sync::oneshot;

#[tokio::test]
async fn discovers_approves_and_calls_mcp_through_the_agent_loop() {
    let directory = tempfile::tempdir().expect("temporary directory");
    run_mcp_agent_loop(&directory, test_mcp_manager(&directory)).await;
}

#[tokio::test]
async fn discovers_approves_and_calls_http_mcp_through_the_agent_loop() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let mcp = test_http_mcp_manager(&directory).await;
    run_mcp_agent_loop(&directory, mcp).await;
}

#[tokio::test]
async fn enforces_allow_and_deny_mcp_policies_without_prompting() {
    for (effect, expected_error) in [
        (McpAccessEffect::Allow, false),
        (McpAccessEffect::Deny, true),
    ] {
        let directory = tempfile::tempdir().expect("temporary directory");
        let manager = test_mcp_manager(&directory);
        let mut settings = manager.settings();
        settings[0].access.default_effect = effect;
        let manager = manager
            .reconfigured(settings)
            .expect("policy should reconfigure");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let base_url = start_model_server(Arc::clone(&requests)).await;
        let core = Core::new_with_mcp(
            ModelConfig {
                api_key: None,
                base_url,
                default_model: String::from("qwen-test"),
            },
            manager,
        );
        let thread = core
            .start_thread(ThreadStartParams {
                model: None,
                workspace_root: Some(directory.path().to_string_lossy().into_owned()),
            })
            .await
            .expect("start thread");
        let (_, mut events) = core
            .start_turn(TurnStartParams {
                thread_id: thread.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: String::from("Use the MCP echo tool"),
                }],
            })
            .await
            .expect("start turn");

        while let Some(event) = events.recv().await {
            assert!(!matches!(event, CoreEvent::ToolApprovalRequested(_)));
            if matches!(event, CoreEvent::TurnCompleted(_)) {
                break;
            }
        }
        let read = core
            .read_thread(&thread.thread.id)
            .await
            .expect("read completed thread");
        assert!(matches!(
            &read.turns[0].items[2],
            Item::ToolResult { is_error, .. } if *is_error == expected_error
        ));
        if expected_error {
            assert!(matches!(
                &read.turns[0].items[2],
                Item::ToolResult { content, .. }
                    if content == "Tool execution was denied by the MCP access policy."
            ));
        }
    }
}

#[tokio::test]
async fn keeps_the_starting_mcp_snapshot_for_an_in_flight_turn() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let manager = test_mcp_manager(&directory);
    let mut settings = manager.settings();
    settings[0].access.default_effect = McpAccessEffect::Allow;
    let manager = manager
        .reconfigured(settings)
        .expect("policy should reconfigure");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (entered_tx, entered_rx) = oneshot::channel();
    let (resume_tx, resume_rx) = oneshot::channel();
    let base_url = start_paused_model_server(Arc::clone(&requests), entered_tx, resume_rx).await;
    let core = Core::new_with_mcp(
        ModelConfig {
            api_key: None,
            base_url,
            default_model: String::from("qwen-test"),
        },
        manager,
    );
    let thread = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .expect("start thread");
    let (_, mut events) = core
        .start_turn(TurnStartParams {
            thread_id: thread.thread.id.clone(),
            input: vec![UserInput::Text {
                text: String::from("Use the MCP echo tool"),
            }],
        })
        .await
        .expect("start turn");

    entered_rx.await.expect("model request should pause");
    core.replace_mcp_client_settings(Vec::new())
        .expect("new MCP configuration should activate");
    resume_tx.send(()).expect("model request should resume");
    while let Some(event) = events.recv().await {
        if matches!(event, CoreEvent::TurnCompleted(_)) {
            break;
        }
    }

    assert!(core.mcp_client_settings().is_empty());
    let read = core
        .read_thread(&thread.thread.id)
        .await
        .expect("read completed thread");
    assert!(matches!(
        &read.turns[0].items[2],
        Item::ToolResult { content, is_error: false, .. }
            if content == r#"{"echo":"hello"}"#
    ));
}

async fn run_mcp_agent_loop(directory: &tempfile::TempDir, mcp: McpManager) {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let base_url = start_model_server(Arc::clone(&requests)).await;
    let core = Core::new_with_mcp(
        ModelConfig {
            api_key: None,
            base_url,
            default_model: String::from("qwen-test"),
        },
        mcp,
    );
    let thread = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .expect("start thread");
    let (_, mut events) = core
        .start_turn(TurnStartParams {
            thread_id: thread.thread.id.clone(),
            input: vec![UserInput::Text {
                text: String::from("Use the MCP echo tool"),
            }],
        })
        .await
        .expect("start turn");

    let mut approval_requested = false;
    while let Some(event) = events.recv().await {
        match event {
            CoreEvent::ToolApprovalRequested(notification) => {
                approval_requested = true;
                assert_eq!(notification.tool_name, "mcp__echo__echo");
                assert!(
                    core.respond_tool_approval(ToolApprovalRespondParams {
                        approval_id: notification.approval_id,
                        decision: ApprovalDecision::Approved,
                    })
                    .await
                    .accepted
                );
            }
            CoreEvent::TurnCompleted(_) => break,
            _ => {}
        }
    }

    let read = core
        .read_thread(&thread.thread.id)
        .await
        .expect("read completed thread");
    assert!(approval_requested);
    assert_eq!(read.turns[0].status, TurnStatus::Completed);
    assert!(matches!(
        &read.turns[0].items[2],
        Item::ToolResult { content, is_error: false, .. }
            if content == r#"{"echo":"hello"}"#
    ));
    let requests = requests.lock().await;
    assert_eq!(requests.len(), 2);
    assert!(requests[0]["tools"].as_array().is_some_and(|tools| {
        tools
            .iter()
            .any(|tool| tool["function"]["name"] == json!("mcp__echo__echo"))
    }));
    assert_eq!(
        requests[1]["messages"][3],
        json!({
            "role": "tool",
            "content": r#"{"echo":"hello"}"#,
            "tool_call_id": "call_mcp"
        })
    );
}

#[tokio::test]
async fn interrupts_a_running_mcp_call_and_terminates_its_server() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let base_url = start_model_server_with_argument(Arc::clone(&requests), "slow").await;
    let core = Core::new_with_mcp(
        ModelConfig {
            api_key: None,
            base_url,
            default_model: String::from("qwen-test"),
        },
        test_mcp_manager(&directory),
    );
    let thread = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .expect("start thread");
    let (turn, mut events) = core
        .start_turn(TurnStartParams {
            thread_id: thread.thread.id.clone(),
            input: vec![UserInput::Text {
                text: String::from("Start and cancel the MCP tool"),
            }],
        })
        .await
        .expect("start turn");

    let completed = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match events.recv().await.expect("turn event") {
                CoreEvent::ToolApprovalRequested(notification) => {
                    assert!(
                        core.respond_tool_approval(ToolApprovalRespondParams {
                            approval_id: notification.approval_id,
                            decision: ApprovalDecision::Approved,
                        })
                        .await
                        .accepted
                    );
                }
                CoreEvent::ToolApprovalResolved(_) => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    assert!(
                        core.interrupt_turn(&TurnInterruptParams {
                            thread_id: thread.thread.id.clone(),
                            turn_id: turn.turn.id.clone(),
                        })
                        .await
                        .expect("interrupt turn")
                        .accepted
                    );
                }
                CoreEvent::TurnCompleted(notification) => break notification.turn,
                _ => {}
            }
        }
    })
    .await
    .expect("interrupted MCP call should finish promptly");

    assert_eq!(completed.status, TurnStatus::Interrupted);
    assert_eq!(requests.lock().await.len(), 1);
}

async fn start_model_server(requests: Arc<Mutex<Vec<Value>>>) -> String {
    start_model_server_with_argument(requests, "hello").await
}

async fn start_paused_model_server(
    requests: Arc<Mutex<Vec<Value>>>,
    entered_tx: oneshot::Sender<()>,
    resume_rx: oneshot::Receiver<()>,
) -> String {
    let entered_tx = Arc::new(Mutex::new(Some(entered_tx)));
    let resume_rx = Arc::new(Mutex::new(Some(resume_rx)));
    let app = Router::new().route(
        "/chat/completions",
        post(move |body: axum::Json<Value>| {
            let requests = Arc::clone(&requests);
            let entered_tx = Arc::clone(&entered_tx);
            let resume_rx = Arc::clone(&resume_rx);
            async move {
                let request_number = {
                    let mut requests = requests.lock().await;
                    requests.push(body.0);
                    requests.len()
                };
                let response = if request_number == 1 {
                    if let Some(sender) = entered_tx.lock().await.take() {
                        let _ = sender.send(());
                    }
                    if let Some(receiver) = resume_rx.lock().await.take() {
                        let _ = receiver.await;
                    }
                    tool_call_response("hello")
                } else {
                    text_response("MCP echo completed")
                };
                Response::builder()
                    .status(StatusCode::OK)
                    .header(CONTENT_TYPE, "text/event-stream")
                    .body(Body::from(response))
                    .expect("mock response")
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind model server");
    let address = listener.local_addr().expect("model server address");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve mock model");
    });
    format!("http://{address}")
}

async fn start_model_server_with_argument(
    requests: Arc<Mutex<Vec<Value>>>,
    argument: &'static str,
) -> String {
    let app = Router::new().route(
        "/chat/completions",
        post(move |body: axum::Json<Value>| {
            let requests = Arc::clone(&requests);
            async move {
                let mut requests = requests.lock().await;
                requests.push(body.0);
                let response = if requests.len() == 1 {
                    tool_call_response(argument)
                } else {
                    text_response("MCP echo completed")
                };
                Response::builder()
                    .status(StatusCode::OK)
                    .header(CONTENT_TYPE, "text/event-stream")
                    .body(Body::from(response))
                    .expect("mock response")
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind model server");
    let address = listener.local_addr().expect("model server address");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve mock model");
    });
    format!("http://{address}")
}

fn tool_call_response(argument: &str) -> String {
    let chunk = json!({
        "choices": [{
            "delta": {
                "tool_calls": [{
                    "index": 0,
                    "id": "call_mcp",
                    "function": {
                        "name": "mcp__echo__echo",
                        "arguments": serde_json::to_string(&json!({"text": argument}))
                            .expect("serialize MCP arguments")
                    }
                }]
            }
        }]
    });
    format!("data: {chunk}\n\ndata: [DONE]\n\n")
}

fn test_mcp_manager(directory: &tempfile::TempDir) -> McpManager {
    let config_path = directory.path().join("mcp.json");
    fs::write(
        &config_path,
        serde_json::to_vec(&json!({
            "clients": {
                "echo": {
                    "enabled": true,
                    "transport": "stdio",
                    "command": env!("CARGO_BIN_EXE_qwenpaw-core-mcp-test-server"),
                    "tools": ["echo"]
                }
            }
        }))
        .expect("serialize MCP config"),
    )
    .expect("write MCP config");
    McpManager::from_path(&config_path).expect("load MCP config")
}

async fn test_http_mcp_manager(directory: &tempfile::TempDir) -> McpManager {
    let app = Router::new().route("/mcp", post(handle_http_mcp));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind HTTP MCP server");
    let address = listener.local_addr().expect("HTTP MCP server address");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve HTTP MCP");
    });
    let config_path = directory.path().join("http-mcp.json");
    fs::write(
        &config_path,
        serde_json::to_vec(&json!({
            "clients": {
                "echo": {
                    "enabled": true,
                    "transport": "streamable_http",
                    "url": format!("http://{address}/mcp"),
                    "tools": ["echo"]
                }
            }
        }))
        .expect("serialize HTTP MCP config"),
    )
    .expect("write HTTP MCP config");
    McpManager::from_path(&config_path).expect("load HTTP MCP config")
}

async fn handle_http_mcp(axum::Json(request): axum::Json<Value>) -> Response {
    let Some(id) = request.get("id").cloned() else {
        return StatusCode::ACCEPTED.into_response();
    };
    let result = match request.get("method").and_then(Value::as_str) {
        Some("initialize") => json!({
            "protocolVersion": "2025-03-26",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "core-http-test", "version": "0.1.0"}
        }),
        Some("tools/list") => json!({
            "tools": [{
                "name": "echo",
                "description": "Echo a text value",
                "inputSchema": {
                    "type": "object",
                    "properties": {"text": {"type": "string"}},
                    "required": ["text"]
                }
            }]
        }),
        Some("tools/call") => {
            let text = request["params"]["arguments"]["text"]
                .as_str()
                .unwrap_or_default();
            json!({
                "content": [{"type": "text", "text": text}],
                "structuredContent": {"echo": text},
                "isError": false
            })
        }
        method => panic!("unexpected HTTP MCP method: {method:?}"),
    };
    axum::Json(json!({"jsonrpc": "2.0", "id": id, "result": result})).into_response()
}

fn text_response(content: &str) -> String {
    let chunk = json!({"choices": [{"delta": {"content": content}}]});
    format!("data: {chunk}\n\ndata: [DONE]\n\n")
}
