//! Private recovery accounts, separate from live Agent and backup credentials.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

const ACCOUNT_PREFIX: &str = "agent-publication-";

/// Recovery identity: stable installation ID and unique per-publication transaction ID.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentPublicationSecretScope {
    installation: Uuid,
    transaction: Uuid,
    agent_id: String,
}

impl<'de> Deserialize<'de> for AgentPublicationSecretScope {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            installation: Uuid,
            transaction: Uuid,
            agent_id: String,
        }
        let fields = Fields::deserialize(deserializer)?;
        Self::new(fields.installation, fields.transaction, fields.agent_id)
            .map_err(|_| serde::de::Error::custom("Invalid Agent publication credential scope"))
    }
}

impl AgentPublicationSecretScope {
    /// Binds recovery to an installation, publication, and validated Agent ID.
    ///
    /// # Errors
    /// Returns an error for nil IDs or an invalid Agent ID.
    pub fn new(installation: Uuid, transaction: Uuid, agent_id: String) -> anyhow::Result<Self> {
        anyhow::ensure!(
            !installation.is_nil()
                && !transaction.is_nil()
                && qwenpaw_storage::is_valid_agent_id(&agent_id),
            "Agent publication credential scope is invalid"
        );
        Ok(Self {
            installation,
            transaction,
            agent_id,
        })
    }

    fn account(&self) -> String {
        // Agent identity is inside the record: a reused transaction must not
        // allocate a second account merely by changing the Agent ID.
        format!("{ACCOUNT_PREFIX}{}-{}", self.installation, self.transaction)
    }

    pub(super) fn agent_secret_key(&self) -> String {
        super::agent_mail_secret_key(&self.agent_id)
    }
}

/// Original and intended values; neither Debug nor errors reveal their contents.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentPublicationSecret {
    previous: SecretValue,
    replacement: SecretValue,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "state",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum SecretValue {
    Missing,
    Present(String),
}

impl AgentPublicationSecret {
    #[must_use]
    pub fn new(previous: Option<String>, replacement: Option<String>) -> Self {
        Self {
            previous: previous.map_or(SecretValue::Missing, SecretValue::Present),
            replacement: replacement.map_or(SecretValue::Missing, SecretValue::Present),
        }
    }

    #[must_use]
    pub fn previous(&self) -> Option<&str> {
        match &self.previous {
            SecretValue::Missing => None,
            SecretValue::Present(value) => Some(value),
        }
    }

    #[must_use]
    pub fn replacement(&self) -> Option<&str> {
        match &self.replacement {
            SecretValue::Missing => None,
            SecretValue::Present(value) => Some(value),
        }
    }
}

impl std::fmt::Debug for AgentPublicationSecret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("AgentPublicationSecret([REDACTED])")
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u32,
    scope: AgentPublicationSecretScope,
    secret: AgentPublicationSecret,
}

// A per-entry seam keeps fault tests off the OS credential store, without
// changing the process-global keyring provider or live Agent credential paths.
pub(super) trait RecoveryEntry {
    fn read(&self) -> anyhow::Result<Option<String>>;
    fn write(&self, value: &str) -> anyhow::Result<()>;
    fn delete(&self) -> anyhow::Result<()>;
}

impl RecoveryEntry for keyring::Entry {
    fn read(&self) -> anyhow::Result<Option<String>> {
        match self.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => anyhow::bail!("Agent publication credential read failed"),
        }
    }

    fn write(&self, value: &str) -> anyhow::Result<()> {
        self.set_password(value)
            .map_err(|_| anyhow::anyhow!("Agent publication credential write failed"))
    }

    fn delete(&self) -> anyhow::Result<()> {
        match self.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => anyhow::bail!("Agent publication credential cleanup failed"),
        }
    }
}

pub(super) fn entry(scope: &AgentPublicationSecretScope) -> anyhow::Result<keyring::Entry> {
    keyring::Entry::new(super::DESKTOP_CREDENTIAL_SERVICE, &scope.account())
        .map_err(|_| anyhow::anyhow!("Agent publication credential storage is unavailable"))
}

pub(super) fn load(
    entry: &impl RecoveryEntry,
    scope: &AgentPublicationSecretScope,
) -> anyhow::Result<Option<AgentPublicationSecret>> {
    let Some(raw) = entry
        .read()
        .map_err(|_| anyhow::anyhow!("Agent publication credential read failed"))?
    else {
        return Ok(None);
    };
    let record: Record = serde_json::from_str(&raw)
        .map_err(|_| anyhow::anyhow!("Agent publication credential record is invalid"))?;
    anyhow::ensure!(
        record.version == 1 && record.scope == *scope,
        "Agent publication credential record does not match its scope"
    );
    Ok(Some(record.secret))
}

pub(super) fn prepare(
    entry: &impl RecoveryEntry,
    scope: &AgentPublicationSecretScope,
    secret: &AgentPublicationSecret,
) -> anyhow::Result<()> {
    // Treat even unreadable/malformed occupied records as retained recovery.
    anyhow::ensure!(
        load(entry, scope)?.is_none(),
        "Agent publication credential record already exists"
    );
    let raw = serde_json::to_string(&Record {
        version: 1,
        scope: scope.clone(),
        secret: secret.clone(),
    })
    .map_err(|_| anyhow::anyhow!("Agent publication credential record could not be encoded"))?;
    entry
        .write(&raw)
        .map_err(|_| anyhow::anyhow!("Agent publication credential write failed"))?;
    anyhow::ensure!(
        load(entry, scope)?.as_ref() == Some(secret),
        "Agent publication credential write could not be verified"
    );
    Ok(())
}

pub(super) fn finish(
    entry: &impl RecoveryEntry,
    scope: &AgentPublicationSecretScope,
    expected: &AgentPublicationSecret,
) -> anyhow::Result<()> {
    if let Some(actual) = load(entry, scope)? {
        anyhow::ensure!(
            actual == *expected,
            "Agent publication credential record changed"
        );
        entry
            .delete()
            .map_err(|_| anyhow::anyhow!("Agent publication credential cleanup failed"))?;
    }
    anyhow::ensure!(
        load(entry, scope)?.is_none(),
        "Agent publication credential cleanup could not be verified"
    );
    Ok(())
}

#[cfg(test)]
#[path = "desktop_publication_credentials_tests.rs"]
mod tests;
