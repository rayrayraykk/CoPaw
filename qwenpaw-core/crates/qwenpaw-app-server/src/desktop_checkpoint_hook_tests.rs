//! Exercise completion hooks through actual producer entry points.

use super::*;
use pretty_assertions::assert_eq;

#[path = "desktop_protocol_hook_tests.rs"]
mod protocol;

#[path = "desktop_final_persistence_browser_tests.rs"]
mod browser;

async fn settle(fixture: &Fixture) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while checkpoint_runtime::task_counts(&fixture.server) != (0, 0) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

async fn summary(fixture: &Fixture, agent: &str) -> Value {
    api(fixture, agent, "GET", "/graph", Value::Null).await["summary"].clone()
}

fn one_auto() -> Value {
    json!({"total":1,"auto":1,"snapshots":0,"safety":0,"heads":1})
}

fn no_auto() -> Value {
    json!({"total":0,"auto":0,"snapshots":0,"safety":0,"heads":0})
}

#[tokio::test]
async fn checkpoint_hook_failed_final_save_skips_all_three_producers() {
    for producer in ["console", "cron", "heartbeat"] {
        let fixture = Fixture::new().await;
        let agent = enabled(&fixture, "default").await;
        fixture
            .request(
                "PUT",
                "/api/workspace/running-config",
                json!({"approval_level":"OFF"}),
            )
            .await;
        let database =
            rusqlite::Connection::open(fixture.directory.path().join("core.sqlite")).unwrap();
        database
            .execute_batch(
                "CREATE TRIGGER reject_completion BEFORE INSERT ON threads
             WHEN json_extract(NEW.snapshot, '$.turns[#-1].status') = 'completed'
             BEGIN SELECT RAISE(FAIL, 'fixture completion write failure'); END;",
            )
            .unwrap();
        match producer {
            "console" => {
                let thread = chat(&fixture, "default", "failure", "console-project").await;
                let (status, stream) = scoped(&fixture, "default", "POST", "/api/console/chat",
                    json!({"session_id":thread,"input":[{"role":"user","content":"write fixture"}],"stream":true})).await;
                assert_eq!(status, StatusCode::OK);
                assert_failed_save_stream(&fixture, &thread, &stream).await;
                crate::desktop_console_runs::drain_runs(
                    crate::desktop_console_runs::workspace_completions(
                        &fixture.server,
                        &agent.data_key,
                    ),
                )
                .await
                .unwrap();
            }
            "cron" => {
                let job = fixture.create(json!({}), "write fixture", false).await;
                fixture.run(&job).await;
                fixture.idle().await;
                assert_eq!(
                    read_data(&fixture.server).unwrap().states[&job]
                        .last_status
                        .as_deref(),
                    Some("error")
                );
            }
            "heartbeat" => heartbeat(&fixture, "write fixture").await,
            _ => unreachable!(),
        }
        settle(&fixture).await;
        assert_eq!(summary(&fixture, "default").await, no_auto(), "{producer}");
        assert_eq!(fixture.remote.requests.lock().unwrap().len(), 2);
    }
}

async fn assert_failed_save_stream(fixture: &Fixture, thread: &str, stream: &Value) {
    let events = stream
        .as_str()
        .unwrap()
        .lines()
        .filter_map(|line| line.strip_prefix("data:").map(str::trim))
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let history = fixture.server.inner.core.read_thread(thread).await.unwrap();
    assert_eq!(history.turns.len(), 1);
    let turn = &history.turns[0];
    assert_eq!(
        events.last().unwrap(),
        &json!({
            "object":"response", "id":turn.id, "status":"failed", "output":[],
            "error":{"message":"Failed to persist the final turn; the latest state may not survive restart."}
        })
    );
    assert!(
        events
            .iter()
            .any(|event| event["text"] == "Cron fixture finished")
    );
    assert!(turn.items.iter().any(|item| matches!(item,
        qwenpaw_protocol::Item::AgentMessage { text, .. } if text == "Cron fixture finished"
    )));
    let reloaded = fixture
        .request("GET", &format!("/api/chats/{thread}"), Value::Null)
        .await;
    let messages = reloaded["messages"].as_array().unwrap();
    assert_eq!(messages.len(), turn.items.len() + 1);
    assert_eq!(
        messages.last().unwrap(),
        &json!({
            "id":format!("{}_error", turn.id),"role":"assistant","type":"error",
            "status":"failed","content":[],"message":turn.error.as_ref().unwrap().message,
            "metadata":messages[0]["metadata"]
        })
    );
}

