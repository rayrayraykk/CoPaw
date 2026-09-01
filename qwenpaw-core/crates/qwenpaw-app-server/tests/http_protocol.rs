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
    assert_eq!(initialized["result"]["protocolVersion"], json!(3));

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
        version.contains("{\"backend\":\"rust-core\",\"protocolVersion\":3,\"version\":\"0.2.0\"}")
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
    let desktop_data = tempfile::tempdir().expect("temporary Desktop data should be created");
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
    let server = AppServer::new_desktop_with_credential_store_and_data_dir(
        core,
        console.path(),
        String::from("desktop-bootstrap-token"),
        credentials.clone(),
        desktop_data.path(),
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
    let desktop_data = tempfile::tempdir().expect("temporary Desktop data should be created");
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
    let server = AppServer::new_desktop_with_credential_store_and_data_dir(
        core,
        console.path(),
        String::from("desktop-stream-token"),
        Arc::new(MemoryCredentialStore::default()),
        desktop_data.path(),
    )
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
    assert_attachment_chat_contract(&client, address, &workspace).await;
    task.abort();
}

async fn assert_attachment_chat_contract(
    client: &reqwest::Client,
    address: SocketAddr,
    workspace: &std::path::Path,
) {
    let upload = multipart_request(
        client,
        format!("http://{address}/api/console/upload"),
        "file",
        "brief.txt",
        b"attachment body",
    )
    .await;
    assert_eq!(upload.status(), reqwest::StatusCode::OK);
    let upload = upload
        .json::<Value>()
        .await
        .expect("attachment upload should be JSON");
    assert_eq!(upload["file_name"], json!("brief.txt"));
    let stored_name = upload["url"]
        .as_str()
        .expect("attachment upload should return a stored name");

    let preview = client
        .get(format!(
            "http://{address}/api/files/preview/{stored_name}?token=ignored"
        ))
        .send()
        .await
        .expect("attachment preview should send");
    assert_eq!(preview.status(), reqwest::StatusCode::OK);
    assert_eq!(
        preview
            .bytes()
            .await
            .expect("attachment preview should read")
            .as_ref(),
        b"attachment body"
    );

    let response = client
        .post(format!("http://{address}/api/console/chat"))
        .json(&json!({
            "input": [{
                "role": "user",
                "content": [
                    {"type": "text", "text": "Read the attachment"},
                    {
                        "type": "file",
                        "file_url": stored_name,
                        "file_name": "brief.txt"
                    }
                ]
            }],
            "session_id": "1700000000000-local",
            "stream": true,
            "request_context": {
                "session_project_dirs": [{"path": workspace}]
            }
        }))
        .send()
        .await
        .expect("attachment chat should send");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let events = parse_sse_events(
        &response
            .text()
            .await
            .expect("attachment chat stream should read"),
    );
    assert_eq!(
        events.last().expect("attachment chat should complete")["status"],
        json!("completed")
    );
    assert_eq!(
        std::fs::read(
            workspace
                .join(".qwenpaw")
                .join("attachments")
                .join(stored_name)
        )
        .expect("attachment should be copied into the Workspace"),
        b"attachment body"
    );
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
            json!({"upload_max_size_mb": 32}),
        ),
        (
            "/api/coding-mode",
            json!({"enabled": false, "agent_id": "default"}),
        ),
        (
            "/api/loops",
            json!([{
                "id": "default",
                "name": "default",
                "slash_command": "",
                "description": "The standard guarded agent loop.",
                "source": "builtin",
                "name_i18n": null,
                "description_i18n": null
            }]),
        ),
        (
            "/api/loops/status?session_id=new",
            json!({"state": "idle", "mode": null}),
        ),
        ("/api/skills", json!([])),
        (
            "/api/workspace/running-config",
            json!({"approval_level": "AUTO"}),
        ),
        (
            "/api/workspace/transcription-provider-type",
            json!({"transcription_provider_type": "disabled"}),
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
    assert_workspace_file_contract(&client, address, &selected).await;
    assert_chat_workspace_contract(&client, address, thread_id, &selected, &rebound).await;
    assert_workspace_rejections(&client, address, thread_id, &selected, &rebound).await;
}

