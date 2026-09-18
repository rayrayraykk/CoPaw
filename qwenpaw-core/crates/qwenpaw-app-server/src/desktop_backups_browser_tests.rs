use pretty_assertions::assert_eq;

use super::*;

async fn roundtrip_output(
    command: &mut tokio::process::Command,
    directory: &Path,
) -> (bool, std::process::Output) {
    let stdout = directory.join("browser-stdout.log");
    let stderr = directory.join("browser-stderr.log");
    command
        .stdout(fs::File::create(&stdout).unwrap())
        .stderr(fs::File::create(&stderr).unwrap());
    eprintln!("Browser diagnostic logs: {}", directory.display());
    let mut child = command.spawn().expect("Node could not start");
    let result = tokio::time::timeout(Duration::from_secs(180), child.wait()).await;
    let timed_out = result.is_err();
    let status = if let Ok(status) = result {
        status.unwrap()
    } else {
        child.kill().await.unwrap();
        child.wait().await.unwrap()
    };
    (
        timed_out,
        std::process::Output {
            status,
            stdout: fs::read(stdout).unwrap(),
            stderr: fs::read(stderr).unwrap(),
        },
    )
}

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_backups_browser_active_reload_and_cancel() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let console = root.join("../console/dist").canonicalize().unwrap();
    let mut fixture = Fixture::new();
    fixture.server.inner.core.write_ui_language("en").unwrap();
    fixture.server = AppServer::new_desktop_with_stores_and_workspace(
        fixture.server.inner.core.clone(),
        &console,
        String::from("backup-test-shutdown-token"),
        fixture.credentials.clone(),
        &fixture.data,
        &fixture.workspace,
    )
    .unwrap();
    let (base, server) = fixture.start_http().await;
    let checkpoints = fixture.server.inner.desktop_checkpoint_lock.lock().await;
    let release_on_cancel = async {
        let state = backup_state(&fixture.server).unwrap();
        let cancellation = loop {
            let token = {
                let coordinator = state.coordinator.lock().await;
                coordinator
                    .active_job
                    .as_ref()
                    .map(|id| coordinator.jobs.get(id).unwrap().cancellation.clone())
            };
            if let Some(token) = token {
                break token;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        cancellation.cancelled().await;
        drop(checkpoints);
    };
    let mut command = tokio::process::Command::new("node");
    command
        .arg(root.join("scripts/console_browser_smoke.mjs"))
        .args([&base, "/backups", "--backups-jobs"])
        .kill_on_drop(true);
    let output = command.output();
    tokio::pin!(output);
    let result = tokio::time::timeout(Duration::from_secs(90), async {
        tokio::select! {
            result = &mut output => result,
            () = release_on_cancel => output.await,
        }
    })
    .await;
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(10), server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let output = result.unwrap().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let report: Value =
        serde_json::from_str(&stdout).unwrap_or_else(|_| panic!("{stdout}\n{stderr}"));
    assert!(output.status.success(), "{report:#}\n{stderr}");
    assert_eq!(report["ok"], true);
    assert_eq!(report["pages"].as_array().unwrap().len(), 1);
    let expected = json!({"activeReload": true, "sseReconnected": true,
        "cancelled": true, "noPartialArchive": true, "subsequentCreation": true});
    assert_eq!(report["pages"][0]["backupsJobs"], expected);
    println!("{expected:#}");
    assert_eq!(
        fs::read_dir(fixture.data.join("backups")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_to_string(fixture.workspace.join("notes.md")).unwrap(),
        "original workspace content"
    );
}

/// Explicit browser gate: requires a built original Console, Chrome and Node.
/// Only temporary data and the in-memory credential store are used.
#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_backups_browser_roundtrip() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let console = root.join("../console/dist").canonicalize().unwrap();
    assert!(console.join("index.html").is_file());
    let foreign = Fixture::new();
    foreign.server.inner.core.write_ui_language("en").unwrap();
    fs::write(
        foreign.workspace.join("notes.md"),
        "foreign workspace content",
    )
    .unwrap();
    let mut requested = request("Browser Foreign Backup");
    requested.scope.include_secrets = true;
    let job = launch_backup_job(&foreign.server, requested).await.unwrap();
    let terminal = completed(&foreign.server, &job.job_id).await;
    assert_eq!(terminal.status, "completed");
    let foreign_archive = find_archive(
        &backups_directory(&foreign.server).unwrap(),
        &terminal.backup_id,
    )
    .unwrap()
    .unwrap();
    let mut fixture = Fixture::new();
    fixture.server.inner.core.write_ui_language("en").unwrap();
    fixture.server = AppServer::new_desktop_with_stores_and_workspace(
        fixture.server.inner.core.clone(),
        &console,
        String::from("backup-test-shutdown-token"),
        fixture.credentials.clone(),
        &fixture.data,
        &fixture.workspace,
    )
    .unwrap();
    let (base, server) = fixture.start_http().await;
    let mut command = tokio::process::Command::new("node");
    command
        .arg(root.join("scripts/console_browser_smoke.mjs"))
        .args([&base, "--all", "--backups-crud"])
        .env("QWENPAW_BROWSER_FOREIGN_ARCHIVE", &foreign_archive)
        .kill_on_drop(true);
    let (timed_out, output) = roundtrip_output(&mut command, fixture.directory.path()).await;
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(10), server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !timed_out,
        "Browser roundtrip timed out\n{stdout}\n{stderr}"
    );
    let report: Value =
        serde_json::from_str(&stdout).unwrap_or_else(|_| panic!("{stdout}\n{stderr}"));
    let summary = json!({"ok": report["ok"], "pages": report["pages"].as_array().unwrap().iter().map(|page| {
        json!({"path": page["path"], "ok": page["ok"], "backups": page["backupsCrud"],
            "failed_api": page["failedApi"], "failures": page["failures"],
            "diagnostics": page["diagnostics"]})
    }).collect::<Vec<_>>()});
    println!("{summary:#}");
    assert!(output.status.success(), "{summary:#}\n{stderr}");
    assert_eq!(report["ok"], true);
    assert_eq!(report["pages"].as_array().unwrap().len(), 24);
    assert_eq!(
        fs::read_to_string(fixture.workspace.join("notes.md")).unwrap(),
        "foreign workspace content"
    );
}
