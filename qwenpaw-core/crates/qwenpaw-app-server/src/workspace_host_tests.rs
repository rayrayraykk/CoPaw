//! Shared Workspace services do not require Desktop presentation resources.

use super::*;
use pretty_assertions::assert_eq;

#[path = "workspace_host_directory_tests.rs"]
mod directories;

#[path = "workspace_stdio_tests.rs"]
mod stdio;

fn offline_model() -> ModelConfig {
    ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1/v1"),
        default_model: String::from("workspace-fixture"),
    }
}

async fn open_headless(fixture: &mut Fixture) {
    fixture.server.inner.shutdown.cancel();
    crate::protocol_runs::shutdown(&fixture.server).await;
    crate::desktop_cron::shutdown(&fixture.server).await;
    checkpoint_runtime::shutdown(&fixture.server).await;
    let core = Core::persistent(
        fixture.model.clone(),
        &fixture.directory.path().join("core.sqlite"),
    )
    .unwrap();
    fixture.server = AppServer::new_workspace_with_stores(
        core,
        Arc::new(Credentials),
        &fixture.directory.path().join("data"),
        &fixture.directory.path().join("workspace"),
    )
    .unwrap();
}

async fn http_status(server: &AppServer, method: &str, path: &str) -> StatusCode {
    server
        .clone()
        .router()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn workspace_host_initializes_without_a_console_and_does_not_expose_desktop_routes() {
    let directory = tempfile::tempdir().unwrap();
    let workspace = directory.path().join("base");
    std::fs::create_dir(&workspace).unwrap();
    let core = Core::persistent(offline_model(), &directory.path().join("core.sqlite")).unwrap();
    let server = AppServer::new_workspace_with_stores(
        core,
        Arc::new(Credentials),
        &directory.path().join("data"),
        &workspace,
    )
    .unwrap();
    assert!(server.inner.console_static_dir.is_none());
    assert!(server.inner.desktop_shutdown_token.is_none());
    assert!(server.inner.desktop_backups.is_some());
    assert!(server.inner.desktop_local_models.is_some());
    assert!(
        !server
            .inner
            .desktop_heartbeat_running
            .load(std::sync::atomic::Ordering::Acquire)
    );
    let context = desktop_agents::context_for_agent(&server, "default")
        .await
        .unwrap();
    assert_eq!(context.workspace, workspace.canonicalize().unwrap());
    assert!(directory.path().join("data/models/registry.json").is_file());
    assert!(!directory.path().join("index.html").exists());
    for (method, path, expected) in [
        ("GET", "/", StatusCode::NOT_FOUND),
        ("GET", "/api/agents", StatusCode::NOT_FOUND),
        (
            "GET",
            "/api/workspace/checkpoints/graph",
            StatusCode::NOT_FOUND,
        ),
        ("POST", "/api/desktop/shutdown", StatusCode::NOT_FOUND),
        ("GET", "/healthz", StatusCode::OK),
    ] {
        assert_eq!(http_status(&server, method, path).await, expected, "{path}");
    }
    assert!(!server.inner.shutdown.is_cancelled());
    assert_eq!(checkpoint_runtime::task_counts(&server), (0, 0));
    server.inner.shutdown.cancel();
}

#[tokio::test]
async fn workspace_host_protocol_completion_and_graph_survive_both_host_reopens() {
    let mut fixture = Fixture::new().await;
    let thread = prepare(&fixture).await;
    fixture
        .remote
        .usage
        .store(true, std::sync::atomic::Ordering::Release);
    let key = fixture.data_key("default");
    let config = fixture.server.inner.core.read_config();
    let runtime = fixture.server.inner.core.agent_runtime_config().unwrap();
    open_headless(&mut fixture).await;
    assert_eq!(fixture.server.inner.core.read_config(), config);
    assert_eq!(
        fixture.server.inner.core.agent_runtime_config().unwrap(),
        runtime
    );
    assert_eq!(fixture.data_key("default"), key);
    let messages = exchange(&fixture, &thread, "write fixture").await;
    assert_eq!(
        messages.last().unwrap()["params"]["turn"]["status"],
        json!("completed")
    );
    settle(&fixture).await;
    let history = fixture
        .server
        .inner
        .core
        .read_thread(&thread)
        .await
        .unwrap();
    let ledger = fixture.server.inner.core.usage_records().await;
    assert_eq!(
        ledger
            .iter()
            .map(|record| json!({
                "agent":record.agent_id,"key":record.data_key,"thread":record.thread_id,
                "input":record.call.prompt_tokens,"output":record.call.completion_tokens
            }))
            .collect::<Vec<_>>(),
        vec![
            json!({"agent":"default","key":key,"thread":thread,
        "input":10,"output":3});
            2
        ]
    );
    open_headless(&mut fixture).await;
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .read_thread(&thread)
            .await
            .unwrap(),
        history
    );
    assert_eq!(fixture.server.inner.core.usage_records().await, ledger);
    fixture.reopen().await;
    assert_eq!(summary(&fixture, "default").await, one_auto());
    let graph = api(&fixture, "default", "GET", "/graph", Value::Null).await;
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .read_thread(&thread)
            .await
            .unwrap(),
        history
    );
    open_headless(&mut fixture).await;
    fixture.reopen().await;
    assert_eq!(
        api(&fixture, "default", "GET", "/graph", Value::Null).await,
        graph
    );
}

