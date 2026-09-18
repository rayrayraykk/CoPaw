use anyhow::Context;
use sha2::Digest as _;

#[path = "desktop_publication_credentials.rs"]
mod publication;
pub use publication::{AgentPublicationSecret, AgentPublicationSecretScope};

const DESKTOP_CREDENTIAL_SERVICE: &str = "io.qwenpaw.desktop";
const MODEL_API_KEY_ACCOUNT: &str = "openai-compatible-api-key";
const ENVIRONMENT_ACCOUNT_PREFIX: &str = "environment-";
const ENVIRONMENT_VALUE_PREFIX: &str = "v1:";
const AGENT_SETTING_ACCOUNT_PREFIX: &str = "agent-setting-";
const AGENT_SETTING_VALUE_PREFIX: &str = "v1:";
const MCP_CLIENT_ACCOUNT_PREFIX: &str = "mcp-client-";
const MCP_CLIENT_VALUE_PREFIX: &str = "v1:";
const BACKUP_SIGNING_KEY_ACCOUNT: &str = "backup-signing-key";
const BACKUP_SIGNING_KEY_VALUE_PREFIX: &str = "v1:";

/// Stores the Desktop model credential outside Core persistence.
pub trait DesktopCredentialStore: Send + Sync {
    /// Reads a private, installation-local Agent publication recovery record.
    ///
    /// # Errors
    /// Returns an error for unsupported stores, unavailable storage, or invalid records.
    fn load_agent_publication_secret(
        &self,
        _scope: &AgentPublicationSecretScope,
    ) -> anyhow::Result<Option<AgentPublicationSecret>> {
        anyhow::bail!("Agent publication credential recovery is unavailable")
    }

    /// Reserves a recovery account without replacing an existing record.
    /// The host must serialize publication; this is not a cross-process CAS.
    ///
    /// # Errors
    /// Returns an error for unsupported stores, occupied accounts, or storage failure.
    fn prepare_agent_publication_secret(
        &self,
        _scope: &AgentPublicationSecretScope,
        _secret: &AgentPublicationSecret,
    ) -> anyhow::Result<()> {
        anyhow::bail!("Agent publication credential recovery is unavailable")
    }

    /// Removes only the expected recovery record after host rollback or cleanup.
    /// An already absent record is accepted for interrupted cleanup retries.
    ///
    /// # Errors
    /// Returns an error for unsupported stores, a changed record, or storage failure.
    fn finish_agent_publication_secret(
        &self,
        _scope: &AgentPublicationSecretScope,
        _expected: &AgentPublicationSecret,
    ) -> anyhow::Result<()> {
        anyhow::bail!("Agent publication credential recovery is unavailable")
    }

    /// Restores the original live Agent secret while retaining its recovery record.
    /// The host must hold the publication lock and have a prepared decision.
    ///
    /// # Errors
    /// Returns an error for missing recovery, an independently changed live value,
    /// or any storage failure. No implicit recovery-account cleanup is performed.
    fn rollback_agent_publication_secret(
        &self,
        scope: &AgentPublicationSecretScope,
    ) -> anyhow::Result<AgentPublicationSecret> {
        let secret = self
            .load_agent_publication_secret(scope)
            .map_err(|_| anyhow::anyhow!("Agent publication credential recovery read failed"))?
            .ok_or_else(|| anyhow::anyhow!("Agent publication credential recovery is missing"))?;
        let key = scope.agent_secret_key();
        let current = self
            .load_agent_setting_secret(&key)
            .map_err(|_| anyhow::anyhow!("Agent publication live credential read failed"))?;
        if current.as_deref() != secret.previous() {
            anyhow::ensure!(
                current.as_deref() == secret.replacement(),
                "Agent publication live credential changed independently"
            );
            self.save_agent_setting_secret(&key, secret.previous())
                .map_err(|_| anyhow::anyhow!("Agent publication credential rollback failed"))?;
            let restored = self.load_agent_setting_secret(&key).map_err(|_| {
                anyhow::anyhow!("Agent publication credential rollback read failed")
            })?;
            anyhow::ensure!(
                restored.as_deref() == secret.previous(),
                "Agent publication credential rollback could not be verified"
            );
        }
        Ok(secret)
    }

    /// Loads the model API key, returning `None` when no key exists.
    ///
    /// # Errors
    ///
    /// Returns an error when the platform credential store cannot be read.
    fn load_api_key(&self) -> anyhow::Result<Option<String>>;

    /// Replaces or deletes the model API key.
    ///
    /// # Errors
    ///
    /// Returns an error when the platform credential store cannot be written.
    fn save_api_key(&self, api_key: Option<&str>) -> anyhow::Result<()>;

