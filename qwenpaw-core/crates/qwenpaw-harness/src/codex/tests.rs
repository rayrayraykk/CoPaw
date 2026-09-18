use super::*;
use std::io::{BufRead, Write};
use tokio::io::{DuplexStream, ReadHalf, WriteHalf};

const TEST_TIMEOUT: Duration = Duration::from_secs(3);

pub(super) struct Peer {
    pub(super) client: CodexClient,
    input: BufReader<ReadHalf<DuplexStream>>,
    output: WriteHalf<DuplexStream>,
    reader: JoinHandle<()>,
    writer: JoinHandle<()>,
}

impl Peer {
    pub(super) fn new() -> Self {
        let (local, remote) = tokio::io::duplex(16384);
        let (input, output) = tokio::io::split(local);
        let (client, reader, writer) = connect(input, output, None);
        let (input, output) = tokio::io::split(remote);
        Self {
            client,
            input: BufReader::new(input),
            output,
            reader,
            writer,
        }
    }

    pub(super) async fn receive(&mut self) -> Value {
        let bytes = tokio::time::timeout(TEST_TIMEOUT, read_frame(&mut self.input))
            .await
            .unwrap()
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    pub(super) async fn send(&mut self, message: Value) {
        self.output
            .write_all(format!("{message}\n").as_bytes())
            .await
            .unwrap();
    }

    fn request(&self, method: &'static str) -> JoinHandle<Result<Value, Error>> {
        let client = self.client.clone();
        tokio::spawn(async move { client.request(method, json!({}), TEST_TIMEOUT).await })
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        self.client.shared.close(Error::Closed);
        self.reader.abort();
        self.writer.abort();
    }
}

#[tokio::test]
async fn concurrent_responses_are_correlated_and_errors_keep_details() {
    let mut peer = Peer::new();
    let first = peer.request("first");
    let one = peer.receive().await;
    let second = peer.request("second");
    let two = peer.receive().await;
    assert_eq!(one, json!({"id":1,"method":"first","params":{}}));
    assert_eq!(two, json!({"id":2,"method":"second","params":{}}));
    peer.send(json!({"id":2,"result":{"ok":2}})).await;
    peer.send(json!({"id":1,"error":{"code":-32001,"message":"busy","data":{"retry":1}}}))
        .await;
    assert_eq!(second.await.unwrap(), Ok(json!({"ok":2})));
    assert_eq!(
        first.await.unwrap(),
        Err(Error::Protocol {
            code: -32001,
            message: "busy".to_owned(),
            data: json!({"retry":1}),
        })
    );
}

#[tokio::test]
async fn approval_with_colliding_id_can_make_nested_request_without_blocking_reader() {
    let mut peer = Peer::new();
    let nested = peer.client.clone();
    peer.client.set_request_handler(Some(Arc::new(move |message| {
        let client = nested.clone();
        Box::pin(async move {
            assert_eq!(message, json!({"id":1,"method":"item/commandExecution/requestApproval","params":{"threadId":"a"}}));
            let status = client.request("account/read", json!({}), TEST_TIMEOUT).await?;
            assert_eq!(status,json!({"authenticated":true}));
            Ok(json!({"decision":"accept"}))
        })
    })));
    let outer = peer.request("turn/start");
    peer.receive().await;
    peer.send(
        json!({"id":1,"method":"item/commandExecution/requestApproval","params":{"threadId":"a"}}),
    )
    .await;
    assert_eq!(
        peer.receive().await,
        json!({"id":2,"method":"account/read","params":{}})
    );
    peer.send(json!({"id":2,"result":{"authenticated":true}}))
        .await;
    assert_eq!(
        peer.receive().await,
        json!({"id":1,"result":{"decision":"accept"}})
    );
    assert!(!outer.is_finished());
    peer.send(json!({"id":1,"result":{"turn":"a"}})).await;
    assert_eq!(outer.await.unwrap(), Ok(json!({"turn":"a"})));
}

#[tokio::test]
async fn absent_failed_and_panicking_handlers_decline_string_and_zero_ids() {
    let mut peer = Peer::new();
    for (id, mode) in [(json!("approval"), 0), (json!(0), 1), (json!(-3), 2)] {
        peer.client.set_request_handler(match mode {
            1 => Some(Arc::new(|_| Box::pin(async { Err(Error::Closed) }))),
            2 => Some(Arc::new(|_| {
                Box::pin(async { panic!("test handler failed") })
            })),
            _ => None,
        });
        peer.send(json!({"id":id,"method":"item/fileChange/requestApproval","params":{}}))
            .await;
        assert_eq!(
            peer.receive().await,
            json!({"id":id,"result":{"decision":"decline"}})
        );
    }
}

#[tokio::test]
async fn notifications_fan_out_and_eof_closes_subscribers_and_requests() {
    let mut peer = Peer::new();
    let mut one = peer.client.subscribe().unwrap();
    let mut two = peer.client.subscribe().unwrap();
    let request = peer.request("waiting");
    peer.receive().await;
    peer.output
        .write_all(b"non-JSON diagnostic\n")
        .await
        .unwrap();
    let event = json!({"method":"item/agentMessage/delta","params":{"delta":"hello"}});
    peer.send(event.clone()).await;
    assert_eq!(one.recv().await.unwrap(), event);
    assert_eq!(two.recv().await.unwrap(), event);
    peer.output.shutdown().await.unwrap();
    assert_eq!(request.await.unwrap(), Err(Error::Closed));
    assert_eq!(one.recv().await, Err(broadcast::error::RecvError::Closed));
    assert!(matches!(peer.client.subscribe(), Err(Error::Closed)));
}

#[tokio::test]
async fn timeout_and_caller_cancellation_remove_pending_without_id_reuse() {
    let mut peer = Peer::new();
    let client = peer.client.clone();
    let timeout = tokio::spawn(async move {
        client
            .request("timeout", json!({}), Duration::from_millis(20))
            .await
    });
    assert_eq!(peer.receive().await["id"], 1);
    assert_eq!(timeout.await.unwrap(), Err(Error::Timeout));
    let cancelled = peer.request("cancelled");
    assert_eq!(peer.receive().await["id"], 2);
    cancelled.abort();
    let _ = cancelled.await;
    assert!(peer.client.shared.state.lock().unwrap().pending.is_empty());
    let next = peer.request("next");
    assert_eq!(peer.receive().await["id"], 3);
    peer.send(json!({"id":1,"result":"late"})).await;
    peer.send(json!({"id":2,"result":"cancelled"})).await;
    peer.send(json!({"id":3,"result":"current"})).await;
    assert_eq!(next.await.unwrap(), Ok(json!("current")));
}

#[tokio::test]
async fn frames_are_bounded_and_partial_eof_is_not_a_successful_response() {
    let mut huge = BufReader::new(std::io::Cursor::new(vec![b'x'; MAX_FRAME_BYTES + 2]));
    assert_eq!(read_frame(&mut huge).await, Err(Error::FrameTooLarge));
    let mut partial = BufReader::new(std::io::Cursor::new(b"{\"id\":1}"));
    assert_eq!(read_frame(&mut partial).await, Err(Error::Closed));
    let mut peer = Peer::new();
    let request = peer.request("bad-frame");
    peer.receive().await;
    peer.send(json!([])).await;
    assert_eq!(request.await.unwrap(), Err(Error::InvalidFrame));
}

pub(super) fn child_command(directory: &std::path::Path, mode: &str) -> Command {
    // A separate target-dir-with-spaces run covers executable paths, without
    // copying a running Mach-O image into an unqualified executable location.
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--quiet",
            "--nocapture",
            "--exact",
            "codex::tests::child_fixture",
        ])
        .env_clear()
        .env("QWENPAW_HARNESS_TEST_MODE", mode)
        .current_dir(directory);
    for name in ["SystemRoot", "WINDIR", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
}

