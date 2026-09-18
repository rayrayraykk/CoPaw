//! Exercise original Job HTTP and scheduling through real Agent bindings.

use super::*;
use crate::desktop_cron::CronJobState;
use pretty_assertions::assert_eq;

async fn ok(fixture: &Fixture, agent: &str, method: &str, path: &str, body: Value) -> Value {
    let (status, body) = scoped(fixture, agent, method, path, body).await;
    assert_eq!(status, StatusCode::OK, "{agent}: {method} {path}: {body}");
    body
}

async fn check_writer_reads_and_toggles(fixture: &Fixture, expected: &Value, state: &CronJobState) {
    assert_eq!(
        ok(fixture, "writer", "GET", "/api/cron/jobs", Value::Null).await,
        json!([expected])
    );
    assert_eq!(
        ok(
            fixture,
            "writer",
            "GET",
            "/api/cron/jobs/shared-job",
            Value::Null
        )
        .await,
        json!({"spec":expected,"state":state})
    );
    for (action, response) in [
        ("resume", json!({"resumed":true})),
        ("pause", json!({"paused":true})),
    ] {
        assert_eq!(
            ok(
                fixture,
                "writer",
                "POST",
                &format!("/api/cron/jobs/shared-job/{action}"),
                Value::Null
            )
            .await,
            response
        );
    }
}