    /// Loads one persisted Desktop environment value.
    ///
    /// # Errors
    ///
    /// Returns an error when the platform credential store cannot be read.
    fn load_environment_value(&self, _key: &str) -> anyhow::Result<Option<String>> {
        Ok(None)
    }

    /// Replaces or deletes one persisted Desktop environment value.
    ///
    /// # Errors
    ///
    /// Returns an error when the platform credential store cannot be written.
    fn save_environment_value(&self, _key: &str, _value: Option<&str>) -> anyhow::Result<()> {
        anyhow::bail!("Desktop environment credential storage is unavailable")
    }

    /// Loads one secret used by Desktop Agent settings.
    ///
    /// # Errors
    ///
    /// Returns an error when the platform credential store cannot be read.
    fn load_agent_setting_secret(&self, _key: &str) -> anyhow::Result<Option<String>> {
        Ok(None)
    }

    /// Replaces or deletes one secret used by Desktop Agent settings.
    ///
    /// # Errors
    ///
    /// Returns an error when the platform credential store cannot be written.
    fn save_agent_setting_secret(&self, _key: &str, _value: Option<&str>) -> anyhow::Result<()> {
        anyhow::bail!("Desktop Agent credential storage is unavailable")
    }

    /// Loads serialized secret fields for one Desktop MCP client.
    ///
    /// # Errors
    ///
    /// Returns an error when the platform credential store cannot be read.
    fn load_mcp_client_secrets(&self, _key: &str) -> anyhow::Result<Option<String>> {
        Ok(None)
    }

    /// Replaces or deletes serialized secret fields for one Desktop MCP client.
    ///
    /// # Errors
    ///
    /// Returns an error when the platform credential store cannot be written.
    fn save_mcp_client_secrets(&self, _key: &str, _value: Option<&str>) -> anyhow::Result<()> {
        anyhow::bail!("Desktop MCP credential storage is unavailable")
    }

    /// Loads the installation-local backup signing key.
    ///
    /// # Errors
    ///
    /// Returns an error when the platform credential store cannot be read.
    fn load_backup_signing_key(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }

    /// Replaces the installation-local backup signing key.
    ///
    /// # Errors
    ///
    /// Returns an error when the platform credential store cannot be written.
    fn save_backup_signing_key(&self, _value: &str) -> anyhow::Result<()> {
        anyhow::bail!("Desktop backup credential storage is unavailable")
    }
}

/// Uses Keychain Services, Windows Credential Manager, or Secret Service.
#[derive(Debug, Default)]
pub struct SystemDesktopCredentialStore;

impl DesktopCredentialStore for SystemDesktopCredentialStore {
    fn load_agent_publication_secret(
        &self,
        scope: &AgentPublicationSecretScope,
    ) -> anyhow::Result<Option<AgentPublicationSecret>> {
        publication::load(&publication::entry(scope)?, scope)
    }

    fn prepare_agent_publication_secret(
        &self,
        scope: &AgentPublicationSecretScope,
        secret: &AgentPublicationSecret,
    ) -> anyhow::Result<()> {
        publication::prepare(&publication::entry(scope)?, scope, secret)
    }

    fn finish_agent_publication_secret(
        &self,
        scope: &AgentPublicationSecretScope,
        expected: &AgentPublicationSecret,
    ) -> anyhow::Result<()> {
        publication::finish(&publication::entry(scope)?, scope, expected)
    }

    fn load_api_key(&self) -> anyhow::Result<Option<String>> {
        let entry = model_api_key_entry()?;
        match entry.get_password() {
            Ok(api_key) => Ok(Some(api_key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error).context("failed to read the Desktop model credential"),
        }
    }

    fn save_api_key(&self, api_key: Option<&str>) -> anyhow::Result<()> {
        let entry = model_api_key_entry()?;
        match api_key {
            Some(api_key) => entry
                .set_password(api_key)
                .context("failed to save the Desktop model credential"),
            None => match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(error) => Err(error).context("failed to delete the Desktop model credential"),
            },
        }
    }

