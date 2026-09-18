use std::sync::Mutex;

use serde_json::json;

use super::*;

#[derive(Default)]
struct FaultStore {
    values: Mutex<BTreeMap<String, McpOAuthCredentials>>,
    writes: Mutex<Vec<String>>,
    fail_writes: Mutex<BTreeSet<usize>>,
}

#[tokio::test]
async fn oauth_binding_uses_candidate_environment_and_does_not_reuse_another_resources_tokens() {
    let store = Arc::new(FaultStore::default());
    let config = serde_json::from_value(json!({"remote": {
        "transport": "streamable_http", "url": "${QWENPAW_OAUTH_RESOURCE}", "oauth": {}
    }}))
    .unwrap();
    let manager = McpManager::new(config, store.clone()).with_environment(BTreeMap::from([(
        String::from("QWENPAW_OAUTH_RESOURCE"),
        String::from("https://original.example/mcp"),
    )]));
    let account = oauth_account("remote", &manager.inner.clients["remote"]);
    let original = BTreeMap::from([(account.clone(), credentials("original", "original-token"))]);
    *store.values.lock().unwrap() = original.clone();
    assert!(manager.oauth_status("remote").await.unwrap().authorized);
    let next = manager.with_environment(BTreeMap::from([(
        String::from("QWENPAW_OAUTH_RESOURCE"),
        String::from("https://restored.example/mcp"),
    )]));
    assert!(!next.oauth_status("remote").await.unwrap().authorized);
    assert_eq!(
        next.stored_oauth_bearer(
            "remote",
            &next.inner.clients["remote"],
            &reqwest::Client::new()
        )
        .await
        .unwrap(),
        None
    );
    assert!(next.backup_oauth_credentials().is_err());
    let mut backup = manager.backup_oauth_credentials().unwrap();
    assert!(next.prepare_oauth_restore(&backup).is_err());
    assert_eq!(*store.values.lock().unwrap(), original);
    assert!(store.writes.lock().unwrap().is_empty());
    backup.clients.insert(
        String::from("remote"),
        Some(credentials("restored", "restored-token")),
    );
    let mut restore = next.prepare_oauth_restore(&backup).unwrap();
    restore.apply().unwrap();
    assert!(next.oauth_status("remote").await.unwrap().authorized);
    assert!(!manager.oauth_status("remote").await.unwrap().authorized);
    assert_eq!(next.backup_oauth_credentials().unwrap(), backup);
    restore.rollback().unwrap();
    assert_eq!(*store.values.lock().unwrap(), original);
    assert!(manager.oauth_status("remote").await.unwrap().authorized);
    assert!(!next.oauth_status("remote").await.unwrap().authorized);
}

impl FaultStore {
    fn changed(&self, account: &str) -> Result<(), String> {
        let mut writes = self.writes.lock().unwrap();
        writes.push(account.to_owned());
        if self.fail_writes.lock().unwrap().remove(&writes.len()) {
            Err(String::from("injected failure after mutation"))
        } else {
            Ok(())
        }
    }
}

impl McpOAuthCredentialStore for FaultStore {
    fn load(&self, account: &str) -> Result<Option<McpOAuthCredentials>, String> {
        Ok(self.values.lock().unwrap().get(account).cloned())
    }

    fn save(&self, account: &str, credentials: &McpOAuthCredentials) -> Result<(), String> {
        self.values
            .lock()
            .unwrap()
            .insert(account.to_owned(), credentials.clone());
        self.changed(account)
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        self.values.lock().unwrap().remove(account);
        self.changed(account)
    }
}

fn credentials(id: &str, token: &str) -> McpOAuthCredentials {
    McpOAuthCredentials {
        issuer: String::from("https://auth.example"),
        resource: format!("https://{id}.example/mcp"),
        client_id: String::from("client"),
        authorization_endpoint: String::from("https://auth.example/authorize"),
        token_endpoint: String::from("https://auth.example/token"),
        scope: String::from("files:read"),
        access_token: token.to_owned(),
        refresh_token: String::from("refresh-secret"),
        expires_at: 0.0,
    }
}

fn fixture() -> (
    McpManager,
    Arc<FaultStore>,
    McpOAuthBackup,
    BTreeMap<String, McpOAuthCredentials>,
) {
    let store = Arc::new(FaultStore::default());
    let config = serde_json::from_value(json!({
        "a": {"transport": "streamable_http", "url": "https://a.example/mcp", "oauth": {}},
        "b": {"transport": "streamable_http", "url": "https://b.example/mcp", "oauth": {}, "enabled": false},
        "plain": {"transport": "streamable_http", "url": "https://plain.example/mcp"}
    })).unwrap();
    let manager = McpManager::new(config, store.clone());
    let mut original = BTreeMap::from([(
        String::from("unrelated-account"),
        credentials("other", "keep-secret"),
    )]);
    for id in ["a", "b"] {
        original.insert(
            oauth_account(id, &manager.inner.clients[id]),
            credentials(id, "original-secret"),
        );
    }
    *store.values.lock().unwrap() = original.clone();
    let backup = McpOAuthBackup {
        version: 1,
        clients: BTreeMap::from([
            (String::from("a"), None),
            (String::from("b"), Some(credentials("b", "restored-secret"))),
        ]),
    };
    (manager, store, backup, original)
}

