use pretty_assertions::assert_eq;
use qwenpaw_core::Core;
use qwenpaw_core::ModelConfig;

use super::super::*;
use super::*;

fn model() -> ModelConfig {
    ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1/v1"),
        default_model: String::from("cron-fixture"),
    }
}

fn server() -> AppServer {
    AppServer::new(Core::new(model()))
}

fn instant(value: &str) -> DateTime<Utc> {
    schedule::datetime(value, chrono_tz::UTC).unwrap()
}

fn spec(schedule: Value) -> CronJobSpec {
    let mut value = json!({"id":"job", "name":"Reminder",
        "task_type":"text", "text":"  scheduled text  ", "save_result_to_inbox":true,
        "dispatch":{"target":{"user_id":"admin", "session_id":"cron-session"}}});
    value["schedule"] = schedule;
    serde_json::from_value(value).unwrap()
}

fn store(server: &AppServer, mut spec: CronJobSpec, now: DateTime<Utc>) {
    validate_and_normalize(&mut spec).unwrap();
    let mut data = read_data(server).unwrap();
    reset_schedule(&mut data, &spec, now).unwrap();
    data.workspace_owners.insert(
        spec.id.clone().unwrap(),
        super::super::super::desktop_agents::default_data_key(server).unwrap(),
    );
    data.jobs.push(spec);
    write_data(server, &data).unwrap();
}

#[tokio::test]
async fn overdue_cron_coalesces_to_latest_slot_and_records_one_real_delivery() {
    let server = server();
    let first = instant("2030-01-01T09:00:00Z");
    store(&server, spec(json!({"cron":"* * * * *"})), first);
    tick(&server, instant("2030-01-01T10:05:10Z"))
        .await
        .unwrap();
    tick(&server, instant("2030-01-01T10:05:20Z"))
        .await
        .unwrap();
    let data = read_data(&server).unwrap();
    assert_eq!(
        data.states["job"].next_run_at,
        Some(String::from("2030-01-01T10:06:00Z"))
    );
    assert_eq!(data.history["job"].len(), 1);
    assert_eq!(
        json_value(&data.history["job"]).unwrap(),
        json!([{
            "run_at":data.states["job"].last_run_at, "status":"success", "error":null, "trigger":"scheduled"
        }])
    );
    let messages = server.inner.desktop_push_messages.read().await;
    assert_eq!(
        messages
            .iter()
            .map(|message| (&*message.text, &*message.session_id))
            .collect::<Vec<_>>(),
        vec![("scheduled text", "cron-session")]
    );
    let inbox: Value =
        serde_json::from_str(&server.inner.core.read_inbox_data().unwrap().unwrap()).unwrap();
    assert_eq!(inbox["events"][0]["payload"]["trigger"], "scheduled");
    assert!(data.active_triggers.is_empty());
}

#[tokio::test]
async fn missed_one_shot_is_skipped_once_and_manual_does_not_consume_schedule() {
    let server = server();
    let now = instant("2030-01-01T09:00:00Z");
    let mut job = spec(json!({"type":"once", "run_at":"2030-01-01T08:00:00Z"}));
    job.runtime.misfire_grace_seconds = 60;
    store(&server, job, now);
    tick(&server, now).await.unwrap();
    tick(&server, now).await.unwrap();
    let data = read_data(&server).unwrap();
    assert_eq!(
        json_value(&data.states["job"]).unwrap(),
        json!({"next_run_at":null,"last_run_at":null,"last_status":"skipped","last_error":"missed scheduled run at 2030-01-01T08:00:00Z: late by 3600s, grace=60s"})
    );
    assert_eq!(data.history["job"].len(), 1);
    assert!(server.inner.desktop_push_messages.read().await.is_empty());
    let Json(response) = run_job(
        State(server.clone()),
        HeaderMap::new(),
        Path(String::from("job")),
    )
    .await
    .unwrap();
    assert_eq!(response, json!({"started":true}));
    assert_eq!(read_data(&server).unwrap().states["job"].next_run_at, None);
    assert_eq!(read_data(&server).unwrap().history["job"].len(), 2);
    tick(&server, now).await.unwrap();
    assert_eq!(read_data(&server).unwrap().history["job"].len(), 2);
}

