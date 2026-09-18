use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;

use qwenpaw_storage::StoreBackup;
use qwenpaw_storage::ThreadStore;
use tokio::sync::OwnedRwLockReadGuard;
use tokio::sync::OwnedRwLockWriteGuard;

use super::Core;
use super::CoreError;
use super::State;
use super::ThreadRecord;

/// Keeps a complete operation, including its detached writes, outside restore.
#[derive(Clone)]
#[must_use = "retain this guard until the operation and its writes finish"]
pub struct CoreOperationGuard {
    _lease: Arc<OwnedRwLockReadGuard<()>>,
}

/// Exclusive access retained through application-level restore or rollback.
#[must_use = "retain this guard until restore or rollback has finished"]
pub struct CoreRestoreGuard {
    core: Core,
    _lease: OwnedRwLockWriteGuard<()>,
}

impl Core {
    /// Captures logical OAuth credentials for an explicitly requested backup.
    ///
    /// This blocks on the secure store; use a backup-owned blocking worker.
    ///
    /// # Errors
    ///
    /// Returns an error for unavailable or invalid stored OAuth credentials.
    pub fn backup_oauth_credentials(&self) -> Result<qwenpaw_mcp::McpOAuthBackup, CoreError> {
        self.mcp_manager()
            .backup_oauth_credentials()
            .map_err(CoreError::mcp)
    }

    /// Protects a multi-step operation against concurrent backup restoration.
    ///
    /// Guards may be nested. Callers must also retain one in detached tasks
    /// that can write after their parent request or Turn returns.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::RestoreBusy`] during exclusive restoration.
    pub fn operation_guard(&self) -> Result<CoreOperationGuard, CoreError> {
        self.inner
            .operations
            .clone()
            .try_read_owned()
            .map(|lease| CoreOperationGuard {
                _lease: Arc::new(lease),
            })
            .map_err(|_| CoreError::RestoreBusy)
    }

    /// Validates a snapshot in a detached, in-memory Core before stopping work.
    ///
    /// The caller must hydrate external configuration and credentials on the
    /// candidate before applying it. This does not restore workspace files.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid snapshot or stored runtime configuration.
    pub fn prepare_restore(&self, snapshot: &StoreBackup) -> Result<Self, CoreError> {
        let _operation = self.operation_guard()?;
        self.restore_candidate(snapshot)
    }

    fn restore_candidate(&self, snapshot: &StoreBackup) -> Result<Self, CoreError> {
        let store = ThreadStore::in_memory().map_err(CoreError::storage)?;
        store
            .replace_from_backup(snapshot)
            .map_err(CoreError::storage)?;
        let runtime = self.inner.model.runtime_snapshot();
        let candidate = Self::from_store(runtime.config.clone(), self.mcp_manager(), store)?;
        // Partial restores retain provider options, but never carry headers
        // across a restored endpoint. Application hydration supplies new ones.
        if candidate.inner.model.config_snapshot().base_url == runtime.config.base_url {
            candidate.inner.model.write_runtime().options = runtime.options;
        }
        Ok(candidate)
    }

