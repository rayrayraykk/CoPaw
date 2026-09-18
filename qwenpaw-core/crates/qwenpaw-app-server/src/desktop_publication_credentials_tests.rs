//! Per-entry fixtures never create a keyring Entry or change its global provider.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use pretty_assertions::assert_eq;
use serde_json::{Value, json};

use super::*;

#[path = "desktop_publication_credential_rollback_tests.rs"]
mod rollback;

#[derive(Clone, Copy, Default)]
enum Fault {
    #[default]
    None,
    Before,
    After,
    Discard,
}

#[derive(Default)]
struct Vault {
    records: BTreeMap<String, String>,
    reads: usize,
    writes: usize,
    deletes: usize,
    fail_read: Option<usize>,
    write_fault: Fault,
    delete_fault: Fault,
}

struct Entry {
    vault: Arc<Mutex<Vault>>,
    account: String,
}

impl Entry {
    fn new(vault: &Arc<Mutex<Vault>>, scope: &AgentPublicationSecretScope) -> Self {
        Self {
            vault: Arc::clone(vault),
            account: scope.account(),
        }
    }
}

impl RecoveryEntry for Entry {
    fn read(&self) -> anyhow::Result<Option<String>> {
        let mut vault = self.vault.lock().unwrap();
        vault.reads += 1;
        anyhow::ensure!(
            vault.fail_read != Some(vault.reads),
            "fixture-secret-in-read-error"
        );
        Ok(vault.records.get(&self.account).cloned())
    }

    fn write(&self, value: &str) -> anyhow::Result<()> {
        let mut vault = self.vault.lock().unwrap();
        vault.writes += 1;
        anyhow::ensure!(
            !matches!(vault.write_fault, Fault::Before),
            "fixture-secret-before-write"
        );
        if !matches!(vault.write_fault, Fault::Discard) {
            vault.records.insert(self.account.clone(), value.into());
        }
        anyhow::ensure!(
            !matches!(vault.write_fault, Fault::After),
            "fixture-secret-after-write"
        );
        Ok(())
    }

    fn delete(&self) -> anyhow::Result<()> {
        let mut vault = self.vault.lock().unwrap();
        vault.deletes += 1;
        anyhow::ensure!(
            !matches!(vault.delete_fault, Fault::Before),
            "fixture-secret-before-delete"
        );
        if !matches!(vault.delete_fault, Fault::Discard) {
            vault.records.remove(&self.account);
        }
        anyhow::ensure!(
            !matches!(vault.delete_fault, Fault::After),
            "fixture-secret-after-delete"
        );
        Ok(())
    }
}

fn scope() -> AgentPublicationSecretScope {
    AgentPublicationSecretScope::new(Uuid::now_v7(), Uuid::now_v7(), "writer".into()).unwrap()
}

fn secret() -> AgentPublicationSecret {
    AgentPublicationSecret::new(
        Some("fixture-secret-old".into()),
        Some("fixture-secret-new".into()),
    )
}

fn fixture() -> (Arc<Mutex<Vault>>, AgentPublicationSecretScope, Entry) {
    let vault = Arc::new(Mutex::new(Vault::default()));
    let scope = scope();
    let entry = Entry::new(&vault, &scope);
    (vault, scope, entry)
}

fn redacted(error: &anyhow::Error) {
    assert!(!format!("{error:?} {error:#}").contains("fixture-secret"));
}

#[test]
fn publication_credentials_roundtrip_preserves_missing_empty_and_unicode_after_reopening_entry() {
    for old in [None, Some(""), Some("fixture-secret-旧\n\"\\")] {
        for new in [None, Some(""), Some("fixture-secret-新\n\"\\")] {
            let (vault, scope, entry) = fixture();
            let secret =
                AgentPublicationSecret::new(old.map(str::to_owned), new.map(str::to_owned));
            assert_eq!(secret.previous(), old);
            assert_eq!(secret.replacement(), new);
            assert_eq!(load(&entry, &scope).unwrap(), None);
            prepare(&entry, &scope, &secret).unwrap();
            drop(entry);
            let entry = Entry::new(&vault, &scope);
            assert_eq!(load(&entry, &scope).unwrap(), Some(secret.clone()));
            finish(&entry, &scope, &secret).unwrap();
            finish(&entry, &scope, &secret).unwrap();
            assert_eq!(load(&entry, &scope).unwrap(), None);
            let vault = vault.lock().unwrap();
            assert_eq!(
                (&vault.records, vault.writes, vault.deletes),
                (&BTreeMap::new(), 1, 1)
            );
        }
    }
}

