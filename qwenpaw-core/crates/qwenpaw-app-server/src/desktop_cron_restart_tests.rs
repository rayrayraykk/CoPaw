use super::*;
use pretty_assertions::assert_eq;

fn toggle_request(
    server: AppServer,
    enabled: bool,
) -> tokio::task::JoinHandle<axum::response::Response> {
    tokio::spawn(async move {
        server
            .router()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/api/agents/writer/toggle")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({"enabled":enabled}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap()
    })
}

#[tokio::test]
async fn disconnected_stop_retains_lifecycle_fence_until_old_run_finishes() {
    let fixture = Fixture::new().await;
    let (_, writer) = pair(&fixture, false, "hold").await;
    start(&fixture, &writer).await;
    fixture.wait_requests(1).await;
    let inbox_guard = fixture.server.inner.desktop_inbox_lock.lock().await;
    let stop = toggle_request(fixture.server.clone(), false);
    let catalog_path = fixture.directory.path().join("data/agents/catalog.json");
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let catalog: Value =
                serde_json::from_slice(&std::fs::read(&catalog_path).unwrap()).unwrap();
            if catalog["agents"]["writer"]["enabled"] == false {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(
        fixture
            .server
            .inner
            .desktop_agent_lifecycle_lock
            .try_lock()
            .is_err()
    );
    stop.abort();
    assert!(stop.await.unwrap_err().is_cancelled());
    let mut restart = toggle_request(fixture.server.clone(), true);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut restart)
            .await
            .is_err()
    );
    assert!(
        fixture
            .server
            .inner
            .desktop_agent_lifecycle_lock
            .try_lock()
            .is_err()
    );
    assert!(fixture.server.inner.desktop_agents_lock.try_lock().is_ok());
    drop(inbox_guard);
    let response = tokio::time::timeout(Duration::from_secs(5), restart)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    fixture.idle().await;
    let data = serde_json::to_value(read_data(&fixture.server).unwrap()).unwrap();
    assert_eq!(
        data["states"][&writer],
        json!({"next_run_at":null,"last_run_at":null,"last_status":null,"last_error":null})
    );
    assert_eq!(data["history"][&writer].as_array().unwrap().len(), 1);
    assert_eq!(data["history"][&writer][0]["status"], "cancelled");
    assert!(data["active_runs"].is_null());
    assert!(
        !fixture
            .directory
            .path()
            .join("data/workspaces/writer/cron-output.txt")
            .exists()
    );
}

#[tokio::test]
async fn unsettled_claims_block_restart_without_erasing_any_state() {
    let fixture = Fixture::new().await;
    let (_, writer) = pair(&fixture, false, "unused").await;
    fixture
        .request(
            "PATCH",
            "/api/agents/writer/toggle",
            json!({"enabled":false}),
        )
        .await;
    for manual in [false, true] {
        let mut data = read_data(&fixture.server).unwrap();
        data.active_triggers.clear();
        data.active_runs.clear();
        if manual {
            data.active_runs.insert(
                String::from("interrupted"),
                AgentRunClaim {
                    job_id: writer.clone(),
                    trigger: String::from("manual"),
                    agent_id: Some(String::from("writer")),
                    data_key: Some(fixture.data_key("writer")),
                },
            );
        } else {
            data.active_triggers
                .insert(writer.clone(), String::from("scheduled"));
        }
        write_data(&fixture.server, &data).unwrap();
        let before = fixture.server.inner.core.read_cron_data().unwrap();
        let catalog =
            std::fs::read(fixture.directory.path().join("data/agents/catalog.json")).unwrap();
        assert_eq!(
            scoped(
                &fixture,
                "default",
                "PATCH",
                "/api/agents/writer/toggle",
                json!({"enabled":true})
            )
            .await,
            (
                StatusCode::CONFLICT,
                json!({"detail":"Agent Cron runs have not finished stopping"})
            )
        );
        assert_eq!(fixture.server.inner.core.read_cron_data().unwrap(), before);
        assert_eq!(
            std::fs::read(fixture.directory.path().join("data/agents/catalog.json")).unwrap(),
            catalog
        );
    }
}

