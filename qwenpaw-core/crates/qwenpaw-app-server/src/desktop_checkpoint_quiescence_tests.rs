//! Restore must drain the selected Workspace without stopping its neighbours.

use super::super::super::approvals::{agent_chat, start_chat, wait_pending};
use super::*;
use crate::desktop_checkpoints::quiescence as gate;
use pretty_assertions::assert_eq;

async fn target(fixture: &Fixture, actor: &str) -> (String, Value, std::path::PathBuf) {
    let context = desktop_agents::context_for_agent(&fixture.server, actor)
        .await
        .unwrap();
    let root = context.workspace;
    std::fs::write(root.join("notes.txt"), "original notes").unwrap();
    let thread = chat(fixture, actor, "restore-target", root.to_str().unwrap()).await;
    let snapshot = api(
        fixture,
        actor,
        "POST",
        "/snapshot",
        json!({"session_id":"restore-target","user_id":"desktop","name":"Before running"}),
    )
    .await;
    std::fs::write(root.join("notes.txt"), "edited notes").unwrap();
    let request = json!({"commit":snapshot["commit"],"session_id":"restore-target","user_id":"desktop",
        "include_files":true,"files":["notes.txt"]});
    (thread, request, root)
}

async fn consume(body: Body, status: &str) {
    let bytes = tokio::time::timeout(
        Duration::from_secs(5),
        axum::body::to_bytes(body, 1024 * 1024),
    )
    .await
    .unwrap()
    .unwrap();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(text.contains(&format!("\"status\":\"{status}\"")), "{text}");
}

#[tokio::test]
async fn checkpoint_active_snapshot_preserves_completed_conversation_and_restores_it() {
    let fixture = Fixture::new().await;
    let id = agent_chat(&fixture, "default").await;
    let before = fixture
        .server
        .inner
        .core
        .export_thread_checkpoint(&id)
        .await
        .unwrap();
    let body = start_chat(&fixture, "default", &id).await;
    let approvals = wait_pending(&fixture, 1).await;
    let running = fixture.server.inner.core.read_thread(&id).await.unwrap();
    let snapshot = tokio::time::timeout(
        Duration::from_secs(2),
        api(
            &fixture,
            "default",
            "POST",
            "/snapshot",
            json!({"session_id":"same-session","user_id":"admin","name":"While running"}),
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        snapshot
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["commit", "ref"]
    );
    assert_eq!(
        fixture.server.inner.core.read_thread(&id).await.unwrap(),
        running
    );
    assert_eq!(wait_pending(&fixture, 1).await, approvals);
    finish_default(&fixture, &approvals, body).await;
    let finished = fixture
        .server
        .inner
        .core
        .export_thread_checkpoint(&id)
        .await
        .unwrap();
    assert_eq!(finished.turns.len(), 1);
    let result = api(
        &fixture,
        "default",
        "POST",
        "/restore",
        json!({"commit":snapshot["commit"],"session_id":"same-session","user_id":"admin"}),
    )
    .await;
    assert_eq!(result["dry_run"], false);
    let restored = fixture
        .server
        .inner
        .core
        .export_thread_checkpoint(&id)
        .await
        .unwrap();
    assert_eq!(
        (restored.turns, restored.messages, restored.turn_metadata),
        (before.turns, before.messages, before.turn_metadata)
    );
    assert_eq!(
        api(&fixture, "default", "GET", "/graph", Value::Null).await["summary"],
        json!({"total":2,"auto":0,"snapshots":1,"safety":1,"heads":1})
    );
}

async fn stop(fixture: &Fixture, actor: &str, id: &str, body: Body) {
    assert_eq!(
        scoped(
            fixture,
            actor,
            "POST",
            &format!("/api/console/chat/stop?chat_id={id}"),
            Value::Null
        )
        .await,
        (StatusCode::OK, json!({"stopped":true}))
    );
    consume(body, "canceled").await;
}

#[tokio::test]
async fn checkpoint_restore_drains_other_console_threads_and_keeps_default_available() {
    let fixture = Fixture::new().await;
    actor(&fixture, "writer").await;
    let (selected, request, root) = target(&fixture, "writer").await;
    let writer = agent_chat(&fixture, "writer").await;
    let default = agent_chat(&fixture, "default").await;
    let writer_body = start_chat(&fixture, "writer", &writer).await;
    let default_body = start_chat(&fixture, "default", &default).await;
    let approvals = wait_pending(&fixture, 2).await;
    let default_before = fixture
        .server
        .inner
        .core
        .read_thread(&default)
        .await
        .unwrap();
    let restoring = scoped(
        &fixture,
        "writer",
        "POST",
        "/api/workspace/checkpoints/restore",
        request,
    );
    tokio::pin!(restoring);
    assert!(
        tokio::time::timeout(Duration::from_millis(40), &mut restoring)
            .await
            .is_err()
    );
    assert_writer_fenced(&fixture, &selected, &root).await;
    let new_chat = scoped(
        &fixture,
        "writer",
        "POST",
        "/api/chats",
        json!({"name":"After restore","session_id":"after-restore","user_id":"desktop"}),
    );
    tokio::pin!(new_chat);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut new_chat)
            .await
            .is_err()
    );
    let approval = approvals["pending_approvals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["agent_id"] == "writer")
        .unwrap();
    assert_eq!(
        scoped(
            &fixture,
            "writer",
            "POST",
            "/api/approval/approve",
            json!({"request_id":approval["request_id"],"session_id":"writer-root"})
        )
        .await
        .0,
        StatusCode::OK
    );
    consume(writer_body, "completed").await;
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), restoring)
            .await
            .unwrap()
            .0,
        StatusCode::OK
    );
    assert_eq!(new_chat.await.0, StatusCode::OK);
    assert!(!gate::is_paused(
        &fixture.server,
        &fixture.data_key("writer")
    ));
    assert_eq!(
        std::fs::read_to_string(root.join("notes.txt")).unwrap(),
        "original notes"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("cron-output.txt")).unwrap(),
        "created by Cron"
    );
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .read_thread(&default)
            .await
            .unwrap(),
        default_before
    );
    assert_eq!(
        api(&fixture, "writer", "GET", "/graph", Value::Null).await["summary"],
        json!({"total":2,"auto":0,"snapshots":1,"safety":1,"heads":1})
    );
    stop(&fixture, "default", &default, default_body).await;
}