#[test]
fn publication_credentials_accounts_are_installation_and_transaction_scoped_not_agent_reuse() {
    let (vault, original, entry) = fixture();
    let secret = secret();
    prepare(&entry, &original, &secret).unwrap();
    let installation =
        AgentPublicationSecretScope::new(Uuid::now_v7(), original.transaction, "writer".into())
            .unwrap();
    let transaction =
        AgentPublicationSecretScope::new(original.installation, Uuid::now_v7(), "writer".into())
            .unwrap();
    for other in [installation, transaction] {
        assert_ne!(other.account(), original.account());
        let entry = Entry::new(&vault, &other);
        assert_eq!(load(&entry, &other).unwrap(), None);
        prepare(&entry, &other, &secret).unwrap();
        finish(&entry, &other, &secret).unwrap();
    }
    let other_agent = AgentPublicationSecretScope::new(
        original.installation,
        original.transaction,
        "editor".into(),
    )
    .unwrap();
    assert_eq!(other_agent.account(), original.account());
    redacted(&load(&entry, &other_agent).unwrap_err());
    redacted(&prepare(&entry, &other_agent, &secret).unwrap_err());
    redacted(&finish(&entry, &other_agent, &secret).unwrap_err());
    assert_eq!(load(&entry, &original).unwrap(), Some(secret));
    assert!(original.account().starts_with(ACCOUNT_PREFIX));
    for prefix in [
        super::super::AGENT_SETTING_ACCOUNT_PREFIX,
        super::super::MCP_CLIENT_ACCOUNT_PREFIX,
        super::super::ENVIRONMENT_ACCOUNT_PREFIX,
        super::super::BACKUP_SIGNING_KEY_ACCOUNT,
    ] {
        assert!(!original.account().starts_with(prefix));
    }
}

#[test]
fn publication_credentials_scope_rejects_nil_invalid_agent_and_deserialization_bypass() {
    let id = Uuid::now_v7();
    for (installation, transaction, agent) in [
        (Uuid::nil(), id, "writer"),
        (id, Uuid::nil(), "writer"),
        (id, id, "../writer"),
        (id, id, ""),
        (id, id, "writer/child"),
    ] {
        assert!(AgentPublicationSecretScope::new(installation, transaction, agent.into()).is_err());
        let raw =
            json!({"installation": installation, "transaction": transaction, "agent_id": agent});
        assert!(serde_json::from_value::<AgentPublicationSecretScope>(raw).is_err());
    }
}

#[test]
fn publication_credentials_duplicate_reservation_and_changed_cleanup_preserve_record() {
    let (vault, scope, entry) = fixture();
    let secret = secret();
    prepare(&entry, &scope, &secret).unwrap();
    let before = vault.lock().unwrap().records.clone();
    redacted(&prepare(&entry, &scope, &secret).unwrap_err());
    let different = AgentPublicationSecret::new(None, None);
    redacted(&prepare(&entry, &scope, &different).unwrap_err());
    redacted(&finish(&entry, &scope, &different).unwrap_err());
    let vault = vault.lock().unwrap();
    assert_eq!(
        (&vault.records, vault.writes, vault.deletes),
        (&before, 1, 0)
    );
}

#[test]
fn publication_credentials_malformed_records_fail_closed_and_never_leak_or_delete() {
    let (vault, scope, entry) = fixture();
    let secret = secret();
    let valid = serde_json::to_value(Record {
        version: 1,
        scope: scope.clone(),
        secret: secret.clone(),
    })
    .unwrap();
    let mut variants = vec![
        json!(null),
        json!("fixture-secret"),
        json!({}),
        json!({"version": 1}),
    ];
    for (field, replacement) in [
        ("version", json!(2)),
        ("scope", json!(null)),
        ("secret", json!(null)),
        ("extra", json!("fixture-secret")),
    ] {
        let mut value = valid.clone();
        value[field] = replacement;
        variants.push(value);
    }
    for field in ["previous", "replacement"] {
        let mut value = valid.clone();
        value["secret"].as_object_mut().unwrap().remove(field);
        variants.push(value);
        let mut value = valid.clone();
        value["secret"][field] = json!({"state": "missing", "value": "fixture-secret"});
        variants.push(value);
    }
    let mut raw: Vec<_> = variants.iter().map(Value::to_string).collect();
    raw.push("{fixture-secret-malformed".into());
    raw.push(
        valid
            .to_string()
            .replacen("\"version\":1", "\"version\":1,\"version\":1", 1),
    );
    for raw in raw {
        vault
            .lock()
            .unwrap()
            .records
            .insert(scope.account(), raw.clone());
        redacted(&load(&entry, &scope).unwrap_err());
        redacted(&prepare(&entry, &scope, &secret).unwrap_err());
        redacted(&finish(&entry, &scope, &secret).unwrap_err());
        let vault = vault.lock().unwrap();
        assert_eq!(
            (&vault.records, vault.writes, vault.deletes),
            (&BTreeMap::from([(scope.account(), raw)]), 0, 0)
        );
    }
}

