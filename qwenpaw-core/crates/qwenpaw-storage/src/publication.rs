//! Installation-local decisions for multi-resource Agent publication.

use rusqlite::{OptionalExtension as _, TransactionBehavior, params};
use uuid::Uuid;

use super::{StorageError, ThreadStore};

/// Canonical Channels setting committed with an Agent publication decision.
pub const CHANNEL_CONFIG_DATA_KEY: &str = "desktop_channel_config_data";

/// Whether restart must undo publication or retain the committed values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentPublicationState {
    Prepared,
    Committed,
}

impl AgentPublicationState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Committed => "committed",
        }
    }
}

/// A local decision only; file and credential recovery remain host-owned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPublication {
    pub id: Uuid,
    pub state: AgentPublicationState,
}

/// Durable host recovery phase, separate from the commit decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentPublicationPhase {
    Staging,
    Publishing,
    Cleaning,
}

/// Non-secret file receipt and restart phase, never logical backup data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPublicationRecovery {
    pub journal: Vec<u8>,
    pub phase: AgentPublicationPhase,
    pub has_secret: bool,
}

impl ThreadStore {
    /// Returns the installation-local identity, assigning it once if absent.
    /// It is deliberately excluded from logical backup and replacement.
    ///
    /// # Errors
    /// Returns an error for unavailable storage or a malformed installation row.
    pub fn installation_id(&self) -> Result<Uuid, StorageError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO core_installation(slot, id) SELECT 1, ?1
             WHERE NOT EXISTS (SELECT 1 FROM core_installation)",
            [Uuid::now_v7().to_string()],
        )?;
        let (slot, raw): (i64, String) =
            transaction.query_row("SELECT slot, id FROM core_installation", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?;
        let count: i64 =
            transaction.query_row("SELECT count(*) FROM core_installation", [], |row| {
                row.get(0)
            })?;
        let id = Uuid::parse_str(&raw).map_err(|_| StorageError::InvalidAgentPublication)?;
        if slot != 1 || count != 1 || id.is_nil() || id.to_string() != raw {
            return Err(StorageError::InvalidAgentPublication);
        }
        transaction.commit()?;
        Ok(id)
    }

    /// Reserves a publication and its trusted file-receipt digest atomically.
    ///
    /// # Errors
    /// Returns an error if any local publication exists or SQLite cannot persist both rows.
    pub fn prepare_agent_publication_with_journal(
        &self,
        id: Uuid,
        digest: [u8; 32],
    ) -> Result<(), StorageError> {
        self.prepare_publication(id, Some(digest), None)
    }

    /// Reserves file recovery metadata, digest and decision in one transaction.
    ///
    /// # Errors
    /// Returns an error for invalid size, pending recovery, or a failed write.
    pub fn prepare_agent_publication_recovery(
        &self,
        id: Uuid,
        digest: [u8; 32],
        journal: &[u8],
        has_secret: bool,
    ) -> Result<(), StorageError> {
        if journal.is_empty() || journal.len() > 131_072 {
            return Err(StorageError::InvalidAgentPublication);
        }
        self.prepare_publication(id, Some(digest), Some((journal, has_secret)))
    }

    /// Reads the exact pending publication's non-secret host recovery record.
    ///
    /// # Errors
    /// Returns an error for malformed recovery metadata or unavailable storage.
    pub fn read_agent_publication_recovery(
        &self,
        id: Uuid,
    ) -> Result<Option<AgentPublicationRecovery>, StorageError> {
        let raw: Option<(Vec<u8>, String, i64)> = self
            .lock()?
            .query_row(
                "SELECT r.journal, r.phase, r.has_secret FROM agent_publication_recovery r
             JOIN agent_publication p ON p.slot = r.slot WHERE p.id = ?1 AND p.slot = 1",
                [id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        raw.map(|(journal, phase, has_secret)| {
            let phase = match phase.as_str() {
                "staging" => AgentPublicationPhase::Staging,
                "publishing" => AgentPublicationPhase::Publishing,
                "cleaning" => AgentPublicationPhase::Cleaning,
                _ => return Err(StorageError::InvalidAgentPublication),
            };
            if journal.is_empty() || journal.len() > 131_072 || ![0, 1].contains(&has_secret) {
                return Err(StorageError::InvalidAgentPublication);
            }
            Ok(AgentPublicationRecovery {
                journal,
                phase,
                has_secret: has_secret == 1,
            })
        })
        .transpose()
    }

    /// Allows live publication only after private recovery snapshots are ready.
    ///
    /// # Errors
    /// Returns an error for stale/non-staging recovery or database failure.
    pub fn start_agent_publication(&self, id: Uuid) -> Result<(), StorageError> {
        require_one(self.lock()?.execute(
            "UPDATE agent_publication_recovery SET phase = 'publishing' WHERE slot = 1 AND phase = 'staging'
             AND EXISTS (SELECT 1 FROM agent_publication WHERE slot = 1 AND id = ?1 AND state = 'prepared')", [id.to_string()],
        )?)
    }

    /// Records completed inverse operations before deleting recovery snapshots.
    ///
    /// # Errors
    /// Returns an error for a stale decision or database failure.
    pub fn clean_agent_publication(
        &self,
        id: Uuid,
        state: AgentPublicationState,
    ) -> Result<(), StorageError> {
        require_one(self.lock()?.execute(
            "UPDATE agent_publication_recovery SET phase = 'cleaning' WHERE slot = 1
             AND EXISTS (SELECT 1 FROM agent_publication WHERE slot = 1 AND id = ?1 AND state = ?2)",
            params![id.to_string(), state.as_str()],
        )?)
    }

    /// Reads the digest belonging to exactly the currently pending publication.
    ///
    /// # Errors
    /// Returns an error for a stale ID, malformed digest, or unavailable storage.
    pub fn agent_publication_journal_digest(
        &self,
        id: Uuid,
    ) -> Result<Option<[u8; 32]>, StorageError> {
        let connection = self.lock()?;
        let raw: Option<Option<Vec<u8>>> = connection.query_row(
            "SELECT j.digest FROM agent_publication p LEFT JOIN agent_publication_journal j ON p.slot = j.slot
             WHERE p.slot = 1 AND p.id = ?1", [id.to_string()], |row| row.get(0),
        ).optional()?;
        raw.ok_or(StorageError::AgentPublicationConflict)?
            .map(|bytes| {
                bytes
                    .try_into()
                    .map_err(|_| StorageError::InvalidAgentPublication)
            })
            .transpose()
    }

    /// Reads the pending local decision, never a logical backup value.
    ///
    /// # Errors
    /// Returns an error for unreadable or malformed local records.
    pub fn read_agent_publication(&self) -> Result<Option<AgentPublication>, StorageError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare("SELECT slot, id, state FROM agent_publication")?;
        let mut rows = statement.query([])?;
        let Some(row) = rows.next()? else {
            ensure_no_publication(&connection)?;
            return Ok(None);
        };
        let slot: i64 = row.get(0)?;
        let raw_id: String = row.get(1)?;
        let state: String = row.get(2)?;
        let id = Uuid::parse_str(&raw_id).map_err(|_| StorageError::InvalidAgentPublication)?;
        let state = match state.as_str() {
            "prepared" => AgentPublicationState::Prepared,
            "committed" => AgentPublicationState::Committed,
            _ => return Err(StorageError::InvalidAgentPublication),
        };
        if slot != 1 || id.to_string() != raw_id || rows.next()?.is_some() {
            return Err(StorageError::InvalidAgentPublication);
        }
        Ok(Some(AgentPublication { id, state }))
    }

    /// Reserves the sole publication slot before files or live credentials change.
    ///
    /// # Errors
    /// Returns an error if a decision already exists or SQLite cannot persist it.
    pub fn prepare_agent_publication(&self, id: Uuid) -> Result<(), StorageError> {
        self.prepare_publication(id, None, None)
    }

    fn prepare_publication(
        &self,
        id: Uuid,
        digest: Option<[u8; 32]>,
        recovery: Option<(&[u8], bool)>,
    ) -> Result<(), StorageError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_no_publication(&transaction)?;
        transaction.execute(
            "INSERT INTO agent_publication(slot, id, state) VALUES (1, ?1, 'prepared')",
            [id.to_string()],
        )?;
        if let Some(digest) = digest {
            transaction.execute(
                "INSERT INTO agent_publication_journal(slot, digest) VALUES (1, ?1)",
                [digest.as_slice()],
            )?;
        }
        if let Some((journal, has_secret)) = recovery {
            transaction.execute(
                "INSERT INTO agent_publication_recovery(slot, journal, phase, has_secret)
                VALUES (1, ?1, 'staging', ?2)",
                params![journal, has_secret],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Commits Channels and the decision together; `None` preserves Channels.
    ///
    /// # Errors
    /// Returns an error for a stale/non-prepared ID or any failed SQL write.
    pub fn commit_agent_publication(
        &self,
        id: Uuid,
        channels: Option<&str>,
    ) -> Result<(), StorageError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_one(transaction.execute(
            "UPDATE agent_publication SET state = 'committed'
             WHERE slot = 1 AND id = ?1 AND state = 'prepared'
             AND NOT EXISTS (SELECT 1 FROM agent_publication_recovery WHERE slot = 1 AND phase != 'publishing')",
            [id.to_string()],
        )?)?;
        if let Some(channels) = channels {
            transaction.execute(
                "INSERT INTO core_settings(key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![CHANNEL_CONFIG_DATA_KEY, channels],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Clears exactly one decision after its rollback or cleanup has completed.
    ///
    /// # Errors
    /// Returns an error for a stale ID/state or a failed database write.
    pub fn finish_agent_publication(
        &self,
        id: Uuid,
        state: AgentPublicationState,
    ) -> Result<(), StorageError> {
        require_one(self.lock()?.execute(
            "DELETE FROM agent_publication WHERE slot = 1 AND id = ?1 AND state = ?2
             AND NOT EXISTS (SELECT 1 FROM agent_publication_recovery WHERE slot = 1 AND phase != 'cleaning')",
            params![id.to_string(), state.as_str()],
        )?)
    }
}

pub(super) fn ensure_no_publication(connection: &rusqlite::Connection) -> Result<(), StorageError> {
    if connection
        .query_row(
            "SELECT 1 FROM agent_publication UNION ALL SELECT 1 FROM agent_publication_journal
            UNION ALL SELECT 1 FROM agent_publication_recovery LIMIT 1",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some()
    {
        return Err(StorageError::AgentPublicationConflict);
    }
    Ok(())
}

fn require_one(changed: usize) -> Result<(), StorageError> {
    if changed != 1 {
        return Err(StorageError::AgentPublicationConflict);
    }
    Ok(())
}
