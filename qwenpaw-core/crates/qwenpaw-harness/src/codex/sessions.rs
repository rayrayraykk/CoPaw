//! Persisted session-to-thread bindings over the projected process pool.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use std::time::Duration;

use serde_json::json;
use tokio::sync::{mpsc, oneshot};

use super::runtime::{CodexRuntimePool, PreparedRuntime};
use super::{CodexClient, Error, Shared};
use crate::capabilities::{RuntimeCapabilities, path_text};

mod store;

#[derive(Clone, Default)]
pub struct ThreadOptions {
    pub sandbox: Option<String>,
    pub approval_policy: Option<String>,
    pub model: Option<String>,
}

#[derive(Clone)]
pub struct SessionRequest {
    pub session_id: String,
    pub capabilities: RuntimeCapabilities,
    pub cwd: PathBuf,
    pub options: ThreadOptions,
}

/// The client and loaded thread both belong to one process generation.
#[derive(Clone)]
pub struct PreparedSession {
    pub runtime: PreparedRuntime,
    pub thread_id: String,
}

type Reply<T> = oneshot::Sender<Result<T, Error>>;
enum Operation {
    Prepare(SessionRequest, Duration, Reply<PreparedSession>),
    Reset(String, Reply<()>),
    Stop(bool, Reply<()>),
}

/// One in-process writer per workspace. Accepted requests finish after waiter
/// cancellation. The parent Provider still owns control and MCP discovery.
#[derive(Clone)]
pub struct CodexSessions {
    sender: mpsc::Sender<Operation>,
}

impl CodexSessions {
    /// Prepares the persisted session and starts an owned raw notification stream.
    /// The adapter must serialize turns/reset/replacement for the same session.
    ///
    /// # Errors
    /// Returns input or session errors. Remote turn errors are reported by finish.
    pub async fn start_turn(
        &self,
        request: SessionRequest,
        input: super::turn::TurnInput,
        timeout: Duration,
    ) -> Result<super::turn::CodexTurn, Error> {
        let params = super::turn::parameters(&request, input)?;
        let session = self.prepare(request, timeout).await?;
        super::turn::CodexTurn::start(session, params, timeout)
    }

    pub(super) async fn open(directory: PathBuf, pool: CodexRuntimePool) -> Result<Self, Error> {
        let (path, threads) = tokio::task::spawn_blocking(move || store::open(&directory))
            .await
            .map_err(|_| Error::Worker)??;
        let (sender, receiver) = mpsc::channel(16);
        tokio::spawn(
            Owner {
                path,
                pool,
                threads,
                pending: BTreeMap::new(),
                loaded: Vec::new(),
            }
            .run(receiver),
        );
        Ok(Self { sender })
    }

    /// Prepares roots, resumes or starts a thread, then publishes its mapping.
    /// Timeout bounds each RPC/handshake separately, not queue or file I/O time.
    ///
    /// # Errors
    /// Returns state I/O, projection or protocol errors without an unsaved success.
    pub async fn prepare(
        &self,
        request: SessionRequest,
        timeout: Duration,
    ) -> Result<PreparedSession, Error> {
        let (reply, receive) = oneshot::channel();
        self.sender
            .send(Operation::Prepare(request, timeout, reply))
            .await
            .map_err(|_| Error::Closed)?;
        receive.await.map_err(|_| Error::Closed)?
    }

    /// Publishes removal before forgetting the pooled binding; the next turn is new.
    /// Does not delete a remote thread or other sessions' mappings.
    ///
    /// # Errors
    /// Returns persistence or pool errors; failed publication keeps the old mapping.
    pub async fn reset_session(&self, session: String) -> Result<(), Error> {
        let (reply, receive) = oneshot::channel();
        self.sender
            .send(Operation::Reset(session, reply))
            .await
            .map_err(|_| Error::Closed)?;
        receive.await.map_err(|_| Error::Closed)?
    }

    /// Stops projected processes and loaded markers, preserving persisted mappings.
    ///
    /// # Errors
    /// Returns pool cleanup errors or Closed.
    pub async fn stop(&self) -> Result<(), Error> {
        self.stop_with(false).await
    }

