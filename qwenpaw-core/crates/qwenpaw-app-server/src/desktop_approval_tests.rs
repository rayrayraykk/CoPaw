//! Real Console approvals reuse the isolated native model/Cron fixture.

use super::scope::scoped;
use super::*;
use pretty_assertions::assert_eq;

pub(super) async fn agent_chat(fixture: &Fixture, agent: &str) -> String {
    let (status, chat) = scoped(
        fixture,
        agent,
        "POST",
        "/api/chats",
        json!({
            "name":agent, "session_id":"same-session", "user_id":"admin",
            "root_session_id":format!("{agent}-root"), "meta":{"agent_id":"forged"}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{chat}");
    chat["id"].as_str().unwrap().to_owned()
}

pub(super) async fn start_chat(fixture: &Fixture, agent: &str, id: &str) -> Body {
    let response = fixture
        .server
        .clone()
        .router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/console/chat")
                .header("x-agent-id", agent)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"session_id":id,
            "input":[{"role":"user","content":"write fixture"}],"stream":true})
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.into_body()
}

pub(super) async fn wait_pending(fixture: &Fixture, count: usize) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let result = fixture
                .request("GET", "/api/console/push-messages", Value::Null)
                .await;
            if result["pending_approvals"].as_array().unwrap().len() == count {
                return result;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}

async fn finish_chat(body: Body) {
    let bytes = tokio::time::timeout(
        Duration::from_secs(5),
        axum::body::to_bytes(body, 1024 * 1024),
    )
    .await
    .unwrap()
    .unwrap();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(text.contains("\"status\":\"completed\""), "{text}");
}

#[tokio::test]
async fn global_console_approvals_keep_durable_agent_and_root_identity() {
    let mut fixture = Fixture::new().await;
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Writer"}),
        )
        .await;
    let default_id = agent_chat(&fixture, "default").await;
    let writer_id = agent_chat(&fixture, "writer").await;
    fixture.reopen().await;
    assert!(
        fixture
            .server
            .inner
            .desktop_session_aliases
            .read()
            .await
            .thread_to_client
            .is_empty()
    );
    let default_body = start_chat(&fixture, "default", &default_id).await;
    let writer_body = start_chat(&fixture, "writer", &writer_id).await;
    let all = wait_pending(&fixture, 2).await;
    assert_global_identity(&fixture, &all, &default_id, &writer_id).await;
    assert_list_contract(&fixture, &all).await;
    let writer_request = resolve_pair(&fixture, &all).await;
    finish_chat(writer_body).await;
    finish_chat(default_body).await;
    assert_writer_output(&fixture, &writer_id).await;
    assert_eq!(
        wait_pending(&fixture, 0).await,
        json!({"messages":[],"pending_approvals":[]})
    );
    assert_eq!(
        scoped(
            &fixture,
            "default",
            "POST",
            "/api/approval/approve",
            writer_request
        )
        .await,
        (
            StatusCode::NOT_FOUND,
            json!({"detail":"Approval request not found"})
        )
    );
}

async fn assert_global_identity(fixture: &Fixture, all: &Value, default_id: &str, writer_id: &str) {
    let mut identities = all["pending_approvals"].as_array().unwrap().iter().map(|approval| {
        json!({"agent":approval["agent_id"],"owner":approval["owner_agent_id"],
            "session":approval["session_id"],"root":approval["root_session_id"],"thread":approval["thread_id"]})
    }).collect::<Vec<_>>();
    identities.sort_by_key(|value| value["agent"].as_str().unwrap().to_owned());
    assert_eq!(
        identities,
        vec![
            json!({"agent":"default","owner":"default","session":"same-session","root":"default-root","thread":default_id}),
            json!({"agent":"writer","owner":"writer","session":"same-session","root":"writer-root","thread":writer_id})
        ]
    );
    assert_eq!(
        scoped(
            fixture,
            "writer",
            "GET",
            "/api/console/push-messages?session_id=unrelated",
            Value::Null
        )
        .await,
        (StatusCode::OK, all.clone()),
        "global Inbox must keep all approvals"
    );
}

