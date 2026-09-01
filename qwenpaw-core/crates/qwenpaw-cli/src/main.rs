use clap::Parser;
use clap::Subcommand;
use qwenpaw_app_server::AppServer;
use qwenpaw_app_server::DesktopCredentialStore;
use qwenpaw_app_server::SystemDesktopCredentialStore;
use qwenpaw_core::Core;
use qwenpaw_core::ModelConfig;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::info;
use tracing::warn;
use tracing_subscriber::EnvFilter;

const DESKTOP_PORT_FILE_ENV: &str = "QWENPAW_DESKTOP_PORT_FILE";

#[derive(Debug, Parser)]
#[command(name = "qwenpaw-core", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the protocol server used by `QwenPaw` clients.
    AppServer {
        /// Use newline-delimited JSON over stdin and stdout.
        #[arg(long, conflicts_with = "listen")]
        stdio: bool,
        /// Listen for HTTP and WebSocket clients on a loopback address.
        #[arg(long, value_name = "ADDR")]
        listen: Option<std::net::SocketAddr>,
        /// Serve the unchanged Console and Desktop compatibility endpoints.
        #[arg(long, requires = "listen", conflicts_with = "stdio")]
        desktop: bool,
        /// Directory containing the built Console `index.html` and assets.
        #[arg(
            long,
            value_name = "DIR",
            requires = "desktop",
            conflicts_with = "stdio"
        )]
        console_static_dir: Option<PathBuf>,
        /// Expose authenticated App Protocol `WebSocket` connections over TLS.
        #[arg(
            long,
            requires = "listen",
            requires_all = ["tls_cert", "tls_key", "auth_token_file"],
            conflicts_with_all = ["stdio", "desktop"]
        )]
        remote: bool,
        /// PEM certificate chain used by remote WSS.
        #[arg(long, value_name = "FILE", requires = "remote")]
        tls_cert: Option<PathBuf>,
        /// PEM private key used by remote WSS.
        #[arg(long, value_name = "FILE", requires = "remote")]
        tls_key: Option<PathBuf>,
        /// Permission-restricted bearer token file, re-read on every handshake.
        #[arg(long, value_name = "FILE", requires = "remote")]
        auth_token_file: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::AppServer {
            stdio,
            listen,
            desktop,
            console_static_dir,
            remote,
            tls_cert,
            tls_key,
            auth_token_file,
        } => {
            let database_path = core_database_path()?;
            let mut model_config = ModelConfig::from_env();
            let desktop_credentials = desktop.then(|| Arc::new(SystemDesktopCredentialStore));
            if model_config.api_key.is_none()
                && let Some(credentials) = &desktop_credentials
            {
                match credentials.load_api_key() {
                    Ok(api_key) => model_config.api_key = api_key,
                    Err(error) => {
                        warn!(error = %error, "Desktop system credential storage is unavailable");
                    }
                }
            }
            let core = Core::persistent(model_config, &database_path)?;
            let mut server = if desktop {
                let console_static_dir = console_static_dir
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("Desktop mode requires --console-static-dir"))?;
                let shutdown_token =
                    std::env::var("QWENPAW_DESKTOP_SHUTDOWN_TOKEN").map_err(|_| {
                        anyhow::anyhow!("Desktop mode requires QWENPAW_DESKTOP_SHUTDOWN_TOKEN")
                    })?;
                AppServer::new_desktop_with_credential_store(
                    core,
                    console_static_dir,
                    shutdown_token,
                    desktop_credentials.expect("Desktop credentials should be configured"),
                )?
            } else {
                AppServer::new(core)
            };
            if remote {
                let token_file = auth_token_file
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("remote mode requires --auth-token-file"))?;
                server = server.with_remote_auth_token_file(token_file)?;
            }
            if let Some(address) = listen {
                if remote {
                    let certificate = tls_cert
                        .as_deref()
                        .ok_or_else(|| anyhow::anyhow!("remote mode requires --tls-cert"))?;
                    let private_key = tls_key
                        .as_deref()
                        .ok_or_else(|| anyhow::anyhow!("remote mode requires --tls-key"))?;
                    let listener = std::net::TcpListener::bind(address)?;
                    let local_address = listener.local_addr()?;
                    info!(address = %local_address, "QwenPaw remote WSS app server listening");
                    return server.run_wss(listener, certificate, private_key).await;
                }
                if !address.ip().is_loopback() {
                    anyhow::bail!("HTTP App Protocol currently requires a loopback listen address");
                }
                let listener = tokio::net::TcpListener::bind(address).await?;
                let local_address = listener.local_addr()?;
                info!(address = %local_address, "QwenPaw HTTP app server listening");
                if desktop {
                    write_desktop_port_file(&database_path, local_address.port())?;
                    let mut stdout = std::io::stdout().lock();
                    writeln!(
                        stdout,
                        "QWENPAW_BACKEND_READY {}",
                        serde_json::json!({"port": local_address.port()})
                    )?;
                    stdout.flush()?;
                }
                server.run_http(listener).await
            } else {
                if !stdio {
                    info!("no transport selected; defaulting to stdio");
                }
                server.run_stdio().await
            }
        }
    }
}

fn core_database_path() -> anyhow::Result<std::path::PathBuf> {
    if let Some(path) = std::env::var_os("QWENPAW_HOME") {
        return Ok(std::path::PathBuf::from(path).join("threads.sqlite3"));
    }
    let data_directory = dirs::data_local_dir()
        .ok_or_else(|| anyhow::anyhow!("local data directory is unavailable"))?;
    Ok(data_directory
        .join("qwenpaw")
        .join("core")
        .join("threads.sqlite3"))
}

fn write_desktop_port_file(database_path: &Path, port: u16) -> anyhow::Result<()> {
    let port_file = match std::env::var_os(DESKTOP_PORT_FILE_ENV) {
        Some(path) => {
            let path = PathBuf::from(path);
            anyhow::ensure!(
                path.is_absolute(),
                "{DESKTOP_PORT_FILE_ENV} must be an absolute path"
            );
            path
        }
        None => database_path
            .parent()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Core database path has no data directory: {}",
                    database_path.display()
                )
            })?
            .join("desktop_port"),
    };
    let parent = port_file.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "Desktop port file has no containing directory: {}",
            port_file.display()
        )
    })?;
    std::fs::create_dir_all(parent)?;
    std::fs::write(port_file, format!("{port}\n"))?;
    Ok(())
}
