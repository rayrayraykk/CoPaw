use clap::Parser;
use clap::Subcommand;
use qwenpaw_app_server::AppServer;
use qwenpaw_core::Core;
use qwenpaw_core::ModelConfig;
use tracing::info;
use tracing_subscriber::EnvFilter;

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
        Command::AppServer { stdio, listen } => {
            let database_path = core_database_path()?;
            let server = AppServer::new(Core::persistent(ModelConfig::from_env(), &database_path)?);
            if let Some(address) = listen {
                if !address.ip().is_loopback() {
                    anyhow::bail!("HTTP App Protocol currently requires a loopback listen address");
                }
                let listener = tokio::net::TcpListener::bind(address).await?;
                info!(address = %listener.local_addr()?, "QwenPaw HTTP app server listening");
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
