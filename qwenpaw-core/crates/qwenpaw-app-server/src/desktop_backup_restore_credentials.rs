//! Rollback ownership for the explicitly authorized Desktop credential changes.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;

use super::DesktopCredentialStore;

/// The signing key deliberately has no representation in a restore plan.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum CredentialKey {
    ApiKey,
    Environment(String),
    AgentSetting(String),
    McpClient(String),
}

impl CredentialKey {
    pub(super) fn load(
        &self,
        store: &dyn DesktopCredentialStore,
    ) -> anyhow::Result<Option<String>> {
        match self {
            Self::ApiKey => store.load_api_key(),
            Self::Environment(key) => store.load_environment_value(key),
            Self::AgentSetting(key) => store.load_agent_setting_secret(key),
            Self::McpClient(key) => store.load_mcp_client_secrets(key),
        }
    }

    fn save(&self, store: &dyn DesktopCredentialStore, value: Option<&str>) -> anyhow::Result<()> {
        match self {
            Self::ApiKey => store.save_api_key(value),
            Self::Environment(key) => store.save_environment_value(key, value),
            Self::AgentSetting(key) => store.save_agent_setting_secret(key, value),
            Self::McpClient(key) => store.save_mcp_client_secrets(key, value),
        }
    }
}

struct Change {
    key: CredentialKey,
    before: Option<String>,
    after: Option<String>,
}

/// Read-only view for detached runtime hydration. Explicit null overrides a
/// local secret; absent keys retain the local secure-store value.
pub(super) struct CandidateCredentials<'a> {
    pub(super) live: &'a dyn DesktopCredentialStore,
    pub(super) replacements: &'a BTreeMap<CredentialKey, Option<String>>,
}

impl CandidateCredentials<'_> {
    fn load(&self, key: &CredentialKey) -> anyhow::Result<Option<String>> {
        match self.replacements.get(key) {
            Some(value) => Ok(value.clone()),
            None => key.load(self.live),
        }
    }
}

impl DesktopCredentialStore for CandidateCredentials<'_> {
    fn load_api_key(&self) -> anyhow::Result<Option<String>> {
        self.load(&CredentialKey::ApiKey)
    }

    fn save_api_key(&self, _: Option<&str>) -> anyhow::Result<()> {
        anyhow::bail!("Candidate credential storage is read-only")
    }

    fn load_environment_value(&self, key: &str) -> anyhow::Result<Option<String>> {
        self.load(&CredentialKey::Environment(key.to_owned()))
    }

    fn load_agent_setting_secret(&self, key: &str) -> anyhow::Result<Option<String>> {
        self.load(&CredentialKey::AgentSetting(key.to_owned()))
    }

    fn load_mcp_client_secrets(&self, key: &str) -> anyhow::Result<Option<String>> {
        self.load(&CredentialKey::McpClient(key.to_owned()))
    }

    fn load_backup_signing_key(&self) -> anyhow::Result<Option<String>> {
        anyhow::bail!("Candidate runtime cannot access the backup signing key")
    }
}

/// Retained by the application-owned worker until all restore domains commit.
/// No Drop implementation: unwinding must not silently perform keyring writes.
#[must_use = "retain credential originals until commit or successful rollback"]
pub(super) struct CredentialRestore {
    store: Arc<dyn DesktopCredentialStore>,
    changes: Vec<Change>,
    dirty: BTreeSet<usize>,
}

impl CredentialRestore {
    /// Capture every original before the first write. The caller must first
    /// validate all logical secret payloads, filter scopes/protected keys and
    /// hold the exclusive Core lease; raw archive keys are not authorized input.
    /// Missing scope is an empty map, not a request to delete existing keys.
    pub(super) fn prepare(
        store: Arc<dyn DesktopCredentialStore>,
        authorized: BTreeMap<CredentialKey, Option<String>>,
    ) -> Result<Self, &'static str> {
        let mut changes = Vec::with_capacity(authorized.len());
        for (key, after) in authorized {
            let before = key
                .load(store.as_ref())
                .map_err(|_| "Restore credential originals could not be read")?;
            if before != after {
                changes.push(Change { key, before, after });
            }
        }
        Ok(Self {
            store,
            changes,
            dirty: BTreeSet::new(),
        })
    }

    /// Blocking writes belong to the restore worker, not the HTTP future.
    pub(super) fn apply(&mut self) -> Result<(), &'static str> {
        if !self.dirty.is_empty() {
            return Err("Credential restore must be rolled back before reapplication");
        }
        for (index, change) in self.changes.iter().enumerate() {
            // A keyring implementation may mutate a key before reporting failure.
            self.dirty.insert(index);
            if change
                .key
                .save(self.store.as_ref(), change.after.as_deref())
                .is_err()
            {
                self.rollback()?;
                return Err("Credential restore failed; original values restored");
            }
        }
        Ok(())
    }

    /// Attempts every dirty key in reverse order. Retain this object and the
    /// exclusive lease if an inverse write fails; retries touch failed keys only.
    pub(super) fn rollback(&mut self) -> Result<(), &'static str> {
        for index in self.dirty.iter().rev().copied().collect::<Vec<_>>() {
            let change = &self.changes[index];
            if change
                .key
                .save(self.store.as_ref(), change.before.as_deref())
                .is_ok()
            {
                self.dirty.remove(&index);
            }
        }
        if self.dirty.is_empty() {
            Ok(())
        } else {
            Err("Credential rollback is incomplete")
        }
    }
}

#[cfg(test)]
#[path = "desktop_backup_restore_credentials_tests.rs"]
mod tests;