    /// Closes session admission and stops the pool; Provider controls remain usable.
    ///
    /// # Errors
    /// Returns pool cleanup errors or Closed. Failed publication was already reported
    /// by prepare; unpublished remote threads are not guaranteed after shutdown.
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

struct Loaded {
    generation: Weak<Shared>,
    threads: HashSet<String>,
}

struct Owner {
    path: PathBuf,
    pool: CodexRuntimePool,
    threads: BTreeMap<String, String>,
    // Retain acknowledged remote IDs after failed publication, avoiding another
    // thread/start on an in-process retry. No crash-recovery guarantee is implied.
    pending: BTreeMap<String, String>,
    loaded: Vec<Loaded>,
}

impl Owner {
    async fn run(mut self, mut receiver: mpsc::Receiver<Operation>) {
        while let Some(operation) = receiver.recv().await {
            match operation {
                Operation::Prepare(request, timeout, reply) => {
                    let _ = reply.send(self.prepare(request, timeout).await);
                }
                Operation::Reset(session, reply) => {
                    let _ = reply.send(self.reset(session).await);
                }
                Operation::Stop(terminal, reply) => {
                    if terminal {
                        receiver.close();
                    }
                    let result = self.pool.stop().await;
                    self.loaded.clear();
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
        let _ = self.pool.stop().await;
    }

    fn loaded_threads(&mut self, client: &CodexClient) -> &mut HashSet<String> {
        self.loaded.retain(|entry| {
            entry
                .generation
                .upgrade()
                .is_some_and(|shared| !shared.stop.is_cancelled())
        });
        let generation = Arc::downgrade(&client.shared);
        let index = self
            .loaded
            .iter()
            .position(|entry| Weak::ptr_eq(&entry.generation, &generation))
            .unwrap_or_else(|| {
                self.loaded.push(Loaded {
                    generation,
                    threads: HashSet::new(),
                });
                self.loaded.len() - 1
            });
        &mut self.loaded[index].threads
    }

    async fn prepare(
        &mut self,
        request: SessionRequest,
        timeout: Duration,
    ) -> Result<PreparedSession, Error> {
        let cwd = path_text(&request.cwd)?.to_owned();
        let runtime = self
            .pool
            .prepare(request.session_id.clone(), request.capabilities, timeout)
            .await?;
        let client = &runtime.client;
        let mut thread = self
            .pending
            .get(&request.session_id)
            .or_else(|| self.threads.get(&request.session_id))
            .cloned();
        if let Some(id) = &thread
            && !self.loaded_threads(client).contains(id)
        {
            match client
                .request("thread/resume", json!({"threadId":id}), timeout)
                .await
            {
                Ok(_) => {
                    self.loaded_threads(client).insert(id.clone());
                }
                Err(Error::Protocol { .. }) => thread = None,
                Err(error) => return Err(error),
            }
        }
        let thread_id = if let Some(id) = thread {
            id
        } else {
            let options = request.options;
            let mut params = json!({"cwd":cwd,
                "sandbox":options.sandbox.as_deref().filter(|s| !s.is_empty()).unwrap_or("workspace-write"),
                "approvalPolicy":options.approval_policy.as_deref().filter(|s| !s.is_empty()).unwrap_or("on-request")});
            if let Some(model) = options.model.filter(|s| !s.is_empty()) {
                params["model"] = json!(model);
            }
            let response = client.request("thread/start", params, timeout).await?;
            let id = response
                .get("thread")
                .and_then(|thread| thread.get("id"))
                .and_then(serde_json::Value::as_str)
                .filter(|s| !s.is_empty())
                .ok_or(Error::MissingThreadId)?
                .to_owned();
            self.loaded_threads(client).insert(id.clone());
            self.pending.insert(request.session_id.clone(), id.clone());
            id
        };
        if self.pending.contains_key(&request.session_id) {
            let mut candidate = self.threads.clone();
            candidate.insert(request.session_id.clone(), thread_id.clone());
            self.publish(candidate).await?;
            self.pending.remove(&request.session_id);
        }
        Ok(PreparedSession { runtime, thread_id })
    }

    async fn publish(&mut self, candidate: BTreeMap<String, String>) -> Result<(), Error> {
        let path = self.path.clone();
        let candidate = tokio::task::spawn_blocking(move || {
            store::write(&path, &candidate)?;
            Ok::<_, Error>(candidate)
        })
        .await
        .map_err(|_| Error::Worker)??;
        self.threads = candidate;
        Ok(())
    }

    async fn reset(&mut self, session: String) -> Result<(), Error> {
        let mut candidate = self.threads.clone();
        let old = candidate.remove(&session);
        self.publish(candidate).await?;
        let pending = self.pending.remove(&session);
        for loaded in &mut self.loaded {
            for id in old.iter().chain(pending.iter()) {
                loaded.threads.remove(id);
            }
        }
        self.pool.forget_session(session).await
    }
}

#[cfg(test)]
mod tests;