async fn assert_writer_fenced(fixture: &Fixture, selected: &str, root: &std::path::Path) {
    assert!(gate::is_paused(
        &fixture.server,
        &fixture.data_key("writer")
    ));
    assert!(!gate::is_paused(
        &fixture.server,
        &fixture.data_key("default")
    ));
    assert!(
        fixture
            .server
            .inner
            .desktop_agent_lifecycle_lock
            .try_lock()
            .is_ok()
    );
    assert!(
        fixture
            .server
            .inner
            .desktop_checkpoint_lock
            .try_lock()
            .is_ok()
    );
    assert!(fixture.server.inner.desktop_cron_lock.try_lock().is_ok());
    tokio::time::timeout(
        Duration::from_secs(1),
        api(fixture, "default", "GET", "/status", Value::Null),
    )
    .await
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join("notes.txt")).unwrap(),
        "edited notes"
    );
    let sdk = fixture.server.dispatch(
        "turn/start",
        json!({"threadId":selected,"input":[{"type":"text","text":"queued"}]}),
    );
    tokio::pin!(sdk);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut sdk)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn checkpoint_restore_timeout_preserves_files_history_and_reopens_admission() {
    for mode in ["conversation", "memory", "files"] {
        let fixture = Fixture::new().await;
        let (_, mut request, root) = target(&fixture, "default").await;
        request["include_files"] = json!(mode == "files");
        request["include_memory"] = json!(mode == "memory");
        if mode != "files" {
            request.as_object_mut().unwrap().remove("files");
        }
        let active = agent_chat(&fixture, "default").await;
        let body = start_chat(&fixture, "default", &active).await;
        let approvals = wait_pending(&fixture, 1).await;
        let before = fixture
            .server
            .inner
            .core
            .read_thread(&active)
            .await
            .unwrap();
        let state = crate::desktop_checkpoints::state_directory(
            &fixture.directory.path().join("data"),
            &fixture.data_key("default"),
        )
        .join("state.json");
        let state_before = std::fs::read(&state).unwrap();
        let restoring = scoped(
            &fixture,
            "default",
            "POST",
            "/api/workspace/checkpoints/restore",
            request,
        );
        tokio::pin!(restoring);
        assert!(
            tokio::time::timeout(Duration::from_millis(30), &mut restoring)
                .await
                .is_err()
        );
        assert!(gate::is_paused(
            &fixture.server,
            &fixture.data_key("default")
        ));
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(31)).await;
        tokio::time::resume();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), restoring)
                .await
                .unwrap(),
            (
                StatusCode::BAD_REQUEST,
                json!({"detail":"Checkpoint restore was cancelled because workspace tasks did not become idle within 30.0s."})
            )
        );
        assert!(!gate::is_paused(
            &fixture.server,
            &fixture.data_key("default")
        ));
        assert_eq!(std::fs::read(&state).unwrap(), state_before);
        assert_eq!(
            std::fs::read_to_string(root.join("notes.txt")).unwrap(),
            "edited notes"
        );
        assert_eq!(
            fixture
                .server
                .inner
                .core
                .read_thread(&active)
                .await
                .unwrap(),
            before
        );
        assert_eq!(wait_pending(&fixture, 1).await, approvals);
        tokio::time::timeout(
            Duration::from_secs(1),
            api(&fixture, "default", "GET", "/status", Value::Null),
        )
        .await
        .unwrap();
        stop(&fixture, "default", &active, body).await;
    }
}