#[tokio::test]
async fn real_child_handshake_echo_and_graceful_shutdown_preserve_exact_messages() {
    let directory = tempfile::Builder::new()
        .prefix("harness workspace ")
        .tempdir()
        .unwrap();
    let result = CodexProcess::spawn(child_command(directory.path(), "normal"), TEST_TIMEOUT).await;
    assert!(
        result.is_ok(),
        "error: {:?}; started: {:?}; received: {:?}",
        result.as_ref().err(),
        std::fs::read_to_string(directory.path().join("started.json")),
        std::fs::read_to_string(directory.path().join("received.json"))
    );
    let process = result.unwrap();
    let client = process.client();
    assert_eq!(
        client
            .request("fixture/echo", json!({"text":"hello 世界"}), TEST_TIMEOUT)
            .await,
        Ok(json!({"text":"hello 世界"}))
    );
    process.shutdown().await.unwrap();
    let record: Value =
        serde_json::from_slice(&std::fs::read(directory.path().join("finished.json")).unwrap())
            .unwrap();
    assert_eq!(
        record,
        json!([
            {"id":1,"method":"initialize","params":{"clientInfo":{"name":"qwenpaw","title":"QwenPaw","version":"1"}}},
            {"method":"initialized","params":{}},
            {"id":2,"method":"fixture/echo","params":{"text":"hello 世界"}}
        ])
    );
    assert_eq!(
        client.request("after-stop", json!({}), TEST_TIMEOUT).await,
        Err(Error::Closed)
    );
}

