use super::scope::scoped;
use super::*;
use crate::desktop_cron::owner;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn agent_copy_preserves_native_specs_but_not_execution_state() {
    let mut fixture = Fixture::new().await;
    let id = fixture.create(json!({}), "copy me", true).await;
    fixture.run(&id).await;
    fixture.idle().await;
    let before = read_data(&fixture.server).unwrap();
    assert!(!before.history[&id].is_empty());
    let copied = fixture
        .request(
            "POST",
            "/api/agents/default/copy",
            json!({"name":"Native copy","copy_jobs":true}),
        )
        .await;
    let agent = copied["id"].as_str().unwrap();
    let after = read_data(&fixture.server).unwrap();
    assert_eq!(after.jobs.len(), before.jobs.len() + 1);
    let job = after.jobs.last().unwrap();
    let key = job.id.as_deref().unwrap();
    assert_ne!(key, id);
    assert_eq!(owner(&after, key), agent);
    assert_eq!(
        serde_json::to_value(job.public_spec()).unwrap(),
        serde_json::to_value(before.jobs[0].public_spec()).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&after.states).unwrap(),
        serde_json::to_value(&before.states).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&after.history).unwrap(),
        serde_json::to_value(&before.history).unwrap()
    );
    assert_eq!(after.scheduled, before.scheduled);
    assert_eq!(after.active_triggers, before.active_triggers);
    assert_eq!(
        serde_json::to_value(&after.active_runs).unwrap(),
        serde_json::to_value(&before.active_runs).unwrap()
    );
    assert!(
        !std::path::Path::new(copied["workspace_dir"].as_str().unwrap())
            .join("jobs.json")
            .exists()
    );
    let serialized = serde_json::to_value(after).unwrap();
    fixture.reopen().await;
    assert_eq!(
        serde_json::to_value(read_data(&fixture.server).unwrap()).unwrap(),
        serialized
    );
}

#[tokio::test]
async fn copying_a_copy_keeps_enabled_specs_and_other_agents_unchanged() {
    let fixture = Fixture::new().await;
    let first = fixture.create(json!({}), "disabled", false).await;
    let second = fixture.create(json!({}), "enabled", false).await;
    let mut data = read_data(&fixture.server).unwrap();
    data.jobs[1].enabled = true;
    data.public_ids
        .insert(first, String::from("public-disabled"));
    data.public_ids
        .insert(second, String::from("public-enabled"));
    write_data(&fixture.server, &data).unwrap();
    let data = read_data(&fixture.server).unwrap();
    let specs = json!(
        data.jobs
            .iter()
            .map(CronJobSpec::public_spec)
            .collect::<Vec<_>>()
    );
    let mut source = String::from("default");
    for _ in 0..3 {
        let before = serde_json::to_value(read_data(&fixture.server).unwrap()).unwrap();
        let copied = fixture
            .request(
                "POST",
                &format!("/api/agents/{source}/copy"),
                json!({"copy_jobs":true}),
            )
            .await;
        let target = copied["id"].as_str().unwrap();
        assert_ne!(target, source);
        let after = read_data(&fixture.server).unwrap();
        let jobs = after
            .jobs
            .iter()
            .filter(|job| owner(&after, job.id.as_deref().unwrap()) == target)
            .collect::<Vec<_>>();
        assert_eq!(
            json!(jobs.iter().map(|job| job.public_spec()).collect::<Vec<_>>()),
            specs
        );
        let keys = jobs
            .iter()
            .map(|job| job.id.as_deref().unwrap())
            .collect::<BTreeSet<_>>();
        let mut retained = serde_json::to_value(&after).unwrap();
        retained["jobs"]
            .as_array_mut()
            .unwrap()
            .retain(|job| !keys.contains(job["id"].as_str().unwrap()));
        for field in ["owners", "public_ids", "workspace_owners"] {
            retained[field]
                .as_object_mut()
                .unwrap()
                .retain(|key, _| !keys.contains(key.as_str()));
            if retained[field].as_object().unwrap().is_empty() {
                retained.as_object_mut().unwrap().remove(field);
            }
        }
        assert_eq!(retained, before);
        source = target.to_owned();
    }
}

