//! Owned, bounded one-shot CLI discovery; independent of the app-server pipe.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};
use tokio::task::{JoinError, JoinSet};
use tokio_util::sync::CancellationToken;

use super::control::{first_text, truthy};
use super::discovery::BinaryResolution;
use super::{Error, STOP_TIMEOUT};

const MAX_STDOUT: usize = 8 * 1024 * 1024;
const MAX_STDERR: usize = 1024 * 1024;
const MAX_PROCESSES: usize = 8;

pub(super) type Launcher =
    fn(&BinaryResolution, &Path, &HashMap<OsString, OsString>) -> Result<Command, Error>;

/// Original read-only metadata, without provider-owned configuration or secrets.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HarnessDiscoveredMcpServer {
    pub name: String,
    pub provider_id: &'static str,
    pub transport: String,
    pub enabled: bool,
    pub auth_status: String,
    pub read_only: bool,
    pub scope: &'static str,
}

pub(super) fn command(
    binary: &BinaryResolution,
    cwd: &Path,
    environment: &HashMap<OsString, OsString>,
) -> Result<Command, Error> {
    if !binary.path.is_absolute() || !cwd.is_absolute() {
        return Err(Error::Io(std::io::ErrorKind::InvalidInput));
    }
    let mut command = Command::new(&binary.path);
    command
        .args(["mcp", "list", "--json"])
        .current_dir(cwd)
        .env_clear()
        .envs(environment);
    Ok(command)
}

type ResultList = Result<Vec<HarnessDiscoveredMcpServer>, Error>;
type Reply = oneshot::Sender<ResultList>;
type Finished = Result<(Reply, ResultList), JoinError>;

struct Request {
    command: Command,
    timeout: Duration,
    reply: Reply,
}

enum Operation {
    Run(Box<Request>),
    Stop(oneshot::Sender<Result<(), Error>>),
    Shutdown(oneshot::Sender<Result<(), Error>>),
}

/// Provider-owned discovery jobs. Last-handle drop requests asynchronous cleanup.
#[derive(Clone)]
pub(super) struct McpDiscovery {
    sender: mpsc::Sender<Operation>,
}

impl McpDiscovery {
    pub(super) fn new() -> Self {
        let (sender, receiver) = mpsc::channel(16);
        tokio::spawn(Owner::default().run(receiver));
        Self { sender }
    }

    pub(super) async fn discover(&self, command: Command, timeout: Duration) -> ResultList {
        let (reply, receive) = oneshot::channel();
        self.sender
            .send(Operation::Run(Box::new(Request {
                command,
                timeout,
                reply,
            })))
            .await
            .map_err(|_| Error::Closed)?;
        receive.await.map_err(|_| Error::Closed)?
    }

    pub(super) async fn stop(&self, terminal: bool) -> Result<(), Error> {
        let (reply, receive) = oneshot::channel();
        let operation = if terminal {
            Operation::Shutdown(reply)
        } else {
            Operation::Stop(reply)
        };
        self.sender
            .send(operation)
            .await
            .map_err(|_| Error::Closed)?;
        receive.await.map_err(|_| Error::Closed)?
    }
}

#[derive(Default)]
struct Owner {
    jobs: JoinSet<(Reply, ResultList)>,
    cancel: CancellationToken,
    fault: Option<Error>,
}

impl Owner {
    async fn run(mut self, mut receiver: mpsc::Receiver<Operation>) {
        loop {
            tokio::select! {
                operation = receiver.recv() => match operation {
                    Some(Operation::Run(request)) => self.start(*request),
                    Some(Operation::Stop(reply)) => { let _ = reply.send(self.stop().await); }
                    Some(Operation::Shutdown(reply)) => {
                        receiver.close();
                        let _ = reply.send(self.stop().await);
                        return;
                    }
                    None => { let _ = self.stop().await; return; }
                },
                Some(result) = self.jobs.join_next(), if !self.jobs.is_empty() => self.finish(result),
            }
        }
    }

