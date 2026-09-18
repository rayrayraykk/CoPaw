//! Logical credential validation independent of any live secure store.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use super::super::desktop_agent_settings;
use super::super::desktop_mcp;
use super::super::desktop_models;
use super::AppServer;
use super::SecretSnapshot;
use super::restore_credentials::CredentialKey;

/// Catalog keys, not an enumeration of unrelated operating-system credentials.
pub(super) fn known_keys(
    server: &AppServer,
    agent_ids: &[String],
) -> Result<BTreeSet<CredentialKey>, &'static str> {
    let mut keys = BTreeSet::from([CredentialKey::ApiKey]);
    keys.extend(
        server
            .inner
            .core
            .read_environment_keys()
            .map_err(|_| "Environment keys could not be loaded")?
            .into_iter()
            .map(CredentialKey::Environment),
    );
    keys.extend(
        server
            .inner
            .core
            .runtime_environment()
            .map_err(|_| "Runtime environment keys could not be loaded")?
            .into_keys()
            .map(CredentialKey::Environment),
    );
    for id in agent_ids {
        super::super::desktop_agents::validate_agent_id(id, true)
            .map_err(|_| "Agent credential catalog is invalid")?;
        keys.extend(
            desktop_agent_settings::backup_secret_keys(id)
                .into_iter()
                .map(CredentialKey::AgentSetting),
        );
    }
    keys.extend(
        server
            .inner
            .core
            .mcp_client_settings()
            .into_iter()
            .map(|client| CredentialKey::McpClient(client.key)),
    );
    keys.extend(
        desktop_models::backup_provider_ids(server)?
            .into_iter()
            .filter(|id| id != "openai-compatible")
            .map(|id| CredentialKey::AgentSetting(format!("model-provider-api-key:{id}"))),
    );
    Ok(keys)
}

pub(super) struct SecretRestorePlan {
    pub(super) credentials: BTreeMap<CredentialKey, Option<String>>,
    pub(super) oauth: Option<qwenpaw_core::McpOAuthBackup>,
}

/// Secret scope is independent of Workspace/Agent selection. Missing payload
/// or disabled scope is a no-op, never authorization to clear local credentials.
/// `preserve_mcp` means the local MCP protected overlay was actually retained.
pub(super) fn plan_restore(
    snapshot: Option<&SecretSnapshot>,
    include_secrets: bool,
    preserve_mcp: bool,
    known: &BTreeSet<CredentialKey>,
) -> Result<SecretRestorePlan, &'static str> {
    let Some(snapshot) = snapshot.filter(|_| include_secrets) else {
        return Ok(SecretRestorePlan {
            credentials: BTreeMap::new(),
            oauth: None,
        });
    };
    snapshot.validate()?;
    let mut credentials = known
        .iter()
        .cloned()
        .map(|key| (key, None))
        .collect::<BTreeMap<_, _>>();
    credentials.insert(CredentialKey::ApiKey, snapshot.api_key.clone());
    credentials.extend(
        snapshot
            .environment
            .iter()
            .map(|(key, value)| (CredentialKey::Environment(key.clone()), Some(value.clone()))),
    );
    credentials.extend(snapshot.agent_settings.iter().map(|(key, value)| {
        (
            CredentialKey::AgentSetting(key.clone()),
            Some(value.clone()),
        )
    }));
    credentials.extend(snapshot.model_providers.iter().map(|(key, value)| {
        (
            CredentialKey::AgentSetting(format!("model-provider-api-key:{key}")),
            Some(value.clone()),
        )
    }));
    credentials.extend(
        snapshot
            .mcp_clients
            .iter()
            .map(|(key, value)| (CredentialKey::McpClient(key.clone()), Some(value.clone()))),
    );
    if preserve_mcp {
        credentials.retain(|key, _| !matches!(key, CredentialKey::McpClient(_)));
    }
    Ok(SecretRestorePlan {
        credentials,
        oauth: (!preserve_mcp).then(|| snapshot.oauth.clone()).flatten(),
    })
}

impl SecretSnapshot {
    pub(super) fn validate(&self) -> Result<(), &'static str> {
        if self.version != 1
            || self.agent_settings.len() > 256 * 4
            || self.mcp_clients.len() > 32
            || self.model_providers.len() > 128
        {
            return Err("Backup credential version or entry count is invalid");
        }
        qwenpaw_core::Core::validate_runtime_environment(&self.environment)
            .map_err(|_| "Backup environment credentials are invalid")?;
        if let Some(value) = &self.api_key {
            desktop_models::validate_backup_secret("openai-compatible", value)?;
        }
        for (key, value) in &self.agent_settings {
            desktop_agent_settings::validate_backup_secret(key, value)?;
        }
        for (key, value) in &self.model_providers {
            if key == "openai-compatible" {
                return Err("Backup duplicates the default model credential");
            }
            desktop_models::validate_backup_secret(key, value)?;
        }
        for (key, value) in &self.mcp_clients {
            if key.trim().is_empty() || key.len() > 1024 || key.chars().any(char::is_control) {
                return Err("Backup MCP client identifier is invalid");
            }
            desktop_mcp::validate_backup_secrets(value)?;
        }
        if let Some(oauth) = &self.oauth {
            oauth
                .validate()
                .map_err(|_| "Backup OAuth credentials are invalid")?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "desktop_backup_secrets_tests.rs"]
mod tests;