#[tokio::test]
async fn repeat_count_pause_resume_replace_and_delete_preserve_cursor_rules() {
    let server = server();
    let start = instant("2030-01-01T09:00:00Z");
    store(
        &server,
        spec(
            json!({"type":"once","run_at":"2030-01-01T09:00:00Z", "repeat_every_days":2,"repeat_end_type":"count","repeat_count":2}),
        ),
        start,
    );
    let _ = run_job(
        State(server.clone()),
        HeaderMap::new(),
        Path(String::from("job")),
    )
    .await
    .unwrap();
    assert_eq!(
        read_data(&server).unwrap().states["job"]
            .next_run_at
            .as_deref(),
        Some("2030-01-01T09:00:00Z")
    );
    tick(&server, start).await.unwrap();
    assert_eq!(
        read_data(&server).unwrap().states["job"]
            .next_run_at
            .as_deref(),
        Some("2030-01-03T09:00:00Z")
    );
    set_job_enabled(&server, &HeaderMap::new(), "job", false)
        .await
        .unwrap();
    tick(&server, instant("2030-01-03T09:00:00Z"))
        .await
        .unwrap();
    assert_eq!(read_data(&server).unwrap().history["job"].len(), 2);
    let mut data = read_data(&server).unwrap();
    data.jobs[0].enabled = true;
    let job = data.jobs[0].clone();
    reset_schedule(&mut data, &job, instant("2030-01-02T09:00:00Z")).unwrap();
    write_data(&server, &data).unwrap();
    tick(&server, instant("2030-01-03T09:00:00Z"))
        .await
        .unwrap();
    tick(&server, instant("2030-01-05T09:00:00Z"))
        .await
        .unwrap();
    assert_eq!(read_data(&server).unwrap().history["job"].len(), 3);
    assert_eq!(read_data(&server).unwrap().states["job"].next_run_at, None);
    let mut job = spec(json!({"cron":"0 9 * * *"}));
    job.enabled = false;
    let _ = replace_job(
        State(server.clone()),
        HeaderMap::new(),
        Path(String::from("job")),
        Json(job),
    )
    .await
    .unwrap();
    assert_eq!(read_data(&server).unwrap().states["job"].next_run_at, None);
    let _ = delete_job(
        State(server.clone()),
        HeaderMap::new(),
        Path(String::from("job")),
    )
    .await
    .unwrap();
    tick(&server, start).await.unwrap();
    let data = read_data(&server).unwrap();
    assert_eq!(
        json_value(data).unwrap(),
        json!({"version":4,"jobs":[],"states":{},"history":{},"scheduled":[],"active_triggers":{}})
    );
}

#[tokio::test]
async fn durable_restart_never_replays_completed_or_in_flight_one_shot() {
    let temporary = tempfile::tempdir().unwrap();
    let database = temporary.path().join("core.sqlite");
    let now = instant("2030-01-01T09:00:00Z");
    {
        let server = AppServer::new(Core::persistent(model(), &database).unwrap());
        store(
            &server,
            spec(json!({"type":"once","run_at":"2030-01-01T09:00:00Z"})),
            now,
        );
        tick(&server, now).await.unwrap();
    }
    let server = AppServer::new(Core::persistent(model(), &database).unwrap());
    tick(&server, now).await.unwrap();
    assert!(server.inner.desktop_push_messages.read().await.is_empty());
    assert_eq!(read_data(&server).unwrap().history["job"].len(), 1);
    let mut data = read_data(&server).unwrap();
    data.active_triggers
        .insert(String::from("job"), String::from("manual"));
    data.states.get_mut("job").unwrap().last_status = Some(String::from("running"));
    write_data(&server, &data).unwrap();
    drop(server);
    let server = AppServer::new(Core::persistent(model(), &database).unwrap());
    tick(&server, now).await.unwrap();
    tick(&server, now).await.unwrap();
    assert_eq!(read_data(&server).unwrap().history["job"].len(), 2);
    assert_eq!(
        read_data(&server).unwrap().history["job"][1].status,
        "cancelled"
    );
    assert_eq!(
        read_data(&server).unwrap().history["job"][1].trigger,
        "manual"
    );
    assert!(server.inner.desktop_push_messages.read().await.is_empty());
}

