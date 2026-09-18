//! Exercise ownership through real streamed model calls and public statistics.

use super::*;
use crate::{desktop_agents, desktop_chats, desktop_console_runs, desktop_stats};
use pretty_assertions::assert_eq;
use qwenpaw_storage::{StoredModelCall, StoredUsageRecord, WorkspaceDataKey};

async fn configured() -> Fixture {
    let fixture = Fixture::new(false).await;
    api(
        &fixture.server,
        "PUT",
        "/api/models/anthropic/config",
        json!({"base_url":fixture.base,"api_key":"isolated-usage-fixture"}),
    )
    .await;
    api(
        &fixture.server,
        "POST",
        "/api/models/anthropic/models",
        json!({"id":"usage-model","name":"Usage Model"}),
    )
    .await;
    api(
        &fixture.server,
        "PUT",
        "/api/models/active",
        json!({"provider_id":"anthropic","model":"usage-model","scope":"global"}),
    )
    .await;
    api(
        &fixture.server,
        "POST",
        "/api/agents",
        json!({"id":"writer","name":"Writer"}),
    )
    .await;
    api(&fixture.server, "PUT", "/api/models/active",
        json!({"provider_id":"anthropic","model":"usage-model","scope":"agent","agent_id":"default"})).await;
    fixture
}

