# wirewolf command line reference

<!-- Generated from `--help` by crates/wirewolf-cli/tests/cli_reference.rs. Don't edit by hand:
     run `WIREWOLF_BLESS=1 cargo test -p wirewolf-cli --test cli_reference`. -->

Every flag that names an environment variable can be set with it instead. Flags and env vars
override `wirewolf.toml`; see [the guide](guide.md#configuration) for how the config is loaded.

## `wirewolf`

```text
See what's wrong with your network

Usage: wirewolf <COMMAND>

Commands:
  serve   Run the probes and serve the dashboard and API
  tui     Open the terminal dashboard for a running wirewolf server
  check   Diagnose the network and exit 0 healthy, 1 degraded, 2 down, 3 unreachable, 4 unknown
  config  Print or check the configuration
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help

  -V, --version
          Print version
```

## `wirewolf serve`

```text
Run the probes and serve the dashboard and API

Usage: wirewolf serve [OPTIONS]

Options:
      --listen <LISTEN>
          Address to listen on [default: 0.0.0.0:7465]

          [env: WIREWOLF_LISTEN]

      --demo
          Serve scripted demo data instead of probing the network

          [env: WIREWOLF_DEMO]

      --scenario <SCENARIO>
          Demo scenario to start with

          [default: healthy]

      --config <PATH>
          Config file. Without it, `wirewolf.toml` in the current directory is used if it exists

          [env: WIREWOLF_CONFIG]

      --interval <INTERVAL>
          Time between probe cycles, for example `30s`

          [env: WIREWOLF_INTERVAL]

      --speed-interval <SPEED_INTERVAL>
          Time between scheduled speed tests, for example `60m`. `0` disables them

          [env: WIREWOLF_SPEED_INTERVAL]

      --speed-backend <SPEED_BACKEND>
          Speed test backend: `cloudflare` or `ookla` (speedtest.net servers). Both are built in

          [env: WIREWOLF_SPEED_BACKEND]

      --db <DB>
          SQLite history file

          [env: WIREWOLF_DB]

      --retention <RETENTION>
          How long to keep history, for example `30d`

          [env: WIREWOLF_RETENTION]

      --read-only-settings
          Show settings without letting the dashboard change them

          [env: WIREWOLF_READONLY_SETTINGS]

  -h, --help
          Print help
```

## `wirewolf tui`

```text
Open the terminal dashboard for a running wirewolf server

Usage: wirewolf tui [OPTIONS]

Options:
      --url <URL>
          The wirewolf server to connect to

          [env: WIREWOLF_URL]
          [default: http://127.0.0.1:7465]

  -h, --help
          Print help
```

## `wirewolf check`

```text
Diagnose the network and exit 0 healthy, 1 degraded, 2 down, 3 unreachable, 4 unknown.

Runs one probe cycle on this machine, or asks a wirewolf server with `--url`.

Usage: wirewolf check [OPTIONS]

Options:
      --url <URL>
          Ask this wirewolf server instead of probing from here

          [env: WIREWOLF_URL]

      --json
          Print the snapshot as JSON instead of text

      --brief
          Print a compact summary to paste into an AI assistant. Public IP addresses are replaced with placeholders unless `--no-redact`

      --no-redact
          With `--brief`, keep public IP addresses. With `--url`, the server allows this only for clients that may change settings

      --config <PATH>
          Config file. Without it, `wirewolf.toml` in the current directory is used if it exists

          [env: WIREWOLF_CONFIG]

  -h, --help
          Print help (see a summary with '-h')
```

## `wirewolf config`

```text
Print or check the configuration

Usage: wirewolf config <COMMAND>

Commands:
  default  Print the default config, fully commented. Start a `wirewolf.toml` from it
  show     Load the config, report errors, and print the effective settings as TOML
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

## `wirewolf config default`

```text
Print the default config, fully commented. Start a `wirewolf.toml` from it

Usage: wirewolf config default

Options:
  -h, --help
          Print help
```

## `wirewolf config show`

```text
Load the config, report errors, and print the effective settings as TOML

Usage: wirewolf config show [OPTIONS]

Options:
      --config <PATH>
          Config file. Without it, `wirewolf.toml` in the current directory is used if it exists

          [env: WIREWOLF_CONFIG]

  -h, --help
          Print help
```
