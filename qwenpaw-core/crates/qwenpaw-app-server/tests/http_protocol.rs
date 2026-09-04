use futures_util::SinkExt;
use futures_util::StreamExt;
use qwenpaw_app_server::AppServer;
use qwenpaw_app_server::DesktopCredentialStore;
use qwenpaw_core::Core;
use qwenpaw_core::ModelConfig;
use qwenpaw_protocol::ThreadStartParams;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;
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
    environment: Mutex<BTreeMap<String, String>>,
    agent_settings: Mutex<BTreeMap<String, String>>,
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

    fn load_environment_value(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self
            .environment
            .lock()
            .expect("test environment credential lock should be available")
            .get(key)
            .cloned())
    }

    fn save_environment_value(&self, key: &str, value: Option<&str>) -> anyhow::Result<()> {
        let mut environment = self
            .environment
            .lock()
            .expect("test environment credential lock should be available");
        match value {
            Some(value) => {
                environment.insert(key.to_owned(), value.to_owned());
            }
            None => {
                environment.remove(key);
            }
        }
        Ok(())
    }

    fn load_agent_setting_secret(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self
            .agent_settings
            .lock()
            .expect("test Agent credential lock should be available")
            .get(key)
            .cloned())
    }

    fn save_agent_setting_secret(&self, key: &str, value: Option<&str>) -> anyhow::Result<()> {
        let mut settings = self
            .agent_settings
            .lock()
            .expect("test Agent credential lock should be available");
        match value {
            Some(value) => {
                settings.insert(key.to_owned(), value.to_owned());
            }
            None => {
                settings.remove(key);
            }
        }
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
    core.write_mail_access_control_data(
        &json!({
            "version": 1,
            "agents": {
                "default": {
                    "whitelist": {},
                    "blacklist": {},
                    "pending": [
                        {
                            "sender_address": "approve@example.com",
                            "agent_id": "default",
                            "display_name": "Approved Sender",
                            "subject": "approve me",
                            "body_preview": "approval preview",
                            "timestamp": 3.0,
                            "remark": "pending approve",
                            "uid": 101,
                            "date": "2026-09-04",
                            "messages": [{"uid": 101, "subject": "approve me"}]
                        },
                        {
                            "sender_address": "deny@example.com",
                            "agent_id": "default",
                            "display_name": "Denied Sender",
                            "subject": "deny me",
                            "body_preview": "denial preview",
                            "timestamp": 2.0,
                            "remark": "pending deny"
                        },
                        {
                            "sender_address": "dismiss@example.com",
                            "agent_id": "default",
                            "timestamp": 1.0
                        }
                    ],
                    "approved_replay": []
                }
            }
        })
        .to_string(),
    )
    .expect("mail access-control fixture should persist");
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
    assert_language_write_contract(address).await;
    assert_navigation_json_contracts(address).await;
    assert_agent_contract(address).await;
    assert_model_contract(address).await;
    assert_model_write_contract(address, &credentials).await;
    assert_environment_contract(address).await;
    assert_access_control_contract(address).await;
    assert_mail_access_control_contract(address).await;
    assert_channel_contract(address).await;
    assert_cron_contract(address).await;
    assert_chat_contract(address, &thread.id).await;
    assert_workspace_contract(address, &thread.id).await;

    task.abort();
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn creates_imports_uploads_clones_and_persists_projects() {
    let console = tempfile::tempdir().expect("temporary Console should be created");
    let desktop_data = tempfile::tempdir().expect("temporary Desktop data should be created");
    let workspace = tempfile::tempdir().expect("temporary Workspace should be created");
    std::fs::write(console.path().join("index.html"), "<html>console</html>")
        .expect("Console index should be written");
    let database = desktop_data.path().join("threads.sqlite3");
    let model_config = ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("qwen-test"),
    };
    let first_core =
        Core::persistent(model_config.clone(), &database).expect("first Core should open");
    let first_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("first Desktop listener should bind");
    let first_address = first_listener
        .local_addr()
        .expect("first Desktop listener should have an address");
    let first_server = AppServer::new_desktop_with_stores_and_workspace(
        first_core,
        console.path(),
        String::from("desktop-project-first-token"),
        Arc::new(MemoryCredentialStore::default()),
        desktop_data.path(),
        workspace.path(),
    )
    .expect("first Desktop server should configure");
    let first_task = tokio::spawn(first_server.run_http(first_listener));
    let client = reqwest::Client::new();
    let base = format!("http://{first_address}/api/workspace/project-directory");

    let created = client
        .post(format!("{base}/create"))
        .json(&json!({"name": "created project"}))
        .send()
        .await
        .expect("project create should send");
    assert_eq!(created.status(), reqwest::StatusCode::OK);
    let created = created
        .json::<Value>()
        .await
        .expect("project create should return JSON");
    let created_path = PathBuf::from(
        created["path"]
            .as_str()
            .expect("project create should return a path"),
    );
    assert_eq!(created["name"], json!("created project"));
    assert!(created_path.join(".git").is_dir());
    for invalid in ["../escape", "CON.txt", "trailing."] {
        let response = client
            .post(format!("{base}/create"))
            .json(&json!({"name": invalid}))
            .send()
            .await
            .expect("invalid project create should send");
        assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    }

    let home = dirs::home_dir().expect("test user should have a home directory");
    let import_parent = tempfile::Builder::new()
        .prefix("qwenpaw-project-import-")
        .tempdir_in(home)
        .expect("temporary import source should be created under home");
    let import_source = import_parent.path().join("source");
    std::fs::create_dir_all(import_source.join("node_modules"))
        .expect("excluded build directory should be created");
    std::fs::create_dir_all(import_source.join(".config/gh"))
        .expect("sensitive nested directory should be created");
    std::fs::write(import_source.join("keep.txt"), "keep")
        .expect("import fixture should be written");
    std::fs::write(import_source.join(".env"), "SECRET=value")
        .expect("sensitive fixture should be written");
    std::fs::write(import_source.join("node_modules/skip.js"), "skip")
        .expect("build fixture should be written");
    std::fs::write(import_source.join(".config/gh/hosts.yml"), "token")
        .expect("nested sensitive fixture should be written");
    #[cfg(unix)]
    std::os::unix::fs::symlink("keep.txt", import_source.join("linked.txt"))
        .expect("import symlink fixture should be created");
    let imported = client
        .post(format!("{base}/import-local"))
        .json(&json!({"path": import_source, "name": "imported"}))
        .send()
        .await
        .expect("local import should send");
    assert_eq!(imported.status(), reqwest::StatusCode::OK);
    let imported = imported
        .json::<Value>()
        .await
        .expect("local import should return JSON");
    let imported_path = PathBuf::from(
        imported["path"]
            .as_str()
            .expect("local import should return a path"),
    );
    assert_eq!(
        std::fs::read(imported_path.join("keep.txt")).unwrap(),
        b"keep"
    );
    assert!(!imported_path.join(".env").exists());
    assert!(!imported_path.join("node_modules").exists());
    assert!(!imported_path.join(".config/gh").exists());
    assert!(!imported_path.join("linked.txt").exists());
    assert_eq!(imported["excluded"], json!([".config/gh", ".env"]));
    let sensitive_source = import_parent.path().join(".ssh/project");
    std::fs::create_dir_all(&sensitive_source).expect("sensitive import source should be created");
    let sensitive_import = client
        .post(format!("{base}/import-local"))
        .json(&json!({"path": sensitive_source, "name": "sensitive"}))
        .send()
        .await
        .expect("sensitive local import should send");
    assert_eq!(sensitive_import.status(), reqwest::StatusCode::FORBIDDEN);

    let zip = make_project_zip(&[("src/main.rs", b"fn main() {}")]);
    let uploaded = multipart_request(
        &client,
        format!("{base}/upload-zip?name=uploaded"),
        "file",
        "uploaded.zip",
        &zip,
    )
    .await;
    assert_eq!(uploaded.status(), reqwest::StatusCode::OK);
    let uploaded = uploaded
        .json::<Value>()
        .await
        .expect("ZIP upload should return JSON");
    let uploaded_path = PathBuf::from(
        uploaded["path"]
            .as_str()
            .expect("ZIP upload should return a path"),
    );
    assert_eq!(
        std::fs::read(uploaded_path.join("src/main.rs")).unwrap(),
        b"fn main() {}"
    );
    assert!(uploaded_path.join(".git").is_dir());

    let traversal_zip = make_project_zip(&[("../escape.txt", b"escape")]);
    let traversal = multipart_request(
        &client,
        format!("{base}/upload-zip?name=unsafe"),
        "file",
        "unsafe.zip",
        &traversal_zip,
    )
    .await;
    assert_eq!(traversal.status(), reqwest::StatusCode::BAD_REQUEST);
    assert!(!workspace.path().join("coding_projects/escape.txt").exists());

    let symlink_zip = make_symlink_zip();
    let symlink = multipart_request(
        &client,
        format!("{base}/upload-zip?name=symlink"),
        "file",
        "symlink.zip",
        &symlink_zip,
    )
    .await;
    assert_eq!(symlink.status(), reqwest::StatusCode::BAD_REQUEST);

    let oversized_zip = make_oversized_metadata_zip();
    let oversized = multipart_request(
        &client,
        format!("{base}/upload-zip?name=oversized"),
        "file",
        "oversized.zip",
        &oversized_zip,
    )
    .await;
    assert_eq!(oversized.status(), reqwest::StatusCode::PAYLOAD_TOO_LARGE);

    let clone_source = workspace.path().join("clone-source");
    run_test_git(&["init", clone_source.to_str().unwrap()]);
    std::fs::write(clone_source.join("README.md"), "clone me")
        .expect("clone fixture should be written");
    run_test_git(&[
        "-C",
        clone_source.to_str().unwrap(),
        "-c",
        "user.email=qwenpaw@localhost",
        "-c",
        "user.name=QwenPaw",
        "add",
        "README.md",
    ]);
    run_test_git(&[
        "-C",
        clone_source.to_str().unwrap(),
        "-c",
        "user.email=qwenpaw@localhost",
        "-c",
        "user.name=QwenPaw",
        "commit",
        "-m",
        "fixture",
    ]);
    let cloned = client
        .post(format!("{base}/clone"))
        .json(&json!({
            "url": clone_source.to_string_lossy(),
            "name": "cloned"
        }))
        .send()
        .await
        .expect("project clone should send");
    assert_eq!(cloned.status(), reqwest::StatusCode::OK);
    assert_eq!(
        cloned.headers()[reqwest::header::CONTENT_TYPE],
        "text/event-stream"
    );
    let clone_events = parse_sse_events(
        &cloned
            .text()
            .await
            .expect("project clone stream should read"),
    );
    assert_eq!(
        clone_events.last().expect("clone should finish")["type"],
        json!("done")
    );
    let cloned_path = workspace
        .path()
        .join("coding_projects/cloned")
        .canonicalize()
        .expect("cloned project should resolve");
    assert_eq!(
        std::fs::read(cloned_path.join("README.md")).unwrap(),
        b"clone me"
    );

    let failed = client
        .post(format!("{base}/clone"))
        .json(&json!({
            "url": workspace.path().join("missing-repository").to_string_lossy(),
            "name": "failed-clone"
        }))
        .send()
        .await
        .expect("failed project clone should send");
    let failed_events = parse_sse_events(
        &failed
            .text()
            .await
            .expect("failed project clone stream should read"),
    );
    assert_eq!(
        failed_events.last().expect("failed clone should finish")["type"],
        json!("error")
    );
    let active = get_json(&client, base.clone()).await;
    assert_eq!(active["path"], json!(cloned_path.to_string_lossy()));
    let projects = get_json(&client, format!("{base}/list")).await;
    assert_eq!(
        projects,
        json!([
            {
                "path": cloned_path.to_string_lossy(),
                "name": "cloned",
                "is_git": true,
                "is_active": true
            },
            {
                "path": created_path.to_string_lossy(),
                "name": "created project",
                "is_git": true,
                "is_active": false
            },
            {
                "path": imported_path.to_string_lossy(),
                "name": "imported",
                "is_git": false,
                "is_active": false
            },
            {
                "path": uploaded_path.to_string_lossy(),
                "name": "uploaded",
                "is_git": true,
                "is_active": false
            }
        ])
    );

    first_task.abort();
    let _ = first_task.await;
    let second_core = Core::persistent(model_config, &database).expect("second Core should open");
    let second_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("second Desktop listener should bind");
    let second_address = second_listener
        .local_addr()
        .expect("second Desktop listener should have an address");
    let second_server = AppServer::new_desktop_with_stores_and_workspace(
        second_core,
        console.path(),
        String::from("desktop-project-second-token"),
        Arc::new(MemoryCredentialStore::default()),
        desktop_data.path(),
        workspace.path(),
    )
    .expect("second Desktop server should configure");
    let second_task = tokio::spawn(second_server.run_http(second_listener));
    let restarted = get_json(
        &client,
        format!("http://{second_address}/api/workspace/project-directory"),
    )
    .await;
    assert_eq!(restarted["path"], json!(cloned_path.to_string_lossy()));
    second_task.abort();
}

