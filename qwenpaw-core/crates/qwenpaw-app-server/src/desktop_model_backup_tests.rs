use pretty_assertions::assert_eq;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;

use super::*;

#[cfg(unix)]
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn local_model_backup_restore_resumes_selected_runtime_rolls_back_or_falls_back() {
    for outcome in ["restore", "rollback", "missing-assets"] {
        let fixture = Fixture::new();
        let root = fixture.data.join("local-models");
        fs::create_dir_all(root.join("bin")).unwrap();
        let executable = root.join("bin/llama-server");
        fs::write(
            &executable,
            include_bytes!("../../../scripts/tests/fixtures/fake_llama_server.py"),
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        for name in ["Fixture/A", "Fixture/B"] {
            let directory = root.join("models").join(name);
            fs::create_dir_all(&directory).unwrap();
            fs::write(directory.join("model.gguf"), b"fixture model bytes").unwrap();
        }
        fixture.credentials.save("api", Some("source-remote-key"));
        let (base, server) = fixture.start_http().await;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        let response = client
            .post(format!("{base}/api/local-models/server"))
            .json(&json!({"model_id": "Fixture/A"}))
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{:?}",
            response.text().await
        );
        let mut requested = request("running local model");
        requested.scope.include_secrets = true;
        let job = launch_backup_job(&fixture.server, requested).await.unwrap();
        let terminal = completed(&fixture.server, &job.job_id).await;
        assert_eq!(terminal.status, "completed", "{:?}", terminal.error);
        let response = client
            .post(format!("{base}/api/local-models/server"))
            .json(&json!({"model_id": "Fixture/B"}))
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{:?}",
            response.text().await
        );
        fixture.credentials.save("api", Some("current-remote-key"));
        if outcome == "rollback" {
            fixture
                .credentials
                .api_failures
                .store(1, std::sync::atomic::Ordering::SeqCst);
        } else if outcome == "missing-assets" {
            fs::rename(
                root.join("models/Fixture/A/model.gguf"),
                fixture.directory.path().join("absent-model.gguf"),
            )
            .unwrap();
        }
        let response = client
            .post(format!("{base}/api/backups/{}/restore", terminal.backup_id))
            .json(
                &json!({"mode": "full", "include_agents": true, "agent_ids": ["default"],
                "include_global_config": true, "include_secrets": true, "include_skill_pool": true,
                "preserve_local_protected_config": false}),
            )
            .send()
            .await
            .unwrap();
        let status = response.status();
        let body = response.json::<Value>().await.unwrap();
        if outcome == "rollback" {
            assert_eq!(
                (status, body),
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    json!({"detail": "Credential restore failed; original values restored"})
                )
            );
        } else {
            assert_eq!(
                (status, body),
                (
                    StatusCode::OK,
                    json!({"ok": true, "preserved_local_keys": []})
                )
            );
        }
        let local: Value = client
            .get(format!("{base}/api/local-models/server"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let config = fixture.server.inner.core.backup_model_config();
        let port = local["port"].as_u64();
        if outcome == "missing-assets" {
            assert_eq!(local["available"], false);
            assert_eq!(config.base_url, "http://127.0.0.1:1/v1");
            assert_eq!(config.default_model, "fixture-model");
            assert_eq!(config.api_key, Some(String::from("source-remote-key")));
        } else {
            let model = if outcome == "rollback" {
                "Fixture/B"
            } else {
                "Fixture/A"
            };
            assert_eq!(local["available"], true);
            assert_eq!(local["model_name"], model);
            assert_eq!(config.default_model, model);
            assert_eq!(
                config.base_url,
                format!("http://127.0.0.1:{}/v1", port.unwrap())
            );
            assert_eq!(config.api_key, None);
        }
        assert_eq!(
            fs::read(root.join("models/Fixture/B/model.gguf")).unwrap(),
            b"fixture model bytes"
        );
        assert!(!is_restoring(&fixture.server));
        fixture.server.inner.shutdown.cancel();
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        if let Some(port) = port {
            assert!(
                client
                    .get(format!("http://127.0.0.1:{port}/health"))
                    .send()
                    .await
                    .is_err()
            );
        }
    }
}

