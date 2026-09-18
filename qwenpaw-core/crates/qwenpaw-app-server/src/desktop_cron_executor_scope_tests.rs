//! Exercise scoped execution through internal admission and public Job HTTP.

use super::scope::scoped;
use super::*;
use pretty_assertions::assert_eq;

#[path = "desktop_cron_executor_scope_browser_tests.rs"]
mod browser;

#[path = "desktop_cron_restart_tests.rs"]
mod restart;

#[path = "desktop_cron_workspace_tests.rs"]
mod workspace;

#[path = "desktop_cron_public_scope_tests.rs"]
mod public_scope;

#[tokio::test]
async fn usage_cron_and_heartbeat_record_validated_workspace_owners() {
    let mut fixture = Fixture::new().await;
    fixture
        .remote
        .usage
        .store(true, std::sync::atomic::Ordering::Release);
    let (first, second) = pair(&fixture, false, "write fixture").await;
    tokio::join!(start(&fixture, &first), start(&fixture, &second));
    fixture.idle().await;
    let snapshot = fixture
        .server
        .inner
        .core
        .backup_snapshot(4 * 1024 * 1024)
        .unwrap();
    assert_eq!(snapshot.usage.len(), 4);
    for (actor, model) in [("default", "cron-fixture"), ("writer", "writer-model")] {
        let records = snapshot
            .usage
            .iter()
            .filter(|record| record.agent_id == actor)
            .collect::<Vec<_>>();
        assert_eq!(records.len(), 2);
        for record in records {
            assert_eq!(record.data_key, Some(fixture.data_key(actor)));
            assert_eq!(
                record.call,
                qwenpaw_storage::StoredModelCall {
                    provider_id: String::from("openai-compatible"),
                    model: model.to_owned(),
                    prompt_tokens: 10,
                    completion_tokens: 3,
                    cache_read_tokens: 0,
                    cache_write_tokens: 0,
                    cache_eligible_input_tokens: 0,
                    cache_observed: false,
                    usage_observed: true,
                }
            );
        }
    }
    fixture
        .request(
            "PUT",
            "/api/workspace/running-config",
            json!({"approval_level":"OFF"}),
        )
        .await;
    std::fs::write(
        fixture.directory.path().join("workspace/HEARTBEAT.md"),
        "write fixture",
    )
    .unwrap();
    assert_eq!(
        fixture
            .request("POST", "/api/config/heartbeat/run", Value::Null)
            .await,
        json!({"started":true})
    );
    wait_heartbeat(&fixture).await;
    let complete = fixture
        .server
        .inner
        .core
        .backup_snapshot(4 * 1024 * 1024)
        .unwrap();
    let heartbeat = complete
        .usage
        .iter()
        .filter(|record| !snapshot.usage.iter().any(|old| old.id == record.id))
        .collect::<Vec<_>>();
    assert_eq!(heartbeat.len(), 2);
    let default = snapshot
        .usage
        .iter()
        .find(|record| record.agent_id == "default")
        .unwrap();
    for record in heartbeat {
        assert_eq!(
            (&record.agent_id, &record.data_key, &record.call),
            (&default.agent_id, &default.data_key, &default.call)
        );
    }
    fixture.reopen().await;
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .backup_snapshot(4 * 1024 * 1024)
            .unwrap()
            .usage,
        complete.usage
    );
}

async fn wait_heartbeat(fixture: &Fixture) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while fixture
            .server
            .inner
            .desktop_heartbeat_running
            .load(std::sync::atomic::Ordering::Acquire)
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

async fn pair(fixture: &Fixture, safety: bool, prompt: &str) -> (String, String) {
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Writer"}),
        )
        .await;
    fixture
        .request(
            "POST",
            "/api/models/openai-compatible/models",
            json!({"id":"writer-model","name":"Writer Model"}),
        )
        .await;
    fixture.request("PUT", "/api/models/active", json!({"provider_id":"openai-compatible","model":"writer-model","scope":"agent","agent_id":"writer"})).await;
    let first = fixture
        .create(json!({"tool_safety":safety}), prompt, true)
        .await;
    let second = fixture
        .create(json!({"tool_safety":safety}), prompt, true)
        .await;
    let mut data = read_data(&fixture.server).unwrap();
    fixture.bind_job(&mut data, &second, "writer");
    data.public_ids
        .insert(first.clone(), String::from("shared-job"));
    data.public_ids
        .insert(second.clone(), String::from("shared-job"));
    data.jobs[1]
        .meta
        .insert(String::from("agent_id"), json!("default"));
    write_data(&fixture.server, &data).unwrap();
    (first, second)
}