    fn start(&mut self, mut request: Request) {
        while let Some(result) = self.jobs.try_join_next() {
            self.finish(result);
        }
        if request.reply.is_closed() {
            return;
        }
        let failure = self
            .fault
            .clone()
            .or_else(|| (self.jobs.len() >= MAX_PROCESSES).then_some(Error::Capacity));
        if let Some(error) = failure {
            let _ = request.reply.send(Err(error));
            return;
        }
        let cancel = self.cancel.clone();
        self.jobs.spawn(async move {
            let result = execute(
                request.command,
                request.timeout,
                &cancel,
                &mut request.reply,
            )
            .await;
            (request.reply, result)
        });
    }

    fn finish(&mut self, result: Finished) {
        match result {
            Ok((reply, result)) => {
                if let Err(error @ Error::McpCleanup(_)) = &result {
                    self.fault = Some(error.clone());
                }
                let _ = reply.send(result);
            }
            Err(_) => self.fault = Some(Error::Worker),
        }
    }

    async fn stop(&mut self) -> Result<(), Error> {
        self.cancel.cancel();
        while let Some(result) = self.jobs.join_next().await {
            self.finish(result);
        }
        self.cancel = CancellationToken::new();
        self.fault.clone().map_or(Ok(()), Err)
    }
}

async fn execute(
    mut command: Command,
    timeout: Duration,
    cancel: &CancellationToken,
    reply: &mut Reply,
) -> ResultList {
    if cancel.is_cancelled() || reply.is_closed() {
        return Err(Error::Closed);
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let result = tokio::select! {
        () = cancel.cancelled() => Err(Error::Closed),
        () = reply.closed() => Err(Error::Closed),
        result = tokio::time::timeout(timeout, collect(&mut child)) => result.unwrap_or(Err(Error::Timeout)),
    };
    if result.is_err() {
        reap(&mut child)
            .await
            .map_err(|error| Error::McpCleanup(Box::new(error)))?;
    }
    result
}

async fn reap(child: &mut Child) -> Result<(), Error> {
    if child.try_wait()?.is_none() {
        tokio::time::timeout(STOP_TIMEOUT, child.kill())
            .await
            .map_err(|_| Error::StopTimeout)??;
    }
    Ok(())
}

async fn read_bounded(reader: impl AsyncRead + Unpin, limit: usize) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    reader
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() > limit {
        return Err(Error::McpOutputLimit);
    }
    Ok(bytes)
}

async fn collect(child: &mut Child) -> ResultList {
    let stdout = child.stdout.take().ok_or(Error::Worker)?;
    let stderr = child.stderr.take().ok_or(Error::Worker)?;
    let (stdout, stderr, status) = tokio::try_join!(
        read_bounded(stdout, MAX_STDOUT),
        read_bounded(stderr, MAX_STDERR),
        async { child.wait().await.map_err(Error::from) }
    )?;
    project(&stdout, &stderr, status.success())
}

fn project(stdout: &[u8], stderr: &[u8], success: bool) -> ResultList {
    if !success {
        return Err(Error::McpDiscovery(format!(
            "Failed to discover Codex MCP servers: {}",
            String::from_utf8_lossy(stderr).trim()
        )));
    }
    let payload: Value = serde_json::from_slice(stdout)
        .map_err(|_| Error::McpDiscovery("Codex returned invalid MCP discovery data".to_owned()))?;
    let Value::Array(items) = payload else {
        return Ok(Vec::new());
    };
    let mut discovered = Vec::new();
    for item in items {
        let Value::Object(item) = item else {
            continue;
        };
        if !item.get("name").is_some_and(truthy) {
            continue;
        }
        let transport = item.get("transport").filter(|value| truthy(value));
        let transport = match transport {
            None => String::new(),
            Some(Value::Object(transport)) => first_text(transport, &["type"])?,
            Some(_) => return Err(Error::InvalidFrame),
        };
        discovered.push(HarnessDiscoveredMcpServer {
            name: first_text(&item, &["name"])?,
            provider_id: "codex",
            transport,
            enabled: item.get("enabled").is_some_and(truthy),
            auth_status: first_text(&item, &["auth_status"])?,
            read_only: true,
            scope: "provider",
        });
    }
    Ok(discovered)
}

#[cfg(test)]
pub(super) mod tests;