fn make_project_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;

    let cursor = std::io::Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(cursor);
    for (name, contents) in entries {
        writer
            .start_file(*name, zip::write::SimpleFileOptions::default())
            .expect("ZIP fixture entry should start");
        writer
            .write_all(contents)
            .expect("ZIP fixture entry should write");
    }
    writer
        .finish()
        .expect("ZIP fixture should finish")
        .into_inner()
}

fn make_symlink_zip() -> Vec<u8> {
    let cursor = std::io::Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(cursor);
    writer
        .add_symlink(
            "linked.txt",
            "../outside.txt",
            zip::write::SimpleFileOptions::default(),
        )
        .expect("ZIP symlink fixture should write");
    writer
        .finish()
        .expect("ZIP symlink fixture should finish")
        .into_inner()
}

fn make_oversized_metadata_zip() -> Vec<u8> {
    let mut archive = make_project_zip(&[("large.txt", b"x")]);
    let oversized = (512_u32 * 1024 * 1024 + 1).to_le_bytes();
    for (signature, offset) in [
        ([0x50, 0x4b, 0x03, 0x04], 22),
        ([0x50, 0x4b, 0x01, 0x02], 24),
    ] {
        if let Some(index) = archive
            .windows(signature.len())
            .position(|candidate| candidate == signature)
        {
            archive[index + offset..index + offset + 4].copy_from_slice(&oversized);
        }
    }
    archive
}

fn run_test_git(arguments: &[&str]) {
    let status = std::process::Command::new("git")
        .args(arguments)
        .env("GIT_TERMINAL_PROMPT", "0")
        .status()
        .expect("test Git command should start");
    assert!(status.success(), "test Git command failed: {arguments:?}");
}

#[tokio::test]
async fn serves_and_persists_the_inbox_event_contracts() {
    let console = tempfile::tempdir().expect("temporary Console should be created");
    let desktop_data = tempfile::tempdir().expect("temporary Desktop data should be created");
    let database = desktop_data.path().join("threads.sqlite3");
    std::fs::write(console.path().join("index.html"), "<html>console</html>")
        .expect("Console index should be written");
    let model_config = ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("qwen-test"),
    };
    let first_core =
        Core::persistent(model_config.clone(), &database).expect("first Core should open");
    first_core
        .write_inbox_data(&inbox_fixture().to_string())
        .expect("Inbox fixture should persist");
    first_core
        .write_mail_access_control_data(
            &json!({
                "version": 1,
                "agents": {
                    "default": {
                        "whitelist": {},
                        "blacklist": {},
                        "pending": [{
                            "sender_address": "pending@example.com",
                            "agent_id": "default",
                            "timestamp": 1.0
                        }],
                        "approved_replay": []
                    }
                }
            })
            .to_string(),
        )
        .expect("mail fixture should persist");
    let first_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("first Desktop listener should bind");
    let first_address = first_listener
        .local_addr()
        .expect("first Desktop listener should have an address");
    let first_server = AppServer::new_desktop_with_credential_store_and_data_dir(
        first_core,
        console.path(),
        String::from("desktop-inbox-first-token"),
        Arc::new(MemoryCredentialStore::default()),
        desktop_data.path(),
    )
    .expect("first Desktop server should configure");
    let first_task = tokio::spawn(first_server.run_http(first_listener));
    let client = reqwest::Client::new();
    assert_inbox_query_and_read_contract(&client, first_address).await;
    assert_inbox_delete_and_validation_contract(&client, first_address).await;
    first_task.abort();
    let _ = first_task.await;

    let second_core = Core::persistent(model_config, &database).expect("second Core should open");
    let second_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("second Desktop listener should bind");
    let second_address = second_listener
        .local_addr()
        .expect("second Desktop listener should have an address");
    let second_server = AppServer::new_desktop_with_credential_store_and_data_dir(
        second_core,
        console.path(),
        String::from("desktop-inbox-second-token"),
        Arc::new(MemoryCredentialStore::default()),
        desktop_data.path(),
    )
    .expect("second Desktop server should configure");
    let second_task = tokio::spawn(second_server.run_http(second_listener));
    let mut persisted_mail = inbox_fixture()["events"][3].clone();
    persisted_mail["read"] = json!(true);
    assert_eq!(
        get_json(
            &client,
            format!("http://{second_address}/api/console/inbox/events"),
        )
        .await,
        json!({
            "events": [inbox_fixture()["events"][2], persisted_mail],
            "total": 2,
            "unread_count": 0
        })
    );
    assert_eq!(
        get_json(
            &client,
            format!("http://{second_address}/api/console/inbox/traces/run-heartbeat"),
        )
        .await,
        inbox_fixture()["traces"]["run-heartbeat"]
    );
    second_task.abort();
}

async fn assert_inbox_query_and_read_contract(client: &reqwest::Client, address: SocketAddr) {
    let base = format!("http://{address}/api/console/inbox");
    assert_eq!(
        get_json(client, format!("{base}/events")).await,
        json!({
            "events": inbox_fixture()["events"],
            "total": 4,
            "unread_count": 3
        })
    );
    assert_eq!(
        get_json(
            client,
            format!(
                "{base}/events?source_types=cron&source_types=memory&unread_only=true&limit=1&offset=1"
            ),
        )
        .await,
        json!({
            "events": [inbox_fixture()["events"][1]],
            "total": 2,
            "unread_count": 2
        })
    );
    assert_eq!(
        get_json(
            client,
            format!("{base}/events?status=success&agent_id=researcher"),
        )
        .await,
        json!({
            "events": [inbox_fixture()["events"][1]],
            "total": 1,
            "unread_count": 1
        })
    );
    assert_eq!(
        get_json(client, format!("{base}/traces/run-shared")).await,
        inbox_fixture()["traces"]["run-shared"]
    );
    assert_eq!(
        post_json(
            client,
            format!("{base}/read"),
            json!({"event_ids": ["event-cron", "event-cron"]}),
        )
        .await,
        json!({"updated": 1})
    );
    assert_eq!(
        post_json(
            client,
            format!("http://{address}/api/mail-access-control/pending/dismiss"),
            json!({"entries": [{
                "agent_id": "default",
                "address": "PENDING@example.com"
            }]}),
        )
        .await,
        json!({"status": "ok", "count": 1})
    );
    assert_eq!(
        get_json(client, format!("{base}/events?unread_only=true&limit=500")).await,
        json!({
            "events": [inbox_fixture()["events"][1]],
            "total": 1,
            "unread_count": 1
        })
    );
    assert_eq!(
        post_json(client, format!("{base}/read"), json!({"all": true})).await,
        json!({"updated": 1})
    );
    assert_eq!(
        post_json(client, format!("{base}/read"), json!({"all": true})).await,
        json!({"updated": 0})
    );
}

async fn assert_inbox_delete_and_validation_contract(
    client: &reqwest::Client,
    address: SocketAddr,
) {
    let base = format!("http://{address}/api/console/inbox");
    let first_delete = client
        .delete(format!("{base}/events/event-cron"))
        .send()
        .await
        .expect("first Inbox delete should send");
    assert_eq!(first_delete.status(), reqwest::StatusCode::OK);
    assert_eq!(
        first_delete
            .json::<Value>()
            .await
            .expect("first Inbox delete should be JSON"),
        json!({
            "deleted": true,
            "trace_deleted": false,
            "run_id": "run-shared"
        })
    );
    assert_eq!(
        get_json(client, format!("{base}/traces/run-shared")).await,
        inbox_fixture()["traces"]["run-shared"]
    );
    assert_eq!(
        client
            .delete(format!("{base}/events/event-memory"))
            .send()
            .await
            .expect("second Inbox delete should send")
            .json::<Value>()
            .await
            .expect("second Inbox delete should be JSON"),
        json!({
            "deleted": true,
            "trace_deleted": true,
            "run_id": "run-shared"
        })
    );
    for (url, expected) in [
        (
            format!("{base}/traces/run-shared"),
            reqwest::StatusCode::NOT_FOUND,
        ),
        (
            format!("{base}/events?limit=0"),
            reqwest::StatusCode::UNPROCESSABLE_ENTITY,
        ),
    ] {
        assert_eq!(
            client
                .get(url)
                .send()
                .await
                .expect("invalid Inbox request should send")
                .status(),
            expected
        );
    }
    assert_eq!(
        client
            .delete(format!("{base}/events/missing"))
            .send()
            .await
            .expect("missing Inbox delete should send")
            .status(),
        reqwest::StatusCode::NOT_FOUND
    );
}

fn inbox_fixture() -> Value {
    json!({
        "version": 1,
        "events": [
            {
                "id": "event-cron",
                "agent_id": "default",
                "source_type": "cron",
                "source_id": "daily-digest",
                "event_type": "cron_result",
                "status": "success",
                "severity": "info",
                "title": "Cron result: Daily digest",
                "body": "Daily summary",
                "payload": {"run_id": "run-shared", "trigger": "manual"},
                "read": false,
                "created_at": 40.0
            },
            {
                "id": "event-memory",
                "agent_id": "researcher",
                "source_type": "memory",
                "source_id": "memory-job",
                "event_type": "memory_result",
                "status": "success",
                "severity": "info",
                "title": "Memory result",
                "body": "Memory summary",
                "payload": {"run_id": "run-shared"},
                "read": false,
                "created_at": 30.0
            },
            {
                "id": "event-heartbeat",
                "agent_id": "default",
                "source_type": "heartbeat",
                "source_id": "heartbeat",
                "event_type": "heartbeat_result",
                "status": "success",
                "severity": "info",
                "title": "Heartbeat result",
                "body": "Heartbeat task finished successfully.",
                "payload": {"run_id": "run-heartbeat"},
                "read": true,
                "created_at": 20.0
            },
            {
                "id": "event-mail-pending",
                "agent_id": "default",
                "source_type": "mail",
                "source_id": "pending@example.com",
                "event_type": "new_email",
                "status": "success",
                "severity": "info",
                "title": "New email",
                "body": "Pending mail",
                "payload": {
                    "acl_status": "pending",
                    "acl_sender_address": "pending@example.com"
                },
                "read": false,
                "created_at": 10.0
            }
        ],
        "traces": {
            "run-shared": {
                "run_id": "run-shared",
                "created_at": 1.0,
                "completed_at": 2.0,
                "status": "success",
                "meta": {"kind": "shared"},
                "events": [{"at": 1.5, "event": {"role": "assistant", "content": "Trace result"}}]
            },
            "run-heartbeat": {
                "run_id": "run-heartbeat",
                "created_at": 3.0,
                "completed_at": 4.0,
                "status": "success",
                "meta": {"kind": "heartbeat"},
                "events": []
            }
        }
    })
}