#[test]
fn checkpoint_hook_query_uses_original_text_blocks_without_expanding_attachments() {
    let cases = [
        json!([]),
        json!([{"role":"user","content":"  original query \n"}]),
        json!([{"role":"user","content":"old"}, {"role":"user","content":[
            {"type":"image","path":"image.png"}, {"type":"text","text":"  /help"},
            {"type":"text","text":""}, {"type":"file","path":"notes.md"},
            {"type":"text","text":" trailing "}]}]),
        json!([{"role":"user","content":[{"type":"image","path":"image.png"}]}]),
    ];
    assert_eq!(
        cases
            .iter()
            .map(|input| crate::desktop_checkpoints::console_query(input.as_array().unwrap()))
            .collect::<Vec<_>>(),
        vec![
            None,
            Some(String::from("  original query \n")),
            Some(String::from("  /help\n trailing ")),
            None
        ]
    );
}

#[tokio::test]
async fn checkpoint_hook_failed_save_preserves_an_earlier_pending_snapshot() {
    let fixture = Fixture::new().await;
    enabled(&fixture, "default").await;
    let job = fixture.create(json!({}), "saved query", false).await;
    fixture.run(&job).await;
    fixture.idle().await;
    assert_eq!(checkpoint_runtime::task_counts(&fixture.server), (1, 0));
    let saved = fixture.server.inner.core.statistics_snapshots().await;
    assert_eq!(saved.len(), 1);
    let thread = &saved[0].thread.id;
    let checkpoint = fixture.server.inner.desktop_checkpoint_lock.lock().await;
    let database =
        rusqlite::Connection::open(fixture.directory.path().join("core.sqlite")).unwrap();
    database
        .execute_batch(
            "CREATE TRIGGER reject_completion BEFORE INSERT ON threads
         WHEN json_extract(NEW.snapshot, '$.turns[#-1].status') = 'completed'
         BEGIN SELECT RAISE(FAIL, 'fixture completion write failure'); END;",
        )
        .unwrap();
    let mut spec = fixture
        .request("GET", &format!("/api/cron/jobs/{job}"), Value::Null)
        .await["spec"]
        .clone();
    spec["request"]["input"][0]["content"] = json!("unsaved query");
    fixture
        .request("PUT", &format!("/api/cron/jobs/{job}"), spec)
        .await;
    fixture.run(&job).await;
    fixture.idle().await;
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .read_thread(thread)
            .await
            .unwrap()
            .turns
            .len(),
        2
    );
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .export_thread_checkpoint(thread)
            .await
            .unwrap(),
        saved[0]
    );
    assert_eq!(checkpoint_runtime::task_counts(&fixture.server), (1, 0));
    drop(checkpoint);
    settle(&fixture).await;
    let graph = api(&fixture, "default", "GET", "/graph", Value::Null).await;
    assert_eq!(graph["summary"], one_auto());
    assert_eq!(graph["nodes"][0]["query"], json!("saved query"));
}

