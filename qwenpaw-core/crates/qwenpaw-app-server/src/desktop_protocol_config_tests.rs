//! SDK settings follow the bound Agent without replacing the Thread model.

use super::*;
use pretty_assertions::assert_eq;

async fn history(fixture: &Fixture, thread: &str) -> qwenpaw_protocol::ThreadReadResponse {
    fixture.server.inner.core.read_thread(thread).await.unwrap()
}

fn enable_usage(fixture: &Fixture) {
    fixture
        .remote
        .usage
        .store(true, std::sync::atomic::Ordering::Release);
}

async fn complete(mut events: qwenpaw_core::TurnEventStream) -> qwenpaw_protocol::Turn {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await.unwrap() {
                CoreEvent::ToolApprovalRequested(_) => panic!("unexpected SDK approval"),
                CoreEvent::TurnCompleted(event) => break event.turn,
                _ => {}
            }
        }
    })
    .await
    .unwrap()
}

async fn writer(fixture: &Fixture, steps: u64) -> String {
    prepare(fixture).await;
    actor(fixture, "writer").await;
    fixture
        .request(
            "PUT",
            "/api/agents/writer",
            json!({
                "active_model":{"provider_id":"openai-compatible","model":"caller-choice"}
            }),
        )
        .await;
    let thread = chat(fixture, "writer", "configured-sdk", "protocol-project").await;
    let mut running = crate::desktop_agent_settings::default_running_config();
    running["approval_level"] = json!("OFF");
    running["loop"]["iteration"]["max_iterations"] = json!(steps);
    fixture
        .request(
            "PUT",
            "/api/agents/writer",
            json!({"active_model":null,"running":running}),
        )
        .await;
    thread
}

#[tokio::test]
async fn protocol_config_agent_step_limit_is_used_without_changing_global_settings() {
    let fixture = Fixture::new().await;
    let thread = writer(&fixture, 1).await;
    let global = fixture.server.inner.core.agent_runtime_config().unwrap();
    let model = fixture.server.inner.core.read_config();
    let messages = exchange(&fixture, &thread, "write fixture").await;
    let turn = &messages.last().unwrap()["params"]["turn"];
    assert_eq!(turn["status"], json!("failed"));
    assert_eq!(
        turn["error"],
        json!({"message":"agent exceeded maximum steps"})
    );
    assert_eq!(fixture.remote.requests.lock().unwrap().len(), 1);
    assert_eq!(
        fixture.server.inner.core.agent_runtime_config().unwrap(),
        global
    );
    assert_eq!(fixture.server.inner.core.read_config(), model);
    assert_eq!(
        history(&fixture, &thread).await.thread.model,
        "caller-choice"
    );
}

#[tokio::test]
async fn protocol_config_usage_follows_bound_workspace_and_keeps_thread_model() {
    let fixture = Fixture::new().await;
    let thread = writer(&fixture, 100).await;
    fixture
        .remote
        .usage
        .store(true, std::sync::atomic::Ordering::Release);
    let context = desktop_agents::context_for_agent(&fixture.server, "writer")
        .await
        .unwrap();
    let messages = exchange(&fixture, &thread, "write fixture").await;
    assert_eq!(
        messages.last().unwrap()["params"]["turn"]["status"],
        json!("completed")
    );
    assert_eq!(
        history(&fixture, &thread).await.thread.model,
        "caller-choice"
    );
    let ledger = fixture.server.inner.core.usage_records().await;
    let calls = ledger.iter().map(|record| json!({
        "agent":record.agent_id,"key":record.data_key,"thread":record.thread_id,
        "model":record.call.model,"input":record.call.prompt_tokens,"output":record.call.completion_tokens
    })).collect::<Vec<_>>();
    let expected = json!({"agent":"writer","key":context.data_key,"thread":thread,
        "model":"caller-choice","input":10,"output":3});
    assert_eq!(calls, vec![expected.clone(), expected]);
}

async fn configure_writer_provider(fixture: &Fixture, remote: &Fixture) {
    fixture.request("POST", "/api/models/custom-providers", json!({
        "id":"writer-provider","name":"Writer Provider","default_base_url":remote.model.base_url,
        "models":[{"id":"caller-choice","name":"Caller","generate_kwargs":{"top_p":0.6}},
            {"id":"provider-default","name":"Default","generate_kwargs":{"top_p":0.9}}]
    })).await;
    fixture
        .request(
            "PUT",
            "/api/models/writer-provider/config",
            json!({"generate_kwargs":{"temperature":0.17}}),
        )
        .await;
    fixture
        .request(
            "PUT",
            "/api/agents/writer",
            json!({
                "active_model":{"provider_id":"writer-provider","model":"provider-default"}
            }),
        )
        .await;
}

