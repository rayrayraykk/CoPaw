//! Durable Agent publication shared by save, restart recovery, and shutdown.

use qwenpaw_core::{AgentPublicationPhase, AgentPublicationState};
use sha2::{Digest as _, Sha256};

use super::{
    AgentCatalog, AgentContext, ApiError, AppServer, Core, DesktopWorkspace, MAX_CATALOG_BYTES,
    Uuid, Value, bad_request, catalog_path, conflict, context_from_catalog, desktop_workspace, fs,
    internal, load_agent_secret, payload_too_large, save_agent_secret, validate_catalog,
};
use crate::{
    AgentPublicationFiles, AgentPublicationSecret, AgentPublicationSecretScope,
    DesktopCredentialStore,
};

const RECOVERY_PENDING: &str =
    "Agent publication recovery is pending; retry Agent configuration save";

impl AppServer {
    pub(crate) async fn agent_catalog_guard(
        &self,
    ) -> Result<tokio::sync::MutexGuard<'_, ()>, ApiError> {
        let guard = self.inner.desktop_agents_lock.lock().await;
        self.ensure_agent_publication_available()?;
        Ok(guard)
    }

    pub(crate) fn ensure_agent_publication_available(&self) -> Result<(), ApiError> {
        if self
            .inner
            .core
            .read_agent_publication()
            .map_err(|_| conflict("Agent publication recovery is unavailable"))?
            .is_some()
        {
            return Err(conflict(RECOVERY_PENDING));
        }
        Ok(())
    }

    /// The caller holds the Agent lock; recovery never races another publisher.
    pub(crate) fn recover_agent_publication(&self) -> Result<(), ApiError> {
        if self
            .inner
            .core
            .read_agent_publication()
            .map_err(|_| conflict(RECOVERY_PENDING))?
            .is_none()
        {
            return Ok(());
        }
        let credentials = self
            .inner
            .desktop_credentials
            .as_ref()
            .ok_or_else(|| conflict(RECOVERY_PENDING))?;
        recover_before_initialization(
            &self.inner.core,
            credentials.as_ref(),
            desktop_workspace(self)?,
        )
        .map_err(|_| conflict(RECOVERY_PENDING))
    }

    pub(crate) async fn recover_agent_publication_on_shutdown(&self) {
        let _guard = self.inner.desktop_agents_lock.lock().await;
        if self.recover_agent_publication().is_err() {
            tracing::warn!("Agent publication recovery remains incomplete at shutdown");
        }
    }
}

pub(crate) fn recover_before_initialization(
    core: &Core,
    credentials: &dyn DesktopCredentialStore,
    workspace: &DesktopWorkspace,
) -> anyhow::Result<()> {
    let Some(decision) = core.read_agent_publication()? else {
        return Ok(());
    };
    let recovery = core
        .read_agent_publication_recovery(decision.id)?
        .ok_or_else(|| anyhow::anyhow!("Agent publication recovery metadata is missing"))?;
    let digest = core
        .agent_publication_journal_digest(decision.id)?
        .ok_or_else(|| anyhow::anyhow!("Agent publication recovery digest is missing"))?;
    let installation = core.installation_id()?;
    let files = AgentPublicationFiles::from_persisted(
        &recovery.journal,
        digest,
        installation,
        decision.id,
        &catalog_path(workspace),
    )?;
    let scope =
        AgentPublicationSecretScope::new(installation, decision.id, files.agent_id().into())?;
    if recovery.phase != AgentPublicationPhase::Cleaning {
        if decision.state == AgentPublicationState::Prepared {
            #[cfg(test)]
            process_tests::pause("rollback-start");
            // Try both inverse domains. A completed credential inverse is
            // detected from its live value, without an in-memory completion flag.
            let file_result = files.rollback();
            #[cfg(test)]
            process_tests::pause("files-rolled-back");
            let secret_result =
                if recovery.has_secret && recovery.phase == AgentPublicationPhase::Publishing {
                    credentials
                        .rollback_agent_publication_secret(&scope)
                        .map(|_| ())
                } else {
                    Ok(())
                };
            file_result?;
            secret_result
                .map_err(|_| anyhow::anyhow!("Agent credential rollback remains incomplete"))?;
            #[cfg(test)]
            process_tests::pause("secret-rolled-back");
        } else {
            anyhow::ensure!(
                recovery.phase == AgentPublicationPhase::Publishing,
                "Invalid committed publication phase"
            );
        }
        core.clean_agent_publication(decision.id, decision.state)?;
        #[cfg(test)]
        process_tests::pause("cleaning");
    }
    files.cleanup(decision.state)?;
    #[cfg(test)]
    process_tests::pause("files-cleaned");
    if recovery.has_secret {
        let secret = credentials
            .load_agent_publication_secret(&scope)
            .map_err(|_| anyhow::anyhow!("Agent credential cleanup read failed"))?;
        if let Some(secret) = secret {
            credentials
                .finish_agent_publication_secret(&scope, &secret)
                .map_err(|_| anyhow::anyhow!("Agent credential cleanup remains incomplete"))?;
        }
    }
    #[cfg(test)]
    process_tests::pause("secrets-cleaned");
    core.finish_agent_publication(decision.id, decision.state)?;
    #[cfg(test)]
    process_tests::pause("finished");
    Ok(())
}

