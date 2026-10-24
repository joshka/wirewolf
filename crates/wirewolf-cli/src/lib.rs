//! Command line interface for wirewolf.

mod check;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use clap::parser::ValueSource;
use clap::{ArgMatches, CommandFactory, FromArgMatches, Parser, Subcommand};
use wirewolf_core::config::{self, Loaded, Override, Source};
use wirewolf_core::editor::Editor;
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
        /// Address to listen on [default: 0.0.0.0:7465].
        #[arg(long, env = "WIREWOLF_LISTEN")]
        listen: Option<SocketAddr>,
        /// Serve scripted demo data instead of probing the network.
        #[arg(long, env = "WIREWOLF_DEMO")]
        demo: bool,
        /// Demo scenario to start with.
        #[arg(long, default_value = "healthy", requires = "demo")]
        scenario: String,
        #[command(flatten)]
        config: ConfigArg,
        #[command(flatten)]
        live: LiveArgs,
        /// Show settings without letting the dashboard change them.
        #[arg(long, env = "WIREWOLF_READONLY_SETTINGS")]
        read_only_settings: bool,
    },
    /// Open the terminal dashboard for a running wirewolf server.
    Tui {
        /// The wirewolf server to connect to.
        #[arg(long, env = "WIREWOLF_URL", default_value = DEFAULT_URL)]
        url: String,
    },
    /// Diagnose the network and exit 0 healthy, 1 degraded, 2 down, 3 unreachable, 4 unknown.
    ///
    /// Runs one probe cycle on this machine, or asks a wirewolf server with `--url`.
    Check {
        /// Ask this wirewolf server instead of probing from here.
        #[arg(long, env = "WIREWOLF_URL")]
        url: Option<String>,
        /// Print the snapshot as JSON instead of text.
        #[arg(long)]
        json: bool,
        #[command(flatten)]
        config: ConfigArg,
    },
    /// Print or check the configuration.
    #[command(subcommand)]
    Config(ConfigCommand),
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Print the default config, fully commented. Start a `wirewolf.toml` from it.
    Default,
    /// Load the config, report errors, and print the effective settings as TOML.
    Show {
        #[command(flatten)]
        config: ConfigArg,
    },
}

#[derive(Debug, clap::Args)]
struct ConfigArg {
    /// Config file. Without it, `wirewolf.toml` in the current directory is used if it exists.
    #[arg(long = "config", env = "WIREWOLF_CONFIG")]
    path: Option<PathBuf>,
}

const DEFAULT_URL: &str = "http://127.0.0.1:7465";

/// Settings for live probing (ignored with `--demo`). Each overrides the config file.
#[derive(Debug, clap::Args)]
struct LiveArgs {
    /// Time between probe cycles, for example `30s`.
    #[arg(long, env = "WIREWOLF_INTERVAL")]
    interval: Option<String>,
    /// Time between scheduled speed tests, for example `60m`. `0` disables them.
    #[arg(long, env = "WIREWOLF_SPEED_INTERVAL")]
    speed_interval: Option<String>,
    /// Speed test backend: `cloudflare` or `ookla` (speedtest.net servers). Both are built in.
    #[arg(long, env = "WIREWOLF_SPEED_BACKEND")]
    speed_backend: Option<String>,
    /// SQLite history file.
    #[arg(long, env = "WIREWOLF_DB")]
    db: Option<String>,
    /// How long to keep history, for example `30d`.
    #[arg(long, env = "WIREWOLF_RETENTION")]
    retention: Option<String>,
}

