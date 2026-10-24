//! Command line interface for wirewolf.

mod check;

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
        #[command(flatten)]
        live: LiveArgs,
    },
    /// Open the terminal dashboard for a running wirewolf server.
    Tui {
        /// The wirewolf server to connect to.
        #[arg(long, env = "WIREWOLF_URL", default_value = DEFAULT_URL)]
        url: String,
    },
    /// Print the current verdict and exit 0 healthy, 1 degraded, 2 down, 3 unreachable,
    /// 4 unknown.
    Check {
        /// The wirewolf server to ask.
        #[arg(long, env = "WIREWOLF_URL", default_value = DEFAULT_URL)]
        url: String,
        /// Print the snapshot as JSON instead of text.
        #[arg(long)]
        json: bool,
    },
}

const DEFAULT_URL: &str = "http://127.0.0.1:7465";

/// Settings for live probing (ignored with `--demo`).
#[derive(Debug, clap::Args)]
struct LiveArgs {
    /// Time between probe cycles, for example `30s`.
    #[arg(long, env = "WIREWOLF_INTERVAL", default_value = "30s", value_parser = parse_duration)]
    interval: std::time::Duration,
    /// Time between scheduled speed tests, for example `60m`. `0` disables them.
    #[arg(long, env = "WIREWOLF_SPEED_INTERVAL", default_value = "60m", value_parser = parse_duration)]
    speed_interval: std::time::Duration,
    /// Speed test backend: `cloudflare` or `ookla` (needs the `speedtest` CLI).
    #[arg(long, env = "WIREWOLF_SPEED_BACKEND", default_value = "cloudflare")]
    speed_backend: wirewolf_probe::Backend,
    /// SQLite history file.
    #[arg(long, env = "WIREWOLF_DB", default_value = "wirewolf.db")]
    db: std::path::PathBuf,
    /// How long to keep history, for example `30d`.
    #[arg(long, env = "WIREWOLF_RETENTION", default_value = "30d", value_parser = parse_duration)]
    retention: std::time::Duration,
}

fn parse_duration(s: &str) -> Result<std::time::Duration, String> {
    if s == "0" {
        return Ok(std::time::Duration::ZERO);
    }
    let span: jiff::Span = s.parse().map_err(|e| format!("{e}"))?;
    let d = span.to_duration(jiff::SpanRelativeTo::days_are_24_hours()).map_err(|e| format!("{e}"))?;
    d.try_into().map_err(|e| format!("{e}"))
}

pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if let Command::Tui { url } = &cli.command {
        // No log subscriber: log lines would corrupt the screen.
        return tokio::runtime::Runtime::new()?.block_on(wirewolf_tui::run(url));
    }
    if let Command::Check { url, json } = &cli.command {
        let code = tokio::runtime::Runtime::new()?.block_on(check::run(url, *json));
        std::process::exit(code);
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    tokio::runtime::Runtime::new()?.block_on(async {
        match cli.command {
            Command::Serve { listen, demo: true, scenario, .. } => {
                let hub = Hub::new(demo::meta(&scenario));
                tokio::spawn(demo::run(hub.clone(), scenario));
                wirewolf_web::serve(hub, listen).await?;
            }
            Command::Serve { listen, demo: false, live, .. } => {
                let config = wirewolf_probe::Config {
                    interval: live.interval.max(std::time::Duration::from_secs(5)),
                    speed_interval: (!live.speed_interval.is_zero()).then_some(live.speed_interval),
                    speed_backend: live.speed_backend,
                    db: Some(live.db),
                    retention: live.retention.try_into()?,
                    ..Default::default()
                };
                let hub = Hub::new(wirewolf_probe::meta());
                wirewolf_probe::attach_store(&hub, &config)?;
                tokio::spawn(wirewolf_probe::run(hub.clone(), config));
                wirewolf_web::serve(hub, listen).await?;
            }
            Command::Tui { .. } | Command::Check { .. } => unreachable!("handled above"),
        }
        Ok(())
    })
}
