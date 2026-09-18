//! Only isolated unit-test children are killed; never packaged Core or user data.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use pretty_assertions::assert_eq;
use qwenpaw_core::{Core, ModelConfig};

use super::*;

const CHILD_ROOT: &str = "QWENPAW_PUBLICATION_FILE_TEST_ROOT";
const CHILD_BOUNDARY: &str = "QWENPAW_PUBLICATION_FILE_TEST_BOUNDARY";
const CHILD_TEST: &str =
    "desktop_restore_files::publication::tests::process::publication_files_process_child";

fn core(root: &Path) -> Core {
    Core::persistent(
        ModelConfig {
            api_key: None,
            base_url: "http://127.0.0.1:1".into(),
            default_model: "fixture".into(),
        },
        &root.join("core.sqlite"),
    )
    .unwrap()
}

#[test]
fn publication_files_process_child() {
    let Some(root) = std::env::var_os(CHILD_ROOT) else {
        return;
    };
    let root = PathBuf::from(root);
    let boundary: usize = std::env::var(CHILD_BOUNDARY).unwrap().parse().unwrap();
    let core = core(&root);
    let installation = core.installation_id().unwrap();
    let transaction = Uuid::now_v7();
    let config = root.join("agent.json");
    let catalog = root.join("catalog.json");
    fs::write(&config, b"old config").unwrap();
    fs::write(&catalog, b"old catalog").unwrap();
    let mut files = AgentPublicationFiles::prepare(
        installation,
        transaction,
        "writer",
        &config,
        b"new config",
        &catalog,
        b"new catalog",
    )
    .unwrap();
    let digest = files.persist(&root.join("journal.json")).unwrap();
    core.prepare_agent_publication_with_journal(transaction, digest)
        .unwrap();
    let operations: Vec<_> = files
        .journal
        .entries
        .iter()
        .flat_map(|entry| {
            [
                (entry.target.path(), entry.recovery().join("original")),
                (entry.recovery().join("replacement"), entry.target.path()),
            ]
        })
        .collect();
    for (source, target) in operations.into_iter().take(boundary.min(4)) {
        move_file(&source, &target).unwrap();
    }
    if boundary >= 5 {
        core.commit_agent_publication(transaction, Some("new channels"))
            .unwrap();
    }
    if boundary == 6 {
        fs::remove_file(files.journal.entries[0].recovery().join("original")).unwrap();
        sync_directory(files.journal.entries[0].recovery()).unwrap();
    }
    write_new(&root.join("ready"), b"ready").unwrap();
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

#[test]
fn publication_files_killed_process_recovers_files_using_reopened_sqlite_decision_and_digest() {
    for boundary in 0..=6 {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CHILD_TEST, "--nocapture"])
            .env(CHILD_ROOT, &root)
            .env(CHILD_BOUNDARY, boundary.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        while !root.join("ready").exists() {
            if let Some(status) = child.try_wait().unwrap() {
                panic!("publication child exited before boundary {boundary}: {status}");
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("publication child did not reach boundary {boundary}");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        child.kill().unwrap();
        assert!(!child.wait().unwrap().success());
        let core = core(&root);
        let decision = core.read_agent_publication().unwrap().unwrap();
        let digest = core
            .agent_publication_journal_digest(decision.id)
            .unwrap()
            .unwrap();
        let files = AgentPublicationFiles::read(
            &root.join("journal.json"),
            digest,
            core.installation_id().unwrap(),
            decision.id,
            &root.join("catalog.json"),
        )
        .unwrap();
        let committed = boundary >= 5;
        assert_eq!(
            decision.state,
            if committed {
                AgentPublicationState::Committed
            } else {
                AgentPublicationState::Prepared
            }
        );
        if !committed {
            files.rollback().unwrap();
        }
        files.cleanup(decision.state).unwrap();
        files.cleanup(decision.state).unwrap();
        assert_eq!(
            fs::read(root.join("agent.json")).unwrap(),
            if committed {
                b"new config"
            } else {
                b"old config"
            }
        );
        assert_eq!(
            fs::read(root.join("catalog.json")).unwrap(),
            if committed {
                b"new catalog"
            } else {
                b"old catalog"
            }
        );
        assert_eq!(
            core.read_channel_config_data().unwrap(),
            committed.then(|| "new channels".into())
        );
        core.finish_agent_publication(decision.id, decision.state)
            .unwrap();
        assert_eq!(core.read_agent_publication().unwrap(), None);
    }
}