async fn assert_language_write_contract(address: SocketAddr) {
    let client = reqwest::Client::new();
    let url = format!("http://{address}/api/settings/language");
    let updated = client
        .put(&url)
        .json(&json!({"language": "pt-BR"}))
        .send()
        .await
        .expect("language update should send");
    assert_eq!(updated.status(), reqwest::StatusCode::OK);
    assert_eq!(
        updated
            .json::<Value>()
            .await
            .expect("language update should be JSON"),
        json!({"language": "pt-BR"})
    );
    assert_eq!(
        get_json(&client, url.clone()).await,
        json!({"language": "pt-BR"})
    );

    let rejected = client
        .put(url)
        .json(&json!({"language": "invalid"}))
        .send()
        .await
        .expect("invalid language update should send");
    assert_eq!(rejected.status(), reqwest::StatusCode::BAD_REQUEST);
    assert_eq!(
        rejected
            .json::<Value>()
            .await
            .expect("invalid language response should be JSON"),
        json!({
            "detail": concat!(
                "configuration is invalid: UI language must be one of: ",
                "en, zh, ja, ru, pt-BR, id, vi"
            )
        })
    );
}

async fn assert_environment_contract(address: SocketAddr) {
    let client = reqwest::Client::new();
    let url = format!("http://{address}/api/envs");
    let saved = client
        .put(&url)
        .json(&json!({
            "SECOND_VALUE": "two",
            "FIRST_VALUE": "one"
        }))
        .send()
        .await
        .expect("environment update should send");
    assert_eq!(saved.status(), reqwest::StatusCode::OK);
    assert_eq!(
        saved
            .json::<Value>()
            .await
            .expect("environment update should be JSON"),
        json!([
            {"key": "FIRST_VALUE", "value": "one"},
            {"key": "SECOND_VALUE", "value": "two"}
        ])
    );
    assert_eq!(
        get_json(&client, url.clone()).await,
        json!([
            {"key": "FIRST_VALUE", "value": "one"},
            {"key": "SECOND_VALUE", "value": "two"}
        ])
    );

    let invalid = client
        .put(&url)
        .json(&json!({"INVALID-NAME": "value"}))
        .send()
        .await
        .expect("invalid environment update should send");
    assert_eq!(invalid.status(), reqwest::StatusCode::BAD_REQUEST);
    assert_eq!(
        invalid
            .json::<Value>()
            .await
            .expect("invalid environment response should be JSON"),
        json!({
            "detail": concat!(
                "configuration is invalid: environment variable name is invalid: ",
                "INVALID-NAME"
            )
        })
    );

    let deleted = client
        .delete(format!("{url}/FIRST_VALUE"))
        .send()
        .await
        .expect("environment delete should send");
    assert_eq!(deleted.status(), reqwest::StatusCode::OK);
    assert_eq!(
        deleted
            .json::<Value>()
            .await
            .expect("environment delete should be JSON"),
        json!([{"key": "SECOND_VALUE", "value": "two"}])
    );

    let missing = client
        .delete(format!("{url}/MISSING"))
        .send()
        .await
        .expect("missing environment delete should send");
    assert_eq!(missing.status(), reqwest::StatusCode::NOT_FOUND);
    assert_eq!(
        missing
            .json::<Value>()
            .await
            .expect("missing environment response should be JSON"),
        json!({"detail": "Env var 'MISSING' not found"})
    );

    let cleared = client
        .put(url)
        .json(&json!({}))
        .send()
        .await
        .expect("environment clear should send");
    assert_eq!(cleared.status(), reqwest::StatusCode::OK);
    assert_eq!(
        cleared
            .json::<Value>()
            .await
            .expect("environment clear should be JSON"),
        json!([])
    );
}

async fn assert_cron_contract(address: SocketAddr) {
    let client = reqwest::Client::new();
    let collection_url = format!("http://{address}/api/cron/jobs");
    let created_response = client
        .post(&collection_url)
        .json(&json!({
            "id": "client-id-is-ignored",
            "name": "Console reminder",
            "schedule": {
                "type": "cron",
                "cron": "9 * * 0",
                "timezone": "UTC"
            },
            "task_type": "text",
            "text": "Time to stretch",
            "save_result_to_inbox": true,
            "dispatch": {
                "target": {
                    "user_id": "admin",
                    "session_id": "cron-ui-session"
                }
            }
        }))
        .send()
        .await
        .expect("cron create should send");
    assert_eq!(created_response.status(), reqwest::StatusCode::OK);
    let created = created_response
        .json::<Value>()
        .await
        .expect("cron create should be JSON");
    let job_id = created["id"]
        .as_str()
        .expect("created cron should have an id")
        .to_owned();
    assert_ne!(job_id, "client-id-is-ignored");
    assert_eq!(created["schedule"]["cron"], json!("0 9 * * sun"));
    assert_eq!(created["enabled"], json!(true));
    assert_eq!(created["save_result_to_inbox"], json!(true));
    assert_eq!(created["runtime"]["timeout_seconds"], json!(120));
    assert_eq!(
        get_json(&client, collection_url.clone()).await,
        json!([created])
    );

    let item_url = format!("{collection_url}/{job_id}");
    let view = get_json(&client, item_url.clone()).await;
    assert_eq!(view["spec"]["name"], json!("Console reminder"));
    assert_eq!(
        view["state"],
        json!({
            "next_run_at": null,
            "last_run_at": null,
            "last_status": null,
            "last_error": null
        })
    );

    assert_cron_execution_contract(&client, address, &collection_url, &item_url).await;
    assert_invalid_cron_is_rejected(&client, &collection_url).await;

    let deleted = client
        .delete(item_url)
        .send()
        .await
        .expect("cron delete should send");
    assert_eq!(deleted.status(), reqwest::StatusCode::OK);
    assert_eq!(get_json(&client, collection_url).await, json!([]));
}

async fn assert_access_control_contract(address: SocketAddr) {
    let client = reqwest::Client::new();
    let base = format!("http://{address}/api/access-control");
    assert_eq!(
        get_json(&client, base.clone()).await,
        json!({
            "console": {"whitelist": {}, "blacklist": {}, "pending": []}
        })
    );
    assert_eq!(
        get_json(&client, format!("{base}/telegram")).await,
        json!({"whitelist": {}, "blacklist": {}, "pending": []})
    );

    assert_access_control_list_mutations(&client, &base).await;
    assert_access_control_pending_actions(&client, &base).await;
}

async fn assert_mail_access_control_contract(address: SocketAddr) {
    let client = reqwest::Client::new();
    let base = format!("http://{address}/api/mail-access-control");
    assert_mail_pending_contract(&client, &base).await;
    assert_mail_list_contract(&client, &base).await;
}

async fn assert_mail_pending_contract(client: &reqwest::Client, base: &str) {
    assert_eq!(
        get_json(client, format!("{base}/agents")).await,
        json!({"agents": ["default"]})
    );
    let initial = get_json(client, base.to_owned()).await;
    assert_eq!(initial["default"]["whitelist"], json!({}));
    assert_eq!(
        initial["default"]["pending"].as_array().map(Vec::len),
        Some(3)
    );
    let pending = get_json(client, format!("{base}/pending/all")).await;
    assert_eq!(pending[0]["sender_address"], json!("approve@example.com"));
    assert_eq!(pending[2]["sender_address"], json!("dismiss@example.com"));
    assert_eq!(
        get_json(client, format!("{base}/pending/count")).await,
        json!({"count": 3})
    );

    assert_eq!(
        post_json(
            client,
            format!("{base}/pending/remark"),
            json!({
                "agent_id": "default",
                "address": "APPROVE@example.com",
                "remark": "reviewed"
            }),
        )
        .await,
        json!({"status": "ok"})
    );
    for (action, address) in [
        ("approve", "approve@example.com"),
        ("deny", "deny@example.com"),
        ("dismiss", "dismiss@example.com"),
    ] {
        assert_eq!(
            post_json(
                client,
                format!("{base}/pending/{action}"),
                json!({"entries": [{"agent_id": "default", "address": address}]}),
            )
            .await,
            json!({"status": "ok", "count": 1})
        );
    }
    let moved = get_json(client, base.to_owned()).await;
    assert_eq!(
        moved["default"]["whitelist"]["approve@example.com"],
        json!({"remark": "reviewed", "display_name": "Approved Sender"})
    );
    assert_eq!(
        moved["default"]["blacklist"]["deny@example.com"],
        json!({"remark": "pending deny", "display_name": "Denied Sender"})
    );
    assert_eq!(moved["default"]["pending"], json!([]));
    assert_eq!(
        moved["default"]["approved_replay"][0]["sender_address"],
        json!("approve@example.com")
    );
}

async fn assert_mail_list_contract(client: &reqwest::Client, base: &str) {
    assert_eq!(
        post_json(
            client,
            format!("{base}/whitelist/add"),
            json!({"entries": [
                {
                    "agent_id": "",
                    "address": "Friend@Example.com",
                    "display_name": "Friend",
                    "remark": "trusted"
                },
                {"agent_id": "", "address": "*@trusted.example.com"}
            ]}),
        )
        .await,
        json!({"status": "ok", "count": 2})
    );
    assert_eq!(
        post_json(
            client,
            format!("{base}/remark"),
            json!({
                "agent_id": "default",
                "address": "friend@example.com",
                "remark": "best friend"
            }),
        )
        .await,
        json!({"status": "ok"})
    );
    assert_eq!(
        post_json(
            client,
            format!("{base}/blacklist/add"),
            json!({"entries": [{
                "agent_id": "default",
                "address": "friend@example.com",
                "display_name": "Blocked Friend"
            }]}),
        )
        .await,
        json!({"status": "ok", "count": 1})
    );
    let listed = get_json(client, base.to_owned()).await;
    assert!(listed["default"]["whitelist"]["friend@example.com"].is_null());
    assert_eq!(
        listed["default"]["blacklist"]["friend@example.com"],
        json!({"remark": "", "display_name": "Blocked Friend"})
    );

    let invalid = client
        .post(format!("{base}/whitelist/add"))
        .json(&json!({"entries": [
            {"agent_id": "default", "address": "valid@example.com"},
            {"agent_id": "default", "address": "not-an-email"}
        ]}))
        .send()
        .await
        .expect("invalid mail address request should send");
    assert_eq!(invalid.status(), reqwest::StatusCode::BAD_REQUEST);
    assert!(
        get_json(client, base.to_owned()).await["default"]["whitelist"]["valid@example.com"]
            .is_null()
    );
    assert_eq!(
        post_json(
            client,
            format!("{base}/whitelist/add"),
            json!({"entries": [{
                "agent_id": "missing",
                "address": "ignored@example.com"
            }]}),
        )
        .await,
        json!({"status": "ok", "count": 0})
    );

    let missing = client
        .post(format!("{base}/pending/remark"))
        .json(&json!({
            "agent_id": "default",
            "address": "missing@example.com",
            "remark": "missing"
        }))
        .send()
        .await
        .expect("missing mail pending remark request should send");
    assert_eq!(missing.status(), reqwest::StatusCode::NOT_FOUND);
}

