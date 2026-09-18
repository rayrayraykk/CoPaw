use super::*;
use crate::capabilities::RuntimeCapabilities;
use crate::codex::runtime::tests::Fixture;
use crate::codex::sessions::{CodexSessions, ThreadOptions};
use std::path::Path;

const TIMEOUT: Duration = Duration::from_secs(3);

fn request() -> SessionRequest {
    SessionRequest {
        session_id: "session".to_owned(),
        capabilities: RuntimeCapabilities::default(),
        cwd: PathBuf::from("workspace with spaces"),
        options: ThreadOptions::default(),
    }
}

async fn open(fixture: &Fixture) -> CodexSessions {
    CodexSessions::open(fixture.directory.path().join("state"), fixture.pool.clone())
        .await
        .unwrap()
}

async fn wait(path: &Path) {
    tokio::time::timeout(TIMEOUT, async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

fn interrupt_record(fixture: &Fixture) -> Value {
    let value: Value =
        serde_json::from_slice(&std::fs::read(fixture.path(0).join("interrupt.json")).unwrap())
            .unwrap();
    json!({"method":value["method"],"params":value["params"]})
}

#[test]
fn full_input_parameters_preserve_original_blocks_defaults_and_sandbox_mapping() {
    let mut req = request();
    req.options = ThreadOptions {
        model: Some("fixture-model".to_owned()),
        sandbox: Some("workspace-write".to_owned()),
        approval_policy: Some("on-request".to_owned()),
    };
    let input = TurnInput {
        prompt: "hi".to_owned(),
        reasoning_effort: Some("high".to_owned()),
        reasoning_summary: Some("detailed".to_owned()),
        attachments: vec![
            Attachment::Image(PathBuf::from("photo.png")),
            Attachment::File {
                path: PathBuf::from("notes.txt"),
                name: String::new(),
            },
            Attachment::File {
                path: PathBuf::from("data.csv"),
                name: "custom.csv".to_owned(),
            },
        ],
    };
    assert_eq!(
        parameters(&req, input).unwrap(),
        json!({"cwd":"workspace with spaces","input":[
        {"type":"text","text":"hi"},{"type":"localImage","path":"photo.png"},
        {"type":"text","text":"Attached file notes.txt: notes.txt"},
        {"type":"text","text":"Attached file custom.csv: data.csv"}],
        "model":"fixture-model","effort":"high","summary":"detailed","approvalPolicy":"on-request","sandboxPolicy":{"type":"workspaceWrite"}})
    );
    assert_eq!(
        parameters(&request(), TurnInput::default()).unwrap(),
        json!({"cwd":"workspace with spaces","input":[],"summary":"auto"})
    );
    for (sandbox, kind) in [
        ("read-only", "readOnly"),
        ("danger-full-access", "dangerFullAccess"),
    ] {
        req.options.sandbox = Some(sandbox.to_owned());
        assert_eq!(
            parameters(&req, TurnInput::default()).unwrap()["sandboxPolicy"],
            json!({"type":kind})
        );
    }
}

#[tokio::test]
async fn session_turn_preserves_early_notifications_and_filters_other_threads_and_turns() {
    let fixture = Fixture::new("normal");
    let sessions = open(&fixture).await;
    let mut turn = sessions
        .start_turn(
            request(),
            TurnInput {
                prompt: "hi".to_owned(),
                ..Default::default()
            },
            TIMEOUT,
        )
        .await
        .unwrap();
    let mut events = Vec::new();
    while let Some(event) = turn.next().await {
        events.push(event);
    }
    let complete = json!({"method":"turn/completed","params":{"threadId":"fixture-thread-1","turn":{"id":"fixture-turn","status":"completed","error":null}}});
    assert_eq!(
        events,
        vec![
            json!({"method":"item/agentMessage/delta","params":{"threadId":"fixture-thread-1","turnId":"fixture-turn","delta":"hello 世界"}}),
            json!({"method":"item/mcpToolCall/progress","params":{"threadId":"fixture-thread-1","turnId":"fixture-turn","itemId":"tool-1","message":"working"}}),
            complete.clone()
        ]
    );
    assert_eq!(turn.finish().await, Ok(TurnEnd::Remote(complete)));
    let messages: Vec<Value> =
        serde_json::from_slice(&std::fs::read(fixture.path(0).join("received.json")).unwrap())
            .unwrap();
    assert_eq!(
        messages
            .iter()
            .find(|m| m["method"] == "turn/start")
            .unwrap()["params"],
        json!({"threadId":"fixture-thread-1","cwd":"workspace with spaces","input":[{"type":"text","text":"hi"}],"summary":"auto"})
    );
    sessions.shutdown().await.unwrap();
    fixture.pool.shutdown().await.unwrap();
}

#[tokio::test]
async fn cancellation_after_start_waits_for_interrupt_acknowledgement() {
    let fixture = Fixture::new("turn-wait");
    let sessions = open(&fixture).await;
    let mut turn = sessions
        .start_turn(request(), TurnInput::default(), TIMEOUT)
        .await
        .unwrap();
    turn.next().await.unwrap();
    turn.cancel();
    assert_eq!(turn.finish().await, Ok(TurnEnd::InterruptAcknowledged));
    assert_eq!(
        interrupt_record(&fixture),
        json!({"method":"turn/interrupt","params":{"threadId":"fixture-thread-1","turnId":"fixture-turn"}})
    );
    sessions.shutdown().await.unwrap();
    fixture.pool.shutdown().await.unwrap();
}

#[tokio::test]
async fn cancellation_before_start_response_still_collects_id_and_interrupts() {
    let fixture = Fixture::new("gated-turn");
    let sessions = open(&fixture).await;
    let turn = sessions
        .start_turn(request(), TurnInput::default(), TIMEOUT)
        .await
        .unwrap();
    wait(&fixture.path(0).join("turn-seen")).await;
    turn.cancel();
    std::fs::write(fixture.path(0).join("release-turn"), b"release").unwrap();
    assert_eq!(turn.finish().await, Ok(TurnEnd::InterruptAcknowledged));
    assert_eq!(
        interrupt_record(&fixture)["params"],
        json!({"threadId":"fixture-thread-1","turnId":"fixture-turn"})
    );
    sessions.shutdown().await.unwrap();
    fixture.pool.shutdown().await.unwrap();
}

#[tokio::test]
async fn dropping_turn_requests_interrupt_without_stopping_shared_process() {
    let fixture = Fixture::new("turn-wait");
    let sessions = open(&fixture).await;
    let mut turn = sessions
        .start_turn(request(), TurnInput::default(), TIMEOUT)
        .await
        .unwrap();
    turn.next().await.unwrap();
    drop(turn);
    wait(&fixture.path(0).join("interrupt.json")).await;
    let prepared = sessions.prepare(request(), TIMEOUT).await.unwrap();
    assert_eq!(
        prepared
            .runtime
            .client
            .request("fixture/echo", json!({"alive":true}), TIMEOUT)
            .await,
        Ok(json!({"alive":true}))
    );
    sessions.shutdown().await.unwrap();
    fixture.pool.shutdown().await.unwrap();
}

#[tokio::test]
async fn remote_failed_status_is_preserved_and_missing_turn_id_is_an_error() {
    for mode in ["turn-failed", "missing-turn"] {
        let fixture = Fixture::new(mode);
        let sessions = open(&fixture).await;
        let turn = sessions
            .start_turn(request(), TurnInput::default(), TIMEOUT)
            .await
            .unwrap();
        let result = turn.finish().await;
        if mode == "missing-turn" {
            assert_eq!(result, Err(Error::MissingTurnId));
        } else {
            assert!(
                matches!(result,Ok(TurnEnd::Remote(value)) if value["params"]["turn"]["status"]=="failed")
            );
        }
        sessions.shutdown().await.unwrap();
        fixture.pool.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn interrupt_failure_and_transport_eof_are_not_successful_completion() {
    for mode in ["interrupt-error", "turn-wait"] {
        let fixture = Fixture::new(mode);
        let sessions = open(&fixture).await;
        let mut turn = sessions
            .start_turn(request(), TurnInput::default(), TIMEOUT)
            .await
            .unwrap();
        turn.next().await.unwrap();
        if mode == "interrupt-error" {
            turn.cancel();
        } else {
            sessions.stop().await.unwrap();
        }
        let result = turn.finish().await;
        if mode == "interrupt-error" {
            assert!(matches!(result, Err(Error::Protocol { code: -32004, .. })));
        } else {
            assert_eq!(result, Err(Error::Closed));
        }
        sessions.shutdown().await.unwrap();
        fixture.pool.shutdown().await.unwrap();
    }
}

#[test]
fn notification_filter_preserves_original_missing_id_and_nested_id_rules() {
    for params in [
        json!({"threadId":"t"}),
        json!({"threadId":"t","turnId":"v"}),
        json!({"threadId":"t","turn":{"id":"v"}}),
    ] {
        assert!(matches_turn(&json!({"params":params}), "t", "v"));
    }
    assert!(!matches_turn(
        &json!({"params":{"threadId":"t","turn":{"id":"other"}}}),
        "t",
        "v"
    ));
    assert!(!matches_turn(
        &json!({"params":{"threadId":"other","turnId":"v"}}),
        "t",
        "v"
    ));
    assert!(!matches_turn(
        &json!({"params":{"threadId":"t","turnId":7}}),
        "t",
        "v"
    ));
    assert!(matches_turn(
        &json!({"params":{"threadId":"t","turnId":7}}),
        "t",
        "7"
    ));
}

#[tokio::test]
async fn lost_notifications_interrupt_before_reporting_explicit_lag() {
    let mut peer = crate::codex::tests::Peer::new();
    let session = PreparedSession {
        runtime: super::super::runtime::PreparedRuntime {
            client: peer.client.clone(),
            fingerprint: "fixture".to_owned(),
        },
        thread_id: "thread".to_owned(),
    };
    let turn = CodexTurn::start(session, json!({}), TIMEOUT).unwrap();
    let start = peer.receive().await;
    {
        let state = peer.client.shared.state.lock().unwrap();
        for _ in 0..1500 {
            state.notifications.as_ref().unwrap().send(json!({"method":"item/agentMessage/delta","params":{"threadId":"thread","turnId":"turn","delta":"x"}})).unwrap();
        }
    }
    peer.send(json!({"id":start["id"],"result":{"turn":{"id":"turn"}}}))
        .await;
    let interrupt = peer.receive().await;
    assert_eq!(interrupt["method"], "turn/interrupt");
    peer.send(json!({"id":interrupt["id"],"result":{}})).await;
    assert!(matches!(turn.finish().await,Err(Error::NotificationLagged(count)) if count > 0));
}

#[tokio::test]
async fn full_consumer_buffer_does_not_block_cancellation_rpc() {
    let mut peer = crate::codex::tests::Peer::new();
    let session = PreparedSession {
        runtime: super::super::runtime::PreparedRuntime {
            client: peer.client.clone(),
            fingerprint: "fixture".to_owned(),
        },
        thread_id: "thread".to_owned(),
    };
    let turn = CodexTurn::start(session, json!({}), TIMEOUT).unwrap();
    let start = peer.receive().await;
    {
        let state = peer.client.shared.state.lock().unwrap();
        for _ in 0..80 {
            state.notifications.as_ref().unwrap().send(json!({"method":"item/agentMessage/delta","params":{"threadId":"thread","turnId":"turn","delta":"x"}})).unwrap();
        }
    }
    peer.send(json!({"id":start["id"],"result":{"turn":{"id":"turn"}}}))
        .await;
    tokio::time::timeout(TIMEOUT, async {
        while turn.incoming.len() < 64 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    turn.cancel();
    let interrupt = peer.receive().await;
    assert_eq!(interrupt["method"], "turn/interrupt");
    peer.send(json!({"id":interrupt["id"],"result":{}})).await;
    assert_eq!(turn.finish().await, Ok(TurnEnd::InterruptAcknowledged));
}
