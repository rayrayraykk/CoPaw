use super::*;
use crate::DesktopCredentialStore;
use pretty_assertions::assert_eq;

#[derive(Default)]
struct Live {
    value: Option<String>,
    reads: usize,
    writes: usize,
    fail_read: Option<usize>,
    fault: Fault,
}

struct HostStore {
    entry: Entry,
    live: Arc<Mutex<Live>>,
}

impl DesktopCredentialStore for HostStore {
    fn load_api_key(&self) -> anyhow::Result<Option<String>> {
        panic!("no model credentials")
    }
    fn save_api_key(&self, _: Option<&str>) -> anyhow::Result<()> {
        panic!("no model credentials")
    }

    fn load_agent_publication_secret(
        &self,
        scope: &AgentPublicationSecretScope,
    ) -> anyhow::Result<Option<AgentPublicationSecret>> {
        load(&self.entry, scope)
    }

    fn load_agent_setting_secret(&self, key: &str) -> anyhow::Result<Option<String>> {
        assert_eq!(key, "agent.writer.mail-auth-code");
        let mut live = self.live.lock().unwrap();
        live.reads += 1;
        anyhow::ensure!(
            live.fail_read != Some(live.reads),
            "fixture-secret-live-read"
        );
        Ok(live.value.clone())
    }

    fn save_agent_setting_secret(&self, key: &str, value: Option<&str>) -> anyhow::Result<()> {
        assert_eq!(key, "agent.writer.mail-auth-code");
        let mut live = self.live.lock().unwrap();
        live.writes += 1;
        anyhow::ensure!(
            !matches!(live.fault, Fault::Before),
            "fixture-secret-live-before"
        );
        if !matches!(live.fault, Fault::Discard) {
            live.value = value.map(str::to_owned);
        }
        anyhow::ensure!(
            !matches!(live.fault, Fault::After),
            "fixture-secret-live-after"
        );
        Ok(())
    }
}

fn host(
    secret: &AgentPublicationSecret,
    current: Option<&str>,
) -> (HostStore, AgentPublicationSecretScope) {
    let (_, scope, entry) = fixture();
    prepare(&entry, &scope, secret).unwrap();
    (
        HostStore {
            entry,
            live: Arc::new(Mutex::new(Live {
                value: current.map(str::to_owned),
                ..Live::default()
            })),
        },
        scope,
    )
}

#[test]
fn publication_credentials_rollback_restores_missing_empty_and_present_without_repeating_completed_writes()
 {
    for previous in [None, Some(""), Some("fixture-secret-old")] {
        for replacement in [None, Some(""), Some("fixture-secret-new")] {
            let secret = AgentPublicationSecret::new(
                previous.map(str::to_owned),
                replacement.map(str::to_owned),
            );
            for current in [previous, replacement] {
                let (host, scope) = host(&secret, current);
                let records = host.entry.vault.lock().unwrap().records.clone();
                for _ in 0..3 {
                    assert_eq!(
                        host.rollback_agent_publication_secret(&scope).unwrap(),
                        secret
                    );
                }
                let live = host.live.lock().unwrap();
                assert_eq!(
                    (live.value.as_deref(), live.writes),
                    (previous, usize::from(current != previous))
                );
                let vault = host.entry.vault.lock().unwrap();
                assert_eq!(
                    (&vault.records, vault.writes, vault.deletes),
                    (&records, 1, 0)
                );
            }
        }
    }
}

#[test]
fn publication_credentials_rollback_preserves_independently_changed_live_value_and_recovery() {
    let (host, scope) = host(&secret(), Some("fixture-secret-independent"));
    let records = host.entry.vault.lock().unwrap().records.clone();
    redacted(&host.rollback_agent_publication_secret(&scope).unwrap_err());
    let live = host.live.lock().unwrap();
    assert_eq!(
        (live.value.as_deref(), live.writes),
        (Some("fixture-secret-independent"), 0)
    );
    assert_eq!(host.entry.vault.lock().unwrap().records, records);
}

#[test]
fn publication_credentials_rollback_missing_or_unreadable_recovery_never_accesses_live_account() {
    for missing in [true, false] {
        let (host, scope) = host(&secret(), Some("fixture-secret-new"));
        {
            let mut vault = host.entry.vault.lock().unwrap();
            if missing {
                vault.records.clear();
            } else {
                vault.fail_read = Some(vault.reads + 1);
            }
        }
        redacted(&host.rollback_agent_publication_secret(&scope).unwrap_err());
        let live = host.live.lock().unwrap();
        assert_eq!(
            (live.value.as_deref(), live.reads, live.writes),
            (Some("fixture-secret-new"), 0, 0)
        );
        assert_eq!(host.entry.vault.lock().unwrap().deletes, 0);
    }
}

#[test]
fn publication_credentials_rollback_failed_write_is_recoverable_from_a_new_host_wrapper() {
    for fault in [Fault::Before, Fault::After, Fault::Discard] {
        let secret = secret();
        let (host, scope) = host(&secret, secret.replacement());
        host.live.lock().unwrap().fault = fault;
        redacted(&host.rollback_agent_publication_secret(&scope).unwrap_err());
        assert_eq!(load(&host.entry, &scope).unwrap(), Some(secret.clone()));
        assert_eq!(host.entry.vault.lock().unwrap().deletes, 0);
        let next = HostStore {
            entry: Entry::new(&host.entry.vault, &scope),
            live: Arc::clone(&host.live),
        };
        drop(host);
        next.live.lock().unwrap().fault = Fault::None;
        assert_eq!(
            next.rollback_agent_publication_secret(&scope).unwrap(),
            secret
        );
        let live = next.live.lock().unwrap();
        assert_eq!(
            (live.value.as_deref(), live.writes),
            (
                secret.previous(),
                if matches!(fault, Fault::After) { 1 } else { 2 }
            )
        );
    }
}

#[test]
fn publication_credentials_rollback_failed_reads_retain_inverse_and_do_not_repeat_verified_live_state()
 {
    for read in [1, 2] {
        let secret = secret();
        let (host, scope) = host(&secret, secret.replacement());
        host.live.lock().unwrap().fail_read = Some(read);
        redacted(&host.rollback_agent_publication_secret(&scope).unwrap_err());
        assert_eq!(load(&host.entry, &scope).unwrap(), Some(secret.clone()));
        assert_eq!(host.entry.vault.lock().unwrap().deletes, 0);
        assert_eq!(
            host.rollback_agent_publication_secret(&scope).unwrap(),
            secret
        );
        let live = host.live.lock().unwrap();
        assert_eq!((live.value.as_deref(), live.writes), (secret.previous(), 1));
    }
}
