//! `wirewolf check`: the current verdict for scripts and humans.
//!
//! Without `--url` it runs one probe cycle in-process (no server, no history), which is the
//! "is it me or the internet?" laptop case. With `--url` it asks a running server.

use std::io::IsTerminal;

use jiff::Timestamp;
use std::sync::Arc;

use wirewolf_core::config::Config;
use wirewolf_core::diagnose::{diagnose, with_targets};
use wirewolf_core::model::{Severity, Snapshot};
use wirewolf_probe::{assess, measure::Prober};
use wirewolf_tui::{Client, format, report};

const UNREACHABLE: i32 = 3;

/// Prints the verdict and returns the process exit code.
pub async fn run(url: Option<&str>, json: bool, config: Option<Config>) -> i32 {
    let Some(url) = url else {
        return match probe(config.unwrap_or_default()).await {
            Ok(snapshot) => print(&snapshot, json),
            Err(error) => {
                eprintln!("wirewolf check: can't run probes: {error:#}");
                exit_code(Severity::Unknown)
            }
        };
    };
    match fetch(url).await {
        Ok(Some(snapshot)) => print(&snapshot, json),
        Ok(None) => {
            if json {
                println!("null");
            } else {
                println!("UNKNOWN: wirewolf at {url} has not measured anything yet");
            }
            exit_code(Severity::Unknown)
        }
        Err(error) => {
            eprintln!("wirewolf check: can't reach {url}: {error:#}");
            UNREACHABLE
        }
    }
}

fn print(snapshot: &Snapshot, json: bool) -> i32 {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(snapshot).unwrap_or_default()
        );
    } else {
        print_human(snapshot);
    }
    exit_code(snapshot.verdict.severity)
}

/// One probe cycle, assessed and diagnosed the way the live driver does it.
///
/// This skips the TTL sweep that the live driver runs after an upstream fault. A sweep is slow,
/// and the verdict doesn't depend on it.
async fn probe(config: Config) -> anyhow::Result<Snapshot> {
    let (measurements, targets) = Prober::new(Arc::new(config))?.cycle(false).await;
    let at = Timestamp::now();
    let hops = assess::hops(&measurements);
    let verdict = with_targets(diagnose(&hops), &targets);
    Ok(Snapshot {
        at,
        verdict,
        hops,
        open_incident: None,
        targets,
    })
}

async fn fetch(url: &str) -> anyhow::Result<Option<Snapshot>> {
    Client::new(url)?.now().await
}

pub fn exit_code(severity: Severity) -> i32 {
    match severity {
        Severity::Healthy => 0,
        Severity::Degraded => 1,
        Severity::Down => 2,
        Severity::Unknown => 4,
    }
}

fn print_human(snapshot: &Snapshot) {
    let text = report::snapshot(snapshot);
    if !std::io::stdout().is_terminal() || std::env::var_os("NO_COLOR").is_some() {
        print!("{text}");
        return;
    }
    // Color the verdict line and the status tags; everything else stays plain.
    let color = match snapshot.verdict.severity {
        Severity::Healthy => "32",
        Severity::Degraded => "33",
        Severity::Down => "31",
        Severity::Unknown => "90",
    };
    let mut lines = text.lines();
    if let Some(first) = lines.next() {
        println!(
            "\x1b[1;{color}m{} {first}\x1b[0m",
            format::severity_icon(snapshot.verdict.severity)
        );
    }
    for line in lines {
        let colored = line
            .replace("[PASS]", "[\x1b[32mPASS\x1b[0m]")
            .replace("[WARN]", "[\x1b[33mWARN\x1b[0m]")
            .replace("[FAIL]", "[\x1b[31mFAIL\x1b[0m]")
            .replace("[SKIP]", "[\x1b[90mSKIP\x1b[0m]")
            .replace(
                "<-- problem here",
                &format!("\x1b[1;{color}m<-- problem here\x1b[0m"),
            );
        println!("{colored}");
    }
}