#[tokio::test]
async fn protocol_config_provider_snapshot_survives_reload_and_a_parallel_default_turn() {
    let mut fixture = Fixture::new().await;
    let remote = Fixture::new().await;
    let thread = writer(&fixture, 100).await;
    let initial = fixture.server.inner.core.statistics_snapshots().await;
    let default = &initial
        .iter()
        .find(|snapshot| snapshot.thread.id != thread)
        .unwrap()
        .thread
        .id;
    let global = fixture.server.inner.core.read_config();
    let runtime = fixture.server.inner.core.agent_runtime_config().unwrap();
    configure_writer_provider(&fixture, &remote).await;
    enable_usage(&fixture);
    enable_usage(&remote);
    let params: Value = serde_json::from_str(&request(&thread, "hold")).unwrap();
    let output = fixture
        .server
        .dispatch("turn/start", params["params"].clone())
        .await
        .unwrap_or_else(|error| panic!("{}", error.message));
    remote.wait_requests(1).await;
    let mut changed = crate::desktop_agent_settings::default_running_config();
    changed["approval_level"] = json!("STRICT");
    changed["loop"]["iteration"]["max_iterations"] = json!(1);
    fixture
        .request(
            "PUT",
            "/api/agents/writer",
            json!({"running":changed,"active_model":null}),
        )
        .await;
    fixture
        .request(
            "PUT",
            "/api/models/writer-provider/config",
            json!({
                "base_url":"http://127.0.0.1:1/v1","generate_kwargs":{"temperature":0.99}
            }),
        )
        .await;
    let default_messages = exchange(&fixture, default, "write fixture").await;
    assert_eq!(
        default_messages.last().unwrap()["params"]["turn"]["status"],
        json!("completed")
    );
    remote
        .remote
        .release_hold
        .store(true, std::sync::atomic::Ordering::Release);
    remote.remote.hold_released.notify_waiters();
    let Some(crate::PostResponse::TurnEvents(events)) = output.post_response else {
        panic!("expected SDK events");
    };
    let terminal = complete(events).await;
    assert_eq!(terminal.status, TurnStatus::Completed);
    let requests = remote.remote.requests.lock().unwrap().clone();
    assert_eq!(
        requests
            .iter()
            .map(|body| json!({
                "model":body["model"],"temperature":body["temperature"],"top_p":body["top_p"]
            }))
            .collect::<Vec<_>>(),
        vec![json!({"model":"caller-choice","temperature":0.17,"top_p":0.6}); 2]
    );
    assert_eq!(
        fixture
            .remote
            .requests
            .lock()
            .unwrap()
            .iter()
            .map(|body| body["model"].clone())
            .collect::<Vec<_>>(),
        vec![json!("cron-fixture"); 2]
    );
    assert_eq!(fixture.server.inner.core.read_config(), global);
    assert_eq!(
        fixture.server.inner.core.agent_runtime_config().unwrap(),
        runtime
    );
    assert_eq!(
        history(&fixture, &thread).await.thread.model,
        "caller-choice"
    );
    let ledger = fixture.server.inner.core.usage_records().await;
    let expected = [("default",default.as_str(),"openai-compatible","cron-fixture"),
        ("writer",thread.as_str(),"writer-provider","caller-choice")].into_iter().flat_map(|(agent,id,provider,model)| {
        vec![json!({"agent":agent,"key":fixture.data_key(agent),"thread":id,"provider":provider,"model":model});2]
    }).collect::<Vec<_>>();
    assert_eq!(ledger.iter().map(|record| json!({"agent":record.agent_id,"key":record.data_key,
        "thread":record.thread_id,"provider":record.call.provider_id,"model":record.call.model
    })).collect::<Vec<_>>(), expected);
    fixture.reopen().await;
    assert_eq!(fixture.server.inner.core.usage_records().await, ledger);
}

