use anyhow::Context;

const DESKTOP_CREDENTIAL_SERVICE: &str = "io.qwenpaw.desktop";
const MODEL_API_KEY_ACCOUNT: &str = "openai-compatible-api-key";

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
}

fn model_api_key_entry() -> anyhow::Result<keyring::Entry> {
    keyring::Entry::new(DESKTOP_CREDENTIAL_SERVICE, MODEL_API_KEY_ACCOUNT)
        .context("Desktop system credential storage is unavailable")
}
