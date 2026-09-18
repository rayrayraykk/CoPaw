//! Codex control entry points composed from discovery and owned native transport.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use super::discovery::{BinaryResolution, DiscoveryContext};
use super::lifecycle::{CodexLifecycle, LaunchConfig, Resolver};
use super::mcp_discovery::{self, McpDiscovery};
use super::runtime::{CodexRuntimePool, PreparedRuntime};
use super::{
    Error, HarnessDiscoveredMcpServer, HarnessDiscoveredSkill, HarnessModel, RequestHandler,
};
use crate::capabilities::RuntimeCapabilities;
use crate::catalog::{self, ProviderStatus};

pub const INSTALL_MESSAGE: &str =
    "Codex runtime not found. Install qwenpaw[codex] or provide a standalone Codex CLI.";

/// Fixed host/settings inputs for one provider, separate from projected sessions.
#[derive(Clone)]
pub struct CodexProviderSettings {
    pub binary: Option<String>,
    pub bundled: Option<BinaryResolution>,
    pub cwd: PathBuf,
    pub home: PathBuf,
    pub environment: HashMap<OsString, OsString>,
}

impl CodexProviderSettings {
    fn resolve(&self) -> Option<BinaryResolution> {
        let binary = self
            .binary
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        DiscoveryContext {
            cwd: &self.cwd,
            home: &self.home,
            environment: &self.environment,
        }
        .resolve(binary.map(OsStr::new), self.bundled.as_ref())
    }

    fn launch(&self) -> LaunchConfig {
        LaunchConfig {
            binary: None,
            cwd: self.cwd.clone(),
            base_environment: self.environment.clone(),
            config_overrides: Vec::new(),
            environment: HashMap::new(),
        }
    }
}

/// Provider controls and read-only discovery; does not implement Agent turns.
pub struct CodexProvider {
    settings: Arc<CodexProviderSettings>,
    lifecycle: CodexLifecycle,
    mcp: McpDiscovery,
    runtime: CodexRuntimePool,
}

impl CodexProvider {
    /// Opens one workspace's persisted session owner over this provider's pool.
    /// The caller must keep a single writer per state directory and serialize
    /// provider replacement against session use. Control calls do not open it.
    ///
    /// # Errors
    /// Returns state-directory or persisted-mapping errors.
    pub async fn open_sessions(
        &self,
        state_dir: PathBuf,
    ) -> Result<super::sessions::CodexSessions, Error> {
        super::sessions::CodexSessions::open(state_dir, self.runtime.clone()).await
    }

    /// Creates a provider without probing files or starting an executable.
    ///
    /// # Errors
    /// Rejects relative host directories instead of reporting a missing CLI.
    ///
    /// # Panics
    /// Panics if called outside a Tokio runtime.
    pub fn new(
        settings: CodexProviderSettings,
        handler: Option<RequestHandler>,
    ) -> Result<Self, Error> {
        if !settings.cwd.is_absolute() || !settings.home.is_absolute() {
            return Err(Error::Io(std::io::ErrorKind::InvalidInput));
        }
        let settings = Arc::new(settings);
        let source = settings.clone();
        let resolver: Resolver = Arc::new(move || source.resolve());
        let runtime = CodexRuntimePool::new(settings.launch(), handler.clone(), resolver.clone());
        let lifecycle = CodexLifecycle::discovering(settings.launch(), handler, resolver);
        Ok(Self {
            settings,
            lifecycle,
            mcp: McpDiscovery::new(),
            runtime,
        })
    }

    async fn resolution(&self) -> Result<Option<BinaryResolution>, Error> {
        let settings = self.settings.clone();
        tokio::task::spawn_blocking(move || settings.resolve())
            .await
            .map_err(|_| Error::Worker)
    }