#[tokio::test]
async fn workspace_host_applies_the_bound_agents_model_runtime_and_usage_identity() {
    let mut fixture = Fixture::new().await;
    let default = prepare(&fixture).await;
    actor(&fixture, "writer").await;
    let mut running = crate::desktop_agent_settings::default_running_config();
    running["approval_level"] = json!("OFF");
    running["loop"]["iteration"]["max_iterations"] = json!(1);
    fixture.request("PUT", "/api/agents/writer", json!({
        "running":running,"active_model":{"provider_id":"openai-compatible","model":"caller-choice"}
    })).await;
    let writer = chat(&fixture, "writer", "writer-sdk", "protocol-project").await;
    let before = fixture
        .server
        .inner
        .core
        .read_thread(&default)
        .await
        .unwrap();
    fixture
        .remote
        .usage
        .store(true, std::sync::atomic::Ordering::Release);
    open_headless(&mut fixture).await;
    let messages = exchange(&fixture, &writer, "write fixture").await;
    let turn = &messages.last().unwrap()["params"]["turn"];
    assert_eq!(turn["status"], json!("failed"));
    assert_eq!(
        turn["error"],
        json!({"message":"agent exceeded maximum steps"})
    );
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .read_thread(&default)
            .await
            .unwrap(),
        before
    );
    let requests = fixture.remote.requests.lock().unwrap().clone();
    assert_eq!(
        requests
            .iter()
            .map(|body| body["model"].clone())
            .collect::<Vec<_>>(),
        vec![json!("caller-choice")]
    );
    let ledger = fixture.server.inner.core.usage_records().await;
    assert_eq!(
        ledger
            .iter()
            .map(
                |record| json!({"agent":record.agent_id,"key":record.data_key,
        "thread":record.thread_id,"model":record.call.model})
            )
            .collect::<Vec<_>>(),
        vec![
            json!({"agent":"writer","key":fixture.data_key("writer"),"thread":writer,"model":"caller-choice"})
        ]
    );
}

#[test]
fn workspace_host_rejects_invalid_directories_without_initializing_workspace_files() {
    for invalid_data in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let data = directory.path().join("data");
        let workspace = directory.path().join("workspace");
        let invalid = if invalid_data { &data } else { &workspace };
        std::fs::write(invalid, b"keep this file").unwrap();
        if invalid_data {
            std::fs::create_dir(&workspace).unwrap();
        }
        let core =
            Core::persistent(offline_model(), &directory.path().join("core.sqlite")).unwrap();
        assert!(
            AppServer::new_workspace_with_stores(core, Arc::new(Credentials), &data, &workspace)
                .is_err()
        );
        assert_eq!(std::fs::read(invalid).unwrap(), b"keep this file");
        let untouched = if invalid_data { &workspace } else { &data };
        assert_eq!(std::fs::read_dir(untouched).unwrap().count(), 0);
    }
}

#[tokio::test]
async fn workspace_host_rejects_corrupt_identity_without_repairing_the_marker() {
    let mut fixture = Fixture::new().await;
    let default = prepare(&fixture).await;
    actor(&fixture, "writer").await;
    let thread = chat(&fixture, "writer", "broken-sdk", "protocol-project").await;
    let data = fixture.directory.path().join("data");
    let workspace = data.join("workspaces/writer");
    let marker = workspace.join(desktop_agents::identity::MARKER_NAME);
    std::fs::write(&marker, b"invalid identity").unwrap();
    let catalog = std::fs::read(data.join("agents/catalog.json")).unwrap();
    open_headless(&mut fixture).await;
    let before = fixture
        .server
        .inner
        .core
        .backup_snapshot(1024 * 1024)
        .unwrap();
    let params = serde_json::from_str::<Value>(&request(&thread, "write fixture")).unwrap();
    let Err(error) = fixture
        .server
        .dispatch("turn/start", params["params"].clone())
        .await
    else {
        panic!("corrupt binding admitted a turn");
    };
    assert_eq!(
        (error.code, error.message.as_str()),
        (-32000, "Workspace identity file is invalid")
    );
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .backup_snapshot(1024 * 1024)
            .unwrap(),
        before
    );
    assert_eq!(std::fs::read(&marker).unwrap(), b"invalid identity");
    assert_eq!(
        std::fs::read(data.join("agents/catalog.json")).unwrap(),
        catalog
    );
    let messages = exchange(&fixture, &default, "write fixture").await;
    assert_eq!(
        messages.last().unwrap()["params"]["turn"]["status"],
        json!("completed")
    );
    settle(&fixture).await;
}

