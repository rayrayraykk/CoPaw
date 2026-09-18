use std::sync::Mutex as SyncMutex;

use pretty_assertions::assert_eq;
use qwenpaw_core::Core;
use qwenpaw_core::ModelConfig;
use qwenpaw_protocol::ThreadStartParams;

use super::*;

#[test]
fn workspace_backup_never_exports_core_identity_markers() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("nested")).unwrap();
    let marker = super::super::desktop_agents::identity::MARKER_NAME;
    fs::write(root.path().join(marker), "local identity").unwrap();
    fs::write(
        root.path().join("nested").join(marker.to_uppercase()),
        "nested identity",
    )
    .unwrap();
    fs::write(root.path().join("notes"), "user data").unwrap();
    let mut sources = Vec::new();
    collect_sources(
        root.path(),
        "data/workspaces/writer",
        &mut sources,
        &CancellationToken::new(),
        |_| false,
    )
    .unwrap();
    assert_eq!(
        sources
            .iter()
            .map(|source| (source.archive_name.as_str(), source.path.clone()))
            .collect::<Vec<_>>(),
        vec![("data/workspaces/writer/notes", root.path().join("notes"))]
    );
}

#[derive(Default)]
struct TestCredentials {
    values: SyncMutex<BTreeMap<String, String>>,
    api_failures: std::sync::atomic::AtomicUsize,
    api_save_gate: SyncMutex<Option<Arc<SaveGate>>>,
}

#[derive(Default)]
struct SaveGate {
    entered: std::sync::atomic::AtomicBool,
    released: SyncMutex<bool>,
    changed: std::sync::Condvar,
}

impl SaveGate {
    fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.changed.notify_all();
    }

    fn wait(&self) -> anyhow::Result<()> {
        self.entered
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let (released, _) = self
            .changed
            .wait_timeout_while(
                self.released.lock().unwrap(),
                Duration::from_secs(10),
                |released| !*released,
            )
            .unwrap();
        anyhow::ensure!(*released, "Fixture credential gate timed out");
        Ok(())
    }
}

impl TestCredentials {
    fn load(&self, key: &str) -> Option<String> {
        self.values.lock().unwrap().get(key).cloned()
    }

    fn save(&self, key: &str, value: Option<&str>) {
        let mut values = self.values.lock().unwrap();
        if let Some(value) = value {
            values.insert(key.to_owned(), value.to_owned());
        } else {
            values.remove(key);
        }
    }
}

impl DesktopCredentialStore for TestCredentials {
    fn load_api_key(&self) -> anyhow::Result<Option<String>> {
        Ok(self.load("api"))
    }

    fn save_api_key(&self, value: Option<&str>) -> anyhow::Result<()> {
        self.save("api", value);
        let gate = self.api_save_gate.lock().unwrap().clone();
        if let Some(gate) = gate {
            gate.wait()?;
        }
        if self
            .api_failures
            .fetch_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |value| value.checked_sub(1),
            )
            .is_ok()
        {
            anyhow::bail!("private fixture keyring failure after write");
        }
        Ok(())
    }

    fn load_backup_signing_key(&self) -> anyhow::Result<Option<String>> {
        Ok(self.load("signing"))
    }

    fn save_backup_signing_key(&self, value: &str) -> anyhow::Result<()> {
        self.save("signing", Some(value));
        Ok(())
    }

    fn load_environment_value(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.load(&format!("env:{key}")))
    }

    fn save_environment_value(&self, key: &str, value: Option<&str>) -> anyhow::Result<()> {
        self.save(&format!("env:{key}"), value);
        Ok(())
    }

    fn load_agent_setting_secret(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.load(key))
    }

    fn save_agent_setting_secret(&self, key: &str, value: Option<&str>) -> anyhow::Result<()> {
        self.save(key, value);
        Ok(())
    }

    fn load_mcp_client_secrets(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.load(&format!("mcp:{key}")))
    }

    fn save_mcp_client_secrets(&self, key: &str, value: Option<&str>) -> anyhow::Result<()> {
        self.save(&format!("mcp:{key}"), value);
        Ok(())
    }
}

struct Fixture {
    directory: tempfile::TempDir,
    workspace: PathBuf,
    data: PathBuf,
    server: AppServer,
    credentials: Arc<TestCredentials>,
}

#[path = "desktop_environment_restore_tests.rs"]
mod environment_restore;

#[path = "desktop_backup_restore_http_tests.rs"]
mod restore_http;

#[path = "desktop_backups_browser_tests.rs"]
mod browser;

#[path = "desktop_model_backup_tests.rs"]
mod model_backup;

fn bootstrap_client(key: &str) -> qwenpaw_core::McpClientSettings {
    serde_json::from_value(json!({"key": key, "enabled": false,
        "transport": "streamable_http", "url": "https://fixture.example/mcp",
        "headers": {"Authorization": "Bearer local-inline"},
        "env": {"LOCAL": "inline-value"},
        "oauth": {"access_token": "inline-access", "refresh_token": "inline-refresh"}
    }))
    .unwrap()
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn mcp_candidate_preservation_replacement_and_clear_survive_restart_and_joint_rollback() {
    for mode in ["preserve", "replace", "clear"] {
        let fixture = Fixture::new();
        let core = &fixture.server.inner.core;
        let local = vec![bootstrap_client("bootstrap")];
        core.replace_mcp_client_settings(local.clone()).unwrap();
        // A bootstrap configuration can be authoritative while an old Desktop
        // secure-store entry is stale. Hydration must preserve the effective value.
        fixture
            .credentials
            .save("mcp:bootstrap", Some("stale credential bytes"));
        fixture.credentials.save("unrelated", Some("keep"));
        let original_keys = fixture.credentials.values.lock().unwrap().clone();
        let original = core.backup_snapshot(MAX_FILE_BYTES).unwrap();
        assert_eq!(core.read_mcp_data().unwrap(), None);
        let fields = match mode {
            "preserve" => json!({"headers": {"Authorization": "Bearer local-inline"},
                "env": {"LOCAL": "inline-value"}, "oauth_access_token": "inline-access", "oauth_refresh_token": "inline-refresh"}),
            "replace" => json!({"headers": {"Authorization": "Bearer restored"},
                "env": {"RESTORED": "new-value"}, "oauth_access_token": "new-access", "oauth_refresh_token": "new-refresh"}),
            _ => {
                json!({"headers": {}, "env": {}, "oauth_access_token": "", "oauth_refresh_token": ""})
            }
        };
        let overrides = match mode {
            "preserve" => BTreeMap::new(),
            "replace" => BTreeMap::from([(String::from("bootstrap"), Some(fields.to_string()))]),
            _ => BTreeMap::from([(String::from("bootstrap"), None)]),
        };
        let mut expected = local.clone();
        expected[0].headers = serde_json::from_value(fields["headers"].clone()).unwrap();
        expected[0].env = serde_json::from_value(fields["env"].clone()).unwrap();
        expected[0].oauth.as_mut().unwrap().access_token =
            fields["oauth_access_token"].as_str().unwrap().to_owned();
        expected[0].oauth.as_mut().unwrap().refresh_token =
            fields["oauth_refresh_token"].as_str().unwrap().to_owned();
        let mut lease = core.begin_restore(Duration::from_secs(2)).await.unwrap();
        let rollback = lease.capture_rollback(MAX_FILE_BYTES).unwrap();
        let candidate = lease.prepare_restore(&original).unwrap();
        let materialized = super::super::desktop_mcp::restore::hydrate(
            &candidate,
            &local,
            fixture.credentials.as_ref(),
            &overrides,
        )
        .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&materialized["bootstrap"]).unwrap(),
            fields
        );
        assert_eq!(candidate.mcp_client_settings(), expected);
        assert_eq!(core.mcp_client_settings(), local);
        assert_eq!(*fixture.credentials.values.lock().unwrap(), original_keys);
        let mut credential_tx = restore_credentials::CredentialRestore::prepare(
            fixture.credentials.clone(),
            materialized
                .into_iter()
                .map(|(key, value)| {
                    (
                        restore_credentials::CredentialKey::McpClient(key),
                        Some(value),
                    )
                })
                .collect(),
        )
        .unwrap();
        credential_tx.apply().unwrap();
        lease.apply(&candidate, MAX_FILE_BYTES).await.unwrap();
        drop(lease);
        let model = ModelConfig {
            api_key: None,
            base_url: String::from("http://127.0.0.1:1/v1"),
            default_model: String::from("fixture-model"),
        };
        let reopened =
            Core::persistent(model.clone(), &fixture.data.join("threads.sqlite3")).unwrap();
        // Reproduce startup ordering: bootstrap manager first, Desktop data next.
        reopened.replace_mcp_client_settings(local.clone()).unwrap();
        super::super::desktop_mcp::initialize(&reopened, fixture.credentials.as_ref()).unwrap();
        assert_eq!(reopened.mcp_client_settings(), expected);
        let mut lease = core.begin_restore(Duration::from_secs(2)).await.unwrap();
        lease.apply(&rollback, MAX_FILE_BYTES).await.unwrap();
        credential_tx.rollback().unwrap();
        assert_eq!(core.backup_snapshot(MAX_FILE_BYTES).unwrap(), original);
        assert_eq!(core.mcp_client_settings(), local);
        assert_eq!(*fixture.credentials.values.lock().unwrap(), original_keys);
        let reopened = Core::persistent(model, &fixture.data.join("threads.sqlite3")).unwrap();
        reopened.replace_mcp_client_settings(local.clone()).unwrap();
        super::super::desktop_mcp::initialize(&reopened, fixture.credentials.as_ref()).unwrap();
        assert_eq!(reopened.mcp_client_settings(), local);
    }
}

#[test]
fn mcp_candidate_rejects_sensitive_metadata_and_late_invalid_secrets_without_partial_changes() {
    let fixture = Fixture::new();
    let core = &fixture.server.inner.core;
    let local = vec![bootstrap_client("first"), bootstrap_client("second")];
    core.replace_mcp_client_settings(local.clone()).unwrap();
    let metadata = super::super::desktop_mcp::backup_data(core).unwrap();
    for bad_metadata in [
        json!({"version": 1, "clients": local}).to_string(),
        String::from("{\"version\":99,\"clients\":[]}"),
        String::from("{}"),
    ] {
        let candidate = core
            .prepare_restore(&core.backup_snapshot(MAX_FILE_BYTES).unwrap())
            .unwrap();
        candidate.write_mcp_data(&bad_metadata).unwrap();
        let before = candidate.backup_snapshot(MAX_FILE_BYTES).unwrap();
        assert!(
            super::super::desktop_mcp::restore::hydrate(
                &candidate,
                &local,
                fixture.credentials.as_ref(),
                &BTreeMap::new()
            )
            .is_err()
        );
        assert_eq!(candidate.backup_snapshot(MAX_FILE_BYTES).unwrap(), before);
        assert_eq!(candidate.mcp_client_settings(), local);
    }
    let candidate = core
        .prepare_restore(&core.backup_snapshot(MAX_FILE_BYTES).unwrap())
        .unwrap();
    candidate.write_mcp_data(&metadata).unwrap();
    let before = candidate.backup_snapshot(MAX_FILE_BYTES).unwrap();
    let overrides = BTreeMap::from([
        (String::from("first"), None),
        (String::from("second"), Some(String::from("invalid secret"))),
    ]);
    assert!(
        super::super::desktop_mcp::restore::hydrate(
            &candidate,
            &local,
            fixture.credentials.as_ref(),
            &overrides
        )
        .is_err()
    );
    assert_eq!(candidate.backup_snapshot(MAX_FILE_BYTES).unwrap(), before);
    assert_eq!(candidate.mcp_client_settings(), local);
    assert!(fixture.credentials.values.lock().unwrap().is_empty());
}