#[tokio::test]
async fn checkpoint_restore_preview_never_pauses_or_waits_for_an_active_workspace() {
    let fixture = Fixture::new().await;
    let (selected, request, root) = target(&fixture, "default").await;
    let body = start_chat(&fixture, "default", &selected).await;
    wait_pending(&fixture, 1).await;
    let before = api(&fixture, "default", "GET", "/graph", Value::Null).await;
    let result = tokio::time::timeout(
        Duration::from_secs(1),
        scoped(
            &fixture,
            "default",
            "POST",
            "/api/workspace/checkpoints/restore/preview",
            request,
        ),
    )
    .await
    .unwrap();
    assert_eq!(result.0, StatusCode::OK, "{}", result.1);
    assert_eq!(result.1["dry_run"], true);
    assert!(!gate::is_paused(
        &fixture.server,
        &fixture.data_key("default")
    ));
    assert_eq!(
        api(&fixture, "default", "GET", "/graph", Value::Null).await,
        before
    );
    assert_eq!(
        std::fs::read_to_string(root.join("notes.txt")).unwrap(),
        "edited notes"
    );
    stop(&fixture, "default", &selected, body).await;
}

fn release_model(fixture: &Fixture) {
    fixture
        .remote
        .release_hold
        .store(true, std::sync::atomic::Ordering::Release);
    fixture.remote.hold_released.notify_waiters();
}

#[tokio::test]
async fn checkpoint_restore_drains_cron_without_advancing_paused_schedules() {
    checkpoint_cron_restore_for_agent("default").await;
}

#[tokio::test]
async fn checkpoint_restore_preserves_non_default_manual_cron_admission() {
    checkpoint_cron_restore_for_agent("writer").await;
}

async fn restore_cron_jobs(fixture: &Fixture, actor: &str) -> (String, String) {
    let active = fixture.create(json!({}), "hold", false).await;
    let scheduled = fixture
        .request(
            "POST",
            "/api/cron/jobs",
            json!({"name":"After restore","enabled":true,
        "schedule":{"cron":"* * * * *"},"task_type":"text","text":"scheduled fixture",
        "dispatch":{"target":{"user_id":"admin","session_id":"schedule"}}}),
        )
        .await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    if actor != "default" {
        let mut data = read_data(&fixture.server).unwrap();
        fixture.bind_job(&mut data, &active, actor);
        fixture.bind_job(&mut data, &scheduled, actor);
        crate::desktop_cron::write_data(&fixture.server, &data).unwrap();
    }
    (active, scheduled)
}