    /// Cancels active Turns, tools, and OAuth callbacks, then drains writes.
    ///
    /// Never call this while retaining an operation guard on the same Core.
    /// A failed or cancelled wait does not apply a backup; cancellation of old
    /// tasks is not undone. Continuous traffic may cause a bounded timeout.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::RestoreTimeout`] if operations cannot drain in time.
    pub async fn begin_restore(&self, timeout: Duration) -> Result<CoreRestoreGuard, CoreError> {
        tokio::time::timeout(timeout, async {
            loop {
                if let Ok(lease) = self.inner.operations.clone().try_write_owned() {
                    let guard = CoreRestoreGuard {
                        core: self.clone(),
                        _lease: lease,
                    };
                    self.mcp_manager().cancel_pending_oauth().await;
                    return guard;
                }
                {
                    let state = self.inner.state.lock().await;
                    for record in state.threads.values() {
                        if let Some(turn) = &record.active_turn {
                            turn.cancellation.cancel();
                        }
                    }
                }
                self.inner.tool_calls.cancel_all().await;
                // Do not queue a writer: existing multi-step operations may
                // still need nested read leases to finish or roll back.
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .map_err(|_| CoreError::RestoreTimeout)
    }
}

impl CoreRestoreGuard {
    /// Revalidates a new candidate after in-flight operations have drained.
    ///
    /// Scope merging must use a snapshot captured under this same exclusive
    /// lease. Unlike [`Core::prepare_restore`], this does not acquire a read
    /// lease that would conflict with the restore already in progress.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid logical data or stored configuration.
    pub fn prepare_restore(&self, snapshot: &StoreBackup) -> Result<Core, CoreError> {
        self.core.restore_candidate(snapshot)
    }

    /// Prepares OAuth credential replacement against a hydrated candidate.
    ///
    /// Retain this Core lease while applying or rolling back the returned
    /// operation. The candidate's MCP configuration determines target accounts.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid credentials or unavailable secure storage.
    pub fn prepare_oauth_restore(
        &self,
        candidate: &Core,
        backup: &qwenpaw_mcp::McpOAuthBackup,
    ) -> Result<qwenpaw_mcp::McpOAuthRestore, CoreError> {
        candidate
            .mcp_manager()
            .prepare_oauth_restore(backup)
            .map_err(CoreError::mcp)
    }

    /// Captures the quiescent state for rollback, including volatile settings.
    ///
    /// Capture after acquiring this lease: data may have changed while old
    /// operations were draining. External files and credentials still require
    /// their own application-level rollback snapshots.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid, oversized, or unavailable runtime state.
    pub fn capture_rollback(&self, max_bytes: u64) -> Result<Core, CoreError> {
        let snapshot = self.core.backup_snapshot(max_bytes)?;
        let store = ThreadStore::in_memory().map_err(CoreError::storage)?;
        store
            .replace_from_backup(&snapshot)
            .map_err(CoreError::storage)?;
        let candidate = Core::from_store(
            self.core.inner.model.config_snapshot(),
            self.core.mcp_manager(),
            store,
        )?;
        candidate
            .inner
            .model
            .replace_runtime(self.core.inner.model.runtime_snapshot());
        candidate.replace_runtime_environment(copy_lock(&self.core.inner.runtime_environment)?)?;
        candidate
            .replace_agent_runtime_config(copy_lock(&self.core.inner.agent_runtime_config)?)?;
        Ok(candidate)
    }

    /// Atomically replaces durable data and the quiescent runtime's state.
    ///
    /// Retains the exclusive lease after success so the application can finish
    /// restoring files and credentials, or apply its original candidate back.
    /// No await or fallible operation follows the database commit.
    ///
    /// # Errors
    ///
    /// Returns an error if the candidate is active, exceeds `max_bytes`, has
    /// poisoned locks, or cannot be persisted. Live data remains unchanged.
    pub async fn apply(&mut self, candidate: &Core, max_bytes: u64) -> Result<(), CoreError> {
        if Arc::ptr_eq(&self.core.inner, &candidate.inner) {
            return Err(CoreError::Config(String::from(
                "restore candidate must be a detached Core",
            )));
        }
        let _candidate_lease = candidate
            .inner
            .operations
            .clone()
            .try_write_owned()
            .map_err(|_| CoreError::RestoreBusy)?;
        candidate.mcp_manager().cancel_pending_oauth().await;
        let source_state = candidate.inner.state.lock().await;
        if !source_state.approvals.is_empty()
            || source_state
                .threads
                .values()
                .any(|record| record.active_turn.is_some())
        {
            return Err(CoreError::RestoreBusy);
        }
        let snapshot = candidate.backup_snapshot(max_bytes)?;
        let restored = State {
            threads: snapshot
                .threads
                .iter()
                .cloned()
                .map(|record| (record.thread.id.clone(), ThreadRecord::from_stored(record)))
                .collect(),
            approvals: HashMap::new(),
            usage_records: snapshot.usage.clone(),
        };
        let model = candidate.inner.model.runtime_snapshot();
        let mcp = copy_lock(&candidate.inner.mcp)?;
        let security = copy_lock(&candidate.inner.security)?;
        let environment = copy_lock(&candidate.inner.runtime_environment)?;
        let agent = copy_lock(&candidate.inner.agent_runtime_config)?;
        let builtins = copy_lock(&candidate.inner.builtin_tool_overrides)?;
        let prompts = copy_lock(&candidate.inner.system_prompt_files)?;
        let offload = candidate.inner.tool_calls.offload_on_deadline();
        let live = &self.core.inner;
        let mut state = live.state.lock().await;
        let mut tools = live.tool_calls.lock_for_restore().await;
        let mut live_mcp = live.mcp.write().map_err(lock_error)?;
        let mut live_security = live.security.write().map_err(lock_error)?;
        let mut live_environment = live.runtime_environment.write().map_err(lock_error)?;
        let mut live_agent = live.agent_runtime_config.write().map_err(lock_error)?;
        let mut live_builtins = live.builtin_tool_overrides.write().map_err(lock_error)?;
        let mut live_prompts = live.system_prompt_files.write().map_err(lock_error)?;

        live.store
            .replace_from_backup(&snapshot)
            .map_err(CoreError::storage)?;
        *state = restored;
        *live_mcp = mcp;
        *live_security = security;
        *live_environment = environment;
        *live_agent = agent;
        *live_builtins = builtins;
        *live_prompts = prompts;
        live.model.replace_runtime(model);
        live.tool_calls.set_offload_on_deadline(offload);
        tools.clear();
        Ok(())
    }
}

fn copy_lock<T: Clone>(lock: &RwLock<T>) -> Result<T, CoreError> {
    lock.read().map(|value| value.clone()).map_err(lock_error)
}

fn lock_error<T>(_: std::sync::PoisonError<T>) -> CoreError {
    CoreError::Config(String::from("restore runtime lock is poisoned"))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::AgentRuntimeConfig;
    use crate::ModelConfig;

    #[tokio::test]
    async fn rollback_captures_late_writes_and_volatile_credentials_after_draining() {
        let core = Core::new(ModelConfig {
            api_key: None,
            base_url: String::from("http://127.0.0.1:1/v1"),
            default_model: String::from("test-model"),
        });
        let candidate = core
            .prepare_restore(&core.backup_snapshot(1024).unwrap())
            .unwrap();
        core.write_ui_language("ru").unwrap();
        core.set_runtime_api_key(Some(String::from("local-key")))
            .unwrap();
        let environment =
            BTreeMap::from([(String::from("LATE_VALUE"), String::from("local-value"))]);
        core.replace_runtime_environment(environment.clone())
            .unwrap();
        let agent = AgentRuntimeConfig {
            max_agent_steps: 5,
            ..AgentRuntimeConfig::default()
        };
        core.replace_agent_runtime_config(agent.clone()).unwrap();
        let original = core.backup_snapshot(1024).unwrap();
        let mut guard = core.begin_restore(Duration::from_secs(1)).await.unwrap();
        let rollback = guard.capture_rollback(1024).unwrap();
        guard.apply(&candidate, 1024).await.unwrap();
        assert_eq!(
            copy_lock(&core.inner.runtime_environment).unwrap(),
            BTreeMap::new()
        );
        assert_eq!(core.inner.model.config_snapshot().api_key, None);
        guard.apply(&rollback, 1024).await.unwrap();
        assert_eq!(core.backup_snapshot(1024).unwrap(), original);
        assert_eq!(
            copy_lock(&core.inner.runtime_environment).unwrap(),
            environment
        );
        assert_eq!(
            core.inner.model.config_snapshot().api_key,
            Some(String::from("local-key"))
        );
        assert_eq!(core.agent_runtime_config().unwrap(), agent);
    }

    #[tokio::test]
    async fn cancelling_application_before_commit_preserves_data_and_exclusive_lease() {
        let core = Core::new(ModelConfig {
            api_key: None,
            base_url: String::from("http://127.0.0.1:1/v1"),
            default_model: String::from("test-model"),
        });
        let original = core.backup_snapshot(1024).unwrap();
        let candidate = core.prepare_restore(&original).unwrap();
        candidate.write_ui_language("zh").unwrap();
        let candidate_state = candidate.inner.state.lock().await;
        let mut guard = core.begin_restore(Duration::from_secs(1)).await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(20), guard.apply(&candidate, 1024))
                .await
                .is_err()
        );
        assert_eq!(core.backup_snapshot(1024).unwrap(), original);
        assert_eq!(core.write_ui_language("ru"), Err(CoreError::RestoreBusy));
        drop(candidate_state);
        guard.apply(&candidate, 1024).await.unwrap();
        assert_eq!(core.read_ui_language().unwrap(), "zh");
    }
}