#[tokio::test]
async fn child_stop_rejects_pending_and_does_not_leave_stdio_workers_alive() {
    let directory = tempfile::tempdir().unwrap();
    let process = CodexProcess::spawn(child_command(directory.path(), "normal"), TEST_TIMEOUT)
        .await
        .unwrap();
    let client = process.client();
    let mut notifications = client.subscribe().unwrap();
    let request = tokio::spawn(async move {
        client
            .request("fixture/wait", json!({}), TEST_TIMEOUT)
            .await
    });
    assert_eq!(
        tokio::time::timeout(TEST_TIMEOUT, notifications.recv())
            .await
            .unwrap()
            .unwrap(),
        json!({"method":"fixture/received","params":{}})
    );
    process.shutdown().await.unwrap();
    assert_eq!(request.await.unwrap(), Err(Error::Closed));
    assert_eq!(
        notifications.recv().await,
        Err(broadcast::error::RecvError::Closed)
    );
    assert!(directory.path().join("finished.json").exists());
}

#[tokio::test]
async fn handshake_failure_and_timeout_reap_the_child() {
    for mode in ["handshake-error", "handshake-timeout"] {
        let directory = tempfile::tempdir().unwrap();
        let result = CodexProcess::spawn(child_command(directory.path(), mode), TEST_TIMEOUT).await;
        match mode {
            "handshake-error" => assert!(
                matches!(result, Err(Error::Protocol { code: -32600, .. })),
                "actual error: {:?}",
                result.as_ref().err()
            ),
            _ => assert!(
                matches!(result, Err(Error::Timeout)),
                "actual error: {:?}",
                result.as_ref().err()
            ),
        }
        let record: Value =
            serde_json::from_slice(&std::fs::read(directory.path().join("finished.json")).unwrap())
                .unwrap();
        assert_eq!(
            record,
            json!([{"id":1,"method":"initialize","params":{"clientInfo":{"name":"qwenpaw","title":"QwenPaw","version":"1"}}}])
        );
    }
}

#[tokio::test]
async fn abnormal_child_exit_is_not_reported_as_clean_shutdown() {
    let directory = tempfile::tempdir().unwrap();
    let process = CodexProcess::spawn(child_command(directory.path(), "normal"), TEST_TIMEOUT)
        .await
        .unwrap();
    let result = process
        .client()
        .request("fixture/exit", json!({}), TEST_TIMEOUT)
        .await;
    assert!(matches!(
        result,
        Err(Error::Closed | Error::ProcessExit(Some(7)))
    ));
    assert_eq!(process.shutdown().await, Err(Error::ProcessExit(Some(7))));
}

