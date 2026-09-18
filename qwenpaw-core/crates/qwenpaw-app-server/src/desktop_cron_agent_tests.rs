use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use pretty_assertions::assert_eq;
use qwenpaw_core::Core;
use qwenpaw_core::ModelConfig;
use qwenpaw_protocol::ApprovalDecision;
use qwenpaw_protocol::ToolApprovalRespondParams;
use tower::ServiceExt as _;

use super::*;

#[derive(Default)]
struct Remote {
    requests: Mutex<Vec<Value>>,
    usage: std::sync::atomic::AtomicBool,
    release_hold: std::sync::atomic::AtomicBool,
    hold_released: tokio::sync::Notify,
}

async fn model(
    State(remote): State<Arc<Remote>>,
    Json(body): Json<Value>,
) -> axum::response::Response {
    remote.requests.lock().unwrap().push(body.clone());
    let messages = body["messages"].as_array().unwrap();
    if messages
        .iter()
        .any(|message| message["content"] == "burst fixture")
    {
        let chunk = json!({"choices":[{"delta":{"content":"x"}}]});
        return axum::response::Response::builder()
            .header("content-type", "text/event-stream")
            .body(Body::from(format!(
                "{}data: [DONE]\n\n",
                format!("data: {chunk}\n\n").repeat(256)
            )))
            .unwrap();
    }
    if messages.iter().any(|message| message["content"] == "hold") {
        loop {
            let released = remote.hold_released.notified();
            tokio::pin!(released);
            released.as_mut().enable();
            if remote
                .release_hold
                .load(std::sync::atomic::Ordering::Acquire)
            {
                break;
            }
            released.await;
        }
    }
    let delta = if messages.last().unwrap()["role"] == "tool" {
        json!({"content":"Cron fixture finished"})
    } else {
        json!({"tool_calls":[{"index":0,"id":Uuid::now_v7().to_string(),"function":{
            "name":"write_file","arguments":json!({"path":"cron-output.txt","content":"created by Cron"}).to_string()
        }}]})
    };
    let mut chunk = json!({"choices":[{"delta":delta}]});
    if remote.usage.load(std::sync::atomic::Ordering::Acquire) {
        chunk["usage"] = json!({"prompt_tokens":10,"completion_tokens":3});
    }
    axum::response::Response::builder()
        .header("content-type", "text/event-stream")
        .body(Body::from(format!("data: {chunk}\n\ndata: [DONE]\n\n")))
        .unwrap()
}

struct Credentials;

impl crate::DesktopCredentialStore for Credentials {
    fn load_api_key(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
    fn save_api_key(&self, _: Option<&str>) -> anyhow::Result<()> {
        panic!("no real credential writes")
    }

    fn save_agent_setting_secret(&self, _: &str, value: Option<&str>) -> anyhow::Result<()> {
        assert_eq!(value, None, "fixture never saves Agent secrets");
        Ok(())
    }
}

struct Fixture {
    directory: tempfile::TempDir,
    server: AppServer,
    remote: Arc<Remote>,
    model: ModelConfig,
    stop: CancellationToken,
}

#[path = "desktop_cron_scope_tests.rs"]
mod scope;

#[path = "desktop_approval_tests.rs"]
mod approvals;

#[path = "desktop_chat_workspace_tests.rs"]
mod chat_workspace;

#[path = "desktop_checkpoint_session_tests.rs"]
mod checkpoint_sessions;

#[path = "desktop_mail_workspace_tests.rs"]
mod mail_workspace;

#[path = "desktop_console_lifecycle_tests.rs"]
mod console_lifecycle;

#[path = "desktop_cron_namespace_runtime_tests.rs"]
mod namespace_runtime;

#[path = "desktop_cron_copy_tests.rs"]
mod copy;

#[path = "desktop_cron_executor_scope_tests.rs"]
mod executor_scope;

#[path = "desktop_agent_workspace_tests.rs"]
mod workspace_creation;

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.inner.shutdown.cancel();
        self.stop.cancel();
    }
}