#[tokio::test]
async fn restore_and_shutdown_block_background_delivery() {
    let server = server();
    let now = instant("2030-01-01T09:00:00Z");
    store(
        &server,
        spec(json!({"type":"once", "run_at":"2030-01-01T09:00:00Z"})),
        now,
    );
    let restoring = server
        .inner
        .core
        .begin_restore(Duration::from_secs(2))
        .await
        .unwrap();
    tick(&server, now).await.unwrap();
    assert!(server.inner.desktop_push_messages.read().await.is_empty());
    drop(restoring);
    tick(&server, now).await.unwrap();
    assert_eq!(server.inner.desktop_push_messages.read().await.len(), 1);
    server.inner.shutdown.cancel();
    tick(&server, now).await.unwrap();
    assert_eq!(server.inner.desktop_push_messages.read().await.len(), 1);
}

#[tokio::test]
async fn scheduler_fires_without_requests_and_exits_with_server() {
    let server = server();
    let due = Utc::now() + chrono::Duration::milliseconds(350);
    store(
        &server,
        spec(json!({"type":"once", "run_at":format_datetime(due)})),
        Utc::now(),
    );
    let task = spawn_scheduler(&server);
    tokio::time::timeout(Duration::from_secs(5), async {
        while server.inner.desktop_push_messages.read().await.is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(server.inner.desktop_push_messages.read().await.len(), 1);
    assert_eq!(
        read_data(&server).unwrap().history["job"][0].trigger,
        "scheduled"
    );
}

#[tokio::test]
async fn malformed_legacy_schedule_is_disabled_and_unsupported_task_is_not_success() {
    let server = server();
    let now = instant("2030-01-01T09:00:00Z");
    let invalid = spec(json!({"cron":"60 0 * * *"}));
    let mut data = CronData::default();
    data.jobs.push(invalid);
    write_data(&server, &data).unwrap();
    tick(&server, now).await.unwrap();
    let data = read_data(&server).unwrap();
    assert!(!data.jobs[0].enabled);
    assert_eq!(data.states["job"].last_status.as_deref(), Some("error"));
    assert!(server.inner.desktop_push_messages.read().await.is_empty());
    let mut job = spec(json!({"type":"once", "run_at":"2030-01-01T09:00:00Z"}));
    job.id = Some(String::from("agent-job"));
    job.task_type = String::from("agent");
    job.dispatch.channel = String::from("unimplemented-channel");
    job.request = Some(json!({"input":"run the agent"}));
    store(&server, job, now);
    tick(&server, now).await.unwrap();
    assert_eq!(
        read_data(&server).unwrap().states["agent-job"]
            .last_status
            .as_deref(),
        Some("error")
    );
    assert_eq!(
        run_job(
            State(server.clone()),
            HeaderMap::new(),
            Path(String::from("agent-job"))
        )
        .await
        .unwrap_err()
        .0,
        StatusCode::NOT_IMPLEMENTED
    );
}

struct Credentials;

impl crate::DesktopCredentialStore for Credentials {
    fn load_api_key(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
    fn save_api_key(&self, _: Option<&str>) -> anyhow::Result<()> {
        panic!("Cron fixture must not write credentials")
    }
}

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_cron_page_creates_toggles_executes_edits_reloads_and_deletes() {
    let directory = tempfile::tempdir().unwrap();
    let workspace = directory.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let core = Core::persistent(model(), &directory.path().join("core.sqlite")).unwrap();
    let server = AppServer::new_desktop_with_stores_and_workspace(
        core,
        &root.join("../console/dist"),
        String::from("cron-browser-fixture-shutdown"),
        Arc::new(Credentials),
        &directory.path().join("data"),
        &workspace,
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(server.clone().run_http(listener));
    let output = tokio::time::timeout(
        Duration::from_secs(100),
        tokio::process::Command::new("node")
            .arg(root.join("scripts/console_browser_smoke.mjs"))
            .args([&origin, "/cron-jobs", "--cron-crud"])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    server.inner.shutdown.cancel();
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
        json!({"created":true,"toggled":true,"manual":true,"history":true,"edited":true,"reload":true,"deleted":true})
    );
    assert_eq!(server.inner.desktop_push_messages.read().await.len(), 1);
    assert!(read_data(&server).unwrap().jobs.is_empty());
}
