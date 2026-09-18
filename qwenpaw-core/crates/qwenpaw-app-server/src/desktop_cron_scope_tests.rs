use pretty_assertions::assert_eq;

use super::*;

pub(super) async fn scoped(
    fixture: &Fixture,
    agent: &str,
    method: &str,
    path: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = fixture
        .server
        .clone()
        .router()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("x-agent-id", agent)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&body)
            .unwrap_or_else(|_| Value::String(String::from_utf8(body.to_vec()).unwrap())),
    )
}

async fn create_chat(
    fixture: &Fixture,
    agent: &str,
    channel: &str,
    user: &str,
    session: &str,
) -> String {
    let (status, value) = scoped(
        fixture,
        agent,
        "POST",
        "/api/chats",
        json!({
            "name":"Target fixture", "channel":channel, "user_id":user, "session_id":session
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    value["id"].as_str().unwrap().to_owned()
}

fn target(channel: &str, user: &str, session: &str) -> Value {
    json!({"channel":channel, "user_id":user, "session_id":session})
}

#[tokio::test]
async fn cron_targets_preserve_durable_tuples_filters_and_order_across_restart() {
    let mut fixture = Fixture::new().await;
    let first = create_chat(&fixture, "default", "console", "alice", "shared").await;
    create_chat(&fixture, "default", "telegram", "bob", "shared").await;
    create_chat(&fixture, "default", "console", "alice", "shared").await;
    create_chat(&fixture, "default", "console", "bob", "shared").await;
    let archived = create_chat(&fixture, "default", "z-channel", "carol", "archived").await;
    // The original Cron endpoint lists all stored chats, including archived.
    fixture
        .server
        .inner
        .core
        .archive_thread(&qwenpaw_protocol::ThreadArchiveParams {
            thread_id: archived,
        })
        .await
        .unwrap();
    fixture
        .server
        .inner
        .desktop_session_aliases
        .write()
        .await
        .client_to_thread
        .insert(String::from("internal\0not-a-target"), first);
    let cases = [
        (
            "",
            json!({"channels":["console","telegram","z-channel"], "items":[
            target("console","alice","shared"), target("telegram","bob","shared"),
            target("console","bob","shared"), target("z-channel","carol","archived")]}),
        ),
        (
            "?limit=2",
            json!({"channels":["console","telegram"], "items":[
            target("console","alice","shared"), target("telegram","bob","shared")]}),
        ),
        (
            "?channel=console&keyword=%20BOB%20",
            json!({"channels":["console"], "items":[
            target("console","bob","shared")]}),
        ),
        (
            "?channel=telegram",
            json!({"channels":["console","telegram"], "items":[
            target("telegram","bob","shared")]}),
        ),
        (
            "?keyword=absent",
            json!({"channels":["console"], "items":[]}),
        ),
    ];
    for restart in [false, true] {
        if restart {
            fixture.reopen().await;
        }
        let before = fixture.catalog();
        for (query, expected) in &cases {
            assert_eq!(
                scoped(
                    &fixture,
                    "default",
                    "GET",
                    &format!("/api/cron/dispatch-targets{query}"),
                    json!({})
                )
                .await,
                (StatusCode::OK, expected.clone())
            );
        }
        assert_eq!(
            fixture.catalog(),
            before,
            "target reads do not alter the catalog"
        );
    }
}

#[tokio::test]
async fn cron_targets_are_agent_scoped_and_reject_missing_disabled_or_invalid_agents() {
    let fixture = Fixture::new().await;
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Writer"}),
        )
        .await;
    create_chat(&fixture, "default", "console", "default-user", "same").await;
    create_chat(&fixture, "writer", "telegram", "writer-user", "same").await;
    assert_eq!(
        scoped(
            &fixture,
            "writer",
            "GET",
            "/api/cron/dispatch-targets",
            json!({})
        )
        .await,
        (
            StatusCode::OK,
            json!({"channels":["console","telegram"],"items":[target("telegram","writer-user","same")]})
        )
    );
    assert_eq!(
        scoped(
            &fixture,
            "default",
            "GET",
            "/api/cron/dispatch-targets",
            json!({})
        )
        .await,
        (
            StatusCode::OK,
            json!({"channels":["console"],"items":[target("console","default-user","same")]})
        )
    );
    for (agent, query, expected) in [
        ("writer", "?limit=0", StatusCode::UNPROCESSABLE_ENTITY),
        ("writer", "?limit=2001", StatusCode::UNPROCESSABLE_ENTITY),
        ("writer", "?limit=-1", StatusCode::UNPROCESSABLE_ENTITY),
        ("writer", "?limit=bad", StatusCode::UNPROCESSABLE_ENTITY),
        ("missing", "", StatusCode::NOT_FOUND),
        ("../writer", "", StatusCode::BAD_REQUEST),
    ] {
        assert_eq!(
            scoped(
                &fixture,
                agent,
                "GET",
                &format!("/api/cron/dispatch-targets{query}"),
                json!({})
            )
            .await
            .0,
            expected
        );
    }
    fixture
        .request(
            "PATCH",
            "/api/agents/writer/toggle",
            json!({"enabled":false}),
        )
        .await;
    assert_eq!(
        scoped(
            &fixture,
            "writer",
            "GET",
            "/api/cron/dispatch-targets",
            json!({})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn owned_restored_jobs_cannot_be_read_mutated_or_executed_by_default_agent() {
    let fixture = Fixture::new().await;
    let id = fixture.create(json!({}), "must not execute", false).await;
    let mut data = read_data(&fixture.server).unwrap();
    data.owners.insert(id.clone(), String::from("writer"));
    data.workspace_owners.insert(
        id.clone(),
        crate::desktop_agents::identity::WorkspaceDataKey::LegacyAgent(String::from("writer")),
    );
    data.jobs[0].enabled = true;
    data.jobs[0].schedule = serde_json::from_value(json!({"type":"once",
        "run_at":super::super::super::format_datetime(Utc::now() - chrono::Duration::seconds(1))}))
    .unwrap();
    let spec = data.jobs[0].clone();
    super::super::super::reset_schedule(&mut data, &spec, Utc::now()).unwrap();
    write_data(&fixture.server, &data).unwrap();
    let before = fixture
        .server
        .inner
        .core
        .backup_snapshot(1024 * 1024)
        .unwrap();
    assert_eq!(
        fixture.request("GET", "/api/cron/jobs", json!({})).await,
        json!([])
    );
    for (method, suffix) in [
        ("GET", ""),
        ("DELETE", ""),
        ("POST", "/run"),
        ("POST", "/pause"),
        ("POST", "/resume"),
        ("GET", "/state"),
        ("GET", "/history"),
    ] {
        assert_eq!(
            scoped(
                &fixture,
                "default",
                method,
                &format!("/api/cron/jobs/{id}{suffix}"),
                serde_json::to_value(&spec).unwrap()
            )
            .await,
            (StatusCode::NOT_FOUND, json!({"detail":"job not found"}))
        );
    }
    super::super::super::runtime::tick(&fixture.server, Utc::now())
        .await
        .unwrap();
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
    assert!(active_ids(&fixture.server).is_empty());
}

#[tokio::test]
async fn recovering_a_claim_never_interrupts_another_agents_trace() {
    let fixture = Fixture::new().await;
    let id = fixture.create(json!({}), "must not execute", false).await;
    let run_id = Uuid::now_v7().to_string();
    let mut data = read_data(&fixture.server).unwrap();
    data.active_runs.insert(
        run_id.clone(),
        AgentRunClaim {
            job_id: id.clone(),
            trigger: String::from("manual"),
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
            error: None,
            meta: json!({"source":"cron","agent_id":"other"}),
            events: Vec::new(),
        },
    )
    .await
    .unwrap();
    let before = fixture.inbox();
    super::super::super::runtime::tick(&fixture.server, Utc::now())
        .await
        .unwrap();
    assert_eq!(fixture.inbox(), before);
    let recovered = read_data(&fixture.server).unwrap();
    assert!(recovered.active_runs.is_empty());
    assert_eq!(recovered.history[&id][0].status, "cancelled");
}