async fn resolve_pair(fixture: &Fixture, all: &Value) -> Value {
    let approvals = all["pending_approvals"].as_array().unwrap();
    let writer = approvals
        .iter()
        .find(|approval| approval["agent_id"] == "writer")
        .unwrap();
    let default = approvals
        .iter()
        .find(|approval| approval["agent_id"] == "default")
        .unwrap();
    let writer_request = json!({"request_id":writer["request_id"],"session_id":"writer-root"});
    assert_eq!(
        scoped(
            fixture,
            "default",
            "POST",
            "/api/approval/approve",
            json!({"request_id":writer["request_id"],"session_id":"default-root"})
        )
        .await,
        (
            StatusCode::FORBIDDEN,
            json!({"detail":"Root session mismatch"})
        )
    );
    assert_eq!(&wait_pending(fixture, 2).await, all);
    // Inbox actions intentionally do not switch the currently selected Agent.
    let approved = scoped(
        fixture,
        "default",
        "POST",
        "/api/approval/approve",
        writer_request.clone(),
    )
    .await;
    assert_eq!(
        approved,
        (
            StatusCode::OK,
            json!({"success":true,"message":"Tool 'write_file' approved",
        "tool_name":"write_file","request_id":writer["request_id"]})
        )
    );
    let denied = scoped(
        fixture,
        "writer",
        "POST",
        "/api/approval/deny",
        json!({"request_id":default["request_id"],"session_id":"default-root"}),
    )
    .await;
    assert_eq!(
        denied,
        (
            StatusCode::OK,
            json!({"success":true,"message":"Tool 'write_file' denied",
        "tool_name":"write_file","request_id":default["request_id"]})
        )
    );
    writer_request
}

async fn assert_writer_output(fixture: &Fixture, writer_id: &str) {
    let thread = fixture
        .server
        .inner
        .core
        .read_thread(writer_id)
        .await
        .unwrap()
        .thread;
    assert_eq!(
        std::fs::read_to_string(
            std::path::Path::new(thread.workspace_root.as_ref().unwrap()).join("cron-output.txt")
        )
        .unwrap(),
        "created by Cron"
    );
    assert!(
        !fixture
            .directory
            .path()
            .join("workspace/cron-output.txt")
            .exists()
    );
}

async fn assert_list_contract(fixture: &Fixture, all: &Value) {
    let expected = all["pending_approvals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|approval| {
            json!({
                "request_id":approval["request_id"],"session_id":"same-session",
                "root_session_id":approval["root_session_id"],"agent_id":approval["agent_id"],
                "owner_agent_id":approval["agent_id"],"tool_name":"write_file",
                "tool_display_name":"write_file","tool_source":"rust-core",
                "exact_target":approval["exact_target"],"similar_target":"","is_generalized":false,
                "severity":"medium","findings_count":0,"created_at":approval["created_at"],
                "timeout_seconds":120,"result_summary":"","reasoning":""
            })
        })
        .collect::<Vec<_>>();
    for (query, entries) in [
        ("", expected.clone()),
        ("?session_id=", expected.clone()),
        ("?session_id=same-session", vec![]),
        (
            "?session_id=writer-root",
            expected
                .iter()
                .filter(|entry| entry["agent_id"] == "writer")
                .cloned()
                .collect(),
        ),
    ] {
        assert_eq!(
            scoped(
                fixture,
                "default",
                "GET",
                &format!("/api/approval/list{query}"),
                Value::Null
            )
            .await,
            (
                StatusCode::OK,
                json!({"count":entries.len(),"pending_approvals":entries})
            )
        );
    }
}

#[tokio::test]
async fn console_cannot_rebind_another_agents_thread_by_id_or_alias() {
    let fixture = Fixture::new().await;
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Writer"}),
        )
        .await;
    let id = agent_chat(&fixture, "default").await;
    fixture
        .server
        .inner
        .desktop_session_aliases
        .write()
        .await
        .client_to_thread
        .insert(String::from("writer\0poisoned"), id.clone());
    let before = fixture
        .server
        .inner
        .core
        .backup_snapshot(1024 * 1024)
        .unwrap();
    for requested in [&id, "poisoned"] {
        let result = crate::desktop_api::resolve_console_thread(
            &fixture.server,
            "writer",
            Some(requested),
            Some(fixture.directory.path().to_str().unwrap()),
        )
        .await;
        assert!(
            result.is_err(),
            "cross-Agent thread was accepted: {requested}"
        );
        assert_eq!(result.unwrap_err().0, StatusCode::NOT_FOUND);
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
    assert_eq!(fixture.remote.requests.lock().unwrap().len(), 0);
}

