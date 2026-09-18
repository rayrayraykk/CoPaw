//! App Protocol completion belongs to the host, not the output transport.

use super::*;
use futures_util::{SinkExt as _, StreamExt as _};
use pretty_assertions::assert_eq;
use tokio_tungstenite::tungstenite::Message;

#[path = "desktop_protocol_config_tests.rs"]
mod config;

#[path = "workspace_host_tests.rs"]
mod workspace_host;

fn request(thread: &str, query: &str) -> String {
    json!({"id":1,"method":"turn/start","params":{
        "threadId":thread,"input":[{"type":"text","text":query}]
    }})
    .to_string()
}

async fn prepare(fixture: &Fixture) -> String {
    enabled(fixture, "default").await;
    fixture
        .request(
            "PUT",
            "/api/workspace/running-config",
            json!({"approval_level":"OFF"}),
        )
        .await;
    chat(fixture, "default", "protocol", "protocol-project").await
}

async fn exchange(fixture: &Fixture, thread: &str, query: &str) -> Vec<Value> {
    let (sender, mut receiver) = tokio::sync::mpsc::channel(128);
    let mut session = crate::ConnectionSession { initialized: true };
    fixture
        .server
        .process_line(&mut session, &request(thread, query), &sender)
        .await;
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut messages = Vec::new();
        loop {
            let message: Value = serde_json::from_str(&receiver.recv().await.unwrap()).unwrap();
            let terminal = message["method"] == "turn/completed";
            messages.push(message);
            if terminal {
                return messages;
            }
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn protocol_hook_saved_completion_creates_a_snapshot() {
    let fixture = Fixture::new().await;
    let thread = prepare(&fixture).await;
    let messages = exchange(&fixture, &thread, "write fixture").await;
    assert_eq!(messages[0]["id"], json!(1));
    assert_eq!(
        messages.last().unwrap()["params"]["turn"]["status"],
        json!("completed")
    );
    settle(&fixture).await;
    assert_eq!(summary(&fixture, "default").await, one_auto());
}

#[tokio::test]
async fn protocol_hook_disconnect_before_response_still_snapshots_saved_completion() {
    let fixture = Fixture::new().await;
    let thread = prepare(&fixture).await;
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    sender.send(String::from("occupied")).await.unwrap();
    let server = fixture.server.clone();
    let input = request(&thread, "hold");
    let process = tokio::spawn(async move {
        let mut session = crate::ConnectionSession { initialized: true };
        server.process_line(&mut session, &input, &sender).await;
    });
    fixture.wait_requests(1).await;
    let completions =
        crate::protocol_runs::workspace_completions(&fixture.server, &fixture.data_key("default"));
    assert_eq!(completions.len(), 1);
    process.abort();
    assert!(process.await.unwrap_err().is_cancelled());
    receiver.close();
    fixture
        .remote
        .release_hold
        .store(true, std::sync::atomic::Ordering::Release);
    fixture.remote.hold_released.notify_waiters();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let turns = fixture
                .server
                .inner
                .core
                .read_thread(&thread)
                .await
                .unwrap()
                .turns;
            if turns
                .last()
                .is_some_and(|turn| turn.status == TurnStatus::Completed)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    // Core persistence precedes the protocol producer's checkpoint hook.
    tokio::time::timeout(
        Duration::from_secs(5),
        checkpoint_runtime::drain(completions),
    )
    .await
    .unwrap();
    settle(&fixture).await;
    assert_eq!(summary(&fixture, "default").await, one_auto());
}

#[tokio::test]
async fn protocol_hook_bounded_transport_preserves_every_healthy_delta() {
    let fixture = Fixture::new().await;
    let thread = prepare(&fixture).await;
    let messages = exchange(&fixture, &thread, "burst fixture").await;
    assert_eq!(messages[0]["id"], json!(1));
    let deltas = messages
        .iter()
        .filter(|message| message["method"] == "item/agentMessage/delta")
        .map(|message| message["params"]["delta"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(deltas, vec!["x"; 256]);
    assert_eq!(
        messages.last().unwrap()["params"]["turn"]["status"],
        json!("completed")
    );
    settle(&fixture).await;
    assert_eq!(summary(&fixture, "default").await, one_auto());
}

#[tokio::test]
async fn protocol_hook_agent_controls_drain_full_output_without_cancelling_default() {
    for delete in [false, true] {
        let fixture = Fixture::new().await;
        let default = prepare(&fixture).await;
        actor(&fixture, "writer").await;
        enabled(&fixture, "writer").await;
        let writer = chat(&fixture, "writer", "protocol-writer", "protocol-project").await;
        let params: Value = serde_json::from_str(&request(&writer, "burst fixture")).unwrap();
        let output = fixture
            .server
            .dispatch("turn/start", params["params"].clone())
            .await
            .unwrap_or_else(|error| panic!("{}", error.message));
        let Some(crate::PostResponse::TurnEvents(mut events)) = output.post_response else {
            panic!("expected events");
        };
        tokio::time::timeout(Duration::from_secs(5), async {
            while events.len() != 64 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let params: Value = serde_json::from_str(&request(&default, "hold")).unwrap();
        let default_output = fixture
            .server
            .dispatch("turn/start", params["params"].clone())
            .await
            .unwrap_or_else(|error| panic!("{}", error.message));
        fixture.wait_requests(2).await;
        let previous = fixture
            .server
            .inner
            .core
            .read_thread(&default)
            .await
            .unwrap();
        fixture
            .request(
                if delete { "DELETE" } else { "PATCH" },
                &if delete {
                    String::from("/api/agents/writer")
                } else {
                    String::from("/api/agents/writer/toggle")
                },
                if delete {
                    Value::Null
                } else {
                    json!({"enabled":false})
                },
            )
            .await;
        assert_eq!(
            fixture
                .server
                .inner
                .core
                .read_thread(&default)
                .await
                .unwrap(),
            previous
        );
        let terminal = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(CoreEvent::TurnCompleted(event)) = events.recv().await {
                    break event.turn;
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(terminal.status, TurnStatus::Interrupted);
        assert!(
            fixture
                .server
                .inner
                .core
                .turn_was_persisted(&writer, &terminal.id)
                .await
        );
        assert_eq!(checkpoint_runtime::task_counts(&fixture.server), (0, 0));
        fixture.server.inner.shutdown.cancel();
        drop(default_output);
        tokio::time::timeout(
            Duration::from_secs(5),
            crate::protocol_runs::shutdown(&fixture.server),
        )
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn protocol_hook_real_websocket_disconnect_and_shutdown_keep_distinct_outcomes() {
    for disconnect in [true, false] {
        let mut fixture = Fixture::new().await;
        let thread = prepare(&fixture).await;
        let agent = desktop_agents::context_for_agent(&fixture.server, "default")
            .await
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = fixture.server.clone();
        let serving = tokio::spawn(server.run_http(listener));
        let (mut socket, _) =
            tokio_tungstenite::connect_async(format!("ws://{address}/app-protocol"))
                .await
                .unwrap();
        socket
            .send(Message::Text(
                json!({"id":0,"method":"initialize","params":{
            "clientInfo":{"name":"protocol-fixture","version":"0"}}})
                .to_string()
                .into(),
            ))
            .await
            .unwrap();
        let initialized: Value =
            serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(initialized["result"]["protocolVersion"], json!(3));
        socket
            .send(Message::Text(request(&thread, "hold").into()))
            .await
            .unwrap();
        let started: Value =
            serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(started["id"], json!(1));
        fixture.wait_requests(1).await;
        let completions =
            crate::protocol_runs::workspace_completions(&fixture.server, &agent.data_key);
        assert_eq!(completions.len(), 1);
        if disconnect {
            socket.close(None).await.unwrap();
            fixture
                .remote
                .release_hold
                .store(true, std::sync::atomic::Ordering::Release);
            fixture.remote.hold_released.notify_waiters();
            crate::desktop_console_runs::drain_runs(completions)
                .await
                .unwrap();
            settle(&fixture).await;
            assert_eq!(summary(&fixture, "default").await, one_auto());
        }
        fixture.server.inner.shutdown.cancel();
        tokio::time::timeout(Duration::from_secs(5), serving)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let completed = fixture
            .server
            .inner
            .core
            .read_thread(&thread)
            .await
            .unwrap();
        assert_eq!(
            completed.turns[0].status,
            if disconnect {
                TurnStatus::Completed
            } else {
                TurnStatus::Interrupted
            }
        );
        assert!(
            crate::protocol_runs::workspace_completions(&fixture.server, &agent.data_key)
                .is_empty()
        );
        let before = api(&fixture, "default", "GET", "/graph", Value::Null).await;
        assert_eq!(
            before["summary"],
            if disconnect { one_auto() } else { no_auto() }
        );
        fixture.reopen().await;
        assert_eq!(
            api(&fixture, "default", "GET", "/graph", Value::Null).await,
            before
        );
    }
}

#[tokio::test]
async fn protocol_hook_skips_disabled_auto_slash_and_failed_storage() {
    for mode in ["disabled", "slash", "storage"] {
        let fixture = Fixture::new().await;
        let thread = prepare(&fixture).await;
        if mode == "disabled" {
            api(
                &fixture,
                "default",
                "PATCH",
                "/auto",
                json!({"enabled":false}),
            )
            .await;
        }
        let database =
            rusqlite::Connection::open(fixture.directory.path().join("core.sqlite")).unwrap();
        if mode == "storage" {
            database
                .execute_batch(
                    "CREATE TRIGGER reject_completion BEFORE INSERT ON threads
                 WHEN json_extract(NEW.snapshot, '$.turns[#-1].status') = 'completed'
                 BEGIN SELECT RAISE(FAIL, 'fixture completion write failure'); END;",
                )
                .unwrap();
        }
        let messages = exchange(
            &fixture,
            &thread,
            if mode == "slash" {
                " \n/help"
            } else {
                "write fixture"
            },
        )
        .await;
        let terminal = &messages.last().unwrap()["params"]["turn"];
        assert_eq!(
            terminal["status"],
            if mode == "storage" {
                "failed"
            } else {
                "completed"
            }
        );
        if mode == "storage" {
            assert_eq!(
                terminal["error"],
                json!({"message":"Failed to persist the final turn; the latest state may not survive restart."})
            );
        }
        assert_eq!(
            fixture
                .server
                .inner
                .core
                .turn_was_persisted(&thread, terminal["id"].as_str().unwrap())
                .await,
            mode != "storage"
        );
        settle(&fixture).await;
        assert_eq!(summary(&fixture, "default").await, no_auto(), "{mode}");
    }
}

#[tokio::test]
async fn protocol_hook_keeps_restore_lease_until_completion_hook_finishes() {
    let fixture = Fixture::new().await;
    let thread = prepare(&fixture).await;
    let agent = desktop_agents::context_for_agent(&fixture.server, "default")
        .await
        .unwrap();
    let checkpoint_lock = fixture.server.inner.desktop_checkpoint_lock.lock().await;
    let params: Value = serde_json::from_str(&request(&thread, "write fixture")).unwrap();
    let output = fixture
        .server
        .dispatch("turn/start", params["params"].clone())
        .await
        .unwrap_or_else(|error| panic!("{}", error.message));
    tokio::time::timeout(Duration::from_secs(5), async {
        while fixture
            .server
            .inner
            .core
            .read_thread(&thread)
            .await
            .unwrap()
            .turns[0]
            .status
            != TurnStatus::Completed
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(matches!(
        fixture
            .server
            .inner
            .core
            .begin_restore(Duration::from_millis(30))
            .await,
        Err(qwenpaw_core::CoreError::RestoreTimeout)
    ));
    let completions = crate::protocol_runs::workspace_completions(&fixture.server, &agent.data_key);
    assert_eq!(completions.len(), 1);
    drop(checkpoint_lock);
    crate::desktop_console_runs::drain_runs(completions)
        .await
        .unwrap();
    let guard = fixture
        .server
        .inner
        .core
        .begin_restore(Duration::from_secs(2))
        .await
        .unwrap();
    drop(guard);
    drop(output);
    settle(&fixture).await;
    assert_eq!(summary(&fixture, "default").await, one_auto());
}

#[tokio::test]
async fn protocol_hook_restore_waits_for_sdk_completion_and_preserves_paused_snapshot() {
    let fixture = Fixture::new().await;
    let thread = prepare(&fixture).await;
    let agent = desktop_agents::context_for_agent(&fixture.server, "default")
        .await
        .unwrap();
    let before = fixture
        .server
        .inner
        .core
        .export_thread_checkpoint(&thread)
        .await
        .unwrap();
    std::fs::write(agent.workspace.join("notes.txt"), "original").unwrap();
    let snapshot = api(
        &fixture,
        "default",
        "POST",
        "/snapshot",
        json!({"session_id":"protocol","user_id":"desktop","name":"Before SDK"}),
    )
    .await;
    std::fs::write(agent.workspace.join("notes.txt"), "edited").unwrap();
    let params: Value = serde_json::from_str(&request(&thread, "hold")).unwrap();
    let output = fixture
        .server
        .dispatch("turn/start", params["params"].clone())
        .await
        .unwrap_or_else(|error| panic!("{}", error.message));
    fixture.wait_requests(1).await;
    let restoring = scoped(
        &fixture,
        "default",
        "POST",
        "/api/workspace/checkpoints/restore",
        json!({"commit":snapshot["commit"],"session_id":"protocol","user_id":"desktop",
            "include_files":true,"files":["notes.txt"]}),
    );
    tokio::pin!(restoring);
    assert!(
        tokio::time::timeout(Duration::from_millis(40), &mut restoring)
            .await
            .is_err()
    );
    assert!(crate::desktop_checkpoints::quiescence::is_paused(
        &fixture.server,
        &agent.data_key
    ));
    let queued = fixture
        .server
        .dispatch("turn/start", params["params"].clone());
    tokio::pin!(queued);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut queued)
            .await
            .is_err()
    );
    fixture
        .remote
        .release_hold
        .store(true, std::sync::atomic::Ordering::Release);
    fixture.remote.hold_released.notify_waiters();
    let (status, result) = tokio::time::timeout(Duration::from_secs(5), restoring)
        .await
        .unwrap();
    assert_eq!(status, StatusCode::OK, "{result}");
    let after = fixture
        .server
        .inner
        .core
        .export_thread_checkpoint(&thread)
        .await
        .unwrap();
    assert_eq!(
        (after.turns, after.messages, after.turn_metadata),
        (before.turns, before.messages, before.turn_metadata)
    );
    assert_eq!(
        std::fs::read_to_string(agent.workspace.join("notes.txt")).unwrap(),
        "original"
    );
    assert!(!crate::desktop_checkpoints::quiescence::is_paused(
        &fixture.server,
        &agent.data_key
    ));
    drop(output);
    settle(&fixture).await;
    assert_eq!(
        summary(&fixture, "default").await,
        json!({"total":3,"auto":1,"snapshots":1,"safety":1,"heads":1})
    );
}
