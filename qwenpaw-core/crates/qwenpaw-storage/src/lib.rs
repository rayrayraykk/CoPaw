use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;

use qwenpaw_protocol::Thread;
use qwenpaw_protocol::Turn;
use rusqlite::Connection;
use rusqlite::params;
use serde::Deserialize;
use serde::Serialize;

mod usage;
pub use usage::{UsageOwner, WorkspaceDataKey, is_valid_agent_id};
mod publication;
pub use publication::{AgentPublication, AgentPublicationState, CHANNEL_CONFIG_DATA_KEY};
pub use publication::{AgentPublicationPhase, AgentPublicationRecovery};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredMessage {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_input: Option<StoredUserInput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<StoredToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_content: BTreeMap<String, Vec<serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_error: Option<bool>,
}

impl StoredMessage {
    #[must_use]
    pub fn text(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            user_input: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
            provider_content: BTreeMap::new(),
            tool_error: None,
        }
    }

    #[must_use]
    pub fn assistant_tool_calls(content: String, tool_calls: Vec<StoredToolCall>) -> Self {
        Self {
            role: String::from("assistant"),
            content,
            tool_calls,
            user_input: None,
            tool_call_id: None,
            provider_content: BTreeMap::new(),
            tool_error: None,
        }
    }

    #[must_use]
    pub fn tool_result(call_id: String, content: String) -> Self {
        Self {
            role: String::from("tool"),
            content,
            tool_calls: Vec::new(),
            user_input: None,
            tool_call_id: Some(call_id),
            provider_content: BTreeMap::new(),
            tool_error: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredUserInput {
    pub item_id: String,
    pub parts: Vec<StoredUserPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum StoredUserPart {
    Text {
        text: String,
    },
    Image {
        path: String,
        mime_type: String,
        size: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: StoredFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredFunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredThread {
    pub thread: Thread,
    pub turns: Vec<Turn>,
    pub messages: Vec<StoredMessage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub turn_metadata: Vec<StoredTurnMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredTurnMetadata {
    pub turn_id: String,
    pub started_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub model_calls: Vec<StoredModelCall>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredModelCall {
    pub provider_id: String,
    pub model: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub cache_eligible_input_tokens: u64,
    pub cache_observed: bool,
    pub usage_observed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredUsageRecord {
    pub id: String,
    pub thread_id: String,
    pub turn_id: String,
    pub agent_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_key: Option<WorkspaceDataKey>,
    pub recorded_at: i64,
    pub call: StoredModelCall,
}

#[derive(Clone)]
pub struct ThreadStore {
    connection: Arc<Mutex<Connection>>,
}

/// Logical backup of business data; local recovery decisions are not portable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoreBackup {
    pub version: u32,
    pub settings: BTreeMap<String, String>,
    pub threads: Vec<StoredThread>,
    pub usage: Vec<StoredUsageRecord>,
}

impl ThreadStore {
    /// Replaces durable tables as one transaction with a validated backup.
    ///
    /// The caller owns scope merging and runtime quiescence. A failed insert
    /// rolls back all tables, including settings and the usage ledger.
    ///
    /// # Errors
    ///
    /// Returns an error for unsupported versions, duplicate IDs, serialization
    /// failures, pending local publication recovery, or a transaction that cannot commit.
    pub fn replace_from_backup(&self, backup: &StoreBackup) -> Result<(), StorageError> {
        backup.validate_usage()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        publication::ensure_no_publication(&transaction)?;
        transaction.execute_batch(
            "DELETE FROM core_settings; DELETE FROM threads; DELETE FROM model_usage;",
        )?;
        for (key, value) in &backup.settings {
            transaction.execute(
                "INSERT INTO core_settings (key, value) VALUES (?1, ?2)",
                params![key, value],
            )?;
        }
        for snapshot in &backup.threads {
            transaction.execute(
                "INSERT INTO threads (id, updated_at, snapshot) VALUES (?1, ?2, ?3)",
                params![
                    snapshot.thread.id,
                    snapshot.thread.updated_at,
                    serde_json::to_string(snapshot)?
                ],
            )?;
        }
        for record in &backup.usage {
            transaction.execute(
                "INSERT INTO model_usage (id, recorded_at, record) VALUES (?1, ?2, ?3)",
                params![record.id, record.recorded_at, usage::encode(record)?],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Captures settings, conversations, and usage at one SQLite snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid stored JSON, query failures, or a snapshot
    /// exceeding `max_bytes` of serialized rows.
    pub fn backup_snapshot(&self, max_bytes: u64) -> Result<StoreBackup, StorageError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        let size: u64 = transaction.query_row(
            "SELECT
                (SELECT COALESCE(SUM(LENGTH(CAST(key AS BLOB)) + LENGTH(CAST(value AS BLOB))), 0) FROM core_settings) +
                (SELECT COALESCE(SUM(LENGTH(CAST(snapshot AS BLOB))), 0) FROM threads) +
                (SELECT COALESCE(SUM(LENGTH(CAST(record AS BLOB))), 0) FROM model_usage)",
            [], |row| row.get(0),
        )?;
        if size > max_bytes {
            return Err(StorageError::BackupTooLarge);
        }
        let mut settings = BTreeMap::new();
        let mut statement =
            transaction.prepare("SELECT key, value FROM core_settings ORDER BY key")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (key, value) = row?;
            settings.insert(key, value);
        }
        let mut threads = Vec::new();
        let mut statement = transaction.prepare("SELECT snapshot FROM threads ORDER BY id")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        for row in rows {
            threads.push(serde_json::from_str(&row?)?);
        }
        let mut usage = Vec::new();
        let mut statement = transaction.prepare("SELECT record FROM model_usage ORDER BY id")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        for row in rows {
            usage.push(usage::decode(&row?)?);
        }
        Ok(StoreBackup {
            version: 2,
            settings,
            threads,
            usage,
        })
    }

    /// Opens or creates a thread database at `path`.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot be opened or migrated.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Self::from_connection(Connection::open(path)?)
    }

    /// Creates a non-durable store for tests and ephemeral runtimes.
    ///
    /// # Errors
    ///
    /// Returns an error when the in-memory database cannot be initialized.
    pub fn in_memory() -> Result<Self, StorageError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    /// Loads every stored thread snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error when the query fails or a stored snapshot is invalid.
    pub fn load_all(&self) -> Result<Vec<StoredThread>, StorageError> {
        let connection = self.lock()?;
        let mut statement =
            connection.prepare("SELECT snapshot FROM threads ORDER BY updated_at DESC, id DESC")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut threads = Vec::new();
        for row in rows {
            threads.push(serde_json::from_str(&row?)?);
        }
        Ok(threads)
    }

    /// Inserts or replaces one complete thread snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error when serialization or the database write fails.
    pub fn upsert(&self, snapshot: &StoredThread) -> Result<(), StorageError> {
        let serialized = serde_json::to_string(snapshot)?;
        let connection = self.lock()?;
        connection.execute(
            "INSERT INTO threads (id, updated_at, snapshot)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET
                updated_at = excluded.updated_at,
                snapshot = excluded.snapshot",
            params![snapshot.thread.id, snapshot.thread.updated_at, serialized],
        )?;
        Ok(())
    }

    /// Persists a Thread snapshot and one immutable model usage record.
    ///
    /// # Errors
    ///
    /// Returns an error when serialization or the atomic database write fails.
    pub fn upsert_with_usage(
        &self,
        snapshot: &StoredThread,
        usage: &StoredUsageRecord,
    ) -> Result<(), StorageError> {
        let serialized_snapshot = serde_json::to_string(snapshot)?;
        let serialized_usage = usage::encode(usage)?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO threads (id, updated_at, snapshot)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET
                updated_at = excluded.updated_at,
                snapshot = excluded.snapshot",
            params![
                snapshot.thread.id,
                snapshot.thread.updated_at,
                serialized_snapshot
            ],
        )?;
        transaction.execute(
            "INSERT INTO model_usage (id, recorded_at, record)
             VALUES (?1, ?2, ?3)",
            params![usage.id, usage.recorded_at, serialized_usage],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Loads the immutable model usage ledger in chronological order.
    ///
    /// # Errors
    ///
    /// Returns an error when the query fails or a stored record is invalid.
    pub fn load_usage(&self) -> Result<Vec<StoredUsageRecord>, StorageError> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare("SELECT record FROM model_usage ORDER BY recorded_at ASC, id ASC")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut records = Vec::new();
        for row in rows {
            records.push(usage::decode(&row?)?);
        }
        Ok(records)
    }

    /// Deletes one complete thread snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error when the database write fails.
    pub fn delete(&self, thread_id: &str) -> Result<bool, StorageError> {
        let connection = self.lock()?;
        let deleted = connection.execute("DELETE FROM threads WHERE id = ?1", [thread_id])?;
        Ok(deleted > 0)
    }

    /// Reads a non-secret Core setting by key.
    ///
    /// # Errors
    ///
    /// Returns an error when the database query fails.
    pub fn read_setting(&self, key: &str) -> Result<Option<String>, StorageError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare("SELECT value FROM core_settings WHERE key = ?1")?;
        let mut rows = statement.query([key])?;
        rows.next()?
            .map(|row| row.get::<_, String>(0))
            .transpose()
            .map_err(StorageError::from)
    }

    /// Writes a set of non-secret Core settings atomically.
    ///
    /// # Errors
    ///
    /// Returns an error when the transaction cannot be committed.
    pub fn write_settings(&self, settings: &[(&str, &str)]) -> Result<(), StorageError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        for (key, value) in settings {
            transaction.execute(
                "INSERT INTO core_settings (key, value)
                 VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    fn from_connection(connection: Connection) -> Result<Self, StorageError> {
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS threads (
                id TEXT PRIMARY KEY NOT NULL,
                updated_at INTEGER NOT NULL,
                snapshot TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_threads_updated_at
                ON threads(updated_at DESC);
             CREATE TABLE IF NOT EXISTS core_settings (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS agent_publication (
                slot INTEGER PRIMARY KEY CHECK(slot = 1),
                id TEXT NOT NULL CHECK(length(id) = 36),
                state TEXT NOT NULL CHECK(state IN ('prepared', 'committed'))
             );
             CREATE TABLE IF NOT EXISTS core_installation (
                slot INTEGER PRIMARY KEY CHECK(slot = 1),
                id TEXT NOT NULL CHECK(length(id) = 36)
             );
             CREATE TABLE IF NOT EXISTS agent_publication_journal (
                slot INTEGER PRIMARY KEY REFERENCES agent_publication(slot) ON DELETE CASCADE,
                digest BLOB NOT NULL CHECK(length(digest) = 32)
             );
             CREATE TABLE IF NOT EXISTS agent_publication_recovery (
                slot INTEGER PRIMARY KEY REFERENCES agent_publication(slot) ON DELETE CASCADE,
                journal BLOB NOT NULL CHECK(length(journal) BETWEEN 1 AND 131072),
                phase TEXT NOT NULL CHECK(phase IN ('staging', 'publishing', 'cleaning')),
                has_secret INTEGER NOT NULL CHECK(has_secret IN (0, 1))
             );
             CREATE TABLE IF NOT EXISTS model_usage (
                id TEXT PRIMARY KEY NOT NULL,
                recorded_at INTEGER NOT NULL,
                record TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_model_usage_recorded_at
                ON model_usage(recorded_at ASC, id ASC);
             ",
        )?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, StorageError> {
        self.connection
            .lock()
            .map_err(|_| StorageError::LockPoisoned)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Agent publication decision does not match the expected state or requires recovery")]
    AgentPublicationConflict,
    #[error("Agent publication decision is invalid")]
    InvalidAgentPublication,
    #[error("invalid or unsupported usage ownership")]
    InvalidUsageOwnership,
    #[error("unsupported backup snapshot version")]
    UnsupportedBackupVersion,
    #[error("backup snapshot exceeds its size limit")]
    BackupTooLarge,
    #[error("thread database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("thread snapshot JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("thread database directory failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("thread database lock is poisoned")]
    LockPoisoned,
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;

#[cfg(test)]
mod publication_tests;