#[tokio::test]
async fn corrupt_catalog_denies_the_real_tool_instead_of_assigning_default_ownership() {
    let fixture = Fixture::new().await;
    let id = agent_chat(&fixture, "default").await;
    let (_, mut events) = fixture
        .server
        .inner
        .core
        .start_turn(TurnStartParams {
            thread_id: id,
            input: vec![qwenpaw_protocol::UserInput::Text {
                text: String::from("write fixture"),
            }],
        })
        .await
        .unwrap();
    let approval = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let event = events.recv().await.unwrap();
            if matches!(event, CoreEvent::ToolApprovalRequested(_)) {
                return event;
            }
        }
    })
    .await
    .unwrap();
    fixture
        .server
        .inner
        .core
        .write_chat_catalog_data("{invalid")
        .unwrap();
    crate::desktop_api::track_pending_approval(&fixture.server, &approval).await;
    assert!(
        fixture
            .server
            .inner
            .desktop_pending_approvals
            .read()
            .await
            .is_empty()
    );
    let turn = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let CoreEvent::TurnCompleted(event) = events.recv().await.unwrap() {
                return event.turn;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(turn.status, TurnStatus::Completed);
    assert!(
        !fixture
            .directory
            .path()
            .join("workspace/cron-output.txt")
            .exists()
    );
}

#[tokio::test]
async fn uncatalogued_protocol_threads_belong_only_to_default() {
    let fixture = Fixture::new().await;
    let thread = fixture
        .server
        .inner
        .core
        .start_thread(qwenpaw_protocol::ThreadStartParams {
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
    assert_eq!(
        crate::desktop_api::resolve_console_thread(
            &fixture.server,
            "default",
            Some(&thread.id),
            None
        )
        .await
        .unwrap(),
        thread.id
    );
    assert_eq!(
        crate::desktop_api::resolve_console_thread(
            &fixture.server,
            "writer",
            Some(&thread.id),
            None
        )
        .await
        .unwrap_err()
        .0,
        StatusCode::NOT_FOUND
    );
    let identity = crate::desktop_chats::approval_session_info(&fixture.server, &thread.id)
        .await
        .unwrap();
    assert_eq!(
        (identity.agent, identity.session, identity.root_session),
        (String::from("default"), thread.id.clone(), thread.id)
    );
}

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_inbox_browser_approves_writer_and_denies_default_without_switching_agents() {
    let mut fixture = Fixture::new().await;
    fixture.server.inner.core.write_ui_language("en").unwrap();
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Writer"}),
        )
        .await;
    let default_id = agent_chat(&fixture, "default").await;
    let writer_id = agent_chat(&fixture, "writer").await;
    fixture.reopen().await;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    fixture.server.inner.shutdown.cancel();
    shutdown(&fixture.server).await;
    fixture.server = AppServer::new_desktop_with_stores_and_workspace(
        fixture.server.inner.core.clone(),
        &root.join("../console/dist"),
        String::from("approval-browser-shutdown"),
        Arc::new(Credentials),
        &fixture.directory.path().join("data"),
        &fixture.directory.path().join("workspace"),
    )
    .unwrap();
    let default_body = start_chat(&fixture, "default", &default_id).await;
    let writer_body = start_chat(&fixture, "writer", &writer_id).await;
    wait_pending(&fixture, 2).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(fixture.server.clone().run_http(listener));
    let result = tokio::time::timeout(
        Duration::from_secs(60),
        tokio::process::Command::new("node")
            .arg(root.join("scripts/console_browser_smoke.mjs"))
            .args([&origin, "/inbox", "--approvals-crud"])
            .kill_on_drop(true)
            .output(),
    )
    .await;
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let output = result.unwrap().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let report: Value =
        serde_json::from_str(&stdout).unwrap_or_else(|_| panic!("{stdout}\n{stderr}"));
    assert!(output.status.success(), "{report:#}\n{stderr}");
    assert_eq!(report["ok"], true);
    assert_eq!(
        report["pages"][0]["approvalsCrud"],
        json!({"globalOwners":true,"reload":true,"deniedDefault":true,"approvedWriter":true})
    );
    finish_chat(default_body).await;
    finish_chat(writer_body).await;
    assert_writer_output(&fixture, &writer_id).await;
    assert_eq!(fixture.remote.requests.lock().unwrap().len(), 4);
}