async fn checkpoint_cron_restore_for_agent(actor: &str) {
    let fixture = Fixture::new().await;
    if actor != "default" {
        fixture
            .request("POST", "/api/agents", json!({"id":actor,"name":actor}))
            .await;
    }
    let (_, request, root) = target(&fixture, actor).await;
    let (active, scheduled) = restore_cron_jobs(&fixture, actor).await;
    assert_eq!(
        scoped(
            &fixture,
            actor,
            "POST",
            &format!("/api/cron/jobs/{active}/run"),
            Value::Null
        )
        .await,
        (StatusCode::OK, json!({"started":true}))
    );
    fixture.wait_requests(1).await;
    let restoring = scoped(
        &fixture,
        actor,
        "POST",
        "/api/workspace/checkpoints/restore",
        request,
    );
    tokio::pin!(restoring);
    assert!(
        tokio::time::timeout(Duration::from_millis(40), &mut restoring)
            .await
            .is_err()
    );
    assert!(gate::is_paused(&fixture.server, &fixture.data_key(actor)));
    let before = fixture.server.inner.core.read_cron_data().unwrap();
    let due = Utc::now() + chrono::Duration::minutes(1);
    crate::desktop_cron::runtime::tick(&fixture.server, due)
        .await
        .unwrap();
    assert_eq!(fixture.server.inner.core.read_cron_data().unwrap(), before);
    assert_eq!(
        tokio::time::timeout(
            Duration::from_secs(1),
            scoped(
                &fixture,
                actor,
                "POST",
                &format!("/api/cron/jobs/{scheduled}/run"),
                json!({})
            )
        )
        .await
        .unwrap(),
        (StatusCode::OK, json!({"started":true}))
    );
    assert_eq!(fixture.server.inner.core.read_cron_data().unwrap(), before);
    release_model(&fixture);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), restoring)
            .await
            .unwrap()
            .0,
        StatusCode::OK
    );
    fixture.idle().await;
    wait_history(&fixture, &scheduled).await;
    let after = read_data(&fixture.server).unwrap();
    assert_eq!(
        after.states[&active].last_status.as_deref(),
        Some("success")
    );
    assert!(!find_job(&after, &active).unwrap().enabled);
    assert!(find_job(&after, &scheduled).unwrap().enabled);
    assert_eq!(
        std::fs::read_to_string(root.join("notes.txt")).unwrap(),
        "original notes"
    );
    crate::desktop_cron::runtime::tick(&fixture.server, due)
        .await
        .unwrap();
    let after = read_data(&fixture.server).unwrap();
    assert_eq!(
        after.history[&scheduled]
            .iter()
            .map(|entry| (entry.trigger.as_str(), entry.status.as_str()))
            .collect::<Vec<_>>(),
        vec![("manual", "success"), ("scheduled", "success")]
    );
    assert!(!find_job(&after, &active).unwrap().enabled);
}

async fn wait_history(fixture: &Fixture, id: &str) {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if read_data(&fixture.server)
                .unwrap()
                .history
                .get(id)
                .is_some_and(|history| !history.is_empty())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn checkpoint_deferred_cron_cannot_run_a_deleted_and_recreated_public_id() {
    let fixture = Fixture::new().await;
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Writer"}),
        )
        .await;
    let (_, request, _) = target(&fixture, "writer").await;
    let (active, scheduled) = restore_cron_jobs(&fixture, "writer").await;
    let original = read_data(&fixture.server).unwrap();
    let spec =
        serde_json::to_value(find_job(&original, &scheduled).unwrap().public_spec()).unwrap();
    assert_eq!(
        scoped(
            &fixture,
            "writer",
            "POST",
            &format!("/api/cron/jobs/{active}/run"),
            Value::Null
        )
        .await
        .0,
        StatusCode::OK
    );
    fixture.wait_requests(1).await;
    let restoring = scoped(
        &fixture,
        "writer",
        "POST",
        "/api/workspace/checkpoints/restore",
        request,
    );
    tokio::pin!(restoring);
    assert!(
        tokio::time::timeout(Duration::from_millis(40), &mut restoring)
            .await
            .is_err()
    );
    assert!(gate::is_paused(
        &fixture.server,
        &fixture.data_key("writer")
    ));
    let path = format!("/api/cron/jobs/{scheduled}");
    assert_eq!(
        scoped(
            &fixture,
            "writer",
            "POST",
            &format!("{path}/run"),
            Value::Null
        )
        .await,
        (StatusCode::OK, json!({"started":true}))
    );
    assert_eq!(
        scoped(&fixture, "writer", "DELETE", &path, Value::Null).await,
        (StatusCode::OK, json!({"deleted":true}))
    );
    assert_eq!(
        scoped(&fixture, "writer", "PUT", &path, spec.clone()).await,
        (StatusCode::OK, spec)
    );
    let before = read_data(&fixture.server).unwrap();
    let replacement = before
        .jobs
        .iter()
        .find(|job| job.public_id() == Some(scheduled.as_str()))
        .unwrap()
        .id
        .clone()
        .unwrap();
    assert_ne!(replacement, scheduled);
    release_model(&fixture);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), restoring)
            .await
            .unwrap()
            .0,
        StatusCode::OK
    );
    fixture.idle().await;
    tokio::time::timeout(
        Duration::from_secs(5),
        crate::desktop_cron::shutdown(&fixture.server),
    )
    .await
    .unwrap();
    assert_deferred_replacement_untouched(&fixture, &before, &replacement, &scheduled).await;
}

