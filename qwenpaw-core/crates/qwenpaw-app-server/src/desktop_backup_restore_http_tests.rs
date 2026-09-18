use pretty_assertions::assert_eq;
use qwenpaw_protocol::ThreadListParams;

use super::*;

#[tokio::test]
async fn scoped_cron_restore_maps_foreign_workspace_keys_and_preserves_unselected_native_jobs() {
    let source = Fixture::new();
    let destination = Fixture::new();
    let (source_base, source_task) = source.start_http().await;
    let (base, task) = destination.start_http().await;
    seed_scoped_backup_state(&source, &source_base).await;
    seed_scoped_backup_state(&destination, &base).await;
    let source_channels = seed_channel_configs(&source_base, "source").await;
    let destination_channels = seed_channel_configs(&base, "destination").await;
    let mail_before = seed_foreign_mail_subjects(&source, &destination);
    let usage_before = [usage_snapshot(&source), usage_snapshot(&destination)];
    let source_chats = chat_catalog(&source);
    let before_chats = chat_catalog(&destination);
    let mut source_cron: Value =
        serde_json::from_str(&source.server.inner.core.read_cron_data().unwrap().unwrap()).unwrap();
    source_cron["jobs"][1]["text"] = json!("restored from another native Core");
    source
        .server
        .inner
        .core
        .write_cron_data(&source_cron.to_string())
        .unwrap();
    let before: Value = serde_json::from_str(
        &destination
            .server
            .inner
            .core
            .read_cron_data()
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_ne!(
        before["workspace_owners"]["writer-job"],
        source_cron["workspace_owners"]["writer-job"]
    );
    let mut requested = request("native scoped identity mapping");
    requested.agents = vec![String::from("writer")];
    requested.scope.include_global_config = false;
    let started = launch_backup_job(&source.server, requested).await.unwrap();
    let terminal = completed(&source.server, &started.job_id).await;
    assert_eq!(terminal.status, "completed", "{:?}", terminal.error);
    let archive = find_archive(
        &backups_directory(&source.server).unwrap(),
        &terminal.backup_id,
    )
    .unwrap()
    .unwrap();
    let target = backups_directory(&destination.server).unwrap();
    fs::create_dir_all(&target).unwrap();
    fs::copy(archive, target.join(format!("{}.zip", terminal.backup_id))).unwrap();
    let response = reqwest::Client::new().post(format!("{base}/api/backups/{}/restore", terminal.backup_id))
        .json(&json!({"mode":"custom","include_agents":true,"agent_ids":["writer"],"include_global_config":false,
            "include_secrets":false,"include_skill_pool":false,"trust_mode":"foreign","preserve_local_protected_config":true}))
        .timeout(Duration::from_secs(10)).send().await.unwrap();
    assert_eq!(
        (response.status(), response.json::<Value>().await.unwrap()),
        (StatusCode::OK, json!({"ok":true,"preserved_local_keys":[]}))
    );
    let mut expected = before;
    expected["jobs"][1] = source_cron["jobs"][1].clone();
    for field in ["states", "history", "public_ids"] {
        expected[field]["writer-job"] = source_cron[field]["writer-job"].clone();
    }
    assert_eq!(
        serde_json::from_str::<Value>(
            &destination
                .server
                .inner
                .core
                .read_cron_data()
                .unwrap()
                .unwrap()
        )
        .unwrap(),
        expected
    );
    assert_eq!(
        serde_json::from_str::<Value>(&source.server.inner.core.read_cron_data().unwrap().unwrap())
            .unwrap(),
        source_cron
    );
    verify_foreign_chat_mapping(&source, &destination, &source_chats, before_chats);
    verify_foreign_usage_mapping(&source, &destination, &usage_before);
    verify_foreign_mail_mapping(&source, &destination, &mail_before);
    assert_eq!(read_channel_configs(&source_base).await, source_channels);
    assert_eq!(
        read_channel_configs(&base).await,
        [destination_channels[0].clone(), source_channels[1].clone()]
    );
    source.server.inner.shutdown.cancel();
    destination.server.inner.shutdown.cancel();
    for server in [source_task, task] {
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}

async fn read_channel_configs(base: &str) -> [Value; 2] {
    let client = reqwest::Client::new();
    let mut values = Vec::new();
    for agent in ["default", "writer"] {
        values.push(
            client
                .get(format!("{base}/api/config/channels/console"))
                .header("X-Agent-Id", agent)
                .timeout(Duration::from_secs(5))
                .send()
                .await
                .unwrap()
                .error_for_status()
                .unwrap()
                .json::<Value>()
                .await
                .unwrap(),
        );
    }
    values.try_into().unwrap()
}

async fn seed_channel_configs(base: &str, label: &str) -> [Value; 2] {
    let client = reqwest::Client::new();
    let mut values = read_channel_configs(base).await;
    for (index, agent) in ["default", "writer"].into_iter().enumerate() {
        values[index]["bot_prefix"] = json!(format!("{label} {agent}"));
        let response = client
            .put(format!("{base}/api/config/channels/console"))
            .header("X-Agent-Id", agent)
            .json(&values[index])
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        assert_eq!(response, values[index]);
    }
    values
}

fn mail_snapshot(fixture: &Fixture) -> Value {
    serde_json::from_str(
        &fixture
            .server
            .inner
            .core
            .read_mail_access_control_data()
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}

fn seed_foreign_mail_subjects(source: &Fixture, destination: &Fixture) -> [Value; 2] {
    let mut mail = mail_snapshot(source);
    let workspace = mail["workspaces"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["agents"].get("writer").is_some())
        .unwrap();
    let mut historical = workspace["agents"]["writer"].clone();
    historical["whitelist"]["person@example.com"]["remark"] = json!("historical subject");
    workspace["agents"]["editor"] = historical;
    workspace["agents"]["writer"]["whitelist"]["person@example.com"]["remark"] =
        json!("foreign writer");
    source
        .server
        .inner
        .core
        .write_mail_access_control_data(&mail.to_string())
        .unwrap();
    [mail, mail_snapshot(destination)]
}

fn verify_foreign_mail_mapping(source: &Fixture, destination: &Fixture, before: &[Value; 2]) {
    let mut expected = before[1].clone();
    let incoming = before[0]["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["agents"].get("writer").is_some())
        .unwrap();
    let target = expected["workspaces"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["agents"].get("writer").is_some())
        .unwrap();
    assert_ne!(target["data_key"], incoming["data_key"]);
    target["agents"] = incoming["agents"].clone();
    assert_eq!(mail_snapshot(destination), expected);
    assert_eq!(mail_snapshot(source), before[0]);
    let reopened = Core::persistent(
        destination.server.inner.core.backup_model_config(),
        &destination.data.join("threads.sqlite3"),
    )
    .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&reopened.read_mail_access_control_data().unwrap().unwrap())
            .unwrap(),
        expected
    );
}

fn usage_snapshot(fixture: &Fixture) -> Vec<qwenpaw_storage::StoredUsageRecord> {
    fixture
        .server
        .inner
        .core
        .backup_snapshot(MAX_FILE_BYTES)
        .unwrap()
        .usage
}

fn verify_foreign_usage_mapping(
    source: &Fixture,
    destination: &Fixture,
    before: &[Vec<qwenpaw_storage::StoredUsageRecord>; 2],
) {
    let local_key = before[1]
        .iter()
        .find(|record| record.agent_id == "writer")
        .unwrap()
        .data_key
        .clone();
    let mut expected = before[1]
        .iter()
        .filter(|record| record.agent_id != "writer")
        .cloned()
        .collect::<Vec<_>>();
    for record in before[0]
        .iter()
        .filter(|record| record.agent_id == "writer")
    {
        assert_ne!(record.data_key, local_key);
        let mut restored = record.clone();
        restored.data_key.clone_from(&local_key);
        expected.push(restored);
    }
    expected.sort_by(|left, right| left.id.cmp(&right.id));
    assert_eq!(usage_snapshot(destination), expected);
    assert_eq!(usage_snapshot(source), before[0]);
    let reopened = Core::persistent(
        destination.server.inner.core.backup_model_config(),
        &destination.data.join("threads.sqlite3"),
    )
    .unwrap();
    assert_eq!(
        reopened.backup_snapshot(MAX_FILE_BYTES).unwrap().usage,
        expected
    );
}

fn chat_catalog(fixture: &Fixture) -> Value {
    serde_json::from_str(
        &fixture
            .server
            .inner
            .core
            .read_chat_catalog_data()
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}

fn verify_foreign_chat_mapping(
    source: &Fixture,
    destination: &Fixture,
    source_chats: &Value,
    mut before: Value,
) {
    let local_key = before["chats"]
        .as_object()
        .unwrap()
        .values()
        .find(|chat| chat["agent_id"] == "writer")
        .unwrap()["data_key"]
        .clone();
    let selected = |record: &Value| record["agent_id"] == "writer";
    before["chats"]
        .as_object_mut()
        .unwrap()
        .retain(|_, chat| !selected(chat));
    before["groups"]
        .as_array_mut()
        .unwrap()
        .retain(|group| !selected(group));
    for (id, chat) in source_chats["chats"].as_object().unwrap() {
        if selected(chat) {
            assert_ne!(chat["data_key"], local_key);
            let mut restored = chat.clone();
            restored["data_key"] = local_key.clone();
            before["chats"][id] = restored;
        }
    }
    for group in source_chats["groups"].as_array().unwrap() {
        if selected(group) {
            let mut restored = group.clone();
            restored["data_key"] = local_key.clone();
            before["groups"].as_array_mut().unwrap().push(restored);
        }
    }
    assert_eq!(chat_catalog(destination), before);
    assert_eq!(&chat_catalog(source), source_chats);
}

async fn create_full(fixture: &Fixture) -> String {
    fixture.credentials.save("api", Some("archived-model-key"));
    fixture
        .server
        .inner
        .core
        .set_runtime_api_key(Some(String::from("archived-model-key")))
        .unwrap();
    fixture.server.inner.core.write_ui_language("en").unwrap();
    let pool = fixture.data.join("skill_pool").join("fixture");
    fs::create_dir_all(&pool).unwrap();
    fs::write(pool.join("SKILL.md"), "archived skill").unwrap();
    let mut requested = request("production restore");
    requested.scope.include_secrets = true;
    let job = launch_backup_job(&fixture.server, requested).await.unwrap();
    let terminal = completed(&fixture.server, &job.job_id).await;
    assert_eq!(terminal.status, "completed", "{:?}", terminal.error);
    terminal.backup_id
}

fn restore_body() -> Value {
    json!({"mode": "full", "include_agents": true, "agent_ids": ["default"],
        "include_global_config": true, "include_secrets": true, "include_skill_pool": true,
        "preserve_local_protected_config": false})
}

#[tokio::test]
async fn production_restore_direct_foreign_trust_is_required_and_persisted_before_restoration() {
    let source = Fixture::new();
    let id = create_full(&source).await;
    let fixture = Fixture::new();
    let directory = backups_directory(&fixture.server).unwrap();
    fs::create_dir_all(&directory).unwrap();
    let destination = directory.join(format!("{id}.zip"));
    let archive = find_archive(&backups_directory(&source.server).unwrap(), &id)
        .unwrap()
        .unwrap();
    fs::copy(archive, &destination).unwrap();
    let key = signing_key(fixture.credentials.as_ref()).unwrap();
    let before = fs::read(&destination).unwrap();
    let (base, server) = fixture.start_http().await;
    let client = reqwest::Client::new();
    let mut requested = restore_body();
    requested
        .as_object_mut()
        .unwrap()
        .remove("preserve_local_protected_config");
    let response = client
        .post(format!("{base}/api/backups/{id}/restore"))
        .json(&requested)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response.json::<Value>().await.unwrap()["detail"]["code"],
        "backup_signature_mismatch"
    );
    std::assert_eq!(fs::read(&destination).unwrap(), before);
    requested["trust_mode"] = json!("foreign");
    let response = client
        .post(format!("{base}/api/backups/{id}/restore"))
        .json(&requested)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .unwrap();
    assert_eq!(
        (response.status(), response.json::<Value>().await.unwrap()),
        (
            StatusCode::OK,
            json!({"ok": true, "preserved_local_keys": ["security", "mcp"]})
        )
    );
    let signed = validate_archive(&destination, &key).unwrap();
    assert!(matches!(signed.trust, ArchiveTrust::Local));
    assert_eq!(signed.meta.accepted_via_trust, Some(true));
    requested.as_object_mut().unwrap().remove("trust_mode");
    let response = client
        .post(format!("{base}/api/backups/{id}/restore"))
        .json(&requested)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .unwrap();
    assert_eq!(
        (response.status(), response.json::<Value>().await.unwrap()),
        (
            StatusCode::OK,
            json!({"ok": true, "preserved_local_keys": ["security", "mcp"]})
        )
    );
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn production_restore_preserves_versioned_security_and_effective_bootstrap_mcp() {
    let fixture = Fixture::new();
    let id = create_full(&fixture).await;
    let mut security = fixture.server.inner.core.security_settings().unwrap();
    security.file_guard.paths.push(
        fixture
            .directory
            .path()
            .join("protected-fixture")
            .to_string_lossy()
            .into_owned(),
    );
    fixture
        .server
        .inner
        .core
        .replace_security_settings(security.clone())
        .unwrap();
    let mcp = vec![bootstrap_client("local-bootstrap")];
    fixture
        .server
        .inner
        .core
        .replace_mcp_client_settings(mcp.clone())
        .unwrap();
    let mut request = restore_body();
    request["preserve_local_protected_config"] = json!(true);
    let (base, server) = fixture.start_http().await;
    let response = reqwest::Client::new()
        .post(format!("{base}/api/backups/{id}/restore"))
        .json(&request)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .unwrap();
    assert_eq!(
        (response.status(), response.json::<Value>().await.unwrap()),
        (
            StatusCode::OK,
            json!({"ok": true, "preserved_local_keys": ["security", "mcp"]})
        )
    );
    assert_eq!(
        fixture.server.inner.core.security_settings().unwrap(),
        security
    );
    assert_eq!(fixture.server.inner.core.mcp_client_settings(), mcp);
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
    assert_eq!(reopened.security_settings().unwrap(), security);
    let restarted = AppServer::new_desktop_with_stores_and_workspace(
        reopened,
        &fixture.directory.path().join("console"),
        String::from("backup-test-shutdown-token"),
        fixture.credentials.clone(),
        &fixture.data,
        &fixture.workspace,
    )
    .unwrap();
    assert_eq!(restarted.inner.core.mcp_client_settings(), mcp);
}

fn mutate(fixture: &Fixture) {
    fixture.credentials.save("api", Some("current-model-key"));
    fixture
        .server
        .inner
        .core
        .set_runtime_api_key(Some(String::from("current-model-key")))
        .unwrap();
    fixture.server.inner.core.write_ui_language("zh").unwrap();
    fs::write(
        fixture.workspace.join("notes.md"),
        "current workspace content",
    )
    .unwrap();
    fs::write(
        fixture.data.join("skill_pool/fixture/SKILL.md"),
        "current skill",
    )
    .unwrap();
}

struct ReleaseSaveGate(Arc<SaveGate>);

impl Drop for ReleaseSaveGate {
    fn drop(&mut self) {
        self.0.release();
    }
}

#[tokio::test]
async fn production_restore_shutdown_waits_for_owned_commit_after_client_disconnect() {
    let fixture = Fixture::new();
    let id = create_full(&fixture).await;
    mutate(&fixture);
    let gate = Arc::new(SaveGate::default());
    let release = ReleaseSaveGate(gate.clone());
    *fixture.credentials.api_save_gate.lock().unwrap() = Some(gate.clone());
    let (base, mut server) = fixture.start_http().await;
    let sending = reqwest::Client::new()
        .post(format!("{base}/api/backups/{id}/restore"))
        .json(&restore_body())
        .timeout(Duration::from_secs(10))
        .send();
    let request = tokio::spawn(sending);
    tokio::time::timeout(Duration::from_secs(5), async {
        while !gate.entered.load(std::sync::atomic::Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    fixture.server.inner.shutdown.cancel();
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut server)
            .await
            .is_err()
    );
    drop(release);
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(!is_restoring(&fixture.server));
    assert_eq!(fixture.server.inner.core.read_ui_language().unwrap(), "en");
    assert_eq!(
        fixture.credentials.load("api"),
        Some(String::from("archived-model-key"))
    );
    assert_eq!(
        fs::read_to_string(fixture.workspace.join("notes.md")).unwrap(),
        "original workspace content"
    );
    let reopened = Core::persistent(
        ModelConfig {
            api_key: None,
            base_url: String::from("http://127.0.0.1:1/v1"),
            default_model: String::from("fixture-model"),
        },
        &fixture.data.join("threads.sqlite3"),
    )
    .unwrap();
    assert_eq!(
        reopened.backup_snapshot(MAX_FILE_BYTES).unwrap(),
        fixture
            .server
            .inner
            .core
            .backup_snapshot(MAX_FILE_BYTES)
            .unwrap()
    );
}

#[tokio::test]
async fn production_restore_finishes_after_http_disconnect_and_blocks_new_operations() {
    let fixture = Fixture::new();
    let id = create_full(&fixture).await;
    mutate(&fixture);
    let gate = Arc::new(SaveGate::default());
    let release = ReleaseSaveGate(gate.clone());
    *fixture.credentials.api_save_gate.lock().unwrap() = Some(gate.clone());
    let (base, server) = fixture.start_http().await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let sending = client
        .post(format!("{base}/api/backups/{id}/restore"))
        .json(&restore_body())
        .send();
    let request = tokio::spawn(sending);
    tokio::time::timeout(Duration::from_secs(5), async {
        while !gate.entered.load(std::sync::atomic::Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    assert_eq!(
        fs::read_to_string(fixture.workspace.join("notes.md")).unwrap(),
        "original workspace content"
    );
    for (endpoint, status) in [
        ("/healthz", StatusCode::OK),
        ("/api/envs", StatusCode::CONFLICT),
    ] {
        assert_eq!(
            client
                .get(format!("{base}{endpoint}"))
                .send()
                .await
                .unwrap()
                .status(),
            status
        );
    }
    let error = fixture
        .server
        .dispatch("thread/list", json!({}))
        .await
        .err()
        .expect("App Protocol must reject new operations during restore");
    assert_eq!(
        (error.code, error.message),
        (-32000, qwenpaw_core::CoreError::RestoreBusy.to_string())
    );
    assert_eq!(
        client
            .post(format!("{base}/api/backups/{id}/restore"))
            .json(&restore_body())
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    drop(release);
    tokio::time::timeout(Duration::from_secs(5), async {
        while is_restoring(&fixture.server) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(fixture.server.inner.core.read_ui_language().unwrap(), "en");
    assert_eq!(
        fixture.server.inner.core.backup_model_config().api_key,
        Some(String::from("archived-model-key"))
    );
    assert_eq!(
        fixture.credentials.load("api"),
        Some(String::from("archived-model-key"))
    );
    assert!(fixture.server.inner.core.operation_guard().is_ok());
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Compare every unselected domain for each scope.
async fn production_restore_custom_scopes_preserve_unselected_files_settings_and_credentials() {
    for scope in ["agents", "skills", "secrets", "none"] {
        let fixture = Fixture::new();
        let id = create_full(&fixture).await;
        mutate(&fixture);
        let project = fixture.directory.path().join("current-project");
        fs::create_dir(&project).unwrap();
        let project = project.canonicalize().unwrap();
        fixture
            .server
            .inner
            .core
            .write_preferred_workspace(&project)
            .unwrap();
        *fixture
            .server
            .inner
            .desktop_workspace
            .as_ref()
            .unwrap()
            .selected
            .write()
            .await = project.clone();
        let before = fixture
            .server
            .inner
            .core
            .backup_snapshot(MAX_FILE_BYTES)
            .unwrap();
        let mut registry: Value =
            serde_json::from_slice(&fs::read(fixture.data.join("models/registry.json")).unwrap())
                .unwrap();
        let (base, server) = fixture.start_http().await;
        let response = reqwest::Client::new()
            .post(format!("{base}/api/backups/{id}/restore"))
            .json(
                &json!({"mode": "custom", "include_agents": scope == "agents",
                "agent_ids": ["default"], "include_global_config": false,
                "include_skill_pool": scope == "skills", "include_secrets": scope == "secrets"}),
            )
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .unwrap();
        let status = response.status();
        let body = response.json::<Value>().await.unwrap();
        assert_eq!(
            (status, body),
            (
                StatusCode::OK,
                json!({"ok": true, "preserved_local_keys": []})
            ),
            "{scope}"
        );
        assert_eq!(
            fs::read_to_string(fixture.workspace.join("notes.md")).unwrap(),
            if scope == "agents" {
                "original workspace content"
            } else {
                "current workspace content"
            },
            "{scope}"
        );
        assert_eq!(
            fs::read_to_string(fixture.data.join("skill_pool/fixture/SKILL.md")).unwrap(),
            if scope == "skills" {
                "archived skill"
            } else {
                "current skill"
            },
            "{scope}"
        );
        let key = if scope == "secrets" {
            "archived-model-key"
        } else {
            "current-model-key"
        };
        assert_eq!(
            fixture.credentials.load("api"),
            Some(key.to_owned()),
            "{scope}"
        );
        assert_eq!(
            fixture.server.inner.core.backup_model_config().api_key,
            Some(key.to_owned()),
            "{scope}"
        );
        assert_eq!(fixture.server.inner.core.read_ui_language().unwrap(), "zh");
        assert_eq!(
            fixture
                .server
                .inner
                .core
                .read_preferred_workspace()
                .unwrap(),
            Some(project.to_string_lossy().into_owned())
        );
        assert_eq!(
            *fixture
                .server
                .inner
                .desktop_workspace
                .as_ref()
                .unwrap()
                .selected
                .read()
                .await,
            project
        );
        if scope == "secrets" {
            registry["providers"]["openai-compatible"]["api_key_configured"] = json!(true);
            registry["revision"] = json!(registry["revision"].as_u64().unwrap() + 1);
        }
        let current_registry: Value =
            serde_json::from_slice(&fs::read(fixture.data.join("models/registry.json")).unwrap())
                .unwrap();
        assert_eq!(current_registry, registry, "{scope}");
        if scope == "none" || scope == "skills" {
            assert_eq!(
                fixture
                    .server
                    .inner
                    .core
                    .backup_snapshot(MAX_FILE_BYTES)
                    .unwrap(),
                before
            );
        }
        fixture.server.inner.shutdown.cancel();
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn production_restore_selects_only_directories_present_in_restored_workspace() {
    for archived_directory in [false, true] {
        let fixture = Fixture::new();
        let project = fixture.workspace.join("project");
        fs::create_dir(&project).unwrap();
        if archived_directory {
            fs::write(project.join("notes"), "archived project").unwrap();
        }
        fixture
            .server
            .inner
            .core
            .write_preferred_workspace(&project)
            .unwrap();
        let id = create_full(&fixture).await;
        // Empty directories have no archive payload; this current file must
        // not make a disappeared directory eligible after tree replacement.
        fs::write(project.join("current"), "current project").unwrap();
        let (base, server) = fixture.start_http().await;
        let response = reqwest::Client::new()
            .post(format!("{base}/api/backups/{id}/restore"))
            .json(&restore_body())
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{:?}",
            response.text().await
        );
        let expected = if archived_directory {
            project
        } else {
            fixture.workspace.clone()
        };
        assert!(expected.is_dir());
        assert_eq!(
            fixture
                .server
                .inner
                .core
                .read_preferred_workspace()
                .unwrap(),
            Some(expected.to_string_lossy().into_owned())
        );
        assert_eq!(
            *fixture
                .server
                .inner
                .desktop_workspace
                .as_ref()
                .unwrap()
                .selected
                .read()
                .await,
            expected
        );
        fixture.server.inner.shutdown.cancel();
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn production_restore_full_http_replaces_selected_files_secrets_and_core_then_reopens() {
    let fixture = Fixture::new();
    let original_thread = fixture
        .server
        .inner
        .core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(fixture.workspace.to_string_lossy().into_owned()),
        })
        .await
        .unwrap()
        .thread
        .id;
    let id = create_full(&fixture).await;
    mutate(&fixture);
    fixture
        .server
        .inner
        .core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(fixture.workspace.to_string_lossy().into_owned()),
        })
        .await
        .unwrap();
    let (base, server) = fixture.start_http().await;
    let response = reqwest::Client::new()
        .post(format!("{base}/api/backups/{id}/restore"))
        .json(&restore_body())
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.json::<Value>().await.unwrap();
    assert_eq!(
        (status, body),
        (
            StatusCode::OK,
            json!({"ok": true, "preserved_local_keys": []})
        )
    );
    assert_eq!(
        fs::read_to_string(fixture.workspace.join("notes.md")).unwrap(),
        "original workspace content"
    );
    assert_eq!(
        fs::read_to_string(fixture.data.join("skill_pool/fixture/SKILL.md")).unwrap(),
        "archived skill"
    );
    assert_eq!(
        fixture.credentials.load("api"),
        Some(String::from("archived-model-key"))
    );
    assert_eq!(
        fixture.server.inner.core.backup_model_config().api_key,
        Some(String::from("archived-model-key"))
    );
    assert_eq!(fixture.server.inner.core.read_ui_language().unwrap(), "en");
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .list_threads(ThreadListParams::default())
            .await
            .data
            .into_iter()
            .map(|thread| thread.id)
            .collect::<Vec<_>>(),
        vec![original_thread]
    );
    let snapshot = fixture
        .server
        .inner
        .core
        .backup_snapshot(MAX_FILE_BYTES)
        .unwrap();
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
    assert_eq!(reopened.backup_snapshot(MAX_FILE_BYTES).unwrap(), snapshot);
    assert!(!is_restoring(&fixture.server));
}

#[tokio::test]
async fn production_restore_http_rolls_back_after_credential_write_failure() {
    let fixture = Fixture::new();
    seed_restore_mail(&fixture, "archived").await;
    let id = create_full(&fixture).await;
    mutate(&fixture);
    seed_restore_mail(&fixture, "current").await;
    let before = fixture
        .server
        .inner
        .core
        .backup_snapshot(MAX_FILE_BYTES)
        .unwrap();
    let credentials = fixture.credentials.values.lock().unwrap().clone();
    fixture
        .credentials
        .api_failures
        .store(1, std::sync::atomic::Ordering::SeqCst);
    let (base, server) = fixture.start_http().await;
    let response = reqwest::Client::new()
        .post(format!("{base}/api/backups/{id}/restore"))
        .json(&restore_body())
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"detail": "Credential restore failed; original values restored"})
    );
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .backup_snapshot(MAX_FILE_BYTES)
            .unwrap(),
        before
    );
    assert_eq!(*fixture.credentials.values.lock().unwrap(), credentials);
    assert_eq!(
        fs::read_to_string(fixture.workspace.join("notes.md")).unwrap(),
        "current workspace content"
    );
    assert_eq!(
        fs::read_to_string(fixture.data.join("skill_pool/fixture/SKILL.md")).unwrap(),
        "current skill"
    );
    assert!(!is_restoring(&fixture.server));
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

async fn seed_restore_mail(fixture: &Fixture, label: &str) {
    let key = crate::desktop_agents::context_for_agent(&fixture.server, "default")
        .await
        .unwrap()
        .data_key;
    let mut agents = BTreeMap::new();
    for actor in ["default", "historical"] {
        agents.insert(
            actor,
            json!({
                "whitelist":{"same@example.com":{"remark":label,"display_name":actor}},
                "blacklist":{},"pending":[],"approved_replay":[{
                    "sender_address":"same@example.com","agent_id":actor,"display_name":actor,
                    "subject":label,"body_preview":label,"timestamp":1.0,"remark":label,
                    "uid":1,"date":"2026-09-09","messages":[{"uid":1,"subject":label}]
                }]
            }),
        );
    }
    fixture
        .server
        .inner
        .core
        .write_mail_access_control_data(
            &json!({"version":2,"workspaces":[{"data_key":key,"agents":agents}]}).to_string(),
        )
        .unwrap();
}

#[tokio::test]
async fn production_restore_shutdown_retries_incomplete_inverse_once_and_retains_failed_recovery() {
    for failures in [3, 4] {
        let fixture = Fixture::new();
        let id = create_full(&fixture).await;
        mutate(&fixture);
        let before = fixture
            .server
            .inner
            .core
            .backup_snapshot(MAX_FILE_BYTES)
            .unwrap();
        let original_keys = fixture.credentials.values.lock().unwrap().clone();
        fixture
            .credentials
            .api_failures
            .store(failures, std::sync::atomic::Ordering::SeqCst);
        let (base, server) = fixture.start_http().await;
        let response = reqwest::Client::new()
            .post(format!("{base}/api/backups/{id}/restore"))
            .json(&restore_body())
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .unwrap();
        assert_eq!(
            (response.status(), response.json::<Value>().await.unwrap()),
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"detail": "Restore rollback is incomplete; recovery data retained"})
            )
        );
        assert!(fixture.server.inner.core.operation_guard().is_err());
        fixture.server.inner.shutdown.cancel();
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(is_restoring(&fixture.server));
        let state = backup_state(&fixture.server).unwrap();
        assert_eq!(
            state.coordinator.lock().await.recovery.is_some(),
            failures == 4
        );
        assert_eq!(
            fixture.server.inner.core.operation_guard().is_err(),
            failures == 4
        );
        assert_eq!(
            fixture
                .credentials
                .api_failures
                .load(std::sync::atomic::Ordering::SeqCst),
            0
        );
        if failures == 4 {
            // A failed shutdown retry must still retain originals and the lease
            // so a later explicit cleanup attempt can finish in this process.
            restore::recover_on_shutdown(&fixture.server).await;
            assert!(state.coordinator.lock().await.recovery.is_none());
            assert!(fixture.server.inner.core.operation_guard().is_ok());
        }
        assert_eq!(
            fixture
                .server
                .inner
                .core
                .backup_snapshot(MAX_FILE_BYTES)
                .unwrap(),
            before
        );
        assert_eq!(*fixture.credentials.values.lock().unwrap(), original_keys);
        assert_eq!(
            fs::read_to_string(fixture.workspace.join("notes.md")).unwrap(),
            "current workspace content"
        );
        let reopened = Core::persistent(
            fixture.server.inner.core.backup_model_config(),
            &fixture.data.join("threads.sqlite3"),
        )
        .unwrap();
        assert_eq!(reopened.backup_snapshot(MAX_FILE_BYTES).unwrap(), before);
    }
}

#[tokio::test]
async fn production_restore_retains_failed_inverse_state_and_explicit_retry_recovers_it() {
    let fixture = Fixture::new();
    let id = create_full(&fixture).await;
    mutate(&fixture);
    let before = fixture
        .server
        .inner
        .core
        .backup_snapshot(MAX_FILE_BYTES)
        .unwrap();
    let credentials = fixture.credentials.values.lock().unwrap().clone();
    fixture
        .credentials
        .api_failures
        .store(3, std::sync::atomic::Ordering::SeqCst);
    let (base, server) = fixture.start_http().await;
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{base}/api/backups/{id}/restore"))
        .json(&restore_body())
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"detail": "Restore rollback is incomplete; recovery data retained"})
    );
    assert!(is_restoring(&fixture.server));
    assert!(fixture.server.inner.core.operation_guard().is_err());
    assert_eq!(
        client
            .get(format!("{base}/api/envs"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        client
            .get(format!("{base}/healthz"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    // Recover the previous operation before rejecting this absent new archive.
    let response = client
        .post(format!("{base}/api/backups/missing/restore"))
        .json(&restore_body())
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(!is_restoring(&fixture.server));
    assert!(fixture.server.inner.core.operation_guard().is_ok());
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .backup_snapshot(MAX_FILE_BYTES)
            .unwrap(),
        before
    );
    assert_eq!(*fixture.credentials.values.lock().unwrap(), credentials);
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}