async fn assert_workspace_file_contract(
    client: &reqwest::Client,
    address: SocketAddr,
    selected: &std::path::Path,
) {
    std::fs::write(selected.join("notes.md"), "héllo")
        .expect("Workspace text fixture should be written");
    std::fs::write(selected.join("page.html"), "<h1>QwenPaw</h1>")
        .expect("Workspace HTML fixture should be written");
    std::fs::write(selected.join("binary.bin"), [0_u8, 1, 2, 3])
        .expect("Workspace binary fixture should be written");
    assert_workspace_tree_and_content(client, address, selected).await;
    assert_workspace_upload_and_rejections(client, address, selected).await;
    assert_workspace_watch_contract(client, address, selected).await;
}

async fn assert_workspace_tree_and_content(
    client: &reqwest::Client,
    address: SocketAddr,
    selected: &std::path::Path,
) {
    assert_workspace_tree(client, address).await;
    assert_workspace_text_content(client, address, selected).await;
    assert_workspace_binary_and_html(client, address).await;
}

async fn assert_workspace_tree(client: &reqwest::Client, address: SocketAddr) {
    let first_page = client
        .get(format!(
            "http://{address}/api/workspace/tree?path=&root=project&limit=2"
        ))
        .send()
        .await
        .expect("Workspace tree should send")
        .json::<Value>()
        .await
        .expect("Workspace tree should be JSON");
    assert_eq!(first_page["entries"].as_array().map(Vec::len), Some(2));
    assert_eq!(first_page["has_more"], json!(true));
    let cursor = first_page["next_cursor"]
        .as_str()
        .expect("Workspace tree should return a cursor");
    let second_page = client
        .get(format!(
            "http://{address}/api/workspace/tree?path=&root=project&limit=20&cursor={cursor}"
        ))
        .send()
        .await
        .expect("Workspace tree continuation should send")
        .json::<Value>()
        .await
        .expect("Workspace tree continuation should be JSON");
    assert_eq!(second_page["has_more"], json!(false));
}

async fn assert_workspace_text_content(
    client: &reqwest::Client,
    address: SocketAddr,
    selected: &std::path::Path,
) {
    let metadata = client
        .get(format!(
            "http://{address}/api/workspace/file-metadata?path=notes.md&root=project"
        ))
        .send()
        .await
        .expect("Workspace metadata should send")
        .json::<Value>()
        .await
        .expect("Workspace metadata should be JSON");
    assert_eq!(metadata["path"], json!("notes.md"));
    assert_eq!(metadata["size"], json!(6));
    assert_eq!(metadata["preview_kind"], json!("text"));

    let content = client
        .get(format!(
            "http://{address}/api/workspace/file-content?path=notes.md&root=project&offset=0&limit=3"
        ))
        .send()
        .await
        .expect("Workspace content should send")
        .json::<Value>()
        .await
        .expect("Workspace content should be JSON");
    assert_eq!(content["content"], json!("hé"));
    assert_eq!(content["next_offset"], json!(3));
    assert_eq!(content["eof"], json!(false));
    let etag = content["etag"]
        .as_str()
        .expect("Workspace content should include an ETag");
    let unicode_chunk = client
        .get(format!(
            "http://{address}/api/workspace/file-content?path=notes.md&root=project&offset=1&limit=1"
        ))
        .send()
        .await
        .expect("Workspace Unicode chunk should send")
        .json::<Value>()
        .await
        .expect("Workspace Unicode chunk should be JSON");
    assert_eq!(unicode_chunk["content"], json!("é"));
    assert_eq!(unicode_chunk["next_offset"], json!(3));

    let saved = client
        .put(format!(
            "http://{address}/api/workspace/file-content?path=notes.md&root=project"
        ))
        .header(reqwest::header::IF_MATCH, etag)
        .json(&json!({"content": "updated"}))
        .send()
        .await
        .expect("Workspace save should send");
    assert_eq!(saved.status(), reqwest::StatusCode::OK);
    assert_eq!(
        std::fs::read_to_string(selected.join("notes.md"))
            .expect("saved Workspace file should read"),
        "updated"
    );
    let stale = client
        .put(format!(
            "http://{address}/api/workspace/file-content?path=notes.md&root=project"
        ))
        .header(reqwest::header::IF_MATCH, etag)
        .json(&json!({"content": "must not win"}))
        .send()
        .await
        .expect("stale Workspace save should send");
    assert_eq!(stale.status(), reqwest::StatusCode::PRECONDITION_FAILED);
}