async fn get(fixture: &Fixture, agent: &str, path: &str) -> Value {
    let today = chrono::Local::now().date_naive();
    let response = desktop_stats::router()
        .with_state(fixture.server.clone())
        .oneshot(
            Request::builder()
                .uri(format!("{path}?start_date={today}&end_date={today}"))
                .header("x-agent-id", agent)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    serde_json::from_slice(&bytes).unwrap()
}

async fn key(fixture: &Fixture, actor: &str) -> WorkspaceDataKey {
    desktop_agents::context_for_agent(&fixture.server, actor)
        .await
        .unwrap()
        .data_key
}

fn usage(fixture: &Fixture) -> Vec<StoredUsageRecord> {
    fixture
        .server
        .inner
        .core
        .backup_snapshot(4 * 1024 * 1024)
        .unwrap()
        .usage
}

fn expected_call() -> StoredModelCall {
    StoredModelCall {
        provider_id: String::from("anthropic"),
        model: String::from("usage-model"),
        prompt_tokens: 35,
        completion_tokens: 7,
        cache_read_tokens: 20,
        cache_write_tokens: 5,
        cache_eligible_input_tokens: 35,
        cache_observed: true,
        usage_observed: true,
    }
}

fn local_metrics(summary: &Value) -> Value {
    let fields = [
        "total_active_sessions",
        "total_messages",
        "total_user_messages",
        "total_assistant_messages",
        "total_tool_calls",
        "agent_prompt_tokens",
        "agent_completion_tokens",
        "agent_llm_calls",
        "agent_cache_read_tokens",
        "agent_cache_eligible_input_tokens",
    ];
    Value::Array(fields.iter().map(|name| summary[name].clone()).collect())
}

async fn trend(fixture: &Fixture, calls: u64, tools: u64) {
    let today = chrono::Local::now().date_naive().to_string();
    let expected = json!([{"date":today,"agent_llm_calls":calls,"tool_calls":tools}]);
    for actor in ["default", "writer"] {
        assert_eq!(
            get(fixture, actor, "/api/agent-stats/llm-tool-trend").await,
            expected
        );
    }
}

#[tokio::test]
async fn usage_console_concurrency_scopes_chats_but_preserves_global_token_overlay() {
    let fixture = configured().await;
    let default = key(&fixture, "default").await;
    let writer = key(&fixture, "writer").await;
    assert_ne!(default, writer);
    // Both requests deliberately select the same project, not their ownership root.
    tokio::join!(
        console_turn(&fixture, "default", "1700000000000-usagedefault"),
        console_turn(&fixture, "writer", "1700000000000-usagewriter")
    );
    let records = usage(&fixture);
    assert_eq!(records.len(), 4);
    for (actor, expected_key) in [("default", default), ("writer", writer)] {
        let owned = records
            .iter()
            .filter(|record| record.agent_id == actor)
            .collect::<Vec<_>>();
        assert_eq!(owned.len(), 2);
        for record in owned {
            assert_eq!(
                (&record.data_key, &record.call),
                (&Some(expected_key.clone()), &expected_call())
            );
        }
        let summary = get(&fixture, actor, "/api/agent-stats").await;
        assert_eq!(
            local_metrics(&summary),
            json!([1, 3, 1, 2, 1, 70, 14, 2, 40, 70])
        );
        assert_eq!(
            json!([
                summary["total_prompt_tokens"],
                summary["total_completion_tokens"],
                summary["total_llm_calls"]
            ]),
            json!([140, 28, 4])
        );
    }
    let details = get(&fixture, "default", "/api/token-usage/details").await;
    let today = chrono::Local::now().date_naive().to_string();
    assert_eq!(details, Value::Array(["default","writer"].into_iter().map(|actor| json!({
        "date":today,"provider_id":"anthropic","model":"usage-model","agent_id":actor,
        "prompt_tokens":70,"completion_tokens":14,"cache_read_tokens":40,"cache_write_tokens":10,
        "cache_eligible_input_tokens":70,"cache_observed_calls":2,"call_count":2
    })).collect()));
    assert_eq!(
        get(&fixture, "writer", "/api/token-usage/details").await,
        details
    );
    assert_eq!(
        get(&fixture, "writer", "/api/token-usage").await,
        get(&fixture, "default", "/api/token-usage").await
    );
    trend(&fixture, 4, 2).await;
    api(
        &fixture.server,
        "PATCH",
        "/api/agents/writer/toggle",
        json!({"enabled":false}),
    )
    .await;
    trend(&fixture, 4, 2).await;
    assert_eq!(
        get(&fixture, "default", "/api/token-usage/details").await,
        details
    );
    fixture.task.abort();
}

#[tokio::test]
async fn usage_survives_reregistration_reopen_and_chat_deletion_without_relabeling_history() {
    let mut fixture = configured().await;
    let session = "1700000000000-usagehistory";
    console_turn(&fixture, "writer", session).await;
    let original_key = key(&fixture, "writer").await;
    let original_usage = usage(&fixture);
    let original_summary = get(&fixture, "writer", "/api/agent-stats").await;
    let original_details = get(&fixture, "writer", "/api/token-usage/details").await;
    trend(&fixture, 2, 1).await;
    api(&fixture.server, "DELETE", "/api/agents/writer", Value::Null).await;
    trend(&fixture, 0, 0).await;
    assert_eq!(
        get(&fixture, "default", "/api/token-usage/details").await,
        original_details
    );
    api(
        &fixture.server,
        "POST",
        "/api/agents",
        json!({"id":"editor","name":"Editor",
        "workspace_dir":fixture.directory.path().join("data/workspaces/writer")}),
    )
    .await;
    api(
        &fixture.server,
        "POST",
        "/api/agents",
        json!({"id":"writer","name":"New Writer",
        "workspace_dir":fixture.directory.path().join("new-writer")}),
    )
    .await;
    assert_eq!(key(&fixture, "editor").await, original_key);
    assert_ne!(key(&fixture, "writer").await, original_key);
    assert_eq!(
        get(&fixture, "editor", "/api/agent-stats").await,
        original_summary
    );
    assert_eq!(
        local_metrics(&get(&fixture, "writer", "/api/agent-stats").await),
        json!([0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
    );
    assert_eq!(usage(&fixture), original_usage);
    trend(&fixture, 2, 1).await;
    reopen(&mut fixture).await;
    assert_eq!(usage(&fixture), original_usage);
    assert_eq!(
        get(&fixture, "editor", "/api/agent-stats").await,
        original_summary
    );
    console_turn(&fixture, "editor", session).await;
    let records = usage(&fixture);
    assert_eq!(records.len(), 4);
    assert_eq!(
        records
            .iter()
            .filter(|record| record.agent_id == "writer")
            .cloned()
            .collect::<Vec<_>>(),
        original_usage
    );
    let later = records
        .iter()
        .filter(|record| record.agent_id == "editor")
        .collect::<Vec<_>>();
    assert_eq!(later.len(), 2);
    for record in later {
        assert_eq!(
            (&record.data_key, &record.call),
            (&Some(original_key.clone()), &expected_call())
        );
    }
    let details = get(&fixture, "default", "/api/token-usage/details").await;
    delete_chat(&fixture, "editor", session).await;
    assert_eq!(usage(&fixture), records);
    assert_eq!(
        get(&fixture, "default", "/api/token-usage/details").await,
        details
    );
    assert_eq!(
        local_metrics(&get(&fixture, "editor", "/api/agent-stats").await),
        json!([0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
    );
    trend(&fixture, 0, 0).await;
    fixture.task.abort();
}

async fn delete_chat(fixture: &Fixture, actor: &str, session: &str) {
    let thread_id = desktop_chats::resolve_existing_thread(&fixture.server, actor, session)
        .await
        .unwrap()
        .unwrap();
    let response = fixture
        .server
        .clone()
        .router()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/chats/{thread_id}"))
                .header("x-agent-id", actor)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn reopen(fixture: &mut Fixture) {
    fixture.server.inner.shutdown.cancel();
    desktop_console_runs::shutdown(&fixture.server).await;
    fixture.server = AppServer::new_desktop_with_stores_and_workspace(
        Core::persistent(config(), &fixture.directory.path().join("threads.sqlite3")).unwrap(),
        &fixture.directory.path().join("console"),
        String::from("usage-reopen-shutdown-token"),
        fixture.credentials.clone(),
        &fixture.directory.path().join("data"),
        &fixture.directory.path().join("workspace"),
    )
    .unwrap();
}
