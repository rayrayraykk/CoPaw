//! Host process death with credentials owned by a separate, memory-only service.

use super::*;
use pretty_assertions::assert_eq;

#[path = "desktop_publication_process_credential_service.rs"]
mod service;
use service::{AFTER, BEFORE, Client, ENDPOINT, Service};

const CHILD_TEST: &str =
    "desktop_agents::publication::process_tests::credentials::publication_credential_process_child";
const FAIL_COMMIT: &str = "QWENPAW_PUBLICATION_TEST_FAIL_COMMIT";

#[tokio::test]
async fn publication_credential_process_child() {
    let Some(root) = std::env::var_os(ROOT) else {
        return;
    };
    let root = PathBuf::from(root);
    let server = server_with_credentials(&root, Arc::new(Client::from_env()));
    if std::env::var_os(FAIL_COMMIT).is_some() {
        rusqlite::Connection::open(root.join("core.sqlite")).unwrap()
            .execute_batch("CREATE TRIGGER fail_publication_commit BEFORE UPDATE OF state ON agent_publication WHEN NEW.state = 'committed' BEGIN SELECT RAISE(ABORT, 'fixture commit failure'); END;").unwrap();
    }
    let mut submitted = profile(&server, "GET", Value::Null).await;
    submitted["name"] = json!("Credential publication");
    submitted["channels"]["console"]["bot_prefix"] = json!("credential-fixture");
    submitted["mail"]["credential"]["auth_code"] = json!(AFTER);
    profile(&server, "PUT", submitted).await;
    panic!("credential child did not pause at its requested boundary");
}

#[tokio::test]
async fn publication_credential_process_death_at_thirteen_forward_boundaries_preserves_full_state()
{
    for previous in [None, Some(""), Some(BEFORE)] {
        for (boundary, committed, live_written) in [
            ("staging", false, false),
            ("private-write-before-return", false, false),
            ("secret-prepared", false, false),
            ("publishing", false, false),
            ("live-write-before-return", false, true),
            ("secret-published", false, true),
            ("files-published", false, true),
            ("committed", true, true),
            ("cleaning", true, true),
            ("files-cleaned", true, true),
            ("private-delete-before-return", true, true),
            ("secrets-cleaned", true, true),
            ("finished", true, true),
        ] {
            run_case(previous, boundary, false, committed, live_written).await;
        }
    }
}

#[tokio::test]
async fn publication_credential_process_death_at_nine_inverse_boundaries_never_repeats_completed_writes()
 {
    for previous in [None, Some(""), Some(BEFORE)] {
        for boundary in [
            "rollback-start",
            "files-rolled-back",
            "inverse-write-before-return",
            "secret-rolled-back",
            "cleaning",
            "files-cleaned",
            "private-delete-before-return",
            "secrets-cleaned",
            "finished",
        ] {
            run_case(previous, boundary, true, false, true).await;
        }
    }
}