async fn assert_workspace_binary_and_html(client: &reqwest::Client, address: SocketAddr) {
    let download = client
        .get(format!(
            "http://{address}/api/workspace/file-download?path=binary.bin&root=project"
        ))
        .send()
        .await
        .expect("Workspace download should send");
    assert_eq!(download.status(), reqwest::StatusCode::OK);
    assert_eq!(
        download
            .bytes()
            .await
            .expect("Workspace download should read")
            .as_ref(),
        [0_u8, 1, 2, 3]
    );

    let html = client
        .get(format!(
            "http://{address}/api/workspace/html-file-uri?path=page.html&root=project"
        ))
        .send()
        .await
        .expect("Workspace HTML resolver should send")
        .json::<Value>()
        .await
        .expect("Workspace HTML resolver should be JSON");
    assert!(
        html["uri"]
            .as_str()
            .is_some_and(|uri| uri.starts_with("file:"))
    );
}

async fn assert_workspace_upload_and_rejections(
    client: &reqwest::Client,
    address: SocketAddr,
    selected: &std::path::Path,
) {
    let uploaded = multipart_request(
        client,
        format!("http://{address}/api/workspace/file-upload?path=&root=project"),
        "files",
        "upload.txt",
        b"first upload",
    )
    .await;
    assert_eq!(uploaded.status(), reqwest::StatusCode::OK);
    assert_eq!(
        std::fs::read(selected.join("upload.txt")).expect("upload should be written"),
        b"first upload"
    );
    let conflict = multipart_request(
        client,
        format!("http://{address}/api/workspace/file-upload?path=&root=project"),
        "files",
        "upload.txt",
        b"second upload",
    )
    .await;
    assert_eq!(conflict.status(), reqwest::StatusCode::CONFLICT);
    let renamed = multipart_request(
        client,
        format!("http://{address}/api/workspace/file-upload?path=&root=project&conflict=rename"),
        "files",
        "upload.txt",
        b"second upload",
    )
    .await;
    assert_eq!(renamed.status(), reqwest::StatusCode::OK);
    assert_eq!(
        std::fs::read(selected.join("upload (1).txt")).expect("renamed upload should be written"),
        b"second upload"
    );

    let traversal = client
        .get(format!(
            "http://{address}/api/workspace/file-content?path=../outside.txt&root=project"
        ))
        .send()
        .await
        .expect("Workspace traversal request should send");
    assert_eq!(traversal.status(), reqwest::StatusCode::BAD_REQUEST);
}

async fn assert_workspace_watch_contract(
    client: &reqwest::Client,
    address: SocketAddr,
    selected: &std::path::Path,
) {
    let mut response = client
        .get(format!("http://{address}/api/workspace/watch?root=project"))
        .send()
        .await
        .expect("Workspace watch should connect");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        response.headers()[reqwest::header::CONTENT_TYPE],
        "text/event-stream"
    );
    std::fs::write(selected.join("watch.txt"), "watched").expect("watched file should be written");
    let payload = tokio::time::timeout(Duration::from_secs(10), async {
        let mut buffer = String::new();
        loop {
            let chunk = response
                .chunk()
                .await
                .expect("Workspace watch chunk should read")
                .expect("Workspace watch should remain open");
            buffer.push_str(
                std::str::from_utf8(&chunk).expect("Workspace watch should return UTF-8 SSE"),
            );
            if let Some(payload) = find_workspace_event(&buffer, "watch.txt") {
                return payload;
            }
        }
    })
    .await
    .expect("Workspace watch should report a change before timeout");
    assert_eq!(payload["type"], json!("file_change"));
}

fn find_workspace_event(buffer: &str, expected_path: &str) -> Option<Value> {
    buffer.split("\n\n").find_map(|frame| {
        let data = frame.lines().find_map(|line| line.strip_prefix("data: "))?;
        let payload = serde_json::from_str::<Value>(data).ok()?;
        payload["events"]
            .as_array()
            .is_some_and(|events| events.iter().any(|event| event["path"] == expected_path))
            .then_some(payload)
    })
}

async fn multipart_request(
    client: &reqwest::Client,
    url: String,
    field: &str,
    file_name: &str,
    contents: &[u8],
) -> reqwest::Response {
    let boundary = "qwenpaw-test-boundary";
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"{field}\"; filename=\"{file_name}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(contents);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    client
        .post(url)
        .header(
            reqwest::header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(body)
        .send()
        .await
        .expect("multipart request should send")
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
