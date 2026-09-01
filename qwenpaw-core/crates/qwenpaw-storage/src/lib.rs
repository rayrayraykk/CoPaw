use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;

use qwenpaw_protocol::Thread;
use qwenpaw_protocol::Turn;
use rusqlite::Connection;
use rusqlite::params;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredMessage {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<StoredToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl StoredMessage {
    #[must_use]
    pub fn text(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    #[must_use]
    pub fn assistant_tool_calls(content: String, tool_calls: Vec<StoredToolCall>) -> Self {
        Self {
            role: String::from("assistant"),
            content,
            tool_calls,
            tool_call_id: None,
        }
    }

    #[must_use]
    pub fn tool_result(call_id: String, content: String) -> Self {
        Self {
            role: String::from("tool"),
            content,
            tool_calls: Vec::new(),
            tool_call_id: Some(call_id),
        }
    }
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
}

#[derive(Clone)]
pub struct ThreadStore {
    connection: Arc<Mutex<Connection>>,
}

impl ThreadStore {
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
             );",
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
