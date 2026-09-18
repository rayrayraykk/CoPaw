//! One process owner per Codex client, not a replacement for workspace sessions.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::process::Command;
use tokio::sync::{mpsc, oneshot};

use super::discovery::BinaryResolution;
use super::{CodexClient, CodexProcess, Error, RequestHandler};

/// Host launch inputs; an absent binary requires discovery before spawning.
/// Deliberately not Debug: overrides can be secrets.
#[derive(Clone, PartialEq, Eq)]
pub struct LaunchConfig {
    pub binary: Option<BinaryResolution>,
    pub cwd: PathBuf,
    pub base_environment: HashMap<OsString, OsString>,
    pub config_overrides: Vec<String>,
    pub environment: HashMap<OsString, OsString>,
}

impl LaunchConfig {
    fn command(&self) -> Result<Command, Error> {
        let binary = self.binary.as_ref().ok_or(Error::NotInstalled)?;
        if !binary.path.is_absolute() || !self.cwd.is_absolute() {
            return Err(Error::Io(std::io::ErrorKind::InvalidInput));
        }
        let mut command = Command::new(&binary.path);
        command.arg("app-server");
        for value in &self.config_overrides {
            command.arg("-c").arg(value);
        }
        command
            .args(["--listen", "stdio://"])
            .current_dir(&self.cwd)
            .env_clear()
            .envs(&self.base_environment)
            .envs(&self.environment);
        Ok(command)
    }
}

type Reply<T> = oneshot::Sender<Result<T, Error>>;
type Launcher = fn(&LaunchConfig) -> Result<Command, Error>;
pub(super) type Resolver = Arc<dyn Fn() -> Option<BinaryResolution> + Send + Sync>;

enum Operation {
    Start(Duration, Reply<CodexClient>),
    Configure(LaunchConfig, Reply<bool>),
    Stop(Reply<()>),
    Shutdown(Reply<()>),
}

/// Cloneable access to one serialized process owner, independent of RPC reads.
///
/// Accepted operations finish even if their caller drops its reply. Dropping all
/// handles requests cleanup; use explicit shutdown to verify that it completed.
#[derive(Clone)]
pub struct CodexLifecycle {
    sender: mpsc::Sender<Operation>,
}

impl CodexLifecycle {
    /// Creates an idle owner. The handler is installed before any protocol read.
    ///
    /// # Panics
    /// Panics if called outside a Tokio runtime.
    #[must_use]
    pub fn new(config: LaunchConfig, handler: Option<RequestHandler>) -> Self {
        Self::with_launcher(config, handler, LaunchConfig::command, None)
    }

    pub(super) fn discovering(
        config: LaunchConfig,
        handler: Option<RequestHandler>,
        resolver: Resolver,
    ) -> Self {
        Self::with_launcher(config, handler, LaunchConfig::command, Some(resolver))
    }

    pub(super) fn with_launcher(
        config: LaunchConfig,
        handler: Option<RequestHandler>,
        launcher: Launcher,
        resolver: Option<Resolver>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(16);
        tokio::spawn(
            Owner {
                config,
                handler,
                process: None,
                fault: None,
                launcher,
                resolver,
            }
            .run(receiver),
        );
        Self { sender }
    }

    /// Starts once or returns the current generation; the timeout bounds handshake.
    ///
    /// # Errors
    /// Returns launch/handshake/cleanup errors, or Closed after owner shutdown.
    pub async fn start(&self, timeout: Duration) -> Result<CodexClient, Error> {
        let (reply, receive) = oneshot::channel();
        self.sender
            .send(Operation::Start(timeout, reply))
            .await
            .map_err(|_| Error::Closed)?;
        receive.await.map_err(|_| Error::Closed)?
    }

    /// Applies changed launch inputs after stopping the old generation.
    /// Returns whether a live generation was stopped; does not eagerly restart.
    ///
    /// # Errors
    /// Returns a latched cleanup error or Closed after owner shutdown.
    pub async fn configure(&self, config: LaunchConfig) -> Result<bool, Error> {
        let (reply, receive) = oneshot::channel();
        self.sender
            .send(Operation::Configure(config, reply))
            .await
            .map_err(|_| Error::Closed)?;
        receive.await.map_err(|_| Error::Closed)?
    }