async fn assert_deferred_replacement_untouched(
    fixture: &Fixture,
    before: &crate::desktop_cron::CronData,
    replacement: &str,
    scheduled: &str,
) {
    let after = read_data(&fixture.server).unwrap();
    assert_eq!(
        serde_json::to_value(&after.states[replacement]).unwrap(),
        serde_json::to_value(&before.states[replacement]).unwrap()
    );
    assert!(!after.history.contains_key(replacement));
    assert!(!after.history.contains_key(scheduled));
    // The admitted Agent turn finishes its tool round trip; the queued text job never runs.
    assert_eq!(
        fixture
            .remote
            .requests
            .lock()
            .unwrap()
            .iter()
            .map(|request| {
                request["messages"].as_array().unwrap().last().unwrap()["role"].clone()
            })
            .collect::<Vec<_>>(),
        vec![json!("user"), json!("tool")]
    );
    assert!(
        fixture
            .server
            .inner
            .desktop_push_messages
            .read()
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn checkpoint_restore_drains_heartbeat_and_refuses_a_second_start() {
    let fixture = Fixture::new().await;
    let (_, request, root) = target(&fixture, "default").await;
    fixture
        .request(
            "PUT",
            "/api/workspace/running-config",
            json!({"approval_level":"OFF"}),
        )
        .await;
    std::fs::write(root.join("HEARTBEAT.md"), "hold").unwrap();
    assert_eq!(
        fixture
            .request("POST", "/api/config/heartbeat/run", Value::Null)
            .await,
        json!({"started":true})
    );
    fixture.wait_requests(1).await;
    let restoring = scoped(
        &fixture,
        "default",
        "POST",
        "/api/workspace/checkpoints/restore",
        request,
    );
    tokio::pin!(restoring);
    assert!(
        tokio::time::timeout(Duration::from_millis(40), &mut restoring)
            .await
            .is_err()
    );
    assert!(gate::is_paused(
        &fixture.server,
        &fixture.data_key("default")
    ));
    assert_eq!(
        fixture
            .request("POST", "/api/config/heartbeat/run", Value::Null)
            .await,
        json!({"started":false})
    );
    release_model(&fixture);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), restoring)
            .await
            .unwrap()
            .0,
        StatusCode::OK
    );
    assert!(
        !fixture
            .server
            .inner
            .desktop_heartbeat_running
            .load(std::sync::atomic::Ordering::Acquire)
    );
    assert!(!gate::is_paused(
        &fixture.server,
        &fixture.data_key("default")
    ));
    assert_eq!(
        std::fs::read_to_string(root.join("notes.txt")).unwrap(),
        "original notes"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("cron-output.txt")).unwrap(),
        "created by Cron"
    );
}

#[tokio::test]
async fn checkpoint_restore_drains_unregistered_sdk_turns_and_blocks_default_creation() {
    let fixture = Fixture::new().await;
    let (_, request, root) = target(&fixture, "default").await;
    let thread = fixture
        .server
        .inner
        .core
        .start_thread(qwenpaw_protocol::ThreadStartParams {
            model: None,
            workspace_root: Some(root.to_string_lossy().into_owned()),
        })
        .await
        .unwrap()
        .thread;
    let (_, mut events) = fixture
        .server
        .inner
        .core
        .start_turn_with_runtime(
            qwenpaw_protocol::TurnStartParams {
                thread_id: thread.id.clone(),
                input: vec![qwenpaw_protocol::UserInput::Text {
                    text: String::from("hold"),
                }],
            },
            None,
            qwenpaw_core::AgentRuntimeConfig {
                approval_level: qwenpaw_core::ToolApprovalLevel::Off,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    fixture.wait_requests(1).await;
    let restoring = scoped(
        &fixture,
        "default",
        "POST",
        "/api/workspace/checkpoints/restore",
        request,
    );
    tokio::pin!(restoring);
    assert!(
        tokio::time::timeout(Duration::from_millis(40), &mut restoring)
            .await
            .is_err()
    );
    assert!(gate::is_paused(
        &fixture.server,
        &fixture.data_key("default")
    ));
    let creation = fixture
        .server
        .dispatch("thread/start", json!({"workspaceRoot":root}));
    tokio::pin!(creation);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut creation)
            .await
            .is_err()
    );
    release_model(&fixture);
    let turn = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(CoreEvent::TurnCompleted(event)) = events.recv().await {
                break event.turn;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(turn.status, TurnStatus::Completed);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), restoring)
            .await
            .unwrap()
            .0,
        StatusCode::OK
    );
    let created = creation
        .await
        .unwrap_or_else(|error| panic!("{}", error.message));
    assert_eq!(created.result["thread"]["status"], "idle");
    assert_eq!(
        std::fs::read_to_string(root.join("notes.txt")).unwrap(),
        "original notes"
    );
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .read_thread(&thread.id)
            .await
            .unwrap()
            .turns,
        vec![turn]
    );
}