async fn assert_channel_contract(address: SocketAddr) {
    let client = reqwest::Client::new();
    let channels = get_json(&client, format!("http://{address}/api/config/channels")).await;
    let channels = channels
        .as_object()
        .expect("channel list should be an object");
    assert_eq!(channels.len(), 18);
    assert_eq!(channels["console"]["enabled"], json!(true));
    assert_eq!(channels["console"]["isBuiltin"], json!(true));
    assert_eq!(
        channels["imessage"]["db_path"],
        json!("~/Library/Messages/chat.db")
    );
    assert_eq!(channels["onebot"]["ws_host"], json!("127.0.0.1"));

    assert_eq!(
        get_json(
            &client,
            format!("http://{address}/api/config/channels/types"),
        )
        .await,
        json!([
            "imessage",
            "discord",
            "dingtalk",
            "feishu",
            "qq",
            "telegram",
            "mattermost",
            "mqtt",
            "console",
            "matrix",
            "slack",
            "voice",
            "sip",
            "wecom",
            "xiaoyi",
            "yuanbao",
            "wechat",
            "onebot"
        ])
    );
    assert_eq!(
        get_json(
            &client,
            format!("http://{address}/api/config/channels/schemas"),
        )
        .await,
        json!({})
    );
    let console_url = format!("http://{address}/api/config/channels/console");
    let mut console = get_json(&client, console_url.clone()).await;
    console["enabled"] = json!(false);
    console["bot_prefix"] = json!("[core] ");
    let updated = client
        .put(&console_url)
        .json(&console)
        .send()
        .await
        .expect("Console channel update should send");
    assert_eq!(updated.status(), reqwest::StatusCode::OK);
    let updated = updated
        .json::<Value>()
        .await
        .expect("Console channel update should be JSON");
    assert_eq!(updated["enabled"], json!(true));
    assert_eq!(updated["bot_prefix"], json!("[core] "));
    assert_eq!(get_json(&client, console_url).await, updated);

    assert_eq!(
        post_json(
            &client,
            format!("http://{address}/api/config/channels/console/conflict-check"),
            json!({"enabled": true}),
        )
        .await,
        json!({"conflict": false, "agents": []})
    );
    let unsupported = client
        .put(format!("http://{address}/api/config/channels/telegram"))
        .json(&json!({"enabled": true, "bot_token": "must-not-persist"}))
        .send()
        .await
        .expect("unsupported channel update should send");
    assert_eq!(unsupported.status(), reqwest::StatusCode::NOT_IMPLEMENTED);
    assert_eq!(
        unsupported
            .json::<Value>()
            .await
            .expect("unsupported response should be JSON"),
        json!({
            "detail": "Rust runtime for channel 'telegram' is not implemented"
        })
    );
}

async fn assert_access_control_list_mutations(client: &reqwest::Client, base: &str) {
    let added = post_json(
        client,
        format!("{base}/whitelist/add"),
        json!({"entries": [{
            "channel": "console",
            "user_id": "alice",
            "remark": "owner",
            "username": "Alice"
        }]}),
    )
    .await;
    assert_eq!(added, json!({"status": "ok", "count": 1}));

    assert_eq!(
        post_json(
            client,
            format!("{base}/remark"),
            json!({"channel": "console", "user_id": "alice", "remark": "admin"}),
        )
        .await,
        json!({"status": "ok"})
    );
    assert_eq!(
        post_json(
            client,
            format!("{base}/username"),
            json!({"channel": "console", "user_id": "alice", "username": "Alice A."}),
        )
        .await,
        json!({"status": "ok"})
    );
    assert_eq!(
        get_json(client, format!("{base}/console")).await,
        json!({
            "whitelist": {"alice": {"remark": "admin", "username": "Alice A."}},
            "blacklist": {},
            "pending": []
        })
    );

    for (list, action) in [("blacklist", "add"), ("blacklist", "remove")] {
        assert_eq!(
            post_json(
                client,
                format!("{base}/{list}/{action}"),
                json!({"entries": [{"channel": "telegram", "user_id": "blocked"}]}),
            )
            .await,
            json!({"status": "ok", "count": 1})
        );
    }
    assert_eq!(
        post_json(
            client,
            format!("{base}/whitelist/remove"),
            json!({"entries": [{"channel": "console", "user_id": "alice"}]}),
        )
        .await,
        json!({"status": "ok", "count": 1})
    );
}

async fn assert_access_control_pending_actions(client: &reqwest::Client, base: &str) {
    for (action, user_id, expected_list) in [
        ("approve", "approved", "whitelist"),
        ("deny", "denied", "blacklist"),
    ] {
        assert_eq!(
            post_json(
                client,
                format!("{base}/pending/{action}"),
                json!({"entries": [{
                    "channel": "console",
                    "user_id": user_id,
                    "remark": action
                }]}),
            )
            .await,
            json!({"status": "ok", "count": 1})
        );
        assert_eq!(
            get_json(client, format!("{base}/console")).await[expected_list][user_id]["remark"],
            json!(action)
        );
    }
    assert_eq!(
        post_json(
            client,
            format!("{base}/pending/dismiss"),
            json!({"entries": [{"channel": "console", "user_id": "missing"}]}),
        )
        .await,
        json!({"status": "ok", "count": 1})
    );
    assert_eq!(
        get_json(client, format!("{base}/pending/all")).await,
        json!([])
    );

    let missing = client
        .post(format!("{base}/pending/remark"))
        .json(&json!({
            "channel": "console",
            "user_id": "missing",
            "remark": "none"
        }))
        .send()
        .await
        .expect("missing pending remark request should send");
    assert_eq!(missing.status(), reqwest::StatusCode::NOT_FOUND);
}

async fn assert_cron_execution_contract(
    client: &reqwest::Client,
    address: SocketAddr,
    collection_url: &str,
    item_url: &str,
) {
    let paused = client
        .post(format!("{item_url}/pause"))
        .send()
        .await
        .expect("cron pause should send");
    assert_eq!(paused.status(), reqwest::StatusCode::OK);
    assert_eq!(
        get_json(client, collection_url.to_owned()).await[0]["enabled"],
        json!(false)
    );
    let resumed = client
        .post(format!("{item_url}/resume"))
        .send()
        .await
        .expect("cron resume should send");
    assert_eq!(resumed.status(), reqwest::StatusCode::OK);

    let run = client
        .post(format!("{item_url}/run"))
        .send()
        .await
        .expect("cron run should send");
    assert_eq!(run.status(), reqwest::StatusCode::OK);
    let messages = get_json(
        client,
        format!("http://{address}/api/console/push-messages?session_id=cron-ui-session"),
    )
    .await;
    assert_eq!(messages["messages"].as_array().map(Vec::len), Some(1));
    assert_eq!(messages["messages"][0]["text"], json!("Time to stretch"));
    assert_eq!(
        get_json(client, format!("{item_url}/state")).await["last_status"],
        json!("success")
    );
    let history = get_json(client, format!("{item_url}/history")).await;
    assert_eq!(history.as_array().map(Vec::len), Some(1));
    assert_eq!(history[0]["status"], json!("success"));
    assert_eq!(history[0]["trigger"], json!("manual"));
    let inbox = get_json(
        client,
        format!("http://{address}/api/console/inbox/events?source_type=cron"),
    )
    .await;
    let event_id = inbox["events"][0]["id"].clone();
    let created_at = inbox["events"][0]["created_at"].clone();
    let job_id = inbox["events"][0]["source_id"].clone();
    assert_eq!(
        inbox,
        json!({
            "events": [{
                "id": event_id,
                "agent_id": "default",
                "source_type": "cron",
                "source_id": job_id,
                "event_type": "cron_result",
                "status": "success",
                "severity": "info",
                "title": "Cron result: Console reminder",
                "body": "Time to stretch",
                "payload": {
                    "job_id": job_id,
                    "job_name": "Console reminder",
                    "task_type": "text",
                    "trigger": "manual",
                    "run_id": null,
                    "save_result_to_inbox": true
                },
                "read": false,
                "created_at": created_at
            }],
            "total": 1,
            "unread_count": 1
        })
    );
}