impl Fixture {
    fn data_key(&self, agent: &str) -> crate::desktop_agents::identity::WorkspaceDataKey {
        let catalog: Value = serde_json::from_slice(
            &std::fs::read(self.directory.path().join("data/agents/catalog.json")).unwrap(),
        )
        .unwrap();
        serde_json::from_value(catalog["agents"][agent]["data_key"].clone()).unwrap()
    }

    fn bind_job(&self, data: &mut CronData, id: &str, agent: &str) {
        data.owners.insert(id.to_owned(), agent.to_owned());
        data.workspace_owners
            .insert(id.to_owned(), self.data_key(agent));
    }

    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("index.html"),
            "<!doctype html><title>Cron fixture</title>",
        )
        .unwrap();
        std::fs::create_dir(directory.path().join("workspace")).unwrap();
        let remote = Arc::new(Remote::default());
        let router = Router::new()
            .route("/v1/chat/completions", axum::routing::post(model))
            .with_state(Arc::clone(&remote));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let model = ModelConfig {
            api_key: None,
            base_url: format!("http://{}/v1", listener.local_addr().unwrap()),
            default_model: String::from("cron-fixture"),
        };
        let stop = CancellationToken::new();
        let cancel = stop.clone();
        tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(cancel.cancelled_owned())
                .await
                .unwrap();
        });
        let server = Self::server(&directory, &model);
        Self {
            directory,
            server,
            remote,
            model,
            stop,
        }
    }

    fn server(directory: &tempfile::TempDir, model: &ModelConfig) -> AppServer {
        let core = Core::persistent(model.clone(), &directory.path().join("core.sqlite")).unwrap();
        AppServer::new_desktop_with_stores_and_workspace(
            core,
            directory.path(),
            String::from("fixture-shutdown"),
            Arc::new(Credentials),
            &directory.path().join("data"),
            &directory.path().join("workspace"),
        )
        .unwrap()
    }

    async fn reopen(&mut self) {
        self.server.inner.shutdown.cancel();
        shutdown(&self.server).await;
        crate::desktop_checkpoints::runtime::shutdown(&self.server).await;
        self.server = Self::server(&self.directory, &self.model);
    }

    async fn request(&self, method: &str, path: &str, body: Value) -> Value {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let response = self.server.clone().router().oneshot(request).await.unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert!(status.is_success(), "{method} {path}: {status} {json}");
        json
    }

    async fn create(&self, runtime: Value, prompt: &str, inbox: bool) -> String {
        self.request("POST", "/api/cron/jobs", json!({"name":"Cron fixture", "enabled":false,
            "schedule":{"cron":"* * * * *"}, "task_type":"agent", "request":{"input":[{"role":"user","content":prompt}]},
            "dispatch":{"target":{"user_id":"admin", "session_id":"target"}}, "runtime":runtime, "save_result_to_inbox":inbox
        })).await["id"].as_str().unwrap().to_owned()
    }

    async fn run(&self, id: &str) {
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            self.request("POST", &format!("/api/cron/jobs/{id}/run"), json!({})),
        )
        .await
        .unwrap();
        assert_eq!(result, json!({"started":true}));
    }

    async fn idle(&self) {
        tokio::time::timeout(Duration::from_secs(8), async {
            while !active_ids(&self.server).is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }

    async fn wait_requests(&self, count: usize) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while self.remote.requests.lock().unwrap().len() < count {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }

    fn inbox(&self) -> Value {
        serde_json::from_str(&self.server.inner.core.read_inbox_data().unwrap().unwrap()).unwrap()
    }

    fn catalog(&self) -> Value {
        serde_json::from_str(
            &self
                .server
                .inner
                .core
                .read_chat_catalog_data()
                .unwrap()
                .unwrap(),
        )
        .unwrap()
    }
}

#[tokio::test]
async fn manual_agent_executes_a_real_tool_and_records_trace_without_a_console_bubble() {
    let fixture = Fixture::new().await;
    let id = fixture
        .create(json!({"share_session":false}), "write fixture", true)
        .await;
    let original = fixture.server.inner.core.agent_runtime_config().unwrap();
    fixture.run(&id).await;
    fixture.idle().await;
    let data = read_data(&fixture.server).unwrap();
    assert_eq!(data.states[&id].last_status.as_deref(), Some("success"));
    assert_eq!(data.history[&id].len(), 1);
    assert_eq!(data.history[&id][0].trigger, "manual");
    assert!(data.active_runs.is_empty());
    assert_eq!(
        std::fs::read_to_string(fixture.directory.path().join("workspace/cron-output.txt"))
            .unwrap(),
        "created by Cron"
    );
    let inbox = fixture.inbox();
    assert_eq!(inbox["events"].as_array().unwrap().len(), 1);
    let run_id = inbox["events"][0]["payload"]["run_id"].as_str().unwrap();
    let trace = &inbox["traces"][run_id];
    assert_eq!(trace["status"], "success");
    assert_eq!(trace["meta"]["session_id"], format!("target:cron:{id}"));
    assert_eq!(trace["events"].as_array().unwrap().len(), 4);
    assert_eq!(trace["events"][1]["event"]["tool_name"], "write_file");
    assert_eq!(
        trace["events"][3]["event"]["content"],
        json!([{"type":"text","text":"Cron fixture finished"}])
    );
    assert!(
        fixture
            .server
            .inner
            .desktop_push_messages
            .read()
            .await
            .is_empty()
    );
    assert_eq!(
        fixture.server.inner.core.agent_runtime_config().unwrap(),
        original
    );
    let catalog = fixture.catalog();
    let chat = catalog["chats"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap();
    assert_eq!(chat["source"], "cron");
    assert_eq!(chat["group_id"], "cron");
}

#[tokio::test]
async fn dedicated_session_and_opt_out_traces_survive_reopen_and_repeat_runs() {
    let mut fixture = Fixture::new().await;
    let id = fixture
        .create(json!({"share_session":false}), "write fixture", false)
        .await;
    fixture.run(&id).await;
    fixture.idle().await;
    let first = fixture.catalog();
    let thread_id = first["chats"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    fixture.reopen().await;
    assert!(
        fixture
            .server
            .inner
            .desktop_session_aliases
            .read()
            .await
            .client_to_thread
            .is_empty()
    );
    fixture.run(&id).await;
    fixture.idle().await;
    assert_eq!(fixture.catalog(), first);
    let thread = fixture
        .server
        .inner
        .core
        .read_thread(&thread_id)
        .await
        .unwrap();
    assert_eq!(thread.turns.len(), 2);
    assert!(
        thread
            .turns
            .iter()
            .all(|turn| turn.status == TurnStatus::Completed)
    );
    let inbox = fixture.inbox();
    assert_eq!(inbox["events"], json!([]));
    assert_eq!(inbox["traces"].as_object().unwrap().len(), 2);
    assert_eq!(fixture.remote.requests.lock().unwrap().len(), 4);
}

#[tokio::test]
async fn safe_task_exposes_approval_and_uses_the_existing_shared_chat_without_renaming() {
    let fixture = Fixture::new().await;
    let chat = fixture.request("POST", "/api/chats", json!({"name":"Keep my name","session_id":"target","user_id":"admin","channel":"console"})).await;
    let before = fixture.catalog();
    let id = fixture
        .create(json!({"tool_safety":true}), "write fixture", false)
        .await;
    fixture.run(&id).await;
    let approval_id = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(id) = fixture
                .server
                .inner
                .desktop_pending_approvals
                .read()
                .await
                .keys()
                .next()
                .cloned()
            {
                break id;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        !fixture
            .directory
            .path()
            .join("workspace/cron-output.txt")
            .exists()
    );
    assert!(
        fixture
            .server
            .inner
            .core
            .respond_tool_approval(ToolApprovalRespondParams {
                approval_id,
                decision: ApprovalDecision::Approved
            })
            .await
            .accepted
    );
    fixture.idle().await;
    assert_eq!(fixture.catalog(), before);
    assert_eq!(
        read_data(&fixture.server).unwrap().states[&id]
            .last_status
            .as_deref(),
        Some("success")
    );
    assert!(
        fixture
            .server
            .inner
            .desktop_pending_approvals
            .read()
            .await
            .is_empty()
    );
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .read_thread(chat["id"].as_str().unwrap())
            .await
            .unwrap()
            .turns
            .len(),
        1
    );
}

#[tokio::test]
async fn queued_runs_timeout_without_recovery_mistaking_live_jobs_for_crashed_ones() {
    let fixture = Fixture::new().await;
    let id = fixture
        .create(json!({"timeout_seconds":1}), "hold", false)
        .await;
    fixture.run(&id).await;
    fixture.wait_requests(1).await;
    fixture.run(&id).await;
    assert_eq!(read_data(&fixture.server).unwrap().active_runs.len(), 2);
    super::super::runtime::tick(&fixture.server, Utc::now())
        .await
        .unwrap();
    assert!(
        !read_data(&fixture.server)
            .unwrap()
            .history
            .contains_key(&id)
    );
    assert_eq!(fixture.remote.requests.lock().unwrap().len(), 1);
    fixture.idle().await;
    let data = read_data(&fixture.server).unwrap();
    assert_eq!(
        data.history[&id]
            .iter()
            .map(|entry| (&*entry.status, entry.error.as_deref(), &*entry.trigger))
            .collect::<Vec<_>>(),
        vec![
            ("error", Some("Cron Agent execution timed out"), "manual"),
            ("error", Some("Cron Agent execution timed out"), "manual")
        ]
    );
    assert!(data.active_runs.is_empty());
    assert_eq!(fixture.remote.requests.lock().unwrap().len(), 2);
    assert!(
        fixture
            .server
            .inner
            .desktop_pending_approvals
            .read()
            .await
            .is_empty()
    );
    let guard = fixture
        .server
        .inner
        .core
        .begin_restore(Duration::from_secs(2))
        .await
        .unwrap();
    drop(guard);
}

#[tokio::test]
async fn deleting_a_running_job_cancels_it_without_resurrecting_state_or_notifications() {
    let fixture = Fixture::new().await;
    let id = fixture.create(json!({}), "hold", true).await;
    fixture.run(&id).await;
    fixture.wait_requests(1).await;
    fixture
        .request("DELETE", &format!("/api/cron/jobs/{id}"), json!({}))
        .await;
    fixture.idle().await;
    let data = read_data(&fixture.server).unwrap();
    assert!(data.jobs.is_empty());
    assert!(data.states.is_empty());
    assert!(data.history.is_empty());
    assert!(data.active_runs.is_empty());
    let inbox = fixture.inbox();
    assert_eq!(inbox["events"], json!([]));
    assert_eq!(
        inbox["traces"]
            .as_object()
            .unwrap()
            .values()
            .next()
            .unwrap()["status"],
        "cancelled"
    );
}

#[tokio::test]
async fn shutdown_cancels_running_and_queued_jobs_and_joins_all_claims() {
    let fixture = Fixture::new().await;
    let id = fixture.create(json!({}), "hold", false).await;
    fixture.run(&id).await;
    fixture.wait_requests(1).await;
    fixture.run(&id).await;
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), shutdown(&fixture.server))
        .await
        .unwrap();
    assert!(active_ids(&fixture.server).is_empty());
    let data = read_data(&fixture.server).unwrap();
    assert!(data.active_runs.is_empty());
    assert_eq!(
        data.history[&id]
            .iter()
            .map(|entry| &*entry.status)
            .collect::<Vec<_>>(),
        vec!["cancelled", "cancelled"]
    );
    let guard = fixture
        .server
        .inner
        .core
        .begin_restore(Duration::from_secs(2))
        .await
        .unwrap();
    drop(guard);
}

