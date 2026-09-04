use anyhow::Context;
use sha2::Digest as _;

const DESKTOP_CREDENTIAL_SERVICE: &str = "io.qwenpaw.desktop";
const MODEL_API_KEY_ACCOUNT: &str = "openai-compatible-api-key";
const ENVIRONMENT_ACCOUNT_PREFIX: &str = "environment-";
const ENVIRONMENT_VALUE_PREFIX: &str = "v1:";
const AGENT_SETTING_ACCOUNT_PREFIX: &str = "agent-setting-";
const AGENT_SETTING_VALUE_PREFIX: &str = "v1:";
const MCP_CLIENT_ACCOUNT_PREFIX: &str = "mcp-client-";
const MCP_CLIENT_VALUE_PREFIX: &str = "v1:";

/// Stores the Desktop model credential outside Core persistence.
pub trait DesktopCredentialStore: Send + Sync {
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
}

/// Uses Keychain Services, Windows Credential Manager, or Secret Service.
#[derive(Debug, Default)]
pub struct SystemDesktopCredentialStore;

impl DesktopCredentialStore for SystemDesktopCredentialStore {
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

fn mcp_client_entry(key: &str) -> anyhow::Result<keyring::Entry> {
    let digest = sha2::Sha256::digest(key.as_bytes());
    let account = format!("{MCP_CLIENT_ACCOUNT_PREFIX}{digest:x}");
    keyring::Entry::new(DESKTOP_CREDENTIAL_SERVICE, &account)
        .context("Desktop system credential storage is unavailable")
}
