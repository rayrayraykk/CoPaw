use super::*;
use crate::desktop_checkpoints::runtime as checkpoint_runtime;
use pretty_assertions::assert_eq;

#[path = "desktop_checkpoint_hook_tests.rs"]
mod hooks;

async fn enabled(fixture: &Fixture, actor: &str) -> desktop_agents::AgentContext {
    api(fixture, actor, "PATCH", "/auto", json!({"enabled":true})).await;
    desktop_agents::context_for_agent(&fixture.server, actor)
        .await
        .unwrap()
}

fn enqueue(
    fixture: &Fixture,
    agent: &desktop_agents::AgentContext,
    thread: &str,
) -> CancellationToken {
    checkpoint_runtime::enqueue(
        &fixture.server,
        agent,
        thread,
        thread.to_owned(),
        None,
        &fixture.server.inner.shutdown,
    )
    .unwrap()
}

async fn finished(completions: Vec<CancellationToken>) {
    tokio::time::timeout(
        Duration::from_secs(5),
        checkpoint_runtime::drain(completions),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn checkpoint_runtime_debounces_one_session_without_merging_other_workspaces() {
    let fixture = Fixture::new().await;
    actor(&fixture, "writer").await;
    let first = chat(&fixture, "writer", "first", "shared-project").await;
    let second = chat(&fixture, "writer", "second", "shared-project").await;
    let default = chat(&fixture, "default", "first", "shared-project").await;
    let writer = enabled(&fixture, "writer").await;
    let initial = enabled(&fixture, "default").await;
    tokio::time::pause();
    let old = enqueue(&fixture, &writer, &first);
    tokio::time::advance(Duration::from_secs(1)).await;
    assert!(!old.is_cancelled());
    let latest = enqueue(&fixture, &writer, &first);
    let independent = enqueue(&fixture, &writer, &second);
    let other = enqueue(&fixture, &initial, &default);
    tokio::time::advance(Duration::from_secs(1)).await;
    assert!(old.is_cancelled());
    assert!(!latest.is_cancelled());
    assert!(!independent.is_cancelled());
    assert!(!other.is_cancelled());
    tokio::time::advance(Duration::from_millis(500)).await;
    tokio::time::resume();
    finished(vec![latest, independent, other]).await;
    assert_eq!(
        api(&fixture, "writer", "GET", "/graph", Value::Null).await["summary"],
        json!({"total":2,"auto":2,"snapshots":0,"safety":0,"heads":2})
    );
    assert_eq!(
        api(&fixture, "default", "GET", "/graph", Value::Null).await["summary"],
        json!({"total":1,"auto":1,"snapshots":0,"safety":0,"heads":1})
    );
}

#[tokio::test]
async fn checkpoint_runtime_shutdown_cancels_pending_and_rejects_new_work() {
    let fixture = Fixture::new().await;
    let thread = chat(&fixture, "default", "session", "external-project").await;
    let agent = enabled(&fixture, "default").await;
    let before = api(&fixture, "default", "GET", "/graph", Value::Null).await;
    tokio::time::pause();
    let pending = enqueue(&fixture, &agent, &thread);
    checkpoint_runtime::shutdown(&fixture.server).await;
    assert!(pending.is_cancelled());
    assert_eq!(checkpoint_runtime::task_counts(&fixture.server), (0, 0));
    assert!(
        checkpoint_runtime::enqueue(
            &fixture.server,
            &agent,
            &thread,
            thread.clone(),
            None,
            &fixture.server.inner.shutdown
        )
        .is_none()
    );
    tokio::time::resume();
    assert_eq!(
        api(&fixture, "default", "GET", "/graph", Value::Null).await,
        before
    );
}

#[tokio::test]
async fn checkpoint_runtime_core_restore_barrier_prevents_background_writes() {
    let fixture = Fixture::new().await;
    let thread = chat(&fixture, "default", "session", "external-project").await;
    let agent = enabled(&fixture, "default").await;
    let before = api(&fixture, "default", "GET", "/graph", Value::Null).await;
    let restore = fixture
        .server
        .inner
        .core
        .begin_restore(Duration::from_secs(2))
        .await
        .unwrap();
    tokio::time::pause();
    let pending = enqueue(&fixture, &agent, &thread);
    tokio::time::advance(Duration::from_secs(2)).await;
    tokio::time::resume();
    finished(vec![pending]).await;
    assert_eq!(checkpoint_runtime::task_counts(&fixture.server), (0, 0));
    drop(restore);
    assert_eq!(
        api(&fixture, "default", "GET", "/graph", Value::Null).await,
        before
    );
}

#[tokio::test]
async fn checkpoint_runtime_invalid_checkpoint_state_does_not_delete_the_chat() {
    let fixture = Fixture::new().await;
    let thread = chat(&fixture, "default", "session", "external-project").await;
    let agent = enabled(&fixture, "default").await;
    let before = fixture
        .server
        .inner
        .core
        .read_thread(&thread)
        .await
        .unwrap();
    let state_path = crate::desktop_checkpoints::state_directory(
        &fixture.directory.path().join("data"),
        &agent.data_key,
    )
    .join("state.json");
    std::fs::write(&state_path, "broken checkpoint fixture").unwrap();
    let result = scoped(
        &fixture,
        "default",
        "DELETE",
        &format!("/api/chats/{thread}"),
        Value::Null,
    )
    .await;
    assert_eq!(
        result,
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"detail":"Checkpoint state is invalid"})
        )
    );
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .read_thread(&thread)
            .await
            .unwrap(),
        before
    );
    assert_eq!(
        std::fs::read_to_string(state_path).unwrap(),
        "broken checkpoint fixture"
    );
}