#[tokio::test]
async fn scheduled_agent_runs_without_http_traffic_and_does_not_replay_a_completed_slot() {
    let fixture = Fixture::new().await;
    let id = fixture.create(json!({}), "write fixture", true).await;
    let mut job = fixture
        .request("GET", &format!("/api/cron/jobs/{id}"), json!({}))
        .await["spec"]
        .clone();
    job["enabled"] = json!(true);
    job["schedule"] = json!({"type":"once","run_at":super::super::format_datetime(Utc::now() - chrono::Duration::seconds(1))});
    fixture
        .request("PUT", &format!("/api/cron/jobs/{id}"), job)
        .await;
    let scheduler = super::super::spawn_scheduler(&fixture.server);
    fixture.wait_requests(1).await;
    fixture.idle().await;
    super::super::runtime::tick(&fixture.server, Utc::now())
        .await
        .unwrap();
    let data = read_data(&fixture.server).unwrap();
    assert_eq!(data.history[&id].len(), 1);
    assert_eq!(data.history[&id][0].status, "success");
    assert_eq!(data.history[&id][0].trigger, "scheduled");
    assert_eq!(data.states[&id].next_run_at, None);
    assert_eq!(
        fixture.inbox()["events"][0]["payload"]["trigger"],
        "scheduled"
    );
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), scheduler)
        .await
        .unwrap()
        .unwrap();
    shutdown(&fixture.server).await;
}