    /// Stops this generation, permitting a later start after a clean stop.
    ///
    /// # Errors
    /// Returns abnormal/forced cleanup failure or a closed owner.
    pub async fn stop(&self) -> Result<(), Error> {
        let (reply, receive) = oneshot::channel();
        self.sender
            .send(Operation::Stop(reply))
            .await
            .map_err(|_| Error::Closed)?;
        receive.await.map_err(|_| Error::Closed)?
    }

    /// Closes admission for every clone and waits for owned process cleanup.
    ///
    /// # Errors
    /// Returns cleanup failure or Closed if already shut down.
    pub async fn shutdown(&self) -> Result<(), Error> {
        let (reply, receive) = oneshot::channel();
        self.sender
            .send(Operation::Shutdown(reply))
            .await
            .map_err(|_| Error::Closed)?;
        receive.await.map_err(|_| Error::Closed)?
    }
}

struct Owner {
    config: LaunchConfig,
    handler: Option<RequestHandler>,
    process: Option<CodexProcess>,
    fault: Option<Error>,
    launcher: Launcher,
    resolver: Option<Resolver>,
}

impl Owner {
    async fn run(mut self, mut receiver: mpsc::Receiver<Operation>) {
        while let Some(operation) = receiver.recv().await {
            match operation {
                Operation::Start(timeout, reply) => {
                    let _ = reply.send(self.start(timeout).await);
                }
                Operation::Configure(config, reply) => {
                    let _ = reply.send(self.configure(config).await);
                }
                Operation::Stop(reply) => {
                    let _ = reply.send(self.stop(false).await.map(|_| ()));
                }
                Operation::Shutdown(reply) => {
                    receiver.close();
                    let result = self.stop(false).await.map(|_| ());
                    // Reject queued callers and release callback captures before
                    // publishing terminal cleanup, not on a later actor poll.
                    drop(receiver);
                    self.handler.take();
                    let _ = reply.send(result);
                    return;
                }
            }
        }
        let _ = self.stop(false).await;
    }

    async fn start(&mut self, timeout: Duration) -> Result<CodexClient, Error> {
        if let Some(error) = &self.fault {
            return Err(error.clone());
        }
        if let Some(process) = &self.process {
            if !process.client.shared.stop.is_cancelled() {
                return Ok(process.client());
            }
            self.stop(true).await?;
        }
        if let Some(resolver) = &self.resolver {
            let resolver = resolver.clone();
            self.config.binary = tokio::task::spawn_blocking(move || resolver())
                .await
                .map_err(|_| Error::Worker)?;
            if self.config.binary.is_none() {
                return Err(Error::NotInstalled);
            }
        }
        let command = (self.launcher)(&self.config)?;
        match CodexProcess::spawn_with_handler(command, timeout, self.handler.clone()).await {
            Ok(process) => {
                let client = process.client();
                self.process = Some(process);
                Ok(client)
            }
            Err(error) => {
                if matches!(error, Error::StartupCleanup(_)) {
                    self.fault = Some(error.clone());
                }
                Err(error)
            }
        }
    }

    async fn stop(&mut self, allow_exited: bool) -> Result<bool, Error> {
        if let Some(error) = &self.fault {
            return Err(error.clone());
        }
        let Some(process) = self.process.take() else {
            return Ok(false);
        };
        let was_running = !process.client.shared.stop.is_cancelled();
        if let Err(error) = process.shutdown().await
            && !(allow_exited && !was_running && matches!(error, Error::ProcessExit(_)))
        {
            self.fault = Some(error.clone());
            return Err(error);
        }
        Ok(was_running)
    }

    async fn configure(&mut self, config: LaunchConfig) -> Result<bool, Error> {
        if let Some(error) = &self.fault {
            return Err(error.clone());
        }
        if config == self.config {
            return Ok(false);
        }
        let was_running = self.stop(true).await?;
        self.config = config;
        Ok(was_running)
    }
}

#[cfg(test)]
mod tests;
