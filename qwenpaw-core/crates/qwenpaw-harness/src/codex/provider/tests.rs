use super::*;
use serde_json::json;
use std::path::Path;
use tokio::process::Command;

const TIMEOUT: Duration = Duration::from_secs(3);

fn settings(directory: &Path, installed: bool, mode: &str) -> CodexProviderSettings {
    CodexProviderSettings {
        binary: Some(if installed {
            std::env::current_exe()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned()
        } else {
            directory.join("missing-cli").to_str().unwrap().to_owned()
        }),
        bundled: None,
        cwd: directory.to_owned(),
        home: directory.to_owned(),
        environment: HashMap::from([
            ("PATH".into(), "".into()),
            ("QWENPAW_HARNESS_TEST_MODE".into(), mode.into()),
        ]),
    }
}

fn fixture_command(config: &LaunchConfig) -> Result<Command, Error> {
    let binary = config.binary.as_ref().ok_or(Error::NotInstalled)?;
    std::fs::write(
        config.cwd.join("resolved-launch.json"),
        serde_json::to_vec(&binary.path).unwrap(),
    )?;
    let mode = config
        .base_environment
        .get(OsStr::new("QWENPAW_HARNESS_TEST_MODE"))
        .and_then(|value| value.to_str())
        .ok_or(Error::InvalidFrame)?;
    Ok(crate::codex::tests::child_command(&config.cwd, mode))
}

fn provider(settings: CodexProviderSettings) -> CodexProvider {
    let settings = Arc::new(settings);
    let source = settings.clone();
    let lifecycle = CodexLifecycle::with_launcher(
        settings.launch(),
        None,
        fixture_command,
        Some(Arc::new(move || source.resolve())),
    );
    let source = settings.clone();
    let runtime = CodexRuntimePool::with_factory(
        settings.launch(),
        Arc::new(move |mut launch| {
            launch.cwd = source.cwd.join("runtime");
            std::fs::create_dir_all(&launch.cwd).unwrap();
            let source = source.clone();
            CodexLifecycle::with_launcher(
                launch,
                None,
                fixture_command,
                Some(Arc::new(move || source.resolve())),
            )
        }),
    );
    CodexProvider {
        settings,
        lifecycle,
        mcp: McpDiscovery::new(),
        runtime,
    }
}

fn expected(resolution: Option<&BinaryResolution>, account: &Value, error: Option<&str>) -> Value {
    json!({
        "id":"codex","name":"Codex","available":true,"coming_soon":false,
        "installed":resolution.is_some(),"authenticated":!account.is_null(),"account":account,
        "runtime_path":resolution.map(|value| &value.path),
        "runtime_source":resolution.map(|value| &value.source),"error":error,
        "capabilities":catalog::codex().capabilities
    })
}

fn read(directory: &Path, name: &str) -> Value {
    serde_json::from_slice(&std::fs::read(directory.join(name)).unwrap()).unwrap()
}

fn account() -> Value {
    json!({"type":"chatgpt","email":"fixture@example.invalid","planType":"plus"})
}

#[test]
fn catalog_preserves_order_and_does_not_convert_declared_capabilities_to_runtime_state() {
    let items = catalog::catalog();
    assert_eq!(
        items
            .each_ref()
            .map(|item| (item.id, item.name, item.coming_soon)),
        [
            ("codex", "Codex", false),
            ("claude", "Claude Code", true),
            ("qoder", "Qoder", false)
        ]
    );
    assert!(!items[1].clone().status().available);
    assert!(items[0].capabilities.provider_mcp_discovery);
    assert!(!items[2].capabilities.provider_mcp_discovery);
}

