use super::*;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::Value;
use tokio::process::Command;

use crate::capabilities::{CapabilityError, McpServerDefinition, SkillDefinition};
use crate::codex::discovery::BinaryResolution;

const TIMEOUT: Duration = Duration::from_secs(3);

pub(in crate::codex) struct Fixture {
    pub(in crate::codex) pool: CodexRuntimePool,
    pub(in crate::codex) directory: tempfile::TempDir,
    launches: Arc<Mutex<Vec<LaunchConfig>>>,
    owners: Arc<Mutex<Vec<CodexLifecycle>>>,
}

impl Fixture {
    pub(in crate::codex) fn new(mode: &str) -> Self {
        let directory = tempfile::Builder::new()
            .prefix("pool with spaces ")
            .tempdir()
            .unwrap();
        let launch = LaunchConfig {
            binary: Some(BinaryResolution {
                path: std::env::current_exe().unwrap(),
                source: "test-fixture".to_owned(),
            }),
            cwd: directory.path().to_owned(),
            base_environment: HashMap::from([
                ("QWENPAW_HARNESS_TEST_MODE".into(), mode.into()),
                ("FIXTURE_BASE_VALUE".into(), "base".into()),
                ("FIXTURE_PROJECTED_VALUE".into(), "base-shadowed".into()),
            ]),
            config_overrides: vec![],
            environment: HashMap::new(),
        };
        let launches = Arc::new(Mutex::new(Vec::new()));
        let owners = Arc::new(Mutex::new(Vec::new()));
        let configs = launches.clone();
        let lifecycles = owners.clone();
        let pool = CodexRuntimePool::with_factory(
            launch,
            Arc::new(move |mut config| {
                let mut configs = configs.lock().unwrap();
                config.cwd = config.cwd.join(configs.len().to_string());
                std::fs::create_dir_all(&config.cwd).unwrap();
                configs.push(config.clone());
                let lifecycle = CodexLifecycle::with_launcher(config, None, command, None);
                lifecycles.lock().unwrap().push(lifecycle.clone());
                lifecycle
            }),
        );
        Self {
            pool,
            directory,
            launches,
            owners,
        }
    }

    pub(in crate::codex) fn path(&self, index: usize) -> PathBuf {
        self.directory.path().join(index.to_string())
    }

    async fn prepare(&self, session: &str, value: &str) -> PreparedRuntime {
        self.pool
            .prepare(session.to_owned(), capabilities(value), TIMEOUT)
            .await
            .unwrap()
    }

    fn roots(&self, index: usize) -> Vec<Value> {
        let messages: Vec<Value> =
            serde_json::from_slice(&std::fs::read(self.path(index).join("received.json")).unwrap())
                .unwrap();
        messages
            .into_iter()
            .filter(|message| message["method"] == "skills/extraRoots/set")
            .map(|message| message["params"].clone())
            .collect()
    }
}

fn command(config: &LaunchConfig) -> Result<Command, Error> {
    let mode = config
        .base_environment
        .get(OsStr::new("QWENPAW_HARNESS_TEST_MODE"))
        .and_then(|value| value.to_str())
        .ok_or(Error::InvalidFrame)?;
    let mut command = crate::codex::tests::child_command(&config.cwd, mode);
    command
        .envs(&config.base_environment)
        .envs(&config.environment);
    Ok(command)
}

pub(in crate::codex) fn capabilities(value: &str) -> RuntimeCapabilities {
    let mut server = McpServerDefinition {
        name: "fixture".to_owned(),
        command: "never-executed-mcp".to_owned(),
        env: [("FIXTURE_PROJECTED_VALUE".to_owned(), value.to_owned())]
            .into_iter()
            .collect(),
        ..Default::default()
    };
    server.refresh_runtime_revision();
    RuntimeCapabilities {
        skills: vec![SkillDefinition {
            name: "skill".to_owned(),
            directory: PathBuf::from("skills with spaces"),
            revision: "revision-1".to_owned(),
            ..Default::default()
        }],
        mcp_servers: vec![server],
    }
}

