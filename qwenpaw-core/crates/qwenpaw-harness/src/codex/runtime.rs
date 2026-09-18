//! Workspace-scoped projected clients; thread persistence belongs to the adapter.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tokio::sync::{mpsc, oneshot};

use super::lifecycle::{CodexLifecycle, LaunchConfig, Resolver};
use super::projection::project_runtime;
use super::{CodexClient, Error, RequestHandler};
use crate::capabilities::RuntimeCapabilities;

type Factory = Arc<dyn Fn(LaunchConfig) -> CodexLifecycle + Send + Sync>;
type Reply<T> = oneshot::Sender<Result<T, Error>>;

enum Operation {
    Prepare(
        String,
        RuntimeCapabilities,
        Duration,
        Reply<PreparedRuntime>,
    ),
    Forget(String, Reply<()>),
    Stop(bool, Reply<()>),
}

/// The endpoint is generation-bound; a fingerprint is not a process identity.
#[derive(Clone)]
pub struct PreparedRuntime {
    pub client: CodexClient,
    pub fingerprint: String,
}

/// Serialized session bindings and projected process owners, not an Agent runtime.
/// Accepted operations finish even if the caller drops its reply. Explicit shutdown
/// verifies cleanup; dropping the last handle only requests it.
#[derive(Clone)]
pub struct CodexRuntimePool {
    sender: mpsc::Sender<Operation>,
}

impl CodexRuntimePool {
    pub(super) fn new(
        launch: LaunchConfig,
        handler: Option<RequestHandler>,
        resolver: Resolver,
    ) -> Self {
        Self::with_factory(
            launch,
            Arc::new(move |launch| {
                CodexLifecycle::discovering(launch, handler.clone(), resolver.clone())
            }),
        )
    }

    pub(super) fn with_factory(launch: LaunchConfig, factory: Factory) -> Self {
        let (sender, receiver) = mpsc::channel(16);
        tokio::spawn(
            Owner {
                launch,
                factory,
                clients: BTreeMap::new(),
                sessions: BTreeMap::new(),
                fault: None,
            }
            .run(receiver),
        );
        Self { sender }
    }

    /// Projects resolved capabilities and binds only after roots are accepted.
    /// Revisions must already reflect resolved credentials and skill contents.
    /// Timeout bounds handshake and roots RPC separately, not queue/discovery time.
    ///
    /// # Errors
    /// Returns projection, process, protocol or latched cleanup errors.
    pub async fn prepare(
        &self,
        session_id: String,
        capabilities: RuntimeCapabilities,
        timeout: Duration,
    ) -> Result<PreparedRuntime, Error> {
        let (reply, receive) = oneshot::channel();
        self.sender
            .send(Operation::Prepare(session_id, capabilities, timeout, reply))
            .await
            .map_err(|_| Error::Closed)?;
        receive.await.map_err(|_| Error::Closed)?
    }

    /// Removes only the in-memory binding, not a persisted thread or shared client.
    ///
    /// # Errors
    /// Returns Closed after shutdown or a latched cleanup error.
    pub async fn forget_session(&self, session_id: String) -> Result<(), Error> {
        let (reply, receive) = oneshot::channel();
        self.sender
            .send(Operation::Forget(session_id, reply))
            .await
            .map_err(|_| Error::Closed)?;
        receive.await.map_err(|_| Error::Closed)?
    }

    /// Reaps every pooled process and clears bindings, permitting a fresh pool.
    ///
    /// # Errors
    /// Returns the first cleanup failure after attempting every owner, or Closed.
    pub async fn stop(&self) -> Result<(), Error> {
        self.stop_with(false).await
    }

    /// Permanently closes every clone and waits for all owned processes.
    ///
    /// # Errors
    /// Returns cleanup failure or Closed if already shut down.
    pub async fn shutdown(&self) -> Result<(), Error> {
        self.stop_with(true).await
    }

    async fn stop_with(&self, terminal: bool) -> Result<(), Error> {
        let (reply, receive) = oneshot::channel();
        self.sender
            .send(Operation::Stop(terminal, reply))
            .await
            .map_err(|_| Error::Closed)?;
        receive.await.map_err(|_| Error::Closed)?
    }
}

struct Entry {
    lifecycle: CodexLifecycle,
    initialized: Option<CodexClient>,
}

struct Owner {
    launch: LaunchConfig,
    factory: Factory,
    clients: BTreeMap<String, Entry>,
    sessions: BTreeMap<String, String>,
    fault: Option<Error>,
}

impl Owner {
    async fn run(mut self, mut receiver: mpsc::Receiver<Operation>) {
        while let Some(operation) = receiver.recv().await {
            match operation {
                Operation::Prepare(session, capabilities, timeout, reply) => {
                    let _ = reply.send(self.prepare(session, capabilities, timeout).await);
                }
                Operation::Forget(session, reply) => {
                    let result = self.fault.clone().map_or_else(
                        || {
                            self.sessions.remove(&session);
                            Ok(())
                        },
                        Err,
                    );
                    let _ = reply.send(result);
                }
                Operation::Stop(terminal, reply) => {
                    if terminal {
                        receiver.close();
                    }
                    let result = self.stop().await;
                    if terminal {
                        drop(receiver);
                        drop(self);
                        let _ = reply.send(result);
                        return;
                    }
                    let _ = reply.send(result);
                }
            }
        }
        let _ = self.stop().await;
    }

    async fn prepare(
        &mut self,
        session: String,
        capabilities: RuntimeCapabilities,
        timeout: Duration,
    ) -> Result<PreparedRuntime, Error> {
        if let Some(error) = &self.fault {
            return Err(error.clone());
        }
        // Preserve original validation even on the same-session fast path.
        let projection = project_runtime(&capabilities)?;
        let fingerprint = capabilities.fingerprint()?;
        let entry = self.clients.entry(fingerprint.clone()).or_insert_with(|| {
            let mut launch = self.launch.clone();
            launch.config_overrides = projection.config_overrides;
            launch.environment = projection
                .environment
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect();
            Entry {
                lifecycle: (self.factory)(launch),
                initialized: None,
            }
        });
        let client = entry.lifecycle.start(timeout).await?;
        let same_generation = entry
            .initialized
            .as_ref()
            .is_some_and(|previous| Arc::ptr_eq(&previous.shared, &client.shared));
        if !same_generation || self.sessions.get(&session) != Some(&fingerprint) {
            // A restarted process has lost roots even if the session is unchanged.
            client
                .request(
                    "skills/extraRoots/set",
                    json!({"extraRoots": projection.skill_roots}),
                    timeout,
                )
                .await?;
            entry.initialized = Some(client.clone());
        }
        self.sessions.insert(session, fingerprint.clone());
        Ok(PreparedRuntime {
            client,
            fingerprint,
        })
    }

    async fn stop(&mut self) -> Result<(), Error> {
        for (_, entry) in std::mem::take(&mut self.clients) {
            if let Err(error) = entry.lifecycle.shutdown().await {
                self.fault.get_or_insert(error);
            }
        }
        self.sessions.clear();
        self.fault.clone().map_or(Ok(()), Err)
    }
}

#[cfg(test)]
pub(super) mod tests;