#[tokio::test]
async fn missing_provider_has_full_status_and_no_fake_successful_operations() {
    let directory = tempfile::tempdir().unwrap();
    let provider = CodexProvider::new(settings(directory.path(), false, "normal"), None).unwrap();
    assert_eq!(
        serde_json::to_value(provider.status(TIMEOUT).await.unwrap()).unwrap(),
        expected(None, &Value::Null, Some(INSTALL_MESSAGE))
    );
    assert_eq!(
        provider.capability_unavailable_message().await,
        Ok(Some(INSTALL_MESSAGE))
    );
    assert_eq!(provider.models(TIMEOUT).await, Err(Error::NotInstalled));
    assert_eq!(
        provider.discover_mcp(directory.path(), TIMEOUT).await,
        Ok(vec![])
    );
    assert_eq!(
        provider.discover_skills(directory.path(), TIMEOUT).await,
        Err(Error::NotInstalled)
    );
    assert_eq!(
        provider.start_login(false, TIMEOUT).await,
        Err(Error::NotInstalled)
    );
    assert_eq!(provider.logout(TIMEOUT).await, Err(Error::NotInstalled));
    assert!(matches!(
        provider
            .prepare_runtime(
                "missing".to_owned(),
                RuntimeCapabilities::default(),
                TIMEOUT
            )
            .await,
        Err(Error::NotInstalled)
    ));
    assert!(!directory.path().join("started.json").exists());
    provider.shutdown().await.unwrap();
}

#[test]
fn invalid_host_context_fails_before_starting_an_async_owner() {
    let directory = tempfile::tempdir().unwrap();
    let mut invalid_cwd = settings(directory.path(), false, "normal");
    invalid_cwd.cwd = "relative".into();
    assert!(matches!(
        CodexProvider::new(invalid_cwd, None),
        Err(Error::Io(std::io::ErrorKind::InvalidInput))
    ));
    let mut invalid_home = settings(directory.path(), false, "normal");
    invalid_home.home = "relative".into();
    assert!(matches!(
        CodexProvider::new(invalid_home, None),
        Err(Error::Io(std::io::ErrorKind::InvalidInput))
    ));
}

#[tokio::test]
async fn provider_uses_owned_transport_for_full_status_models_login_and_logout() {
    let directory = tempfile::tempdir().unwrap();
    let settings = settings(directory.path(), true, "normal");
    let resolution = settings.resolve();
    let provider = provider(settings);
    assert_eq!(
        serde_json::to_value(provider.status(TIMEOUT).await.unwrap()).unwrap(),
        expected(resolution.as_ref(), &account(), None)
    );
    assert_eq!(provider.capability_unavailable_message().await, Ok(None));
    assert_eq!(
        serde_json::to_value(provider.models(TIMEOUT).await.unwrap()).unwrap(),
        json!([
            {"id":"first","name":"first","description":"","is_default":false,"reasoning_efforts":[],"default_reasoning_effort":null},
            {"id":"second","name":"second","description":"","is_default":false,"reasoning_efforts":[],"default_reasoning_effort":null}
        ])
    );
    assert_eq!(
        provider.start_login(false, TIMEOUT).await,
        Ok(json!({"type":"chatgpt","loginId":"fixture-login"}))
    );
    assert_eq!(
        provider.start_login(true, TIMEOUT).await,
        Ok(json!({"type":"chatgptDeviceCode","loginId":"fixture-login"}))
    );
    assert_eq!(provider.logout(TIMEOUT).await, Ok(()));
    provider.shutdown().await.unwrap();
    let messages = read(directory.path(), "finished.json");
    let methods: Vec<_> = messages
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value["method"].as_str().unwrap())
        .collect();
    assert_eq!(
        methods,
        [
            "initialize",
            "initialized",
            "account/read",
            "model/list",
            "model/list",
            "account/login/start",
            "account/login/start",
            "account/logout"
        ]
    );
}