#[tokio::test]
async fn catalog_publication_failure_restores_exact_cron_bytes() {
    publication_failure(false).await;
}

#[tokio::test]
async fn rollback_failure_reports_recovery_and_keeps_agent_disabled() {
    publication_failure(true).await;
}

async fn publication_failure(fail_rollback: bool) {
    let fixture = Fixture::new().await;
    let (_, writer) = pair(&fixture, false, "write fixture").await;
    for id in ["editor", "reviewer", "planner"] {
        fixture
            .request("POST", "/api/agents", json!({"id":id,"name":id}))
            .await;
    }
    start(&fixture, &writer).await;
    fixture.idle().await;
    fixture
        .request(
            "PATCH",
            "/api/agents/writer/toggle",
            json!({"enabled":false}),
        )
        .await;
    let before = fixture.server.inner.core.read_cron_data().unwrap().unwrap();
    // Preserve noncanonical bytes too, not merely an equivalent JSON value.
    let before = format!(" {before}\n");
    fixture.server.inner.core.write_cron_data(&before).unwrap();
    let path = fixture.directory.path().join("data/agents/catalog.json");
    let mut catalog: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for agent in catalog["agents"].as_object_mut().unwrap().values_mut() {
        agent["config"]["fixture_padding"] = json!(vec![""; 8]);
    }
    let length = serde_json::to_vec_pretty(&catalog).unwrap().len();
    let padding = 2 * 1024 * 1024 + 1024 - length;
    for (index, agent) in catalog["agents"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .flat_map(|agent| {
            agent["config"]["fixture_padding"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
        })
        .enumerate()
    {
        *agent = json!("x".repeat(padding / 40 + usize::from(index < padding % 40)));
    }
    // Each config and the compact input fit; pretty-printed publication does not.
    let catalog = serde_json::to_vec(&catalog).unwrap();
    assert!(catalog.len() < 2 * 1024 * 1024);
    std::fs::write(&path, &catalog).unwrap();
    let connection =
        rusqlite::Connection::open(fixture.directory.path().join("core.sqlite")).unwrap();
    if fail_rollback {
        connection.execute_batch("CREATE TRIGGER fail_restart_rollback BEFORE INSERT ON core_settings WHEN NEW.key = 'desktop_cron_data' AND EXISTS (SELECT 1 FROM json_each(NEW.value, '$.states') WHERE json_extract(value, '$.last_status') = 'success') BEGIN SELECT RAISE(ABORT, 'fixture rollback failure'); END;").unwrap();
    }
    let result = scoped(
        &fixture,
        "default",
        "PATCH",
        "/api/agents/writer/toggle",
        json!({"enabled":true}),
    )
    .await;
    if fail_rollback {
        assert_eq!(
            result,
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"detail":"Agent restart rollback failed; Cron state requires recovery"})
            )
        );
        assert_ne!(
            fixture.server.inner.core.read_cron_data().unwrap(),
            Some(before)
        );
        assert_eq!(
            read_data(&fixture.server).unwrap().states[&writer].last_status,
            None
        );
    } else {
        assert_eq!(
            result,
            (
                StatusCode::PAYLOAD_TOO_LARGE,
                json!({"detail":"Rust Agent catalog is too large"})
            )
        );
        assert_eq!(
            fixture.server.inner.core.read_cron_data().unwrap(),
            Some(before)
        );
    }
    assert_eq!(std::fs::read(path).unwrap(), catalog);
}