#[tokio::test]
async fn unchecked_copy_ignores_corrupt_cron_and_legacy_jobs_files() {
    let fixture = Fixture::new().await;
    fixture
        .server
        .inner
        .core
        .write_cron_data("corrupt native fixture")
        .unwrap();
    std::fs::write(
        fixture.directory.path().join("workspace/jobs.json"),
        "not a source",
    )
    .unwrap();
    let copied = fixture
        .request("POST", "/api/agents/default/copy", json!({}))
        .await;
    assert_eq!(
        fixture.server.inner.core.read_cron_data().unwrap(),
        Some(String::from("corrupt native fixture"))
    );
    assert!(
        !std::path::Path::new(copied["workspace_dir"].as_str().unwrap())
            .join("jobs.json")
            .exists()
    );
}

#[tokio::test]
async fn copying_no_jobs_does_not_create_a_cron_setting() {
    let fixture = Fixture::new().await;
    assert_eq!(fixture.server.inner.core.read_cron_data().unwrap(), None);
    fixture
        .request(
            "POST",
            "/api/agents/default/copy",
            json!({"copy_jobs":true}),
        )
        .await;
    assert_eq!(fixture.server.inner.core.read_cron_data().unwrap(), None);
}

#[tokio::test]
async fn copying_a_running_job_does_not_duplicate_or_interrupt_its_claim() {
    let fixture = Fixture::new().await;
    let id = fixture.create(json!({}), "hold", false).await;
    fixture.run(&id).await;
    fixture.wait_requests(1).await;
    let before = serde_json::to_value(read_data(&fixture.server).unwrap()).unwrap();
    assert_eq!(before["active_runs"].as_object().unwrap().len(), 1);
    tokio::time::timeout(
        Duration::from_secs(2),
        fixture.request(
            "POST",
            "/api/agents/default/copy",
            json!({"copy_jobs":true}),
        ),
    )
    .await
    .unwrap();
    let after = serde_json::to_value(read_data(&fixture.server).unwrap()).unwrap();
    assert_eq!(after["jobs"].as_array().unwrap().len(), 2);
    for field in [
        "states",
        "history",
        "scheduled",
        "active_runs",
        "active_triggers",
    ] {
        assert_eq!(after[field], before[field]);
    }
    assert_eq!(fixture.remote.requests.lock().unwrap().len(), 1);
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), shutdown(&fixture.server))
        .await
        .unwrap();
    let stopped = read_data(&fixture.server).unwrap();
    let key = stopped.jobs[1].id.as_ref().unwrap();
    assert!(stopped.active_runs.is_empty());
    assert!(!stopped.states.contains_key(key));
    assert!(!stopped.history.contains_key(key));
}

