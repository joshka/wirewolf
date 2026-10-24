# wirewolf v1 design (draft for review)

wirewolf is a network diagnostics service for your own network. Run it in a container on a
home server, NAS, or laptop, open it in a browser, and it tells you what is wrong with the
network: where the path breaks, how bad it is, and since when. It shows that in a modern
dashboard.

## Prior art

| Project | Stack | What it does | Gap for us |
|---|---|---|---|
| [kanywst/wtfi2](https://github.com/kanywst/wtfi2) | Rust, ratatui, tokio, hickory, reqwest | Diagnoses the hop chain You → Wi-Fi → Gateway → Uplink → WAN → DNS → Portal and reports the first break with a confidence level. It has a live `-w` dashboard, local history, `--json`, and exit codes. | Runs in a local terminal only. It is Wi-Fi/laptop-centric (CoreWLAN), has no server or web UI, and has no throughput test. |
| [mplaczek99/network-doctor](https://pkg.go.dev/github.com/mplaczek99/network-doctor) | Go TUI | A probe dependency graph (Interface → Egress/Proxy → DNS → TCP → TLS → HTTP) with Pass/Warn/Fail/Skip/N/A states, a verdict banner with a suggested fix, and drill-down tools such as mtr, dig, and curl. | Diagnoses one target from a local terminal. It has no history and no throughput test. |
| Speedtest Tracker | PHP/Laravel + Ookla CLI | Scheduled speed tests with history charts. | Measures speed but does no diagnosis. Depends on Ookla, and the stack doesn't match our preferences. |

Nothing is a 95% solution. wtfi2 is the closest in taste (Rust/ratatui) and in thinking, but
it answers "why is *my laptop* offline right now". wirewolf answers "what has *the network*
been doing, viewed from an always-on vantage point". We borrow its ideas freely:

- From wtfi2: blame the **first break** and treat downstream failures as collateral. Degraded
  is different from broken (≥25% loss counts as degraded). Each verdict states its method and
  confidence. Probes use TCP handshakes rather than ICMP where possible, so no root is needed.
  The TTL sweep runs only when an upstream fault exists. DNS checks compare the system resolver
  with public resolvers.
- From network-doctor: build checks as a dependency graph with independent branches. Use the
  five states Pass/Warn/Fail/Skip/N/A. The verdict banner includes a suggested fix. Probes are
  "evidence" and drill-down tools are "proof". Stable JSON schema and exit codes. Sanitize
  remote text.

## v1 scope

**Checks** run on a schedule, every 30s for the cheap ones and every 30–60 min for throughput,
and can also be triggered on demand:

1. **Host / LAN**: interface, IP, default gateway, MTU, and whether the address is behind CGNAT.
2. **Gateway**: RTT, loss, and jitter over a short burst (ICMP if `CAP_NET_RAW` is available,
   otherwise skipped with a note).
3. **WAN reachability**: TCP :443 handshakes to Cloudflare, Google, and Quad9 over v4 and v6,
   with loss and jitter.
4. **DNS**: system resolver compared with 1.1.1.1 and 8.8.8.8: correctness, latency, failures.
5. **HTTP/TLS**: a plain HTTP captive/interception check and a TLS handshake to a known host.
6. **Path** (on fault or on demand): a TTL sweep / mtr-style hop list.
7. **Throughput**: download/upload against Cloudflare's speed endpoints, with no Ookla
   dependency. Also loaded latency, i.e. a bufferbloat grade.
8. **User targets** (optional): a config list of hosts/URLs, such as the NAS or work VPN, run
   through the same chain.

**Diagnosis**: the walk over the chain produces one verdict, for example "Healthy",
"Degraded: packet loss starts at the gateway (Wi-Fi/LAN side)", or "Down: DNS is failing; the
internet is reachable by IP". Each verdict carries a confidence level and a fix hint.

**History**: SQLite (embedded, single file in a volume) with configurable retention. History
also feeds an **incident timeline**: when a verdict changes, wirewolf opens or closes an
incident ("Down 14:02–14:07, DNS").

**Out of v1**: Prometheus `/metrics` (cheap to add later, so keep the metric names in mind),
Grafana, alerts/notifications, multi-agent fleets, Wi-Fi radio telemetry (meaningless in a
container).

## Architecture

```
wirewolf (single static binary, distroless/alpine image)
├─ probes/      async probe impls, no UI deps (tokio, hickory-resolver, reqwest+rustls,
│               surge-ping or socket2 for ICMP when permitted)
├─ engine/      dependency graph, scheduler, diagnosis → Verdict
├─ store/       SQLite (rusqlite or sqlx), retention
├─ api/         axum: REST + SSE (live probe stream), JSON schema shared with UI
└─ ui/          static SPA embedded in the binary (rust-embed)
```

- The container runs with `--network host` by default so it sees the real LAN and gateway.
  It still works in bridge mode, with a note in the UI that gateway results reflect the docker
  bridge.
- Config comes from env vars and an optional TOML file. No auth in v1 because it is meant for
  the LAN, though a basic-auth/token option is easy to add.
- A `wirewolf check` CLI subcommand runs the same engine headless with `--json` and exit codes.
  This is useful on a laptop without the UI.

## UI

Views:

1. **Now**: a large verdict banner, then the path chain drawn as a topology
   (wirewolf → gateway → ISP → internet → DNS) with per-hop status, RTT, and loss. Below it,
   live sparklines (SSE). Clicking a hop opens a detail drawer with the raw evidence and
   "run deeper" actions (mtr, dig, speed test).
2. **History**: latency/loss/DNS time series with zoom, throughput runs, and incident bands
   overlaid.
3. **Incidents**: a timeline list. Each incident shows the verdict, duration, and an evidence
   snapshot that can be copied as a report for an ISP support call.
4. **Speed**: the latest and historical down/up/loaded-latency results, the bufferbloat grade,
   and a "run now" button.
5. **Settings**: targets, intervals, and retention (read-only from config in v1 is fine).

UI stack options (decision needed):

- **A. TypeScript SPA** (Svelte or React with Vite, Tailwind/shadcn, uPlot/ECharts), built and
  embedded in the binary. Best-looking, most "modern" results with the richest chart
  ecosystem. Costs a Node build step.
- **B. Rust-only** (Leptos/Dioxus WASM). One language, but a thinner component and chart
  ecosystem.
- **C. Server-rendered HTMX + a small chart lib**. Simplest build, but less slick for live
  interactive views.

Recommendation: **A with Svelte + uPlot**. It gives the best UX per unit of effort, and node
is among the maintainer's preferred stacks.

## Open questions for the maintainer

1. Should the UI stack be A, B, or C?
2. Should we also ship a ratatui TUI client against the same API? This would be a v1.x, not v1.
3. Is the throughput schedule acceptable? Cloudflare speed tests use real data, roughly
   100–500 MB per run.
4. Where should the image be published? The GitHub user/org and the Docker Hub namespace
   `wirewolf` are both taken. The proposal is `ghcr.io/joshka/wirewolf`. The crates.io name
   is free.

## Decisions (2026-10-23)

- The UI is chosen by building three variants against the demo API and comparing them:
  shadcn (React + Vite, Node at build time only), HTMX + Basecoat (no Node), and Topcoat
  (tokio-rs server-rendered framework and UI crate). No Leptos or Dioxus. The scorecard
  includes maintenance burden.
- A ratatui TUI ships as an API client.
- Throughput: Cloudflare is the default. Ookla is an optional backend. Intervals get tuned
  over time.
- Publishing: `ghcr.io/joshka/wirewolf`. The crates are `wirewolf` (facade + binary),
  `wirewolf-core`, `-probe`, `-web`, `-tui`, `-cli`. The maintainer publishes them.
- Work plan: see `docs/workstreams.md`.