fn same(left: &PreparedRuntime, right: &PreparedRuntime) -> bool {
    Arc::ptr_eq(&left.client.shared, &right.client.shared)
}

async fn wait_file(path: &Path) {
    tokio::time::timeout(TIMEOUT, async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn sessions_share_only_matching_fingerprints_and_forget_keeps_shared_process() {
    let fixture = Fixture::new("normal");
    let a = fixture.prepare("a", "one").await;
    assert!(same(&a, &fixture.prepare("a", "one").await));
    assert!(same(&a, &fixture.prepare("b", "one").await));
    let changed = fixture.prepare("a", "two").await;
    assert!(!same(&a, &changed));
    assert_ne!(a.fingerprint, changed.fingerprint);
    assert!(same(&a, &fixture.prepare("b", "one").await));
    assert!(same(&a, &fixture.prepare("a", "one").await));
    fixture.pool.forget_session("a".to_owned()).await.unwrap();
    assert!(same(&a, &fixture.prepare("a", "one").await));
    assert_eq!(
        fixture.roots(0),
        vec![json!({"extraRoots":["skills with spaces"]}); 4]
    );
    assert_eq!(
        fixture.roots(1),
        vec![json!({"extraRoots":["skills with spaces"]})]
    );
    assert_eq!(fixture.launches.lock().unwrap().len(), 2);
    fixture.pool.shutdown().await.unwrap();
    assert!(fixture.path(0).join("finished.json").exists());
    assert!(fixture.path(1).join("finished.json").exists());
}

#[tokio::test]
async fn projection_reaches_launch_and_resolved_credentials_isolate_after_revision_refresh() {
    let fixture = Fixture::new("normal");
    let first = fixture.prepare("a", "one").await;
    let mut renamed = capabilities("one");
    renamed.mcp_servers[0].display_name = "renamed".to_owned();
    assert!(same(
        &first,
        &fixture
            .pool
            .prepare("a".to_owned(), renamed, TIMEOUT)
            .await
            .unwrap()
    ));
    let second = fixture.prepare("a", "two").await;
    for (index, prepared, value) in [(0, &first, "one"), (1, &second, "two")] {
        let projection = project_runtime(&capabilities(value)).unwrap();
        {
            let configs = fixture.launches.lock().unwrap();
            assert_eq!(configs[index].config_overrides, projection.config_overrides);
            assert_eq!(
                configs[index].environment,
                projection
                    .environment
                    .into_iter()
                    .map(|(key, value)| (key.into(), value.into()))
                    .collect()
            );
        }
        assert_eq!(
            prepared
                .client
                .request("fixture/launch", json!({}), TIMEOUT)
                .await
                .unwrap(),
            json!({"cwd":fixture.path(index).canonicalize().unwrap(),"value":value,"base":"base"})
        );
    }
    fixture.pool.shutdown().await.unwrap();
}

#[tokio::test]
async fn concurrent_prepares_have_one_process_and_one_same_session_roots_request() {
    let fixture = Fixture::new("normal");
    let mut tasks = Vec::new();
    for _ in 0..12 {
        let pool = fixture.pool.clone();
        tasks.push(tokio::spawn(async move {
            pool.prepare("a".to_owned(), capabilities("one"), TIMEOUT)
                .await
                .unwrap()
        }));
    }
    let first = tasks.remove(0).await.unwrap();
    for task in tasks {
        assert!(same(&first, &task.await.unwrap()));
    }
    assert_eq!(fixture.launches.lock().unwrap().len(), 1);
    assert_eq!(
        fixture.roots(0),
        vec![json!({"extraRoots":["skills with spaces"]})]
    );
    fixture.pool.shutdown().await.unwrap();
}

#[tokio::test]
async fn roots_failure_does_not_replace_previous_session_binding_and_retry_reuses_owner() {
    let fixture = Fixture::new("normal");
    let first = fixture.prepare("a", "one").await;
    let other = fixture.prepare("b", "two").await;
    std::fs::write(fixture.path(1).join("reject-roots"), b"reject").unwrap();
    assert!(matches!(
        fixture
            .pool
            .prepare("a".to_owned(), capabilities("two"), TIMEOUT)
            .await,
        Err(Error::Protocol { code: -32002, .. })
    ));
    assert!(same(&first, &fixture.prepare("a", "one").await));
    assert_eq!(fixture.roots(0).len(), 1);
    std::fs::rename(
        fixture.path(1).join("reject-roots"),
        fixture.path(1).join("rejected-roots"),
    )
    .unwrap();
    assert!(same(&other, &fixture.prepare("a", "two").await));
    assert_eq!(
        fixture.roots(1),
        vec![json!({"extraRoots":["skills with spaces"]}); 3]
    );
    fixture.pool.shutdown().await.unwrap();
}

#[tokio::test]
async fn timeout_does_not_bind_and_later_retry_resets_roots() {
    let fixture = Fixture::new("normal");
    fixture.prepare("a", "one").await;
    std::fs::write(fixture.path(0).join("timeout-roots"), b"wait").unwrap();
    assert!(matches!(
        fixture
            .pool
            .prepare(
                "b".to_owned(),
                capabilities("one"),
                Duration::from_millis(30)
            )
            .await,
        Err(Error::Timeout)
    ));
    std::fs::rename(
        fixture.path(0).join("timeout-roots"),
        fixture.path(0).join("timed-out-roots"),
    )
    .unwrap();
    fixture.prepare("b", "one").await;
    assert_eq!(fixture.roots(0).len(), 3);
    fixture.pool.shutdown().await.unwrap();
}

#[tokio::test]
async fn restarted_generation_restores_roots_for_the_same_session() {
    let fixture = Fixture::new("normal");
    let old = fixture.prepare("a", "one").await;
    let _ = old.client.request("fixture/exit", json!({}), TIMEOUT).await;
    tokio::time::timeout(TIMEOUT, old.client.shared.stop.cancelled())
        .await
        .unwrap();
    let new = fixture.prepare("a", "one").await;
    assert!(!same(&old, &new));
    assert_eq!(old.fingerprint, new.fingerprint);
    assert_eq!(
        fixture.roots(0),
        vec![json!({"extraRoots":["skills with spaces"]})]
    );
    assert_eq!(fixture.launches.lock().unwrap().len(), 1);
    fixture.pool.shutdown().await.unwrap();
}

#[tokio::test]
async fn cancelled_prepare_waiter_does_not_lose_ownership_or_duplicate_initialization() {
    let fixture = Fixture::new("gated-roots");
    let pool = fixture.pool.clone();
    let task = tokio::spawn(async move {
        pool.prepare("a".to_owned(), capabilities("one"), TIMEOUT)
            .await
    });
    wait_file(&fixture.path(0).join("roots-seen")).await;
    task.abort();
    assert!(matches!(task.await, Err(error) if error.is_cancelled()));
    std::fs::write(fixture.path(0).join("release-roots"), b"release").unwrap();
    fixture.prepare("a", "one").await;
    assert_eq!(fixture.roots(0).len(), 1);
    fixture.pool.shutdown().await.unwrap();
    assert!(fixture.path(0).join("finished.json").exists());
}

#[tokio::test]
async fn clean_stop_clears_bindings_and_shutdown_closes_clones_and_old_endpoints() {
    let fixture = Fixture::new("normal");
    let old = fixture.prepare("a", "one").await;
    fixture.prepare("b", "two").await;
    fixture.pool.stop().await.unwrap();
    assert!(old.client.shared.stop.is_cancelled());
    assert!(fixture.path(0).join("finished.json").exists());
    assert!(fixture.path(1).join("finished.json").exists());
    let new = fixture.prepare("a", "one").await;
    assert!(!same(&old, &new));
    assert_eq!(fixture.roots(2).len(), 1);
    let clone = fixture.pool.clone();
    fixture.pool.shutdown().await.unwrap();
    assert!(new.client.shared.stop.is_cancelled());
    assert_eq!(
        clone.forget_session("a".to_owned()).await,
        Err(Error::Closed)
    );
    assert!(matches!(
        clone
            .prepare("a".to_owned(), capabilities("one"), TIMEOUT)
            .await,
        Err(Error::Closed)
    ));
    assert_eq!(clone.stop().await, Err(Error::Closed));
}

#[tokio::test]
async fn cleanup_error_still_drains_other_owners_and_is_latched() {
    let fixture = Fixture::new("normal");
    fixture.prepare("a", "one").await;
    let second = fixture.prepare("b", "two").await;
    // Inject an already-closed lifecycle; real OS cleanup failure is not claimed.
    let owner = fixture.owners.lock().unwrap()[0].clone();
    owner.shutdown().await.unwrap();
    assert_eq!(fixture.pool.stop().await, Err(Error::Closed));
    assert!(second.client.shared.stop.is_cancelled());
    assert!(fixture.path(1).join("finished.json").exists());
    assert!(matches!(
        fixture
            .pool
            .prepare("c".to_owned(), capabilities("three"), TIMEOUT)
            .await,
        Err(Error::Closed)
    ));
    assert_eq!(fixture.launches.lock().unwrap().len(), 2);
    assert_eq!(fixture.pool.shutdown().await, Err(Error::Closed));
}

#[tokio::test]
async fn invalid_projection_fails_before_launch_and_drop_requests_cleanup() {
    let fixture = Fixture::new("normal");
    let mut invalid = capabilities("one");
    invalid
        .mcp_servers
        .push(capabilities("two").mcp_servers.remove(0));
    assert!(
        matches!(fixture.pool.prepare("a".to_owned(), invalid, TIMEOUT).await,
        Err(Error::Capability(CapabilityError::EnvironmentConflict(name))) if name == "FIXTURE_PROJECTED_VALUE")
    );
    assert!(fixture.launches.lock().unwrap().is_empty());
    let prepared = fixture.prepare("a", "one").await;
    let Fixture {
        pool, directory, ..
    } = fixture;
    drop(pool);
    wait_file(&directory.path().join("0").join("finished.json")).await;
    assert!(prepared.client.shared.stop.is_cancelled());
}

#[tokio::test]
async fn cancelled_shutdown_waiter_still_drains_and_closes_every_clone() {
    let fixture = Fixture::new("gated-eof");
    let prepared = fixture.prepare("a", "one").await;
    let pool = fixture.pool.clone();
    let task = tokio::spawn(async move { pool.shutdown().await });
    wait_file(&fixture.path(0).join("eof-seen")).await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(
        fixture.pool.forget_session("a".to_owned()).await,
        Err(Error::Closed)
    );
    std::fs::write(fixture.path(0).join("release-stop"), b"release").unwrap();
    wait_file(&fixture.path(0).join("finished.json")).await;
    assert!(prepared.client.shared.stop.is_cancelled());
}

#[tokio::test]
async fn empty_capabilities_set_empty_roots_and_separate_pools_never_share_clients() {
    let left = Fixture::new("normal");
    let right = Fixture::new("normal");
    let a = left
        .pool
        .prepare("same".to_owned(), RuntimeCapabilities::default(), TIMEOUT)
        .await
        .unwrap();
    let b = right
        .pool
        .prepare("same".to_owned(), RuntimeCapabilities::default(), TIMEOUT)
        .await
        .unwrap();
    assert_eq!(a.fingerprint, b.fingerprint);
    assert!(!same(&a, &b));
    assert_eq!(left.roots(0), vec![json!({"extraRoots":[]})]);
    left.pool.shutdown().await.unwrap();
    assert_eq!(
        b.client
            .request("fixture/echo", json!({"alive":true}), TIMEOUT)
            .await,
        Ok(json!({"alive":true}))
    );
    right.pool.shutdown().await.unwrap();
}