async fn assert_invalid_cron_is_rejected(client: &reqwest::Client, collection_url: &str) {
    let invalid = client
        .post(collection_url)
        .json(&json!({
            "name": "Invalid",
            "schedule": {"type": "cron", "cron": "0 0 0 1 1 1"},
            "task_type": "text",
            "text": "invalid",
            "dispatch": {"target": {"user_id": "admin", "session_id": "invalid"}}
        }))
        .send()
        .await
        .expect("invalid cron create should send");
    assert_eq!(invalid.status(), reqwest::StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn serves_and_persists_chat_catalog_management_contracts() {
    let console = tempfile::tempdir().expect("temporary Console should be created");
    let desktop_data = tempfile::tempdir().expect("temporary Desktop data should be created");
    let database = desktop_data.path().join("threads.sqlite3");
    let workspace = desktop_data.path().join("workspace");
    std::fs::create_dir(&workspace).expect("Workspace should be created");
    let workspace = workspace
        .canonicalize()
        .expect("Workspace should canonicalize");
    std::fs::write(console.path().join("index.html"), "<html>console</html>")
        .expect("Console index should be written");
    let model_config = ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("qwen-test"),
    };
    let first_core =
        Core::persistent(model_config.clone(), &database).expect("first Core should open");
    let first_thread = first_core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(workspace.to_string_lossy().into_owned()),
        })
        .await
        .expect("first thread should start")
        .thread;
    let second_thread = first_core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(workspace.to_string_lossy().into_owned()),
        })
        .await
        .expect("second thread should start")
        .thread;
    let first_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("first Desktop listener should bind");
    let first_address = first_listener
        .local_addr()
        .expect("first Desktop listener should have an address");
    let first_server = AppServer::new_desktop_with_credential_store_and_data_dir(
        first_core,
        console.path(),
        String::from("desktop-chats-first-token"),
        Arc::new(MemoryCredentialStore::default()),
        desktop_data.path(),
    )
    .expect("first Desktop server should configure");
    let first_task = tokio::spawn(first_server.run_http(first_listener));
    let client = reqwest::Client::new();
    let first_base = format!("http://{first_address}/api/chats");

    let chats = get_json(
        &client,
        format!("{first_base}?archived=false&user_id=desktop&channel=console"),
    )
    .await;
    assert_eq!(chats.as_array().map(Vec::len), Some(2));
    let first_spec = chats
        .as_array()
        .and_then(|items| items.iter().find(|item| item["id"] == first_thread.id))
        .expect("first thread should be listed");
    assert_eq!(first_spec["name"], json!("New Chat"));
    assert_eq!(first_spec["group_id"], json!("default"));
    assert_eq!(first_spec["pinned"], json!(false));
    assert_eq!(first_spec["meta"]["workspace_root"], json!(workspace));

    assert_eq!(
        get_json(&client, format!("{first_base}/groups")).await,
        json!([
            {
                "id": "default",
                "name": "Uncategorized",
                "order": 0,
                "kind": "default",
                "source": "chat",
                "pinned": false
            },
            {
                "id": "cron",
                "name": "Scheduled tasks",
                "order": 1,
                "kind": "cron",
                "source": "cron",
                "pinned": false
            },
            {
                "id": "subagents",
                "name": "Subagents",
                "order": 2,
                "kind": "subagents",
                "source": "subagent",
                "pinned": false
            }
        ])
    );
    let custom = post_json(
        &client,
        format!("{first_base}/groups"),
        json!({"name": "  Project Alpha  "}),
    )
    .await;
    let custom_id = custom["id"]
        .as_str()
        .expect("custom group should have an ID")
        .to_owned();
    assert_eq!(
        custom,
        json!({
            "id": custom_id,
            "name": "Project Alpha",
            "order": 3,
            "kind": "custom",
            "source": null,
            "pinned": false
        })
    );
    let renamed_group = client
        .put(format!("{first_base}/groups/{custom_id}"))
        .json(&json!({"name": "Important", "pinned": true}))
        .send()
        .await
        .expect("group update should send");
    assert_eq!(renamed_group.status(), reqwest::StatusCode::OK);
    assert_eq!(
        renamed_group
            .json::<Value>()
            .await
            .expect("group update should be JSON"),
        json!({
            "id": custom_id,
            "name": "Important",
            "order": 3,
            "kind": "custom",
            "source": null,
            "pinned": true
        })
    );
    let reordered = client
        .put(format!("{first_base}/groups/order"))
        .json(&json!({
            "group_ids": [custom_id, "default", "cron", "subagents"]
        }))
        .send()
        .await
        .expect("group reorder should send");
    assert_eq!(reordered.status(), reqwest::StatusCode::OK);
    assert_eq!(
        reordered
            .json::<Value>()
            .await
            .expect("group reorder should be JSON")
            .as_array()
            .expect("groups should be an array")
            .iter()
            .map(|group| group["id"].clone())
            .collect::<Vec<_>>(),
        vec![
            json!(custom_id),
            json!("default"),
            json!("cron"),
            json!("subagents")
        ]
    );

    let updated = client
        .put(format!("{first_base}/{}", first_thread.id))
        .json(&json!({
            "name": "Renamed conversation",
            "pinned": true,
            "group_id": custom_id
        }))
        .send()
        .await
        .expect("chat update should send");
    assert_eq!(updated.status(), reqwest::StatusCode::OK);
    let updated = updated
        .json::<Value>()
        .await
        .expect("chat update should be JSON");
    assert_eq!(updated["name"], json!("Renamed conversation"));
    assert_eq!(updated["pinned"], json!(true));
    assert_eq!(updated["group_id"], json!(custom_id));

    let archived = post_json(
        &client,
        format!("{first_base}/actions/batch-archive"),
        json!({"chat_ids": [first_thread.id, "missing"]}),
    )
    .await;
    assert_eq!(
        archived,
        json!({
            "succeeded": [first_thread.id],
            "failed": [{
                "chat_id": "missing",
                "reason": "not_found",
                "message": "Chat not found: missing"
            }]
        })
    );
    assert_eq!(
        get_json(&client, format!("{first_base}?archived=true")).await[0]["id"],
        json!(first_thread.id)
    );
    assert_eq!(
        post_json(
            &client,
            format!("{first_base}/actions/batch-unarchive"),
            json!({"chat_ids": [first_thread.id, "missing"]}),
        )
        .await,
        json!({
            "succeeded": [first_thread.id],
            "failed": [{
                "chat_id": "missing",
                "reason": "not_found",
                "message": "Chat not found: missing"
            }]
        })
    );
    first_task.abort();
    let _ = first_task.await;

    let second_core = Core::persistent(model_config, &database).expect("second Core should open");
    let second_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("second Desktop listener should bind");
    let second_address = second_listener
        .local_addr()
        .expect("second Desktop listener should have an address");
    let second_server = AppServer::new_desktop_with_credential_store_and_data_dir(
        second_core,
        console.path(),
        String::from("desktop-chats-second-token"),
        Arc::new(MemoryCredentialStore::default()),
        desktop_data.path(),
    )
    .expect("second Desktop server should configure");
    let second_task = tokio::spawn(second_server.run_http(second_listener));
    let second_base = format!("http://{second_address}/api/chats");
    let restarted = get_json(&client, format!("{second_base}?archived=false")).await;
    let restarted_spec = restarted
        .as_array()
        .and_then(|items| items.iter().find(|item| item["id"] == first_thread.id))
        .expect("updated thread should survive restart");
    assert_eq!(restarted_spec["name"], json!("Renamed conversation"));
    assert_eq!(restarted_spec["pinned"], json!(true));
    assert_eq!(restarted_spec["group_id"], json!(custom_id));
    assert_eq!(
        get_json(&client, format!("{second_base}/groups")).await[0]["id"],
        json!(custom_id)
    );

    let deleted_group = client
        .delete(format!("{second_base}/groups/{custom_id}"))
        .send()
        .await
        .expect("group delete should send");
    assert_eq!(deleted_group.status(), reqwest::StatusCode::OK);
    assert_eq!(
        deleted_group
            .json::<Value>()
            .await
            .expect("group delete should be JSON"),
        json!({"success": true, "group_id": custom_id})
    );
    let rehomed = get_json(&client, format!("{second_base}?archived=false")).await;
    assert_eq!(
        rehomed
            .as_array()
            .and_then(|items| items.iter().find(|item| item["id"] == first_thread.id))
            .expect("re-homed thread should remain listed")["group_id"],
        json!("default")
    );

    let created = post_json(
        &client,
        second_base.clone(),
        json!({
            "session_id": "created-session",
            "user_id": "desktop",
            "channel": "console",
            "name": "Created through HTTP",
            "meta": {"runtime_context": {"project_dir": workspace}},
            "source": "chat"
        }),
    )
    .await;
    let created_id = created["id"]
        .as_str()
        .expect("created chat should have an ID")
        .to_owned();
    assert_eq!(created["session_id"], json!("created-session"));
    assert_eq!(created["name"], json!("Created through HTTP"));
    assert_eq!(created["group_id"], json!("default"));

    assert_eq!(
        post_json(
            &client,
            format!("{second_base}/batch-delete"),
            json!([first_thread.id, "missing"]),
        )
        .await,
        json!({"deleted": true})
    );
    let deleted_read = client
        .get(format!("{second_base}/{}", first_thread.id))
        .send()
        .await
        .expect("deleted chat read should send");
    assert_eq!(deleted_read.status(), reqwest::StatusCode::NOT_FOUND);
    let single_deleted = client
        .delete(format!("{second_base}/{created_id}"))
        .send()
        .await
        .expect("single chat delete should send");
    assert_eq!(single_deleted.status(), reqwest::StatusCode::OK);
    assert_eq!(
        single_deleted
            .json::<Value>()
            .await
            .expect("single delete should be JSON"),
        json!({"deleted": true})
    );
    assert_eq!(
        get_json(&client, format!("{second_base}?archived=false")).await,
        json!([{
            "id": second_thread.id,
            "session_id": second_thread.id,
            "user_id": "desktop",
            "channel": "console",
            "name": "New Chat",
            "created_at": chrono::DateTime::from_timestamp(second_thread.created_at, 0)
                .map(|value| value.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
            "updated_at": chrono::DateTime::from_timestamp(second_thread.updated_at, 0)
                .map(|value| value.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
            "last_finished_at": null,
            "meta": {"model": "qwen-test", "workspace_root": workspace},
            "status": "idle",
            "pinned": false,
            "archived_at": null,
            "archived": false,
            "source": "chat",
            "group_id": "default",
            "parent_session_id": null,
            "root_session_id": null
        }])
    );

    let fixed_delete = client
        .delete(format!("{second_base}/groups/default"))
        .send()
        .await
        .expect("built-in group delete should send");
    assert_eq!(fixed_delete.status(), reqwest::StatusCode::CONFLICT);
    let oversized_batch = client
        .post(format!("{second_base}/actions/batch-archive"))
        .json(&json!({"chat_ids": vec!["id"; 501]}))
        .send()
        .await
        .expect("oversized batch should send");
    assert_eq!(
        oversized_batch.status(),
        reqwest::StatusCode::UNPROCESSABLE_ENTITY
    );
    second_task.abort();
}

#[tokio::test]
async fn persists_the_console_language_across_desktop_restarts() {
    let console = tempfile::tempdir().expect("temporary Console should be created");
    let desktop_data = tempfile::tempdir().expect("temporary Desktop data should be created");
    let database = desktop_data.path().join("threads.sqlite3");
    std::fs::write(console.path().join("index.html"), "<html>console</html>")
        .expect("Console index should be written");
    let model_config = ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("qwen-test"),
    };
    let first_core =
        Core::persistent(model_config.clone(), &database).expect("first Core should open");
    let first_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("first Desktop listener should bind");
    let first_address = first_listener
        .local_addr()
        .expect("first Desktop listener should have an address");
    let first_server = AppServer::new_desktop_with_credential_store_and_data_dir(
        first_core,
        console.path(),
        String::from("desktop-language-first-token"),
        Arc::new(MemoryCredentialStore::default()),
        desktop_data.path(),
    )
    .expect("first Desktop server should configure");
    let first_task = tokio::spawn(first_server.run_http(first_listener));
    let client = reqwest::Client::new();
    assert_eq!(
        client
            .put(format!("http://{first_address}/api/settings/language"))
            .json(&json!({"language": "ja"}))
            .send()
            .await
            .expect("first language update should send")
            .json::<Value>()
            .await
            .expect("first language update should be JSON"),
        json!({"language": "ja"})
    );
    first_task.abort();
    let _ = first_task.await;

    let second_core = Core::persistent(model_config, &database).expect("second Core should open");
    let second_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("second Desktop listener should bind");
    let second_address = second_listener
        .local_addr()
        .expect("second Desktop listener should have an address");
    let second_server = AppServer::new_desktop_with_credential_store_and_data_dir(
        second_core,
        console.path(),
        String::from("desktop-language-second-token"),
        Arc::new(MemoryCredentialStore::default()),
        desktop_data.path(),
    )
    .expect("second Desktop server should configure");
    let second_task = tokio::spawn(second_server.run_http(second_listener));
    assert_eq!(
        get_json(
            &client,
            format!("http://{second_address}/api/settings/language"),
        )
        .await,
        json!({"language": "ja"})
    );
    second_task.abort();
}