#[tokio::test]
async fn unauthenticated_and_protocol_error_status_keep_installation_and_hide_private_data() {
    for (mode, error) in [
        ("provider-no-account", None),
        ("provider-error", Some("login required")),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let settings = settings(directory.path(), true, mode);
        let resolution = settings.resolve();
        let provider = provider(settings);
        assert_eq!(
            serde_json::to_value(provider.status(TIMEOUT).await.unwrap()).unwrap(),
            expected(resolution.as_ref(), &Value::Null, error)
        );
        provider.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn provider_skill_discovery_uses_request_workspace_and_restarts_after_stop() {
    let directory = tempfile::tempdir().unwrap();
    let provider = provider(settings(directory.path(), true, "normal"));
    let workspace = directory.path().join("requested workspace 技能");
    let expected = json!([{
        "name":"fixture-skill","description":"Fixture skill","provider_id":"codex",
        "source":"user","enabled":true,"read_only":true,"scope":"provider"
    }]);
    for _ in 0..2 {
        assert_eq!(
            serde_json::to_value(provider.discover_skills(&workspace, TIMEOUT).await.unwrap())
                .unwrap(),
            expected
        );
        provider.stop().await.unwrap();
        let messages = read(directory.path(), "finished.json");
        assert_eq!(messages.as_array().unwrap().len(), 3);
        assert_eq!(messages[2]["method"], "skills/list");
        assert_eq!(
            messages[2]["params"],
            json!({"cwds":[workspace],"forceReload":false})
        );
    }
    provider.shutdown().await.unwrap();
    assert_eq!(
        provider.discover_skills(&workspace, TIMEOUT).await,
        Err(Error::Closed)
    );
}

#[tokio::test]
async fn mcp_discovery_uses_a_separate_owned_process_without_starting_app_server() {
    fn launcher(
        binary: &BinaryResolution,
        cwd: &Path,
        environment: &HashMap<OsString, OsString>,
    ) -> Result<Command, Error> {
        mcp_discovery::command(binary, cwd, environment)?;
        Ok(mcp_discovery::tests::fixture_command(cwd, "success"))
    }
    let directory = tempfile::tempdir().unwrap();
    let workspace = tempfile::Builder::new()
        .prefix("provider MCP workspace ")
        .tempdir()
        .unwrap();
    let provider = provider(settings(directory.path(), true, "normal"));
    assert_eq!(
        serde_json::to_value(
            provider
                .discover_mcp_with(workspace.path(), TIMEOUT, launcher)
                .await
                .unwrap()
        )
        .unwrap(),
        mcp_discovery::tests::expected()
    );
    assert!(!directory.path().join("started.json").exists());
    assert!(workspace.path().join("mcp-started").exists());
    assert_eq!(
        provider.status(TIMEOUT).await.unwrap().account,
        Some(serde_json::from_value(account()).unwrap())
    );
    provider.shutdown().await.unwrap();
    assert_eq!(
        provider
            .discover_mcp_with(workspace.path(), TIMEOUT, launcher)
            .await,
        Err(Error::Closed)
    );
}

#[tokio::test]
async fn provider_shutdown_drains_mcp_and_app_server_without_blocking_account_calls() {
    fn launcher(
        binary: &BinaryResolution,
        cwd: &Path,
        environment: &HashMap<OsString, OsString>,
    ) -> Result<Command, Error> {
        mcp_discovery::command(binary, cwd, environment)?;
        Ok(mcp_discovery::tests::fixture_command(cwd, "wait"))
    }
    let directory = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let provider = Arc::new(provider(settings(directory.path(), true, "normal")));
    provider.status(TIMEOUT).await.unwrap();
    let projected = provider
        .prepare_runtime(
            "session".to_owned(),
            RuntimeCapabilities::default(),
            TIMEOUT,
        )
        .await
        .unwrap();
    let handle = provider.clone();
    let cwd = workspace.path().to_owned();
    let pending = tokio::spawn(async move {
        handle
            .discover_mcp_with(&cwd, Duration::from_secs(30), launcher)
            .await
    });
    mcp_discovery::tests::started(workspace.path()).await;
    assert!(provider.status(TIMEOUT).await.unwrap().authenticated);
    tokio::time::timeout(Duration::from_secs(10), provider.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pending.await.unwrap(), Err(Error::Closed));
    assert!(directory.path().join("finished.json").exists());
    assert!(directory.path().join("runtime/finished.json").exists());
    assert_eq!(
        projected
            .client
            .request("fixture/echo", json!({}), TIMEOUT)
            .await,
        Err(Error::Closed)
    );
    assert_eq!(provider.models(TIMEOUT).await, Err(Error::Closed));
}

#[tokio::test]
async fn provider_projected_pool_is_separate_from_control_and_stops_and_restarts_with_it() {
    let directory = tempfile::tempdir().unwrap();
    let provider = provider(settings(directory.path(), true, "normal"));
    let control = provider.lifecycle.start(TIMEOUT).await.unwrap();
    let first = provider
        .prepare_runtime(
            "session".to_owned(),
            RuntimeCapabilities::default(),
            TIMEOUT,
        )
        .await
        .unwrap();
    assert!(!Arc::ptr_eq(&control.shared, &first.client.shared));
    assert!(provider.status(TIMEOUT).await.unwrap().authenticated);
    provider
        .forget_runtime_session("session".to_owned())
        .await
        .unwrap();
    let again = provider
        .prepare_runtime(
            "session".to_owned(),
            RuntimeCapabilities::default(),
            TIMEOUT,
        )
        .await
        .unwrap();
    assert!(Arc::ptr_eq(&first.client.shared, &again.client.shared));
    provider.stop().await.unwrap();
    assert!(control.shared.stop.is_cancelled());
    assert!(first.client.shared.stop.is_cancelled());
    let restarted = provider
        .prepare_runtime(
            "session".to_owned(),
            RuntimeCapabilities::default(),
            TIMEOUT,
        )
        .await
        .unwrap();
    assert!(!Arc::ptr_eq(&first.client.shared, &restarted.client.shared));
    assert!(provider.status(TIMEOUT).await.unwrap().authenticated);
    provider.shutdown().await.unwrap();
    assert!(matches!(
        provider
            .prepare_runtime(
                "session".to_owned(),
                RuntimeCapabilities::default(),
                TIMEOUT
            )
            .await,
        Err(Error::Closed)
    ));
}

fn executable(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"discovery fixture; never executed").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
}

#[tokio::test]
async fn discovery_changes_metadata_but_only_changes_executable_after_stop() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let name = if cfg!(windows) { "codex.EXE" } else { "codex" };
    let first = root.join("first").join(name);
    let second = root.join("second").join(name);
    executable(&first);
    executable(&second);
    let mut settings = settings(directory.path(), false, "normal");
    settings.binary = None;
    settings.environment.insert(
        "PATH".into(),
        std::env::join_paths([first.parent().unwrap(), second.parent().unwrap()]).unwrap(),
    );
    let provider = provider(settings);
    assert_eq!(
        provider.status(TIMEOUT).await.unwrap().runtime_path,
        Some(first.to_str().unwrap().to_owned())
    );
    let old = provider.lifecycle.start(TIMEOUT).await.unwrap();
    std::fs::rename(&first, first.with_extension("retired")).unwrap();
    assert_eq!(
        provider.status(TIMEOUT).await.unwrap().runtime_path,
        Some(second.to_str().unwrap().to_owned())
    );
    assert_eq!(read(directory.path(), "resolved-launch.json"), json!(first));
    assert!(Arc::ptr_eq(
        &old.shared,
        &provider.lifecycle.start(TIMEOUT).await.unwrap().shared
    ));
    provider.stop().await.unwrap();
    provider.models(TIMEOUT).await.unwrap();
    assert_eq!(
        read(directory.path(), "resolved-launch.json"),
        json!(second)
    );
    assert!(!Arc::ptr_eq(
        &old.shared,
        &provider.lifecycle.start(TIMEOUT).await.unwrap().shared
    ));
    provider.shutdown().await.unwrap();
}

