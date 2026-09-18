//! Passive Turn draining for an application-owned checkpoint transaction.

use std::collections::BTreeSet;
use std::time::Duration;

use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard};

use super::{Core, CoreError, CoreOperationGuard};

/// Prevents new Turns in a validated set until restore or rollback finishes.
///
/// This is an execution fence, not Workspace ownership or file authorization.
/// The application must also fence new Thread creation and non-Turn writers.
#[must_use = "retain this guard until checkpoint restore or rollback finishes"]
pub struct CoreThreadQuiescenceGuard {
    _leases: Vec<OwnedRwLockWriteGuard<()>>,
    _operation: CoreOperationGuard,
}

impl Core {
    pub(super) async fn turn_execution_guard(
        &self,
        thread_id: &str,
    ) -> Result<(OwnedRwLockReadGuard<()>, Option<String>), CoreError> {
        let state = self.inner.state.lock().await;
        let record = state
            .threads
            .get(thread_id)
            .ok_or_else(|| CoreError::ThreadNotFound(thread_id.to_owned()))?;
        if record.thread.archived {
            return Err(CoreError::ThreadArchived(thread_id.to_owned()));
        }
        if record.active_turn.is_some() {
            return Err(CoreError::ThreadBusy(thread_id.to_owned()));
        }
        let lease = record
            .execution
            .clone()
            .try_read_owned()
            .map_err(|_| CoreError::ThreadBusy(thread_id.to_owned()))?;
        Ok((lease, record.thread.workspace_root.clone()))
    }

    /// Waits for selected Turn producers and their final writes without
    /// interrupting them, then prevents new Turns until the guard is dropped.
    ///
    /// Validate and freeze the application's Workspace membership before
    /// calling this method. Do not hold a lock needed by existing producers.
    /// Duplicate IDs are accepted. A failed or cancelled wait releases every
    /// acquired fence and does not change conversation or cancellation state.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown Thread, a concurrent global restore,
    /// or when producers do not finish within the supplied timeout.
    pub async fn quiesce_threads(
        &self,
        thread_ids: &[String],
        timeout: Duration,
    ) -> Result<CoreThreadQuiescenceGuard, CoreError> {
        let operation = self.operation_guard()?;
        tokio::time::timeout(timeout, async {
            // Resolve the complete set before reserving any execution gate.
            // Stable ordering prevents overlapping transactions deadlocking.
            let gates = {
                let state = self.inner.state.lock().await;
                thread_ids
                    .iter()
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .map(|id| {
                        state
                            .threads
                            .get(id)
                            .map(|record| record.execution.clone())
                            .ok_or_else(|| CoreError::ThreadNotFound(id.clone()))
                    })
                    .collect::<Result<Vec<_>, _>>()?
            };
            let mut leases = Vec::with_capacity(gates.len());
            for gate in gates {
                leases.push(gate.write_owned().await);
            }
            Ok(CoreThreadQuiescenceGuard {
                _leases: leases,
                _operation: operation,
            })
        })
        .await
        .map_err(|_| CoreError::RestoreTimeout)?
    }
}
