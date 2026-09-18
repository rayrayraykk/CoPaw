//! Public registration changes must not change durable Cron ownership.

use super::*;
use crate::desktop_cron::owner_key;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn old_native_jobs_run_only_with_a_proven_historical_registration() {
    for historical in [false, true] {
        let mut fixture = Fixture::new().await;
        let (_, writer) = pair(&fixture, false, "write fixture").await;
        let mut old: Value =
            serde_json::from_str(&fixture.server.inner.core.read_cron_data().unwrap().unwrap())
                .unwrap();
        old["version"] = json!(3);
        old.as_object_mut().unwrap().remove("workspace_owners");
        fixture
            .server
            .inner
            .core
            .write_cron_data(&old.to_string())
            .unwrap();
        if historical {
            let path = fixture.directory.path().join("data/agents/catalog.json");
            let mut catalog: Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            catalog["schema_version"] = json!(1);
            catalog.as_object_mut().unwrap().remove("workspace_keys");
            for agent in catalog["agents"].as_object_mut().unwrap().values_mut() {
                agent.as_object_mut().unwrap().remove("data_key");
                std::fs::remove_file(
                    std::path::Path::new(agent["workspace_dir"].as_str().unwrap())
                        .join(crate::desktop_agents::identity::MARKER_NAME),
                )
                .unwrap();
            }
            std::fs::write(path, serde_json::to_vec(&catalog).unwrap()).unwrap();
        }
        fixture.reopen().await;
        if historical {
            start(&fixture, &writer).await;
            fixture.idle().await;
            let data = read_data(&fixture.server).unwrap();
            assert_eq!(data.states[&writer].last_status.as_deref(), Some("success"));
            assert_eq!(
                serde_json::to_value(owner_key(&data, &writer)).unwrap(),
                json!({"kind":"legacy_agent","id":"writer"})
            );
            let stored: Value =
                serde_json::from_str(&fixture.server.inner.core.read_cron_data().unwrap().unwrap())
                    .unwrap();
            assert_eq!(stored["version"], 4);
        } else {
            let _guard = fixture.server.inner.desktop_cron_lock.lock().await;
            let mut data = read_data(&fixture.server).unwrap();
            let job = find_job(&data, &writer).unwrap().clone();
            let error = enqueue(
                &fixture.server,
                &mut data,
                job,
                "manual",
                fixture.server.inner.core.operation_guard().unwrap(),
            )
            .await
            .unwrap_err();
            assert_eq!(error.0, StatusCode::NOT_FOUND);
            assert_eq!(
                serde_json::from_str::<Value>(
                    &fixture.server.inner.core.read_cron_data().unwrap().unwrap()
                )
                .unwrap(),
                old
            );
            assert!(fixture.remote.requests.lock().unwrap().is_empty());
        }
    }
}

#[tokio::test]
async fn retained_workspace_runs_and_copies_under_new_id_without_giving_jobs_to_reused_id() {
    let mut fixture = Fixture::new().await;
    let (default_job, retained_job) = pair(&fixture, false, "write fixture").await;
    let root = fixture.directory.path().join("data/workspaces/writer");
    let key = fixture.data_key("writer");
    start(&fixture, &retained_job).await;
    fixture.idle().await;
    let initial_inbox = fixture.inbox()["events"].clone();
    fixture
        .request("DELETE", "/api/agents/writer", Value::Null)
        .await;
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"editor","name":"Editor","workspace_dir":root}),
        )
        .await;
    fixture.request("POST", "/api/agents", json!({"id":"writer","name":"New Writer","workspace_dir":fixture.directory.path().join("new-writer")})).await;
    assert_eq!(fixture.data_key("editor"), key);
    assert_ne!(fixture.data_key("writer"), key);
    fixture.reopen().await;
    start(&fixture, &retained_job).await;
    fixture.idle().await;
    let data = read_data(&fixture.server).unwrap();
    assert_eq!(owner_key(&data, &retained_job), key);
    assert_eq!(data.history[&retained_job].len(), 2);
    assert!(
        data.history[&retained_job]
            .iter()
            .all(|entry| entry.status == "success")
    );
    assert!(!data.history.contains_key(&default_job));
    assert!(data.active_runs.is_empty());
    assert_eq!(
        std::fs::read_to_string(root.join("cron-output.txt")).unwrap(),
        "created by Cron"
    );
    assert!(
        !fixture
            .directory
            .path()
            .join("new-writer/cron-output.txt")
            .exists()
    );
    let events = fixture.inbox()["events"].as_array().unwrap().clone();
    for event in initial_inbox.as_array().unwrap() {
        assert!(events.contains(event));
    }
    let mut actors = events
        .iter()
        .map(|event| event["agent_id"].as_str().unwrap())
        .collect::<Vec<_>>();
    actors.sort_unstable();
    assert_eq!(actors, vec!["editor", "writer"]);

    for (agent, expected_jobs) in [("editor", 1), ("writer", 0)] {
        let copied = fixture
            .request(
                "POST",
                &format!("/api/agents/{agent}/copy"),
                json!({"copy_jobs":true}),
            )
            .await;
        let target_key = fixture.data_key(copied["id"].as_str().unwrap());
        let data = read_data(&fixture.server).unwrap();
        let jobs = data
            .jobs
            .iter()
            .filter(|job| owner_key(&data, job.id.as_deref().unwrap()) == target_key)
            .collect::<Vec<_>>();
        assert_eq!(jobs.len(), expected_jobs, "{agent}");
        for job in jobs {
            assert_eq!(
                serde_json::to_value(job.public_spec()).unwrap(),
                serde_json::to_value(find_job(&data, &retained_job).unwrap().public_spec())
                    .unwrap()
            );
            assert!(!data.history.contains_key(job.id.as_deref().unwrap()));
        }
    }
    let before = read_data(&fixture.server).unwrap();
    for enabled in [false, true] {
        fixture
            .request(
                "PATCH",
                "/api/agents/editor/toggle",
                json!({"enabled":enabled}),
            )
            .await;
    }
    let after = read_data(&fixture.server).unwrap();
    assert_eq!(
        serde_json::to_value(&after.history).unwrap(),
        serde_json::to_value(&before.history).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&after.states[&default_job]).unwrap(),
        serde_json::to_value(&before.states[&default_job]).unwrap()
    );
    assert_eq!(after.states[&retained_job].last_status, None);
}

#[tokio::test]
async fn orphaned_workspace_cannot_enqueue_through_a_new_agent_with_the_same_label() {
    let fixture = Fixture::new().await;
    let (_, job_id) = pair(&fixture, false, "write fixture").await;
    fixture
        .request("DELETE", "/api/agents/writer", Value::Null)
        .await;
    fixture.request("POST", "/api/agents", json!({"id":"writer","name":"New Writer","workspace_dir":fixture.directory.path().join("different")})).await;
    let before = fixture
        .server
        .inner
        .core
        .backup_snapshot(1024 * 1024)
        .unwrap();
    let _guard = fixture.server.inner.desktop_cron_lock.lock().await;
    let mut data = read_data(&fixture.server).unwrap();
    let job = find_job(&data, &job_id).unwrap().clone();
    let error = enqueue(
        &fixture.server,
        &mut data,
        job,
        "manual",
        fixture.server.inner.core.operation_guard().unwrap(),
    )
    .await
    .unwrap_err();
    assert_eq!(
        (error.0, error.1.0),
        (
            StatusCode::NOT_FOUND,
            json!({"detail":"Cron Workspace has no registered Agent"})
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
    assert!(fixture.remote.requests.lock().unwrap().is_empty());
}