#[tokio::test]
async fn removal_and_restoration_are_detected_without_killing_an_existing_connection() {
    let directory = tempfile::tempdir().unwrap();
    let candidate = directory.path().join("candidate");
    executable(&candidate);
    let mut settings = settings(directory.path(), false, "normal");
    settings.binary = Some(candidate.to_str().unwrap().to_owned());
    let provider = provider(settings);
    provider.status(TIMEOUT).await.unwrap();
    std::fs::rename(&candidate, candidate.with_extension("retired")).unwrap();
    assert_eq!(
        serde_json::to_value(provider.status(TIMEOUT).await.unwrap()).unwrap(),
        expected(None, &Value::Null, Some(INSTALL_MESSAGE))
    );
    assert_eq!(
        provider.capability_unavailable_message().await,
        Ok(Some(INSTALL_MESSAGE))
    );
    assert_eq!(provider.models(TIMEOUT).await.unwrap().len(), 2);
    provider.stop().await.unwrap();
    assert_eq!(provider.models(TIMEOUT).await, Err(Error::NotInstalled));
    std::fs::rename(candidate.with_extension("retired"), &candidate).unwrap();
    assert_eq!(provider.models(TIMEOUT).await.unwrap().len(), 2);
    provider.shutdown().await.unwrap();
}