#[test]
fn logical_backup_and_restore_preserve_revocation_disabled_clients_and_unrelated_keys() {
    let (manager, store, replacement, original) = fixture();
    let snapshot = manager.backup_oauth_credentials().unwrap();
    assert_eq!(
        snapshot.clients,
        BTreeMap::from([
            (String::from("a"), Some(credentials("a", "original-secret"))),
            (String::from("b"), Some(credentials("b", "original-secret"))),
        ])
    );
    assert!(!format!("{snapshot:?}").contains("original-secret"));
    let mut restore = manager.prepare_oauth_restore(&replacement).unwrap();
    assert!(store.writes.lock().unwrap().is_empty());
    restore.apply().unwrap();
    let mut expected = original.clone();
    expected.remove(&oauth_account("a", &manager.inner.clients["a"]));
    expected.insert(
        oauth_account("b", &manager.inner.clients["b"]),
        credentials("b", "restored-secret"),
    );
    assert_eq!(*store.values.lock().unwrap(), expected);
    assert_eq!(manager.backup_oauth_credentials().unwrap(), replacement);
    restore.rollback().unwrap();
    assert_eq!(*store.values.lock().unwrap(), original);
    restore.rollback().unwrap();
    assert_eq!(store.writes.lock().unwrap().len(), 4);
}

#[test]
fn a_failed_write_is_itself_rolled_back_before_earlier_changes() {
    let (manager, store, replacement, original) = fixture();
    store.fail_writes.lock().unwrap().insert(2);
    let mut restore = manager.prepare_oauth_restore(&replacement).unwrap();
    assert!(
        restore
            .apply()
            .unwrap_err()
            .to_string()
            .contains("original values restored")
    );
    assert_eq!(*store.values.lock().unwrap(), original);
    let a = oauth_account("a", &manager.inner.clients["a"]);
    let b = oauth_account("b", &manager.inner.clients["b"]);
    assert_eq!(
        *store.writes.lock().unwrap(),
        vec![a.clone(), b.clone(), b, a]
    );
    restore.apply().unwrap();
    restore.rollback().unwrap();
    assert_eq!(*store.values.lock().unwrap(), original);
}

#[test]
fn incomplete_rollback_keeps_failed_keys_for_retry_without_rewriting_recovered_keys() {
    let (manager, store, replacement, original) = fixture();
    *store.fail_writes.lock().unwrap() = BTreeSet::from([2, 3]);
    let mut restore = manager.prepare_oauth_restore(&replacement).unwrap();
    assert!(
        restore
            .apply()
            .unwrap_err()
            .to_string()
            .contains("rollback is incomplete")
    );
    assert_eq!(restore.dirty, BTreeSet::from([1]));
    assert!(restore.apply().is_err());
    assert_eq!(store.writes.lock().unwrap().len(), 4);
    restore.rollback().unwrap();
    assert_eq!(store.writes.lock().unwrap().len(), 5);
    assert_eq!(*store.values.lock().unwrap(), original);
    assert!(restore.dirty.is_empty());
}

#[test]
fn all_credentials_are_validated_before_any_key_is_changed() {
    let (manager, store, replacement, original) = fixture();
    let mut bad_version = replacement.clone();
    bad_version.version = 2;
    let mut bad_resource = replacement.clone();
    bad_resource
        .clients
        .insert(String::from("b"), Some(credentials("elsewhere", "secret")));
    let mut bad_url = replacement.clone();
    bad_url
        .clients
        .get_mut("b")
        .unwrap()
        .as_mut()
        .unwrap()
        .token_endpoint = String::from("http://not-loopback.example/token");
    let mut bad_token = replacement.clone();
    bad_token
        .clients
        .get_mut("b")
        .unwrap()
        .as_mut()
        .unwrap()
        .access_token = String::from("line\nbreak");
    let mut bad_client = replacement.clone();
    bad_client.clients.insert(String::from("plain"), None);
    let mut bad_size = replacement.clone();
    bad_size
        .clients
        .get_mut("b")
        .unwrap()
        .as_mut()
        .unwrap()
        .scope = "x".repeat(70_000);
    for invalid in [
        bad_version,
        bad_resource,
        bad_url,
        bad_token,
        bad_client,
        bad_size,
    ] {
        assert!(manager.prepare_oauth_restore(&invalid).is_err());
        assert_eq!(*store.values.lock().unwrap(), original);
        assert!(store.writes.lock().unwrap().is_empty());
    }
}

#[test]
fn archive_validation_is_independent_of_local_clients_but_restore_requires_a_matching_resource() {
    let (manager, store, mut backup, original) = fixture();
    backup.clients.insert(
        String::from("foreign"),
        Some(credentials("foreign", "token")),
    );
    assert!(backup.validate().is_ok());
    assert!(manager.prepare_oauth_restore(&backup).is_err());
    backup.clients.remove("foreign");
    backup
        .clients
        .insert(String::from("b"), Some(credentials("elsewhere", "token")));
    assert!(backup.validate().is_ok());
    assert!(manager.prepare_oauth_restore(&backup).is_err());
    for invalid in ["", "\nclient"] {
        let mut invalid_backup = backup.clone();
        invalid_backup.clients.insert(invalid.to_owned(), None);
        assert!(invalid_backup.validate().is_err());
    }
    assert_eq!(*store.values.lock().unwrap(), original);
    assert!(store.writes.lock().unwrap().is_empty());
}
