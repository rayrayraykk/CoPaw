use futures_util::SinkExt;
use futures_util::StreamExt;
use qwenpaw_app_server::AppServer;
use qwenpaw_app_server::DesktopCredentialStore;
use qwenpaw_core::Core;
use qwenpaw_core::ModelConfig;
use qwenpaw_protocol::ThreadStartParams;
use serde_json::Value;
use serde_json::json;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio_tungstenite::MaybeTlsStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;

type ClientSocket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Default)]
struct MemoryCredentialStore {
    api_key: Mutex<Option<String>>,
}

impl DesktopCredentialStore for MemoryCredentialStore {
    fn load_api_key(&self) -> anyhow::Result<Option<String>> {
        Ok(self
            .api_key
            .lock()
            .expect("test credential lock should be available")
            .clone())
    }

    fn save_api_key(&self, api_key: Option<&str>) -> anyhow::Result<()> {
        *self
            .api_key
            .lock()
            .expect("test credential lock should be available") = api_key.map(str::to_owned);
        Ok(())
    }
}

#[tokio::test]
async fn serves_health_and_independent_websocket_sessions() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener should bind");
    let address = listener
        .local_addr()
        .expect("test listener should have an address");
    let server = AppServer::new(Core::new(ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("qwen-test"),
    }));
    let task = tokio::spawn(server.run_http(listener));

    let mut health = tokio::net::TcpStream::connect(address)
        .await
        .expect("health client should connect");
    health
        .write_all(b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .expect("health request should write");
    let mut health_response = Vec::new();
    health
        .read_to_end(&mut health_response)
        .await
        .expect("health response should read");
    let health_response =
        String::from_utf8(health_response).expect("health response should be UTF-8");
    assert!(health_response.starts_with("HTTP/1.1 200 OK"));
    assert!(health_response.contains("{\"status\":\"ok\"}"));

    let url = format!("ws://{address}/app-protocol");
    let (mut first, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("first WebSocket should connect");
    send_json(
        &mut first,
        json!({
            "id": 1,
            "method": "initialize",
            "params": {"clientInfo": {"name": "test", "version": "0.1.0"}}
        }),
    )
    .await;
    let initialized = receive_json(&mut first).await;
    assert_eq!(initialized["result"]["protocolVersion"], json!(2));

    let (mut second, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("second WebSocket should connect");
    send_json(
        &mut second,
        json!({"id": 2, "method": "thread/list", "params": {}}),
    )
    .await;
    assert_eq!(
        receive_json(&mut second).await,
        json!({
            "id": 2,
            "error": {"code": -32000, "message": "server is not initialized"}
        })
    );

    first.close(None).await.expect("first socket should close");
    second
        .close(None)
        .await
        .expect("second socket should close");
    task.abort();
}

#[tokio::test]
async fn rejects_a_non_loopback_http_listener() {
    let listener = tokio::net::TcpListener::bind("0.0.0.0:0")
        .await
        .expect("test listener should bind");
    let server = AppServer::new(Core::new(ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("qwen-test"),
    }));

    let error = server
        .run_http(listener)
        .await
        .expect_err("non-loopback listener should fail closed");
    assert_eq!(
        error.to_string(),
        "HTTP App Protocol requires a loopback listener"
    );
}

#[tokio::test]
async fn serves_the_console_and_requires_the_desktop_shutdown_token() {
    let console = tempfile::tempdir().expect("temporary Console should be created");
    std::fs::create_dir(console.path().join("assets"))
        .expect("Console assets directory should be created");
    std::fs::write(console.path().join("index.html"), "<html>console</html>")
        .expect("Console index should be written");
    std::fs::write(
        console.path().join("assets/app.js"),
        "window.qwenpaw = true;",
    )
    .expect("Console asset should be written");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Desktop listener should bind");
    let address = listener
        .local_addr()
        .expect("Desktop listener should have an address");
    let server = AppServer::new_desktop(
        Core::new(ModelConfig {
            api_key: None,
            base_url: String::from("http://127.0.0.1:1"),
            default_model: String::from("qwen-test"),
        }),
        console.path(),
        String::from("desktop-shutdown-token"),
    )
    .expect("Desktop server should configure");
    let task = tokio::spawn(server.run_http(listener));

    let version = http_request(
        address,
        "GET /api/version HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(version.starts_with("HTTP/1.1 200 OK"));
    assert!(
        version.contains("{\"backend\":\"rust-core\",\"protocolVersion\":2,\"version\":\"0.1.0\"}")
    );

    let console_index = http_request(
        address,
        "GET /console HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(console_index.starts_with("HTTP/1.1 200 OK"));
    assert!(console_index.contains("cache-control: no-cache, no-store, must-revalidate"));
    assert!(console_index.ends_with("<html>console</html>"));

    let asset = http_request(
        address,
        "GET /assets/app.js HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(asset.starts_with("HTTP/1.1 200 OK"));
    assert!(asset.ends_with("window.qwenpaw = true;"));
    let spa = http_request(
        address,
        "GET /chat/thread HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(spa.ends_with("<html>console</html>"));
    let missing_api = http_request(
        address,
        "GET /api/not-migrated HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(missing_api.starts_with("HTTP/1.1 404 Not Found"));
    assert!(!missing_api.contains("<html>console</html>"));

    for authorization in [None, Some("wrong-token")] {
        let header = authorization.map_or_else(String::new, |token| {
            format!("X-QwenPaw-Desktop-Shutdown-Token: {token}\r\n")
        });
        let response = http_request(
            address,
            &format!(
                "POST /api/desktop/shutdown HTTP/1.1\r\nHost: localhost\r\n{header}Content-Length: 0\r\nConnection: close\r\n\r\n"
            ),
        )
        .await;
        assert!(response.starts_with("HTTP/1.1 404 Not Found"));
        assert!(!task.is_finished());
    }

    let shutdown = http_request(
        address,
        "POST /api/desktop/shutdown HTTP/1.1\r\nHost: localhost\r\nX-QwenPaw-Desktop-Shutdown-Token: desktop-shutdown-token\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(shutdown.starts_with("HTTP/1.1 200 OK"));
    assert!(shutdown.contains("{\"ok\":true}"));
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("Desktop server should stop before timeout")
        .expect("Desktop server task should join")
        .expect("Desktop server should stop cleanly");
}

#[tokio::test]
async fn serves_the_unchanged_console_bootstrap_contracts() {
    let console = tempfile::tempdir().expect("temporary Console should be created");
    std::fs::write(console.path().join("index.html"), "<html>console</html>")
        .expect("Console index should be written");
    let core = Core::new(ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("qwen-test"),
    });
    let thread = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: None,
        })
        .await
        .expect("Core thread should start")
        .thread;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Desktop listener should bind");
    let address = listener
        .local_addr()
        .expect("Desktop listener should have an address");
    let credentials = Arc::new(MemoryCredentialStore::default());
    let server = AppServer::new_desktop_with_credential_store(
        core,
        console.path(),
        String::from("desktop-bootstrap-token"),
        credentials.clone(),
    )
    .expect("Desktop server should configure");
    let task = tokio::spawn(server.run_http(listener));

    assert_bootstrap_json_contracts(address).await;
    assert_agent_contract(address).await;
    assert_model_contract(address).await;
    assert_model_write_contract(address, &credentials).await;
    assert_chat_contract(address, &thread.id).await;
    assert_workspace_contract(address, &thread.id).await;

    task.abort();
}

#[tokio::test]
async fn streams_console_chat_with_the_unchanged_frontend_sse_contract() {
    let model_base_url = start_model_server().await;
    let console = tempfile::tempdir().expect("temporary Console should be created");
    std::fs::write(console.path().join("index.html"), "<html>console</html>")
        .expect("Console index should be written");
    let core = Core::new(ModelConfig {
        api_key: Some(String::from("test-key")),
        base_url: model_base_url,
        default_model: String::from("qwen-test"),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Desktop listener should bind");
    let address = listener
        .local_addr()
        .expect("Desktop listener should have an address");
    let server = AppServer::new_desktop(core, console.path(), String::from("desktop-stream-token"))
        .expect("Desktop server should configure");
    let task = tokio::spawn(server.run_http(listener));
    let client = reqwest::Client::new();
    let workspace = console.path().join("workspace");
    std::fs::create_dir(&workspace).expect("Console workspace should be created");

    let response = client
        .post(format!("http://{address}/api/console/chat"))
        .json(&json!({
            "input": [{
                "role": "user",
                "content": [{"type": "text", "text": "Say hello"}]
            }],
            "session_id": "1700000000000-local",
            "user_id": "desktop",
            "channel": "console",
            "stream": true,
            "request_context": {
                "session_project_dirs": [{"path": workspace}]
            }
        }))
        .send()
        .await
        .expect("Console chat request should send");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        response.headers()[reqwest::header::CONTENT_TYPE],
        "text/event-stream"
    );
    let events = parse_sse_events(
        &response
            .text()
            .await
            .expect("Console chat stream should read"),
    );
    assert_eq!(events[0]["object"], json!("response"));
    assert_eq!(events[0]["status"], json!("in_progress"));
    assert_eq!(
        events
            .iter()
            .filter(|event| event["object"] == "content")
            .filter_map(|event| event["text"].as_str())
            .collect::<String>(),
        "Hello from QwenPaw"
    );
    assert_eq!(
        events.last().expect("stream should complete")["status"],
        "completed"
    );

    assert_streamed_chat_persisted(&client, address, &workspace).await;
    task.abort();
}

async fn assert_streamed_chat_persisted(
    client: &reqwest::Client,
    address: SocketAddr,
    workspace: &std::path::Path,
) {
    let chats = client
        .get(format!("http://{address}/api/chats?archived=false"))
        .send()
        .await
        .expect("chat list should send")
        .json::<Value>()
        .await
        .expect("chat list should be JSON");
    assert_eq!(chats[0]["session_id"], json!("1700000000000-local"));
    assert_eq!(
        chats[0]["meta"]["workspace_root"],
        json!(
            workspace
                .canonicalize()
                .expect("Console workspace should resolve")
                .to_string_lossy()
        )
    );
    let thread_id = chats[0]["id"]
        .as_str()
        .expect("chat should contain a thread id");
    let history = client
        .get(format!("http://{address}/api/chats/{thread_id}"))
        .send()
        .await
        .expect("chat history should send")
        .json::<Value>()
        .await
        .expect("chat history should be JSON");
    assert_eq!(history["status"], json!("idle"));
    assert_eq!(history["messages"][0]["role"], json!("user"));
    assert_eq!(history["messages"][1]["role"], json!("assistant"));
    assert_eq!(
        history["messages"][1]["content"][0]["text"],
        json!("Hello from QwenPaw")
    );
}

#[tokio::test]
async fn stops_an_active_console_chat_by_its_local_session_id() {
    let model_base_url = start_delayed_model_server().await;
    let console = tempfile::tempdir().expect("temporary Console should be created");
    std::fs::write(console.path().join("index.html"), "<html>console</html>")
        .expect("Console index should be written");
    let core = Core::new(ModelConfig {
        api_key: Some(String::from("test-key")),
        base_url: model_base_url,
        default_model: String::from("qwen-test"),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Desktop listener should bind");
    let address = listener
        .local_addr()
        .expect("Desktop listener should have an address");
    let server = AppServer::new_desktop(core, console.path(), String::from("desktop-cancel-token"))
        .expect("Desktop server should configure");
    let task = tokio::spawn(server.run_http(listener));
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://{address}/api/console/chat"))
        .json(&json!({
            "input": [{"role": "user", "content": "Wait"}],
            "session_id": "1700000000001-cancel",
            "stream": true
        }))
        .send()
        .await
        .expect("Console chat request should send");

    let stopped = client
        .post(format!(
            "http://{address}/api/console/chat/stop?chat_id=1700000000001-cancel"
        ))
        .send()
        .await
        .expect("Console stop request should send")
        .json::<Value>()
        .await
        .expect("Console stop response should be JSON");
    assert_eq!(stopped, json!({"stopped": true}));

    let events = parse_sse_events(
        &response
            .text()
            .await
            .expect("cancelled Console stream should read"),
    );
    assert_eq!(
        events.last().expect("cancelled stream should terminate")["status"],
        json!("canceled")
    );
    task.abort();
}

#[tokio::test]
async fn exposes_and_denies_tool_approval_through_the_console_contract() {
    let model_base_url = start_tool_model_server().await;
    let console = tempfile::tempdir().expect("temporary Console should be created");
    std::fs::write(console.path().join("index.html"), "<html>console</html>")
        .expect("Console index should be written");
    let core = Core::new(ModelConfig {
        api_key: Some(String::from("test-key")),
        base_url: model_base_url,
        default_model: String::from("qwen-test"),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Desktop listener should bind");
    let address = listener
        .local_addr()
        .expect("Desktop listener should have an address");
    let server =
        AppServer::new_desktop(core, console.path(), String::from("desktop-approval-token"))
            .expect("Desktop server should configure");
    let task = tokio::spawn(server.run_http(listener));
    let client = reqwest::Client::new();
    let session_id = "1700000000002-approval";
    let response = client
        .post(format!("http://{address}/api/console/chat"))
        .json(&json!({
            "input": [{"role": "user", "content": "Run a command"}],
            "session_id": session_id,
            "stream": true
        }))
        .send()
        .await
        .expect("Console chat request should send");

    let approval = wait_for_pending_approval(&client, address).await;
    assert_eq!(approval["session_id"], json!(session_id));
    assert_eq!(approval["root_session_id"], json!(session_id));
    assert_eq!(approval["tool_name"], json!("shell"));
    assert_eq!(approval["tool_params"], json!({"command": "must-not-run"}));
    let request_id = approval["request_id"]
        .as_str()
        .expect("approval should have a request id");
    let denied = client
        .post(format!("http://{address}/api/approval/deny"))
        .json(&json!({"request_id": request_id, "session_id": session_id}))
        .send()
        .await
        .expect("approval denial should send");
    assert_eq!(denied.status(), reqwest::StatusCode::OK);
    assert_eq!(
        denied
            .json::<Value>()
            .await
            .expect("approval denial should be JSON")["success"],
        json!(true)
    );

    let events = parse_sse_events(
        &response
            .text()
            .await
            .expect("approved Console stream should read"),
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event["object"] == "content")
            .filter_map(|event| event["text"].as_str())
            .collect::<String>(),
        "Denied safely"
    );
    assert_eq!(
        events.last().expect("denied stream should complete")["status"],
        json!("completed")
    );
    let push_messages = client
        .get(format!("http://{address}/api/console/push-messages"))
        .send()
        .await
        .expect("push messages request should send")
        .json::<Value>()
        .await
        .expect("push messages response should be JSON");
    assert_eq!(push_messages["pending_approvals"], json!([]));
    task.abort();
}

async fn assert_bootstrap_json_contracts(address: SocketAddr) {
    for (path, expected) in [
        (
            "/api/auth/status",
            json!({"enabled": false, "has_users": false}),
        ),
        ("/api/auth/verify", json!({"valid": true, "username": ""})),
        ("/api/settings/language", json!({"language": "en"})),
        (
            "/api/settings/upload-limit",
            json!({"upload_max_size_mb": null}),
        ),
        (
            "/api/coding-mode",
            json!({"enabled": false, "agent_id": "default"}),
        ),
        (
            "/api/console/push-messages",
            json!({"messages": [], "pending_approvals": []}),
        ),
        (
            "/api/console/inbox/events?unread_only=true&limit=1",
            json!({"events": [], "total": 0, "unread_count": 0}),
        ),
        ("/api/frontend_plugin", json!([])),
    ] {
        let response = http_request(
            address,
            &format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"),
        )
        .await;
        assert!(
            response.starts_with("HTTP/1.1 200 OK"),
            "{path}: {response}"
        );
        assert_eq!(response_json(&response), expected, "{path}");
    }
}

async fn assert_agent_contract(address: SocketAddr) {
    let agents = http_request(
        address,
        "GET /api/agents HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    let agents = response_json(&agents);
    assert_eq!(agents["agents"][0]["id"], json!("default"));
    assert_eq!(agents["agents"][0]["name"], json!("QwenPaw"));
    assert_eq!(agents["agents"][0]["backend"], json!("qwenpaw"));
    let workspace = agents["agents"][0]["workspace_dir"]
        .as_str()
        .expect("default agent should expose its Workspace");
    assert!(PathBuf::from(workspace).is_dir());
}

async fn assert_model_contract(address: SocketAddr) {
    let active = http_request(
        address,
        "GET /api/models/active?scope=effective&agent_id=default HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert_eq!(
        response_json(&active),
        json!({
            "active_llm": {
                "provider_id": "openai-compatible",
                "model": "qwen-test"
            },
            "effective_max_input_length": 128_000
        })
    );

    let models = http_request(
        address,
        "GET /api/models HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    let models = response_json(&models);
    assert_eq!(models[0]["id"], json!("openai-compatible"));
    assert_eq!(models[0]["models"][0]["id"], json!("qwen-test"));
    assert_eq!(models[0]["api_key"], json!(""));
    assert_eq!(models[0]["base_url"], json!("http://127.0.0.1:1"));
}

async fn assert_model_write_contract(
    address: SocketAddr,
    credentials: &Arc<MemoryCredentialStore>,
) {
    let client = reqwest::Client::new();
    let configured = client
        .put(format!(
            "http://{address}/api/models/openai-compatible/config"
        ))
        .json(&json!({
            "api_key": "new-secret-key",
            "base_url": "https://model.example.test/v1"
        }))
        .send()
        .await
        .expect("provider configuration should send")
        .json::<Value>()
        .await
        .expect("provider configuration should be JSON");
    assert_eq!(configured["api_key"], json!("********"));
    assert_eq!(
        credentials
            .load_api_key()
            .expect("test credential should load"),
        Some(String::from("new-secret-key"))
    );
    let listed = client
        .get(format!("http://{address}/api/models"))
        .send()
        .await
        .expect("provider list should send")
        .text()
        .await
        .expect("provider list should read");
    assert!(!listed.contains("new-secret-key"));
    assert!(listed.contains("********"));

    let added = client
        .post(format!(
            "http://{address}/api/models/openai-compatible/models"
        ))
        .json(&json!({"id": "qwen-next", "name": "Qwen Next"}))
        .send()
        .await
        .expect("model add should send")
        .json::<Value>()
        .await
        .expect("model add should be JSON");
    assert_eq!(added["models"][0]["id"], json!("qwen-next"));

    let active = client
        .put(format!("http://{address}/api/models/active"))
        .json(&json!({
            "provider_id": "openai-compatible",
            "model": "qwen-next",
            "scope": "agent",
            "agent_id": "default"
        }))
        .send()
        .await
        .expect("active model update should send")
        .json::<Value>()
        .await
        .expect("active model update should be JSON");
    assert_eq!(
        active,
        json!({
            "active_llm": {
                "provider_id": "openai-compatible",
                "model": "qwen-next"
            },
            "effective_max_input_length": 128_000
        })
    );

    let disabled = client
        .put(format!(
            "http://{address}/api/models/openai-compatible/config"
        ))
        .json(&json!({"api_key": ""}))
        .send()
        .await
        .expect("provider disable should send")
        .json::<Value>()
        .await
        .expect("provider disable should be JSON");
    assert_eq!(disabled["api_key"], json!(""));
    assert_eq!(
        credentials
            .load_api_key()
            .expect("test credential should load"),
        None
    );
}

async fn assert_chat_contract(address: SocketAddr, thread_id: &str) {
    let chats = http_request(
        address,
        "GET /api/chats?archived=false&include_app_owned=false HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    let chats = response_json(&chats);
    let chats = chats.as_array().expect("chat response should be an array");
    assert_eq!(chats.len(), 1);
    assert_eq!(chats[0]["id"], json!(thread_id));
    assert_eq!(chats[0]["session_id"], chats[0]["id"]);
    assert_eq!(chats[0]["user_id"], json!("desktop"));
    assert_eq!(chats[0]["channel"], json!("console"));
    assert_eq!(chats[0]["status"], json!("idle"));
    assert_eq!(chats[0]["archived"], json!(false));
    assert!(
        chats[0]["created_at"]
            .as_str()
            .is_some_and(|value| value.ends_with('Z'))
    );

    let history = http_request(
        address,
        &format!(
            "GET /api/chats/{thread_id}?include_app_owned=false HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
        ),
    )
    .await;
    assert_eq!(
        response_json(&history),
        json!({"messages": [], "status": "idle"})
    );

    let archive = http_request(
        address,
        &format!(
            "POST /api/chats/{thread_id}/archive HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        ),
    )
    .await;
    assert_eq!(response_json(&archive)["archived"], json!(true));

    let unarchive = http_request(
        address,
        &format!(
            "POST /api/chats/{thread_id}/unarchive HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        ),
    )
    .await;
    assert_eq!(response_json(&unarchive)["archived"], json!(false));

    let missing = http_request(
        address,
        "GET /api/chats/missing HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(missing.starts_with("HTTP/1.1 404 Not Found"));
    assert_eq!(
        response_json(&missing),
        json!({"detail": "thread not found: missing"})
    );
}

async fn assert_workspace_contract(address: SocketAddr, thread_id: &str) {
    let client = reqwest::Client::new();
    let directory = tempfile::tempdir().expect("temporary Workspace should be created");
    let selected = directory.path().join("selected");
    let rebound = directory.path().join("rebound");
    std::fs::create_dir(&selected).expect("selected Workspace should be created");
    std::fs::create_dir(&rebound).expect("rebound Workspace should be created");
    std::fs::create_dir(selected.join("visible")).expect("visible directory should be created");
    std::fs::create_dir(selected.join(".hidden")).expect("hidden directory should be created");
    let selected = assert_global_workspace_contract(&client, address, &selected).await;
    assert_chat_workspace_contract(&client, address, thread_id, &selected, &rebound).await;
    assert_workspace_rejections(&client, address, thread_id, &selected, &rebound).await;
}

async fn assert_global_workspace_contract(
    client: &reqwest::Client,
    address: SocketAddr,
    selected: &std::path::Path,
) -> PathBuf {
    let selected_info = client
        .put(format!("http://{address}/api/workspace/project-directory"))
        .json(&json!({"path": selected}))
        .send()
        .await
        .expect("Workspace selection should send")
        .json::<Value>()
        .await
        .expect("Workspace selection should be JSON");
    let selected = selected
        .canonicalize()
        .expect("selected Workspace should resolve");
    assert_eq!(selected_info["path"], json!(selected.to_string_lossy()));
    assert_eq!(selected_info["exists"], json!(true));
    assert_eq!(selected_info["is_workspace_default"], json!(false));

    let projects = client
        .get(format!(
            "http://{address}/api/workspace/project-directory/list"
        ))
        .send()
        .await
        .expect("Workspace list should send")
        .json::<Value>()
        .await
        .expect("Workspace list should be JSON");
    assert!(projects.as_array().is_some_and(|projects| {
        projects.iter().any(|project| {
            project["path"] == json!(selected.to_string_lossy())
                && project["is_active"] == json!(true)
        })
    }));

    let mut browse_url = reqwest::Url::parse(&format!(
        "http://{address}/api/workspace/project-directory/browse-dirs"
    ))
    .expect("Workspace browse URL should parse");
    browse_url
        .query_pairs_mut()
        .append_pair("path", &selected.to_string_lossy());
    let browsed = client
        .get(browse_url)
        .send()
        .await
        .expect("Workspace browse should send")
        .json::<Value>()
        .await
        .expect("Workspace browse should be JSON");
    assert_eq!(browsed["current"], json!(selected.to_string_lossy()));
    assert_eq!(browsed["dirs"].as_array().map(Vec::len), Some(1));
    assert_eq!(browsed["dirs"][0]["name"], json!("visible"));

    let created = client
        .post(format!(
            "http://{address}/api/workspace/project-directory/browse-dirs/create"
        ))
        .json(&json!({"parent": selected, "name": "created"}))
        .send()
        .await
        .expect("directory create should send")
        .json::<Value>()
        .await
        .expect("directory create should be JSON");
    assert_eq!(created["name"], json!("created"));
    assert!(selected.join("created").is_dir());
    selected
}

async fn assert_chat_workspace_contract(
    client: &reqwest::Client,
    address: SocketAddr,
    thread_id: &str,
    selected: &std::path::Path,
    rebound: &std::path::Path,
) {
    let rebound_response = client
        .put(format!(
            "http://{address}/api/chats/{thread_id}/project-dirs"
        ))
        .json(&json!({
            "project_dirs": [{"path": rebound, "label": "Rebound"}]
        }))
        .send()
        .await
        .expect("chat Workspace rebind should send")
        .json::<Value>()
        .await
        .expect("chat Workspace rebind should be JSON");
    let rebound = rebound
        .canonicalize()
        .expect("rebound Workspace should resolve");
    assert_eq!(
        rebound_response["project_dirs"][0]["path"],
        json!(rebound.to_string_lossy())
    );
    let singular = client
        .get(format!(
            "http://{address}/api/chats/{thread_id}/project-dir"
        ))
        .send()
        .await
        .expect("chat Workspace read should send")
        .json::<Value>()
        .await
        .expect("chat Workspace read should be JSON");
    assert_eq!(singular["project_dir"], json!(rebound.to_string_lossy()));

    let cleared = client
        .delete(format!(
            "http://{address}/api/chats/{thread_id}/project-dirs"
        ))
        .send()
        .await
        .expect("chat Workspace clear should send")
        .json::<Value>()
        .await
        .expect("chat Workspace clear should be JSON");
    assert_eq!(
        cleared["project_dirs"][0]["path"],
        json!(selected.to_string_lossy())
    );
}

async fn assert_workspace_rejections(
    client: &reqwest::Client,
    address: SocketAddr,
    thread_id: &str,
    first: &std::path::Path,
    second: &std::path::Path,
) {
    let invalid_create = client
        .post(format!(
            "http://{address}/api/workspace/project-directory/browse-dirs/create"
        ))
        .json(&json!({"parent": first, "name": "../escape"}))
        .send()
        .await
        .expect("invalid directory create should send");
    assert_eq!(invalid_create.status(), reqwest::StatusCode::BAD_REQUEST);

    let multiple = json!({
        "project_dirs": [{"path": first}, {"path": second}]
    });
    let chat_rebind = client
        .put(format!(
            "http://{address}/api/chats/{thread_id}/project-dirs"
        ))
        .json(&multiple)
        .send()
        .await
        .expect("multi-Workspace rebind should send");
    assert_eq!(chat_rebind.status(), reqwest::StatusCode::NOT_IMPLEMENTED);

    let chat = client
        .post(format!("http://{address}/api/console/chat"))
        .json(&json!({
            "input": [{"role": "user", "content": "Do not start"}],
            "session_id": "1700000000003-multiple",
            "request_context": {"session_project_dirs": multiple["project_dirs"]}
        }))
        .send()
        .await
        .expect("multi-Workspace chat should send");
    assert_eq!(chat.status(), reqwest::StatusCode::NOT_IMPLEMENTED);
}

async fn send_json(socket: &mut ClientSocket, value: Value) {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .expect("WebSocket request should send");
}

async fn receive_json(socket: &mut ClientSocket) -> Value {
    let message = socket
        .next()
        .await
        .expect("server should send a response")
        .expect("WebSocket response should be valid");
    serde_json::from_str(message.to_text().expect("response should be text"))
        .expect("response should be JSON")
}

async fn http_request(address: SocketAddr, request: &str) -> String {
    let mut stream = tokio::net::TcpStream::connect(address)
        .await
        .expect("HTTP client should connect");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("HTTP request should write");
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .expect("HTTP response should read");
    String::from_utf8(response).expect("HTTP response should be UTF-8")
}

fn response_json(response: &str) -> Value {
    let (_, body) = response
        .split_once("\r\n\r\n")
        .expect("HTTP response should contain a body separator");
    serde_json::from_str(body).expect("HTTP response body should be JSON")
}

fn parse_sse_events(body: &str) -> Vec<Value> {
    body.split("\n\n")
        .filter_map(|frame| frame.strip_prefix("data: "))
        .map(|payload| serde_json::from_str(payload).expect("SSE data should be JSON"))
        .collect()
}

async fn wait_for_pending_approval(client: &reqwest::Client, address: SocketAddr) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let push_messages = client
                .get(format!("http://{address}/api/console/push-messages"))
                .send()
                .await
                .expect("push messages request should send")
                .json::<Value>()
                .await
                .expect("push messages response should be JSON");
            if let Some(approval) = push_messages["pending_approvals"]
                .as_array()
                .and_then(|approvals| approvals.first())
            {
                return approval.clone();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("pending approval should appear before timeout")
}

async fn start_model_server() -> String {
    let app = axum::Router::new().route(
        "/chat/completions",
        axum::routing::post(|| async {
            axum::response::Response::builder()
                .status(axum::http::StatusCode::OK)
                .header(axum::http::header::CONTENT_TYPE, "text/event-stream")
                .body(axum::body::Body::from(concat!(
                    "data: {\"choices\":[{\"delta\":{\"content\":\"Hello \"}}]}\n\n",
                    "data: {\"choices\":[{\"delta\":{\"content\":\"from QwenPaw\"}}]}\n\n",
                    "data: [DONE]\n\n"
                )))
                .expect("mock model response should build")
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock model listener should bind");
    let address = listener
        .local_addr()
        .expect("mock model listener should have an address");
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("mock model server should run");
    });
    format!("http://{address}")
}

async fn start_delayed_model_server() -> String {
    let app = axum::Router::new().route(
        "/chat/completions",
        axum::routing::post(|| async {
            tokio::time::sleep(Duration::from_secs(30)).await;
            axum::response::Response::builder()
                .status(axum::http::StatusCode::OK)
                .header(axum::http::header::CONTENT_TYPE, "text/event-stream")
                .body(axum::body::Body::from("data: [DONE]\n\n"))
                .expect("mock model response should build")
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock model listener should bind");
    let address = listener
        .local_addr()
        .expect("mock model listener should have an address");
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("mock model server should run");
    });
    format!("http://{address}")
}

async fn start_tool_model_server() -> String {
    let requests = Arc::new(AtomicUsize::new(0));
    let app = axum::Router::new().route(
        "/chat/completions",
        axum::routing::post(move || {
            let requests = Arc::clone(&requests);
            async move {
                let response = if requests.fetch_add(1, Ordering::SeqCst) == 0 {
                    String::from(concat!(
                        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_shell\",\"function\":{\"name\":\"shell\",\"arguments\":\"{\\\"command\\\":\\\"must-not-run\\\"}\"}}]}}]}\n\n",
                        "data: [DONE]\n\n"
                    ))
                } else {
                    String::from(concat!(
                        "data: {\"choices\":[{\"delta\":{\"content\":\"Denied safely\"}}]}\n\n",
                        "data: [DONE]\n\n"
                    ))
                };
                axum::response::Response::builder()
                    .status(axum::http::StatusCode::OK)
                    .header(axum::http::header::CONTENT_TYPE, "text/event-stream")
                    .body(axum::body::Body::from(response))
                    .expect("mock model response should build")
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock model listener should bind");
    let address = listener
        .local_addr()
        .expect("mock model listener should have an address");
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("mock model server should run");
    });
    format!("http://{address}")
}
