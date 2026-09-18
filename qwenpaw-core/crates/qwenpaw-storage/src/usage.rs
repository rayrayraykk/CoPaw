//! Versioned usage ownership shared by storage and trusted Core hosts.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{StorageError, StoreBackup, StoredUsageRecord};

#[cfg(test)]
#[path = "usage_tests.rs"]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "id",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum WorkspaceDataKey {
    LegacyAgent(String),
    Workspace(Uuid),
}

impl WorkspaceDataKey {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        match self {
            Self::LegacyAgent(id) => is_valid_agent_id(id),
            Self::Workspace(id) => !id.is_nil(),
        }
    }
}

#[must_use]
pub fn is_valid_agent_id(id: &str) -> bool {
    (2..=64).contains(&id.len())
        && id.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric)
        && id.as_bytes().last().is_some_and(u8::is_ascii_alphanumeric)
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

/// A trusted host's event-time label and permanent data namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageOwner {
    pub agent_id: String,
    pub data_key: WorkspaceDataKey,
}

impl UsageOwner {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        is_valid_agent_id(&self.agent_id) && self.data_key.is_valid()
    }
}

impl StoreBackup {
    /// Validates usage ownership before a restore can change any durable state.
    ///
    /// # Errors
    /// Returns an error for unsupported versions or invalid/legacy-mixed keys.
    pub fn validate_usage(&self) -> Result<(), StorageError> {
        if !matches!(self.version, 1 | 2) {
            return Err(StorageError::UnsupportedBackupVersion);
        }
        for record in &self.usage {
            validate_record(record)?;
            if self.version == 1 && record.data_key.is_some() {
                return Err(StorageError::InvalidUsageOwnership);
            }
        }
        Ok(())
    }
}

fn validate_record(record: &StoredUsageRecord) -> Result<(), StorageError> {
    if !is_valid_agent_id(&record.agent_id)
        || record.data_key.as_ref().is_some_and(|key| !key.is_valid())
    {
        return Err(StorageError::InvalidUsageOwnership);
    }
    Ok(())
}

pub(super) fn encode(record: &StoredUsageRecord) -> Result<String, StorageError> {
    validate_record(record)?;
    // Nest the new row so old readers fail instead of silently dropping keys.
    Ok(serde_json::to_string(
        &serde_json::json!({"version":2,"usage":record}),
    )?)
}

pub(super) fn decode(serialized: &str) -> Result<StoredUsageRecord, StorageError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct VersionedRow {
        version: u32,
        usage: StoredUsageRecord,
    }
    let value: serde_json::Value = serde_json::from_str(serialized)?;
    let record = if value.get("version").is_some() {
        let row: VersionedRow = serde_json::from_value(value)?;
        if row.version != 2 {
            return Err(StorageError::InvalidUsageOwnership);
        }
        row.usage
    } else {
        let record: StoredUsageRecord = serde_json::from_value(value)?;
        if record.data_key.is_some() {
            return Err(StorageError::InvalidUsageOwnership);
        }
        record
    };
    validate_record(&record)?;
    Ok(record)
}
