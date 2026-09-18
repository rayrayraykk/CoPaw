//! Host-owned publication decisions respecting the Core restore barrier.

use qwenpaw_storage::{AgentPublication, AgentPublicationState};
use uuid::Uuid;

use super::{Core, CoreError};

impl Core {
    /// Atomically prepares non-secret host recovery metadata and its decision.
    ///
    /// # Errors
    /// Returns an error during restoration, pending recovery, or storage failure.
    pub fn prepare_agent_publication_recovery(
        &self,
        id: Uuid,
        digest: [u8; 32],
        journal: &[u8],
        has_secret: bool,
    ) -> Result<(), CoreError> {
        let _operation = self.operation_guard()?;
        self.inner
            .store
            .prepare_agent_publication_recovery(id, digest, journal, has_secret)
            .map_err(CoreError::storage)
    }

    /// Reads non-secret recovery metadata for the exact pending publication.
    ///
    /// # Errors
    /// Returns an error for invalid or unavailable recovery data.
    pub fn read_agent_publication_recovery(
        &self,
        id: Uuid,
    ) -> Result<Option<crate::AgentPublicationRecovery>, CoreError> {
        self.inner
            .store
            .read_agent_publication_recovery(id)
            .map_err(CoreError::storage)
    }

    /// Allows publication after private snapshots have been prepared.
    ///
    /// # Errors
    /// Returns an error during restoration, for a stale phase, or on storage failure.
    pub fn start_agent_publication(&self, id: Uuid) -> Result<(), CoreError> {
        let _operation = self.operation_guard()?;
        self.inner
            .store
            .start_agent_publication(id)
            .map_err(CoreError::storage)
    }

    /// Marks inverse operations complete before recovery snapshots are removed.
    ///
    /// # Errors
    /// Returns an error during restoration, for a stale decision, or on storage failure.
    pub fn clean_agent_publication(
        &self,
        id: Uuid,
        state: AgentPublicationState,
    ) -> Result<(), CoreError> {
        let _operation = self.operation_guard()?;
        self.inner
            .store
            .clean_agent_publication(id, state)
            .map_err(CoreError::storage)
    }

    /// Reads or initializes the installation identity outside logical backup data.
    ///
    /// # Errors
    /// Returns an error during restoration or when the local identity is invalid.
    pub fn installation_id(&self) -> Result<Uuid, CoreError> {
        let _operation = self.operation_guard()?;
        self.inner
            .store
            .installation_id()
            .map_err(CoreError::storage)
    }

    /// Atomically reserves the publication and trusted file-journal digest.
    ///
    /// # Errors
    /// Returns an error during restoration, pending recovery, or storage failure.
    pub fn prepare_agent_publication_with_journal(
        &self,
        id: Uuid,
        digest: [u8; 32],
    ) -> Result<(), CoreError> {
        let _operation = self.operation_guard()?;
        self.inner
            .store
            .prepare_agent_publication_with_journal(id, digest)
            .map_err(CoreError::storage)
    }

    /// Reads the trusted file-receipt digest for the exact pending publication.
    ///
    /// # Errors
    /// Returns an error for a stale ID or unavailable/invalid local storage.
    pub fn agent_publication_journal_digest(
        &self,
        id: Uuid,
    ) -> Result<Option<[u8; 32]>, CoreError> {
        self.inner
            .store
            .agent_publication_journal_digest(id)
            .map_err(CoreError::storage)
    }

    /// Reads the local publication decision for host recovery.
    ///
    /// # Errors
    /// Returns an error when the decision is unreadable or invalid.
    pub fn read_agent_publication(&self) -> Result<Option<AgentPublication>, CoreError> {
        self.inner
            .store
            .read_agent_publication()
            .map_err(CoreError::storage)
    }

    /// Reserves publication before the host changes files or credentials.
    ///
    /// # Errors
    /// Returns an error during restoration or when a decision already exists.
    pub fn prepare_agent_publication(&self, id: Uuid) -> Result<(), CoreError> {
        let _operation = self.operation_guard()?;
        self.inner
            .store
            .prepare_agent_publication(id)
            .map_err(CoreError::storage)
    }

    /// Commits Channels and the publication decision in one durable transaction.
    ///
    /// # Errors
    /// Returns an error during restoration, for stale IDs, or on database failure.
    pub fn commit_agent_publication(
        &self,
        id: Uuid,
        channels: Option<&str>,
    ) -> Result<(), CoreError> {
        let _operation = self.operation_guard()?;
        self.inner
            .store
            .commit_agent_publication(id, channels)
            .map_err(CoreError::storage)
    }

    /// Clears the decision only after host rollback or cleanup has completed.
    ///
    /// # Errors
    /// Returns an error during restoration, for stale IDs/states, or on database failure.
    pub fn finish_agent_publication(
        &self,
        id: Uuid,
        state: AgentPublicationState,
    ) -> Result<(), CoreError> {
        let _operation = self.operation_guard()?;
        self.inner
            .store
            .finish_agent_publication(id, state)
            .map_err(CoreError::storage)
    }
}

#[cfg(test)]
#[path = "runtime_agent_publication_tests.rs"]
mod tests;