#[test]
fn child_fixture() {
    let Ok(mode) = std::env::var("QWENPAW_HARNESS_TEST_MODE") else {
        return;
    };
    std::fs::write(
        "started.json",
        serde_json::to_vec(&json!({"mode":mode,"pid":std::process::id()})).unwrap(),
    )
    .unwrap();
    if mode == "early-approval" {
        println!(
            "{}",
            json!({"id":"startup-approval","method":"item/commandExecution/requestApproval","params":{}})
        );
        std::io::stdout().flush().unwrap();
    }
    let mut messages = Vec::new();
    for line in std::io::stdin().lock().lines() {
        let message: Value = serde_json::from_str(&line.unwrap()).unwrap();
        messages.push(message.clone());
        std::fs::write("received.json", serde_json::to_vec(&messages).unwrap()).unwrap();
        if message.get("method").is_none() {
            assert_eq!(message["id"], "startup-approval");
            std::fs::write("approval.json", serde_json::to_vec(&message).unwrap()).unwrap();
            continue;
        }
        if message["method"] == "initialize" && mode == "gated-handshake" {
            std::fs::write("initialize-seen", b"ready").unwrap();
            fixture_gate("release-start");
        }
        let response = match message["method"].as_str().unwrap() {
            "initialize" if mode.starts_with("handshake-timeout") => continue,
            "initialize" if mode == "handshake-error" => {
                json!({"id":message["id"],"error":{"code":-32600,"message":"no"}})
            }
            "initialize" => json!({"id":message["id"],"result":{"userAgent":"isolated-rust-peer"}}),
            "initialized" => continue,
            "skills/extraRoots/set" => {
                let Some(response) = roots_response(&message, &mode) else {
                    continue;
                };
                response
            }
            "fixture/launch" => json!({"id":message["id"],"result":{
                "cwd":std::env::current_dir().unwrap(),
                "value":std::env::var("FIXTURE_PROJECTED_VALUE").ok(),
                "base":std::env::var("FIXTURE_BASE_VALUE").ok()
            }}),
            "fixture/echo" => json!({"id":message["id"],"result":message["params"]}),
            "thread/start" | "thread/resume" => match thread_response(&message, &messages, &mode) {
                Some(response) => response,
                None => continue,
            },
            "turn/start" | "turn/interrupt" => turn_response(&message, &mode),
            "account/read" if mode == "provider-no-account" => {
                json!({"id":message["id"],"result":{"account":null,"requiresOpenaiAuth":false}})
            }
            "account/read" if mode == "provider-error" => {
                json!({"id":message["id"],"error":{"code":-32001,"message":"login required","data":{"private":"not-public"}}})
            }
            "account/read" => json!({"id":message["id"],"result":{"account":{
                "type":"chatgpt","email":"fixture@example.invalid","planType":"plus","privateToken":"not-public"
            }}}),
            "account/login/start" => json!({"id":message["id"],"result":{
                "type":message["params"]["type"],"loginId":"fixture-login"
            }}),
            "account/logout" => json!({"id":message["id"],"result":{}}),
            "skills/list" => json!({"id":message["id"],"result":{"data":[{
                "cwd":message["params"]["cwds"][0],"skills":[
                    {"name":"fixture-skill","description":"Fixture skill","scope":"user",
                     "enabled":true,"path":"private-skill-path","private":"not-public"}
                ]
            }]}}),
            "model/list" => {
                if message["params"]["cursor"].is_null() {
                    json!({"id":message["id"],"result":{"data":[{"id":"first"}],"nextCursor":"page-two"}})
                } else {
                    json!({"id":message["id"],"result":{"data":[{"model":"second"}],"nextCursor":null}})
                }
            }
            "fixture/wait" => json!({"method":"fixture/received","params":{}}),
            "fixture/exit" => std::process::exit(7),
            _ => panic!("Unexpected fixture method"),
        };
        println!("{response}");
        std::io::stdout().flush().unwrap();
    }
    if mode == "gated-eof" {
        std::fs::write("eof-seen", b"ready").unwrap();
        fixture_gate("release-stop");
    }
    std::fs::write("finished.json", serde_json::to_vec(&messages).unwrap()).unwrap();
    if mode == "ignore-eof" || mode == "handshake-timeout-ignore-eof" {
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
}

fn roots_response(message: &Value, mode: &str) -> Option<Value> {
    if mode == "gated-roots" {
        std::fs::write("roots-seen", b"ready").unwrap();
        fixture_gate("release-roots");
    }
    if std::path::Path::new("timeout-roots").exists() {
        return None;
    }
    Some(if std::path::Path::new("reject-roots").exists() {
        json!({"id":message["id"],"error":{"code":-32002,"message":"roots rejected"}})
    } else {
        json!({"id":message["id"],"result":{}})
    })
}

fn turn_response(message: &Value, mode: &str) -> Value {
    if message["method"] == "turn/interrupt" {
        std::fs::write("interrupt.json", serde_json::to_vec(message).unwrap()).unwrap();
        return if mode == "interrupt-error" {
            json!({"id":message["id"],"error":{"code":-32004,"message":"cannot interrupt"}})
        } else {
            json!({"id":message["id"],"result":{}})
        };
    }
    let thread = &message["params"]["threadId"];
    for event in [
        json!({"method":"item/agentMessage/delta","params":{"threadId":"other","turnId":"fixture-turn","delta":"wrong thread"}}),
        json!({"method":"turn/completed","params":{"threadId":thread,"turn":{"id":"other","status":"completed"}}}),
        json!({"method":"item/agentMessage/delta","params":{"threadId":thread,"turnId":"fixture-turn","delta":"hello 世界"}}),
        json!({"method":"item/mcpToolCall/progress","params":{"threadId":thread,"turnId":"fixture-turn","itemId":"tool-1","message":"working"}}),
    ] {
        println!("{event}");
    }
    std::io::stdout().flush().unwrap();
    if mode == "gated-turn" {
        std::fs::write("turn-seen", b"ready").unwrap();
        fixture_gate("release-turn");
    }
    if mode == "missing-turn" {
        return json!({"id":message["id"],"result":{"turn":{}}});
    }
    if !["turn-wait", "gated-turn", "interrupt-error"].contains(&mode) {
        let status = if mode == "turn-failed" {
            "failed"
        } else {
            "completed"
        };
        println!(
            "{}",
            json!({"method":"turn/completed","params":{"threadId":thread,"turn":{"id":"fixture-turn","status":status,"error":null}}})
        );
        std::io::stdout().flush().unwrap();
    }
    json!({"id":message["id"],"result":{"turn":{"id":"fixture-turn"}}})
}

fn thread_response(message: &Value, messages: &[Value], mode: &str) -> Option<Value> {
    Some(match message["method"].as_str().unwrap() {
        "thread/start" => {
            if mode == "gated-thread" {
                std::fs::write("thread-seen", b"ready").unwrap();
                fixture_gate("release-thread");
            }
            if std::path::Path::new("missing-thread-id").exists() {
                json!({"id":message["id"],"result":{"thread":{}}})
            } else {
                let count = messages
                    .iter()
                    .filter(|m| m["method"] == "thread/start")
                    .count();
                json!({"id":message["id"],"result":{"thread":{"id":format!("fixture-thread-{count}")}}})
            }
        }
        "thread/resume" if std::path::Path::new("timeout-resume").exists() => return None,
        "thread/resume" if std::path::Path::new("reject-resume").exists() => {
            json!({"id":message["id"],"error":{"code":-32003,"message":"thread missing"}})
        }
        "thread/resume" => {
            json!({"id":message["id"],"result":{"thread":{"id":message["params"]["threadId"]}}})
        }
        _ => panic!("Unexpected thread fixture method"),
    })
}

fn fixture_gate(path: &str) {
    let deadline = std::time::Instant::now() + TEST_TIMEOUT;
    while !std::path::Path::new(path).exists() {
        assert!(std::time::Instant::now() < deadline, "fixture gate expired");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[tokio::test]
async fn fixture_child_emits_handshake_without_transport() {
    let directory = tempfile::tempdir().unwrap();
    let mut child = child_command(directory.path(), "normal")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    input
        .write_all(b"{\"id\":1,\"method\":\"initialize\",\"params\":{}}\n")
        .await
        .unwrap();
    input.flush().await.unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut transcript = Vec::new();
    let result = tokio::time::timeout(TEST_TIMEOUT, async {
        while let Some(line) = output.next_line().await.unwrap() {
            transcript.push(line.clone());
            if serde_json::from_str::<Value>(&line).is_ok_and(|m| m["id"] == 1) {
                return true;
            }
        }
        false
    })
    .await;
    let started = std::fs::read_to_string(directory.path().join("started.json"));
    let received = std::fs::read_to_string(directory.path().join("received.json"));
    assert_eq!(
        result,
        Ok(true),
        "started: {started:?}; received: {received:?}; stdout: {transcript:?}"
    );
    drop(input);
    assert!(
        tokio::time::timeout(TEST_TIMEOUT, child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
}

#[tokio::test]
async fn dropping_process_closes_client_and_finishes_owned_child() {
    let directory = tempfile::tempdir().unwrap();
    let process = CodexProcess::spawn(child_command(directory.path(), "normal"), TEST_TIMEOUT)
        .await
        .unwrap();
    let client = process.client();
    drop(process);
    assert_eq!(
        client.request("after-drop", json!({}), TEST_TIMEOUT).await,
        Err(Error::Closed)
    );
    tokio::time::timeout(TEST_TIMEOUT, async {
        while !directory.path().join("finished.json").exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn uncooperative_child_is_reaped_but_forced_stop_remains_an_error() {
    let directory = tempfile::tempdir().unwrap();
    let process = CodexProcess::spawn(child_command(directory.path(), "ignore-eof"), TEST_TIMEOUT)
        .await
        .unwrap();
    assert_eq!(process.shutdown().await, Err(Error::StopTimeout));
    assert!(directory.path().join("finished.json").exists());
}

#[tokio::test]
async fn slow_subscribers_receive_an_explicit_lag_error() {
    let mut peer = Peer::new();
    let mut events = peer.client.subscribe().unwrap();
    for index in 0..1200 {
        peer.send(json!({"method":"delta","params":{"index":index}}))
            .await;
    }
    let barrier = peer.request("barrier");
    let request = peer.receive().await;
    peer.send(json!({"id":request["id"],"result":{}})).await;
    barrier.await.unwrap().unwrap();
    assert!(matches!(events.recv().await,Err(broadcast::error::RecvError::Lagged(n)) if n>0));
}

#[tokio::test]
async fn closing_releases_handler_captures_including_the_client_itself() {
    let peer = Peer::new();
    let captured = peer.client.clone();
    peer.client.set_request_handler(Some(Arc::new(move |_| {
        let _client = captured.clone();
        Box::pin(async { Ok(json!({"decision":"decline"})) })
    })));
    let weak = Arc::downgrade(&peer.client.shared);
    drop(peer);
    tokio::time::timeout(TEST_TIMEOUT, async {
        while weak.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn request_ids_stay_in_codex_signed_integer_range() {
    let mut peer = Peer::new();
    peer.client.shared.state.lock().unwrap().next_id = i64::MAX as u64;
    let last = peer.request("last-id");
    assert_eq!(peer.receive().await["id"], i64::MAX);
    peer.send(json!({"id":i64::MAX,"result":"last"})).await;
    assert_eq!(last.await.unwrap(), Ok(json!("last")));
    assert_eq!(
        peer.client
            .request("overflow", json!({}), TEST_TIMEOUT)
            .await,
        Err(Error::Capacity)
    );
}