struct NoCredentialReads;

#[tokio::test]
async fn workspace_host_protocol_preserves_strict_approval_and_persists_the_denial() {
    let mut fixture = Fixture::new().await;
    let thread = prepare(&fixture).await;
    fixture
        .request(
            "PUT",
            "/api/workspace/running-config",
            json!({"approval_level":"STRICT"}),
        )
        .await;
    open_headless(&mut fixture).await;
    let params = serde_json::from_str::<Value>(&request(&thread, "write fixture")).unwrap();
    let output = fixture
        .server
        .dispatch("turn/start", params["params"].clone())
        .await
        .unwrap_or_else(|error| panic!("{}", error.message));
    let Some(crate::PostResponse::TurnEvents(mut events)) = output.post_response else {
        panic!("expected SDK event stream");
    };
    let approval = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await.unwrap() {
                CoreEvent::ToolApprovalRequested(event) => break event,
                CoreEvent::TurnCompleted(_) => panic!("strict approval was bypassed"),
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(approval.tool_name, "write_file");
    assert!(
        !fixture
            .directory
            .path()
            .join("protocol-project/cron-output.txt")
            .exists()
    );
    let response = fixture
        .server
        .dispatch(
            "tool/approval/respond",
            json!({
                "approvalId":approval.approval_id,"decision":"denied"
            }),
        )
        .await
        .unwrap_or_else(|error| panic!("{}", error.message));
    assert_eq!(response.result, json!({"accepted":true}));
    let terminal = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let CoreEvent::TurnCompleted(event) = events.recv().await.unwrap() {
                break event.turn;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(terminal.status, TurnStatus::Completed);
    assert!(terminal.items.iter().any(|item| matches!(
        item,
        qwenpaw_protocol::Item::ToolResult { is_error: true, .. }
    )));
    assert!(
        !fixture
            .directory
            .path()
            .join("protocol-project/cron-output.txt")
            .exists()
    );
    settle(&fixture).await;
    let history = fixture
        .server
        .inner
        .core
        .read_thread(&thread)
        .await
        .unwrap();
    fixture.reopen().await;
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .read_thread(&thread)
            .await
            .unwrap(),
        history
    );
    assert_eq!(summary(&fixture, "default").await, one_auto());
}

impl crate::DesktopCredentialStore for NoCredentialReads {
    fn load_api_key(&self) -> anyhow::Result<Option<String>> {
        panic!("presentation validation must precede credential reads");
    }

    fn save_api_key(&self, _: Option<&str>) -> anyhow::Result<()> {
        panic!("no credential writes");
    }

    fn load_environment_value(&self, _: &str) -> anyhow::Result<Option<String>> {
        panic!("presentation validation must precede environment initialization");
    }
}

#[test]
fn workspace_host_desktop_validates_presentation_before_shared_initialization() {
    for invalid_token in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let data = directory.path().join("data");
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        if invalid_token {
            std::fs::write(directory.path().join("index.html"), "fixture").unwrap();
        }
        let core =
            Core::persistent(offline_model(), &directory.path().join("core.sqlite")).unwrap();
        core.write_environment_keys(&[String::from("FAIL_IF_READ")])
            .unwrap();
        let before = core.backup_snapshot(1024 * 1024).unwrap();
        let Err(error) = AppServer::new_desktop_with_stores_and_workspace(
            core.clone(),
            directory.path(),
            String::from(if invalid_token {
                "short"
            } else {
                "fixture-valid-token"
            }),
            Arc::new(NoCredentialReads),
            &data,
            &workspace,
        ) else {
            panic!("invalid presentation admitted initialization");
        };
        assert!(error.to_string().contains(if invalid_token {
            "Desktop shutdown token"
        } else {
            "Console static directory must contain index.html"
        }));
        assert_eq!(core.backup_snapshot(1024 * 1024).unwrap(), before);
        assert_eq!(std::fs::read_dir(&workspace).unwrap().count(), 0);
        assert_eq!(std::fs::read_dir(&data).unwrap().count(), 0);
    }
}
