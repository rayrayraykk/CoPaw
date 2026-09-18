use super::*;
use std::path::Path;
use std::sync::Arc;

use serde_json::{Value, json};

const TIMEOUT: Duration = Duration::from_secs(3);

fn config(directory: &Path, mode: &str) -> LaunchConfig {
    LaunchConfig {
        binary: Some(BinaryResolution {
            path: std::env::current_exe().unwrap(),
            source: "test-fixture".to_owned(),
        }),
        cwd: directory.to_owned(),
        base_environment: HashMap::new(),
        config_overrides: vec![],
        environment: HashMap::from([("QWENPAW_HARNESS_TEST_MODE".into(), mode.into())]),
    }
}

fn fixture_command(config: &LaunchConfig) -> Result<Command, Error> {
    // Only the executable/argv is a test substitute. The owner, pipes,
    // handshake, notifications and stop/reap behavior are the production code.
    let mode = config
        .environment
        .get(&OsString::from("QWENPAW_HARNESS_TEST_MODE"))
        .and_then(|mode| mode.to_str())
        .ok_or(Error::InvalidFrame)?;
    if let Some(previous) = config
        .environment
        .get(&OsString::from("FIXTURE_PREVIOUS_FINISHED"))
        && !Path::new(previous).exists()
    {
        return Err(Error::InvalidFrame);
    }
    let mut command = crate::codex::tests::child_command(&config.cwd, mode);
    command
        .envs(&config.base_environment)
        .envs(&config.environment);
    Ok(command)
}

fn owner(config: LaunchConfig) -> CodexLifecycle {
    CodexLifecycle::with_launcher(config, None, fixture_command, None)
}

