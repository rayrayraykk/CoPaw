use pretty_assertions::assert_eq;
use qwenpaw_protocol::CoreEvent;
use qwenpaw_protocol::Item;

use super::*;
use crate::desktop_environment;
use restore_credentials::CredentialKey;
use restore_credentials::CredentialRestore;

fn values(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn environment_archive_scopes_survive_candidate_apply_restart_and_joint_rollback() {
    let source = Fixture::new();
    let source_core = &source.server.inner.core;
    source
        .credentials
        .save("env:REGISTERED", Some("registered-secret"));
    source.credentials.save("env:SHARED", Some("stale-secret"));
    source_core
        .write_environment_keys(&[String::from("REGISTERED"), String::from("SHARED")])
        .unwrap();
    let effective = values(&[
        ("SHARED", "effective-secret"),
        ("VOLATILE", "volatile-secret"),
    ]);
    source_core
        .replace_runtime_environment(effective.clone())
        .unwrap();
    let exported = values(&[
        ("REGISTERED", "registered-secret"),
        ("SHARED", "effective-secret"),
        ("VOLATILE", "volatile-secret"),
    ]);
    let source_before = source_core.backup_snapshot(MAX_FILE_BYTES).unwrap();
    for globals in [false, true] {
        for include in [false, true] {
            let mut requested = request("environment scopes");
            requested.scope = BackupScope {
                include_agents: false,
                include_global_config: globals,
                include_secrets: include,
                include_skill_pool: false,
            };
            let job = launch_backup_job(&source.server, requested).await.unwrap();
            let terminal = completed(&source.server, &job.job_id).await;
            assert_eq!(terminal.status, "completed", "{:?}", terminal.error);
            let mut archive = ZipArchive::new(
                fs::File::open(
                    source
                        .data
                        .join("backups")
                        .join(format!("{}.zip", terminal.backup_id)),
                )
                .unwrap(),
            )
            .unwrap();
            let archived_secrets = if include {
                let snapshot: SecretSnapshot =
                    read_json_entry(&mut archive, SECRETS_FILE, MAX_FILE_BYTES).unwrap();
                assert_eq!(snapshot.environment, exported);
                Some(snapshot)
            } else {
                assert!(archive.by_name(SECRETS_FILE).is_err());
                None
            };
            let destination = Fixture::new();
            let core = &destination.server.inner.core;
            let local = values(&[("LOCAL", "local-secret"), ("SHARED", "target-secret")]);
            core.replace_runtime_environment(local.clone()).unwrap();
            core.write_environment_keys(&[String::from("SHARED")])
                .unwrap();
            destination
                .credentials
                .save("env:SHARED", Some("target-secret"));
            destination
                .credentials
                .save("env:LOCAL", Some("stale-target-secret"));
            destination
                .credentials
                .save("unrelated", Some("keep-secret"));
            let keys_before = destination.credentials.values.lock().unwrap().clone();
            let before = core.backup_snapshot(MAX_FILE_BYTES).unwrap();
            let mut lease = core.begin_restore(Duration::from_secs(2)).await.unwrap();
            let rollback = lease.capture_rollback(MAX_FILE_BYTES).unwrap();
            let state = if globals {
                let state: qwenpaw_storage::StoreBackup =
                    read_json_entry(&mut archive, CORE_STATE_FILE, MAX_FILE_BYTES).unwrap();
                assert!(!serde_json::to_string(&state).unwrap().contains("-secret"));
                state
            } else {
                assert!(archive.by_name(CORE_STATE_FILE).is_err());
                before.clone()
            };
            let candidate = lease.prepare_restore(&state).unwrap();
            let known = secrets::known_keys(&destination.server, &[]).unwrap();
            // Volatile keys also belong to the application credential catalog.
            assert!(known.contains(&CredentialKey::Environment(String::from("LOCAL"))));
            let mut plan =
                secrets::plan_restore(archived_secrets.as_ref(), include, false, &known).unwrap();
            let staged = desktop_environment::hydrate_restore(
                &candidate,
                core,
                archived_secrets
                    .as_ref()
                    .filter(|_| include)
                    .map(|snapshot| &snapshot.environment),
            )
            .unwrap();
            let expected = if include { &exported } else { &local };
            assert_eq!(&staged, expected);
            assert_eq!(candidate.runtime_environment().unwrap(), *expected);
            assert_eq!(
                candidate.read_environment_keys().unwrap(),
                expected.keys().cloned().collect::<Vec<_>>()
            );
            assert_eq!(core.backup_snapshot(MAX_FILE_BYTES).unwrap(), before);
            assert_eq!(core.runtime_environment().unwrap(), local);
            assert_eq!(*destination.credentials.values.lock().unwrap(), keys_before);
            plan.credentials.extend(
                staged
                    .into_iter()
                    .map(|(key, value)| (CredentialKey::Environment(key), Some(value))),
            );
            let mut credential_tx =
                CredentialRestore::prepare(destination.credentials.clone(), plan.credentials)
                    .unwrap();
            credential_tx.apply().unwrap();
            lease.apply(&candidate, MAX_FILE_BYTES).await.unwrap();
            assert_eq!(core.runtime_environment().unwrap(), *expected);
            assert_eq!(
                desktop_environment::backup_values(core, destination.credentials.as_ref()).unwrap(),
                *expected
            );
            let reopened = Core::persistent(
                ModelConfig {
                    api_key: None,
                    base_url: String::from("http://127.0.0.1:1/v1"),
                    default_model: String::from("fixture-model"),
                },
                &destination.data.join("threads.sqlite3"),
            )
            .unwrap();
            desktop_environment::initialize(&reopened, destination.credentials.as_ref()).unwrap();
            assert_eq!(reopened.runtime_environment().unwrap(), *expected);
            assert_eq!(
                reopened.backup_snapshot(MAX_FILE_BYTES).unwrap(),
                core.backup_snapshot(MAX_FILE_BYTES).unwrap()
            );
            lease.apply(&rollback, MAX_FILE_BYTES).await.unwrap();
            credential_tx.rollback().unwrap();
            assert_eq!(core.backup_snapshot(MAX_FILE_BYTES).unwrap(), before);
            assert_eq!(core.runtime_environment().unwrap(), local);
            assert_eq!(*destination.credentials.values.lock().unwrap(), keys_before);
            assert_eq!(
                source_core.backup_snapshot(MAX_FILE_BYTES).unwrap(),
                source_before
            );
            assert_eq!(source_core.runtime_environment().unwrap(), effective);
        }
    }
}

#[tokio::test]
async fn missing_environment_payload_preserves_values_but_explicit_empty_snapshot_clears_them() {
    for snapshot in [
        None,
        Some(SecretSnapshot {
            version: 1,
            ..SecretSnapshot::default()
        }),
    ] {
        let fixture = Fixture::new();
        let core = &fixture.server.inner.core;
        let local = values(&[("LOCAL", "local-value")]);
        core.replace_runtime_environment(local.clone()).unwrap();
        fixture
            .credentials
            .save("env:LOCAL", Some("old-stored-value"));
        let before = core.backup_snapshot(MAX_FILE_BYTES).unwrap();
        let mut lease = core.begin_restore(Duration::from_secs(2)).await.unwrap();
        let candidate = lease.prepare_restore(&before).unwrap();
        let mut plan = secrets::plan_restore(
            snapshot.as_ref(),
            true,
            false,
            &secrets::known_keys(&fixture.server, &[]).unwrap(),
        )
        .unwrap();
        let materialized = desktop_environment::hydrate_restore(
            &candidate,
            core,
            snapshot.as_ref().map(|snapshot| &snapshot.environment),
        )
        .unwrap();
        let expected = if snapshot.is_some() {
            BTreeMap::new()
        } else {
            local
        };
        assert_eq!(materialized, expected);
        plan.credentials.extend(
            materialized
                .into_iter()
                .map(|(key, value)| (CredentialKey::Environment(key), Some(value))),
        );
        let mut credential_tx =
            CredentialRestore::prepare(fixture.credentials.clone(), plan.credentials).unwrap();
        credential_tx.apply().unwrap();
        lease.apply(&candidate, MAX_FILE_BYTES).await.unwrap();
        desktop_environment::initialize(&candidate, fixture.credentials.as_ref()).unwrap();
        assert_eq!(candidate.runtime_environment().unwrap(), expected);
        assert_eq!(core.runtime_environment().unwrap(), expected);
        assert_eq!(
            fixture.credentials.load("env:LOCAL"),
            expected.get("LOCAL").cloned()
        );
        assert_eq!(
            core.read_environment_keys().unwrap(),
            expected.keys().cloned().collect::<Vec<_>>()
        );
    }
}

#[test]
fn invalid_environment_payload_or_catalog_leaves_the_candidate_unchanged() {
    let fixture = Fixture::new();
    let core = &fixture.server.inner.core;
    let before = core.backup_snapshot(MAX_FILE_BYTES).unwrap();
    for incoming in [
        values(&[("BAD-NAME", "secret")]),
        values(&[("VALID", "private\0secret")]),
        (0..257)
            .map(|index| (format!("KEY_{index}"), String::new()))
            .collect(),
    ] {
        let candidate = core.prepare_restore(&before).unwrap();
        assert_eq!(
            desktop_environment::hydrate_restore(&candidate, core, Some(&incoming)),
            Err("Restored environment is invalid")
        );
        assert_eq!(candidate.backup_snapshot(MAX_FILE_BYTES).unwrap(), before);
        assert_eq!(candidate.runtime_environment().unwrap(), BTreeMap::new());
    }
    for catalog in [r#"["DUPLICATE","DUPLICATE"]"#, r#"["BAD-NAME"]"#, "{}"] {
        let mut malformed = before.clone();
        malformed
            .settings
            .insert(String::from("desktop_environment_keys"), catalog.to_owned());
        let candidate = core.prepare_restore(&malformed).unwrap();
        assert_eq!(
            desktop_environment::hydrate_restore(&candidate, core, Some(&BTreeMap::new())),
            Err("Restored environment catalog is invalid")
        );
        assert_eq!(
            candidate.backup_snapshot(MAX_FILE_BYTES).unwrap(),
            malformed
        );
        assert_eq!(candidate.runtime_environment().unwrap(), BTreeMap::new());
    }
    assert_eq!(core.backup_snapshot(MAX_FILE_BYTES).unwrap(), before);
}

#[tokio::test]
async fn restored_and_rolled_back_environments_reach_real_agent_shell_processes() {
    const KEY: &str = "QWENPAW_RESTORE_ENV_FIXTURE";
    let host_before = std::env::var_os(KEY);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let command = if cfg!(windows) {
        format!("echo %{KEY}%")
    } else {
        format!("printf '%s' \"${KEY}\"")
    };
    let router = Router::new().route(
        "/v1/chat/completions",
        post(move |Json(body): Json<Value>| {
            let command = command.clone();
            async move {
                let tool_finished = body["messages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|message| message["role"] == "tool");
                let delta = if tool_finished {
                    json!({"content": "fixture completed"})
                } else {
                    json!({"tool_calls": [{"index": 0, "id": "environment_shell", "function": {
                        "name": "shell", "arguments": json!({"command": command}).to_string()
                    }}]})
                };
                let event = json!({"choices": [{"delta": delta}]});
                (
                    [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
                    format!("data: {event}\n\ndata: [DONE]\n\n"),
                )
            }
        }),
    );
    let stop = CancellationToken::new();
    let shutdown = stop.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
            .unwrap();
    });
    let core = Core::new(ModelConfig {
        api_key: None,
        base_url: url,
        default_model: String::from("environment-fixture"),
    });
    core.replace_runtime_environment(values(&[(KEY, "original-fixture-value")]))
        .unwrap();
    let before = core.backup_snapshot(MAX_FILE_BYTES).unwrap();
    let mut lease = core.begin_restore(Duration::from_secs(2)).await.unwrap();
    let rollback = lease.capture_rollback(MAX_FILE_BYTES).unwrap();
    let candidate = lease.prepare_restore(&before).unwrap();
    desktop_environment::hydrate_restore(
        &candidate,
        &core,
        Some(&values(&[(KEY, "restored-fixture-value")])),
    )
    .unwrap();
    lease.apply(&candidate, MAX_FILE_BYTES).await.unwrap();
    drop(lease);
    assert_shell_environment(&core, "restored-fixture-value").await;
    let mut lease = core.begin_restore(Duration::from_secs(2)).await.unwrap();
    lease.apply(&rollback, MAX_FILE_BYTES).await.unwrap();
    drop(lease);
    assert_shell_environment(&core, "original-fixture-value").await;
    assert_eq!(std::env::var_os(KEY), host_before);
    stop.cancel();
    tokio::time::timeout(Duration::from_secs(2), server)
        .await
        .unwrap()
        .unwrap();
}

async fn assert_shell_environment(core: &Core, expected: &str) {
    let directory = tempfile::tempdir().unwrap();
    let thread = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .unwrap()
        .thread;
    let (_, mut events) = core
        .start_turn(qwenpaw_protocol::TurnStartParams {
            thread_id: thread.id,
            input: vec![qwenpaw_protocol::UserInput::Text {
                text: String::from("Read the fixture variable"),
            }],
        })
        .await
        .unwrap();
    let mut results = Vec::new();
    let mut status = None;
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(event) = events.recv().await {
            match event {
                CoreEvent::ToolApprovalRequested(notification) => {
                    assert!(
                        core.respond_tool_approval(qwenpaw_protocol::ToolApprovalRespondParams {
                            approval_id: notification.approval_id,
                            decision: qwenpaw_protocol::ApprovalDecision::Approved,
                        })
                        .await
                        .accepted
                    );
                }
                CoreEvent::ItemCompleted(notification) => {
                    if let Item::ToolResult {
                        content, is_error, ..
                    } = notification.item
                    {
                        assert!(!is_error, "{content}");
                        results.push(content);
                    }
                }
                CoreEvent::TurnCompleted(completed) => status = Some(completed.turn.status),
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(status, Some(qwenpaw_protocol::TurnStatus::Completed));
    assert_eq!(results.len(), 1);
    assert!(results[0].contains(expected), "{}", results[0]);
}