#[tokio::test]
async fn reenable_clears_recent_state_preserves_history_and_survives_reopen() {
    let mut fixture = Fixture::new().await;
    let (_, writer) = pair(&fixture, false, "write fixture").await;
    start(&fixture, &writer).await;
    fixture.idle().await;
    fixture
        .request(
            "PATCH",
            "/api/agents/writer/toggle",
            json!({"enabled":false}),
        )
        .await;
    let before = serde_json::to_value(read_data(&fixture.server).unwrap()).unwrap();
    assert_eq!(before["states"][&writer]["last_status"], "success");
    fixture.reopen().await;
    fixture
        .request(
            "PATCH",
            "/api/agents/writer/toggle",
            json!({"enabled":true}),
        )
        .await;
    let mut expected = before;
    expected["states"][&writer] = json!({
        "next_run_at":null,"last_run_at":null,"last_status":null,"last_error":null
    });
    assert_eq!(
        serde_json::to_value(read_data(&fixture.server).unwrap()).unwrap(),
        expected
    );
    fixture
        .request(
            "PATCH",
            "/api/agents/writer/toggle",
            json!({"enabled":true}),
        )
        .await;
    assert_eq!(
        serde_json::to_value(read_data(&fixture.server).unwrap()).unwrap(),
        expected
    );
    fixture.reopen().await;
    assert_eq!(
        serde_json::to_value(read_data(&fixture.server).unwrap()).unwrap(),
        expected
    );
}

#[tokio::test]
async fn restart_rebuilds_each_trigger_and_preserves_all_unselected_data() {
    let fixture = Fixture::new().await;
    let (_, writer) = pair(&fixture, false, "unused").await;
    let mut data = read_data(&fixture.server).unwrap();
    let template = data.jobs.pop().unwrap();
    let cases = [
        (
            "cron",
            json!({"cron":"0 12 * * *"}),
            true,
            Some("2026-01-05T12:00:00Z"),
        ),
        (
            "interval",
            json!({"type":"once","run_at":"2026-01-01T09:00:00Z","repeat_every_days":2}),
            true,
            Some("2026-01-07T09:00:00Z"),
        ),
        (
            "past",
            json!({"type":"once","run_at":"2026-01-01T09:00:00Z"}),
            true,
            Some("2026-01-01T09:00:00Z"),
        ),
        ("disabled", json!({"cron":"0 12 * * *"}), false, None),
        ("invalid", json!({"cron":"invalid"}), true, None),
        (
            "zero",
            json!({"type":"once","run_at":"2026-01-01T09:00:00Z","repeat_every_days":0}),
            true,
            None,
        ),
        (
            "unknown",
            json!({"type":"unknown","run_at":"2026-01-01T09:00:00Z"}),
            true,
            None,
        ),
    ];
    data.states.remove(&writer);
    data.owners.remove(&writer);
    data.workspace_owners.remove(&writer);
    data.public_ids.remove(&writer);
    data.scheduled.remove(&writer);
    data.active_runs.insert(
        String::from("other-agent-running"),
        AgentRunClaim {
            job_id: data.jobs[0].id.clone().unwrap(),
            trigger: String::from("manual"),
            agent_id: Some(String::from("default")),
            data_key: Some(fixture.data_key("default")),
        },
    );
    for (id, schedule, enabled, _) in &cases {
        let mut job = template.clone();
        job.id = Some((*id).to_owned());
        job.public_id = None;
        job.schedule = serde_json::from_value(schedule.clone()).unwrap();
        job.enabled = *enabled;
        data.jobs.push(job);
        fixture.bind_job(&mut data, id, "writer");
        seed_previous_run(&mut data, id);
    }
    write_data(&fixture.server, &data).unwrap();
    let mut expected = serde_json::to_value(read_data(&fixture.server).unwrap()).unwrap();
    let now = "2026-01-05T10:00:00Z".parse().unwrap();
    let change =
        crate::desktop_cron::prepare_restart(&fixture.server, &fixture.data_key("writer"), now)
            .unwrap()
            .unwrap();
    change.apply(&fixture.server).unwrap();
    for (id, _, _, next) in &cases {
        expected["states"][id] =
            json!({"next_run_at":next,"last_run_at":null,"last_status":null,"last_error":null});
        expected["scheduled"]
            .as_array_mut()
            .unwrap()
            .push(json!(id));
    }
    expected["scheduled"]
        .as_array_mut()
        .unwrap()
        .sort_by_key(Value::to_string);
    for job in expected["jobs"].as_array_mut().unwrap() {
        if matches!(job["id"].as_str(), Some("invalid" | "zero" | "unknown")) {
            job["enabled"] = json!(false);
        }
    }
    assert_eq!(
        serde_json::to_value(read_data(&fixture.server).unwrap()).unwrap(),
        expected
    );
    assert!(
        crate::desktop_cron::prepare_restart(&fixture.server, &fixture.data_key("writer"), now)
            .unwrap()
            .is_none()
    );
}