async fn wait_file(directory: &Path, name: &str) {
    tokio::time::timeout(TIMEOUT, async {
        while !directory.join(name).exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

fn read(directory: &Path, name: &str) -> Value {
    serde_json::from_slice(&std::fs::read(directory.join(name)).unwrap()).unwrap()
}

#[test]
fn launch_uses_original_argument_order_and_explicit_environment_overlay() {
    let directory = tempfile::tempdir().unwrap();
    let mut launch = config(directory.path(), "normal");
    launch.base_environment = HashMap::from([
        ("UNCHANGED".into(), "base".into()),
        ("SAME".into(), "base".into()),
    ]);
    launch.environment = HashMap::from([
        ("SAME".into(), "projected".into()),
        ("EMPTY".into(), "".into()),
    ]);
    launch.config_overrides = vec![
        "name=\"value with spaces\"".to_owned(),
        "literal=$(not-a-shell)".to_owned(),
    ];
    let command = launch.command().unwrap();
    let command = command.as_std();
    assert_eq!(command.get_program(), launch.binary.as_ref().unwrap().path);
    assert_eq!(command.get_current_dir(), Some(directory.path()));
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        [
            "app-server",
            "-c",
            "name=\"value with spaces\"",
            "-c",
            "literal=$(not-a-shell)",
            "--listen",
            "stdio://"
        ]
    );
    let env: std::collections::BTreeMap<_, _> = command
        .get_envs()
        .map(|(name, value)| (name.to_str().unwrap(), value.unwrap().to_str().unwrap()))
        .collect();
    assert_eq!(
        env,
        std::collections::BTreeMap::from([
            ("EMPTY", ""),
            ("SAME", "projected"),
            ("UNCHANGED", "base")
        ])
    );
    launch.cwd = "relative".into();
    assert!(matches!(
        launch.command(),
        Err(Error::Io(std::io::ErrorKind::InvalidInput))
    ));
}

#[tokio::test]
async fn concurrent_start_is_one_generation_and_shutdown_closes_every_clone() {
    let directory = tempfile::tempdir().unwrap();
    let lifecycle = owner(config(directory.path(), "normal"));
    let mut tasks = Vec::new();
    for _ in 0..12 {
        let lifecycle = lifecycle.clone();
        tasks.push(tokio::spawn(async move {
            lifecycle.start(TIMEOUT).await.unwrap()
        }));
    }
    let first = tasks.remove(0).await.unwrap();
    for task in tasks {
        assert!(Arc::ptr_eq(&first.shared, &task.await.unwrap().shared));
    }
    assert_eq!(
        first
            .request("fixture/echo", json!({"alive":true}), TIMEOUT)
            .await,
        Ok(json!({"alive":true}))
    );
    lifecycle.shutdown().await.unwrap();
    assert!(directory.path().join("finished.json").exists());
    assert!(matches!(
        lifecycle.clone().start(TIMEOUT).await,
        Err(Error::Closed)
    ));
    assert_eq!(
        first.request("after-shutdown", json!({}), TIMEOUT).await,
        Err(Error::Closed)
    );
    let messages = read(directory.path(), "finished.json");
    assert_eq!(
        messages
            .as_array()
            .unwrap()
            .iter()
            .filter(|message| message["method"] == "initialize")
            .count(),
        1
    );
}

#[tokio::test]
async fn unchanged_configuration_keeps_client_and_changed_configuration_drains_old() {
    let old = tempfile::tempdir().unwrap();
    let next = tempfile::tempdir().unwrap();
    let original = config(old.path(), "normal");
    let lifecycle = owner(original.clone());
    let client = lifecycle.start(TIMEOUT).await.unwrap();
    assert_eq!(lifecycle.configure(original).await, Ok(false));
    assert!(Arc::ptr_eq(
        &client.shared,
        &lifecycle.start(TIMEOUT).await.unwrap().shared
    ));
    let mut events = client.subscribe().unwrap();
    let pending_client = client.clone();
    let pending = tokio::spawn(async move {
        pending_client
            .request("fixture/wait", json!({}), TIMEOUT)
            .await
    });
    assert_eq!(
        events.recv().await.unwrap(),
        json!({"method":"fixture/received","params":{}})
    );
    assert_eq!(
        lifecycle.configure(config(next.path(), "normal")).await,
        Ok(true)
    );
    assert_eq!(pending.await.unwrap(), Err(Error::Closed));
    assert!(old.path().join("finished.json").exists());
    assert!(!next.path().join("started.json").exists());
    let replacement = lifecycle.start(TIMEOUT).await.unwrap();
    assert!(!Arc::ptr_eq(&client.shared, &replacement.shared));
    assert!(matches!(
        events.recv().await,
        Err(tokio::sync::broadcast::error::RecvError::Closed)
    ));
    lifecycle.shutdown().await.unwrap();
}

#[tokio::test]
async fn cancelling_accepted_start_does_not_create_a_second_process() {
    let directory = tempfile::tempdir().unwrap();
    let lifecycle = owner(config(directory.path(), "gated-handshake"));
    let caller = lifecycle.clone();
    let start = tokio::spawn(async move { caller.start(TIMEOUT).await });
    wait_file(directory.path(), "initialize-seen").await;
    let pid = read(directory.path(), "started.json")["pid"].clone();
    start.abort();
    assert!(matches!(start.await, Err(error) if error.is_cancelled()));
    std::fs::write(directory.path().join("release-start"), b"continue").unwrap();
    let client = lifecycle.start(TIMEOUT).await.unwrap();
    assert_eq!(read(directory.path(), "started.json")["pid"], pid);
    assert_eq!(
        client.request("fixture/echo", json!({}), TIMEOUT).await,
        Ok(json!({}))
    );
    lifecycle.shutdown().await.unwrap();
}

#[tokio::test]
async fn cancelling_reconfigure_cannot_start_replacement_before_old_exit() {
    let old = tempfile::tempdir().unwrap();
    let next = tempfile::tempdir().unwrap();
    let lifecycle = owner(config(old.path(), "gated-eof"));
    lifecycle.start(TIMEOUT).await.unwrap();
    let caller = lifecycle.clone();
    let mut replacement = config(next.path(), "normal");
    replacement.environment.insert(
        "FIXTURE_PREVIOUS_FINISHED".into(),
        old.path().join("finished.json").into(),
    );
    let configure = tokio::spawn(async move { caller.configure(replacement).await });
    wait_file(old.path(), "eof-seen").await;
    configure.abort();
    assert!(configure.await.unwrap_err().is_cancelled());
    let caller = lifecycle.clone();
    let start = tokio::spawn(async move { caller.start(TIMEOUT).await });
    assert!(!old.path().join("finished.json").exists());
    assert!(!next.path().join("started.json").exists());
    assert!(!start.is_finished());
    std::fs::write(old.path().join("release-stop"), b"continue").unwrap();
    let client = start.await.unwrap().unwrap();
    assert!(old.path().join("finished.json").exists());
    assert_eq!(
        client.request("fixture/echo", json!({}), TIMEOUT).await,
        Ok(json!({}))
    );
    lifecycle.shutdown().await.unwrap();
}

#[tokio::test]
async fn clean_stop_and_observed_process_exit_allow_new_generations() {
    let directory = tempfile::tempdir().unwrap();
    let lifecycle = owner(config(directory.path(), "normal"));
    let first = lifecycle.start(TIMEOUT).await.unwrap();
    lifecycle.stop().await.unwrap();
    let second = lifecycle.start(TIMEOUT).await.unwrap();
    assert!(!Arc::ptr_eq(&first.shared, &second.shared));
    assert!(matches!(
        second.request("fixture/exit", json!({}), TIMEOUT).await,
        Err(Error::Closed | Error::ProcessExit(Some(7)))
    ));
    let third = lifecycle.start(TIMEOUT).await.unwrap();
    assert!(!Arc::ptr_eq(&second.shared, &third.shared));
    assert_eq!(
        third.request("fixture/echo", json!({}), TIMEOUT).await,
        Ok(json!({}))
    );
    lifecycle.shutdown().await.unwrap();
}

#[tokio::test]
async fn early_approval_has_handler_and_terminal_shutdown_releases_it() {
    let directory = tempfile::tempdir().unwrap();
    let marker = Arc::new(());
    let weak = Arc::downgrade(&marker);
    let handler: RequestHandler = Arc::new(move |request| {
        let _marker = marker.clone();
        assert_eq!(
            request,
            json!({"id":"startup-approval","method":"item/commandExecution/requestApproval","params":{}})
        );
        Box::pin(async { Ok(json!({"decision":"accept"})) })
    });
    let lifecycle = CodexLifecycle::with_launcher(
        config(directory.path(), "early-approval"),
        Some(handler),
        fixture_command,
        None,
    );
    lifecycle.start(TIMEOUT).await.unwrap();
    wait_file(directory.path(), "approval.json").await;
    assert_eq!(
        read(directory.path(), "approval.json"),
        json!({"id":"startup-approval","result":{"decision":"accept"}})
    );
    lifecycle.shutdown().await.unwrap();
    assert!(weak.upgrade().is_none());
}

#[tokio::test]
async fn forced_stop_fault_is_latched_and_never_launches_replacement() {
    let old = tempfile::tempdir().unwrap();
    let next = tempfile::tempdir().unwrap();
    let lifecycle = owner(config(old.path(), "ignore-eof"));
    lifecycle.start(TIMEOUT).await.unwrap();
    assert_eq!(
        lifecycle.configure(config(next.path(), "normal")).await,
        Err(Error::StopTimeout)
    );
    assert!(matches!(
        lifecycle.start(TIMEOUT).await,
        Err(Error::StopTimeout)
    ));
    assert!(!next.path().join("started.json").exists());
    assert_eq!(lifecycle.shutdown().await, Err(Error::StopTimeout));
}

#[tokio::test]
async fn dropping_last_owner_handle_cleans_up_without_explicit_stop() {
    let directory = tempfile::tempdir().unwrap();
    let lifecycle = owner(config(directory.path(), "normal"));
    let client = lifecycle.start(TIMEOUT).await.unwrap();
    drop(lifecycle);
    wait_file(directory.path(), "finished.json").await;
    assert_eq!(
        client.request("closed", json!({}), TIMEOUT).await,
        Err(Error::Closed)
    );
}

#[tokio::test]
async fn handshake_cleanup_failure_is_reported_and_prevents_retry() {
    let directory = tempfile::tempdir().unwrap();
    let lifecycle = owner(config(directory.path(), "handshake-timeout-ignore-eof"));
    let expected = Error::StartupCleanup(Box::new(Error::StopTimeout));
    assert!(matches!(lifecycle.start(TIMEOUT).await, Err(error) if error == expected));
    assert!(matches!(lifecycle.start(TIMEOUT).await, Err(error) if error == expected));
    assert_eq!(lifecycle.shutdown().await, Err(expected));
}

#[tokio::test]
async fn ordinary_handshake_failure_is_cleaned_up_and_can_be_reconfigured() {
    let bad = tempfile::tempdir().unwrap();
    let good = tempfile::tempdir().unwrap();
    let lifecycle = owner(config(bad.path(), "handshake-error"));
    assert!(matches!(
        lifecycle.start(TIMEOUT).await,
        Err(Error::Protocol { code: -32600, .. })
    ));
    assert!(bad.path().join("finished.json").exists());
    assert_eq!(
        lifecycle.configure(config(good.path(), "normal")).await,
        Ok(false)
    );
    let client = lifecycle.start(TIMEOUT).await.unwrap();
    assert_eq!(
        client.request("fixture/echo", json!({}), TIMEOUT).await,
        Ok(json!({}))
    );
    lifecycle.shutdown().await.unwrap();
}

#[tokio::test]
async fn cancelling_terminal_shutdown_keeps_admission_closed_while_cleanup_finishes() {
    let directory = tempfile::tempdir().unwrap();
    let lifecycle = owner(config(directory.path(), "gated-eof"));
    let client = lifecycle.start(TIMEOUT).await.unwrap();
    let caller = lifecycle.clone();
    let shutdown = tokio::spawn(async move { caller.shutdown().await });
    wait_file(directory.path(), "eof-seen").await;
    shutdown.abort();
    assert!(shutdown.await.unwrap_err().is_cancelled());
    assert!(matches!(lifecycle.start(TIMEOUT).await, Err(Error::Closed)));
    assert!(!directory.path().join("finished.json").exists());
    std::fs::write(directory.path().join("release-stop"), b"continue").unwrap();
    wait_file(directory.path(), "finished.json").await;
    assert_eq!(
        client.request("closed", json!({}), TIMEOUT).await,
        Err(Error::Closed)
    );
    assert_eq!(lifecycle.shutdown().await, Err(Error::Closed));
}