#[tokio::test]
async fn launch_io_failure_is_not_reported_as_missing_or_authenticated() {
    let directory = tempfile::tempdir().unwrap();
    let settings = Arc::new(settings(directory.path(), true, "normal"));
    let source = settings.clone();
    let lifecycle = CodexLifecycle::with_launcher(
        settings.launch(),
        None,
        |_| Err(Error::Io(std::io::ErrorKind::PermissionDenied)),
        Some(Arc::new(move || source.resolve())),
    );
    let source = settings.clone();
    let runtime =
        CodexRuntimePool::new(settings.launch(), None, Arc::new(move || source.resolve()));
    let provider = CodexProvider {
        settings,
        lifecycle,
        mcp: McpDiscovery::new(),
        runtime,
    };
    assert_eq!(
        provider.status(TIMEOUT).await,
        Err(Error::Io(std::io::ErrorKind::PermissionDenied))
    );
    provider.shutdown().await.unwrap();
}

#[tokio::test]
async fn independent_provider_owners_do_not_stop_each_others_control_calls() {
    let first_dir = tempfile::tempdir().unwrap();
    let second_dir = tempfile::tempdir().unwrap();
    let first = provider(settings(first_dir.path(), true, "normal"));
    let second = provider(settings(second_dir.path(), true, "provider-no-account"));
    assert!(first.status(TIMEOUT).await.unwrap().authenticated);
    assert!(!second.status(TIMEOUT).await.unwrap().authenticated);
    first.shutdown().await.unwrap();
    assert_eq!(second.models(TIMEOUT).await.unwrap().len(), 2);
    second.shutdown().await.unwrap();
}

#[tokio::test]
async fn provider_opens_persisted_sessions_without_changing_control_ownership() {
    use crate::codex::sessions::{SessionRequest, ThreadOptions};
    let directory = tempfile::tempdir().unwrap();
    let provider = provider(settings(directory.path(), true, "normal"));
    let state = directory.path().join("sessions");
    assert!(!state.exists());
    assert!(provider.status(TIMEOUT).await.unwrap().authenticated);
    assert!(!state.exists());
    let sessions = provider.open_sessions(state.clone()).await.unwrap();
    let prepared = sessions
        .prepare(
            SessionRequest {
                session_id: "session".to_owned(),
                capabilities: RuntimeCapabilities::default(),
                cwd: directory.path().to_owned(),
                options: ThreadOptions::default(),
            },
            TIMEOUT,
        )
        .await
        .unwrap();
    assert_eq!(prepared.thread_id, "fixture-thread-1");
    assert_eq!(
        read(&state, "codex_sessions.json"),
        json!({"session":"fixture-thread-1"})
    );
    sessions.shutdown().await.unwrap();
    assert!(provider.status(TIMEOUT).await.unwrap().authenticated);
    provider.shutdown().await.unwrap();
}

async fn reference(input: Value) -> Value {
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/harness_provider_reference.py");
    let output = tokio::time::timeout(
        Duration::from_secs(20),
        Command::new("python")
            .arg(script)
            .arg(input.to_string())
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
    serde_json::from_slice(&output.stdout).unwrap()
}

#[tokio::test]
#[ignore = "requires qwenpaw Python environment; compare full provider metadata/status"]
async fn full_catalog_and_provider_status_match_original_python_output() {
    assert_eq!(
        serde_json::to_value(catalog::catalog()).unwrap(),
        reference(json!({"operation":"catalog"})).await
    );
    for (installed, mode) in [
        (false, "normal"),
        (true, "normal"),
        (true, "provider-no-account"),
        (true, "provider-error"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let settings = settings(directory.path(), installed, mode);
        let resolution = settings.resolve();
        let provider = provider(settings);
        let actual = serde_json::to_value(provider.status(TIMEOUT).await.unwrap()).unwrap();
        let mut input = json!({"operation":"status","installed":installed,
            "runtime_path":resolution.as_ref().map(|value| &value.path),
            "runtime_source":resolution.as_ref().map(|value| &value.source),
            "response":{"account":if mode == "normal" { json!({"type":"chatgpt","email":"fixture@example.invalid","planType":"plus","privateToken":"not-public"}) } else {Value::Null}}});
        if mode == "provider-error" {
            input["error"] = json!("login required");
        }
        assert_eq!(
            actual,
            reference(input).await,
            "{mode}, installed={installed}"
        );
        provider.shutdown().await.unwrap();
    }
}