#[tokio::test]
async fn persists_environment_credentials_across_desktop_restarts() {
    let console = tempfile::tempdir().expect("temporary Console should be created");
    let desktop_data = tempfile::tempdir().expect("temporary Desktop data should be created");
    let database = desktop_data.path().join("threads.sqlite3");
    std::fs::write(console.path().join("index.html"), "<html>console</html>")
        .expect("Console index should be written");
    let model_config = ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("qwen-test"),
    };
    let credentials = Arc::new(MemoryCredentialStore::default());
    let first_core =
        Core::persistent(model_config.clone(), &database).expect("first Core should open");
    let first_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("first Desktop listener should bind");
    let first_address = first_listener
        .local_addr()
        .expect("first Desktop listener should have an address");
    let first_server = AppServer::new_desktop_with_credential_store_and_data_dir(
        first_core,
        console.path(),
        String::from("desktop-environment-first-token"),
        credentials.clone(),
        desktop_data.path(),
    )
    .expect("first Desktop server should configure");
    let first_task = tokio::spawn(first_server.run_http(first_listener));
    let client = reqwest::Client::new();
    let first_response = client
        .put(format!("http://{first_address}/api/envs"))
        .json(&json!({"PERSISTED_VALUE": "not-in-sqlite"}))
        .send()
        .await
        .expect("environment update should send");
    assert_eq!(first_response.status(), reqwest::StatusCode::OK);
    first_task.abort();
    let _ = first_task.await;

    assert!(
        !String::from_utf8_lossy(
            &std::fs::read(&database).expect("environment database should be readable")
        )
        .contains("not-in-sqlite")
    );
    let second_core = Core::persistent(model_config, &database).expect("second Core should open");
    let second_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("second Desktop listener should bind");
    let second_address = second_listener
        .local_addr()
        .expect("second Desktop listener should have an address");
    let second_server = AppServer::new_desktop_with_credential_store_and_data_dir(
        second_core,
        console.path(),
        String::from("desktop-environment-second-token"),
        credentials,
        desktop_data.path(),
    )
    .expect("second Desktop server should configure");
    let second_task = tokio::spawn(second_server.run_http(second_listener));
    assert_eq!(
        get_json(&client, format!("http://{second_address}/api/envs")).await,
        json!([{"key": "PERSISTED_VALUE", "value": "not-in-sqlite"}])
    );
    second_task.abort();
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn persists_and_applies_agent_and_voice_settings_without_storing_secrets() {
    let console = tempfile::tempdir().expect("temporary Console should be created");
    let desktop_data = tempfile::tempdir().expect("temporary Desktop data should be created");
    let workspace = tempfile::tempdir().expect("temporary Workspace should be created");
    let database = desktop_data.path().join("threads.sqlite3");
    std::fs::write(console.path().join("index.html"), "<html>console</html>")
        .expect("Console index should be written");
    let transcription_requests = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let transcription_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("transcription listener should bind");
    let transcription_address = transcription_listener
        .local_addr()
        .expect("transcription listener should have an address");
    let captured_requests = transcription_requests.clone();
    let transcription_app = axum::Router::new().route(
        "/v1/audio/transcriptions",
        axum::routing::post(
            move |headers: axum::http::HeaderMap, body: axum::body::Bytes| {
                let captured_requests = captured_requests.clone();
                async move {
                    let authorization = headers
                        .get(axum::http::header::AUTHORIZATION)
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or_default()
                        .to_owned();
                    captured_requests
                        .lock()
                        .expect("transcription request lock should be available")
                        .push((authorization, String::from_utf8_lossy(&body).into_owned()));
                    axum::Json(json!({"text": "mock transcript"}))
                }
            },
        ),
    );
    let transcription_task = tokio::spawn(async move {
        axum::serve(transcription_listener, transcription_app)
            .await
            .expect("transcription server should run");
    });
    let model_config = ModelConfig {
        api_key: Some(String::from("transcription-test-key")),
        base_url: format!("http://{transcription_address}/v1"),
        default_model: String::from("qwen-test"),
    };
    let credentials = Arc::new(MemoryCredentialStore::default());
    *credentials
        .api_key
        .lock()
        .expect("test API key lock should be available") =
        Some(String::from("transcription-test-key"));
    let first_core =
        Core::persistent(model_config.clone(), &database).expect("first Core should open");
    let runtime_probe = first_core.clone();
    let first_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("first Desktop listener should bind");
    let first_address = first_listener
        .local_addr()
        .expect("first Desktop listener should have an address");
    let first_server = AppServer::new_desktop_with_stores_and_workspace(
        first_core,
        console.path(),
        String::from("desktop-agent-settings-first-token"),
        credentials.clone(),
        desktop_data.path(),
        workspace.path(),
    )
    .expect("first Desktop server should configure");
    let first_task = tokio::spawn(first_server.run_http(first_listener));
    let client = reqwest::Client::new();
    let base = format!("http://{first_address}/api");

    let initial = get_json(&client, format!("{base}/workspace/running-config")).await;
    assert_eq!(initial["max_iters"], json!(100));
    assert_eq!(initial["approval_level"], json!("AUTO"));
    assert_eq!(
        initial["reme_light_memory_config"]["embedding_model_config"]["health_check_timeout"],
        json!(15.0)
    );

    let embedding_secret = "embedding-secret-not-in-sqlite";
    let adbpg_secret = "adbpg-secret-not-in-sqlite";
    let updated = client
        .put(format!("{base}/workspace/running-config"))
        .json(&json!({
            "max_iters": 7,
            "loop": {"iteration": {"enabled": true, "max_iterations": 3}},
            "shell_command_timeout": 2.5,
            "shell_command_executable": "/bin/sh",
            "approval_level": "OFF",
            "reme_light_memory_config": {
                "embedding_model_config": {"api_key": embedding_secret}
            },
            "adbpg_memory_config": {
                "rest_base_url": "https://memory.example.test",
                "rest_api_key": adbpg_secret,
                "memory_isolation": true,
                "search_timeout": 10.0,
                "auto_memory_search_config": {"enabled": true, "max_results": 3}
            }
        }))
        .send()
        .await
        .expect("running config update should send");
    assert_eq!(updated.status(), reqwest::StatusCode::OK);
    let updated = updated
        .json::<Value>()
        .await
        .expect("running config update should return JSON");
    assert_eq!(updated["max_iters"], json!(7));
    assert_eq!(updated["loop"]["iteration"]["max_iterations"], json!(3));
    assert_eq!(updated["approval_level"], json!("OFF"));
    assert_eq!(
        updated["reme_light_memory_config"]["embedding_model_config"]["api_key"],
        json!(embedding_secret)
    );
    assert_eq!(
        updated["adbpg_memory_config"]["rest_api_key"],
        json!(adbpg_secret)
    );
    assert_eq!(
        runtime_probe
            .agent_runtime_config()
            .expect("runtime config should read"),
        qwenpaw_core::AgentRuntimeConfig {
            max_agent_steps: 3,
            shell_timeout_ms: 2_500,
            shell_executable: String::from("/bin/sh"),
            approval_level: qwenpaw_core::ToolApprovalLevel::Off,
        }
    );

    let disabled_transcription = multipart_request(
        &client,
        format!("{base}/workspace/transcribe"),
        "file",
        "voice.wav",
        b"not-real-audio",
    )
    .await;
    assert_eq!(
        disabled_transcription.status(),
        reqwest::StatusCode::BAD_REQUEST
    );
    assert_eq!(
        disabled_transcription
            .json::<Value>()
            .await
            .expect("disabled transcription error should be JSON"),
        json!({
            "detail": {
                "code": "TRANSCRIPTION_DISABLED",
                "message": "Transcription is disabled. Configure a transcription provider in Settings."
            }
        })
    );

    for invalid in [
        json!({"shell_command_timeout": 0}),
        json!({"llm_backoff_base": 4.0, "llm_backoff_cap": 3.0}),
        json!({"approval_level": "sometimes"}),
        json!({
            "reme_light_memory_config": {
                "embedding_model_config": {"api_key": "line\nbreak"}
            }
        }),
    ] {
        let response = client
            .put(format!("{base}/workspace/running-config"))
            .json(&invalid)
            .send()
            .await
            .expect("invalid running config update should send");
        assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    }

    assert_eq!(
        client
            .put(format!("{base}/workspace/language"))
            .json(&json!({"language": "ID"}))
            .send()
            .await
            .expect("language update should send")
            .json::<Value>()
            .await
            .expect("language update should return JSON"),
        json!({
            "language": "id",
            "copied_files": [
                "AGENTS.md",
                "BOOTSTRAP.md",
                "CONTACTS.md",
                "HEARTBEAT.md",
                "MAIL_TRIAGE.md",
                "MEMORY.md",
                "PROFILE.md",
                "SOUL.md"
            ],
            "agent_id": "default"
        })
    );
    assert_eq!(
        std::fs::read_to_string(workspace.path().join("SOUL.md"))
            .expect("Indonesian SOUL template should be installed"),
        include_str!("../../../../src/qwenpaw/agents/md_files/id/SOUL.md")
    );
    assert_eq!(
        client
            .put(format!("{base}/config/user-timezone"))
            .json(&json!({"timezone": "Asia/Shanghai"}))
            .send()
            .await
            .expect("timezone update should send")
            .json::<Value>()
            .await
            .expect("timezone update should return JSON"),
        json!({"timezone": "Asia/Shanghai"})
    );
    assert_eq!(
        client
            .put(format!("{base}/workspace/audio-mode"))
            .json(&json!({"audio_mode": "native"}))
            .send()
            .await
            .expect("audio mode update should send")
            .json::<Value>()
            .await
            .expect("audio mode update should return JSON"),
        json!({"audio_mode": "native"})
    );
    assert_eq!(
        client
            .put(format!("{base}/workspace/transcription-provider-type"))
            .json(&json!({"transcription_provider_type": "whisper_api"}))
            .send()
            .await
            .expect("provider type update should send")
            .json::<Value>()
            .await
            .expect("provider type update should return JSON"),
        json!({"transcription_provider_type": "whisper_api"})
    );
    assert_eq!(
        client
            .put(format!("{base}/workspace/transcription-provider"))
            .json(&json!({"provider_id": "openai-compatible"}))
            .send()
            .await
            .expect("provider update should send")
            .json::<Value>()
            .await
            .expect("provider update should return JSON"),
        json!({"provider_id": "openai-compatible"})
    );
    let completed_transcription = multipart_request(
        &client,
        format!("{base}/workspace/transcribe"),
        "file",
        "voice.wav",
        b"not-real-audio",
    )
    .await;
    assert_eq!(completed_transcription.status(), reqwest::StatusCode::OK);
    assert_eq!(
        completed_transcription
            .json::<Value>()
            .await
            .expect("completed transcription should be JSON"),
        json!({"text": "mock transcript"})
    );
    {
        let captured = transcription_requests
            .lock()
            .expect("transcription request lock should be available");
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].0, "Bearer transcription-test-key");
        assert!(captured[0].1.contains("name=\"model\""));
        assert!(captured[0].1.contains("whisper-1"));
        assert!(captured[0].1.contains("not-real-audio"));
    }

    let invalid_language = client
        .put(format!("{base}/workspace/language"))
        .json(&json!({"language": "xx"}))
        .send()
        .await
        .expect("invalid language update should send");
    assert_eq!(invalid_language.status(), reqwest::StatusCode::BAD_REQUEST);
    let invalid_timezone = client
        .put(format!("{base}/config/user-timezone"))
        .json(&json!({"timezone": "../secret"}))
        .send()
        .await
        .expect("invalid timezone update should send");
    assert_eq!(invalid_timezone.status(), reqwest::StatusCode::BAD_REQUEST);

    let concurrent_audio = client
        .put(format!("{base}/workspace/audio-mode"))
        .json(&json!({"audio_mode": "auto"}))
        .send();
    let concurrent_provider = client
        .put(format!("{base}/workspace/transcription-provider-type"))
        .json(&json!({"transcription_provider_type": "disabled"}))
        .send();
    let (audio_response, provider_response) = tokio::join!(concurrent_audio, concurrent_provider);
    assert_eq!(
        audio_response
            .expect("concurrent audio update should send")
            .status(),
        reqwest::StatusCode::OK
    );
    assert_eq!(
        provider_response
            .expect("concurrent provider update should send")
            .status(),
        reqwest::StatusCode::OK
    );

    first_task.abort();
    let _ = first_task.await;
    drop(runtime_probe);
    for entry in
        std::fs::read_dir(desktop_data.path()).expect("Desktop data directory should be readable")
    {
        let entry = entry.expect("Desktop data entry should be readable");
        if !entry.path().is_file() {
            continue;
        }
        let data = std::fs::read(entry.path()).expect("Desktop data file should be readable");
        assert!(
            !data
                .windows(embedding_secret.len())
                .any(|window| { window == embedding_secret.as_bytes() })
        );
        assert!(
            !data
                .windows(adbpg_secret.len())
                .any(|window| window == adbpg_secret.as_bytes())
        );
    }

    let second_core = Core::persistent(model_config, &database).expect("second Core should open");
    let second_runtime_probe = second_core.clone();
    let second_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("second Desktop listener should bind");
    let second_address = second_listener
        .local_addr()
        .expect("second Desktop listener should have an address");
    let second_server = AppServer::new_desktop_with_stores_and_workspace(
        second_core,
        console.path(),
        String::from("desktop-agent-settings-second-token"),
        credentials,
        desktop_data.path(),
        workspace.path(),
    )
    .expect("second Desktop server should configure");
    let second_task = tokio::spawn(second_server.run_http(second_listener));
    let second_base = format!("http://{second_address}/api");
    let restarted = get_json(&client, format!("{second_base}/workspace/running-config")).await;
    assert_eq!(restarted["max_iters"], json!(7));
    assert_eq!(restarted["approval_level"], json!("OFF"));
    assert_eq!(
        restarted["reme_light_memory_config"]["embedding_model_config"]["api_key"],
        json!(embedding_secret)
    );
    assert_eq!(
        second_runtime_probe
            .agent_runtime_config()
            .expect("restarted runtime config should read")
            .max_agent_steps,
        3
    );
    assert_eq!(
        get_json(&client, format!("{second_base}/workspace/language")).await,
        json!({"language": "id", "agent_id": "default"})
    );
    assert_eq!(
        get_json(&client, format!("{second_base}/config/user-timezone")).await,
        json!({"timezone": "Asia/Shanghai"})
    );
    assert_eq!(
        get_json(&client, format!("{second_base}/workspace/audio-mode")).await,
        json!({"audio_mode": "auto"})
    );
    assert_eq!(
        get_json(
            &client,
            format!("{second_base}/workspace/transcription-provider-type"),
        )
        .await,
        json!({"transcription_provider_type": "disabled"})
    );
    assert_eq!(
        get_json(
            &client,
            format!("{second_base}/workspace/transcription-providers"),
        )
        .await,
        json!({
            "providers": [{
                "id": "openai-compatible",
                "name": "OpenAI Compatible",
                "available": true
            }],
            "configured_provider_id": "openai-compatible"
        })
    );
    second_task.abort();
    transcription_task.abort();
}