fn configure_provider(fixture: &Fixture, provider: &str) {
    let path = fixture.data.join("models/registry.json");
    let mut registry: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let mut record = registry["providers"]["openai-compatible"].clone();
    record["api_key_configured"] = json!(true);
    registry["providers"]["openai-compatible"] = record.clone();
    record["id"] = json!(provider);
    registry["providers"][provider] = record.clone();
    record["id"] = json!("dormant");
    record["base_url"] = json!("http://127.0.0.1:2/v1");
    registry["providers"]["dormant"] = record;
    registry["active_provider_id"] = json!(provider);
    fs::write(path, serde_json::to_vec_pretty(&registry).unwrap()).unwrap();
    fixture.credentials.save("api", Some("stored-api-key"));
    fixture
        .credentials
        .save("model-provider-api-key:dormant", Some("dormant-key"));
    if provider != "openai-compatible" {
        fixture.credentials.save(
            &format!("model-provider-api-key:{provider}"),
            Some("stale-provider-key"),
        );
    }
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn model_backup_captures_effective_keys_and_clear_without_leaking_into_global_scope() {
    for provider in ["openai-compatible", "custom"] {
        for effective in [Some("volatile-model-key"), None] {
            let fixture = Fixture::new();
            configure_provider(&fixture, provider);
            fixture
                .server
                .inner
                .core
                .set_runtime_api_key(effective.map(str::to_owned))
                .unwrap();
            signing_key(fixture.credentials.as_ref()).unwrap();
            let original = fixture.credentials.values.lock().unwrap().clone();
            for globals in [false, true] {
                for secrets in [false, true] {
                    let mut requested = request("model scope fixture");
                    requested.scope = BackupScope {
                        include_agents: false,
                        include_global_config: globals,
                        include_secrets: secrets,
                        include_skill_pool: false,
                    };
                    let job = launch_backup_job(&fixture.server, requested).await.unwrap();
                    let terminal = completed(&fixture.server, &job.job_id).await;
                    assert_eq!(terminal.status, "completed", "{:?}", terminal.error);
                    let path = find_archive(
                        &backups_directory(&fixture.server).unwrap(),
                        &terminal.backup_id,
                    )
                    .unwrap()
                    .unwrap();
                    let mut archive = ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
                    assert_eq!(archive.by_name(SECRETS_FILE).is_ok(), secrets);
                    assert_eq!(
                        archive.by_name("data/config/models/registry.json").is_ok(),
                        globals
                    );
                    if secrets {
                        let value: Value =
                            read_json_entry(&mut archive, SECRETS_FILE, MAX_FILE_BYTES).unwrap();
                        let mut providers = serde_json::Map::from_iter([(
                            String::from("dormant"),
                            json!("dormant-key"),
                        )]);
                        if provider != "openai-compatible"
                            && let Some(key) = effective
                        {
                            providers.insert(provider.to_owned(), json!(key));
                        }
                        let api_key = if provider == "openai-compatible" {
                            effective
                        } else {
                            Some("stored-api-key")
                        };
                        assert_eq!(
                            value,
                            json!({"version": 1, "api_key": api_key, "model_providers": providers,
                            "agent_settings": {}, "environment": {}, "mcp_clients": {}, "oauth": {"version": 1, "clients": {}}})
                        );
                    }
                    for index in 0..archive.len() {
                        let mut entry = archive.by_index(index).unwrap();
                        if entry.name() != SECRETS_FILE {
                            let mut content = String::new();
                            entry.read_to_string(&mut content).unwrap();
                            for key in [
                                "volatile-model-key",
                                "stored-api-key",
                                "stale-provider-key",
                                "dormant-key",
                            ] {
                                assert!(
                                    !content.contains(key),
                                    "Credential leaked outside secrets: {}",
                                    entry.name()
                                );
                            }
                        }
                    }
                    assert_eq!(*fixture.credentials.values.lock().unwrap(), original);
                    assert_eq!(
                        fixture.server.inner.core.backup_model_config().api_key,
                        effective.map(str::to_owned)
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn model_backup_rejects_unmatched_runtime_credentials_without_publishing_or_reassigning() {
    let fixture = Fixture::new();
    configure_provider(&fixture, "custom");
    fixture
        .server
        .inner
        .core
        .write_config(qwenpaw_protocol::ConfigWriteParams {
            base_url: Some(String::from("http://127.0.0.1:3/v1")),
            default_model: None,
        })
        .unwrap();
    fixture
        .server
        .inner
        .core
        .set_runtime_api_key(Some(String::from("unmatched-private-key")))
        .unwrap();
    signing_key(fixture.credentials.as_ref()).unwrap();
    let before = fixture.credentials.values.lock().unwrap().clone();
    let mut requested = request("unmatched source");
    requested.scope.include_secrets = true;
    let job = launch_backup_job(&fixture.server, requested).await.unwrap();
    let terminal = completed(&fixture.server, &job.job_id).await;
    assert_eq!(terminal.status, "failed");
    assert_eq!(
        terminal.error.as_deref(),
        Some("Effective model credential cannot be matched to its provider")
    );
    assert_eq!(
        list_backups(State(fixture.server.clone())).await.unwrap().0,
        json!([])
    );
    assert_eq!(*fixture.credentials.values.lock().unwrap(), before);
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn model_secrets_only_restore_updates_credential_indicators_and_survives_desktop_restart() {
    for effective in [Some("volatile-model-key"), None] {
        let fixture = Fixture::new();
        configure_provider(&fixture, "custom");
        fixture
            .server
            .inner
            .core
            .set_runtime_api_key(effective.map(str::to_owned))
            .unwrap();
        let mut requested = request("secrets-only source");
        requested.scope = BackupScope {
            include_agents: false,
            include_global_config: false,
            include_secrets: true,
            include_skill_pool: false,
        };
        let job = launch_backup_job(&fixture.server, requested).await.unwrap();
        let terminal = completed(&fixture.server, &job.job_id).await;
        assert_eq!(terminal.status, "completed");
        fixture
            .credentials
            .save("model-provider-api-key:custom", Some("target-key"));
        fixture
            .server
            .inner
            .core
            .set_runtime_api_key(Some(String::from("target-key")))
            .unwrap();
        let mut expected: Value =
            serde_json::from_slice(&fs::read(fixture.data.join("models/registry.json")).unwrap())
                .unwrap();
        if effective.is_none() {
            expected["providers"]["custom"]["api_key_configured"] = json!(false);
            expected["revision"] = json!(expected["revision"].as_u64().unwrap() + 1);
        }
        let (base, server) = fixture.start_http().await;
        let response = reqwest::Client::new().post(format!("{base}/api/backups/{}/restore", terminal.backup_id))
            .json(&json!({"include_agents": false, "include_global_config": false, "include_skill_pool": false,
                "include_secrets": true, "mode": "custom"})).timeout(Duration::from_secs(10)).send().await.unwrap();
        assert_eq!(
            (response.status(), response.json::<Value>().await.unwrap()),
            (
                StatusCode::OK,
                json!({"ok": true, "preserved_local_keys": []})
            )
        );
        let actual: Value =
            serde_json::from_slice(&fs::read(fixture.data.join("models/registry.json")).unwrap())
                .unwrap();
        assert_eq!(actual, expected);
        assert_eq!(
            fixture.credentials.load("model-provider-api-key:custom"),
            effective.map(str::to_owned)
        );
        assert_eq!(
            fixture.server.inner.core.backup_model_config().api_key,
            effective.map(str::to_owned)
        );
        fixture.server.inner.shutdown.cancel();
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let reopened = Core::persistent(
            ModelConfig {
                api_key: None,
                base_url: String::from("http://127.0.0.1:1/v1"),
                default_model: String::from("fixture-model"),
            },
            &fixture.data.join("threads.sqlite3"),
        )
        .unwrap();
        let restarted = AppServer::new_desktop_with_stores_and_workspace(
            reopened,
            &fixture.directory.path().join("console"),
            String::from("backup-test-shutdown-token"),
            fixture.credentials.clone(),
            &fixture.data,
            &fixture.workspace,
        )
        .unwrap();
        assert_eq!(
            restarted.inner.core.backup_model_config().api_key,
            effective.map(str::to_owned)
        );
        assert_eq!(
            restarted.inner.core.read_config().config.api_key_configured,
            effective.is_some()
        );
        assert_eq!(
            fs::read_to_string(fixture.workspace.join("notes.md")).unwrap(),
            "original workspace content"
        );
    }
}