    fn load_environment_value(&self, key: &str) -> anyhow::Result<Option<String>> {
        let entry = environment_entry(key)?;
        match entry.get_password() {
            Ok(value) => value
                .strip_prefix(ENVIRONMENT_VALUE_PREFIX)
                .map(str::to_owned)
                .map(Some)
                .ok_or_else(|| anyhow::anyhow!("Desktop environment credential is invalid")),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error).context("failed to read a Desktop environment credential"),
        }
    }

    fn save_environment_value(&self, key: &str, value: Option<&str>) -> anyhow::Result<()> {
        let entry = environment_entry(key)?;
        match value {
            Some(value) => entry
                .set_password(&format!("{ENVIRONMENT_VALUE_PREFIX}{value}"))
                .context("failed to save a Desktop environment credential"),
            None => match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(error) => {
                    Err(error).context("failed to delete a Desktop environment credential")
                }
            },
        }
    }

    fn load_agent_setting_secret(&self, key: &str) -> anyhow::Result<Option<String>> {
        let entry = agent_setting_entry(key)?;
        match entry.get_password() {
            Ok(value) => value
                .strip_prefix(AGENT_SETTING_VALUE_PREFIX)
                .map(str::to_owned)
                .map(Some)
                .ok_or_else(|| anyhow::anyhow!("Desktop Agent credential is invalid")),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error).context("failed to read a Desktop Agent credential"),
        }
    }

    fn save_agent_setting_secret(&self, key: &str, value: Option<&str>) -> anyhow::Result<()> {
        let entry = agent_setting_entry(key)?;
        match value {
            Some(value) => entry
                .set_password(&format!("{AGENT_SETTING_VALUE_PREFIX}{value}"))
                .context("failed to save a Desktop Agent credential"),
            None => match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(error) => Err(error).context("failed to delete a Desktop Agent credential"),
            },
        }
    }

    fn load_mcp_client_secrets(&self, key: &str) -> anyhow::Result<Option<String>> {
        let entry = mcp_client_entry(key)?;
        match entry.get_password() {
            Ok(value) => value
                .strip_prefix(MCP_CLIENT_VALUE_PREFIX)
                .map(str::to_owned)
                .map(Some)
                .ok_or_else(|| anyhow::anyhow!("Desktop MCP credential is invalid")),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error).context("failed to read a Desktop MCP credential"),
        }
    }

    fn save_mcp_client_secrets(&self, key: &str, value: Option<&str>) -> anyhow::Result<()> {
        let entry = mcp_client_entry(key)?;
        match value {
            Some(value) => entry
                .set_password(&format!("{MCP_CLIENT_VALUE_PREFIX}{value}"))
                .context("failed to save a Desktop MCP credential"),
            None => match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(error) => Err(error).context("failed to delete a Desktop MCP credential"),
            },
        }
    }

    fn load_backup_signing_key(&self) -> anyhow::Result<Option<String>> {
        let entry = backup_signing_key_entry()?;
        match entry.get_password() {
            Ok(value) => value
                .strip_prefix(BACKUP_SIGNING_KEY_VALUE_PREFIX)
                .map(str::to_owned)
                .map(Some)
                .ok_or_else(|| anyhow::anyhow!("Desktop backup credential is invalid")),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error).context("failed to read the Desktop backup credential"),
        }
    }

    fn save_backup_signing_key(&self, value: &str) -> anyhow::Result<()> {
        backup_signing_key_entry()?
            .set_password(&format!("{BACKUP_SIGNING_KEY_VALUE_PREFIX}{value}"))
            .context("failed to save the Desktop backup credential")
    }
}

fn model_api_key_entry() -> anyhow::Result<keyring::Entry> {
    keyring::Entry::new(DESKTOP_CREDENTIAL_SERVICE, MODEL_API_KEY_ACCOUNT)
        .context("Desktop system credential storage is unavailable")
}

fn environment_entry(key: &str) -> anyhow::Result<keyring::Entry> {
    let digest = sha2::Sha256::digest(key.as_bytes());
    let account = format!("{ENVIRONMENT_ACCOUNT_PREFIX}{digest:x}");
    keyring::Entry::new(DESKTOP_CREDENTIAL_SERVICE, &account)
        .context("Desktop system credential storage is unavailable")
}

fn agent_setting_entry(key: &str) -> anyhow::Result<keyring::Entry> {
    let digest = sha2::Sha256::digest(key.as_bytes());
    let account = format!("{AGENT_SETTING_ACCOUNT_PREFIX}{digest:x}");
    keyring::Entry::new(DESKTOP_CREDENTIAL_SERVICE, &account)
        .context("Desktop system credential storage is unavailable")
}

pub(super) fn agent_mail_secret_key(agent_id: &str) -> String {
    format!("agent.{agent_id}.mail-auth-code")
}

fn mcp_client_entry(key: &str) -> anyhow::Result<keyring::Entry> {
    let digest = sha2::Sha256::digest(key.as_bytes());
    let account = format!("{MCP_CLIENT_ACCOUNT_PREFIX}{digest:x}");
    keyring::Entry::new(DESKTOP_CREDENTIAL_SERVICE, &account)
        .context("Desktop system credential storage is unavailable")
}

fn backup_signing_key_entry() -> anyhow::Result<keyring::Entry> {
    keyring::Entry::new(DESKTOP_CREDENTIAL_SERVICE, BACKUP_SIGNING_KEY_ACCOUNT)
        .context("Desktop system credential storage is unavailable")
}