async fn start(fixture: &Fixture, key: &str) {
    let _guard = fixture.server.inner.desktop_cron_lock.lock().await;
    let mut data = read_data(&fixture.server).unwrap();
    let job = find_job(&data, key).unwrap().clone();
    assert!(
        enqueue(
            &fixture.server,
            &mut data,
            job,
            "manual",
            fixture.server.inner.core.operation_guard().unwrap()
        )
        .await
        .unwrap()
    );
}

async fn start_public(fixture: &Fixture, actor: &str) {
    assert_eq!(
        scoped(
            fixture,
            actor,
            "POST",
            "/api/cron/jobs/shared-job/run",
            Value::Null
        )
        .await,
        (StatusCode::OK, json!({"started":true}))
    );
}

#[tokio::test]
async fn parallel_agent_jobs_use_their_own_models_workspaces_sessions_and_notifications() {
    let mut fixture = Fixture::new().await;
    let (first, second) = pair(&fixture, false, "write fixture").await;
    fixture.reopen().await;
    tokio::join!(start(&fixture, &first), start(&fixture, &second));
    fixture.idle().await;
    let models = fixture
        .remote
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|request| request["model"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        models
            .iter()
            .filter(|model| *model == "writer-model")
            .count(),
        2
    );
    assert_eq!(
        models
            .iter()
            .filter(|model| **model == fixture.model.default_model)
            .count(),
        2
    );
    for path in [
        "workspace/cron-output.txt",
        "data/workspaces/writer/cron-output.txt",
    ] {
        assert_eq!(
            std::fs::read_to_string(fixture.directory.path().join(path)).unwrap(),
            "created by Cron"
        );
    }
    let catalog = fixture.catalog();
    let mut chats = catalog["chats"]
        .as_object()
        .unwrap()
        .values()
        .map(|chat| json!({"agent":chat["agent_id"],"session":chat["session_id"]}))
        .collect::<Vec<_>>();
    chats.sort_by_key(|chat| chat["agent"].as_str().unwrap().to_owned());
    assert_eq!(
        chats,
        vec![
            json!({"agent":"default","session":"target"}),
            json!({"agent":"writer","session":"target"})
        ]
    );
    let inbox = fixture.inbox();
    for event in inbox["events"].as_array().unwrap() {
        let trace = &inbox["traces"][event["payload"]["run_id"].as_str().unwrap()];
        assert_eq!(trace["meta"]["agent_id"], event["agent_id"]);
        assert_eq!(trace["meta"]["job_id"], "shared-job");
        assert_eq!(trace["meta"]["session_id"], "target");
    }
    let mut owners = inbox["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["agent_id"].as_str().unwrap())
        .collect::<Vec<_>>();
    owners.sort_unstable();
    assert_eq!(owners, vec!["default", "writer"]);
    for key in [&first, &second] {
        assert_eq!(
            read_data(&fixture.server).unwrap().states[key]
                .last_status
                .as_deref(),
            Some("success")
        );
    }
}

async fn pending(fixture: &Fixture, count: usize) -> Vec<Value> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let response = fixture
                .request("GET", "/api/console/push-messages", Value::Null)
                .await;
            let approvals = response["pending_approvals"].as_array().unwrap();
            if approvals.len() == count {
                return approvals.clone();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}

async fn respond(fixture: &Fixture, approval: &Value, action: &str, selected_agent: &str) {
    assert_eq!(
        scoped(
            fixture,
            selected_agent,
            "POST",
            &format!("/api/approval/{action}"),
            json!({
                "request_id":approval["request_id"],"session_id":approval["root_session_id"]
            })
        )
        .await
        .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn scoped_cron_approvals_keep_global_inbox_actions_and_tool_effects_isolated() {
    let fixture = Fixture::new().await;
    pair(&fixture, true, "write fixture").await;
    tokio::join!(
        start_public(&fixture, "default"),
        start_public(&fixture, "writer")
    );
    let approvals = pending(&fixture, 2).await;
    for approval in &approvals {
        assert_eq!(approval["agent_id"], approval["owner_agent_id"]);
        assert_eq!(approval["session_id"], "target");
        assert_eq!(approval["root_session_id"], "target");
        let writer = approval["agent_id"] == "writer";
        respond(
            &fixture,
            approval,
            if writer { "approve" } else { "deny" },
            if writer { "default" } else { "writer" },
        )
        .await;
    }
    fixture.idle().await;
    assert_eq!(pending(&fixture, 0).await, Vec::<Value>::new());
    assert!(
        !fixture
            .directory
            .path()
            .join("workspace/cron-output.txt")
            .exists()
    );
    assert_eq!(
        std::fs::read_to_string(
            fixture
                .directory
                .path()
                .join("data/workspaces/writer/cron-output.txt")
        )
        .unwrap(),
        "created by Cron"
    );
    assert_eq!(fixture.remote.requests.lock().unwrap().len(), 4);
}

async fn stop_writer(delete: bool) {
    let fixture = Fixture::new().await;
    let (first, second) = pair(&fixture, true, "write fixture").await;
    let marker = fixture
        .directory
        .path()
        .join("data/workspaces/writer/keep.txt");
    std::fs::write(&marker, "keep registered Workspace data").unwrap();
    start_public(&fixture, "default").await;
    start_public(&fixture, "writer").await;
    start_public(&fixture, "writer").await;
    let approvals = pending(&fixture, 2).await;
    let before = serde_json::to_value(read_data(&fixture.server).unwrap()).unwrap();
    let (method, path, body, expected) = if delete {
        (
            "DELETE",
            "/api/agents/writer",
            Value::Null,
            json!({"success":true,"agent_id":"writer"}),
        )
    } else {
        (
            "PATCH",
            "/api/agents/writer/toggle",
            json!({"enabled":false}),
            json!({"success":true,"agent_id":"writer","enabled":false}),
        )
    };
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), fixture.request(method, path, body))
            .await
            .unwrap(),
        expected
    );
    let after = serde_json::to_value(read_data(&fixture.server).unwrap()).unwrap();
    assert_eq!(after["jobs"], before["jobs"]);
    assert_eq!(after["states"][&first], before["states"][&first]);
    assert_eq!(after["states"][&second]["last_status"], "cancelled");
    assert_eq!(
        after["history"][&second]
            .as_array()
            .unwrap()
            .iter()
            .map(|record| record["status"].clone())
            .collect::<Vec<_>>(),
        vec![json!("cancelled"), json!("cancelled")]
    );
    assert_eq!(after["active_runs"].as_object().unwrap().len(), 1);
    assert_eq!(active_ids(&fixture.server).len(), 1);
    assert_eq!(
        std::fs::read_to_string(marker).unwrap(),
        "keep registered Workspace data"
    );
    assert_eq!(pending(&fixture, 1).await[0]["agent_id"], "default");
    let writer = approvals
        .iter()
        .find(|approval| approval["agent_id"] == "writer")
        .unwrap();
    assert_eq!(
        scoped(
            &fixture,
            "default",
            "POST",
            "/api/approval/approve",
            json!({"request_id":writer["request_id"],"session_id":"target"})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let default = approvals
        .iter()
        .find(|approval| approval["agent_id"] == "default")
        .unwrap();
    respond(&fixture, default, "approve", "default").await;
    fixture.idle().await;
    assert_eq!(fixture.remote.requests.lock().unwrap().len(), 3);
    assert!(
        !fixture
            .directory
            .path()
            .join("data/workspaces/writer/cron-output.txt")
            .exists()
    );
    assert!(
        fixture
            .directory
            .path()
            .join("workspace/cron-output.txt")
            .is_file()
    );
}

#[tokio::test]
async fn disabling_an_agent_cancels_and_drains_running_and_queued_cron_jobs() {
    stop_writer(false).await;
}

#[tokio::test]
async fn deleting_an_agent_drains_cron_jobs_without_deleting_retained_workspace_data() {
    stop_writer(true).await;
}

#[tokio::test]
async fn cancellation_fence_waits_for_captured_runs_not_later_runs() {
    let fixture = Fixture::new().await;
    let (_, second) = pair(&fixture, true, "write fixture").await;
    start(&fixture, &second).await;
    pending(&fixture, 1).await;
    let completed = {
        let _guard = fixture.server.inner.desktop_cron_lock.lock().await;
        cancel_agent(&fixture.server, "writer")
    };
    assert_eq!(completed.len(), 1);
    start(&fixture, &second).await;
    tokio::time::timeout(Duration::from_secs(5), drain_runs(completed))
        .await
        .unwrap()
        .unwrap();
    let current = pending(&fixture, 1).await;
    assert_eq!(active_ids(&fixture.server).len(), 1);
    assert_eq!(
        read_data(&fixture.server).unwrap().history[&second].len(),
        1
    );
    respond(&fixture, &current[0], "approve", "default").await;
    fixture.idle().await;
    let statuses = read_data(&fixture.server).unwrap().history[&second]
        .iter()
        .map(|record| record.status.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        statuses,
        vec![String::from("cancelled"), String::from("success")]
    );
}

#[tokio::test]
async fn explicit_unavailable_agent_model_never_runs_with_the_default_model() {
    let fixture = Fixture::new().await;
    let (_, second) = pair(&fixture, false, "write fixture").await;
    fixture
        .request(
            "PUT",
            "/api/agents/writer",
            json!({"active_model":{"provider_id":"missing","model":"missing"}}),
        )
        .await;
    start(&fixture, &second).await;
    fixture.idle().await;
    assert_eq!(scoped(&fixture, "writer", "POST", "/api/console/chat", json!({"session_id":"missing-model","input":[{"role":"user","content":"write fixture"}],"stream":true})).await, (StatusCode::BAD_REQUEST, json!({"detail":"Agent model selection is unavailable"})));
    assert!(fixture.remote.requests.lock().unwrap().is_empty());
    let data = read_data(&fixture.server).unwrap();
    assert_eq!(data.states[&second].last_status.as_deref(), Some("error"));
    assert_eq!(
        data.states[&second].last_error.as_deref(),
        Some("Agent model selection is unavailable")
    );
    assert!(
        !fixture
            .directory
            .path()
            .join("data/workspaces/writer/cron-output.txt")
            .exists()
    );
}

#[tokio::test]
async fn scoped_text_delivery_keeps_global_push_contract_and_owned_inbox() {
    let fixture = Fixture::new().await;
    let (_, second) = pair(&fixture, false, "unused").await;
    {
        let _guard = fixture.server.inner.desktop_cron_lock.lock().await;
        let mut data = read_data(&fixture.server).unwrap();
        let mut spec = find_job(&data, &second).unwrap().clone();
        spec.task_type = String::from("text");
        spec.text = Some(String::from("Writer text"));
        spec.request = None;
        crate::desktop_cron::execute_text(&fixture.server, &mut data, &spec, "manual")
            .await
            .unwrap();
    }
    let pushed = fixture
        .request("GET", "/api/console/push-messages", Value::Null)
        .await;
    assert_eq!(pushed["pending_approvals"], json!([]));
    let messages = pushed["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0],
        json!({"id":messages[0]["id"],"text":"Writer text","sticky":false})
    );
    let inbox = fixture.inbox();
    assert_eq!(inbox["events"].as_array().unwrap().len(), 1);
    assert_eq!(inbox["events"][0]["agent_id"], "writer");
    assert_eq!(inbox["events"][0]["source_id"], "shared-job");
    assert_eq!(
        inbox["events"][0]["payload"],
        json!({"job_id":"shared-job","job_name":"Cron fixture","task_type":"text","trigger":"manual","run_id":null,"save_result_to_inbox":true})
    );
    assert!(fixture.remote.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn stopping_an_agent_also_drains_jobs_waiting_for_its_shared_session() {
    let fixture = Fixture::new().await;
    let (first, second) = pair(&fixture, true, "write fixture").await;
    start(&fixture, &first).await;
    start(&fixture, &second).await;
    let approvals = pending(&fixture, 2).await;
    let waiting = fixture
        .create(json!({"tool_safety":true}), "write fixture", true)
        .await;
    let mut data = read_data(&fixture.server).unwrap();
    fixture.bind_job(&mut data, &waiting, "writer");
    write_data(&fixture.server, &data).unwrap();
    start(&fixture, &waiting).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while read_data(&fixture.server).unwrap().states[&waiting]
            .last_status
            .as_deref()
            != Some("running")
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    fixture
        .request(
            "PATCH",
            "/api/agents/writer/toggle",
            json!({"enabled":false}),
        )
        .await;
    let stopped = read_data(&fixture.server).unwrap();
    for key in [&second, &waiting] {
        assert_eq!(
            stopped.states[key].last_status.as_deref(),
            Some("cancelled")
        );
    }
    assert_eq!(active_ids(&fixture.server).len(), 1);
    assert_eq!(fixture.remote.requests.lock().unwrap().len(), 2);
    let default = approvals
        .iter()
        .find(|approval| approval["agent_id"] == "default")
        .unwrap();
    respond(&fixture, default, "deny", "default").await;
    fixture.idle().await;
}

#[tokio::test]
async fn each_cron_turn_uses_its_agents_runtime_limits() {
    let fixture = Fixture::new().await;
    let (first, second) = pair(&fixture, false, "write fixture").await;
    let mut running = crate::desktop_agent_settings::default_running_config();
    running["max_iters"] = json!(1);
    fixture
        .request("PUT", "/api/agents/writer", json!({"running":running}))
        .await;
    tokio::join!(start(&fixture, &first), start(&fixture, &second));
    fixture.idle().await;
    let data = read_data(&fixture.server).unwrap();
    assert_eq!(data.states[&first].last_status.as_deref(), Some("success"));
    assert_eq!(data.states[&second].last_status.as_deref(), Some("error"));
    assert_eq!(
        data.states[&second].last_error.as_deref(),
        Some("agent exceeded maximum steps")
    );
    let models = fixture
        .remote
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|request| request["model"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        models
            .iter()
            .filter(|model| *model == "writer-model")
            .count(),
        1
    );
    assert_eq!(
        models
            .iter()
            .filter(|model| **model == fixture.model.default_model)
            .count(),
        2
    );
}

#[tokio::test]
async fn disabled_or_deleted_owner_never_executes_through_default_fallback() {
    let fixture = Fixture::new().await;
    let (_, second) = pair(&fixture, false, "write fixture").await;
    for (method, path, body, expected) in [
        (
            "PATCH",
            "/api/agents/writer/toggle",
            json!({"enabled":false}),
            "Agent 'writer' is disabled",
        ),
        (
            "DELETE",
            "/api/agents/writer",
            Value::Null,
            "Cron Workspace has no registered Agent",
        ),
    ] {
        fixture.request(method, path, body).await;
        let before = fixture
            .server
            .inner
            .core
            .backup_snapshot(1024 * 1024)
            .unwrap();
        let _guard = fixture.server.inner.desktop_cron_lock.lock().await;
        let mut data = read_data(&fixture.server).unwrap();
        let job = find_job(&data, &second).unwrap().clone();
        let error = enqueue(
            &fixture.server,
            &mut data,
            job,
            "manual",
            fixture.server.inner.core.operation_guard().unwrap(),
        )
        .await
        .unwrap_err();
        assert_eq!(error.1.0, json!({"detail":expected}));
        assert_eq!(
            fixture
                .server
                .inner
                .core
                .backup_snapshot(1024 * 1024)
                .unwrap(),
            before
        );
        assert!(fixture.remote.requests.lock().unwrap().is_empty());
    }
    assert!(active_ids(&fixture.server).is_empty());
}

#[tokio::test]
async fn catalog_removal_keeps_explicit_cron_model_and_empty_slot_uses_global_model() {
    let fixture = Fixture::new().await;
    let (_, second) = pair(&fixture, false, "write fixture").await;
    fixture
        .request(
            "DELETE",
            "/api/models/openai-compatible/models/writer-model",
            Value::Null,
        )
        .await;
    assert_eq!(
        fixture
            .request(
                "GET",
                "/api/models/active?scope=effective&agent_id=writer",
                Value::Null
            )
            .await,
        json!({"active_llm":{"provider_id":"openai-compatible","model":"writer-model"},"effective_max_input_length":131_072})
    );
    start(&fixture, &second).await;
    fixture.idle().await;
    fixture
        .request(
            "PUT",
            "/api/agents/writer",
            json!({"active_model":{"provider_id":"","model":""}}),
        )
        .await;
    start(&fixture, &second).await;
    fixture.idle().await;
    assert_eq!(
        fixture
            .remote
            .requests
            .lock()
            .unwrap()
            .iter()
            .map(|request| request["model"].clone())
            .collect::<Vec<_>>(),
        vec![
            json!("writer-model"),
            json!("writer-model"),
            json!(fixture.model.default_model),
            json!(fixture.model.default_model)
        ]
    );
    assert!(
        !fixture
            .directory
            .path()
            .join("workspace/cron-output.txt")
            .exists()
    );
    assert!(
        fixture
            .directory
            .path()
            .join("data/workspaces/writer/cron-output.txt")
            .is_file()
    );
}