#[tokio::test]
async fn protocol_config_invalid_agent_settings_fail_before_recording_input() {
    for missing_provider in [false, true] {
        let fixture = Fixture::new().await;
        let thread = writer(&fixture, 100).await;
        if missing_provider {
            fixture
                .request(
                    "PUT",
                    "/api/agents/writer",
                    json!({
                        "active_model":{"provider_id":"missing-provider","model":"caller-choice"}
                    }),
                )
                .await;
        } else {
            let mut running = crate::desktop_agent_settings::default_running_config();
            running["approval_level"] = json!("invalid");
            fixture
                .request("PUT", "/api/agents/writer", json!({"running":running}))
                .await;
        }
        let before = fixture
            .server
            .inner
            .core
            .backup_snapshot(1024 * 1024)
            .unwrap();
        let before_history = history(&fixture, &thread).await;
        let params: Value = serde_json::from_str(&request(&thread, "write fixture")).unwrap();
        let Err(error) = fixture
            .server
            .dispatch("turn/start", params["params"].clone())
            .await
        else {
            panic!("invalid settings admitted input");
        };
        assert_eq!(
            (error.code, error.message.as_str()),
            (
                -32000,
                if missing_provider {
                    "Agent model selection is unavailable"
                } else {
                    "approval_level is invalid"
                }
            )
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
        assert_eq!(history(&fixture, &thread).await, before_history);
        assert!(fixture.remote.requests.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn protocol_config_wire_cannot_override_strict_approval_or_thread_model() {
    let fixture = Fixture::new().await;
    let thread = writer(&fixture, 100).await;
    enable_usage(&fixture);
    let global = fixture.server.inner.core.agent_runtime_config().unwrap();
    let mut running = crate::desktop_agent_settings::default_running_config();
    running["approval_level"] = json!("STRICT");
    fixture
        .request("PUT", "/api/agents/writer", json!({"running":running}))
        .await;
    let mut params =
        serde_json::from_str::<Value>(&request(&thread, "write fixture")).unwrap()["params"]
            .clone();
    params["runtime"] = json!({"approval_level":"OFF"});
    params["usageOwner"] = json!({"agentId":"default"});
    params["model"] = json!("forged-model");
    let output = fixture
        .server
        .dispatch("turn/start", params)
        .await
        .unwrap_or_else(|error| panic!("{}", error.message));
    let Some(crate::PostResponse::TurnEvents(mut events)) = output.post_response else {
        panic!("expected SDK events");
    };
    let approval = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await.unwrap() {
                CoreEvent::ToolApprovalRequested(event) => break event,
                CoreEvent::TurnCompleted(_) => panic!("wire input bypassed strict approval"),
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(approval.tool_name, "write_file");
    let waiting = history(&fixture, &thread).await;
    let snapshots = fixture.server.inner.core.statistics_snapshots().await;
    let default = &snapshots
        .iter()
        .find(|snapshot| snapshot.thread.id != thread)
        .unwrap()
        .thread
        .id;
    let normal = exchange(&fixture, default, "write fixture").await;
    assert_eq!(
        normal.last().unwrap()["params"]["turn"]["status"],
        json!("completed")
    );
    assert_eq!(history(&fixture, &thread).await, waiting);
    let approved = fixture
        .server
        .dispatch(
            "tool/approval/respond",
            json!({
                "approvalId":approval.approval_id,"decision":"denied"
            }),
        )
        .await
        .unwrap_or_else(|error| panic!("{}", error.message));
    assert_eq!(approved.result, json!({"accepted":true}));
    let terminal = complete(events).await;
    assert_eq!(terminal.status, TurnStatus::Completed);
    assert!(terminal.items.iter().any(|item| matches!(
        item,
        qwenpaw_protocol::Item::ToolResult { is_error: true, .. }
    )));
    assert_eq!(
        fixture.server.inner.core.agent_runtime_config().unwrap(),
        global
    );
    assert_eq!(
        history(&fixture, &thread).await.thread.model,
        "caller-choice"
    );
    let ledger = fixture.server.inner.core.usage_records().await;
    let writer_calls = ledger
        .iter()
        .filter(|record| record.thread_id == thread)
        .map(|record| json!({"agent":record.agent_id,"key":record.data_key}))
        .collect::<Vec<_>>();
    assert_eq!(
        writer_calls,
        vec![json!({"agent":"writer","key":fixture.data_key("writer")}); 2]
    );
}