#[tokio::test]
async fn checkpoint_restore_failure_preserves_pending_auto_without_creating_safety() {
    let fixture = Fixture::new().await;
    let (selected, mut request, root) = target(&fixture, "default").await;
    api(
        &fixture,
        "default",
        "PATCH",
        "/auto",
        json!({"enabled":true}),
    )
    .await;
    let context = desktop_agents::context_for_agent(&fixture.server, "default")
        .await
        .unwrap();
    let before = api(&fixture, "default", "GET", "/graph", Value::Null).await;
    let pending = crate::desktop_checkpoints::runtime::enqueue(
        &fixture.server,
        &context,
        &selected,
        String::from("restore-target"),
        None,
        &fixture.server.inner.shutdown,
    )
    .unwrap();
    request["files"] = json!(["not-in-preview.txt"]);
    assert_eq!(
        scoped(
            &fixture,
            "default",
            "POST",
            "/api/workspace/checkpoints/restore",
            request
        )
        .await,
        (
            StatusCode::BAD_REQUEST,
            json!({"detail":"Selected restore files do not match the preview"})
        )
    );
    assert!(!pending.is_cancelled());
    assert!(!gate::is_paused(&fixture.server, &context.data_key));
    assert_eq!(
        api(&fixture, "default", "GET", "/graph", Value::Null).await,
        before
    );
    assert_eq!(
        std::fs::read_to_string(root.join("notes.txt")).unwrap(),
        "edited notes"
    );
    tokio::time::timeout(Duration::from_secs(5), pending.cancelled())
        .await
        .unwrap();
    assert_eq!(
        api(&fixture, "default", "GET", "/graph", Value::Null).await["summary"],
        json!({"total":2,"auto":1,"snapshots":1,"safety":0,"heads":1})
    );
    let body = start_chat(&fixture, "default", &selected).await;
    wait_pending(&fixture, 1).await;
    stop(&fixture, "default", &selected, body).await;
}

#[tokio::test]
async fn checkpoint_restore_retains_due_auto_and_completion_hooks_after_success() {
    retained_auto(false).await;
}

#[tokio::test]
async fn checkpoint_restore_retains_due_auto_after_timeout() {
    retained_auto(true).await;
}