#[tokio::test]
async fn persists_access_control_across_desktop_restarts() {
    let console = tempfile::tempdir().expect("temporary Console should be created");
    let desktop_data = tempfile::tempdir().expect("temporary Desktop data should be created");
    let database = desktop_data.path().join("threads.sqlite3");
    std::fs::write(console.path().join("index.html"), "<html>console</html>")
        .expect("Console index should be written");
    let model_config = ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("qwen-test"),
    };
    let first_core =
        Core::persistent(model_config.clone(), &database).expect("first Core should open");
    let first_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("first Desktop listener should bind");
    let first_address = first_listener
        .local_addr()
        .expect("first Desktop listener should have an address");
    let first_server = AppServer::new_desktop_with_credential_store_and_data_dir(
        first_core,
        console.path(),
        String::from("desktop-access-first-token"),
        Arc::new(MemoryCredentialStore::default()),
        desktop_data.path(),
    )
    .expect("first Desktop server should configure");
    let first_task = tokio::spawn(first_server.run_http(first_listener));
    let client = reqwest::Client::new();
    let added = post_json(
        &client,
        format!("http://{first_address}/api/access-control/whitelist/add"),
        json!({"entries": [{
            "channel": "console",
            "user_id": "persisted-user",
            "remark": "persisted remark",
            "username": "Persisted User"
        }]}),
    )
    .await;
    assert_eq!(added, json!({"status": "ok", "count": 1}));
    first_task.abort();
    let _ = first_task.await;

    let second_core = Core::persistent(model_config, &database).expect("second Core should open");
    let second_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("second Desktop listener should bind");
    let second_address = second_listener
        .local_addr()
        .expect("second Desktop listener should have an address");
    let second_server = AppServer::new_desktop_with_credential_store_and_data_dir(
        second_core,
        console.path(),
        String::from("desktop-access-second-token"),
        Arc::new(MemoryCredentialStore::default()),
        desktop_data.path(),
    )
    .expect("second Desktop server should configure");
    let second_task = tokio::spawn(second_server.run_http(second_listener));
    assert_eq!(
        get_json(
            &client,
            format!("http://{second_address}/api/access-control/console"),
        )
        .await["whitelist"]["persisted-user"],
        json!({"remark": "persisted remark", "username": "Persisted User"})
    );
    second_task.abort();
}

#[tokio::test]
async fn persists_mail_access_control_across_desktop_restarts() {
    let console = tempfile::tempdir().expect("temporary Console should be created");
    let desktop_data = tempfile::tempdir().expect("temporary Desktop data should be created");
    let database = desktop_data.path().join("threads.sqlite3");
    std::fs::write(console.path().join("index.html"), "<html>console</html>")
        .expect("Console index should be written");
    let model_config = ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("qwen-test"),
    };
    let first_core =
        Core::persistent(model_config.clone(), &database).expect("first Core should open");
    let first_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("first Desktop listener should bind");
    let first_address = first_listener
        .local_addr()
        .expect("first Desktop listener should have an address");
    let first_server = AppServer::new_desktop_with_credential_store_and_data_dir(
        first_core,
        console.path(),
        String::from("desktop-mail-access-first-token"),
        Arc::new(MemoryCredentialStore::default()),
        desktop_data.path(),
    )
    .expect("first Desktop server should configure");
    let first_task = tokio::spawn(first_server.run_http(first_listener));
    let client = reqwest::Client::new();
    assert_eq!(
        post_json(
            &client,
            format!("http://{first_address}/api/mail-access-control/whitelist/add"),
            json!({"entries": [{
                "agent_id": "default",
                "address": "Persisted@Example.com",
                "remark": "persisted remark",
                "display_name": "Persisted Sender"
            }]}),
        )
        .await,
        json!({"status": "ok", "count": 1})
    );
    first_task.abort();
    let _ = first_task.await;

    let second_core = Core::persistent(model_config, &database).expect("second Core should open");
    let second_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("second Desktop listener should bind");
    let second_address = second_listener
        .local_addr()
        .expect("second Desktop listener should have an address");
    let second_server = AppServer::new_desktop_with_credential_store_and_data_dir(
        second_core,
        console.path(),
        String::from("desktop-mail-access-second-token"),
        Arc::new(MemoryCredentialStore::default()),
        desktop_data.path(),
    )
    .expect("second Desktop server should configure");
    let second_task = tokio::spawn(second_server.run_http(second_listener));
    assert_eq!(
        get_json(
            &client,
            format!("http://{second_address}/api/mail-access-control"),
        )
        .await["default"]["whitelist"]["persisted@example.com"],
        json!({
            "remark": "persisted remark",
            "display_name": "Persisted Sender"
        })
    );
    second_task.abort();
}

#[tokio::test]
async fn persists_console_channel_configuration_across_desktop_restarts() {
    let console = tempfile::tempdir().expect("temporary Console should be created");
    let desktop_data = tempfile::tempdir().expect("temporary Desktop data should be created");
    let database = desktop_data.path().join("threads.sqlite3");
    std::fs::write(console.path().join("index.html"), "<html>console</html>")
        .expect("Console index should be written");
    let model_config = ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("qwen-test"),
    };
    let first_core =
        Core::persistent(model_config.clone(), &database).expect("first Core should open");
    let first_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("first Desktop listener should bind");
    let first_address = first_listener
        .local_addr()
        .expect("first Desktop listener should have an address");
    let first_server = AppServer::new_desktop_with_credential_store_and_data_dir(
        first_core,
        console.path(),
        String::from("desktop-channel-first-token"),
        Arc::new(MemoryCredentialStore::default()),
        desktop_data.path(),
    )
    .expect("first Desktop server should configure");
    let first_task = tokio::spawn(first_server.run_http(first_listener));
    let client = reqwest::Client::new();
    let channel_url = format!("http://{first_address}/api/config/channels/console");
    let mut config = get_json(&client, channel_url.clone()).await;
    config["bot_prefix"] = json!("persisted-prefix");
    config["enabled"] = json!(false);
    let response = client
        .put(channel_url)
        .json(&config)
        .send()
        .await
        .expect("Console channel update should send");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        response
            .json::<Value>()
            .await
            .expect("Console channel response should be JSON")["enabled"],
        json!(true)
    );
    first_task.abort();
    let _ = first_task.await;

    let second_core = Core::persistent(model_config, &database).expect("second Core should open");
    let second_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("second Desktop listener should bind");
    let second_address = second_listener
        .local_addr()
        .expect("second Desktop listener should have an address");
    let second_server = AppServer::new_desktop_with_credential_store_and_data_dir(
        second_core,
        console.path(),
        String::from("desktop-channel-second-token"),
        Arc::new(MemoryCredentialStore::default()),
        desktop_data.path(),
    )
    .expect("second Desktop server should configure");
    let second_task = tokio::spawn(second_server.run_http(second_listener));
    let persisted = get_json(
        &client,
        format!("http://{second_address}/api/config/channels/console"),
    )
    .await;
    assert_eq!(persisted["bot_prefix"], json!("persisted-prefix"));
    assert_eq!(persisted["enabled"], json!(true));
    second_task.abort();
}

#[tokio::test]
async fn serves_workspace_git_read_and_write_contracts() {
    let console = tempfile::tempdir().expect("temporary Console should be created");
    let desktop_data = tempfile::tempdir().expect("temporary Desktop data should be created");
    let workspace = tempfile::tempdir().expect("temporary Workspace should be created");
    std::fs::write(console.path().join("index.html"), "<html>console</html>")
        .expect("Console index should be written");
    std::fs::write(workspace.path().join("tracked.txt"), "initial\n")
        .expect("tracked fixture should be written");
    let core = Core::new(ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("qwen-test"),
    });
    core.write_preferred_workspace(workspace.path())
        .expect("temporary Workspace should be selected");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Desktop listener should bind");
    let address = listener
        .local_addr()
        .expect("Desktop listener should have an address");
    let server = AppServer::new_desktop_with_credential_store_and_data_dir(
        core,
        console.path(),
        String::from("desktop-git-token"),
        Arc::new(MemoryCredentialStore::default()),
        desktop_data.path(),
    )
    .expect("Desktop server should configure");
    let task = tokio::spawn(server.run_http(listener));

    assert_git_contracts(address, workspace.path()).await;
    task.abort();
}

async fn assert_git_contracts(address: SocketAddr, workspace: &std::path::Path) {
    let client = reqwest::Client::new();
    let base = format!("http://{address}/api/workspace/git");
    let coding_mode = post_json(
        &client,
        format!("http://{address}/api/coding-mode"),
        json!({"enabled": true}),
    )
    .await;
    assert_eq!(coding_mode, json!({"enabled": true, "agent_id": "default"}));
    assert_eq!(
        get_json(&client, format!("http://{address}/api/coding-mode")).await,
        coding_mode
    );
    let initialized = get_json(&client, format!("{base}/status")).await;
    assert_eq!(
        initialized["changes"],
        json!([{"path": "tracked.txt", "status": "?", "staged": false}])
    );
    assert_eq!(initialized["ahead"], json!(0));
    assert_eq!(initialized["behind"], json!(0));
    assert!(
        !initialized["branch"]
            .as_str()
            .unwrap_or_default()
            .is_empty()
    );
    assert!(workspace.join(".git").is_dir());

    assert_eq!(
        post_json(
            &client,
            format!("{base}/stage"),
            json!({"paths": ["tracked.txt"]}),
        )
        .await,
        json!({"staged": ["tracked.txt"]})
    );
    assert_eq!(
        post_json(
            &client,
            format!("{base}/commit"),
            json!({"message": "Track fixture"}),
        )
        .await["committed"],
        json!(true)
    );

    std::fs::write(workspace.join("tracked.txt"), "changed\n")
        .expect("tracked fixture should change");
    std::fs::write(workspace.join("new.txt"), "new file\n")
        .expect("untracked fixture should be written");
    assert_git_status_changes(&client, &base).await;
    assert_git_stage_and_unstage(&client, &base).await;
    let commit_hash = assert_git_commit_and_history(&client, &base).await;
    assert_git_branch_discard_and_revert(&client, &base, workspace, &commit_hash).await;
    assert_git_rejects_unsafe_inputs(&client, &base).await;
}

