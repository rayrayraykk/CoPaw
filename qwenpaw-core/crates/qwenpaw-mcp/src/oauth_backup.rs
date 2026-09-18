//! Logical OAuth credential backup; no platform keyring identifiers in archives.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;

use serde::Deserialize;
use serde::Serialize;

use super::McpError;
use super::McpManager;
use super::McpOAuthCredentialStore;
use super::McpOAuthCredentials;
use super::OAuthActivity;
use super::oauth_account;
use super::validate_bearer_token;
use super::validate_refresh_token;
use super::validate_secure_oauth_url;

const MAX_BACKUP_BYTES: usize = 2 * 1024 * 1024;

/// Sensitive, versioned credentials keyed by configured MCP client ID.
///
/// A null value records a deliberately unauthorized client. This payload is
/// not encrypted and must only be exported with explicit secret-backup consent.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct McpOAuthBackup {
    version: u32,
    clients: BTreeMap<String, Option<McpOAuthCredentials>>,
}

impl std::fmt::Debug for McpOAuthBackup {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("McpOAuthBackup")
            .field("version", &self.version)
            .field("client_count", &self.clients.len())
            .finish_non_exhaustive()
    }
}

struct CredentialChange {
    account: String,
    before: Option<McpOAuthCredentials>,
    after: Option<McpOAuthCredentials>,
}

/// Retains original credentials for application-level restore or rollback.
///
/// The caller must hold exclusive Core access until commit or rollback finishes.
/// Dropping this value does not access the keyring or implicitly roll back.
#[must_use = "retain original credentials until the complete restore is committed"]
pub struct McpOAuthRestore {
    store: Arc<dyn McpOAuthCredentialStore>,
    activity: Arc<OAuthActivity>,
    changes: Vec<CredentialChange>,
    dirty: BTreeSet<usize>,
}

impl McpManager {
    /// Captures only credentials for currently configured OAuth clients.
    ///
    /// This performs blocking secure-store reads without network discovery.
    ///
    /// # Errors
    ///
    /// Returns an error if a credential cannot be read or the snapshot is invalid.
    pub fn backup_oauth_credentials(&self) -> Result<McpOAuthBackup, McpError> {
        let mut clients = BTreeMap::new();
        for (id, config) in &self.inner.clients {
            if config.oauth.is_none()
                || !matches!(config.transport.as_str(), "streamable_http" | "sse")
            {
                continue;
            }
            let credentials = self
                .inner
                .oauth_store
                .load(&oauth_account(id, config))
                .map_err(|_| backup_error("OAuth backup credential read failed"))?;
            clients.insert(id.clone(), credentials);
        }
        let backup = McpOAuthBackup {
            version: 1,
            clients,
        };
        self.validate_oauth_backup(&backup)?;
        Ok(backup)
    }

    /// Validates every restored credential and captures the original values.
    ///
    /// No credentials are changed here. Client configuration must already
    /// describe the intended restored resources, not unrelated local servers.
    ///
    /// # Errors
    ///
    /// Returns an error for unknown clients, mismatched resources, unsafe URLs,
    /// invalid credentials, unsupported versions, or failed secure-store reads.
    pub fn prepare_oauth_restore(
        &self,
        backup: &McpOAuthBackup,
    ) -> Result<McpOAuthRestore, McpError> {
        self.validate_oauth_backup(backup)?;
        let mut changes = Vec::with_capacity(backup.clients.len());
        for (id, after) in &backup.clients {
            let config = self.oauth_client_config(id)?;
            let account = oauth_account(id, &config);
            let before = self
                .inner
                .oauth_store
                .load(&account)
                .map_err(|_| backup_error("OAuth rollback credential read failed"))?;
            changes.push(CredentialChange {
                account,
                before,
                after: after.clone(),
            });
        }
        Ok(McpOAuthRestore {
            store: Arc::clone(&self.inner.oauth_store),
            activity: Arc::clone(&self.inner.oauth_activity),
            changes,
            dirty: BTreeSet::new(),
        })
    }

    fn validate_oauth_backup(&self, backup: &McpOAuthBackup) -> Result<(), McpError> {
        backup.validate()?;
        for (id, credentials) in &backup.clients {
            let config = self.oauth_client_config(id)?;
            if let Some(credentials) = credentials
                && credentials.resource != self.expand_environment(&config.url)?
            {
                return Err(backup_error(
                    "OAuth backup credential does not match its client",
                ));
            }
        }
        Ok(())
    }
}

