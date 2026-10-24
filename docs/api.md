# wirewolf API v1

All UIs and the TUI consume this API. The Rust types in `crates/wirewolf-core/src/model.rs`
are the source of truth, and this page summarizes them. Changing the contract means editing
`model.rs` and this page together. Additive changes (new optional fields, new endpoints) are
fine; breaking changes need the maintainer's agreement first.

Run the demo server with `cargo run -- serve --demo [--scenario dns_down]`. It listens on
`http://127.0.0.1:7465` by default (`--listen` or `WIREWOLF_LISTEN` to change). The demo
backfills 24h of history with three past incidents and an evening of bufferbloat, then
publishes a live snapshot every 2s.

## Conventions

- JSON with `snake_case` fields. Enums are lowercase strings, except the bloat grades
  (`"A+"`, `"A"` … `"F"`).
- Timestamps are RFC 3339 UTC strings.
- Optional values are `null` and the field is always present.
- Numbers: milliseconds (`*_ms`), percent 0–100 (`loss_pct`), megabits per second (`*_mbps`).

## Endpoints

| Method | Path | Returns |
|---|---|---|
| GET | `/api/v1/meta` | `Meta`: version, `mode` (`demo`/`live`), host, scenarios, active scenario |
| GET | `/api/v1/config` | `ConfigView`: the effective config and where each value came from (1.1). `404` if the server didn't publish one |
| PUT | `/api/v1/config` | Body: a whole `ConfigView.config`. Saves it to the config file and applies it from the next probe cycle (1.2). `200` with the new `ConfigView`, `422` `{ errors: [FieldError] }`, `403` with `--read-only-settings` (or when the auth layer refuses the write), `404` in demo mode |
| GET | `/api/v1/now` | `Snapshot`, or `204` before the first measurement |
| GET | `/api/v1/history?hop=&since=&until=` | `[Sample]` in time order. All params are optional. `hop` is a hop id, `since`/`until` are RFC 3339 |
| GET | `/api/v1/speed` | `[SpeedRun]`, newest first |
| POST | `/api/v1/speed/run` | `202` when started, `409` if one is already running. Progress arrives as `speed` events |
| GET | `/api/v1/incidents` | `[Incident]`, newest first |
| GET | `/api/v1/incidents/{id}` | `Incident` or `404` |
| GET | `/api/v1/brief?format=&redact=&window=&incident=` | The brief: a compact summary for an AI assistant (1.5). See below |
| GET | `/api/v1/brief/prompt?redact=&window=&incident=` | `text/plain`: a request to explain the problem, then the brief in `<brief>` tags (1.5) |
| GET | `/api/v1/events` | SSE stream (see below) |
| GET | `/api/v1/activity?kind=&limit=` | `[Activity]`, newest first, at most 1000 (1.6). `kind` keeps one kind |
| GET | `/api/v1/logs?after=&level=&target=&q=&limit=` | `{ filter, capacity, lines: [LogLine] }`: recent server log lines, oldest first (1.6). Needs a user who can write (`403`). See below |
| GET | `/api/v1/logs/events?level=&target=&q=` | SSE: each new `LogLine` as a `log` event, same filters (1.6). Needs a user who can write |
| PUT | `/api/v1/logs/filter` | Body `{"filter":"info,wirewolf_probe=debug"}`. Changes the log filter now and saves it as `log.filter` when it can (1.6). `200` `{ filter, saved }`, `422` `{ errors: [FieldError] }` |
| POST | `/api/v1/demo/scenario` | Body `{"id":"dns_down"}`. Returns `202`, `422` for an unknown id, or `404` in live mode |
| GET | `/api/v1/session` | `Session`: who is signed in and what they may do (1.3). Always readable |

## Sign-in and writes (1.3)

`[auth]` in `wirewolf.toml` configures sign-in: OIDC (`/auth/login` → issuer →
`/auth/callback`, kept in an HttpOnly cookie), and/or a trusted header such as `Remote-User`
set by a proxy listed in `auth.trusted_header.proxies`.

- **Reads** (every `GET`, including SSE) are open, or need a user with
  `auth.require_for_reads` (`401` otherwise). Viewers who can't write get `GET /config` with
  `config.auth` at its defaults and no `save_path`, and `403` for `/brief?redact=false`.
  At most 64 `/events` streams are open at once; more get `503` with `Retry-After`.
- **Writes** (every other method) need a user when a sign-in method is configured (`401`).
  With none configured they're refused (`403`) unless `auth.allow_unauthenticated_writes`;
  `serve --demo` allows them.