async fn assert_git_status_changes(client: &reqwest::Client, base: &str) {
    assert_eq!(
        get_json(client, format!("{base}/status")).await["changes"],
        json!([
            {"path": "tracked.txt", "status": "M", "staged": false},
            {"path": "new.txt", "status": "?", "staged": false}
        ])
    );
    let diff = get_json(client, format!("{base}/diff?path=new.txt&untracked=true")).await;
    assert!(
        diff["diff"]
            .as_str()
            .is_some_and(|text| { text.contains("new file") && text.contains("+++ b/new.txt") })
    );
}

async fn assert_git_stage_and_unstage(client: &reqwest::Client, base: &str) {
    let staged = post_json(
        client,
        format!("{base}/stage"),
        json!({"paths": ["tracked.txt", "new.txt"]}),
    )
    .await;
    assert_eq!(staged, json!({"staged": ["tracked.txt", "new.txt"]}));
    let unstaged = post_json(
        client,
        format!("{base}/unstage"),
        json!({"paths": ["new.txt"]}),
    )
    .await;
    assert_eq!(unstaged, json!({"unstaged": ["new.txt"]}));
    let restaged = post_json(
        client,
        format!("{base}/stage"),
        json!({"paths": ["new.txt"]}),
    )
    .await;
    assert_eq!(restaged, json!({"staged": ["new.txt"]}));
}

async fn assert_git_commit_and_history(client: &reqwest::Client, base: &str) -> String {
    let committed = post_json(
        client,
        format!("{base}/commit"),
        json!({"message": "Git contract update"}),
    )
    .await;
    assert_eq!(committed["committed"], json!(true));
    let log = get_json(client, format!("{base}/log?limit=10")).await;
    assert_eq!(log[0]["author"], json!("QwenPaw"));
    assert_eq!(log[0]["message"], json!("Git contract update"));
    let hash = log[0]["hash"]
        .as_str()
        .expect("Git log should contain a hash")
        .to_owned();
    assert_eq!(hash.len(), 8);
    let commit_diff = get_json(client, format!("{base}/commit-diff?commit_hash={hash}")).await;
    assert_eq!(commit_diff["hash"], json!(hash));
    assert!(
        commit_diff["diff"]
            .as_str()
            .is_some_and(|text| text.contains("Git contract update") && text.contains("new.txt"))
    );
    hash
}

async fn assert_git_branch_discard_and_revert(
    client: &reqwest::Client,
    base: &str,
    workspace: &std::path::Path,
    commit_hash: &str,
) {
    assert_eq!(
        post_json(
            client,
            format!("{base}/checkout"),
            json!({"branch": "test/rust-core", "create": true}),
        )
        .await,
        json!({"branch": "test/rust-core"})
    );
    let branches = get_json(client, format!("{base}/branches")).await;
    assert!(
        branches
            .as_array()
            .is_some_and(|branches| branches.iter().any(|branch| branch
                == &json!({"name": "test/rust-core", "current": true, "remote": false})))
    );
    std::fs::write(workspace.join("tracked.txt"), "discard me\n")
        .expect("tracked fixture should change before discard");
    assert_eq!(
        post_json(
            client,
            format!("{base}/discard"),
            json!({"paths": ["tracked.txt"]}),
        )
        .await,
        json!({"discarded": ["tracked.txt"]})
    );
    let discarded = std::fs::read_to_string(workspace.join("tracked.txt"))
        .expect("discarded fixture should read");
    assert_eq!(discarded.lines().collect::<Vec<_>>(), ["changed"]);
    let reverted = post_json(
        client,
        format!("{base}/revert"),
        json!({"commit_hash": commit_hash}),
    )
    .await;
    assert_eq!(reverted["reverted"], json!(commit_hash));
    let reverted = std::fs::read_to_string(workspace.join("tracked.txt"))
        .expect("reverted fixture should read");
    assert_eq!(reverted.lines().collect::<Vec<_>>(), ["initial"]);
    assert!(!workspace.join("new.txt").exists());
}

async fn assert_git_rejects_unsafe_inputs(client: &reqwest::Client, base: &str) {
    let traversal = client
        .post(format!("{base}/stage"))
        .json(&json!({"paths": ["../outside"]}))
        .send()
        .await
        .expect("unsafe path request should send");
    assert_eq!(traversal.status(), reqwest::StatusCode::BAD_REQUEST);
    assert_eq!(
        traversal
            .json::<Value>()
            .await
            .expect("unsafe path response should be JSON"),
        json!({"detail": "Git path must be relative without traversal"})
    );
    let option = client
        .get(format!(
            "{base}/commit-diff?commit_hash=--upload-pack%3Devil"
        ))
        .send()
        .await
        .expect("unsafe hash request should send");
    assert_eq!(option.status(), reqwest::StatusCode::BAD_REQUEST);
    assert_eq!(
        option
            .json::<Value>()
            .await
            .expect("unsafe hash response should be JSON"),
        json!({"detail": "Git commit hash is invalid"})
    );
}

async fn get_json(client: &reqwest::Client, url: String) -> Value {
    let response = client.get(url).send().await.expect("GET should send");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    response.json().await.expect("GET response should be JSON")
}

async fn post_json(client: &reqwest::Client, url: String, body: Value) -> Value {
    let response = client
        .post(url)
        .json(&body)
        .send()
        .await
        .expect("POST should send");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    response.json().await.expect("POST response should be JSON")
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
    let running = http_request(
        address,
        "GET /api/workspace/running-config HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    let running = response_json(&running);
    assert_eq!(running["approval_level"], json!("AUTO"));
    assert_eq!(running["max_iters"], json!(100));
    assert_eq!(
        running["reme_light_memory_config"]["needs_reindex"],
        json!(false)
    );
}

async fn assert_navigation_json_contracts(address: SocketAddr) {
    assert_navigation_control_contracts(address).await;
    assert_navigation_agent_contracts(address).await;
    assert_navigation_settings_contracts(address).await;

    let checkpoint_status = http_request(
        address,
        "GET /api/workspace/checkpoints/status HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    let checkpoint_status = response_json(&checkpoint_status);
    assert_eq!(checkpoint_status["auto_enabled"], json!(false));
    assert_eq!(checkpoint_status["has_checkpoints"], json!(false));
    assert!(
        checkpoint_status["workspace_dir"]
            .as_str()
            .is_some_and(|path| PathBuf::from(path).is_dir())
    );
}

async fn assert_navigation_control_contracts(address: SocketAddr) {
    assert_json_contracts(
        address,
        vec![
            ("/api/workspace/files", json!([])),
            ("/api/workspace/system-prompt-files", json!([])),
            ("/api/access-control/pending/all", json!([])),
            ("/api/pawapps", json!({"apps": [], "total": 0})),
            ("/api/config/user-timezone", json!({"timezone": "UTC"})),
            (
                "/api/cron/dispatch-targets",
                json!({"channels": ["console"], "items": []}),
            ),
            ("/api/cron/jobs", json!([])),
            (
                "/api/config/heartbeat",
                json!({
                    "enabled": false,
                    "every": "6h",
                    "target": "main",
                    "timeoutSeconds": 300,
                    "activeHours": null
                }),
            ),
            ("/api/tools", json!([])),
            ("/api/config/acp", json!({"agents": {}})),
        ],
    )
    .await;
}

async fn assert_navigation_agent_contracts(address: SocketAddr) {
    let memory_runtime = memory_runtime_contract();
    assert_json_contracts(
        address,
        vec![
            (
                "/api/agents/default/memory/runtime-status",
                memory_runtime.clone(),
            ),
            (
                "/api/agents/default/memory/status",
                json!({
                    "components": {},
                    "components_total": "0 B",
                    "process_rss": "0 B",
                    "runtime": memory_runtime
                }),
            ),
            (
                "/api/workspace/language",
                json!({"language": "en", "agent_id": "default"}),
            ),
            (
                "/api/agent-stats?start_date=2026-08-25&end_date=2026-09-01",
                json!({
                    "total_active_sessions": 0,
                    "total_messages": 0,
                    "total_user_messages": 0,
                    "total_assistant_messages": 0,
                    "total_prompt_tokens": 0,
                    "total_completion_tokens": 0,
                    "total_llm_calls": 0,
                    "total_tool_calls": 0,
                    "agent_prompt_tokens": 0,
                    "agent_completion_tokens": 0,
                    "agent_llm_calls": 0,
                    "agent_cache_read_tokens": 0,
                    "agent_cache_eligible_input_tokens": 0,
                    "agent_cache_hit_rate": null,
                    "by_date": [],
                    "channel_stats": [],
                    "start_date": "2026-08-25",
                    "end_date": "2026-09-01"
                }),
            ),
            ("/api/agent-stats/llm-tool-trend", json!([])),
            (
                "/api/workspace/checkpoints/graph?limit=500",
                json!({
                    "nodes": [],
                    "sessions": [],
                    "summary": {
                        "total": 0,
                        "auto": 0,
                        "snapshots": 0,
                        "safety": 0,
                        "heads": 0
                    },
                    "truncated": false
                }),
            ),
        ],
    )
    .await;
}

fn memory_runtime_contract() -> Value {
    json!({
        "worker": {
            "status": "idle",
            "queue_pending": 0,
            "tasks_running": 0
        },
        "auto_memory": {"enabled": false, "interval": 0},
        "tasks": [],
        "recent": {"last_error": null},
        "reindexing": false,
        "embedding_reindex_required": false,
        "embedding_reindex_undo_available": false
    })
}

async fn assert_navigation_settings_contracts(address: SocketAddr) {
    assert_json_contracts(
        address,
        vec![
            ("/api/envs", json!([])),
            (
                "/api/settings/offload-policy",
                json!({"default_action": "keep_foreground"}),
            ),
            (
                "/api/config/security/sandbox",
                json!({
                    "enabled": false,
                    "effective": false,
                    "reason": "Rust Core confines file tools to the selected Workspace"
                }),
            ),
            (
                "/api/config/security/sandbox/deny-paths-protection",
                json!({
                    "active": false,
                    "protected_paths": [],
                    "failed_paths": [],
                    "platform_supported": false,
                    "message": "Rust Core does not use the legacy Python ACL sandbox"
                }),
            ),
            (
                "/api/config/security/tool-guard",
                json!({
                    "enabled": true,
                    "guarded_tools": null,
                    "denied_tools": [],
                    "custom_rules": [],
                    "disabled_rules": [],
                    "auto_denied_rules": [],
                    "shell_evasion_checks": {}
                }),
            ),
            ("/api/config/security/tool-guard/builtin-rules", json!([])),
            ("/api/token-usage/details", json!([])),
            ("/api/workspace/audio-mode", json!({"audio_mode": "auto"})),
            (
                "/api/workspace/transcription-providers",
                json!({
                    "providers": [{
                        "id": "openai-compatible",
                        "name": "OpenAI Compatible",
                        "available": false
                    }],
                    "configured_provider_id": ""
                }),
            ),
            (
                "/api/console/debug/backend-logs?lines=200",
                json!({
                    "path": "",
                    "exists": false,
                    "lines": 0,
                    "updated_at": null,
                    "size": 0,
                    "content": ""
                }),
            ),
            ("/api/backups", json!([])),
            ("/api/backups/jobs/active", Value::Null),
        ],
    )
    .await;
    let local_whisper = get_json(
        &reqwest::Client::new(),
        format!("http://{address}/api/workspace/local-whisper-status"),
    )
    .await;
    assert!(local_whisper["ffmpeg_installed"].is_boolean());
    assert!(local_whisper["whisper_installed"].is_boolean());
    assert_eq!(
        local_whisper["available"],
        json!(
            local_whisper["ffmpeg_installed"] == json!(true)
                && local_whisper["whisper_installed"] == json!(true)
        )
    );
}

async fn assert_json_contracts(address: SocketAddr, contracts: Vec<(&str, Value)>) {
    for (path, expected) in contracts {
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
    assert_eq!(projects, json!([]));

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
