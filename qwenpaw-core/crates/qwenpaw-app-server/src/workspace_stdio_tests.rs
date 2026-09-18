use std::time::Duration;

use pretty_assertions::assert_eq;
use qwenpaw_protocol::TurnStatus;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines};

use super::{Fixture, checkpoint_runtime, open_headless, prepare, request};

struct Transport {
    input: DuplexStream,
    output: Lines<BufReader<DuplexStream>>,
    task: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl Transport {
    async fn start(fixture: &Fixture) -> Self {
        let (input, server_input) = tokio::io::duplex(65_536);
        let (server_output, output) = tokio::io::duplex(65_536);
        let task = tokio::spawn(crate::stdio::run(
            fixture.server.clone(),
            server_input,
            server_output,
        ));
        let mut transport = Self {
            input,
            output: BufReader::new(output).lines(),
            task,
        };
        transport
            .send(
                &json!({"id":0,"method":"initialize","params":{
                    "clientInfo":{"name":"workspace-stdio","version":"1"}
                }})
                .to_string(),
            )
            .await;
        assert_eq!(transport.next().await["result"]["protocolVersion"], 3);
        transport
    }

    async fn send(&mut self, line: &str) {
        self.input
            .write_all(format!("{line}\n").as_bytes())
            .await
            .unwrap();
    }

    async fn next(&mut self) -> Value {
        let line = tokio::time::timeout(Duration::from_secs(5), self.output.next_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        serde_json::from_str(&line).unwrap()
    }

    async fn event(&mut self, method: &str) -> Value {
        loop {
            let value = self.next().await;
            if value["method"] == method {
                return value;
            }
        }
    }

    async fn finish(mut self) {
        self.input.shutdown().await.unwrap();
        let result = tokio::time::timeout(Duration::from_secs(5), &mut self.task).await;
        if result.is_err() {
            self.task.abort();
            let _ = self.task.await;
            panic!("Workspace stdio host did not finish");
        }
        result.unwrap().unwrap().unwrap();
    }
}

#[tokio::test]
async fn eof_interrupts_protocol_approval_and_persists_before_returning() {
    let mut fixture = Fixture::new().await;
    let thread = prepare(&fixture).await;
    fixture
        .request(
            "PUT",
            "/api/workspace/running-config",
            json!({"approval_level":"STRICT"}),
        )
        .await;
    open_headless(&mut fixture).await;
    let mut transport = Transport::start(&fixture).await;
    transport.send(&request(&thread, "write fixture")).await;
    let approval = transport.event("tool/approval/requested").await;
    assert_eq!(approval["params"]["toolName"], "write_file");
    assert_eq!(
        fixture
            .server
            .inner
            .desktop_pending_approvals
            .read()
            .await
            .len(),
        1
    );
    transport.finish().await;
    let history = fixture
        .server
        .inner
        .core
        .read_thread(&thread)
        .await
        .unwrap();
    assert_eq!(
        history.turns.last().unwrap().status,
        TurnStatus::Interrupted
    );
    assert!(
        fixture
            .server
            .inner
            .desktop_pending_approvals
            .read()
            .await
            .is_empty()
    );
    assert_eq!(checkpoint_runtime::task_counts(&fixture.server), (0, 0));
    assert!(
        !fixture
            .directory
            .path()
            .join("protocol-project/cron-output.txt")
            .exists()
    );
    fixture.reopen().await;
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .read_thread(&thread)
            .await
            .unwrap(),
        history
    );
}

#[tokio::test]
async fn eof_drains_pending_checkpoint_tasks_without_losing_the_saved_turn() {
    let mut fixture = Fixture::new().await;
    let thread = prepare(&fixture).await;
    open_headless(&mut fixture).await;
    let mut transport = Transport::start(&fixture).await;
    transport.send(&request(&thread, "write fixture")).await;
    let terminal = transport.event("turn/completed").await;
    assert_eq!(terminal["params"]["turn"]["status"], "completed");
    assert_eq!(checkpoint_runtime::task_counts(&fixture.server), (1, 0));
    let history = fixture
        .server
        .inner
        .core
        .read_thread(&thread)
        .await
        .unwrap();
    transport.finish().await;
    assert_eq!(checkpoint_runtime::task_counts(&fixture.server), (0, 0));
    assert_eq!(
        std::fs::read_to_string(
            fixture
                .directory
                .path()
                .join("protocol-project/cron-output.txt")
        )
        .unwrap(),
        "created by Cron"
    );
    fixture.reopen().await;
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .read_thread(&thread)
            .await
            .unwrap(),
        history
    );
}

#[tokio::test]
async fn eof_waits_for_admitted_heartbeat_completion_and_rejects_new_admission() {
    let mut fixture = Fixture::new().await;
    open_headless(&mut fixture).await;
    let lease = crate::desktop_checkpoints::quiescence::begin_heartbeat(&fixture.server).unwrap();
    let transport = Transport::start(&fixture).await;
    let task = tokio::spawn(transport.finish());
    tokio::time::timeout(
        Duration::from_secs(5),
        fixture.server.inner.shutdown.cancelled(),
    )
    .await
    .unwrap();
    assert!(!task.is_finished());
    drop(lease);
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    assert!(crate::desktop_checkpoints::quiescence::begin_heartbeat(&fixture.server).is_none());
    assert!(
        !fixture
            .server
            .inner
            .desktop_heartbeat_running
            .load(std::sync::atomic::Ordering::Acquire)
    );
}

#[tokio::test]
async fn eof_stops_a_real_heartbeat_model_request_and_keeps_its_result_on_reopen() {
    let mut fixture = Fixture::new().await;
    std::fs::write(
        fixture.directory.path().join("workspace/HEARTBEAT.md"),
        "hold",
    )
    .unwrap();
    assert_eq!(
        fixture
            .request("POST", "/api/config/heartbeat/run", Value::Null)
            .await,
        json!({"started":true})
    );
    fixture.wait_requests(1).await;
    assert!(
        fixture
            .server
            .inner
            .desktop_heartbeat_running
            .load(std::sync::atomic::Ordering::Acquire)
    );
    let transport = Transport::start(&fixture).await;
    transport.finish().await;
    assert!(
        !fixture
            .server
            .inner
            .desktop_heartbeat_running
            .load(std::sync::atomic::Ordering::Acquire)
    );
    let snapshot = fixture
        .server
        .inner
        .core
        .backup_snapshot(1024 * 1024)
        .unwrap();
    assert!(!snapshot.threads.is_empty());
    for stored in &snapshot.threads {
        let thread = fixture
            .server
            .inner
            .core
            .read_thread(&stored.thread.id)
            .await
            .unwrap();
        assert_eq!(thread.turns.last().unwrap().status, TurnStatus::Interrupted);
    }
    fixture.reopen().await;
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .backup_snapshot(1024 * 1024)
            .unwrap()
            .threads,
        snapshot.threads
    );
    assert!(
        !fixture
            .directory
            .path()
            .join("workspace/cron-output.txt")
            .exists()
    );
}