#[tokio::test]
async fn restart_finalizes_orphaned_claim_and_trace_once_without_calling_the_model() {
    let mut fixture = Fixture::new().await;
    let id = fixture.create(json!({}), "write fixture", false).await;
    let run_id = Uuid::now_v7().to_string();
    let mut data = read_data(&fixture.server).unwrap();
    data.active_runs.insert(
        run_id.clone(),
        AgentRunClaim {
            job_id: id.clone(),
            trigger: String::from("scheduled"),
            agent_id: Some(String::from("default")),
            data_key: Some(fixture.data_key("default")),
        },
    );
    write_data(&fixture.server, &data).unwrap();
    crate::desktop_inbox::write_cron_trace(
        &fixture.server,
        NewInboxTrace {
            run_id: run_id.clone(),
            status: String::from("running"),
            meta: json!({"source":"cron","agent_id":"default"}),
            events: Vec::new(),
            error: None,
        },
    )
    .await
    .unwrap();
    fixture.reopen().await;
    for _ in 0..2 {
        super::super::runtime::tick(&fixture.server, Utc::now())
            .await
            .unwrap();
    }
    let data = read_data(&fixture.server).unwrap();
    assert!(data.active_runs.is_empty());
    assert_eq!(data.history[&id].len(), 1);
    assert_eq!(data.history[&id][0].status, "cancelled");
    assert_eq!(data.history[&id][0].trigger, "scheduled");
    assert_eq!(fixture.inbox()["traces"][&run_id]["status"], "cancelled");
    assert!(fixture.remote.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn unregistered_agent_requests_cannot_run_or_create_default_agent_jobs() {
    let fixture = Fixture::new().await;
    let id = fixture.create(json!({}), "write fixture", false).await;
    let data = read_data(&fixture.server).unwrap();
    let body = serde_json::to_string(find_job(&data, &id).unwrap()).unwrap();
    let before = fixture
        .server
        .inner
        .core
        .backup_snapshot(1024 * 1024)
        .unwrap();
    for (method, path) in [
        ("GET", String::from("/api/cron/jobs")),
        ("POST", String::from("/api/cron/jobs")),
        ("POST", format!("/api/cron/jobs/{id}/run")),
        ("DELETE", format!("/api/cron/jobs/{id}")),
    ] {
        let response = fixture
            .server
            .clone()
            .router()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("x-agent-id", "other")
                    .header("content-type", "application/json")
                    .body(Body::from(body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .backup_snapshot(1024 * 1024)
            .unwrap(),
        before
    );
    assert!(active_ids(&fixture.server).is_empty());
    assert!(fixture.remote.requests.lock().unwrap().is_empty());
}

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_agent_cron_page_runs_native_tools_silent_trace_history_and_crud() {
    let mut fixture = Fixture::new().await;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    fixture
        .request(
            "POST",
            "/api/chats",
            json!({"name":"Known target", "session_id":"cron-browser-session", "user_id":"admin"}),
        )
        .await;
    fixture.request("POST", "/api/chats", json!({"name":"Different user", "session_id":"wrong-user-session", "user_id":"another-user"})).await;
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Writer"}),
        )
        .await;
    super::super::super::desktop_chats::resolve_cron_chat(
        &fixture.server,
        "writer",
        "hidden-agent-session",
        "admin",
        "Private target",
    )
    .await
    .unwrap();
    // Reopen before loading the original page: candidates must come from the
    // durable catalog, not the aliases populated by chat creation.
    fixture.server = AppServer::new_desktop_with_stores_and_workspace(
        Core::persistent(
            fixture.model.clone(),
            &fixture.directory.path().join("core.sqlite"),
        )
        .unwrap(),
        &root.join("../console/dist"),
        String::from("cron-agent-browser-shutdown"),
        Arc::new(Credentials),
        &fixture.directory.path().join("data"),
        &fixture.directory.path().join("workspace"),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(fixture.server.clone().run_http(listener));
    let output = tokio::time::timeout(
        Duration::from_secs(100),
        tokio::process::Command::new("node")
            .arg(root.join("scripts/console_browser_smoke.mjs"))
            .args([&origin, "/cron-jobs", "--cron-agent-crud"])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), task)
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
        report["pages"][0]["cronCrud"],
        json!({"created":true,"toggled":true,"manual":true,"history":true,"edited":true,"reload":true,"deleted":true,"agent":true,"knownTargets":true})
    );
    assert_eq!(
        std::fs::read_to_string(fixture.directory.path().join("workspace/cron-output.txt"))
            .unwrap(),
        "created by Cron"
    );
    assert_eq!(fixture.remote.requests.lock().unwrap().len(), 2);
    assert!(
        fixture
            .server
            .inner
            .desktop_push_messages
            .read()
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn concurrent_runs_share_a_session_without_failing_with_thread_busy() {
    let fixture = Fixture::new().await;
    let id = fixture
        .create(
            json!({"tool_safety":true,"max_concurrency":2}),
            "write fixture",
            false,
        )
        .await;
    fixture.run(&id).await;
    fixture.run(&id).await;
    let mut approved = Vec::<String>::new();
    for _ in 0..2 {
        let approval_id = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(id) = fixture
                    .server
                    .inner
                    .desktop_pending_approvals
                    .read()
                    .await
                    .keys()
                    .find(|id| !approved.contains(*id))
                    .cloned()
                {
                    break id;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        if approved.is_empty() {
            assert_eq!(active_ids(&fixture.server).len(), 2);
            assert_eq!(fixture.remote.requests.lock().unwrap().len(), 1);
        }
        approved.push(approval_id.clone());
        assert!(
            fixture
                .server
                .inner
                .core
                .respond_tool_approval(ToolApprovalRespondParams {
                    approval_id,
                    decision: ApprovalDecision::Approved
                })
                .await
                .accepted
        );
    }
    fixture.idle().await;
    let data = read_data(&fixture.server).unwrap();
    assert_eq!(
        data.history[&id]
            .iter()
            .map(|entry| &*entry.status)
            .collect::<Vec<_>>(),
        vec!["success", "success"]
    );
    assert_eq!(fixture.catalog()["chats"].as_object().unwrap().len(), 1);
    assert_eq!(fixture.remote.requests.lock().unwrap().len(), 4);
}

#[tokio::test]
async fn scheduled_overlap_is_skipped_without_consuming_a_manual_run() {
    let fixture = Fixture::new().await;
    let id = fixture.create(json!({}), "hold", false).await;
    fixture.run(&id).await;
    fixture.wait_requests(1).await;
    let mut job = fixture
        .request("GET", &format!("/api/cron/jobs/{id}"), json!({}))
        .await["spec"]
        .clone();
    job["enabled"] = json!(true);
    job["schedule"] = json!({"type":"once","run_at":super::super::format_datetime(Utc::now() - chrono::Duration::seconds(1))});
    fixture
        .request("PUT", &format!("/api/cron/jobs/{id}"), job)
        .await;
    super::super::runtime::tick(&fixture.server, Utc::now())
        .await
        .unwrap();
    let data = read_data(&fixture.server).unwrap();
    assert_eq!(data.history[&id].len(), 1);
    assert_eq!(data.history[&id][0].status, "skipped");
    assert_eq!(data.history[&id][0].trigger, "scheduled");
    assert_eq!(data.states[&id].next_run_at, None);
    assert_eq!(data.active_runs.len(), 1);
    assert_eq!(fixture.remote.requests.lock().unwrap().len(), 1);
    fixture.server.inner.shutdown.cancel();
    shutdown(&fixture.server).await;
    assert_eq!(
        read_data(&fixture.server).unwrap().history[&id][1].status,
        "cancelled"
    );
}

#[tokio::test]
async fn unrelated_inbox_trace_updates_and_deletion_preserve_opt_out_cron_traces() {
    let fixture = Fixture::new().await;
    let id = fixture.create(json!({}), "write fixture", false).await;
    fixture.run(&id).await;
    fixture.idle().await;
    let retained = fixture.inbox()["traces"].clone();
    let run_id = Uuid::now_v7().to_string();
    let event = crate::desktop_inbox::append_event_with_trace(
        &fixture.server,
        NewInboxEvent {
            agent_id: String::from("default"),
            source_type: String::from("heartbeat"),
            source_id: String::from("heartbeat"),
            event_type: String::from("heartbeat_result"),
            status: String::from("success"),
            severity: String::from("info"),
            title: String::from("Other trace"),
            body: String::from("other"),
            payload: json!({"run_id":run_id}),
        },
        NewInboxTrace {
            run_id: run_id.clone(),
            status: String::from("success"),
            meta: json!({"source":"heartbeat"}),
            events: Vec::new(),
            error: None,
        },
    )
    .await
    .unwrap();
    let mut traces = fixture.inbox()["traces"].as_object().unwrap().clone();
    assert!(traces.remove(&run_id).is_some());
    assert_eq!(json!(traces), retained);
    let deleted = fixture
        .request(
            "DELETE",
            &format!(
                "/api/console/inbox/events/{}",
                event["id"].as_str().unwrap()
            ),
            json!({}),
        )
        .await;
    assert_eq!(
        deleted,
        json!({"deleted":true,"trace_deleted":true,"run_id":run_id})
    );
    assert_eq!(fixture.inbox()["traces"], retained);
}

#[tokio::test]
async fn owned_opt_out_traces_roundtrip_scoped_backup_without_exporting_another_agent() {
    let fixture = Fixture::new().await;
    let id = fixture.create(json!({}), "write fixture", false).await;
    fixture.run(&id).await;
    fixture.idle().await;
    let original = fixture.inbox();
    let selected = BTreeSet::from(["default"]);
    let encoded =
        crate::desktop_inbox::filter_backup_data(&original.to_string(), &selected).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&encoded).unwrap(), original);
    let excluded =
        crate::desktop_inbox::filter_backup_data(&original.to_string(), &BTreeSet::from(["other"]))
            .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&excluded).unwrap(),
        json!({"version":1,"events":[],"traces":{}})
    );
    let restored = crate::desktop_inbox::merge_restore_data(
        Some(&original.to_string()),
        Some(&encoded),
        &selected,
    )
    .unwrap();
    assert_eq!(serde_json::from_str::<Value>(&restored).unwrap(), original);
}

#[tokio::test]
async fn core_restore_drains_running_and_queued_agent_claims_before_granting_exclusivity() {
    let fixture = Fixture::new().await;
    let id = fixture.create(json!({}), "hold", false).await;
    fixture.run(&id).await;
    fixture.wait_requests(1).await;
    fixture.run(&id).await;
    let guard = fixture
        .server
        .inner
        .core
        .begin_restore(Duration::from_secs(5))
        .await
        .unwrap();
    assert!(active_ids(&fixture.server).is_empty());
    let data = read_data(&fixture.server).unwrap();
    assert!(data.active_runs.is_empty());
    assert_eq!(
        data.history[&id]
            .iter()
            .map(|record| &*record.status)
            .collect::<Vec<_>>(),
        vec!["cancelled", "cancelled"]
    );
    let before = fixture
        .server
        .inner
        .core
        .backup_snapshot(1024 * 1024)
        .unwrap();
    tokio::time::sleep(Duration::from_millis(25)).await;
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .backup_snapshot(1024 * 1024)
            .unwrap(),
        before
    );
    drop(guard);
}