#[test]
fn checkpoint_hook_eligibility_matches_completed_and_slash_boundaries() {
    let results = [
        "hello",
        "",
        "/help",
        " \n\t/help",
        "\u{1c}/help",
        "\u{a0}/help",
        "say /help",
    ]
    .into_iter()
    .map(|text| {
        let mut turn = Turn {
            id: String::from("turn"),
            thread_id: String::from("thread"),
            status: TurnStatus::Completed,
            error: None,
            items: vec![qwenpaw_protocol::Item::UserMessage {
                id: String::from("item"),
                text: text.to_owned(),
                input: None,
            }],
        };
        [
            TurnStatus::Completed,
            TurnStatus::Interrupted,
            TurnStatus::Failed,
            TurnStatus::InProgress,
        ]
        .map(|status| {
            turn.status = status;
            crate::desktop_checkpoints::auto_snapshot_eligible(&turn, Some(text))
        })
    })
    .collect::<Vec<_>>();
    assert_eq!(
        results,
        vec![
            [true, false, false, false],
            [true, false, false, false],
            [false; 4],
            [false; 4],
            [false; 4],
            [false; 4],
            [true, false, false, false],
        ]
    );
}

#[tokio::test]
async fn checkpoint_hook_scoped_cron_uses_base_workspace_not_shared_project() {
    let fixture = Fixture::new().await;
    actor(&fixture, "writer").await;
    let root = fixture.directory.path().join("shared-project");
    std::fs::create_dir(&root).unwrap();
    for id in ["default", "writer"] {
        fixture
            .request(
                "PUT",
                &format!("/api/agents/{id}"),
                json!({"project_dir":root}),
            )
            .await;
        enabled(&fixture, id).await;
    }
    let original = api(&fixture, "default", "GET", "/graph", Value::Null).await;
    let job_id = fixture.create(json!({}), "write fixture", false).await;
    {
        let _guard = fixture.server.inner.desktop_cron_lock.lock().await;
        let mut data = read_data(&fixture.server).unwrap();
        fixture.bind_job(&mut data, &job_id, "writer");
        write_data(&fixture.server, &data).unwrap();
        let job = find_job(&data, &job_id).unwrap().clone();
        assert!(
            crate::desktop_cron::agent::enqueue(
                &fixture.server,
                &mut data,
                job,
                "manual",
                fixture.server.inner.core.operation_guard().unwrap()
            )
            .await
            .unwrap()
        );
    }
    fixture.idle().await;
    assert_eq!(
        read_data(&fixture.server).unwrap().states[&job_id]
            .last_status
            .as_deref(),
        Some("success")
    );
    settle(&fixture).await;
    assert_eq!(summary(&fixture, "writer").await, one_auto());
    assert_eq!(
        api(&fixture, "default", "GET", "/graph", Value::Null).await,
        original
    );
    assert_eq!(
        std::fs::read_to_string(root.join("cron-output.txt")).unwrap(),
        "created by Cron"
    );
}