#[test]
fn mcp_candidate_can_load_an_existing_secure_value_for_a_newly_configured_client() {
    let fixture = Fixture::new();
    let core = &fixture.server.inner.core;
    core.replace_mcp_client_settings(vec![bootstrap_client("new-client")])
        .unwrap();
    let data = super::super::desktop_mcp::backup_data(core).unwrap();
    core.replace_mcp_client_settings(Vec::new()).unwrap();
    let fields = json!({"headers": {"Authorization": "Bearer already-stored"}, "env": {}, "oauth_access_token": "", "oauth_refresh_token": ""});
    fixture
        .credentials
        .save("mcp:new-client", Some(&fields.to_string()));
    let candidate = core
        .prepare_restore(&core.backup_snapshot(MAX_FILE_BYTES).unwrap())
        .unwrap();
    candidate.write_mcp_data(&data).unwrap();
    let before = fixture.credentials.values.lock().unwrap().clone();
    let plan = super::super::desktop_mcp::restore::hydrate(
        &candidate,
        &[],
        fixture.credentials.as_ref(),
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&plan["new-client"]).unwrap(),
        fields
    );
    let mut expected = bootstrap_client("new-client");
    expected.headers = serde_json::from_value(fields["headers"].clone()).unwrap();
    expected.env.clear();
    expected.oauth.as_mut().unwrap().access_token.clear();
    expected.oauth.as_mut().unwrap().refresh_token.clear();
    assert_eq!(candidate.mcp_client_settings(), vec![expected]);
    assert_eq!(*fixture.credentials.values.lock().unwrap(), before);
    assert!(core.mcp_client_settings().is_empty());
}

#[test]
fn mcp_candidate_validates_expanded_bindings_before_changing_metadata_or_runtime() {
    let fixture = Fixture::new();
    let core = &fixture.server.inner.core;
    core.replace_runtime_environment(BTreeMap::from([
        (
            String::from("QWENPAW_BINDING_URL"),
            String::from("https://fixture.example/mcp"),
        ),
        (
            String::from("QWENPAW_BINDING_TOKEN"),
            String::from("fixture-token"),
        ),
    ]))
    .unwrap();
    core.replace_mcp_client_settings(serde_json::from_value(json!([{
        "key": "remote", "enabled": true, "transport": "streamable_http",
        "url": "${QWENPAW_BINDING_URL}", "headers": {"Authorization": "Bearer ${QWENPAW_BINDING_TOKEN}"}
    }])).unwrap()).unwrap();
    let current = core.mcp_client_settings();
    let original = core.backup_snapshot(MAX_FILE_BYTES).unwrap();
    for (url, token) in [
        ("file:///fixture", "fixture-token"),
        (
            "https://fixture.example/mcp",
            "private-fixture\r\nInjected: value",
        ),
    ] {
        let candidate = core.prepare_restore(&original).unwrap();
        candidate
            .write_mcp_data(&super::super::desktop_mcp::backup_data(core).unwrap())
            .unwrap();
        candidate
            .replace_runtime_environment(BTreeMap::from([
                (String::from("QWENPAW_BINDING_URL"), url.to_owned()),
                (String::from("QWENPAW_BINDING_TOKEN"), token.to_owned()),
            ]))
            .unwrap();
        let before = candidate.backup_snapshot(MAX_FILE_BYTES).unwrap();
        assert_eq!(
            super::super::desktop_mcp::restore::hydrate(
                &candidate,
                &current,
                fixture.credentials.as_ref(),
                &BTreeMap::new(),
            ),
            Err("Restored MCP configuration is invalid")
        );
        assert_eq!(candidate.backup_snapshot(MAX_FILE_BYTES).unwrap(), before);
        assert_eq!(candidate.mcp_client_settings(), current);
    }
    assert_eq!(core.backup_snapshot(MAX_FILE_BYTES).unwrap(), original);
    assert_eq!(core.mcp_client_settings(), current);
    assert!(fixture.credentials.values.lock().unwrap().is_empty());
}

#[derive(Default)]
struct OAuthCredentials {
    value: SyncMutex<Option<qwenpaw_core::McpOAuthCredentials>>,
    reads: std::sync::atomic::AtomicUsize,
}

impl qwenpaw_core::McpOAuthCredentialStore for OAuthCredentials {
    fn load(&self, _: &str) -> Result<Option<qwenpaw_core::McpOAuthCredentials>, String> {
        self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(self.value.lock().unwrap().clone())
    }

    fn save(&self, _: &str, value: &qwenpaw_core::McpOAuthCredentials) -> Result<(), String> {
        *self.value.lock().unwrap() = Some(value.clone());
        Ok(())
    }

    fn delete(&self, _: &str) -> Result<(), String> {
        *self.value.lock().unwrap() = None;
        Ok(())
    }
}

impl Fixture {
    fn new() -> Self {
        Self::with_mcp(None)
    }

    fn with_mcp(mcp: Option<qwenpaw_core::McpManager>) -> Self {
        Self::with_data_name(mcp, ".qwenpaw-core")
    }

    fn with_data_name(mcp: Option<qwenpaw_core::McpManager>, data_name: &str) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let workspace = directory.path().join("workspace");
        let data = workspace.join(data_name);
        let console = directory.path().join("console");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&console).unwrap();
        fs::write(console.join("index.html"), "test console").unwrap();
        fs::write(workspace.join("notes.md"), "original workspace content").unwrap();
        let config = ModelConfig {
            api_key: None,
            base_url: String::from("http://127.0.0.1:1/v1"),
            default_model: String::from("fixture-model"),
        };
        let core = match mcp {
            Some(mcp) => Core::new_with_mcp(config, mcp),
            None => Core::persistent(config, &data.join("threads.sqlite3")).unwrap(),
        };
        let credentials = Arc::new(TestCredentials::default());
        let server = AppServer::new_desktop_with_stores_and_workspace(
            core,
            &console,
            String::from("backup-test-shutdown-token"),
            credentials.clone(),
            &data,
            &workspace,
        )
        .unwrap();
        Self {
            directory,
            workspace: workspace.canonicalize().unwrap(),
            data: data.canonicalize().unwrap(),
            server,
            credentials,
        }
    }

    async fn start_http(&self) -> (String, tokio::task::JoinHandle<anyhow::Result<()>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = self.server.clone();
        (base, tokio::spawn(server.run_http(listener)))
    }
}

fn request(name: &str) -> CreateBackupRequest {
    CreateBackupRequest {
        name: name.to_owned(),
        description: String::new(),
        scope: BackupScope::default(),
        agents: vec![String::from("default")],
    }
}

