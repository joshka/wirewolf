//! Keeps `docs/cli.md` in step with `--help`.
//!
//! The reference is generated from the clap definitions. When a flag or help text changes, this
//! test fails; regenerate the page with `WIREWOLF_BLESS=1 cargo test -p wirewolf-cli --test
//! cli_reference`.

use std::path::Path;

use clap::CommandFactory;
use wirewolf_cli::Cli;

const HEADER: &str = "\
# wirewolf command line reference

<!-- Generated from `--help` by crates/wirewolf-cli/tests/cli_reference.rs. Don't edit by hand:
     run `WIREWOLF_BLESS=1 cargo test -p wirewolf-cli --test cli_reference`. -->

Every flag that names an environment variable can be set with it instead. Flags and env vars
override `wirewolf.toml`; see [the guide](guide.md#configuration) for how the config is loaded.
";

#[test]
fn cli_reference_matches_help() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/cli.md");
    let expected = render();
    if std::env::var_os("WIREWOLF_BLESS").is_some() {
        std::fs::write(&path, &expected).unwrap();
        return;
    }
    let Ok(actual) = std::fs::read_to_string(&path) else {
        // Outside the workspace (a packaged crate) there's no docs directory to check.
        return;
    };
    assert!(
        actual == expected,
        "docs/cli.md is out of date with `--help`. Regenerate it with \
         `WIREWOLF_BLESS=1 cargo test -p wirewolf-cli --test cli_reference`."
    );
}

fn render() -> String {
    let mut out = String::from(HEADER);
    let mut root = Cli::command().term_width(100);
    root.build();
    section(&mut out, &mut root, "wirewolf");
    out
}

fn section(out: &mut String, cmd: &mut clap::Command, path: &str) {
    let help = cmd.render_long_help().to_string();
    out.push_str(&format!("\n## `{path}`\n\n```text\n{}\n```\n", clean(&help)));
    for sub in cmd.get_subcommands_mut().filter(|s| s.get_name() != "help") {
        let name = format!("{path} {}", sub.get_name());
        section(out, sub, &name);
    }
}

/// Drops values clap shows for env vars that happen to be set, and trailing spaces.
fn clean(help: &str) -> String {
    let lines = help.trim_end().lines().map(|line| {
        let mut line = line.trim_end().to_string();
        if let Some(start) = line.find("[env: ")
            && let Some(eq) = line[start..].find('=')
            && let Some(end) = line[start..].find(']')
        {
            line.replace_range(start + eq..start + end, "");
        }
        line
    });
    lines.collect::<Vec<_>>().join("\n")
}
