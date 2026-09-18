use super::*;
use serde_json::json;

const TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) fn fixture_command(directory: &Path, mode: &str) -> Command {
    // Tests require the documented qwenpaw conda environment, not a product SDK.
    let output = std::process::Command::new("python")
        .args(["-I", "-c", "import sys; print(sys.executable)"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "qwenpaw Python test environment required"
    );
    let executable = String::from_utf8(output.stdout).unwrap();
    let mut command = Command::new(executable.trim());
    command
        .arg("-I")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/mcp_fixture.py"))
        .current_dir(directory)
        .env_clear()
        .env("QWENPAW_MCP_FIXTURE_MODE", mode);
    for name in ["SystemRoot", "WINDIR", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
}

pub(crate) async fn started(directory: &Path) {
    tokio::time::timeout(TIMEOUT, async {
        while !directory.join("mcp-started").exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

pub(crate) fn expected() -> Value {
    json!([{"name":"fixture","provider_id":"codex","transport":"stdio","enabled":true,
        "auth_status":"supported","read_only":true,"scope":"provider"}])
}

#[test]
fn original_command_is_structured_and_environment_is_explicit() {
    let directory = tempfile::tempdir().unwrap();
    let binary = BinaryResolution {
        path: directory.path().join("runtime with spaces"),
        source: "fixture".to_owned(),
    };
    let environment = HashMap::from([(
        OsString::from("EXAMPLE"),
        OsString::from("literal ; $value"),
    )]);
    let command = command(&binary, directory.path(), &environment).unwrap();
    let command = command.as_std();
    assert_eq!(command.get_program(), binary.path);
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        ["mcp", "list", "--json"]
    );
    assert_eq!(command.get_current_dir(), Some(directory.path()));
    assert_eq!(
        command.get_envs().collect::<Vec<_>>(),
        vec![(
            std::ffi::OsStr::new("EXAMPLE"),
            Some(std::ffi::OsStr::new("literal ; $value"))
        )]
    );
    assert!(matches!(
        super::command(&binary, Path::new("relative"), &environment),
        Err(Error::Io(std::io::ErrorKind::InvalidInput))
    ));
}

#[test]
fn projection_preserves_order_duplicates_defaults_and_original_error_messages() {
    let payload = json!([null, 9, {}, {"name":""},
        {"name":"same","transport":{"type":"http","url":"private"},"enabled":true,"auth_status":"oauth","token":"hidden"},
        {"name":"same"}, {"name":true,"transport":null,"enabled":null,"auth_status":2}]);
    assert_eq!(
        serde_json::to_value(project(payload.to_string().as_bytes(), b"ignored", true).unwrap())
            .unwrap(),
        json!([
            {"name":"same","provider_id":"codex","transport":"http","enabled":true,"auth_status":"oauth","read_only":true,"scope":"provider"},
            {"name":"same","provider_id":"codex","transport":"","enabled":false,"auth_status":"","read_only":true,"scope":"provider"},
            {"name":"True","provider_id":"codex","transport":"","enabled":false,"auth_status":"2","read_only":true,"scope":"provider"}
        ])
    );
    for payload in [b"null".as_slice(), b"{}", b"17", b"\"text\""] {
        assert_eq!(project(payload, b"", true), Ok(vec![]));
    }
    assert_eq!(
        project(b"invalid", b"", true),
        Err(Error::McpDiscovery(
            "Codex returned invalid MCP discovery data".to_owned()
        ))
    );
    assert_eq!(
        project(b"ignored", b" denied \xff \n", false),
        Err(Error::McpDiscovery(
            "Failed to discover Codex MCP servers: denied �".to_owned()
        ))
    );
    assert_eq!(
        project(
            br#"[{"name":"good"},{"name":"bad","transport":4}]"#,
            b"",
            true
        ),
        Err(Error::InvalidFrame)
    );
}

#[tokio::test]
async fn isolated_child_success_errors_and_both_pipes_are_collected() {
    let directory = tempfile::Builder::new()
        .prefix("mcp workspace ")
        .tempdir()
        .unwrap();
    let owner = McpDiscovery::new();
    for (mode, expected) in [
        ("success", Ok(expected())),
        ("both", Ok(json!([]))),
        (
            "invalid",
            Err(Error::McpDiscovery(
                "Codex returned invalid MCP discovery data".to_owned(),
            )),
        ),
        (
            "error",
            Err(Error::McpDiscovery(
                "Failed to discover Codex MCP servers: denied �".to_owned(),
            )),
        ),
    ] {
        assert_eq!(
            owner
                .discover(fixture_command(directory.path(), mode), TIMEOUT)
                .await
                .map(|value| serde_json::to_value(value).unwrap()),
            expected
        );
    }
    owner.stop(true).await.unwrap();
}

#[tokio::test]
async fn timeout_and_output_limits_reap_before_reply_and_allow_later_requests() {
    let directory = tempfile::tempdir().unwrap();
    let owner = McpDiscovery::new();
    for (mode, timeout, error) in [
        ("wait", Duration::from_millis(100), Error::Timeout),
        ("stdout-limit", TIMEOUT, Error::McpOutputLimit),
        ("stderr-limit", TIMEOUT, Error::McpOutputLimit),
    ] {
        let command = fixture_command(directory.path(), mode);
        assert_eq!(
            tokio::time::timeout(TIMEOUT, owner.discover(command, timeout))
                .await
                .unwrap(),
            Err(error)
        );
    }
    assert_eq!(
        serde_json::to_value(
            owner
                .discover(fixture_command(directory.path(), "success"), TIMEOUT)
                .await
                .unwrap()
        )
        .unwrap(),
        expected()
    );
    owner.stop(true).await.unwrap();
}

#[tokio::test]
async fn caller_cancellation_stop_and_terminal_shutdown_drain_owned_jobs() {
    let directory = tempfile::tempdir().unwrap();
    let owner = McpDiscovery::new();
    let command = fixture_command(directory.path(), "wait");
    let handle = owner.clone();
    let pending =
        tokio::spawn(async move { handle.discover(command, Duration::from_secs(30)).await });
    started(directory.path()).await;
    pending.abort();
    assert!(pending.await.unwrap_err().is_cancelled());
    tokio::time::timeout(TIMEOUT, owner.stop(false))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::to_value(
            owner
                .discover(fixture_command(directory.path(), "success"), TIMEOUT)
                .await
                .unwrap()
        )
        .unwrap(),
        expected()
    );
    owner.stop(true).await.unwrap();
    assert_eq!(
        owner
            .discover(fixture_command(directory.path(), "success"), TIMEOUT)
            .await,
        Err(Error::Closed)
    );
}

#[tokio::test]
async fn concurrent_jobs_are_bounded_and_stop_cancels_every_accepted_request() {
    let owner = McpDiscovery::new();
    let mut directories = Vec::new();
    let mut tasks = Vec::new();
    for _ in 0..MAX_PROCESSES {
        let directory = tempfile::tempdir().unwrap();
        let command = fixture_command(directory.path(), "wait");
        let handle = owner.clone();
        tasks.push(tokio::spawn(async move {
            handle.discover(command, Duration::from_secs(30)).await
        }));
        started(directory.path()).await;
        directories.push(directory);
    }
    assert_eq!(
        owner
            .discover(fixture_command(directories[0].path(), "success"), TIMEOUT)
            .await,
        Err(Error::Capacity)
    );
    tokio::time::timeout(TIMEOUT, owner.stop(false))
        .await
        .unwrap()
        .unwrap();
    for task in tasks {
        assert_eq!(task.await.unwrap(), Err(Error::Closed));
    }
    owner.stop(true).await.unwrap();
}

#[tokio::test]
async fn dropped_owner_cancels_active_jobs_and_reports_joined_completion() {
    let directory = tempfile::tempdir().unwrap();
    let owner = McpDiscovery::new();
    let (reply, receive) = oneshot::channel();
    owner
        .sender
        .send(Operation::Run(Box::new(Request {
            command: fixture_command(directory.path(), "wait"),
            timeout: Duration::from_secs(30),
            reply,
        })))
        .await
        .unwrap();
    started(directory.path()).await;
    drop(owner);
    assert_eq!(
        tokio::time::timeout(TIMEOUT, receive)
            .await
            .unwrap()
            .unwrap(),
        Err(Error::Closed)
    );
}

#[tokio::test]
async fn cleanup_faults_are_latched_and_spawn_failures_are_not() {
    let mut state = Owner::default();
    let (reply, receive) = oneshot::channel();
    let fault = Error::McpCleanup(Box::new(Error::StopTimeout));
    state.finish(Ok((reply, Err(fault.clone()))));
    assert_eq!(receive.await.unwrap(), Err(fault.clone()));
    assert_eq!(state.stop().await, Err(fault.clone()));
    let (reply, receive) = oneshot::channel();
    state.start(Request {
        command: Command::new("never-executed"),
        timeout: TIMEOUT,
        reply,
    });
    assert_eq!(receive.await.unwrap(), Err(fault));

    let directory = tempfile::tempdir().unwrap();
    let owner = McpDiscovery::new();
    assert_eq!(
        owner
            .discover(Command::new(directory.path().join("missing")), TIMEOUT)
            .await,
        Err(Error::Io(std::io::ErrorKind::NotFound))
    );
    owner.stop(true).await.unwrap();
}

#[tokio::test]
async fn closing_only_the_reply_cancels_and_reaps_without_owner_stop() {
    let directory = tempfile::tempdir().unwrap();
    let command = fixture_command(directory.path(), "wait");
    let (mut reply, receive) = oneshot::channel();
    let task = tokio::spawn(async move {
        execute(
            command,
            Duration::from_secs(30),
            &CancellationToken::new(),
            &mut reply,
        )
        .await
    });
    started(directory.path()).await;
    drop(receive);
    assert_eq!(
        tokio::time::timeout(TIMEOUT, task).await.unwrap().unwrap(),
        Err(Error::Closed)
    );
}

#[tokio::test]
async fn terminal_shutdown_cancels_in_flight_process_and_closes_all_handles() {
    let directory = tempfile::tempdir().unwrap();
    let owner = McpDiscovery::new();
    let handle = owner.clone();
    let command = fixture_command(directory.path(), "wait");
    let task = tokio::spawn(async move { handle.discover(command, Duration::from_secs(30)).await });
    started(directory.path()).await;
    tokio::time::timeout(TIMEOUT, owner.stop(true))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(task.await.unwrap(), Err(Error::Closed));
    assert_eq!(owner.stop(false).await, Err(Error::Closed));
}

#[tokio::test]
#[ignore = "requires qwenpaw Python environment; compare original MCP subprocess discovery"]
async fn mcp_discovery_matches_original_python_method_and_command() {
    let directory = tempfile::tempdir().unwrap();
    let binary = BinaryResolution {
        path: directory.path().join("Codex runtime"),
        source: "fixture".to_owned(),
    };
    let cwd = directory.path().join("workspace with spaces 技能");
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/codex_control_reference.py");
    for (installed, stdout, stderr, code) in [
        (false, b"[]".as_slice(), b"".as_slice(), 0),
        (true, b"[]", b"", 0),
        (true, b"null", b"", 0),
        (true, b"{}", b"", 0),
        (true, br#"[null,{}, {"name":"same","transport":{"type":"http","url":"hidden"},"enabled":true,"auth_status":"oauth"},{"name":"same"},{"name":true,"enabled":null,"auth_status":2}]"#, b"", 0),
        (true, b"invalid", b"", 0),
        (true, b"ignored", b" denied \xff \n", 7),
    ] {
        let fixture = json!({"operation":"mcp","responses":[],"binary":installed.then_some(&binary.path),
            "cwd":cwd,"stdout":stdout,"stderr":stderr,"returncode":code});
        let result = if installed {project(stdout,stderr,code==0)} else {Ok(vec![])};
        let calls = if installed {
            let command = super::command(&binary, &cwd, &HashMap::new()).unwrap();
            vec![json!({"program":command.as_std().get_program().to_str().unwrap(),
                "args":command.as_std().get_args().map(|value| value.to_str().unwrap()).collect::<Vec<_>>(),"cwd":command.as_std().get_current_dir(),
                "stdout":-1,"stderr":-1})]
        } else {vec![]};
        let actual = match result {
            Ok(result) => json!({"result":result,"requests":calls}),
            Err(error) => json!({"error":error.to_string(),"requests":calls})
        };
        let output = tokio::time::timeout(Duration::from_secs(20),
            Command::new("python").arg(&script).arg(fixture.to_string()).kill_on_drop(true).output())
            .await.unwrap().unwrap();
        assert!(output.status.success(), "reference: {}",String::from_utf8_lossy(&output.stderr));
        assert_eq!(actual,serde_json::from_slice::<Value>(&output.stdout).unwrap());
    }
}