#[tokio::test]
async fn checkpoint_runtime_manual_gc_never_removes_manual_snapshot_archives() {
    let fixture = Fixture::new().await;
    chat(&fixture, "default", "session", "external-project").await;
    let agent = enabled(&fixture, "default").await;
    for name in ["first", "second"] {
        api(
            &fixture,
            "default",
            "POST",
            "/snapshot",
            json!({"session_id":"session","user_id":"desktop","name":name}),
        )
        .await;
    }
    let before = api(&fixture, "default", "GET", "/graph", Value::Null).await;
    let directory = crate::desktop_checkpoints::state_directory(
        &fixture.directory.path().join("data"),
        &agent.data_key,
    );
    let files = std::fs::read_dir(directory.join("snapshots"))
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            let bytes = std::fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect::<Vec<_>>();
    for compact in [false, true] {
        for (suffix, dry_run) in [("/gc/preview", true), ("/gc", false)] {
            assert_eq!(
                api(
                    &fixture,
                    "default",
                    "POST",
                    suffix,
                    json!({"compact":compact,"keep_count":0,"keep_days":0,"pre_restore_days":0})
                )
                .await,
                json!({"deleted_refs":[],"kept_refs":[],"dry_run":dry_run})
            );
            assert_eq!(
                api(&fixture, "default", "GET", "/graph", Value::Null).await,
                before
            );
            for (path, bytes) in &files {
                assert_eq!(&std::fs::read(path).unwrap(), bytes);
            }
        }
    }
}