fn workspaces(fixture: &Fixture) -> Vec<std::path::PathBuf> {
    let directory = fixture.directory.path().join("data/workspaces");
    if !directory.exists() {
        return Vec::new();
    }
    let mut paths = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

#[tokio::test]
async fn copy_preflight_rejects_invalid_name_corrupt_data_and_capacity_without_artifacts() {
    let fixture = Fixture::new().await;
    fixture.create(json!({}), "source", false).await;
    let mut data = read_data(&fixture.server).unwrap();
    let template = data.jobs[0].clone();
    while data.jobs.len() < crate::desktop_cron::MAX_CRON_JOBS {
        let mut job = template.clone();
        job.id = Some(Uuid::now_v7().to_string());
        data.workspace_owners
            .insert(job.id.clone().unwrap(), fixture.data_key("default"));
        data.jobs.push(job);
    }
    write_data(&fixture.server, &data).unwrap();
    let catalog = std::fs::read(fixture.directory.path().join("data/agents/catalog.json")).unwrap();
    let paths = workspaces(&fixture);
    for (body, status) in [
        (
            json!({"copy_jobs":true,"name":"x".repeat(1025)}),
            StatusCode::BAD_REQUEST,
        ),
        (json!({"copy_jobs":true}), StatusCode::UNPROCESSABLE_ENTITY),
    ] {
        let before = fixture.server.inner.core.read_cron_data().unwrap();
        assert_eq!(
            scoped(
                &fixture,
                "default",
                "POST",
                "/api/agents/default/copy",
                body
            )
            .await
            .0,
            status
        );
        assert_eq!(fixture.server.inner.core.read_cron_data().unwrap(), before);
        assert_eq!(workspaces(&fixture), paths);
        assert_eq!(
            std::fs::read(fixture.directory.path().join("data/agents/catalog.json")).unwrap(),
            catalog
        );
    }
    fixture
        .server
        .inner
        .core
        .write_cron_data("corrupt")
        .unwrap();
    assert_eq!(
        scoped(
            &fixture,
            "default",
            "POST",
            "/api/agents/default/copy",
            json!({"copy_jobs":true})
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        fixture.server.inner.core.read_cron_data().unwrap(),
        Some(String::from("corrupt"))
    );
    assert_eq!(workspaces(&fixture), paths);
    assert_eq!(
        std::fs::read(fixture.directory.path().join("data/agents/catalog.json")).unwrap(),
        catalog
    );
}

#[tokio::test]
async fn cron_commit_failure_does_not_publish_or_leave_a_copied_agent() {
    let fixture = Fixture::new().await;
    fixture.create(json!({}), "source", false).await;
    let before = fixture.server.inner.core.read_cron_data().unwrap();
    let catalog = std::fs::read(fixture.directory.path().join("data/agents/catalog.json")).unwrap();
    let paths = workspaces(&fixture);
    let connection =
        rusqlite::Connection::open(fixture.directory.path().join("core.sqlite")).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_cron_copy BEFORE INSERT ON core_settings WHEN NEW.key = 'desktop_cron_data' BEGIN SELECT RAISE(ABORT, 'fixture copy failure'); END;").unwrap();
    assert_eq!(
        scoped(
            &fixture,
            "default",
            "POST",
            "/api/agents/default/copy",
            json!({"copy_jobs":true})
        )
        .await
        .0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(fixture.server.inner.core.read_cron_data().unwrap(), before);
    assert_eq!(
        std::fs::read(fixture.directory.path().join("data/agents/catalog.json")).unwrap(),
        catalog
    );
    assert_eq!(workspaces(&fixture), paths);
}

#[derive(Clone, Copy)]
enum CopyFault {
    Catalog,
    Rollback,
    Credential,
}

struct FaultCredentials {
    directory: std::path::PathBuf,
    fault: CopyFault,
    armed: std::sync::atomic::AtomicBool,
}

impl crate::DesktopCredentialStore for FaultCredentials {
    fn load_api_key(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }

    fn save_api_key(&self, _: Option<&str>) -> anyhow::Result<()> {
        panic!("fixture must not save model credentials")
    }

    fn save_agent_setting_secret(&self, _: &str, value: Option<&str>) -> anyhow::Result<()> {
        assert_eq!(value, None);
        if !self.armed.swap(false, std::sync::atomic::Ordering::SeqCst) {
            return Ok(());
        }
        if matches!(self.fault, CopyFault::Credential) {
            anyhow::bail!("fixture credential failure");
        }
        let catalog = self.directory.join("data/agents/catalog.json");
        std::fs::rename(&catalog, catalog.with_extension("saved"))?;
        std::fs::create_dir(&catalog)?;
        if matches!(self.fault, CopyFault::Rollback) {
            let connection = rusqlite::Connection::open(self.directory.join("core.sqlite"))?;
            connection.execute_batch("CREATE TRIGGER fail_copy_rollback BEFORE INSERT ON core_settings WHEN NEW.key = 'desktop_cron_data' AND json_array_length(NEW.value, '$.jobs') = 1 BEGIN SELECT RAISE(ABORT, 'fixture rollback failure'); END;")?;
        }
        Ok(())
    }
}

async fn copy_with_fault(fault: CopyFault) {
    let mut fixture = Fixture::new().await;
    fixture.create(json!({}), "source", false).await;
    let before = fixture.server.inner.core.read_cron_data().unwrap();
    let catalog = fixture.directory.path().join("data/agents/catalog.json");
    let original_catalog = std::fs::read(&catalog).unwrap();
    let paths = workspaces(&fixture);
    fixture.server.inner.shutdown.cancel();
    shutdown(&fixture.server).await;
    let credentials = Arc::new(FaultCredentials {
        directory: fixture.directory.path().to_path_buf(),
        fault,
        armed: std::sync::atomic::AtomicBool::new(false),
    });
    fixture.server = AppServer::new_desktop_with_stores_and_workspace(
        fixture.server.inner.core.clone(),
        fixture.directory.path(),
        String::from("copy-fault-shutdown"),
        credentials.clone(),
        &fixture.directory.path().join("data"),
        &fixture.directory.path().join("workspace"),
    )
    .unwrap();
    credentials
        .armed
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let (status, body) = scoped(
        &fixture,
        "default",
        "POST",
        "/api/agents/default/copy",
        json!({"copy_jobs":true}),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    if matches!(fault, CopyFault::Rollback) {
        assert_eq!(
            body,
            json!({"detail":"Agent copy rollback failed; workspace and credentials retained for recovery"})
        );
        assert_eq!(read_data(&fixture.server).unwrap().jobs.len(), 2);
        let retained = workspaces(&fixture);
        assert_eq!(retained.len(), paths.len() + 1);
        assert!(
            retained
                .iter()
                .any(|path| !paths.contains(path) && path.join("agent.json").is_file())
        );
    } else {
        assert_eq!(fixture.server.inner.core.read_cron_data().unwrap(), before);
        assert_eq!(workspaces(&fixture), paths);
    }
    if catalog.is_dir() {
        std::fs::remove_dir(&catalog).unwrap();
        std::fs::rename(catalog.with_extension("saved"), &catalog).unwrap();
    }
    assert_eq!(std::fs::read(catalog).unwrap(), original_catalog);
}

#[tokio::test]
async fn catalog_publication_failure_rolls_back_exact_cron_bytes_and_cleans_workspace() {
    copy_with_fault(CopyFault::Catalog).await;
}

#[tokio::test]
async fn rollback_failure_reports_recovery_and_retains_workspace() {
    copy_with_fault(CopyFault::Rollback).await;
}

#[tokio::test]
async fn credential_failure_does_not_leave_an_agent_workspace() {
    copy_with_fault(CopyFault::Credential).await;
}

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_agent_copy_modal_copies_native_jobs_only_when_checked() {
    let mut fixture = Fixture::new().await;
    let source_id = fixture
        .create(json!({}), "browser copy source", false)
        .await;
    fixture.server.inner.core.write_ui_language("en").unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    fixture.server.inner.shutdown.cancel();
    shutdown(&fixture.server).await;
    fixture.server = AppServer::new_desktop_with_stores_and_workspace(
        fixture.server.inner.core.clone(),
        &root.join("../console/dist"),
        String::from("agent-copy-shutdown"),
        Arc::new(Credentials),
        &fixture.directory.path().join("data"),
        &fixture.directory.path().join("workspace"),
    )
    .unwrap();
    let mut expected = serde_json::to_value(read_data(&fixture.server).unwrap()).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(fixture.server.clone().run_http(listener));
    let result = tokio::time::timeout(
        Duration::from_secs(90),
        tokio::process::Command::new("node")
            .arg(root.join("scripts/console_browser_smoke.mjs"))
            .args([&origin, "/agents", "--agent-jobs-copy"])
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
    let result = &report["pages"][0]["agentJobsCopy"];
    let with_jobs = result["withJobs"].as_str().unwrap();
    let without_jobs = result["withoutJobs"].as_str().unwrap();
    assert_ne!(with_jobs, without_jobs);
    assert_eq!(
        result,
        &json!({"withJobs":with_jobs,"withoutJobs":without_jobs,"defaults":true,"reload":true})
    );
    let after = read_data(&fixture.server).unwrap();
    assert_eq!(after.jobs.len(), 2);
    let copied = after.jobs.last().unwrap();
    let key = copied.id.as_deref().unwrap();
    let mut copied_spec = expected["jobs"][0].clone();
    copied_spec["id"] = json!(key);
    expected["jobs"].as_array_mut().unwrap().push(copied_spec);
    expected["owners"] = json!({key:with_jobs});
    expected["public_ids"] = json!({key:source_id});
    expected["workspace_owners"][key] = serde_json::to_value(fixture.data_key(with_jobs)).unwrap();
    expected["version"] = json!(4);
    assert_eq!(serde_json::to_value(after).unwrap(), expected);
    assert!(fixture.remote.requests.lock().unwrap().is_empty());
    fixture.reopen().await;
    assert_eq!(
        serde_json::to_value(read_data(&fixture.server).unwrap()).unwrap(),
        expected
    );
}