async fn heartbeat(fixture: &Fixture, query: &str) {
    std::fs::write(
        fixture.directory.path().join("workspace/HEARTBEAT.md"),
        query,
    )
    .unwrap();
    assert_eq!(
        fixture
            .request("POST", "/api/config/heartbeat/run", Value::Null)
            .await,
        json!({"started":true})
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while fixture
            .server
            .inner
            .desktop_heartbeat_running
            .load(std::sync::atomic::Ordering::Acquire)
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn checkpoint_hook_cron_completes_and_snapshots_without_inbox_delivery() {
    let fixture = Fixture::new().await;
    enabled(&fixture, "default").await;
    let job = fixture.create(json!({}), "write fixture", false).await;
    fixture.run(&job).await;
    fixture.idle().await;
    assert_eq!(
        read_data(&fixture.server).unwrap().states[&job]
            .last_status
            .as_deref(),
        Some("success")
    );
    settle(&fixture).await;
    assert_eq!(summary(&fixture, "default").await, one_auto());
}

#[tokio::test]
async fn checkpoint_hook_heartbeat_completes_and_snapshots_without_inbox_delivery() {
    let fixture = Fixture::new().await;
    enabled(&fixture, "default").await;
    fixture
        .request(
            "PUT",
            "/api/workspace/running-config",
            json!({"approval_level":"OFF"}),
        )
        .await;
    heartbeat(&fixture, "write fixture").await;
    assert_eq!(fixture.remote.requests.lock().unwrap().len(), 2);
    settle(&fixture).await;
    assert_eq!(summary(&fixture, "default").await, one_auto());
}

#[tokio::test]
async fn checkpoint_hook_slash_input_is_skipped_by_all_three_producers() {
    let fixture = Fixture::new().await;
    enabled(&fixture, "default").await;
    fixture
        .request(
            "PUT",
            "/api/workspace/running-config",
            json!({"approval_level":"OFF"}),
        )
        .await;
    let job = fixture.create(json!({}), "  /help", false).await;
    fixture.run(&job).await;
    fixture.idle().await;
    heartbeat(&fixture, "\n\t/help").await;
    let thread = chat(&fixture, "default", "slash", "console-project").await;
    let (status, stream) = scoped(&fixture, "default", "POST", "/api/console/chat",
        json!({"session_id":thread, "input":[{"role":"user","content":"\u{1c}/help"}],"stream":true})).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        stream
            .as_str()
            .unwrap()
            .contains("\"status\":\"completed\"")
    );
    settle(&fixture).await;
    assert_eq!(checkpoint_runtime::task_counts(&fixture.server), (0, 0));
    assert_eq!(summary(&fixture, "default").await, no_auto());
}

#[tokio::test]
async fn checkpoint_hook_timeout_and_cancel_do_not_create_snapshots() {
    for cancel in [false, true] {
        let fixture = Fixture::new().await;
        enabled(&fixture, "default").await;
        let job = fixture
            .create(json!({"timeout_seconds":1}), "hold", false)
            .await;
        fixture.run(&job).await;
        fixture.wait_requests(1).await;
        if cancel {
            crate::desktop_cron::agent::cancel_job(&fixture.server, &job);
        }
        fixture.idle().await;
        assert_eq!(
            read_data(&fixture.server).unwrap().states[&job]
                .last_status
                .as_deref(),
            Some(if cancel { "cancelled" } else { "error" })
        );
        settle(&fixture).await;
        assert_eq!(summary(&fixture, "default").await, no_auto());
    }
    let fixture = Fixture::new().await;
    enabled(&fixture, "default").await;
    fixture
        .request("PUT", "/api/config/heartbeat", json!({"timeoutSeconds":1}))
        .await;
    heartbeat(&fixture, "hold").await;
    settle(&fixture).await;
    assert_eq!(summary(&fixture, "default").await, no_auto());
}

#[tokio::test]
async fn checkpoint_hook_pending_keeps_full_trigger_query_after_a_slash_turn() {
    let fixture = Fixture::new().await;
    enabled(&fixture, "default").await;
    let query = format!("write fixture {}", "original query ".repeat(20));
    let job = fixture.create(json!({}), &query, false).await;
    fixture.run(&job).await;
    fixture.idle().await;
    assert_eq!(checkpoint_runtime::task_counts(&fixture.server), (1, 0));
    let checkpoint = fixture.server.inner.desktop_checkpoint_lock.lock().await;
    let mut spec = fixture
        .request("GET", &format!("/api/cron/jobs/{job}"), Value::Null)
        .await["spec"]
        .clone();
    spec["request"]["input"][0]["content"] = json!("/help");
    fixture
        .request("PUT", &format!("/api/cron/jobs/{job}"), spec)
        .await;
    fixture.run(&job).await;
    fixture.idle().await;
    drop(checkpoint);
    settle(&fixture).await;
    let graph = api(&fixture, "default", "GET", "/graph", Value::Null).await;
    assert_eq!(graph["summary"], one_auto());
    assert_eq!(graph["nodes"][0]["query"], json!(query));
}