fn seed_previous_run(data: &mut CronData, id: &str) {
    data.states.insert(
        id.to_owned(),
        serde_json::from_value(json!({
            "next_run_at":"2026-01-01T00:00:00Z", "last_run_at":"2025-12-31T00:00:00Z",
            "last_status":"error", "last_error":"retained in history"
        }))
        .unwrap(),
    );
    data.history.insert(
        id.to_owned(),
        vec![
            serde_json::from_value(json!({
                "run_at":"2025-12-31T00:00:00Z", "status":"error",
                "error":"retained in history", "trigger":"scheduled"
            }))
            .unwrap(),
        ],
    );
}

#[tokio::test]
async fn failed_cron_write_does_not_enable_agent_or_change_data() {
    let fixture = Fixture::new().await;
    let (_, writer) = pair(&fixture, false, "write fixture").await;
    start(&fixture, &writer).await;
    fixture.idle().await;
    fixture
        .request(
            "PATCH",
            "/api/agents/writer/toggle",
            json!({"enabled":false}),
        )
        .await;
    let before = fixture.server.inner.core.read_cron_data().unwrap();
    let catalog_path = fixture.directory.path().join("data/agents/catalog.json");
    let catalog = std::fs::read(&catalog_path).unwrap();
    let connection =
        rusqlite::Connection::open(fixture.directory.path().join("core.sqlite")).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_restart BEFORE INSERT ON core_settings WHEN NEW.key = 'desktop_cron_data' BEGIN SELECT RAISE(ABORT, 'fixture restart failure'); END;").unwrap();
    assert_eq!(
        scoped(
            &fixture,
            "default",
            "PATCH",
            "/api/agents/writer/toggle",
            json!({"enabled":true})
        )
        .await
        .0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(fixture.server.inner.core.read_cron_data().unwrap(), before);
    assert_eq!(std::fs::read(catalog_path).unwrap(), catalog);
}

#[tokio::test]
async fn no_jobs_does_not_create_cron_data_and_repeat_enable_does_not_parse_it() {
    let fixture = Fixture::new().await;
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Writer"}),
        )
        .await;
    for enabled in [false, true] {
        fixture
            .request(
                "PATCH",
                "/api/agents/writer/toggle",
                json!({"enabled":enabled}),
            )
            .await;
        assert_eq!(fixture.server.inner.core.read_cron_data().unwrap(), None);
    }
    fixture
        .server
        .inner
        .core
        .write_cron_data("invalid fixture")
        .unwrap();
    fixture
        .request(
            "PATCH",
            "/api/agents/writer/toggle",
            json!({"enabled":true}),
        )
        .await;
    assert_eq!(
        fixture.server.inner.core.read_cron_data().unwrap(),
        Some(String::from("invalid fixture"))
    );
    fixture
        .request(
            "PATCH",
            "/api/agents/writer/toggle",
            json!({"enabled":false}),
        )
        .await;
    let catalog = std::fs::read(fixture.directory.path().join("data/agents/catalog.json")).unwrap();
    assert_eq!(
        scoped(
            &fixture,
            "default",
            "PATCH",
            "/api/agents/writer/toggle",
            json!({"enabled":true})
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        std::fs::read(fixture.directory.path().join("data/agents/catalog.json")).unwrap(),
        catalog
    );
}