#[tokio::test]
async fn public_job_crud_uses_the_request_workspace_and_preserves_other_jobs() {
    let mut fixture = Fixture::new().await;
    let (first, second) = pair(&fixture, false, "write fixture").await;
    let before = read_data(&fixture.server).unwrap();
    let expected = serde_json::to_value(find_job(&before, &second).unwrap().public_spec()).unwrap();
    check_writer_reads_and_toggles(&fixture, &expected, &before.states[&second]).await;
    let mut replacement = expected.clone();
    replacement["name"] = json!("Writer edited");
    replacement["meta"] = json!({"agent_id":"default"});
    assert_eq!(
        ok(
            &fixture,
            "writer",
            "PUT",
            "/api/cron/jobs/shared-job",
            replacement.clone()
        )
        .await,
        replacement
    );
    let created = ok(
        &fixture,
        "writer",
        "POST",
        "/api/cron/jobs",
        replacement.clone(),
    )
    .await;
    assert_ne!(created["id"], replacement["id"]);
    let new_id = created["id"].as_str().unwrap();
    let mut expected_created = replacement.clone();
    expected_created["id"] = json!(new_id);
    assert_eq!(created, expected_created);
    for suffix in ["", "/state", "/history"] {
        assert_eq!(
            scoped(
                &fixture,
                "default",
                "GET",
                &format!("/api/cron/jobs/{new_id}{suffix}"),
                Value::Null
            )
            .await,
            (StatusCode::NOT_FOUND, json!({"detail":"job not found"}))
        );
    }
    assert_eq!(
        ok(
            &fixture,
            "writer",
            "GET",
            "/api/cron/jobs/shared-job/history",
            Value::Null
        )
        .await,
        json!([])
    );
    fixture.reopen().await;
    assert_eq!(
        ok(
            &fixture,
            "writer",
            "GET",
            "/api/cron/jobs/shared-job",
            Value::Null
        )
        .await["spec"],
        replacement
    );
    for id in ["shared-job", new_id] {
        assert_eq!(
            ok(
                &fixture,
                "writer",
                "DELETE",
                &format!("/api/cron/jobs/{id}"),
                Value::Null
            )
            .await,
            json!({"deleted":true})
        );
    }
    assert_eq!(
        ok(&fixture, "writer", "GET", "/api/cron/jobs", Value::Null).await,
        json!([])
    );
    let after = read_data(&fixture.server).unwrap();
    assert_eq!(
        serde_json::to_value(&after.jobs).unwrap(),
        json!([before.jobs[0]])
    );
    assert_eq!(
        serde_json::to_value(&after.states[&first]).unwrap(),
        serde_json::to_value(&before.states[&first]).unwrap()
    );
    assert_eq!(
        after.workspace_owners[&first],
        before.workspace_owners[&first]
    );
    assert!(after.history.is_empty());
    assert!(fixture.remote.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn public_put_creates_only_in_its_namespace_even_with_a_foreign_public_id() {
    let fixture = Fixture::new().await;
    let (first, second) = pair(&fixture, false, "write fixture").await;
    let mut data = read_data(&fixture.server).unwrap();
    data.public_ids.insert(second, String::from("writer-job"));
    write_data(&fixture.server, &data).unwrap();
    let template = serde_json::to_value(find_job(&data, &first).unwrap().public_spec()).unwrap();
    assert_eq!(
        ok(
            &fixture,
            "writer",
            "PUT",
            "/api/cron/jobs/shared-job",
            template.clone()
        )
        .await,
        template
    );
    let after = read_data(&fixture.server).unwrap();
    assert_eq!(after.jobs.len(), 3);
    assert_eq!(
        serde_json::to_value(&after.jobs[..2]).unwrap(),
        serde_json::to_value(&data.jobs).unwrap()
    );
    let key = after.jobs[2].id.as_deref().unwrap();
    assert_ne!(key, first);
    assert_eq!(after.jobs[2].public_id(), Some("shared-job"));
    assert_eq!(
        crate::desktop_cron::owner_key(&after, key),
        fixture.data_key("writer")
    );
}

#[tokio::test]
async fn public_parallel_runs_keep_models_files_and_same_named_sessions_separate() {
    let fixture = Fixture::new().await;
    let (first, second) = pair(&fixture, false, "write fixture").await;
    let (default, writer) = tokio::join!(
        scoped(
            &fixture,
            "default",
            "POST",
            "/api/cron/jobs/shared-job/run",
            Value::Null
        ),
        scoped(
            &fixture,
            "writer",
            "POST",
            "/api/cron/jobs/shared-job/run",
            Value::Null
        )
    );
    assert_eq!(default, (StatusCode::OK, json!({"started":true})));
    assert_eq!(writer, (StatusCode::OK, json!({"started":true})));
    fixture.idle().await;
    let data = read_data(&fixture.server).unwrap();
    for (actor, key, path) in [
        ("default", first, "workspace"),
        ("writer", second, "data/workspaces/writer"),
    ] {
        assert_eq!(data.states[&key].last_status.as_deref(), Some("success"));
        assert_eq!(
            ok(
                &fixture,
                actor,
                "GET",
                "/api/cron/jobs/shared-job/state",
                Value::Null
            )
            .await,
            serde_json::to_value(&data.states[&key]).unwrap()
        );
        assert_eq!(
            ok(
                &fixture,
                actor,
                "GET",
                "/api/cron/jobs/shared-job/history",
                Value::Null
            )
            .await,
            serde_json::to_value(&data.history[&key]).unwrap()
        );
        assert_eq!(
            std::fs::read_to_string(fixture.directory.path().join(path).join("cron-output.txt"))
                .unwrap(),
            "created by Cron"
        );
    }
    let mut models = fixture
        .remote
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|r| r["model"].clone())
        .collect::<Vec<_>>();
    models.sort_by_key(Value::to_string);
    assert_eq!(
        models,
        json!([
            "cron-fixture",
            "cron-fixture",
            "writer-model",
            "writer-model"
        ])
        .as_array()
        .unwrap()
        .clone()
    );
    assert_eq!(fixture.catalog()["chats"].as_object().unwrap().len(), 2);
}

#[tokio::test]
async fn scheduler_runs_registered_non_default_jobs_without_default_fallback() {
    let fixture = Fixture::new().await;
    let (first, second) = pair(&fixture, false, "write fixture").await;
    let now = Utc::now();
    let mut data = read_data(&fixture.server).unwrap();
    for job in &mut data.jobs {
        job.enabled = true;
        job.schedule.kind = String::from("once");
        job.schedule.cron = None;
        job.schedule.run_at = Some(crate::desktop_cron::format_datetime(now));
    }
    for job in data.jobs.clone() {
        crate::desktop_cron::reset_schedule(&mut data, &job, now).unwrap();
    }
    write_data(&fixture.server, &data).unwrap();
    crate::desktop_cron::runtime::tick(&fixture.server, now)
        .await
        .unwrap();
    fixture.idle().await;
    let after = read_data(&fixture.server).unwrap();
    for key in [first, second] {
        assert_eq!(after.states[&key].last_status.as_deref(), Some("success"));
        assert_eq!(after.history[&key][0].trigger, "scheduled");
    }
}

#[tokio::test]
async fn invalid_or_disabled_scopes_reject_all_job_operations_without_changes() {
    for condition in ["missing", "disabled", "marker"] {
        let fixture = Fixture::new().await;
        let (_, second) = pair(&fixture, false, "write fixture").await;
        if condition == "disabled" {
            fixture
                .request(
                    "PATCH",
                    "/api/agents/writer/toggle",
                    json!({"enabled":false}),
                )
                .await;
        } else if condition == "marker" {
            std::fs::remove_file(
                fixture
                    .directory
                    .path()
                    .join("data/workspaces/writer")
                    .join(crate::desktop_agents::identity::MARKER_NAME),
            )
            .unwrap();
        }
        let data = read_data(&fixture.server).unwrap();
        let spec = serde_json::to_value(find_job(&data, &second).unwrap().public_spec()).unwrap();
        let before = fixture
            .server
            .inner
            .core
            .backup_snapshot(4 * 1024 * 1024)
            .unwrap();
        let actor = if condition == "missing" {
            "missing"
        } else {
            "writer"
        };
        let expected = match condition {
            "missing" => StatusCode::NOT_FOUND,
            "disabled" => StatusCode::FORBIDDEN,
            _ => StatusCode::CONFLICT,
        };
        for (method, suffix) in [
            ("GET", ""),
            ("POST", ""),
            ("GET", "/shared-job"),
            ("PUT", "/shared-job"),
            ("DELETE", "/shared-job"),
            ("POST", "/shared-job/pause"),
            ("POST", "/shared-job/resume"),
            ("POST", "/shared-job/run"),
            ("GET", "/shared-job/state"),
            ("GET", "/shared-job/history"),
        ] {
            let (status, body) = scoped(
                &fixture,
                actor,
                method,
                &format!("/api/cron/jobs{suffix}"),
                spec.clone(),
            )
            .await;
            assert_eq!(status, expected, "{condition} {method} {suffix}: {body}");
        }
        assert_eq!(
            fixture
                .server
                .inner
                .core
                .backup_snapshot(4 * 1024 * 1024)
                .unwrap(),
            before
        );
        assert!(fixture.remote.requests.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn scheduler_skips_invalid_owner_but_runs_a_healthy_copied_job() {
    let fixture = Fixture::new().await;
    let (first, second) = pair(&fixture, false, "write fixture").await;
    let now = Utc::now();
    let mut data = read_data(&fixture.server).unwrap();
    for job in &mut data.jobs {
        job.enabled = true;
        job.schedule.kind = String::from("once");
        job.schedule.cron = None;
        job.schedule.run_at = Some(crate::desktop_cron::format_datetime(now));
    }
    for job in data.jobs.clone() {
        crate::desktop_cron::reset_schedule(&mut data, &job, now).unwrap();
    }
    write_data(&fixture.server, &data).unwrap();
    let copy = fixture
        .request("POST", "/api/agents/writer/copy", json!({"copy_jobs":true}))
        .await;
    let actor = copy["id"].as_str().unwrap();
    fixture
        .request(
            "PATCH",
            "/api/agents/writer/toggle",
            json!({"enabled":false}),
        )
        .await;
    std::fs::remove_file(
        fixture
            .directory
            .path()
            .join("workspace")
            .join(crate::desktop_agents::identity::MARKER_NAME),
    )
    .unwrap();
    let before = read_data(&fixture.server).unwrap();
    crate::desktop_cron::runtime::tick(&fixture.server, now)
        .await
        .unwrap();
    fixture.idle().await;
    let after = read_data(&fixture.server).unwrap();
    for key in [first, second] {
        assert_eq!(
            serde_json::to_value(&after.states[&key]).unwrap(),
            serde_json::to_value(&before.states[&key]).unwrap()
        );
        assert!(!after.history.contains_key(&key));
    }
    let history = ok(
        &fixture,
        actor,
        "GET",
        "/api/cron/jobs/shared-job/history",
        Value::Null,
    )
    .await;
    assert_eq!(history.as_array().unwrap().len(), 1);
    assert_eq!(history[0]["status"], "success");
    assert_eq!(history[0]["trigger"], "scheduled");
    let requests = fixture.remote.requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .map(|r| r["model"].clone())
            .collect::<Vec<_>>(),
        vec![json!("writer-model"), json!("writer-model")]
    );
}

#[tokio::test]
async fn public_jobs_follow_retained_workspace_not_a_reused_agent_name() {
    let mut fixture = Fixture::new().await;
    let (_, second) = pair(&fixture, false, "write fixture").await;
    let old_root = fixture.directory.path().join("data/workspaces/writer");
    let expected = serde_json::to_value(
        find_job(&read_data(&fixture.server).unwrap(), &second)
            .unwrap()
            .public_spec(),
    )
    .unwrap();
    fixture
        .request("DELETE", "/api/agents/writer", Value::Null)
        .await;
    fixture.request("POST", "/api/agents", json!({"id":"writer","name":"New writer","workspace_dir":fixture.directory.path().join("new-writer")})).await;
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"editor","name":"Editor","workspace_dir":old_root}),
        )
        .await;
    fixture.reopen().await;
    assert_eq!(
        ok(&fixture, "writer", "GET", "/api/cron/jobs", Value::Null).await,
        json!([])
    );
    assert_eq!(
        ok(&fixture, "editor", "GET", "/api/cron/jobs", Value::Null).await,
        json!([expected])
    );
    assert_eq!(
        scoped(
            &fixture,
            "writer",
            "POST",
            "/api/cron/jobs/shared-job/run",
            Value::Null
        )
        .await,
        (StatusCode::NOT_FOUND, json!({"detail":"job not found"}))
    );
    assert_eq!(
        ok(
            &fixture,
            "editor",
            "POST",
            "/api/cron/jobs/shared-job/run",
            Value::Null
        )
        .await,
        json!({"started":true})
    );
    fixture.idle().await;
    assert_eq!(
        read_data(&fixture.server).unwrap().states[&second]
            .last_status
            .as_deref(),
        Some("success")
    );
    assert_eq!(
        std::fs::read_to_string(old_root.join("cron-output.txt")).unwrap(),
        "created by Cron"
    );
    assert!(
        !fixture
            .directory
            .path()
            .join("new-writer/cron-output.txt")
            .exists()
    );
}

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_cron_page_switches_agent_and_runs_scoped_crud() {
    let mut fixture = Fixture::new().await;
    pair(&fixture, false, "write fixture").await;
    for (actor, user, session) in [
        ("writer", "admin", "cron-browser-session"),
        ("writer", "another-user", "wrong-user-session"),
        ("default", "admin", "hidden-agent-session"),
    ] {
        ok(
            &fixture,
            actor,
            "POST",
            "/api/chats",
            json!({"name":"Known target","user_id":user,"session_id":session}),
        )
        .await;
    }
    let before = serde_json::to_value(read_data(&fixture.server).unwrap()).unwrap();
    fixture.server.inner.shutdown.cancel();
    shutdown(&fixture.server).await;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    fixture.server = AppServer::new_desktop_with_stores_and_workspace(
        Core::persistent(
            fixture.model.clone(),
            &fixture.directory.path().join("core.sqlite"),
        )
        .unwrap(),
        &root.join("../console/dist"),
        String::from("cron-writer-browser-shutdown"),
        Arc::new(Credentials),
        &fixture.directory.path().join("data"),
        &fixture.directory.path().join("workspace"),
    )
    .unwrap();
    fixture.server.inner.core.write_ui_language("en").unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(fixture.server.clone().run_http(listener));
    let result = tokio::time::timeout(
        Duration::from_secs(100),
        tokio::process::Command::new("node")
            .arg(root.join("scripts/console_browser_smoke.mjs"))
            .args([&origin, "/cron-jobs", "--cron-writer-crud"])
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
        report["pages"][0]["cronCrud"],
        json!({"created":true,"toggled":true,"manual":true,"history":true,"edited":true,"reload":true,"deleted":true,"scoped":"writer","agent":true,"knownTargets":true})
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
    assert!(
        !fixture
            .directory
            .path()
            .join("workspace/cron-output.txt")
            .exists()
    );
    assert_eq!(
        serde_json::to_value(read_data(&fixture.server).unwrap()).unwrap(),
        before
    );
    assert_eq!(
        fixture
            .remote
            .requests
            .lock()
            .unwrap()
            .iter()
            .map(|r| r["model"].clone())
            .collect::<Vec<_>>(),
        vec![json!("writer-model"), json!("writer-model")]
    );
}
