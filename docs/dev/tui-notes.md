# TUI notes (`wirewolf tui`, `wirewolf check`)

`wirewolf tui [--url http://host:7465]` is a ratatui client of the HTTP API. It loads state
over REST, then follows `/api/v1/events` (SSE) and reconnects with backoff. It never links the
hub in-process, so it works against any wirewolf on the LAN. `wirewolf check [--json] [--url …]`
prints the verdict and path, then exits 0 healthy, 1 degraded, 2 down, 3 can't reach the server,
or 4 unknown. Without `--url` (or `$WIREWOLF_URL`) it runs one probe cycle in-process, with no
server and no history, in about 3s. With `--url` it asks a running server. `wirewolf tui`
defaults to `http://127.0.0.1:7465`.

## Captures

The `*.txt` files are plain text and the `*.html` files are color captures that look like the
terminal did. They were rendered from real app state against `serve --demo` with
`cargo run -p wirewolf-tui --example capture -- http://127.0.0.1:7475 docs/compare/tui`. The
example also renders every view and popup at about 1,400 terminal sizes (10×3 up to 160×50) as a
no-panic sweep.

- Now: `now-<scenario>` for all seven scenarios at 140×42, plus `now-{healthy,dns_down,wifi_loss}-narrow`
  at 80 columns, where the path chain turns vertical.
- `hop-detail-dns`, `history-1h`, `history-24h`, `incidents`, `incident-detail`, `speed-idle`,
  `speed-running`, `now-speed-running`, `scenarios`, and `help`.

## Views and keys

| Key | Action |
|---|---|
| `1`–`4`, tab | Now, History, Incidents, Speed |
| `←→`, `enter` | select a hop, then open its checks, evidence, 1h RTT chart, and status strip |
| `t`, `space` | History: cycle the range (1h/6h/24h), show or hide a hop |
| `c` | Incidents: copy the plain-text ISP report (OSC 52, so it works over SSH) |
| `r` | run a speed test with live progress |
| `s` | demo scenario switcher, only when `meta.mode == "demo"` |
| `i` | Now: jump to the open incident |

## Notes for the scorecard

- **Easy:** the API contract. One `reqwest` client and a 30-line SSE parser cover everything.
  ratatui 0.30's `Layout::areas`, `Rect::centered`, `Paragraph::line_count` (for a verdict
  banner that sizes itself to its wrapped text), per-bar `SparklineBar` styles, and
  `tui-big-text` made the views quick to build.
- **Fought:** `Chart` can't shade background bands. Incident periods are tinted after render by
  finding the plot area from the axis corner. Mixed-resolution history (1/min backfill, 2s live)
  makes the 15-minute sparklines sparse right after startup.
- **Build:** pure Rust, no extra toolchain. Deps: ratatui 0.30, tui-big-text, reqwest (plain
  HTTP, no TLS), tokio, jiff, and serde_json.
- **Lines of code:** about 1,900 in `crates/wirewolf-tui` including the capture example, and
  about 100 for `check`.
- **Maintenance:** one language. ratatui and reqwest each ship a breaking minor about once a
  year. `unstable-rendered-line-info` is an unstable ratatui feature, used for one line-count
  call.
- **Gaps:** no mouse support. No HTTPS (enable reqwest's rustls feature if it's needed). There
  is no light theme; the TUI draws on the terminal's own background. The in-process `check` skips
  the TTL sweep the live driver runs after an upstream fault, so it can't name ISP path hops.