pub(super) fn publish(
    server: &AppServer,
    context: &AgentContext,
    catalog: &AgentCatalog,
    config: &Value,
    secret: Option<&str>,
    channels: &crate::desktop_channels::ProfileUpdate,
) -> Result<(), ApiError> {
    let workspace = desktop_workspace(server)?;
    validate_catalog(catalog, workspace)?;
    if catalog.bootstrap_identity {
        return Err(conflict(
            "Agent identities must be initialized before publication",
        ));
    }
    let catalog_bytes = serde_json::to_vec_pretty(catalog)
        .map_err(|_| internal("Rust Agent catalog could not be encoded"))?;
    if catalog_bytes.len() as u64 > MAX_CATALOG_BYTES {
        return Err(payload_too_large("Rust Agent catalog is too large"));
    }
    let config_bytes = serde_json::to_vec_pretty(config)
        .map_err(|_| internal("Agent config could not be encoded"))?;
    match fs::symlink_metadata(context.workspace.join("agent.json")) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return Err(bad_request("Agent config is not a regular file")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(internal("Agent config could not be inspected")),
    }
    let previous = if secret.is_some() {
        load_agent_secret(server, &context.agent_id)?
    } else {
        None
    };
    let core = &server.inner.core;
    let installation = core
        .installation_id()
        .map_err(|_| internal("Agent installation identity is unavailable"))?;
    let transaction = Uuid::now_v7();
    let staged = AgentPublicationFiles::prepare(
        installation,
        transaction,
        &context.agent_id,
        &context.workspace.join("agent.json"),
        &config_bytes,
        &catalog_path(workspace),
        &catalog_bytes,
    )
    .map_err(|_| internal("Agent publication files could not be staged"))?;
    let Ok(bytes) = staged.encode() else {
        let _ = staged.cleanup(AgentPublicationState::Prepared);
        return Err(internal("Agent publication metadata could not be encoded"));
    };
    let digest: [u8; 32] = Sha256::digest(&bytes).into();
    if core
        .prepare_agent_publication_recovery(transaction, digest, &bytes, secret.is_some())
        .is_err()
    {
        let _ = staged.cleanup(AgentPublicationState::Prepared);
        return Err(internal("Agent publication recovery could not be prepared"));
    }
    #[cfg(test)]
    process_tests::pause("staging");
    let result = publish_prepared(
        server,
        context,
        catalog,
        secret,
        previous,
        channels,
        transaction,
    );
    if let Err(error) = result {
        if server.recover_agent_publication().is_err() {
            return Err(internal(
                "Agent publication failed and recovery is incomplete; recovery is retained by the host",
            ));
        }
        return Err(error);
    }
    #[cfg(test)]
    process_tests::pause("committed");
    server
        .recover_agent_publication()
        .map_err(|_| internal("Agent publication committed but recovery cleanup is incomplete"))
}

fn publish_prepared(
    server: &AppServer,
    context: &AgentContext,
    catalog: &AgentCatalog,
    secret: Option<&str>,
    previous: Option<String>,
    channels: &crate::desktop_channels::ProfileUpdate,
    transaction: Uuid,
) -> Result<(), ApiError> {
    let core = &server.inner.core;
    let installation = core
        .installation_id()
        .map_err(|_| internal("Agent installation identity is unavailable"))?;
    if secret.is_some() {
        let scope =
            AgentPublicationSecretScope::new(installation, transaction, context.agent_id.clone())
                .map_err(|_| internal("Agent publication credential scope is invalid"))?;
        let snapshot = AgentPublicationSecret::new(previous, secret.map(str::to_owned));
        server
            .inner
            .desktop_credentials
            .as_ref()
            .ok_or_else(|| internal("Credential storage is unavailable"))?
            .prepare_agent_publication_secret(&scope, &snapshot)
            .map_err(|_| internal("Agent credential recovery could not be prepared"))?;
        #[cfg(test)]
        process_tests::pause("secret-prepared");
    }
    core.start_agent_publication(transaction)
        .map_err(|_| internal("Agent publication could not enter publishing"))?;
    #[cfg(test)]
    process_tests::pause("publishing");
    if secret.is_some() {
        save_agent_secret(server, &context.agent_id, secret)?;
    }
    #[cfg(test)]
    process_tests::pause("secret-published");
    let current = context_from_catalog(catalog, &context.agent_id, false)?;
    if current.workspace != context.workspace || current.data_key != context.data_key {
        return Err(conflict(
            "Agent Workspace binding changed before publication",
        ));
    }
    let recovery = core
        .read_agent_publication_recovery(transaction)
        .map_err(|_| internal("Agent publication metadata is unavailable"))?
        .ok_or_else(|| internal("Agent publication metadata is missing"))?;
    let digest = core
        .agent_publication_journal_digest(transaction)
        .map_err(|_| internal("Agent publication digest is unavailable"))?
        .ok_or_else(|| internal("Agent publication digest is missing"))?;
    let files = AgentPublicationFiles::from_persisted(
        &recovery.journal,
        digest,
        installation,
        transaction,
        &catalog_path(desktop_workspace(server)?),
    )
    .map_err(|_| internal("Agent publication metadata is invalid"))?;
    files
        .apply()
        .map_err(|_| internal("Agent files could not be published"))?;
    #[cfg(test)]
    process_tests::pause("files-published");
    channels.commit(server, transaction)
}

#[cfg(test)]
#[path = "desktop_publication_file_recovery_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "desktop_publication_host_process_tests.rs"]
mod process_tests;