async fn retained_auto(timeout: bool) {
    let fixture = Fixture::new().await;
    let (selected, request, root) = target(&fixture, "default").await;
    api(
        &fixture,
        "default",
        "PATCH",
        "/auto",
        json!({"enabled":true}),
    )
    .await;
    let context = desktop_agents::context_for_agent(&fixture.server, "default")
        .await
        .unwrap();
    let active = agent_chat(&fixture, "default").await;
    let body = start_chat(&fixture, "default", &active).await;
    let approvals = wait_pending(&fixture, 1).await;
    let pending = crate::desktop_checkpoints::runtime::enqueue(
        &fixture.server,
        &context,
        &selected,
        String::from("restore-target"),
        None,
        &fixture.server.inner.shutdown,
    )
    .unwrap();
    let state = crate::desktop_checkpoints::state_directory(
        &fixture.directory.path().join("data"),
        &context.data_key,
    )
    .join("state.json");
    let before = std::fs::read(&state).unwrap();
    let restoring = scoped(
        &fixture,
        "default",
        "POST",
        "/api/workspace/checkpoints/restore",
        request,
    );
    tokio::pin!(restoring);
    assert!(
        tokio::time::timeout(Duration::from_millis(40), &mut restoring)
            .await
            .is_err()
    );
    advance_clock(Duration::from_secs(2)).await;
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert!(gate::is_paused(&fixture.server, &context.data_key));
    assert!(!pending.is_cancelled());
    assert_eq!(
        crate::desktop_checkpoints::runtime::task_counts(&fixture.server),
        (0, 1)
    );
    assert_eq!(std::fs::read(&state).unwrap(), before);
    assert_eq!(
        std::fs::read_to_string(root.join("notes.txt")).unwrap(),
        "edited notes"
    );
    if timeout {
        advance_clock(Duration::from_secs(31)).await;
        assert_eq!(
            restoring.await,
            (
                StatusCode::BAD_REQUEST,
                json!({"detail":"Checkpoint restore was cancelled because workspace tasks did not become idle within 30.0s."})
            )
        );
        stop(&fixture, "default", &active, body).await;
    } else {
        finish_default(&fixture, &approvals, body).await;
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), restoring)
                .await
                .unwrap()
                .0,
            StatusCode::OK
        );
    }
    wait_auto_idle(&fixture).await;
    assert!(pending.is_cancelled());
    assert!(!gate::is_paused(&fixture.server, &context.data_key));
    assert_eq!(
        std::fs::read_to_string(root.join("notes.txt")).unwrap(),
        if timeout {
            "edited notes"
        } else {
            "original notes"
        }
    );
    assert_eq!(
        api(&fixture, "default", "GET", "/graph", Value::Null).await["summary"],
        if timeout {
            json!({"total":2,"auto":1,"snapshots":1,"safety":0,"heads":1})
        } else {
            json!({"total":4,"auto":2,"snapshots":1,"safety":1,"heads":2})
        }
    );
}

async fn finish_default(fixture: &Fixture, approvals: &Value, body: Body) {
    assert_eq!(
        scoped(
            fixture,
            "default",
            "POST",
            "/api/approval/approve",
            json!({"request_id":approvals["pending_approvals"][0]["request_id"],
                "session_id":"default-root"})
        )
        .await
        .0,
        StatusCode::OK
    );
    consume(body, "completed").await;
}

async fn advance_clock(duration: Duration) {
    tokio::time::pause();
    tokio::time::advance(duration).await;
    tokio::time::resume();
}

#[tokio::test]
async fn checkpoint_restore_auto_waiters_can_still_be_drained_for_shutdown_or_backup() {
    for shutdown in [false, true] {
        let fixture = Fixture::new().await;
        let (selected, request, root) = target(&fixture, "default").await;
        api(
            &fixture,
            "default",
            "PATCH",
            "/auto",
            json!({"enabled":true}),
        )
        .await;
        let context = desktop_agents::context_for_agent(&fixture.server, "default")
            .await
            .unwrap();
        let active = agent_chat(&fixture, "default").await;
        let body = start_chat(&fixture, "default", &active).await;
        wait_pending(&fixture, 1).await;
        let pending = crate::desktop_checkpoints::runtime::enqueue(
            &fixture.server,
            &context,
            &selected,
            String::from("restore-target"),
            None,
            &fixture.server.inner.shutdown,
        )
        .unwrap();
        let restoring = scoped(
            &fixture,
            "default",
            "POST",
            "/api/workspace/checkpoints/restore",
            request,
        );
        tokio::pin!(restoring);
        assert!(
            tokio::time::timeout(Duration::from_millis(40), &mut restoring)
                .await
                .is_err()
        );
        advance_clock(Duration::from_secs(2)).await;
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(!pending.is_cancelled());
        assert_eq!(
            crate::desktop_checkpoints::runtime::task_counts(&fixture.server),
            (0, 1)
        );
        tokio::time::timeout(Duration::from_secs(1), async {
            if shutdown {
                crate::desktop_checkpoints::runtime::shutdown(&fixture.server).await;
            } else {
                crate::desktop_checkpoints::runtime::cancel_all_and_drain(&fixture.server).await;
            }
        })
        .await
        .unwrap();
        assert!(pending.is_cancelled());
        assert_eq!(
            crate::desktop_checkpoints::runtime::task_counts(&fixture.server),
            (0, 0)
        );
        assert!(gate::is_paused(&fixture.server, &context.data_key));
        stop(&fixture, "default", &active, body).await;
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), restoring)
                .await
                .unwrap()
                .0,
            StatusCode::OK
        );
        assert_eq!(
            std::fs::read_to_string(root.join("notes.txt")).unwrap(),
            "original notes"
        );
        assert_eq!(
            api(&fixture, "default", "GET", "/graph", Value::Null).await["summary"],
            json!({"total":2,"auto":0,"snapshots":1,"safety":1,"heads":1})
        );
    }
}