#[tokio::test]
async fn restore_lease_blocks_http_and_protocol_operations_without_stopping_health_checks() {
    let fixture = Fixture::new();
    let (base, server_task) = fixture.start_http().await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let original = fixture
        .server
        .inner
        .core
        .backup_snapshot(MAX_ARCHIVE_BYTES)
        .unwrap();
    let guard = fixture
        .server
        .inner
        .core
        .begin_restore(Duration::from_secs(1))
        .await
        .unwrap();
    for method in [reqwest::Method::GET, reqwest::Method::POST] {
        let endpoint = if method == reqwest::Method::GET {
            "backups"
        } else {
            "backups/jobs"
        };
        let response = client
            .request(method, format!("{base}/api/{endpoint}"))
            .json(&json!({"name": "blocked", "agents": ["default"]}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert_eq!(
            response.json::<Value>().await.unwrap(),
            json!({
                "detail": qwenpaw_core::CoreError::RestoreBusy.to_string()
            })
        );
    }
    let response = client.get(format!("{base}/healthz")).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let Err(error) = fixture.server.dispatch("thread/list", json!({})).await else {
        panic!("App Protocol must reject requests during restore");
    };
    assert_eq!(
        (error.code, error.message),
        (-32000, qwenpaw_core::CoreError::RestoreBusy.to_string())
    );
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .backup_snapshot(MAX_ARCHIVE_BYTES)
            .unwrap(),
        original
    );
    drop(guard);
    assert!(
        fixture
            .server
            .dispatch("thread/list", json!({}))
            .await
            .is_ok()
    );
    let response = client
        .get(format!("{base}/api/backups"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.json::<Value>().await.unwrap(), json!([]));
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(3), server_task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

async fn completed(server: &AppServer, job_id: &str) -> BackupJobSnapshot {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let snapshot = get_backup_job(State(server.clone()), AxumPath(job_id.to_owned()))
                .await
                .unwrap()
                .0;
            if is_terminal(&snapshot.status) {
                return snapshot;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("backup must reach a terminal state")
}

#[tokio::test]
async fn unobserved_jobs_publish_unique_signed_archives_with_core_state() {
    let fixture = Fixture::new();
    fixture.server.inner.core.write_ui_language("zh").unwrap();
    let thread = fixture
        .server
        .inner
        .core
        .start_thread(ThreadStartParams {
            workspace_root: Some(fixture.workspace.to_string_lossy().into_owned()),
            model: None,
        })
        .await
        .unwrap()
        .thread;
    let mut ids = std::collections::BTreeSet::new();
    for _ in 0..2 {
        let initial = launch_backup_job(&fixture.server, request("Full fixture"))
            .await
            .unwrap();
        let terminal = completed(&fixture.server, &initial.job_id).await;
        assert_eq!(terminal.status, "completed", "{:?}", terminal.error);
        assert!(ids.insert(terminal.backup_id.clone()));
        let mut subscriber = subscribe_job(&fixture.server, &initial.job_id)
            .await
            .unwrap();
        assert_eq!(subscriber.borrow_and_update().status, "completed");
        let key = signing_key(fixture.credentials.as_ref()).unwrap();
        let path = fixture
            .data
            .join("backups")
            .join(format!("{}.zip", terminal.backup_id));
        let validated = validate_archive(&path, &key).unwrap();
        assert!(matches!(validated.trust, ArchiveTrust::Local));
        let mut archive = ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
        let manifest: Manifest =
            read_json_entry(&mut archive, MANIFEST_FILE, 8 * 1024 * 1024).unwrap();
        assert!(
            manifest
                .entries
                .contains_key("data/workspaces/default/notes.md")
        );
        assert!(
            manifest
                .entries
                .contains_key("data/config/models/registry.json")
        );
        assert!(!manifest.entries.contains_key(SECRETS_FILE));
        assert!(
            !manifest
                .entries
                .keys()
                .any(|name| name.contains(".qwenpaw-core")
                    || name.contains(".sqlite")
                    || name.contains("/backups/"))
        );
        let state: qwenpaw_storage::StoreBackup =
            read_json_entry(&mut archive, CORE_STATE_FILE, MAX_FILE_BYTES).unwrap();
        assert_eq!(state.settings.get("ui_language"), Some(&String::from("zh")));
        assert_eq!(state.threads.len(), 1);
        assert_eq!(state.threads[0].thread, thread);
        assert!(
            active_backup_job(State(fixture.server.clone()))
                .await
                .unwrap()
                .0
                .is_none()
        );
    }
    let list = list_backups(State(fixture.server.clone())).await.unwrap().0;
    assert_eq!(list.as_array().unwrap().len(), 2);
    assert!(
        list.as_array()
            .unwrap()
            .iter()
            .all(|meta| meta.get("signature").is_none())
    );
}

#[tokio::test]
async fn cancellation_and_concurrency_do_not_publish_partial_archives() {
    let fixture = Fixture::new();
    let initial = launch_backup_job(&fixture.server, request("cancel"))
        .await
        .unwrap();
    let error = launch_backup_job(&fixture.server, request("concurrent"))
        .await
        .unwrap_err();
    assert_eq!(error.0, StatusCode::CONFLICT);
    let cancellation = cancel_backup_job(
        State(fixture.server.clone()),
        AxumPath(initial.job_id.clone()),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(cancellation.status, "cancel_requested");
    let terminal = completed(&fixture.server, &initial.job_id).await;
    assert_eq!(terminal.status, "cancelled");
    assert!(terminal.result.is_none());
    assert_eq!(
        list_backups(State(fixture.server.clone())).await.unwrap().0,
        json!([])
    );
    assert!(
        fs::read_dir(fixture.data.join("backups"))
            .unwrap()
            .next()
            .is_none()
    );
    let repeated = cancel_backup_job(State(fixture.server.clone()), AxumPath(initial.job_id))
        .await
        .unwrap()
        .0;
    assert_eq!(repeated.status, "cancelled");
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn http_export_import_conflict_and_foreign_trust_round_trip() {
    let fixture = Fixture::new();
    let (base, task) = fixture.start_http().await;
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{base}/api/backups/jobs"))
        .json(&json!({"name": "HTTP backup", "agents": ["default"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let initial: BackupJobSnapshot = response.json().await.unwrap();
    let terminal = completed(&fixture.server, &initial.job_id).await;
    assert_eq!(terminal.status, "completed");
    let stream = client
        .get(format!("{base}/api/backups/jobs/{}/events", initial.job_id))
        .send()
        .await
        .unwrap();
    assert!(
        stream.headers()[CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/event-stream")
    );
    let text = tokio::time::timeout(Duration::from_secs(2), stream.text())
        .await
        .unwrap()
        .unwrap();
    let snapshot: Value =
        serde_json::from_str(text.trim().strip_prefix("data: ").unwrap()).unwrap();
    assert_eq!(snapshot["status"], "completed");
    let export = client
        .get(format!("{base}/api/backups/{}/export", terminal.backup_id))
        .send()
        .await
        .unwrap();
    assert_eq!(export.status(), StatusCode::OK);
    assert_eq!(export.headers()[CONTENT_TYPE], "application/zip");
    let bytes = export.bytes().await.unwrap().to_vec();
    let upload = |data: Vec<u8>| {
        reqwest::multipart::Form::new().part(
            "file",
            reqwest::multipart::Part::bytes(data)
                .file_name("fixture.zip")
                .mime_str("application/zip")
                .unwrap(),
        )
    };
    let response = client
        .post(format!("{base}/api/backups/import"))
        .multipart(upload(bytes.clone()))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let conflict: Value = response.json().await.unwrap();
    assert_eq!(conflict["detail"], "backup_conflict");
    let token = conflict["pending_token"].as_str().unwrap().to_owned();
    let response = client
        .post(format!("{base}/api/backups/import"))
        .multipart(reqwest::multipart::Form::new().text("pending_token", token.clone()))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = client
        .post(format!("{base}/api/backups/import"))
        .multipart(reqwest::multipart::Form::new().text("pending_token", token))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let foreign = Fixture::new();
    let (foreign_base, foreign_task) = foreign.start_http().await;
    for (mode, expected) in [
        (None, StatusCode::BAD_REQUEST),
        (Some("legacy"), StatusCode::BAD_REQUEST),
        (Some("foreign"), StatusCode::OK),
    ] {
        let form = match mode {
            None => upload(bytes.clone()),
            Some(mode) => upload(bytes.clone()).text("trust_mode", mode),
        };
        let response = client
            .post(format!("{foreign_base}/api/backups/import"))
            .multipart(form)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
        let body: Value = response.json().await.unwrap();
        if expected == StatusCode::BAD_REQUEST {
            assert_eq!(body["detail"]["code"], "backup_signature_mismatch");
        } else {
            assert_eq!(body["accepted_via_trust"], true);
            assert!(body.get("signature").is_none());
        }
    }
    let key = signing_key(foreign.credentials.as_ref()).unwrap();
    let validated = validate_archive(
        &foreign
            .data
            .join("backups")
            .join(format!("{}.zip", terminal.backup_id)),
        &key,
    )
    .unwrap();
    assert!(matches!(validated.trust, ArchiveTrust::Local));
    assert_eq!(validated.meta.accepted_via_trust, Some(true));
    let deleted = client
        .post(format!("{base}/api/backups/delete"))
        .json(&json!({"ids": [terminal.backup_id, "absent"]}))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        deleted,
        json!({"deleted": [terminal.backup_id], "failed": [{"id": "absent", "reason": "not found"}]})
    );
    fixture.server.inner.shutdown.cancel();
    foreign.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), foreign_task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn explicit_secret_scope_captures_actual_credential_keys() {
    let fixture = Fixture::new();
    fixture.credentials.save("api", Some("model-secret"));
    fixture
        .server
        .inner
        .core
        .set_runtime_api_key(Some(String::from("model-secret")))
        .unwrap();
    fixture.credentials.save("env:FIXTURE", Some("env-secret"));
    fixture
        .credentials
        .save("agent.default.mail-auth-code", Some("mail-secret"));
    fixture
        .server
        .inner
        .core
        .write_environment_keys(&[String::from("FIXTURE")])
        .unwrap();
    let mut request = request("secrets");
    request.scope.include_secrets = true;
    request.scope.include_agents = false;
    let initial = launch_backup_job(&fixture.server, request).await.unwrap();
    let terminal = completed(&fixture.server, &initial.job_id).await;
    assert_eq!(terminal.status, "completed");
    let path = fixture
        .data
        .join("backups")
        .join(format!("{}.zip", terminal.backup_id));
    let mut archive = ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    let secrets: Value = read_json_entry(&mut archive, SECRETS_FILE, MAX_FILE_BYTES).unwrap();
    assert_eq!(
        secrets,
        json!({"version": 1, "api_key": "model-secret", "environment": {"FIXTURE": "env-secret"},
        "agent_settings": {"agent.default.mail-auth-code": "mail-secret"}, "mcp_clients": {}, "model_providers": {},
        "oauth": {"version": 1, "clients": {}}})
    );
    assert!(
        !secrets
            .to_string()
            .contains(&fixture.credentials.load("signing").unwrap())
    );
}

#[test]
fn portable_archive_paths_reject_traversal_and_windows_aliases() {
    for path in [
        "../file",
        "data/../file",
        "/absolute",
        "C:/absolute",
        "data\\file",
        "data/file:stream",
        "data/./file",
        "data//file",
        "data/NUL.txt",
        "data/COM1",
        "data/file.",
        "data/file ",
        "data/line\nfile",
    ] {
        assert!(validate_archive_name(path).is_err(), "{path}");
    }
    for path in [
        "data/workspaces/default/notes.md",
        "data/skill_pool/技能/SKILL.md",
        "meta.json",
    ] {
        assert_eq!(validate_archive_name(path), Ok(()));
    }
}

#[tokio::test]
async fn secret_scope_controls_oauth_reads_and_archive_credentials_can_be_restored_and_rolled_back()
{
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("mcp.json");
    fs::write(
        &config,
        serde_json::to_vec(&json!({"clients": {"remote": {
            "transport": "streamable_http", "url": "https://resource.example/mcp", "oauth": {}
        }}}))
        .unwrap(),
    )
    .unwrap();
    let value = json!({
        "issuer": "https://auth.example", "resource": "https://resource.example/mcp",
        "clientId": "client", "authorizationEndpoint": "https://auth.example/authorize",
        "tokenEndpoint": "https://auth.example/token", "scope": "files:read",
        "accessToken": "access-secret", "refreshToken": "refresh-secret", "expiresAt": 0.0
    });
    let store = Arc::new(OAuthCredentials::default());
    *store.value.lock().unwrap() = Some(serde_json::from_value(value.clone()).unwrap());
    let mcp = qwenpaw_core::McpManager::from_path_with_oauth_store(&config, store.clone()).unwrap();
    let fixture = Fixture::with_mcp(Some(mcp));
    store.reads.store(0, std::sync::atomic::Ordering::SeqCst);
    let mut exported = None;
    for include in [false, true] {
        let mut req = request("oauth scope");
        req.scope.include_secrets = include;
        let job = launch_backup_job(&fixture.server, req).await.unwrap();
        let terminal = completed(&fixture.server, &job.job_id).await;
        assert_eq!(terminal.status, "completed", "{:?}", terminal.error);
        let path = fixture
            .data
            .join("backups")
            .join(format!("{}.zip", terminal.backup_id));
        let mut archive = ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
        if include {
            let secrets: SecretSnapshot =
                read_json_entry(&mut archive, SECRETS_FILE, MAX_FILE_BYTES).unwrap();
            assert_eq!(
                serde_json::to_value(&secrets.oauth).unwrap(),
                json!({"version": 1, "clients": {"remote": value}})
            );
            exported = secrets.oauth;
            assert_eq!(store.reads.load(std::sync::atomic::Ordering::SeqCst), 1);
        } else {
            assert!(archive.by_name(SECRETS_FILE).is_err());
            assert_eq!(store.reads.load(std::sync::atomic::Ordering::SeqCst), 0);
        }
    }
    *store.value.lock().unwrap() = None;
    let core = &fixture.server.inner.core;
    let candidate = core
        .prepare_restore(&core.backup_snapshot(MAX_FILE_BYTES).unwrap())
        .unwrap();
    let guard = core.begin_restore(Duration::from_secs(1)).await.unwrap();
    let mut restore = guard
        .prepare_oauth_restore(&candidate, &exported.unwrap())
        .unwrap();
    restore.apply().unwrap();
    assert_eq!(
        serde_json::to_value(store.value.lock().unwrap().as_ref()).unwrap(),
        value
    );
    restore.rollback().unwrap();
    assert_eq!(*store.value.lock().unwrap(), None);
}

#[tokio::test]
async fn an_empty_workspace_keeps_its_agent_metadata_without_global_config() {
    let fixture = Fixture::new();
    for entry in fs::read_dir(&fixture.workspace).unwrap() {
        let path = entry.unwrap().path();
        if path == fixture.data
            || path
                .file_name()
                .is_some_and(|name| name == super::super::desktop_agents::identity::MARKER_NAME)
        {
            continue;
        }
        if path.is_dir() {
            fs::remove_dir_all(path).unwrap();
        } else {
            fs::remove_file(path).unwrap();
        }
    }
    let mut req = request("empty workspace");
    req.scope.include_global_config = false;
    req.scope.include_skill_pool = false;
    let job = launch_backup_job(&fixture.server, req).await.unwrap();
    let terminal = completed(&fixture.server, &job.job_id).await;
    assert_eq!(terminal.status, "completed", "{:?}", terminal.error);
    let path = fixture
        .data
        .join("backups")
        .join(format!("{}.zip", terminal.backup_id));
    let mut archive = ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    assert_eq!(
        archive
            .file_names()
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from([
            AGENT_STATE_FILE,
            CORE_STATE_FILE,
            META_FILE,
            MANIFEST_FILE
        ])
    );
    let agent_snapshot: super::super::desktop_agents::AgentBackupSnapshot =
        read_json_entry(&mut archive, AGENT_STATE_FILE, MAX_FILE_BYTES).unwrap();
    assert_eq!(agent_snapshot.agents.len(), 1);
    assert_eq!(agent_snapshot.agents[0].id, "default");
    let detail = get_backup(State(fixture.server.clone()), AxumPath(terminal.backup_id))
        .await
        .unwrap()
        .0;
    assert_eq!(
        detail["workspace_stats"],
        json!({"default": {"files": 0, "size": 0, "name": "QwenPaw"}})
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn bootstrap_mcp_backup_keeps_effective_config_and_secrets_in_their_independent_scopes() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("bootstrap-mcp.json");
    fs::write(&config, serde_json::to_vec(&json!({"clients": {"bootstrap": {
        "enabled": false, "transport": "streamable_http", "url": "https://fixture.example/mcp",
        "headers": {"Authorization": "Bearer hidden-header-secret"},
        "env": {"FIXTURE_VALUE": "hidden-env-secret"},
        "oauth": {"accessToken": "hidden-access-secret", "refreshToken": "hidden-refresh-secret"}
    }}})).unwrap()).unwrap();
    let oauth = Arc::new(OAuthCredentials::default());
    let manager =
        qwenpaw_core::McpManager::from_path_with_oauth_store(&config, oauth.clone()).unwrap();
    let source = Fixture::with_mcp(Some(manager));
    let original = source
        .server
        .inner
        .core
        .backup_snapshot(MAX_FILE_BYTES)
        .unwrap();
    assert_eq!(source.server.inner.core.read_mcp_data().unwrap(), None);
    let effective = source.server.inner.core.mcp_client_settings();
    let mut sanitized = effective.clone();
    for client in &mut sanitized {
        client.headers.clear();
        client.env.clear();
        let settings = client.oauth.as_mut().unwrap();
        settings.access_token.clear();
        settings.refresh_token.clear();
    }
    for globals in [false, true] {
        for include in [false, true] {
            oauth.reads.store(0, std::sync::atomic::Ordering::SeqCst);
            let mut requested = request("bootstrap scope");
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
            let secrets = if include {
                let secrets: SecretSnapshot =
                    read_json_entry(&mut archive, SECRETS_FILE, MAX_FILE_BYTES).unwrap();
                let serialized: Value =
                    serde_json::from_str(&secrets.mcp_clients["bootstrap"]).unwrap();
                assert_eq!(
                    serialized,
                    json!({
                        "headers": {"Authorization": "Bearer hidden-header-secret"},
                        "env": {"FIXTURE_VALUE": "hidden-env-secret"},
                        "oauth_access_token": "hidden-access-secret", "oauth_refresh_token": "hidden-refresh-secret"
                    })
                );
                Some(secrets)
            } else {
                assert!(archive.by_name(SECRETS_FILE).is_err());
                None
            };
            if globals {
                let state: qwenpaw_storage::StoreBackup =
                    read_json_entry(&mut archive, CORE_STATE_FILE, MAX_FILE_BYTES).unwrap();
                let stored: Value =
                    serde_json::from_str(&state.settings["desktop_mcp_data"]).unwrap();
                assert_eq!(stored, json!({"version": 1, "clients": sanitized}));
                assert!(!serde_json::to_string(&state).unwrap().contains("hidden-"));
                let destination = Fixture::new();
                let mut lease = destination
                    .server
                    .inner
                    .core
                    .begin_restore(Duration::from_secs(2))
                    .await
                    .unwrap();
                let rollback = lease.capture_rollback(MAX_FILE_BYTES).unwrap();
                let candidate = lease.prepare_restore(&state).unwrap();
                let plan = secrets::plan_restore(
                    secrets.as_ref(),
                    include,
                    false,
                    &std::collections::BTreeSet::new(),
                )
                .unwrap();
                let credentials = restore_credentials::CandidateCredentials {
                    live: destination.credentials.as_ref(),
                    replacements: &plan.credentials,
                };
                super::super::desktop_mcp::initialize(&candidate, &credentials).unwrap();
                let expected = if include { &effective } else { &sanitized };
                assert_eq!(&candidate.mcp_client_settings(), expected);
                lease.apply(&candidate, MAX_FILE_BYTES).await.unwrap();
                assert_eq!(
                    &destination.server.inner.core.mcp_client_settings(),
                    expected
                );
                lease.apply(&rollback, MAX_FILE_BYTES).await.unwrap();
                assert_eq!(
                    destination.server.inner.core.mcp_client_settings(),
                    Vec::new()
                );
            } else {
                assert!(archive.by_name(CORE_STATE_FILE).is_err());
            }
            assert_eq!(
                oauth.reads.load(std::sync::atomic::Ordering::SeqCst),
                usize::from(include)
            );
            assert_eq!(
                source
                    .server
                    .inner
                    .core
                    .backup_snapshot(MAX_FILE_BYTES)
                    .unwrap(),
                original
            );
            assert_eq!(source.server.inner.core.mcp_client_settings(), effective);
        }
    }
}

#[tokio::test]
async fn agent_backup_includes_the_real_checkpoint_state_and_snapshot_bytes() {
    for data_name in [".qwenpaw-core", "custom-core-data"] {
        assert_checkpoint_backup(data_name).await;
    }
}

async fn assert_checkpoint_backup(data_name: &str) {
    let fixture = Fixture::with_data_name(None, data_name);
    let recovery_dir = fixture.workspace.join(".qwenpaw-restore-retained-fixture");
    fs::create_dir(&recovery_dir).unwrap();
    fs::write(
        recovery_dir.join("original"),
        "must not recursively archive recovery data",
    )
    .unwrap();
    let thread = fixture
        .server
        .inner
        .core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(fixture.workspace.to_string_lossy().into_owned()),
        })
        .await
        .unwrap()
        .thread;
    let (base, task) = fixture.start_http().await;
    let response = reqwest::Client::new().post(format!("{base}/api/workspace/checkpoints/snapshot"))
        .json(&json!({"session_id": thread.id, "user_id": "desktop", "channel": "console", "name": "before backup"}))
        .send().await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    let (state_dir, _) =
        super::super::desktop_checkpoints::backup_sources(&fixture.server, &fixture.workspace)
            .await
            .unwrap();
    fs::write(
        state_dir.join("snapshots/orphan.zip"),
        "not part of checkpoint state",
    )
    .unwrap();
    let mut req = request("with checkpoints");
    req.scope.include_global_config = false;
    let job = launch_backup_job(&fixture.server, req).await.unwrap();
    let terminal = completed(&fixture.server, &job.job_id).await;
    assert_eq!(terminal.status, "completed", "{:?}", terminal.error);
    let path = fixture
        .data
        .join("backups")
        .join(format!("{}.zip", terminal.backup_id));
    let mut archive = ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    let manifest: Manifest = read_json_entry(&mut archive, MANIFEST_FILE, 8 * 1024 * 1024).unwrap();
    assert!(
        !manifest
            .entries
            .keys()
            .any(|name| name.contains(".qwenpaw-restore-"))
    );
    let names = manifest
        .entries
        .keys()
        .filter(|name| name.starts_with(CHECKPOINT_PREFIX))
        .collect::<Vec<_>>();
    assert!(!names.iter().any(|name| name.ends_with("orphan.zip")));
    assert!(names.iter().any(|name| name.contains("/snapshots/")));
    for name in names {
        let relative = name.strip_prefix("data/checkpoints/default/").unwrap();
        let expected = fs::read(state_dir.join(relative)).unwrap();
        let mut actual = Vec::new();
        archive
            .by_name(name)
            .unwrap()
            .read_to_end(&mut actual)
            .unwrap();
        assert_eq!(actual, expected);
        if relative.starts_with("snapshots/") {
            let nested = ZipArchive::new(std::io::Cursor::new(&actual)).unwrap();
            assert!(
                !nested
                    .file_names()
                    .any(|name| name.contains(".qwenpaw-restore-"))
            );
            assert!(
                !nested
                    .file_names()
                    .any(|name| name.starts_with(&format!("files/{data_name}/"))),
                "checkpoint must not copy live Core control data"
            );
        }
    }
    assert_rejects_control_checkpoint(&fixture, &base, &state_dir, &thread.id, data_name).await;
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

async fn assert_rejects_control_checkpoint(
    fixture: &Fixture,
    base: &str,
    state_dir: &Path,
    thread_id: &str,
    data_name: &str,
) {
    let state_path = state_dir.join("state.json");
    let mut state: Value = serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
    let old_commit = state["entries"][0]["commit"].as_str().unwrap();
    let input = fs::read(state_dir.join(format!("snapshots/{old_commit}.zip"))).unwrap();
    let mut original = ZipArchive::new(std::io::Cursor::new(input)).unwrap();
    let mut writer = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for index in 0..original.len() {
        let mut entry = original.by_index(index).unwrap();
        writer
            .start_file(entry.name(), SimpleFileOptions::default())
            .unwrap();
        std::io::copy(&mut entry, &mut writer).unwrap();
    }
    let upper = data_name.to_uppercase();
    let name = if fixture.workspace.join(&upper).canonicalize().ok().as_ref() == Some(&fixture.data)
    {
        upper.as_str()
    } else {
        data_name
    };
    writer
        .start_file(
            format!("files/{name}/threads.sqlite3"),
            SimpleFileOptions::default(),
        )
        .unwrap();
    writer
        .write_all(b"must never restore over a live database")
        .unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    let commit = format!("{:x}", Sha256::digest(&bytes));
    fs::write(state_dir.join(format!("snapshots/{commit}.zip")), bytes).unwrap();
    state["entries"][0]["commit"] = json!(commit);
    for head in state["heads"].as_object_mut().unwrap().values_mut() {
        *head = json!(commit);
    }
    fs::write(state_path, serde_json::to_vec(&state).unwrap()).unwrap();
    let before = fixture
        .server
        .inner
        .core
        .backup_snapshot(MAX_FILE_BYTES)
        .unwrap();
    for endpoint in ["restore/preview", "restore"] {
        let response = reqwest::Client::new()
            .post(format!("{base}/api/workspace/checkpoints/{endpoint}"))
            .json(&json!({"commit": commit, "session_id": thread_id, "user_id": "desktop", "channel": "console", "include_files": true, "files": [format!("{name}/threads.sqlite3")]}))
            .send().await.unwrap();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            response.json::<Value>().await.unwrap(),
            json!({"detail": "Checkpoint archive contains Core control data"})
        );
    }
    let job = launch_backup_job(&fixture.server, request("unsafe nested checkpoint"))
        .await
        .unwrap();
    let terminal = completed(&fixture.server, &job.job_id).await;
    assert_eq!(terminal.status, "failed");
    assert_eq!(
        terminal.error.as_deref(),
        Some("Checkpoint data is invalid or unsafe")
    );
    assert!(
        !fixture
            .data
            .join("backups")
            .join(format!("{}.zip", terminal.backup_id))
            .exists()
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
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn checkpoint_thread_rejection_rolls_back_all_file_swaps_and_retains_the_safety_head() {
    let fixture = Fixture::new();
    fs::write(fixture.workspace.join("old.txt"), "snapshot-only file").unwrap();
    fs::create_dir(fixture.workspace.join("nested")).unwrap();
    fs::write(
        fixture.workspace.join("nested/old.txt"),
        "snapshot-only directory",
    )
    .unwrap();
    let core = &fixture.server.inner.core;
    let thread = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(fixture.workspace.to_string_lossy().into_owned()),
        })
        .await
        .unwrap()
        .thread;
    let (base, task) = fixture.start_http().await;
    reqwest::Client::new().post(format!("{base}/api/workspace/checkpoints/snapshot"))
        .json(&json!({"session_id": thread.id, "user_id": "desktop", "channel": "console", "name": "rollback fixture"}))
        .send().await.unwrap().error_for_status().unwrap();
    let (state_dir, _) =
        super::super::desktop_checkpoints::backup_sources(&fixture.server, &fixture.workspace)
            .await
            .unwrap();
    let state_path = state_dir.join("state.json");
    let mut state: Value = serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
    let original_commit = state["entries"][0]["commit"].as_str().unwrap();
    let mut archive = ZipArchive::new(
        fs::File::open(state_dir.join(format!("snapshots/{original_commit}.zip"))).unwrap(),
    )
    .unwrap();
    let mut writer = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        if entry.name() == "thread.json" {
            let mut checkpoint: Value = serde_json::from_reader(&mut entry).unwrap();
            checkpoint["turns"] = json!([qwenpaw_protocol::Turn {
                id: String::from("invalid-turn"),
                thread_id: String::from("different-thread"),
                status: qwenpaw_protocol::TurnStatus::Completed,
                items: Vec::new(),
                error: None,
            }]);
            writer
                .start_file("thread.json", SimpleFileOptions::default())
                .unwrap();
            serde_json::to_writer(&mut writer, &checkpoint).unwrap();
        } else {
            writer.raw_copy_file(entry).unwrap();
        }
    }
    let bytes = writer.finish().unwrap().into_inner();
    let commit = format!("{:x}", Sha256::digest(&bytes));
    fs::write(state_dir.join(format!("snapshots/{commit}.zip")), bytes).unwrap();
    state["entries"][0]["commit"] = json!(commit);
    for head in state["heads"].as_object_mut().unwrap().values_mut() {
        *head = json!(commit);
    }
    fs::write(&state_path, serde_json::to_vec(&state).unwrap()).unwrap();
    fs::write(fixture.workspace.join("notes.md"), "live file must survive").unwrap();
    fs::write(fixture.workspace.join("new.txt"), "live-only file").unwrap();
    fs::remove_file(fixture.workspace.join("old.txt")).unwrap();
    fs::remove_file(fixture.workspace.join("nested/old.txt")).unwrap();
    fs::remove_dir(fixture.workspace.join("nested")).unwrap();
    let before = core.backup_snapshot(MAX_FILE_BYTES).unwrap();
    let response = reqwest::Client::new().post(format!("{base}/api/workspace/checkpoints/restore"))
        .json(&json!({"commit": commit, "session_id": thread.id, "user_id": "desktop", "channel": "console", "include_files": true, "files": ["notes.md", "old.txt", "new.txt", "nested/old.txt"]}))
        .send().await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"detail": "checkpoint is invalid: checkpoint contains invalid Turn state"})
    );
    assert_eq!(
        fs::read_to_string(fixture.workspace.join("notes.md")).unwrap(),
        "live file must survive"
    );
    assert_eq!(
        fs::read_to_string(fixture.workspace.join("new.txt")).unwrap(),
        "live-only file"
    );
    assert!(!fixture.workspace.join("old.txt").exists());
    assert!(!fixture.workspace.join("nested").exists());
    assert_eq!(core.backup_snapshot(MAX_FILE_BYTES).unwrap(), before);
    assert!(!fs::read_dir(&fixture.workspace).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".qwenpaw-restore-")
    }));
    let final_state: Value = serde_json::from_slice(&fs::read(state_path).unwrap()).unwrap();
    let safety = final_state["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["kind"] == "pre-restore")
        .unwrap();
    assert_eq!(
        final_state["heads"][safety["session_key"].as_str().unwrap()],
        safety["commit"]
    );
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

fn archive_fixture(path: &Path, files: &[(&str, &[u8])], signed: bool) {
    archive_fixture_with_scope(path, files, signed, BackupScope::default());
}

fn archive_fixture_with_scope(
    path: &Path,
    files: &[(&str, &[u8])],
    signed: bool,
    scope: BackupScope,
) {
    let mut writer = ZipWriter::new(fs::File::create(path).unwrap());
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut manifest = Manifest {
        format: String::from(FORMAT_VERSION),
        entries: BTreeMap::new(),
    };
    for (name, bytes) in files {
        write_bytes(&mut writer, name, bytes, options, &mut manifest).unwrap();
    }
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).unwrap();
    let mut meta = BackupMeta {
        id: String::from("fixture-archive"),
        name: String::from("Fixture archive"),
        description: String::new(),
        created_at: Utc::now(),
        version: String::from(FORMAT_VERSION),
        scope,
        agent_count: 1,
        qwenpaw_version: String::from("0.2.0"),
        system_info: json!({"backend": "rust-core"}),
        signature: None,
        accepted_via_trust: Some(false),
    };
    if signed {
        meta.signature = Some(sign_meta(&meta, &manifest_bytes, &[7; 32]).unwrap());
    }
    writer.start_file(MANIFEST_FILE, options).unwrap();
    writer.write_all(&manifest_bytes).unwrap();
    writer.start_file(META_FILE, options).unwrap();
    serde_json::to_writer(&mut writer, &meta).unwrap();
    writer.finish().unwrap();
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn http_import_validates_credentials_before_trust_or_publication_without_changing_live_keys()
{
    let fixture = Fixture::new();
    fixture
        .credentials
        .save("api", Some("private-fixture-secret"));
    signing_key(fixture.credentials.as_ref()).unwrap();
    let before = fixture.credentials.values.lock().unwrap().clone();
    let original = serde_json::to_value(
        collect_secret_snapshot(&fixture.server, &[String::from("default")]).unwrap(),
    )
    .unwrap();
    let (base, task) = fixture.start_http().await;
    let client = reqwest::Client::new();
    let path = fixture.directory.path().join("credential-import.zip");
    for (field, value, allowed, expected) in [
        (
            "version",
            json!(2),
            true,
            "Backup credential version or entry count is invalid",
        ),
        (
            "api_key",
            json!("private-fixture-secret\ninjected"),
            true,
            "Backup model credential is invalid",
        ),
        (
            "environment",
            json!({"BAD=NAME": "private-fixture-secret"}),
            true,
            "Backup environment credentials are invalid",
        ),
        (
            "agent_settings",
            json!({"backup-signing-key": "private-fixture-secret"}),
            true,
            "Backup Agent credential key is invalid",
        ),
        (
            "model_providers",
            json!({"openai-compatible": "alias"}),
            true,
            "Backup duplicates the default model credential",
        ),
        (
            "oauth",
            json!({"version": 2, "clients": {}}),
            true,
            "Backup OAuth credentials are invalid",
        ),
        (
            "unknown",
            json!("private-fixture-secret"),
            true,
            "Backup metadata is invalid",
        ),
        (
            "version",
            json!(1),
            false,
            "Backup credential payload is outside its declared scope",
        ),
    ] {
        let mut payload = original.clone();
        payload[field] = value;
        let bytes = serde_json::to_vec(&payload).unwrap();
        archive_fixture_with_scope(
            &path,
            &[(SECRETS_FILE, &bytes)],
            true,
            BackupScope {
                include_secrets: allowed,
                ..BackupScope::default()
            },
        );
        assert_eq!(
            validate_archive(&path, &[7; 32]).err().as_deref(),
            Some(expected)
        );
        let response = client
            .post(format!("{base}/api/backups/import"))
            .multipart(
                reqwest::multipart::Form::new()
                    .text("trust_mode", "foreign")
                    .part(
                        "file",
                        reqwest::multipart::Part::bytes(fs::read(&path).unwrap())
                            .file_name("credentials.zip"),
                    ),
            )
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.json::<Value>().await.unwrap(),
            json!({"detail": expected})
        );
        assert_eq!(*fixture.credentials.values.lock().unwrap(), before);
        assert_eq!(
            list_backups(State(fixture.server.clone())).await.unwrap().0,
            json!([])
        );
        assert!(
            backup_state(&fixture.server)
                .unwrap()
                .coordinator
                .lock()
                .await
                .pending_imports
                .is_empty()
        );
    }
    let bytes = serde_json::to_vec(&original).unwrap();
    archive_fixture_with_scope(
        &path,
        &[(SECRETS_FILE, &bytes)],
        true,
        BackupScope {
            include_agents: false,
            include_secrets: true,
            ..BackupScope::default()
        },
    );
    let mut expected = read_archive_meta(&path).unwrap();
    expected.accepted_via_trust = Some(true);
    let response = client
        .post(format!("{base}/api/backups/import"))
        .multipart(
            reqwest::multipart::Form::new()
                .text("trust_mode", "foreign")
                .part(
                    "file",
                    reqwest::multipart::Part::bytes(fs::read(&path).unwrap())
                        .file_name("credentials.zip"),
                ),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        public_meta(expected)
    );
    assert_eq!(*fixture.credentials.values.lock().unwrap(), before);
    let mut published =
        ZipArchive::new(fs::File::open(fixture.data.join("backups/fixture-archive.zip")).unwrap())
            .unwrap();
    assert_eq!(
        read_json_entry::<Value, _>(&mut published, SECRETS_FILE, MAX_FILE_BYTES).unwrap(),
        original
    );
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn invalid_persisted_secret_fails_creation_without_publishing_an_unrestorable_archive() {
    let fixture = Fixture::new();
    fixture
        .credentials
        .save("env:VALUE", Some("private-fixture-secret\0invalid"));
    fixture
        .server
        .inner
        .core
        .write_environment_keys(&[String::from("VALUE")])
        .unwrap();
    let before = fixture.credentials.values.lock().unwrap().clone();
    let mut requested = request("invalid secret");
    requested.scope.include_secrets = true;
    let job = launch_backup_job(&fixture.server, requested).await.unwrap();
    let terminal = completed(&fixture.server, &job.job_id).await;
    assert_eq!(terminal.status, "failed");
    assert_eq!(
        terminal.error.as_deref(),
        Some("Backup environment credentials are invalid")
    );
    assert_eq!(*fixture.credentials.values.lock().unwrap(), before);
    assert_eq!(
        list_backups(State(fixture.server.clone())).await.unwrap().0,
        json!([])
    );
    assert!(
        fs::read_dir(fixture.data.join("backups"))
            .unwrap()
            .next()
            .is_none()
    );
}

#[test]
fn archive_integrity_checks_precede_foreign_or_legacy_trust() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("fixture.zip");
    archive_fixture(
        &path,
        &[("data/workspaces/default/notes.md", b"original")],
        true,
    );
    assert!(matches!(
        validate_archive(&path, &[7; 32]).unwrap().trust,
        ArchiveTrust::Local
    ));
    assert!(matches!(
        validate_archive(&path, &[8; 32]).unwrap().trust,
        ArchiveTrust::Foreign
    ));
    let tampered = directory.path().join("tampered.zip");
    let mut source = ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
    let mut writer = ZipWriter::new(fs::File::create(&tampered).unwrap());
    for index in 0..source.len() {
        let entry = source.by_index(index).unwrap();
        if entry.name() == "data/workspaces/default/notes.md" {
            writer
                .start_file(entry.name(), SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"tampered").unwrap();
        } else {
            writer.raw_copy_file(entry).unwrap();
        }
    }
    writer.finish().unwrap();
    assert_eq!(
        validate_archive(&tampered, &[7; 32]).err().as_deref(),
        Some("Backup file integrity check failed")
    );
    assert!(validate_archive(&tampered, &[8; 32]).is_err());
    for names in [
        vec!["data/../outside"],
        vec![
            "data/workspaces/default/file",
            "data/workspaces/default/FILE",
        ],
        vec!["other/payload"],
    ] {
        let files = names
            .iter()
            .map(|name| (*name, b"data".as_slice()))
            .collect::<Vec<_>>();
        archive_fixture(&path, &files, true);
        assert!(validate_archive(&path, &[7; 32]).is_err(), "{names:?}");
    }
    archive_fixture(
        &path,
        &[("data/workspaces/default/notes.md", b"legacy")],
        false,
    );
    let validated = validate_archive(&path, &[7; 32]).unwrap();
    assert!(matches!(validated.trust, ArchiveTrust::Legacy));
    assert_eq!(
        require_trust(&validated.trust, None).unwrap_err().1.0["detail"]["code"],
        "backup_legacy_unsigned"
    );
    assert!(require_trust(&validated.trust, Some(TrustMode::Foreign)).is_err());
    assert!(require_trust(&validated.trust, Some(TrustMode::Legacy)).is_ok());
}

#[test]
fn archive_global_registry_must_match_selected_references_and_cannot_embed_runtime_profiles() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("registry.zip");
    let agent = json!({"id": "default", "workspace_dir": "C:\\source\\default",
        "enabled": true, "pinned": true});
    let mut profile = agent.clone();
    profile["config"] = json!({"id": "default", "workspace_dir": "C:\\source\\default"});
    let snapshot = serde_json::to_vec(&json!({"version": 1, "agents": [profile]})).unwrap();
    for kind in ["valid", "mismatch", "unexpected-config"] {
        let mut reference = agent.clone();
        match kind {
            "mismatch" => reference["workspace_dir"] = json!("D:\\different\\default"),
            "unexpected-config" => reference["config"] = json!({"name": "must not appear here"}),
            _ => {}
        }
        let registry = serde_json::to_vec(&json!({"version": 1, "agents": [reference]})).unwrap();
        archive_fixture(
            &path,
            &[
                (AGENT_STATE_FILE, &snapshot),
                (GLOBAL_AGENT_STATE_FILE, &registry),
            ],
            true,
        );
        match (kind, validate_archive(&path, &[7; 32])) {
            ("valid", Ok(validated)) => assert!(matches!(validated.trust, ArchiveTrust::Local)),
            ("mismatch", Err(error)) => assert_eq!(error, "Backup Agent snapshots disagree"),
            ("unexpected-config", Err(error)) => assert_eq!(error, "Backup metadata is invalid"),
            _ => panic!("Unexpected archive validation outcome for {kind}"),
        }
    }
}

#[test]
fn logical_payloads_count_toward_the_same_archive_entry_limit_as_workspace_files() {
    let mut manifest = Manifest {
        format: FORMAT_VERSION.to_owned(),
        entries: (0..MAX_ARCHIVE_FILES)
            .map(|index| {
                (
                    format!("data/workspaces/default/{index}"),
                    ManifestEntry {
                        size: 0,
                        sha256: format!("{:x}", Sha256::digest([])),
                    },
                )
            })
            .collect(),
    };
    let mut writer = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    assert!(
        write_bytes(
            &mut writer,
            CORE_STATE_FILE,
            b"{}",
            SimpleFileOptions::default(),
            &mut manifest
        )
        .is_err()
    );
    assert_eq!(manifest.entries.len(), MAX_ARCHIVE_FILES);
    assert!(!manifest.entries.contains_key(CORE_STATE_FILE));
}

#[test]
fn expired_tokens_remove_only_their_owned_upload() {
    let directory = tempfile::tempdir().unwrap();
    let expired = tempfile::NamedTempFile::new_in(directory.path()).unwrap();
    let expired_path = expired.path().to_owned();
    let active = tempfile::NamedTempFile::new_in(directory.path()).unwrap();
    let active_path = active.path().to_owned();
    let mut coordinator = Coordinator::default();
    coordinator.pending_imports.insert(
        String::from("expired"),
        PendingImport {
            file: expired,
            trust_mode: None,
            created_at: SystemTime::now() - PENDING_IMPORT_TTL - Duration::from_secs(1),
        },
    );
    coordinator.pending_imports.insert(
        String::from("active"),
        PendingImport {
            file: active,
            trust_mode: Some(TrustMode::Foreign),
            created_at: SystemTime::now(),
        },
    );
    cleanup_pending_imports(&mut coordinator);
    assert!(!expired_path.exists());
    assert!(active_path.exists());
    assert_eq!(
        coordinator
            .pending_imports
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec![String::from("active")]
    );
}

#[tokio::test]
async fn data_nested_agent_workspaces_remain_in_the_archive() {
    let fixture = Fixture::new();
    let (base, task) = fixture.start_http().await;
    let response = reqwest::Client::new()
        .post(format!("{base}/api/agents"))
        .json(&json!({"id": "writer", "name": "Writer"}))
        .send()
        .await
        .unwrap();
    assert!(
        response.status().is_success(),
        "{}",
        response.text().await.unwrap()
    );
    let workspaces = super::super::desktop_agents::agent_workspaces(&fixture.server)
        .await
        .unwrap();
    let (_, _, workspace) = workspaces.iter().find(|(id, _, _)| id == "writer").unwrap();
    assert!(workspace.starts_with(&fixture.data));
    fs::write(workspace.join("agent-note.md"), "auto-created workspace").unwrap();
    let mut request = request("nested workspace");
    request.agents = vec![String::from("writer")];
    request.scope.include_global_config = false;
    let initial = launch_backup_job(&fixture.server, request).await.unwrap();
    let terminal = completed(&fixture.server, &initial.job_id).await;
    assert_eq!(terminal.status, "completed", "{:?}", terminal.error);
    let path = fixture
        .data
        .join("backups")
        .join(format!("{}.zip", terminal.backup_id));
    let mut archive = ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    let manifest: Manifest = read_json_entry(&mut archive, MANIFEST_FILE, 8 * 1024 * 1024).unwrap();
    let agents: Value = read_json_entry(&mut archive, AGENT_STATE_FILE, MAX_FILE_BYTES).unwrap();
    assert_eq!(agents["version"], 2);
    assert_eq!(
        agents["agents"][0]["data_key"],
        serde_json::to_value(
            super::super::desktop_agents::context_for_agent(&fixture.server, "writer")
                .await
                .unwrap()
                .data_key
        )
        .unwrap()
    );
    assert_eq!(agents["agents"].as_array().unwrap().len(), 1);
    assert_eq!(agents["agents"][0]["id"], "writer");
    assert_eq!(agents["agents"][0]["config"]["name"], "Writer");
    assert_eq!(
        agents["agents"][0]["workspace_dir"],
        workspace.to_string_lossy().as_ref()
    );
    assert!(
        !manifest
            .entries
            .keys()
            .any(|name| name.starts_with(CONFIG_PREFIX))
    );
    assert!(
        manifest
            .entries
            .contains_key("data/workspaces/writer/agent-note.md")
    );
    assert!(
        !manifest
            .entries
            .contains_key("data/workspaces/default/notes.md")
    );
    let detail = get_backup(State(fixture.server.clone()), AxumPath(terminal.backup_id))
        .await
        .unwrap()
        .0;
    assert_eq!(detail["workspace_stats"]["writer"]["name"], "Writer");
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn backup_agent_scope_keeps_chat_and_inbox_state_without_exporting_other_agents() {
    let fixture = Fixture::new();
    let (base, task) = fixture.start_http().await;
    seed_scoped_backup_state(&fixture, &base).await;
    let registry = super::super::desktop_agents::backup_agent_snapshot(&fixture.server)
        .await
        .unwrap()
        .registry();
    let expected_registry = serde_json::to_value(&registry).unwrap();
    let original = fixture
        .server
        .inner
        .core
        .backup_snapshot(MAX_FILE_BYTES)
        .unwrap();
    let chat_key = "desktop_chat_catalog_data";
    let inbox_key = "desktop_inbox_data";
    let mail_key = "desktop_mail_access_control_data";
    let original_chats: Value = serde_json::from_str(&original.settings[chat_key]).unwrap();
    for (agent, globals) in [
        (Some("writer"), false),
        (Some("writer"), true),
        (Some("default"), false),
        (None, true),
    ] {
        let mut request = request("scoped state");
        request.agents = agent.into_iter().map(str::to_owned).collect();
        request.scope.include_agents = agent.is_some();
        request.scope.include_global_config = globals;
        let job = launch_backup_job(&fixture.server, request).await.unwrap();
        let terminal = completed(&fixture.server, &job.job_id).await;
        assert_eq!(terminal.status, "completed", "{:?}", terminal.error);
        let path = fixture
            .data
            .join("backups")
            .join(format!("{}.zip", terminal.backup_id));
        let mut archive = ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
        assert!(archive.by_name("data/config/agents/catalog.json").is_err());
        if globals {
            let actual_registry: Value =
                read_json_entry(&mut archive, GLOBAL_AGENT_STATE_FILE, MAX_FILE_BYTES).unwrap();
            assert_eq!(actual_registry, expected_registry);
            assert_eq!(actual_registry["agents"].as_array().unwrap().len(), 2);
            assert!(
                actual_registry["agents"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|agent| agent.get("config").is_none())
            );
        } else {
            assert!(archive.by_name(GLOBAL_AGENT_STATE_FILE).is_err());
        }
        let state: qwenpaw_storage::StoreBackup =
            read_json_entry(&mut archive, CORE_STATE_FILE, MAX_FILE_BYTES).unwrap();
        let mut expected: BTreeMap<String, Value> = if globals {
            original
                .settings
                .iter()
                .map(|(key, value)| (key.clone(), json!(value)))
                .collect()
        } else {
            BTreeMap::new()
        };
        if globals {
            // An empty effective MCP registry must be explicit so a full
            // restore cannot accidentally inherit the destination's clients.
            expected.insert(
                String::from("desktop_mcp_data"),
                json!("{\"version\":1,\"clients\":[]}"),
            );
        }
        assert_eq!(
            state.usage,
            original
                .usage
                .iter()
                .filter(|record| agent == Some(record.agent_id.as_str()))
                .cloned()
                .collect::<Vec<_>>()
        );
        for key in [
            chat_key,
            inbox_key,
            mail_key,
            "desktop_cron_data",
            "desktop_heartbeat_data",
        ] {
            expected.remove(key);
        }
        let mut expected_threads = Vec::new();
        if let Some(agent) = agent {
            let mut chats = original_chats.clone();
            chats["chats"].as_object_mut().unwrap().retain(|id, chat| {
                let selected = chat["agent_id"] == agent;
                if selected {
                    expected_threads.push(
                        original
                            .threads
                            .iter()
                            .find(|stored| stored.thread.id == *id)
                            .unwrap()
                            .clone(),
                    );
                }
                selected
            });
            chats["groups"]
                .as_array_mut()
                .unwrap()
                .retain(|group| group["agent_id"].as_str().unwrap_or("default") == agent);
            expected.insert(chat_key.to_owned(), chats);
            let mut inbox: Value = serde_json::from_str(&original.settings[inbox_key]).unwrap();
            inbox["events"]
                .as_array_mut()
                .unwrap()
                .retain(|event| event["agent_id"] == agent);
            inbox["traces"]
                .as_object_mut()
                .unwrap()
                .retain(|_, trace| trace["meta"]["agent_id"] == agent);
            expected.insert(inbox_key.to_owned(), inbox);
            let mut mail: Value = serde_json::from_str(&original.settings[mail_key]).unwrap();
            mail["workspaces"]
                .as_array_mut()
                .unwrap()
                .retain(|workspace| workspace["agents"].get(agent).is_some());
            expected.insert(mail_key.to_owned(), mail);
            let mut cron: Value =
                serde_json::from_str(&original.settings["desktop_cron_data"]).unwrap();
            let job_id = format!("{agent}-job");
            cron["jobs"]
                .as_array_mut()
                .unwrap()
                .retain(|job| job["id"] == job_id);
            for field in [
                "owners",
                "states",
                "history",
                "public_ids",
                "workspace_owners",
            ] {
                cron[field]
                    .as_object_mut()
                    .unwrap()
                    .retain(|id, _| id == &job_id);
            }
            if cron["owners"].as_object().unwrap().is_empty() {
                cron.as_object_mut().unwrap().remove("owners");
            }
            cron["scheduled"] = json!([job_id]);
            expected.insert(String::from("desktop_cron_data"), cron);
            if agent == "default" {
                expected_threads.extend(
                    original
                        .threads
                        .iter()
                        .filter(|stored| original_chats["chats"].get(&stored.thread.id).is_none())
                        .cloned(),
                );
                expected.insert(
                    String::from("desktop_heartbeat_data"),
                    json!(original.settings["desktop_heartbeat_data"]),
                );
            }
        }
        let actual = state
            .settings
            .iter()
            .map(|(key, value)| {
                let value = if [chat_key, inbox_key, mail_key, "desktop_cron_data"]
                    .contains(&key.as_str())
                {
                    serde_json::from_str(value).unwrap()
                } else {
                    json!(value)
                };
                (key.clone(), value)
            })
            .collect::<BTreeMap<_, _>>();
        assert_eq!(actual, expected);
        expected_threads.sort_by(|left, right| left.thread.id.cmp(&right.thread.id));
        let mut actual_threads = state.threads;
        actual_threads.sort_by(|left, right| left.thread.id.cmp(&right.thread.id));
        assert_eq!(actual_threads, expected_threads);
    }
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .backup_snapshot(MAX_FILE_BYTES)
            .unwrap(),
        original
    );
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

async fn seed_scoped_backup_state(fixture: &Fixture, base: &str) {
    let client = reqwest::Client::new();
    client
        .post(format!("{base}/api/agents"))
        .json(&json!({"id": "writer", "name": "Writer"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    for agent in ["default", "writer"] {
        let group: Value = client
            .post(format!("{base}/api/chats/groups"))
            .header("X-Agent-Id", agent)
            .json(&json!({"name": format!("{agent} group")}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        let chat: Value = client.post(format!("{base}/api/chats"))
            .header("X-Agent-Id", agent)
            .json(&json!({"name": format!("{agent} title"), "session_id": format!("{agent} session"), "user_id": "desktop", "group_id": group["id"], "meta": {"runtime_context": {"project_dir": fixture.workspace}}}))
            .send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
        client
            .put(format!("{base}/api/chats/{}", chat["id"].as_str().unwrap()))
            .header("X-Agent-Id", agent)
            .json(&json!({"pinned": true}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        super::super::desktop_inbox::append_event_with_trace(
            &fixture.server,
            super::super::desktop_inbox::NewInboxEvent {
                agent_id: agent.to_owned(),
                source_type: String::from("cron"),
                source_id: String::from("job"),
                event_type: String::from("result"),
                status: String::from("success"),
                severity: String::from("info"),
                title: format!("{agent} event"),
                body: format!("{agent} result"),
                payload: json!({"run_id": format!("{agent} run")}),
            },
            super::super::desktop_inbox::NewInboxTrace {
                run_id: format!("{agent} run"),
                status: String::from("success"),
                meta: json!({"agent_id": agent}),
                events: vec![json!({"text": format!("{agent} output")})],
                error: None,
            },
        )
        .await
        .unwrap();
    }
    let mut mail_workspaces = BTreeMap::new();
    for agent in ["default", "writer"] {
        let key = super::super::desktop_agents::context_for_agent(&fixture.server, agent)
            .await
            .unwrap()
            .data_key;
        mail_workspaces.insert(
            key.clone(),
            json!({"data_key": key, "agents": {agent: {
                "whitelist": {"person@example.com": {
                    "remark": format!("{agent} contact"), "display_name": agent}},
                "blacklist": {}, "pending": [], "approved_replay": []
            }}}),
        );
    }
    let core = &fixture.server.inner.core;
    core.write_mail_access_control_data(
        &json!({"version": 2, "workspaces": mail_workspaces.into_values().collect::<Vec<_>>()})
            .to_string(),
    )
    .unwrap();
    seed_scoped_cron_state(fixture, base).await;
    core.write_heartbeat_data(&json!({"version": 1, "config": {"enabled": false}}).to_string())
        .unwrap();
    core.write_ui_language("zh").unwrap();
    seed_scoped_backup_usage(fixture);
    let project = fixture.directory.path().join("sdk-project");
    fs::create_dir(&project).unwrap();
    core.start_thread(ThreadStartParams {
        workspace_root: Some(project.to_string_lossy().into_owned()),
        model: None,
    })
    .await
    .unwrap();
}

async fn seed_scoped_cron_state(fixture: &Fixture, base: &str) {
    let client = reqwest::Client::new();
    let mut cron = json!({"version":4,"jobs":[],"states":{},"history":{},"workspace_owners":{},
        "public_ids":{"default-job":"shared-job","writer-job":"shared-job"},
        "owners":{"writer-job":"writer"},"scheduled":["default-job","writer-job"],"active_triggers":{}});
    for agent in ["default", "writer"] {
        let mut job: Value = client
            .post(format!("{base}/api/cron/jobs"))
            .json(&json!({"name":format!("{agent} reminder"),"enabled":false,
                "task_type":"text","text":format!("{agent} result"),"schedule":{"cron":"* * * * *"},
                "dispatch":{"target":{"user_id":agent,"session_id":"same"}}}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        let id = format!("{agent}-job");
        job["id"] = json!(id);
        cron["workspace_owners"][&id] = serde_json::to_value(
            super::super::desktop_agents::context_for_agent(&fixture.server, agent)
                .await
                .unwrap()
                .data_key,
        )
        .unwrap();
        cron["jobs"].as_array_mut().unwrap().push(job);
        cron["states"][&id] = json!({"next_run_at":null,"last_run_at":null,
            "last_status":"success","last_error":null});
        cron["history"][&id] = json!([{"run_at":"2030-01-01T00:00:00Z", "status":"success",
            "error":null,"trigger":"manual"}]);
    }
    fixture
        .server
        .inner
        .core
        .write_cron_data(&cron.to_string())
        .unwrap();
}

fn seed_scoped_backup_usage(fixture: &Fixture) {
    let state = fixture
        .server
        .inner
        .core
        .backup_snapshot(MAX_FILE_BYTES)
        .unwrap();
    let chats: Value = serde_json::from_str(&state.settings["desktop_chat_catalog_data"]).unwrap();
    let store = qwenpaw_storage::ThreadStore::open(&fixture.data.join("threads.sqlite3")).unwrap();
    for stored in &state.threads {
        let id = chats["chats"][&stored.thread.id]["agent_id"]
            .as_str()
            .unwrap();
        store
            .upsert_with_usage(
                stored,
                &qwenpaw_storage::StoredUsageRecord {
                    id: format!("{id}-backup-usage"),
                    thread_id: stored.thread.id.clone(),
                    turn_id: format!("{id}-turn"),
                    agent_id: id.to_owned(),
                    data_key: Some(
                        serde_json::from_value(
                            chats["chats"][&stored.thread.id]["data_key"].clone(),
                        )
                        .unwrap(),
                    ),
                    recorded_at: 1,
                    call: qwenpaw_storage::StoredModelCall {
                        provider_id: String::from("fixture"),
                        model: String::from("fixture-model"),
                        prompt_tokens: 10,
                        completion_tokens: 20,
                        cache_read_tokens: 0,
                        cache_write_tokens: 0,
                        cache_eligible_input_tokens: 0,
                        cache_observed: false,
                        usage_observed: true,
                    },
                },
            )
            .unwrap();
    }
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn real_agent_archive_swaps_files_credentials_catalog_and_core_with_joint_rollback() {
    let fixture = Fixture::new();
    let (base, task) = fixture.start_http().await;
    seed_scoped_backup_state(&fixture, &base).await;
    let core = &fixture.server.inner.core;
    let writer = super::super::desktop_agents::backup_agent_snapshot(&fixture.server)
        .await
        .unwrap()
        .agents
        .into_iter()
        .find(|agent| agent.id == "writer")
        .unwrap();
    let writer_root = PathBuf::from(&writer.workspace_dir);
    fs::write(writer_root.join("original.txt"), "archived writer file").unwrap();
    let mail_key = "agent.writer.mail-auth-code";
    fixture
        .credentials
        .save(mail_key, Some("archived writer credential"));
    fixture.credentials.save(
        "agent.default.mail-auth-code",
        Some("keep default credential"),
    );
    let original = core.backup_snapshot(MAX_FILE_BYTES).unwrap();
    let mut requested = request("writer state restoration");
    requested.agents = vec![String::from("writer")];
    requested.scope.include_global_config = false;
    requested.scope.include_secrets = true;
    let job = launch_backup_job(&fixture.server, requested).await.unwrap();
    let terminal = completed(&fixture.server, &job.job_id).await;
    assert_eq!(terminal.status, "completed", "{:?}", terminal.error);
    let path = fixture
        .data
        .join("backups")
        .join(format!("{}.zip", terminal.backup_id));
    let mut zip = ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    let archived: qwenpaw_storage::StoreBackup =
        read_json_entry(&mut zip, CORE_STATE_FILE, MAX_FILE_BYTES).unwrap();
    let archived_agents: super::super::desktop_agents::AgentBackupSnapshot =
        read_json_entry(&mut zip, AGENT_STATE_FILE, MAX_FILE_BYTES).unwrap();
    let archived_secrets: SecretSnapshot =
        read_json_entry(&mut zip, SECRETS_FILE, MAX_FILE_BYTES).unwrap();
    let manifest: Manifest = read_json_entry(&mut zip, MANIFEST_FILE, MAX_FILE_BYTES).unwrap();
    let desktop = fixture.server.inner.desktop_workspace.as_ref().unwrap();
    let catalog_path = super::super::desktop_agents::catalog_path(desktop);
    let original_catalog = fs::read(&catalog_path).unwrap();
    fs::write(writer_root.join("original.txt"), "new local writer file").unwrap();
    fs::write(writer_root.join("added.txt"), "added since backup").unwrap();
    fs::write(
        fixture.workspace.join("notes.md"),
        "unselected default edit",
    )
    .unwrap();
    let original_chats: Value =
        serde_json::from_str(&original.settings["desktop_chat_catalog_data"]).unwrap();
    let mut edited_chats = original_chats.clone();
    for chat in edited_chats["chats"].as_object_mut().unwrap().values_mut() {
        chat["name"] = json!(format!("new local {}", chat["agent_id"].as_str().unwrap()));
    }
    core.write_chat_catalog_data(&edited_chats.to_string())
        .unwrap();
    let original_cron: Value =
        serde_json::from_str(&original.settings["desktop_cron_data"]).unwrap();
    let mut edited_cron = original_cron.clone();
    for job in edited_cron["jobs"].as_array_mut().unwrap() {
        job["text"] = json!(format!("local edit {}", job["id"].as_str().unwrap()));
    }
    core.write_cron_data(&edited_cron.to_string()).unwrap();
    core.write_ui_language("en").unwrap();
    let before = core.backup_snapshot(MAX_FILE_BYTES).unwrap();
    fixture
        .credentials
        .save(mail_key, Some("new local writer credential"));
    fixture.credentials.save(
        "agent.default.mail-auth-code",
        Some("new local default credential"),
    );
    let before_credentials = fixture.credentials.values.lock().unwrap().clone();
    let plan = super::super::desktop_agents::restore::plan_restore(
        desktop,
        &archived_agents,
        None,
        &std::collections::BTreeSet::from([String::from("writer")]),
        None,
    )
    .unwrap();
    let restored =
        restore_state::merge_for_agents(core, &before, &archived, &plan, false, true).unwrap();
    let candidate = core.prepare_restore(&restored.snapshot).unwrap();
    let expected = candidate.backup_snapshot(MAX_FILE_BYTES).unwrap();
    let expected_chats: Value =
        serde_json::from_str(&expected.settings["desktop_chat_catalog_data"]).unwrap();
    let mut expected_cron = edited_cron;
    expected_cron["jobs"][1] = original_cron["jobs"][1].clone();
    assert_eq!(
        serde_json::from_str::<Value>(&expected.settings["desktop_cron_data"]).unwrap(),
        expected_cron
    );
    for (id, chat) in edited_chats["chats"].as_object().unwrap() {
        assert_eq!(
            expected_chats["chats"][id],
            if chat["agent_id"] == "writer" {
                original_chats["chats"][id].clone()
            } else {
                chat.clone()
            }
        );
    }
    assert_eq!(expected.settings["ui_language"], "en");
    assert_eq!(core.backup_snapshot(MAX_FILE_BYTES).unwrap(), before);
    let mut lease = core.begin_restore(Duration::from_secs(2)).await.unwrap();
    let rollback = lease.capture_rollback(MAX_FILE_BYTES).unwrap();
    let known = secrets::known_keys(
        &fixture.server,
        &[String::from("default"), String::from("writer")],
    )
    .unwrap();
    let secret_plan = secrets::plan_restore(Some(&archived_secrets), true, false, &known).unwrap();
    let mut credentials = restore_credentials::CredentialRestore::prepare(
        fixture.credentials.clone(),
        secret_plan.credentials,
    )
    .unwrap();
    super::super::desktop_agent_settings::hydrate_restore(&candidate, &plan.agents).unwrap();
    let expected_runtime = candidate.agent_runtime_config().unwrap();
    let mut files = super::super::desktop_restore_files::RestoreFiles::default();
    restore_workspaces::stage_agents(desktop, &plan, &mut zip, &manifest, &mut files).unwrap();
    files.apply().unwrap();
    assert_eq!(
        fs::read(writer_root.join("original.txt")).unwrap(),
        b"archived writer file"
    );
    assert!(!writer_root.join("added.txt").exists());
    assert_eq!(
        fs::read(fixture.workspace.join("notes.md")).unwrap(),
        b"unselected default edit"
    );
    assert_eq!(fs::read(&catalog_path).unwrap(), plan.catalog);
    credentials.apply().unwrap();
    let mut expected_credentials = before_credentials.clone();
    expected_credentials.insert(
        mail_key.to_owned(),
        String::from("archived writer credential"),
    );
    // Secrets are an independent scope: the unselected default Workspace stays
    // local, while its explicitly selected secret snapshot is restored too.
    expected_credentials.insert(
        String::from("agent.default.mail-auth-code"),
        String::from("keep default credential"),
    );
    assert_eq!(
        *fixture.credentials.values.lock().unwrap(),
        expected_credentials
    );
    lease.apply(&candidate, MAX_FILE_BYTES).await.unwrap();
    assert_eq!(core.backup_snapshot(MAX_FILE_BYTES).unwrap(), expected);
    assert_eq!(core.agent_runtime_config().unwrap(), expected_runtime);
    for stored in &expected.threads {
        assert_eq!(
            core.read_thread(&stored.thread.id).await.unwrap().thread,
            stored.thread
        );
    }
    let store = qwenpaw_storage::ThreadStore::open(&fixture.data.join("threads.sqlite3")).unwrap();
    assert_eq!(store.backup_snapshot(MAX_FILE_BYTES).unwrap(), expected);
    lease.apply(&rollback, MAX_FILE_BYTES).await.unwrap();
    credentials.rollback().unwrap();
    files.rollback().unwrap();
    drop(files);
    assert_eq!(
        fs::read(writer_root.join("original.txt")).unwrap(),
        b"new local writer file"
    );
    assert_eq!(
        fs::read(writer_root.join("added.txt")).unwrap(),
        b"added since backup"
    );
    assert_eq!(
        fs::read(fixture.workspace.join("notes.md")).unwrap(),
        b"unselected default edit"
    );
    assert_eq!(fs::read(&catalog_path).unwrap(), original_catalog);
    assert_eq!(core.backup_snapshot(MAX_FILE_BYTES).unwrap(), before);
    assert_eq!(store.backup_snapshot(MAX_FILE_BYTES).unwrap(), before);
    assert_eq!(
        core.agent_runtime_config().unwrap(),
        rollback.agent_runtime_config().unwrap()
    );
    assert_eq!(
        *fixture.credentials.values.lock().unwrap(),
        before_credentials
    );
    drop(lease);
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[test]
fn scoped_backup_settings_reject_corrupt_selected_data_and_skip_unselected_data() {
    for (key, value) in [
        (
            "desktop_channel_config_data",
            json!({"version":99,"workspaces":[]}),
        ),
        (
            "desktop_cron_data",
            json!({"version":99,"jobs":[],"states":{},"history":{}}),
        ),
        ("desktop_chat_catalog_data", json!({"version": 99})),
        (
            "desktop_inbox_data",
            json!({"version": 99, "events": [], "traces": {}}),
        ),
        (
            "desktop_mail_access_control_data",
            json!({"version": 99, "agents": {}}),
        ),
    ] {
        let settings = BTreeMap::from([
            (key.to_owned(), value.to_string()),
            (String::from("ui_language"), String::from("zh")),
        ]);
        let mut selected = settings.clone();
        assert!(
            filter_backup_settings(
                &mut selected,
                &std::collections::BTreeSet::from(["default"]),
                &std::collections::BTreeSet::new(),
                true,
                &BTreeMap::new(),
            )
            .is_err()
        );
        let mut unselected = settings;
        filter_backup_settings(
            &mut unselected,
            &std::collections::BTreeSet::new(),
            &std::collections::BTreeSet::new(),
            true,
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(
            unselected,
            BTreeMap::from([(String::from("ui_language"), String::from("zh"))])
        );
    }
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn restored_backup_checkpoints_use_destination_paths_and_work_over_http() {
    let source = Fixture::new();
    let (source_base, source_task) = source.start_http().await;
    let client = reqwest::Client::new();
    let thread = source
        .server
        .inner
        .core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(source.workspace.to_string_lossy().into_owned()),
        })
        .await
        .unwrap()
        .thread;
    for (name, content) in [("first", "first version"), ("second", "second version")] {
        fs::write(source.workspace.join("notes.md"), content).unwrap();
        client.post(format!("{source_base}/api/workspace/checkpoints/snapshot"))
            .json(&json!({"session_id": thread.id, "user_id": "desktop", "channel": "console", "name": name}))
            .send().await.unwrap().error_for_status().unwrap();
    }
    let source_graph: Value = client
        .get(format!("{source_base}/api/workspace/checkpoints/graph"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let mut request = request("portable checkpoints");
    request.scope.include_global_config = false;
    let job = launch_backup_job(&source.server, request).await.unwrap();
    let terminal = completed(&source.server, &job.job_id).await;
    assert_eq!(terminal.status, "completed", "{:?}", terminal.error);
    let archive_path = source
        .data
        .join("backups")
        .join(format!("{}.zip", terminal.backup_id));
    let mut archive = ZipArchive::new(fs::File::open(archive_path).unwrap()).unwrap();
    let manifest: Manifest = read_json_entry(&mut archive, MANIFEST_FILE, MAX_FILE_BYTES).unwrap();
    let agents = read_json_entry(&mut archive, AGENT_STATE_FILE, MAX_FILE_BYTES).unwrap();
    let state = read_json_entry(&mut archive, CORE_STATE_FILE, MAX_FILE_BYTES).unwrap();
    source.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), source_task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    let destination = Fixture::with_data_name(None, "different-core-control");
    let (base, task) = destination.start_http().await;
    let desktop = destination.server.inner.desktop_workspace.as_ref().unwrap();
    let core = &destination.server.inner.core;
    let plan = super::super::desktop_agents::restore::plan_restore(
        desktop,
        &agents,
        None,
        &std::collections::BTreeSet::from([String::from("default")]),
        None,
    )
    .unwrap();
    let before = core.backup_snapshot(MAX_FILE_BYTES).unwrap();
    let merged =
        restore_state::merge_for_agents(core, &before, &state, &plan, false, false).unwrap();
    let candidate = core.prepare_restore(&merged.snapshot).unwrap();
    let mut lease = core.begin_restore(Duration::from_secs(2)).await.unwrap();
    let mut files = super::super::desktop_restore_files::RestoreFiles::default();
    restore_workspaces::stage_agents(desktop, &plan, &mut archive, &manifest, &mut files).unwrap();
    restore_workspaces::stage_checkpoints(
        desktop,
        &plan,
        &state,
        &mut archive,
        &manifest,
        &mut files,
    )
    .unwrap();
    files.apply().unwrap();
    lease.apply(&candidate, MAX_FILE_BYTES).await.unwrap();
    files.commit();
    drop(files);
    drop(lease);
    assert_eq!(
        core.read_thread(&thread.id)
            .await
            .unwrap()
            .thread
            .workspace_root,
        Some(destination.workspace.to_string_lossy().into_owned())
    );
    assert_eq!(
        fs::read(destination.workspace.join("notes.md")).unwrap(),
        b"second version"
    );
    let graph: Value = client
        .get(format!("{base}/api/workspace/checkpoints/graph"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(graph["summary"], source_graph["summary"]);
    assert_eq!(graph["sessions"], source_graph["sessions"]);
    let find = |graph: &Value, name: &str| {
        graph["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|node| node["name"] == name)
            .unwrap()
            .clone()
    };
    let first = find(&graph, "first");
    let second = find(&graph, "second");
    assert_ne!(first["commit"], find(&source_graph, "first")["commit"]);
    assert_ne!(second["commit"], find(&source_graph, "second")["commit"]);
    assert_eq!(second["parent_commit"], first["commit"]);
    assert_eq!(second["is_head"], true);
    let body = json!({"commit": first["commit"], "session_id": thread.id,
        "user_id": "desktop", "channel": "console", "include_files": true, "files": ["notes.md"]});
    let preview: Value = client
        .post(format!("{base}/api/workspace/checkpoints/restore/preview"))
        .json(&body)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(preview.to_string().contains("notes.md"));
    let response = client
        .post(format!("{base}/api/workspace/checkpoints/restore"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    assert_eq!(
        fs::read(destination.workspace.join("notes.md")).unwrap(),
        b"first version"
    );
    assert_eq!(
        fs::read(source.workspace.join("notes.md")).unwrap(),
        b"second version"
    );
    assert_eq!(
        core.read_thread(&thread.id)
            .await
            .unwrap()
            .thread
            .workspace_root,
        Some(destination.workspace.to_string_lossy().into_owned())
    );
    destination.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

async fn seed_shared_project_checkpoints(fixture: &Fixture, base: &str, catalog: &Value) {
    let client = reqwest::Client::new();
    for agent in ["default"] {
        client.post(format!("{base}/api/workspace/checkpoints/snapshot"))
            .json(&json!({"session_id": format!("{agent} session"), "user_id": "desktop", "channel": "console", "name": agent}))
            .send().await.unwrap().error_for_status().unwrap();
    }
    // The default HTTP surface cannot capture a different Workspace's chat.
    let rejected = client.post(format!("{base}/api/workspace/checkpoints/snapshot"))
        .json(&json!({"session_id":"writer session", "user_id":"desktop", "channel":"console", "name":"writer"}))
        .send().await.unwrap();
    assert_eq!(rejected.status(), StatusCode::NOT_FOUND);
    client
        .patch(format!("{base}/api/workspace/checkpoints/auto"))
        .json(&json!({"enabled":true}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let writer_thread = catalog["chats"]
        .as_object()
        .unwrap()
        .iter()
        .find(|(_, chat)| chat["agent_id"] == "writer")
        .unwrap()
        .0;
    // Keep the mixed historical fixture without relying on a live producer
    // writing another Agent's checkpoint namespace.
    super::super::desktop_checkpoints::seed_mixed_owner_snapshot(&fixture.server, writer_thread)
        .await;
}

#[tokio::test]
async fn checkpoint_backup_excludes_other_agents_even_when_their_chats_share_the_workspace() {
    let fixture = Fixture::new();
    let (base, task) = fixture.start_http().await;
    seed_scoped_backup_state(&fixture, &base).await;
    let snapshot = fixture
        .server
        .inner
        .core
        .backup_snapshot(MAX_FILE_BYTES)
        .unwrap();
    let catalog: Value =
        serde_json::from_str(&snapshot.settings["desktop_chat_catalog_data"]).unwrap();
    let default_thread = catalog["chats"]
        .as_object()
        .unwrap()
        .iter()
        .find(|(_, chat)| chat["agent_id"] == "default")
        .unwrap()
        .0
        .clone();
    seed_shared_project_checkpoints(&fixture, &base, &catalog).await;
    let (directory, _) =
        super::super::desktop_checkpoints::backup_sources(&fixture.server, &fixture.workspace)
            .await
            .unwrap();
    let original: Value =
        serde_json::from_slice(&fs::read(directory.join("state.json")).unwrap()).unwrap();
    assert_eq!(original["entries"].as_array().unwrap().len(), 2);
    let mut request = request("selected checkpoint scope");
    request.agents = vec![String::from("default")];
    request.scope.include_global_config = false;
    let job = launch_backup_job(&fixture.server, request).await.unwrap();
    let terminal = completed(&fixture.server, &job.job_id).await;
    assert_eq!(terminal.status, "completed", "{:?}", terminal.error);
    let path = fixture
        .data
        .join("backups")
        .join(format!("{}.zip", terminal.backup_id));
    let mut archive = ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    let actual: Value = read_json_entry(
        &mut archive,
        "data/checkpoints/default/state.json",
        MAX_FILE_BYTES,
    )
    .unwrap();
    let mut expected = original.clone();
    expected["entries"]
        .as_array_mut()
        .unwrap()
        .retain(|entry| entry["thread_id"] == default_thread);
    let commit = expected["entries"][0]["commit"]
        .as_str()
        .unwrap()
        .to_owned();
    expected["heads"]
        .as_object_mut()
        .unwrap()
        .retain(|_, head| *head == commit);
    assert_eq!(actual, expected);
    let names = archive
        .file_names()
        .filter(|name| {
            name.starts_with("data/checkpoints/")
                && Path::new(name)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
        })
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![format!("data/checkpoints/default/snapshots/{commit}.zip")]
    );
    let mut bytes = Vec::new();
    archive
        .by_name(&names[0])
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    let mut nested = ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let checkpoint: qwenpaw_core::ThreadCheckpoint =
        serde_json::from_reader(nested.by_name("thread.json").unwrap()).unwrap();
    assert_eq!(checkpoint.thread.id, default_thread);
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(directory.join("state.json")).unwrap()).unwrap(),
        original
    );
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[test]
fn inbox_backup_does_not_export_ambiguous_or_unreferenced_traces() {
    let event = |agent: &str, run: &str| {
        json!({
            "id": format!("{agent}-{run}"), "agent_id": agent, "source_type": "cron", "source_id": "job",
            "event_type": "result", "status": "success", "severity": "info", "title": "result", "body": "body",
            "payload": {"run_id": run}, "read": false, "created_at": 1.0
        })
    };
    let trace = |run: &str, meta: Value| {
        json!({"run_id": run, "created_at": 1.0, "completed_at": 2.0,
        "status": "success", "meta": meta, "events": []})
    };
    let selected = event("writer", "selected");
    let ambiguous = event("writer", "shared");
    let wrong_owner = event("writer", "wrong-owner");
    let included_trace = trace("selected", json!({}));
    let original = json!({"version": 1,
        "events": [selected, ambiguous, wrong_owner, event("default", "shared")],
        "traces": {"selected": included_trace, "shared": trace("shared", json!({})),
            "orphan": trace("orphan", json!({})), "wrong-owner": trace("wrong-owner", json!({"agent_id": "default"}))}
    });
    let filtered = super::super::desktop_inbox::filter_backup_data(
        &original.to_string(),
        &std::collections::BTreeSet::from(["writer"]),
    )
    .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&filtered).unwrap(),
        json!({"version": 1,
        "events": [selected, ambiguous, wrong_owner], "traces": {"selected": included_trace}})
    );
}

#[test]
fn mail_backup_rejects_pending_entries_owned_by_another_agent() {
    let original = json!({"version": 1, "agents": {"writer": {
        "pending": [{"sender_address": "person@example.com", "agent_id": "default"}]
    }}});
    assert_eq!(
        super::super::desktop_mail_access_control::filter_backup_data(
            &original.to_string(),
            &std::collections::BTreeSet::from(["writer"]),
            &BTreeMap::new(),
        ),
        Err("Mail access-control data is invalid")
    );
}