#[tokio::test]
async fn checkpoint_runtime_compaction_ignores_auto_retention_but_not_manual_snapshots() {
    let fixture = Fixture::new().await;
    let thread = chat(&fixture, "default", "session", "external-project").await;
    let agent = enabled(&fixture, "default").await;
    automatic(&fixture, &agent, &thread).await;
    automatic(&fixture, &agent, &thread).await;
    let before = api(&fixture, "default", "GET", "/graph", Value::Null).await;
    let snapshot = api(
        &fixture,
        "default",
        "POST",
        "/snapshot",
        json!({"session_id":"session","user_id":"desktop","name":"Keep manual HEAD"}),
    )
    .await;
    let graph = api(&fixture, "default", "GET", "/graph", Value::Null).await;
    let manual = graph["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["kind"] == "snap")
        .unwrap()
        .clone();
    assert_eq!(
        api(
            &fixture,
            "default",
            "POST",
            "/gc",
            json!({"keep_count":1,"keep_days":0})
        )
        .await,
        json!({"deleted_refs":[before["nodes"][1]["ref"]],"kept_refs":[before["nodes"][0]["ref"]],"dry_run":false})
    );
    assert_eq!(
        api(&fixture, "default", "POST", "/gc", json!({"compact":true})).await,
        json!({"deleted_refs":[before["nodes"][0]["ref"]],"kept_refs":[],"dry_run":false})
    );
    let graph = api(&fixture, "default", "GET", "/graph", Value::Null).await;
    assert_eq!(graph["nodes"], json!([manual]));
    assert_eq!(
        graph["summary"],
        json!({"total":1,"auto":0,"snapshots":1,"safety":0,"heads":1})
    );
    // Compaction must not classify a referenced manual ZIP as an orphan.
    api(
        &fixture,
        "default",
        "POST",
        "/restore/preview",
        json!({"commit":snapshot["commit"],"session_id":"session","user_id":"desktop"}),
    )
    .await;
}

#[tokio::test]
async fn checkpoint_runtime_active_waiter_is_cancelled_and_drained_without_a_lifecycle_lock() {
    let fixture = Fixture::new().await;
    actor(&fixture, "writer").await;
    let thread = chat(&fixture, "writer", "session", "external-project").await;
    let agent = enabled(&fixture, "writer").await;
    let before = api(&fixture, "writer", "GET", "/graph", Value::Null).await;
    let lifecycle = fixture
        .server
        .inner
        .desktop_agent_lifecycle_lock
        .lock()
        .await;
    let guard = fixture.server.inner.desktop_checkpoint_lock.lock().await;
    tokio::time::pause();
    let active = enqueue(&fixture, &agent, &thread);
    tokio::time::advance(Duration::from_secs(2)).await;
    tokio::task::yield_now().await;
    tokio::time::resume();
    assert_eq!(checkpoint_runtime::task_counts(&fixture.server), (0, 1));
    let tokens = checkpoint_runtime::cancel_agent(&fixture.server, "writer");
    assert_eq!(tokens.len(), 1);
    assert!(!active.is_cancelled());
    drop(guard);
    finished(tokens).await;
    assert!(active.is_cancelled());
    drop(lifecycle);
    assert_eq!(
        api(&fixture, "writer", "GET", "/graph", Value::Null).await,
        before
    );
}

async fn automatic(fixture: &Fixture, agent: &desktop_agents::AgentContext, thread: &str) {
    crate::desktop_checkpoints::maybe_create_auto_checkpoint(
        &fixture.server,
        thread,
        agent,
        None,
        &fixture.server.inner.shutdown,
    )
    .await;
}

#[tokio::test]
async fn checkpoint_runtime_gc_runs_after_fifteen_minutes_and_keeps_manual_and_other_sessions() {
    let fixture = Fixture::new().await;
    actor(&fixture, "writer").await;
    let first = chat(&fixture, "writer", "first", "shared-project").await;
    let second = chat(&fixture, "writer", "second", "shared-project").await;
    let clock_start = tokio::time::Instant::now();
    let agent = enabled(&fixture, "writer").await;
    let clock_ready = tokio::time::Instant::now();
    let default = api(&fixture, "default", "GET", "/graph", Value::Null).await;
    api(
        &fixture,
        "writer",
        "PATCH",
        "/gc/settings",
        json!({"gc_keep_count":0,"gc_keep_days":0,"pre_restore_retention_days":0}),
    )
    .await;
    let manual = api(
        &fixture,
        "writer",
        "POST",
        "/snapshot",
        json!({"session_id":"first","user_id":"desktop","name":"Keep manual"}),
    )
    .await;
    automatic(&fixture, &agent, &first).await;
    automatic(&fixture, &agent, &first).await;
    automatic(&fixture, &agent, &second).await;
    let before = api(&fixture, "writer", "GET", "/graph", Value::Null).await;
    let other = before["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["session_id"] == "second")
        .unwrap()
        .clone();
    tokio::time::pause();
    tokio::time::advance(
        (clock_start + Duration::from_secs(899))
            .saturating_duration_since(tokio::time::Instant::now()),
    )
    .await;
    tokio::time::resume();
    automatic(&fixture, &agent, &first).await;
    assert_eq!(
        api(&fixture, "writer", "GET", "/graph", Value::Null).await["summary"]["total"],
        5
    );
    tokio::time::pause();
    tokio::time::advance(
        (clock_ready + Duration::from_secs(901))
            .saturating_duration_since(tokio::time::Instant::now()),
    )
    .await;
    tokio::time::resume();
    automatic(&fixture, &agent, &first).await;
    let graph = api(&fixture, "writer", "GET", "/graph", Value::Null).await;
    assert_eq!(
        graph["summary"],
        json!({"total":3,"auto":2,"snapshots":1,"safety":0,"heads":2})
    );
    assert!(graph["nodes"].as_array().unwrap().contains(&other));
    assert!(
        graph["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["commit"] == manual["commit"])
    );
    assert_eq!(
        api(&fixture, "default", "GET", "/graph", Value::Null).await,
        default
    );
}

#[tokio::test]
async fn checkpoint_runtime_agent_disable_cancels_pending_before_reenable() {
    let fixture = Fixture::new().await;
    actor(&fixture, "writer").await;
    let thread = chat(&fixture, "writer", "session", "external-project").await;
    let agent = enabled(&fixture, "writer").await;
    let before = api(&fixture, "writer", "GET", "/graph", Value::Null).await;
    let pending = enqueue(&fixture, &agent, &thread);
    fixture
        .request(
            "PATCH",
            "/api/agents/writer/toggle",
            json!({"enabled":false}),
        )
        .await;
    assert!(pending.is_cancelled());
    fixture
        .request(
            "PATCH",
            "/api/agents/writer/toggle",
            json!({"enabled":true}),
        )
        .await;
    assert_eq!(
        api(&fixture, "writer", "GET", "/graph", Value::Null).await,
        before
    );
}

#[tokio::test]
async fn checkpoint_runtime_reset_and_auto_disable_invalidate_pending_before_reenable() {
    for reset in [false, true] {
        let fixture = Fixture::new().await;
        let thread = chat(&fixture, "default", "session", "external-project").await;
        let agent = enabled(&fixture, "default").await;
        let before = api(&fixture, "default", "GET", "/graph", Value::Null).await;
        let pending = enqueue(&fixture, &agent, &thread);
        if reset {
            api(&fixture, "default", "DELETE", "", Value::Null).await;
        } else {
            api(
                &fixture,
                "default",
                "PATCH",
                "/auto",
                json!({"enabled":false}),
            )
            .await;
        }
        enabled(&fixture, "default").await;
        finished(vec![pending]).await;
        assert_eq!(
            api(&fixture, "default", "GET", "/graph", Value::Null).await,
            before
        );
    }
}

#[tokio::test]
async fn checkpoint_runtime_chat_deletion_removes_only_its_history_and_head() {
    let mut fixture = Fixture::new().await;
    actor(&fixture, "writer").await;
    let mut threads = Vec::new();
    for (agent, session) in [
        ("default", "default"),
        ("writer", "first"),
        ("writer", "second"),
    ] {
        threads.push(chat(&fixture, agent, session, "shared-project").await);
        api(
            &fixture,
            agent,
            "POST",
            "/snapshot",
            json!({"session_id":session,"user_id":"desktop","name":session}),
        )
        .await;
    }
    let default = api(&fixture, "default", "GET", "/graph", Value::Null).await;
    let agent = enabled(&fixture, "writer").await;
    let pending = enqueue(&fixture, &agent, &threads[1]);
    let mut expected = api(&fixture, "writer", "GET", "/graph", Value::Null).await;
    expected["nodes"]
        .as_array_mut()
        .unwrap()
        .retain(|node| node["session_id"] == "second");
    expected["sessions"]
        .as_array_mut()
        .unwrap()
        .retain(|session| session["session_id"] == "second");
    expected["summary"] = json!({"total":1,"auto":0,"snapshots":1,"safety":0,"heads":1});
    assert_eq!(
        scoped(
            &fixture,
            "writer",
            "DELETE",
            &format!("/api/chats/{}", threads[1]),
            Value::Null
        )
        .await,
        (StatusCode::OK, json!({"deleted":true}))
    );
    assert_eq!(
        api(&fixture, "writer", "GET", "/graph", Value::Null).await,
        expected
    );
    finished(vec![pending]).await;
    assert_eq!(
        scoped(
            &fixture,
            "writer",
            "POST",
            "/api/chats/batch-delete",
            json!([threads[0], threads[2]])
        )
        .await,
        (StatusCode::OK, json!({"deleted":true}))
    );
    fixture.reopen().await;
    expected["nodes"] = json!([]);
    expected["sessions"] = json!([]);
    expected["summary"] = json!({"total":0,"auto":0,"snapshots":0,"safety":0,"heads":0});
    assert_eq!(
        api(&fixture, "writer", "GET", "/graph", Value::Null).await,
        expected
    );
    assert_eq!(
        api(&fixture, "default", "GET", "/graph", Value::Null).await,
        default
    );
    let directory = crate::desktop_checkpoints::state_directory(
        &fixture.directory.path().join("data"),
        &agent.data_key,
    );
    assert_eq!(
        std::fs::read_dir(directory.join("snapshots"))
            .unwrap()
            .count(),
        0
    );
}