#[test]
fn publication_credentials_read_failure_does_not_write_or_clean_anything() {
    for action in 0..3 {
        let (vault, scope, entry) = fixture();
        vault.lock().unwrap().fail_read = Some(1);
        let result = match action {
            0 => load(&entry, &scope).map(|_| ()),
            1 => prepare(&entry, &scope, &secret()),
            _ => finish(&entry, &scope, &secret()),
        };
        redacted(&result.unwrap_err());
        let vault = vault.lock().unwrap();
        assert_eq!(
            (&vault.records, vault.writes, vault.deletes),
            (&BTreeMap::new(), 0, 0)
        );
    }
}

#[test]
fn publication_credentials_write_failures_retain_any_written_snapshot_without_implicit_cleanup() {
    for fault in [Fault::Before, Fault::After, Fault::Discard] {
        let (vault, scope, entry) = fixture();
        vault.lock().unwrap().write_fault = fault;
        let secret = secret();
        redacted(&prepare(&entry, &scope, &secret).unwrap_err());
        assert_eq!(vault.lock().unwrap().deletes, 0);
        assert_eq!(
            load(&entry, &scope).unwrap(),
            if matches!(fault, Fault::After) {
                Some(secret)
            } else {
                None
            }
        );
    }
}

#[test]
fn publication_credentials_failed_verification_retains_snapshot_for_a_new_entry() {
    let (vault, scope, entry) = fixture();
    vault.lock().unwrap().fail_read = Some(2);
    let secret = secret();
    redacted(&prepare(&entry, &scope, &secret).unwrap_err());
    drop(entry);
    let entry = Entry::new(&vault, &scope);
    assert_eq!(load(&entry, &scope).unwrap(), Some(secret.clone()));
    assert_eq!(vault.lock().unwrap().deletes, 0);
    finish(&entry, &scope, &secret).unwrap();
}

#[test]
fn publication_credentials_cleanup_failure_preserves_state_or_retries_already_completed_delete() {
    for fault in [Fault::Before, Fault::After, Fault::Discard] {
        let (vault, scope, entry) = fixture();
        let secret = secret();
        prepare(&entry, &scope, &secret).unwrap();
        vault.lock().unwrap().delete_fault = fault;
        redacted(&finish(&entry, &scope, &secret).unwrap_err());
        assert_eq!(
            load(&entry, &scope).unwrap(),
            if matches!(fault, Fault::After) {
                None
            } else {
                Some(secret.clone())
            }
        );
        vault.lock().unwrap().delete_fault = Fault::None;
        finish(&entry, &scope, &secret).unwrap();
        assert_eq!(load(&entry, &scope).unwrap(), None);
        assert_eq!(vault.lock().unwrap().writes, 1);
    }
}

#[test]
fn publication_credentials_debug_and_unsupported_store_do_not_expose_or_claim_recovery() {
    struct Unsupported;
    impl crate::DesktopCredentialStore for Unsupported {
        fn load_api_key(&self) -> anyhow::Result<Option<String>> {
            panic!("no live credential access")
        }
        fn save_api_key(&self, _: Option<&str>) -> anyhow::Result<()> {
            panic!("no live credential access")
        }
    }
    use crate::DesktopCredentialStore as _;
    assert_eq!(
        format!("{:?}", secret()),
        "AgentPublicationSecret([REDACTED])"
    );
    redacted(
        &Unsupported
            .load_agent_publication_secret(&scope())
            .unwrap_err(),
    );
    redacted(
        &Unsupported
            .prepare_agent_publication_secret(&scope(), &secret())
            .unwrap_err(),
    );
    redacted(
        &Unsupported
            .finish_agent_publication_secret(&scope(), &secret())
            .unwrap_err(),
    );
}
