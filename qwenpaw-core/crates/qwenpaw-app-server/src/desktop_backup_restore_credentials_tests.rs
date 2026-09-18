use std::sync::Mutex;

use super::*;

#[derive(Default)]
struct State {
    values: BTreeMap<String, String>,
    reads: Vec<String>,
    writes: Vec<(String, Option<String>)>,
    fail_read: Option<String>,
    fail_writes: BTreeMap<usize, bool>,
}

#[derive(Default)]
struct Store(Mutex<State>);

impl Store {
    fn load(&self, key: &str) -> anyhow::Result<Option<String>> {
        let mut state = self.0.lock().unwrap();
        state.reads.push(key.to_owned());
        if state.fail_read.as_deref() == Some(key) {
            anyhow::bail!("private keyring diagnostics and secret values");
        }
        Ok(state.values.get(key).cloned())
    }

    fn save(&self, key: &str, value: Option<&str>) -> anyhow::Result<()> {
        let mut state = self.0.lock().unwrap();
        state
            .writes
            .push((key.to_owned(), value.map(str::to_owned)));
        let failure = state.fail_writes.get(&state.writes.len()).copied();
        if failure != Some(false) {
            if let Some(value) = value {
                state.values.insert(key.to_owned(), value.to_owned());
            } else {
                state.values.remove(key);
            }
        }
        if failure.is_some() {
            anyhow::bail!("private keyring diagnostics and secret values");
        }
        Ok(())
    }
}

impl DesktopCredentialStore for Store {
    fn load_api_key(&self) -> anyhow::Result<Option<String>> {
        self.load("api")
    }

    fn save_api_key(&self, value: Option<&str>) -> anyhow::Result<()> {
        self.save("api", value)
    }

    fn load_environment_value(&self, key: &str) -> anyhow::Result<Option<String>> {
        self.load(&format!("env:{key}"))
    }

    fn save_environment_value(&self, key: &str, value: Option<&str>) -> anyhow::Result<()> {
        self.save(&format!("env:{key}"), value)
    }

    fn load_agent_setting_secret(&self, key: &str) -> anyhow::Result<Option<String>> {
        self.load(&format!("agent:{key}"))
    }

    fn save_agent_setting_secret(&self, key: &str, value: Option<&str>) -> anyhow::Result<()> {
        self.save(&format!("agent:{key}"), value)
    }

    fn load_mcp_client_secrets(&self, key: &str) -> anyhow::Result<Option<String>> {
        self.load(&format!("mcp:{key}"))
    }

    fn save_mcp_client_secrets(&self, key: &str, value: Option<&str>) -> anyhow::Result<()> {
        self.save(&format!("mcp:{key}"), value)
    }

    fn load_backup_signing_key(&self) -> anyhow::Result<Option<String>> {
        panic!("Restore must never read the signing key")
    }

    fn save_backup_signing_key(&self, _: &str) -> anyhow::Result<()> {
        panic!("Restore must never replace the signing key")
    }
}

fn initial() -> BTreeMap<String, String> {
    [
        ("api", "old-api"),
        ("env:EMPTY", ""),
        ("agent:model-provider-api-key:provider", "old-provider"),
        ("mcp:client", "old-client"),
        ("agent:unselected", "keep-unselected"),
        ("signing", "keep-signing"),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_owned(), value.to_owned()))
    .collect()
}

fn changes() -> BTreeMap<CredentialKey, Option<String>> {
    BTreeMap::from([
        (CredentialKey::ApiKey, Some(String::from("new-api"))),
        (CredentialKey::Environment(String::from("EMPTY")), None),
        (
            CredentialKey::AgentSetting(String::from("agent.writer.mail-auth-code")),
            Some(String::from("new-mail")),
        ),
        (
            CredentialKey::AgentSetting(String::from("model-provider-api-key:provider")),
            Some(String::from("new-provider")),
        ),
        (CredentialKey::McpClient(String::from("client")), None),
    ])
}

fn fixture() -> Arc<Store> {
    Arc::new(Store(Mutex::new(State {
        values: initial(),
        ..State::default()
    })))
}

#[test]
fn captures_every_original_then_applies_and_rolls_back_all_domains() {
    let store = fixture();
    let mut restore = CredentialRestore::prepare(store.clone(), changes()).unwrap();
    {
        let state = store.0.lock().unwrap();
        assert_eq!(state.values, initial());
        assert_eq!(
            state.reads,
            [
                "api",
                "env:EMPTY",
                "agent:agent.writer.mail-auth-code",
                "agent:model-provider-api-key:provider",
                "mcp:client"
            ]
        );
        assert_eq!(state.writes, Vec::new());
    }
    restore.apply().unwrap();
    let mut expected = initial();
    expected.insert(String::from("api"), String::from("new-api"));
    expected.remove("env:EMPTY");
    expected.insert(
        String::from("agent:agent.writer.mail-auth-code"),
        String::from("new-mail"),
    );
    expected.insert(
        String::from("agent:model-provider-api-key:provider"),
        String::from("new-provider"),
    );
    expected.remove("mcp:client");
    assert_eq!(store.0.lock().unwrap().values, expected);
    assert_eq!(
        restore.apply(),
        Err("Credential restore must be rolled back before reapplication")
    );
    restore.rollback().unwrap();
    let state = store.0.lock().unwrap();
    assert_eq!(state.values, initial());
    let keys = state
        .writes
        .iter()
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        &keys[5..],
        keys[..5].iter().rev().copied().collect::<Vec<_>>()
    );
}

