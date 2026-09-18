use super::*;
use crate::codex::runtime::tests::{Fixture, capabilities};
use serde_json::Value;
use std::path::Path;

const TIMEOUT: Duration = Duration::from_secs(3);

#[tokio::test]
#[ignore = "requires qwenpaw Python environment; compare original session methods"]
async fn session_sequences_match_original_python_requests_and_mappings() {
    let cases = [
        json!({"operations":[{"session":"a"},{"session":"a"}]}),
        json!({"operations":[{"session":"a","settings":{"sandbox":"read-only","approval_policy":"never","model":"fixture-model"}},{"session":"b"}]}),
        json!({"threads":{"a":"old"},"operations":[{"session":"a"},{"session":"a"}]}),
        json!({"threads":{"a":"old"},"reject_resume":true,"operations":[{"session":"a"}]}),
        json!({"operations":[{"session":"a"},{"session":"a","reset":true},{"session":"a"}]}),
        json!({"operations":[{"session":"a","settings":{"sandbox":"","approval_policy":"","model":""}}]}),
    ];
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/codex_session_reference.py");
    for mut case in cases {
        let fixture = Fixture::new("normal");
        let state = fixture.directory.path().join("state");
        std::fs::create_dir(&state).unwrap();
        std::fs::write(
            state.join("codex_sessions.json"),
            case.get("threads").unwrap_or(&json!({})).to_string(),
        )
        .unwrap();
        let sessions = open(&fixture).await;
        if case["reject_resume"] == true {
            fixture
                .pool
                .prepare("a".to_owned(), capabilities("one"), TIMEOUT)
                .await
                .unwrap();
            std::fs::write(fixture.path(0).join("reject-resume"), b"reject").unwrap();
        }
        let mut results = Vec::new();
        for operation in case["operations"].as_array().unwrap() {
            let session = operation["session"].as_str().unwrap();
            if operation["reset"] == true {
                sessions.reset_session(session.to_owned()).await.unwrap();
                results.push(Value::Null);
            } else {
                let mut request = request(session, "one");
                request.options = ThreadOptions {
                    sandbox: operation["settings"]["sandbox"].as_str().map(str::to_owned),
                    approval_policy: operation["settings"]["approval_policy"]
                        .as_str()
                        .map(str::to_owned),
                    model: operation["settings"]["model"].as_str().map(str::to_owned),
                };
                results.push(json!(
                    sessions.prepare(request, TIMEOUT).await.unwrap().thread_id
                ));
            }
        }
        case["cwd"] = json!("workspace with spaces");
        let output = tokio::time::timeout(
            Duration::from_secs(20),
            tokio::process::Command::new("python")
                .arg(&script)
                .arg(case.to_string())
                .kill_on_drop(true)
                .output(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            json!({"results":results,"requests":messages(&fixture,0),"threads":saved(&fixture)}),
            expected
        );
        sessions.shutdown().await.unwrap();
        fixture.pool.shutdown().await.unwrap();
    }
}

fn request(session: &str, value: &str) -> SessionRequest {
    SessionRequest {
        session_id: session.to_owned(),
        capabilities: capabilities(value),
        cwd: PathBuf::from("workspace with spaces"),
        options: ThreadOptions::default(),
    }
}

async fn open(fixture: &Fixture) -> CodexSessions {
    CodexSessions::open(fixture.directory.path().join("state"), fixture.pool.clone())
        .await
        .unwrap()
}

fn saved(fixture: &Fixture) -> Value {
    serde_json::from_slice(
        &std::fs::read(fixture.directory.path().join("state/codex_sessions.json")).unwrap(),
    )
    .unwrap()
}

fn messages(fixture: &Fixture, index: usize) -> Vec<Value> {
    let messages: Vec<Value> =
        serde_json::from_slice(&std::fs::read(fixture.path(index).join("received.json")).unwrap())
            .unwrap();
    messages
        .into_iter()
        .filter(|m| {
            m["method"]
                .as_str()
                .is_some_and(|s| s.starts_with("thread/"))
        })
        .map(|m| json!({"method":m["method"],"params":m["params"]}))
        .collect()
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

#[tokio::test]
async fn creates_once_with_original_defaults_and_persists_full_mapping() {
    let fixture = Fixture::new("normal");
    let sessions = open(&fixture).await;
    let first = sessions
        .prepare(request("a", "one"), TIMEOUT)
        .await
        .unwrap();
    let second = sessions
        .prepare(request("a", "one"), TIMEOUT)
        .await
        .unwrap();
    assert_eq!(first.thread_id, second.thread_id);
    assert_eq!(saved(&fixture), json!({"a":"fixture-thread-1"}));
    assert_eq!(
        messages(&fixture, 0),
        vec![json!({"method":"thread/start","params":{
        "cwd":"workspace with spaces","sandbox":"workspace-write","approvalPolicy":"on-request"}})]
    );
    sessions.shutdown().await.unwrap();
    fixture.pool.shutdown().await.unwrap();
}

#[tokio::test]
async fn concurrent_sessions_preserve_all_bindings_and_explicit_options() {
    let fixture = Fixture::new("normal");
    let sessions = open(&fixture).await;
    let mut custom = request("a", "one");
    custom.options = ThreadOptions {
        sandbox: Some("read-only".to_owned()),
        approval_policy: Some("never".to_owned()),
        model: Some("fixture-model".to_owned()),
    };
    sessions.prepare(custom, TIMEOUT).await.unwrap();
    let mut tasks = Vec::new();
    for _ in 0..12 {
        let sessions = sessions.clone();
        tasks.push(tokio::spawn(async move {
            sessions
                .prepare(request("b", "one"), TIMEOUT)
                .await
                .unwrap()
                .thread_id
        }));
    }
    for task in tasks {
        assert_eq!(task.await.unwrap(), "fixture-thread-2");
    }
    assert_eq!(
        saved(&fixture),
        json!({"a":"fixture-thread-1","b":"fixture-thread-2"})
    );
    assert_eq!(
        messages(&fixture, 0),
        vec![
            json!({"method":"thread/start","params":{"cwd":"workspace with spaces","sandbox":"read-only","approvalPolicy":"never","model":"fixture-model"}}),
            json!({"method":"thread/start","params":{"cwd":"workspace with spaces","sandbox":"workspace-write","approvalPolicy":"on-request"}})
        ]
    );
    sessions.shutdown().await.unwrap();
    fixture.pool.shutdown().await.unwrap();
}

#[tokio::test]
async fn reopen_resumes_persisted_id_and_config_switch_resumes_per_generation() {
    let fixture = Fixture::new("normal");
    let sessions = open(&fixture).await;
    sessions
        .prepare(request("a", "one"), TIMEOUT)
        .await
        .unwrap();
    sessions.shutdown().await.unwrap();
    let reopened = open(&fixture).await;
    reopened
        .prepare(request("a", "one"), TIMEOUT)
        .await
        .unwrap();
    reopened
        .prepare(request("a", "two"), TIMEOUT)
        .await
        .unwrap();
    reopened
        .prepare(request("a", "one"), TIMEOUT)
        .await
        .unwrap();
    for index in [1, 2] {
        assert_eq!(
            messages(&fixture, index),
            vec![json!({"method":"thread/resume","params":{"threadId":"fixture-thread-1"}})]
        );
    }
    assert_eq!(saved(&fixture), json!({"a":"fixture-thread-1"}));
    reopened.shutdown().await.unwrap();
    fixture.pool.shutdown().await.unwrap();
}

#[tokio::test]
async fn process_exit_resets_loaded_marker_without_changing_fingerprint() {
    let fixture = Fixture::new("normal");
    let sessions = open(&fixture).await;
    let old = sessions
        .prepare(request("a", "one"), TIMEOUT)
        .await
        .unwrap();
    let _ = old
        .runtime
        .client
        .request("fixture/exit", json!({}), TIMEOUT)
        .await;
    tokio::time::timeout(TIMEOUT, old.runtime.client.shared.stop.cancelled())
        .await
        .unwrap();
    let new = sessions
        .prepare(request("a", "one"), TIMEOUT)
        .await
        .unwrap();
    assert_eq!(old.thread_id, new.thread_id);
    assert_eq!(
        messages(&fixture, 0),
        vec![json!({"method":"thread/resume","params":{"threadId":"fixture-thread-1"}})]
    );
    sessions.shutdown().await.unwrap();
    fixture.pool.shutdown().await.unwrap();
}

#[tokio::test]
async fn remote_resume_rejection_creates_new_but_timeout_does_not() {
    let fixture = Fixture::new("normal");
    let state = fixture.directory.path().join("state");
    std::fs::create_dir(&state).unwrap();
    std::fs::write(state.join("codex_sessions.json"), br#"{"a":"old"}"#).unwrap();
    fixture
        .pool
        .prepare("a".to_owned(), capabilities("one"), TIMEOUT)
        .await
        .unwrap();
    std::fs::write(fixture.path(0).join("timeout-resume"), b"wait").unwrap();
    let sessions = open(&fixture).await;
    assert!(matches!(
        sessions
            .prepare(request("a", "one"), Duration::from_millis(30))
            .await,
        Err(Error::Timeout)
    ));
    assert_eq!(saved(&fixture), json!({"a":"old"}));
    std::fs::rename(
        fixture.path(0).join("timeout-resume"),
        fixture.path(0).join("timed-out-resume"),
    )
    .unwrap();
    std::fs::write(fixture.path(0).join("reject-resume"), b"reject").unwrap();
    assert_eq!(
        sessions
            .prepare(request("a", "one"), TIMEOUT)
            .await
            .unwrap()
            .thread_id,
        "fixture-thread-1"
    );
    assert_eq!(saved(&fixture), json!({"a":"fixture-thread-1"}));
    assert_eq!(
        messages(&fixture, 0)
            .iter()
            .map(|m| m["method"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["thread/resume", "thread/resume", "thread/start"]
    );
    sessions.shutdown().await.unwrap();
    fixture.pool.shutdown().await.unwrap();
}

#[tokio::test]
async fn reset_persists_removal_and_stop_keeps_other_sessions_for_resume() {
    let fixture = Fixture::new("normal");
    let sessions = open(&fixture).await;
    sessions
        .prepare(request("a", "one"), TIMEOUT)
        .await
        .unwrap();
    sessions
        .prepare(request("b", "one"), TIMEOUT)
        .await
        .unwrap();
    sessions.reset_session("a".to_owned()).await.unwrap();
    assert_eq!(saved(&fixture), json!({"b":"fixture-thread-2"}));
    assert_eq!(
        sessions
            .prepare(request("a", "one"), TIMEOUT)
            .await
            .unwrap()
            .thread_id,
        "fixture-thread-3"
    );
    sessions.stop().await.unwrap();
    assert_eq!(
        saved(&fixture),
        json!({"a":"fixture-thread-3","b":"fixture-thread-2"})
    );
    sessions
        .prepare(request("b", "one"), TIMEOUT)
        .await
        .unwrap();
    assert_eq!(
        messages(&fixture, 1),
        vec![json!({"method":"thread/resume","params":{"threadId":"fixture-thread-2"}})]
    );
    sessions.shutdown().await.unwrap();
    assert_eq!(
        sessions.reset_session("b".to_owned()).await,
        Err(Error::Closed)
    );
    fixture.pool.shutdown().await.unwrap();
}

#[tokio::test]
async fn failed_publication_retries_acknowledged_thread_without_duplicate_start() {
    let fixture = Fixture::new("normal");
    let sessions = open(&fixture).await;
    let path = fixture.directory.path().join("state/codex_sessions.json");
    std::fs::create_dir(&path).unwrap();
    assert!(matches!(
        sessions.prepare(request("a", "one"), TIMEOUT).await,
        Err(Error::InvalidSessionState)
    ));
    std::fs::rename(&path, path.with_extension("blocked-directory")).unwrap();
    assert_eq!(
        sessions
            .prepare(request("a", "one"), TIMEOUT)
            .await
            .unwrap()
            .thread_id,
        "fixture-thread-1"
    );
    assert_eq!(messages(&fixture, 0).len(), 1);
    assert_eq!(saved(&fixture), json!({"a":"fixture-thread-1"}));
    std::fs::rename(&path, path.with_extension("saved-json")).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert_eq!(
        sessions.reset_session("a".to_owned()).await,
        Err(Error::InvalidSessionState)
    );
    std::fs::rename(&path, path.with_extension("blocked-reset")).unwrap();
    std::fs::rename(path.with_extension("saved-json"), &path).unwrap();
    assert_eq!(
        sessions
            .prepare(request("a", "one"), TIMEOUT)
            .await
            .unwrap()
            .thread_id,
        "fixture-thread-1"
    );
    assert_eq!(saved(&fixture), json!({"a":"fixture-thread-1"}));
    sessions.shutdown().await.unwrap();
    fixture.pool.shutdown().await.unwrap();
}

#[tokio::test]
async fn cancelled_prepare_waiter_still_persists_and_shutdown_drains() {
    let fixture = Fixture::new("gated-thread");
    let sessions = open(&fixture).await;
    let handle = sessions.clone();
    let task = tokio::spawn(async move { handle.prepare(request("a", "one"), TIMEOUT).await });
    wait(&fixture.path(0).join("thread-seen")).await;
    task.abort();
    assert!(matches!(task.await, Err(error) if error.is_cancelled()));
    std::fs::write(fixture.path(0).join("release-thread"), b"release").unwrap();
    sessions.shutdown().await.unwrap();
    assert_eq!(saved(&fixture), json!({"a":"fixture-thread-1"}));
    assert!(fixture.path(0).join("finished.json").exists());
    fixture.pool.shutdown().await.unwrap();
}

#[tokio::test]
async fn missing_id_does_not_publish_and_drop_requests_pool_cleanup() {
    let fixture = Fixture::new("normal");
    let sessions = open(&fixture).await;
    fixture
        .pool
        .prepare("a".to_owned(), capabilities("one"), TIMEOUT)
        .await
        .unwrap();
    std::fs::write(fixture.path(0).join("missing-thread-id"), b"missing").unwrap();
    assert!(matches!(
        sessions.prepare(request("a", "one"), TIMEOUT).await,
        Err(Error::MissingThreadId)
    ));
    assert!(
        !fixture
            .directory
            .path()
            .join("state/codex_sessions.json")
            .exists()
    );
    drop(sessions);
    wait(&fixture.path(0).join("finished.json")).await;
    fixture.pool.shutdown().await.unwrap();
}

#[test]
fn store_handles_original_empty_state_rules_and_preserves_private_complete_json() {
    let directory = tempfile::tempdir().unwrap();
    let (path, empty) = store::open(directory.path()).unwrap();
    assert!(empty.is_empty());
    for bytes in [
        b"invalid".as_slice(),
        b"[]",
        br#"{"":"ignored","null":null,"empty":""}"#,
    ] {
        std::fs::write(&path, bytes).unwrap();
        assert!(store::open(directory.path()).unwrap().1.is_empty());
    }
    let data = BTreeMap::from([("会话".to_owned(), "thread".to_owned())]);
    store::write(&path, &data).unwrap();
    assert_eq!(store::open(directory.path()).unwrap().1, data);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    std::fs::write(&path, br#"{"a":true}"#).unwrap();
    assert!(matches!(
        store::open(directory.path()),
        Err(Error::InvalidSessionState)
    ));
    assert!(matches!(
        store::open(Path::new("relative")),
        Err(Error::Io(std::io::ErrorKind::InvalidInput))
    ));
}

#[cfg(unix)]
#[test]
fn store_rejects_symlink_and_preserves_new_private_and_existing_file_modes() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("codex_sessions.json");
    let outside = directory.path().join("outside.json");
    std::fs::write(&outside, b"{}").unwrap();
    symlink(&outside, &path).unwrap();
    assert!(matches!(
        store::open(directory.path()),
        Err(Error::InvalidSessionState)
    ));
    assert_eq!(
        store::write(&path, &BTreeMap::new()),
        Err(Error::InvalidSessionState)
    );
    std::fs::rename(&path, path.with_extension("symlink")).unwrap();
    store::write(&path, &BTreeMap::new()).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
    store::write(&path, &BTreeMap::new()).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert_eq!(std::fs::read(&outside).unwrap(), b"{}");
}