impl McpOAuthBackup {
    /// Validates a logical archive without network or credential-store access.
    /// Restoring still requires matching the intended client's configuration
    /// through [`McpManager::prepare_oauth_restore`].
    ///
    /// # Errors
    ///
    /// Returns an error for unsupported versions, invalid credentials, URLs,
    /// client identifiers, or exceeded size limits.
    pub fn validate(&self) -> Result<(), McpError> {
        if self.version != 1 || self.clients.len() > crate::MAX_CLIENTS {
            return Err(backup_error(
                "OAuth backup version or client count is invalid",
            ));
        }
        let encoded =
            serde_json::to_vec(self).map_err(|_| backup_error("OAuth backup is invalid"))?;
        if encoded.len() > MAX_BACKUP_BYTES {
            return Err(backup_error("OAuth backup is too large"));
        }
        for (id, credentials) in &self.clients {
            if id.trim().is_empty() || id.len() > 1024 || id.chars().any(char::is_control) {
                return Err(backup_error("OAuth backup client identifier is invalid"));
            }
            let Some(credentials) = credentials else {
                continue;
            };
            if credentials.client_id.is_empty()
                || !credentials.expires_at.is_finite()
                || credentials.expires_at < 0.0
                || serde_json::to_vec(credentials)
                    .map_or(true, |value| value.len() > super::MAX_OAUTH_RESPONSE_BYTES)
            {
                return Err(backup_error(
                    "OAuth backup credential does not match its client",
                ));
            }
            for url in [
                &credentials.resource,
                &credentials.issuer,
                &credentials.authorization_endpoint,
                &credentials.token_endpoint,
            ] {
                validate_secure_oauth_url("OAuth backup endpoint", url)?;
            }
            for value in [&credentials.client_id, &credentials.scope] {
                if value.len() > 16_384 || value.chars().any(char::is_control) {
                    return Err(backup_error("OAuth backup metadata is invalid"));
                }
            }
            if !credentials.access_token.is_empty() {
                validate_bearer_token(&credentials.access_token)?;
            }
            if !credentials.refresh_token.is_empty() {
                validate_refresh_token(&credentials.refresh_token)?;
            }
        }
        Ok(())
    }
}

impl McpOAuthRestore {
    /// Applies the credential replacement, rolling back a failed write.
    ///
    /// This blocks on the secure store and must run in a restore-owned worker.
    /// A store may mutate before returning an error; that key is rolled back too.
    ///
    /// # Errors
    ///
    /// Returns an error when application or rollback fails. If rollback fails,
    /// retain this object and retry [`Self::rollback`] before releasing restore.
    pub fn apply(&mut self) -> Result<(), McpError> {
        if !self.dirty.is_empty() {
            return Err(backup_error(
                "OAuth restore must be rolled back before reapplication",
            ));
        }
        let _activity = self.activity.tasks.token();
        for (index, change) in self.changes.iter().enumerate() {
            self.dirty.insert(index);
            if replace(self.store.as_ref(), &change.account, change.after.as_ref()).is_err() {
                self.rollback()?;
                return Err(backup_error(
                    "OAuth credential restore failed; original values restored",
                ));
            }
        }
        Ok(())
    }

    /// Restores original values for all keys touched by this operation.
    ///
    /// # Errors
    ///
    /// Returns an error if any key cannot be rolled back. Other keys are still
    /// attempted, and only failed keys remain pending for the next retry.
    pub fn rollback(&mut self) -> Result<(), McpError> {
        let _activity = self.activity.tasks.token();
        for index in self.dirty.iter().rev().copied().collect::<Vec<_>>() {
            let change = &self.changes[index];
            if replace(self.store.as_ref(), &change.account, change.before.as_ref()).is_ok() {
                self.dirty.remove(&index);
            }
        }
        if self.dirty.is_empty() {
            Ok(())
        } else {
            Err(backup_error("OAuth credential rollback is incomplete"))
        }
    }
}

fn replace(
    store: &dyn McpOAuthCredentialStore,
    account: &str,
    credentials: Option<&McpOAuthCredentials>,
) -> Result<(), String> {
    match credentials {
        Some(credentials) => store.save(account, credentials),
        None => store.delete(account),
    }
}

fn backup_error(message: &str) -> McpError {
    McpError::OAuth(message.to_owned())
}

#[cfg(test)]
#[path = "oauth_backup_tests.rs"]
mod tests;