    /// Returns the original missing-runtime message for capability routes.
    ///
    /// # Errors
    /// Returns worker failure, not a fabricated missing installation.
    pub async fn capability_unavailable_message(&self) -> Result<Option<&'static str>, Error> {
        Ok(self
            .resolution()
            .await?
            .is_none()
            .then_some(INSTALL_MESSAGE))
    }

    /// Probes live account state and includes complete original catalog metadata.
    ///
    /// # Errors
    /// Propagates I/O/worker failures instead of treating them as an absent CLI.
    pub async fn status(&self, timeout: Duration) -> Result<ProviderStatus, Error> {
        let mut status = catalog::codex().status();
        let Some(resolution) = self.resolution().await? else {
            status.error = Some(INSTALL_MESSAGE.to_owned());
            return Ok(status);
        };
        status.installed = true;
        status.runtime_path = Some(resolution.path.to_string_lossy().into_owned());
        status.runtime_source = Some(resolution.source);
        let result = async {
            self.lifecycle
                .start(timeout)
                .await?
                .account_status(timeout)
                .await
        }
        .await;
        match result {
            Ok(account) => {
                status.authenticated = account.authenticated;
                status.account = account.account;
            }
            Err(error @ (Error::Io(_) | Error::Worker)) => return Err(error),
            Err(Error::Protocol { message, .. }) => status.error = Some(message),
            Err(error) => status.error = Some(error.to_string()),
        }
        Ok(status)
    }

    /// Reads all models from the provider; absence is an error, not an empty list.
    ///
    /// # Errors
    /// Returns discovery, process or model protocol failures.
    pub async fn models(&self, timeout: Duration) -> Result<Vec<HarnessModel>, Error> {
        self.lifecycle.start(timeout).await?.models(timeout).await
    }

    /// Discovers read-only skills using the existing provider-owned connection.
    ///
    /// # Errors
    /// Returns discovery, startup or skill protocol failures without empty fallback.
    pub async fn discover_skills(
        &self,
        cwd: &Path,
        timeout: Duration,
    ) -> Result<Vec<HarnessDiscoveredSkill>, Error> {
        self.lifecycle
            .start(timeout)
            .await?
            .discover_skills(cwd, timeout)
            .await
    }

    /// Lists provider-owned MCP metadata using a separate, owned CLI process.
    ///
    /// # Errors
    /// Returns discovery/process/output failures; missing installation is empty.
    pub async fn discover_mcp(
        &self,
        cwd: &Path,
        timeout: Duration,
    ) -> Result<Vec<HarnessDiscoveredMcpServer>, Error> {
        self.discover_mcp_with(cwd, timeout, mcp_discovery::command)
            .await
    }

    async fn discover_mcp_with(
        &self,
        cwd: &Path,
        timeout: Duration,
        launcher: mcp_discovery::Launcher,
    ) -> Result<Vec<HarnessDiscoveredMcpServer>, Error> {
        let Some(binary) = self.resolution().await? else {
            return Ok(Vec::new());
        };
        self.mcp
            .discover(launcher(&binary, cwd, &self.settings.environment)?, timeout)
            .await
    }

    /// Starts the original browser/device-code flow; does not persist settings.
    ///
    /// # Errors
    /// Returns discovery, process or login protocol failures.
    pub async fn start_login(&self, device_code: bool, timeout: Duration) -> Result<Value, Error> {
        self.lifecycle
            .start(timeout)
            .await?
            .start_login(device_code, timeout)
            .await
    }

    /// Waits for provider-owned logout without inspecting credential storage.
    ///
    /// # Errors
    /// Returns discovery, process or logout protocol failures.
    pub async fn logout(&self, timeout: Duration) -> Result<(), Error> {
        self.lifecycle.start(timeout).await?.logout(timeout).await
    }

    /// Prepares a projected session separately from the provider control connection.
    ///
    /// # Errors
    /// Returns projection, discovery, process or roots initialization failures.
    pub async fn prepare_runtime(
        &self,
        session_id: String,
        capabilities: RuntimeCapabilities,
        timeout: Duration,
    ) -> Result<PreparedRuntime, Error> {
        self.runtime
            .prepare(session_id, capabilities, timeout)
            .await
    }

    /// Forgets only the runtime binding; the adapter must handle persisted threads.
    ///
    /// # Errors
    /// Returns closed or failed pool errors.
    pub async fn forget_runtime_session(&self, session_id: String) -> Result<(), Error> {
        self.runtime.forget_session(session_id).await
    }

    /// Stops all control, discovery and projected processes, permitting restart.
    ///
    /// # Errors
    /// Returns cleanup failures from the process owner.
    pub async fn stop(&self) -> Result<(), Error> {
        let (process, discovery, runtime) = tokio::join!(
            self.lifecycle.stop(),
            self.mcp.stop(false),
            self.runtime.stop()
        );
        process.and(discovery).and(runtime)
    }

    /// Permanently closes this provider's process owner and confirms cleanup.
    ///
    /// # Errors
    /// Returns cleanup failures or an already closed owner.
    pub async fn shutdown(&self) -> Result<(), Error> {
        let (process, discovery, runtime) = tokio::join!(
            self.lifecycle.shutdown(),
            self.mcp.stop(true),
            self.runtime.shutdown()
        );
        process.and(discovery).and(runtime)
    }
}

#[cfg(test)]
mod tests;