#[test]
fn empty_authority_never_accesses_credentials_and_unchanged_keys_are_not_written() {
    let store = fixture();
    let mut restore = CredentialRestore::prepare(store.clone(), BTreeMap::new()).unwrap();
    restore.apply().unwrap();
    restore.rollback().unwrap();
    assert_eq!(store.0.lock().unwrap().reads, Vec::<String>::new());
    let mut restore = CredentialRestore::prepare(
        store.clone(),
        BTreeMap::from([(CredentialKey::ApiKey, Some(String::from("old-api")))]),
    )
    .unwrap();
    restore.apply().unwrap();
    restore.rollback().unwrap();
    assert_eq!(store.0.lock().unwrap().writes, Vec::new());
    assert_eq!(store.0.lock().unwrap().values, initial());
}

#[test]
fn failed_original_read_does_not_write_any_key_or_expose_store_diagnostics() {
    let store = fixture();
    store.0.lock().unwrap().fail_read = Some(String::from("mcp:client"));
    assert!(matches!(
        CredentialRestore::prepare(store.clone(), changes()),
        Err("Restore credential originals could not be read")
    ));
    assert_eq!(store.0.lock().unwrap().writes, Vec::new());
    assert_eq!(store.0.lock().unwrap().values, initial());
}

#[test]
fn every_failed_write_rolls_back_including_a_key_mutated_before_the_error() {
    for failure in 1..=5 {
        for mutate in [false, true] {
            let store = fixture();
            store.0.lock().unwrap().fail_writes.insert(failure, mutate);
            let mut restore = CredentialRestore::prepare(store.clone(), changes()).unwrap();
            assert_eq!(
                restore.apply(),
                Err("Credential restore failed; original values restored")
            );
            let state = store.0.lock().unwrap();
            assert_eq!(state.values, initial());
            assert_eq!(state.writes.len(), failure * 2);
            assert!(restore.dirty.is_empty());
        }
    }
}

#[test]
fn incomplete_rollback_continues_other_keys_and_retries_only_the_failed_inverse() {
    let store = fixture();
    // The final apply mutates then fails; its inverse fails without mutation.
    store.0.lock().unwrap().fail_writes = BTreeMap::from([(5, true), (6, false)]);
    let mut restore = CredentialRestore::prepare(store.clone(), changes()).unwrap();
    assert_eq!(restore.apply(), Err("Credential rollback is incomplete"));
    let mut pending = initial();
    pending.remove("mcp:client");
    assert_eq!(store.0.lock().unwrap().values, pending);
    assert_eq!(restore.dirty, BTreeSet::from([4]));
    restore.rollback().unwrap();
    let state = store.0.lock().unwrap();
    assert_eq!(state.values, initial());
    assert_eq!(state.writes.len(), 11);
    assert_eq!(
        state.writes[10],
        (String::from("mcp:client"), Some(String::from("old-client")))
    );
    drop(state);
    restore.rollback().unwrap();
    assert_eq!(store.0.lock().unwrap().writes.len(), 11);
}

#[test]
fn dropping_a_committed_transaction_never_performs_implicit_keyring_writes() {
    let store = fixture();
    let mut restore = CredentialRestore::prepare(store.clone(), changes()).unwrap();
    restore.apply().unwrap();
    let committed = store.0.lock().unwrap().values.clone();
    drop(restore);
    assert_eq!(store.0.lock().unwrap().values, committed);
    assert_eq!(store.0.lock().unwrap().writes.len(), 5);
}

#[test]
fn candidate_credentials_use_explicit_overrides_including_null_without_reading_local_values() {
    let store = fixture();
    let replacements = changes();
    let candidate = CandidateCredentials {
        live: store.as_ref(),
        replacements: &replacements,
    };
    assert_eq!(
        candidate.load_api_key().unwrap(),
        Some(String::from("new-api"))
    );
    assert_eq!(candidate.load_environment_value("EMPTY").unwrap(), None);
    assert_eq!(
        candidate
            .load_agent_setting_secret("agent.writer.mail-auth-code")
            .unwrap(),
        Some(String::from("new-mail"))
    );
    assert_eq!(candidate.load_mcp_client_secrets("client").unwrap(), None);
    assert_eq!(store.0.lock().unwrap().reads, Vec::<String>::new());
    assert_eq!(
        candidate.load_agent_setting_secret("unselected").unwrap(),
        Some(String::from("keep-unselected"))
    );
    assert_eq!(store.0.lock().unwrap().reads, ["agent:unselected"]);
    assert_eq!(store.0.lock().unwrap().values, initial());
}

#[test]
fn candidate_credentials_reject_all_writes_signing_access_and_propagate_read_failure() {
    let store = fixture();
    let replacements = BTreeMap::new();
    let candidate = CandidateCredentials {
        live: store.as_ref(),
        replacements: &replacements,
    };
    assert!(candidate.save_api_key(None).is_err());
    assert!(candidate.save_environment_value("EMPTY", None).is_err());
    assert!(
        candidate
            .save_agent_setting_secret("unselected", None)
            .is_err()
    );
    assert!(candidate.save_mcp_client_secrets("client", None).is_err());
    assert!(candidate.save_backup_signing_key("not-allowed").is_err());
    assert!(candidate.load_backup_signing_key().is_err());
    store.0.lock().unwrap().fail_read = Some(String::from("api"));
    assert!(candidate.load_api_key().is_err());
    assert_eq!(store.0.lock().unwrap().values, initial());
    assert_eq!(store.0.lock().unwrap().writes, Vec::new());
}