- Writes from an OIDC session must send `X-Wirewolf-CSRF: <Session.csrf>` (`403` otherwise).
  Writes the browser marks cross-site (`Sec-Fetch-Site`, or an `Origin` that isn't the host)
  are refused.
- `POST /auth/logout` with the CSRF header ends an OIDC session (`204`).
- Every route, the dashboard included, answers `421` when the `Host` header is a public DNS
  name the server wasn't told about. IP addresses, `localhost`, single-label names, local
  suffixes (`.local`, `.lan`, `.home.arpa`, `.internal`, `.ts.net`), the `auth.redirect_url`
  host, and `server.allowed_hosts` are accepted. This stops DNS rebinding.

Non-browser clients such as the TUI send neither cookies nor `Origin`. They can write when the
server allows unauthenticated writes, or through a trusted proxy.

## Live events (SSE)

The event name is the type, and `data` is the JSON of that type. The current snapshot is
sent first on connect.

| Event | Data | When |
|---|---|---|
| `snapshot` | `Snapshot` | Every probe cycle (2s in demo, about 30s in live mode) |
| `sample` | `Sample` | One per hop per cycle, for sparklines |
| `speed` | `SpeedRun` | Progress updates (`state: running`, `progress` 0–1), then the final result |
| `incident` | `Incident` | Opened, escalated, or closed (`ended` set) |
| `activity` | `Activity` | Anything added to the activity timeline (1.6) |

## Shapes (abridged)

```text
Snapshot  { at, verdict: Verdict, hops: [Hop], open_incident: string?, targets: [Target] }
Target    { id: "target:<slug>", name, check, kind: tcp|http|https|ping|dns, critical,
            status, detail, rtt_ms?, loss_pct? }                                  (1.1)
ConfigView { path?, config: <wirewolf.toml as JSON>, sources: { "probe.interval": file|env|flag },
             editable, save_path?, restart_keys: [string] }                           (1.2)
             defaults: <same shape as config, built-in values>                         (1.4)
FieldError { key: "probe.interval" | "targets.0.check" | "" (whole config), message }  (1.2)
Verdict   { severity: healthy|degraded|down|unknown, headline, detail, blame: hop_id?,
            confidence: low|medium|high, fix_hint?, method }
Hop       { id, kind: host|gateway|isp|internet|dns|web, label, target?, status, summary,
            metrics: { rtt_ms?, jitter_ms?, loss_pct? },
            evidence: [{ key, value }], checks: [{ name, status, detail, rtt_ms? }],
            interfaces: [Interface] }                                  (interfaces: 1.6)
Interface { name, via?, default_route, link, wireless, speed_mbps?, duplex: full|half?,
            addresses: [cidr], rx_errors?, tx_errors?, rx_dropped?, tx_dropped?,
            carrier_changes? }                                                    (1.6)
status    = pass | warn | fail | skip | not_applicable | pending
Sample    { at, hop, status, rtt_ms?, jitter_ms?, loss_pct? }
SpeedRun  { id, started, state: running|complete|failed, backend, server?, down_mbps?,
            up_mbps?, idle_latency_ms?, loaded_latency_ms?, bloat?, bytes_used?, progress?,
            error? }
Incident  { id, started, ended?, severity, headline, blame?, evidence: Snapshot }
Activity  { id, at, kind: verdict|incident|speed|config|auth|probe, message, subject?,
            actor? }                                                              (1.6)
LogLine   { seq, at, level: error|warn|info|debug|trace, target, message }        (1.6)
Session   { methods: [oidc|header], user: { id, name?, email?, via: oidc|header }?,
            can_write, reads_require_login, csrf?, login_url?, logout_url? }      (1.3)
```

User targets (`[[targets]]` in `wirewolf.toml`) arrive in `Snapshot.targets`, in config
order. Each one has history under its id: `/history?hop=target:nas`, and `sample` events
with `hop: "target:nas"`. A failing `critical` target makes an otherwise healthy verdict
`degraded` with `blame` set to the target id, so incidents can blame a target as well as a
hop. `ConfigView.config` has the same shape as the TOML file (`wirewolf config default`),
with credentials in URLs replaced by `***`. A key that's missing from `sources` has its
default value.

`PUT /config` (1.2) writes the file with its comments, ordering, and untouched values kept.
It writes only settings that differ from what the file says, or from the default when the
file doesn't mention them, and creates the file at `save_path` if needed. The server rejects
changes it can't honour, with one `FieldError` per key: a value set by an env var or flag
(the override would win), a key in `restart_keys` (`server.listen`, `storage.*`), and
anything that fails validation (bad target checks, intervals under 5s, and so on). `[auth]`
is never taken from the request. A `***` credential in a URL is kept as it was when the
URL is otherwise unchanged. `editable` is false with `--read-only-settings`
(`WIREWOLF_READONLY_SETTINGS=1`). The auth layer can also refuse writes; see `GET /session`.

Incidents open after `incidents.open_after` unhealthy snapshots in a row, and close after
`incidents.close_after` healthy ones. Both default to 2. The open is backdated to the first
bad snapshot, and the close is dated at the first good one.

`interfaces` (1.6) lists the host's physical network ports on the `host` hop, connected or
not, with the default-route port first; it's empty on every other hop. Virtual interfaces
(loopback, bridges, `docker0`, `veth*`, VPN tunnels) are left out. When a bridge holds the
addresses, as with Synology's `ovs_eth0`, the port is listed as `eth0` with `via: "ovs_eth0"`.
`link` is false for an unplugged port. Counters are totals since boot, and `null` where the OS
doesn't expose them (macOS reports no counters or duplex). The host hop warns, with one check
each, when a connected wired port negotiates below 1 Gbps (`eth0 speed`) or half duplex
(`eth0 duplex`), when its error counters rise between cycles (`eth0 errors`), when it loses
its link and reconnects (`eth0 flapping`), and when two connected ports share an IPv4 subnet
(`eth0 eth1 subnet`). Unplugged ports never warn.

Hops always come in path order: `host`, `gateway`, `isp`, `internet`, `dns`, `web`. A
`skip` hop is collateral damage from an earlier failure. Show it, but de-emphasize it.

## Activity and server logs (1.6)

The activity timeline records what wirewolf did or noticed: verdict changes, incidents
opening, getting worse, and closing, speed tests starting and finishing, settings saved
(the keys, never the values), sign-ins and sign-outs, log filter changes, and what the
probes can do here (ICMP and TTL replies, recorded when it changes). It's kept in the
history database with the same retention. `subject` is the incident or speed run id.
`actor` says who made a change, and is `null` for viewers who can't write.

`GET /logs` serves the last 2,000 server log lines (`capacity`) from memory. They name
hosts and addresses, so they need a user who can write, like `/brief?redact=false`. Before a
line is stored, URL credentials, the values of secret-looking keys (`client_secret=`,
`token=`, `code=`, `Cookie:`, `X-Wirewolf-CSRF:` and the like), and the configured OIDC
client secret are replaced with `***`. `level` keeps that level and more severe ones,
`target` keeps modules starting with it, `q` matches text case-insensitively, and `after`
returns only lines with a higher `seq`. `message` reads like the console's: any span
context first (`cycle{n=3}: `), then the message, then its fields as `key=value`.

`filter` is the log filter in effect, in `RUST_LOG` syntax. It decides what the server
records at all, both here and on stderr. `PUT /logs/filter` changes it at once. It also
saves it as `log.filter` (`saved: true`), unless settings are read-only, an env var
(`WIREWOLF_LOG_FILTER`) sets it, or the server has no config file to write (the demo).
`PUT /config` with a new `log.filter` applies it the same way. At startup, `RUST_LOG`, when
set, wins over `log.filter`.

## Demo scenarios

`healthy`, `wifi_loss`, `isp_outage`, `dns_down`, `captive_portal`, `bufferbloat`,
`slow_link` (the host's port negotiated 100 Mbps; 1.6), and `flapping` (healthy and ISP outage alternate every 40s, which exercises the live incident
open/close).

## The brief (1.5)

`GET /api/v1/brief` summarizes the network in a few dozen lines for an AI assistant. It covers:

- the verdict
- each hop in path order, with only its non-passing checks
- user targets
- the latest 5 incidents
- per-hop history statistics (p50/p95/max RTT, max loss, non-pass count) over `window`
- the latest 3 speed runs

It's about 25 times smaller than the raw `/now`, `/incidents`, and `/history` JSON for a day.
The dashboard's "Ask an AI" button and `wirewolf check --brief` use it. The builder is
`wirewolf_core::brief`.

- `format`: `text` (the default, `text/plain`, first line `# wirewolf brief v1 · …`) or `json`
  (`Brief`, same fields).
- `redact`: defaults to `true`. Public IP addresses become `<public-ip-N>`, the same
  placeholder for the same address within one brief. Private, CGNAT, link-local,
  documentation, and configured probe addresses stay, because they explain the network.
  MAC addresses and the DNS query list are left out. `redact=false` returns `403` unless the
  viewer may write (`Session.can_write`), because reads are open by default.
- `window`: how far back the history summary looks, for example `6h`. Defaults to `24h`.
- `incident`: describe that incident's evidence instead of now, with only the incidents up to
  it. `404` if unknown.

Returns `204` before the first measurement and `422` for a bad `window`.