fn kill_at(root: &Path, service: &Service, boundary: &str, fail_commit: bool) {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", CHILD_TEST, "--nocapture"])
        .env(ROOT, root)
        .env(BOUNDARY, boundary)
        .env(ENDPOINT, service.endpoint())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(
            fs::File::create(root.join("child-stderr.log")).unwrap(),
        ));
    if fail_commit {
        command.env(FAIL_COMMIT, "1");
    }
    let mut child = IsolatedChild(command.spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(20);
    while !root.join("ready").exists() {
        if let Some(status) = child.0.try_wait().unwrap() {
            panic!(
                "credential child exited before {boundary}: {status}\n{}",
                fs::read_to_string(root.join("child-stderr.log")).unwrap()
            );
        }
        assert!(
            Instant::now() < deadline,
            "credential child missed {boundary}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(fs::read_to_string(root.join("ready")).unwrap(), boundary);
    child.0.kill().unwrap();
    assert!(!child.0.wait().unwrap().success());
}

async fn run_case(
    previous: Option<&str>,
    boundary: &str,
    fail_commit: bool,
    committed: bool,
    live_written: bool,
) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    fs::write(
        root.join("index.html"),
        "<!doctype html><title>fixture</title>",
    )
    .unwrap();
    fs::create_dir(root.join("workspace")).unwrap();
    let credentials = Service::new(previous);
    let initial = server_with_credentials(&root, credentials.client());
    let mut expected = profile(&initial, "GET", Value::Null).await;
    let before = files_and_channels(&root, &initial);
    drop(initial);
    kill_at(&root, &credentials, boundary, fail_commit);
    assert_no_plaintext_credentials(&root);
    let before_recovery = credentials.state.lock().unwrap().clone();
    let inverse_written =
        fail_commit && !["rollback-start", "files-rolled-back"].contains(&boundary);
    let expected_before = if live_written && !inverse_written {
        Some(AFTER)
    } else {
        previous
    };
    assert_eq!(before_recovery.live.as_deref(), expected_before);
    let private_exists = ![
        "staging",
        "private-delete-before-return",
        "secrets-cleaned",
        "finished",
    ]
    .contains(&boundary);
    assert_eq!(
        before_recovery.recovery.is_some(),
        private_exists,
        "{boundary}"
    );
    if let Some((_, secret)) = &before_recovery.recovery {
        assert_eq!(secret.previous(), previous);
        assert_eq!(secret.replacement(), Some(AFTER));
    }
    let recovered = server_with_credentials(&root, credentials.client());
    if committed {
        expected["name"] = json!("Credential publication");
        expected["channels"]["console"]["bot_prefix"] = json!("credential-fixture");
        expected["mail"] = json!({"credential":{}});
    } else {
        assert_eq!(files_and_channels(&root, &recovered), before, "{boundary}");
    }
    assert_eq!(
        profile(&recovered, "GET", Value::Null).await,
        expected,
        "{boundary}"
    );
    assert_eq!(recovered.inner.core.read_agent_publication().unwrap(), None);
    let expected_live = if committed { Some(AFTER) } else { previous };
    let mut writes = Vec::new();
    if live_written {
        writes.push(Some(AFTER.to_owned()));
        if !committed {
            writes.push(previous.map(str::to_owned));
        }
    }
    let after = credentials.state.lock().unwrap().clone();
    assert_eq!(after.live.as_deref(), expected_live);
    assert_eq!(after.writes, writes, "{boundary}");
    assert_eq!(after.recovery, None);
    let stable = files_and_channels(&root, &recovered);
    drop(recovered);
    let reopened = server_with_credentials(&root, credentials.client());
    assert_eq!(profile(&reopened, "GET", Value::Null).await, expected);
    assert_eq!(files_and_channels(&root, &reopened), stable);
    let after = credentials.state.lock().unwrap().clone();
    assert_eq!(after.live.as_deref(), expected_live);
    assert_eq!(after.writes, writes);
    assert_eq!(after.recovery, None);
    assert_no_plaintext_credentials(&root);
}

fn files_and_channels(
    root: &Path,
    server: &AppServer,
) -> (Option<Vec<u8>>, Vec<u8>, Option<String>) {
    (
        fs::read(root.join("workspace/agent.json")).ok(),
        fs::read(root.join("data/agents/catalog.json")).unwrap(),
        server.inner.core.read_channel_config_data().unwrap(),
    )
}

fn assert_no_plaintext_credentials(root: &Path) {
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        assert!(!kind.is_symlink(), "fixture must not traverse links");
        if kind.is_dir() {
            assert_no_plaintext_credentials(&entry.path());
        } else if kind.is_file() {
            let bytes = fs::read(entry.path()).unwrap();
            for value in [BEFORE, AFTER] {
                assert!(
                    !bytes
                        .windows(value.len())
                        .any(|window| window == value.as_bytes()),
                    "plaintext fixture credential found in {}",
                    entry.path().display()
                );
            }
        }
    }
}