/// Loads the config: defaults, then the file, then env vars and flags.
///
/// `sub` is the subcommand's matches, which say whether each value came from the environment or
/// a flag.
fn load_config(arg: &ConfigArg, sub: Option<&ArgMatches>, overrides: &[(&'static str, &str, Option<String>)]) -> anyhow::Result<Loaded> {
    load_config_for_edit(arg, sub, overrides).map(|(loaded, _, _)| loaded)
}

/// [`load_config`], plus the file path and overrides an [`Editor`] needs to save changes.
fn load_config_for_edit(arg: &ConfigArg, sub: Option<&ArgMatches>, overrides: &[(&'static str, &str, Option<String>)]) -> anyhow::Result<(Loaded, PathBuf, Vec<Override>)> {
    let from_flag = sub.and_then(|m| m.value_source("path")) == Some(ValueSource::CommandLine);
    let (path, required) = match &arg.path {
        Some(p) => (p.clone(), from_flag),
        None => (PathBuf::from("wirewolf.toml"), false),
    };
    let overrides: Vec<Override> = overrides
        .iter()
        .filter_map(|(key, id, value)| {
            let source = match sub.and_then(|m| m.value_source(id)) {
                Some(ValueSource::CommandLine) => Source::Flag,
                _ => Source::Env,
            };
            value.clone().map(|value| Override { key, value, source })
        })
        .collect();
    let loaded = config::load(Some(&path), required, &overrides).map_err(anyhow::Error::msg)?;
    wirewolf_probe::validate(&loaded.config)?;
    Ok((loaded, path, overrides))
}

fn live_overrides(listen: Option<SocketAddr>, live: &LiveArgs) -> Vec<(&'static str, &'static str, Option<String>)> {
    vec![
        ("server.listen", "listen", listen.map(|l| l.to_string())),
        ("probe.interval", "interval", live.interval.clone()),
        ("speed.interval", "speed_interval", live.speed_interval.clone()),
        ("speed.backend", "speed_backend", live.speed_backend.clone()),
        ("storage.db", "db", live.db.clone()),
        ("storage.retention", "retention", live.retention.clone()),
    ]
}

pub fn run() -> anyhow::Result<()> {
    let matches = Cli::command().get_matches();
    let cli = Cli::from_arg_matches(&matches)?;
    let sub = matches.subcommand().map(|(_, m)| m);
    match &cli.command {
        Command::Tui { url } => {
            // No log subscriber: log lines would corrupt the screen.
            return tokio::runtime::Runtime::new()?.block_on(wirewolf_tui::run(url));
        }
        Command::Check { url, json, config } => {
            let loaded = if url.is_none() { Some(load_config(config, sub, &[])?) } else { None };
            let code = tokio::runtime::Runtime::new()?.block_on(check::run(url.as_deref(), *json, loaded.map(|l| l.config)));
            std::process::exit(code);
        }
        Command::Config(ConfigCommand::Default) => {
            print!("{}", config::DEFAULT_TOML);
            return Ok(());
        }
        Command::Config(ConfigCommand::Show { config }) => {
            let sub = sub.and_then(|m| m.subcommand()).map(|(_, m)| m);
            let loaded = load_config(config, sub, &[])?;
            match &loaded.path {
                Some(p) => println!("# from {} (non-default: {})", p.display(), loaded.sources.keys().cloned().collect::<Vec<_>>().join(", ")),
                None => println!("# no config file; all defaults"),
            }
            print!("{}", toml::to_string(&loaded.config.redacted())?);
            return Ok(());
        }
        Command::Serve { .. } => {}
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let Command::Serve { listen, demo, scenario, config, live, read_only_settings } = cli.command else { unreachable!("handled above") };
    tokio::runtime::Runtime::new()?.block_on(async {
        if demo {
            let addr = listen.unwrap_or_else(|| ([0, 0, 0, 0], 7465).into());
            let hub = Hub::new(demo::meta(&scenario));
            tokio::spawn(demo::run(hub.clone(), scenario));
            wirewolf_web::serve(hub, addr).await?;
            return Ok(());
        }
        let (loaded, path, overrides) = load_config_for_edit(&config, sub, &live_overrides(listen, &live))?;
        match &loaded.path {
            Some(p) => tracing::info!("config from {} ({} settings)", p.display(), loaded.sources.len()),
            None => tracing::info!("no config file; using defaults (`wirewolf config default` prints them)"),
        }
        let addr: SocketAddr = loaded.config.server.listen.parse().map_err(|e| anyhow::anyhow!("server.listen {:?}: {e}", loaded.config.server.listen))?;
        let hub = Hub::new(wirewolf_probe::meta());
        let editor = Arc::new(Editor::new(path, overrides, wirewolf_probe::field_errors, read_only_settings));
        hub.set_config(editor.view(&loaded));
        hub.set_editor(editor);
        let config = Arc::new(loaded.config);
        wirewolf_probe::attach_store(&hub, &config)?;
        tokio::spawn(wirewolf_probe::run(hub.clone(), config));
        wirewolf_web::serve(hub, addr).await?;
        Ok(())
    })
}