async fn wait_auto_idle(fixture: &Fixture) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while crate::desktop_checkpoints::runtime::task_counts(&fixture.server) != (0, 0) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn checkpoint_restore_client_disconnect_keeps_admission_until_transaction_finishes() {
    let fixture = Fixture::new().await;
    let (_, request, root) = target(&fixture, "default").await;
    let active = agent_chat(&fixture, "default").await;
    let body = start_chat(&fixture, "default", &active).await;
    let approvals = wait_pending(&fixture, 1).await;
    let before = fixture
        .server
        .inner
        .core
        .read_thread(&active)
        .await
        .unwrap();
    {
        let restoring = scoped(
            &fixture,
            "default",
            "POST",
            "/api/workspace/checkpoints/restore",
            request,
        );
        tokio::pin!(restoring);
        assert!(
            tokio::time::timeout(Duration::from_millis(40), &mut restoring)
                .await
                .is_err()
        );
    }
    // The HTTP waiter is gone; the admitted transaction still owns its fences.
    assert!(gate::is_paused(
        &fixture.server,
        &fixture.data_key("default")
    ));
    assert!(
        fixture
            .server
            .inner
            .desktop_agent_lifecycle_lock
            .try_lock()
            .is_ok()
    );
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .read_thread(&active)
            .await
            .unwrap(),
        before
    );
    assert_eq!(
        std::fs::read_to_string(root.join("notes.txt")).unwrap(),
        "edited notes"
    );
    assert_eq!(
        scoped(
            &fixture,
            "default",
            "POST",
            "/api/approval/approve",
            json!({"request_id":approvals["pending_approvals"][0]["request_id"],
                "session_id":"default-root"}),
        )
        .await
        .0,
        StatusCode::OK,
    );
    consume(body, "completed").await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while gate::is_paused(&fixture.server, &fixture.data_key("default")) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join("notes.txt")).unwrap(),
        "original notes"
    );
    assert_eq!(
        api(&fixture, "default", "GET", "/graph", Value::Null).await["summary"],
        json!({"total":2,"auto":0,"snapshots":1,"safety":1,"heads":1})
    );
    let next = fixture
        .server
        .dispatch("thread/start", json!({"workspaceRoot":root}))
        .await
        .unwrap_or_else(|error| panic!("{}", error.message));
    assert_eq!(next.result["thread"]["status"], "idle");
}

#[tokio::test]
async fn checkpoint_restore_finishes_before_selected_agent_deletion() {
    let fixture = Fixture::new().await;
    actor(&fixture, "writer").await;
    let (_, request, root) = target(&fixture, "writer").await;
    let writer = agent_chat(&fixture, "writer").await;
    let body = start_chat(&fixture, "writer", &writer).await;
    let approvals = wait_pending(&fixture, 1).await;
    let restoring = scoped(
        &fixture,
        "writer",
        "POST",
        "/api/workspace/checkpoints/restore",
        request,
    );
    tokio::pin!(restoring);
    assert!(
        tokio::time::timeout(Duration::from_millis(40), &mut restoring)
            .await
            .is_err()
    );
    let deletion = fixture.request("DELETE", "/api/agents/writer", Value::Null);
    tokio::pin!(deletion);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut deletion)
            .await
            .is_err()
    );
    tokio::time::timeout(
        Duration::from_secs(1),
        api(&fixture, "default", "GET", "/status", Value::Null),
    )
    .await
    .unwrap();
    assert!(
        desktop_agents::registered_data_key(&fixture.server, "writer")
            .await
            .unwrap()
            .is_some()
    );
    let approval = &approvals["pending_approvals"][0];
    assert_eq!(
        scoped(
            &fixture,
            "writer",
            "POST",
            "/api/approval/approve",
            json!({"request_id":approval["request_id"],"session_id":"writer-root"})
        )
        .await
        .0,
        StatusCode::OK
    );
    consume(body, "completed").await;
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), restoring)
            .await
            .unwrap()
            .0,
        StatusCode::OK
    );
    assert_eq!(deletion.await, json!({"success":true,"agent_id":"writer"}));
    assert_eq!(
        std::fs::read_to_string(root.join("notes.txt")).unwrap(),
        "original notes"
    );
    assert_eq!(
        desktop_agents::registered_data_key(&fixture.server, "writer")
            .await
            .unwrap(),
        None
    );
}
