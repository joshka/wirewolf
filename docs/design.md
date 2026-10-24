# Why wirewolf works the way it does

wirewolf answers one question from an always-on machine on your network: **is it broken, where,
and since when?** This page explains the ideas behind it and how the code is laid out. For using
it, see the [guide](guide.md).

## Prior art

| Project | What it does | What wirewolf wanted that it doesn't do |
|---|---|---|
| [kanywst/wtfi2](https://github.com/kanywst/wtfi2) (Rust, ratatui) | Walks the hop chain You → Wi-Fi → Gateway → Uplink → WAN → DNS → Portal and reports the first break with a confidence level. Live terminal view, local history, `--json`, exit codes | It runs in a local terminal, centres on a laptop's Wi-Fi, and has no server, web UI, or throughput test |
| [mplaczek99/network-doctor](https://pkg.go.dev/github.com/mplaczek99/network-doctor) (Go TUI) | A probe dependency graph (interface → egress → DNS → TCP → TLS → HTTP) with pass/warn/fail/skip/n/a states, a verdict banner with a suggested fix, and drill-down tools | It diagnoses one target from a local terminal, with no history and no throughput test |
| Speedtest Tracker (PHP, Ookla CLI) | Scheduled speed tests with history charts | It measures speed but doesn't diagnose anything |

wtfi2 is closest in spirit, but it answers "why is *my laptop* offline right now?". wirewolf
answers "what has *the network* been doing?", from a vantage point that is always on.

## Principles

- **Blame the first break.** Everything after a failed hop can't be tested meaningfully, so it's
  shown as a collateral skip, not as more failures. With no hard failure, the first degraded hop
  is blamed. (From wtfi2.)
- **Degraded isn't broken.** Slow or lossy is its own severity, and never counts as a failure.
- **Show the evidence.** Every verdict says how it was reached (checks, method) and how
  confident it is, and every hop keeps the raw evidence behind its status.
- **Five states and a fix.** Checks are pass, warn, fail, skip, or not applicable, and the
  verdict comes with a suggested next step. (From network-doctor.)
- **Compare to tell causes apart.** DNS compares your resolvers with public ones, so "my resolver
  is broken" and "DNS is blocked" read differently. IPv4 and IPv6 are tested side by side.
- **Avoid needing root.** The internet hop uses TCP handshakes, not ICMP. ICMP is used where the
  host allows it, and the ISP hop is inferred when it can't be measured. The slow TTL sweep only
  runs when something upstream is already wrong.
- **History, not just now.** Incidents open and close on their own, keep their evidence, and
  produce a report you can send your ISP. A short blip isn't an incident.
- **Works when the internet doesn't.** The dashboard is embedded in the binary and loads nothing
  from a CDN.
- **Safe on a LAN by default.** Reads are open, writes need sign-in or an explicit opt-in, and a
  host-name check stops DNS rebinding.

## Layout

One binary, `wirewolf`, built from a Rust workspace:

| Crate | Role |
|---|---|
| `wirewolf-core` | The API types, the diagnosis engine, config loading and editing, the AI brief, the state hub, and the demo driver |
| `wirewolf-probe` | The live probes, speed tests, and SQLite history. Publishes snapshots into the hub |
| `wirewolf-web` | The HTTP API and server-sent events, sign-in, and the embedded React dashboard |
| `wirewolf-tui` | The terminal dashboard, a client of the HTTP API |
| `wirewolf-cli` | The command line: `serve`, `check`, `tui`, `config` |
| `wirewolf` | The binary |

The probe driver and the demo driver publish the same shapes into the hub, so the dashboard, the
TUI, and `wirewolf check --url` work identically against either. Everything a client sees goes
through the API in [api.md](api.md).

## Not built (yet)

Prometheus metrics, alerts and notifications, monitoring several networks from one server, and
Wi-Fi radio telemetry, which a container can't see.
