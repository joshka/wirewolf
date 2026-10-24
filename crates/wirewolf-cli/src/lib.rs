//! Command line interface for wirewolf.

use std::net::SocketAddr;

use clap::{Parser, Subcommand};
use wirewolf_core::{Hub, demo};

#[derive(Debug, Parser)]
#[command(name = "wirewolf", version, about = "See what's wrong with your network")]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the probes and serve the dashboard and API.
    Serve {
        /// Address to listen on.
        #[arg(long, env = "WIREWOLF_LISTEN", default_value = "0.0.0.0:7465")]
        listen: SocketAddr,
        /// Serve scripted demo data instead of probing the network.
        #[arg(long, env = "WIREWOLF_DEMO")]
        demo: bool,
        /// Demo scenario to start with.
        #[arg(long, default_value = "healthy", requires = "demo")]
        scenario: String,
    },
}

pub fn run() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let cli = Cli::parse();
    tokio::runtime::Runtime::new()?.block_on(async {
        match cli.command {
            Command::Serve { listen, demo: true, scenario } => {
                let hub = Hub::new(demo::meta(&scenario));
                tokio::spawn(demo::run(hub.clone(), scenario));
                wirewolf_web::serve(hub, listen).await?;
            }
            Command::Serve { demo: false, .. } => {
                anyhow::bail!("live probes are not implemented yet; use --demo");
            }
        }
        Ok(())
    })
}
