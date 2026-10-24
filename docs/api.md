# wirewolf API v1

All UIs and the TUI consume this API. The Rust types in `crates/wirewolf-core/src/model.rs`
are the source of truth, and this page summarizes them. Changing the contract means editing
`model.rs` and this page together on `main`, then telling the other workstreams (see
`docs/workstreams.md`). Additive changes (new optional fields, new endpoints) are fine.
Breaking changes need agreement first.

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
| GET | `/api/v1/events` | SSE stream (see below) |
| POST | `/api/v1/demo/scenario` | Body `{"id":"dns_down"}`. Returns `202`, `422` for an unknown id, or `404` in live mode |
| GET | `/api/v1/session` | `Session`: who is signed in and what they may do (1.3). Always readable |

## Sign-in and writes (1.3)

`[auth]` in `wirewolf.toml` configures sign-in: OIDC (`/auth/login` → issuer →
`/auth/callback`, kept in an HttpOnly cookie), and/or a trusted header such as `Remote-User`
set by a proxy listed in `auth.trusted_header.proxies`.

- **Reads** (every `GET`, including SSE) are open, or need a user with
  `auth.require_for_reads` (`401` otherwise).
- **Writes** (every other method) need a user when a sign-in method is configured (`401`).
  With none configured they're refused (`403`) unless `auth.allow_unauthenticated_writes`;
  `serve --demo` allows them.
- Writes from an OIDC session must send `X-Wirewolf-CSRF: <Session.csrf>` (`403` otherwise).
  Writes the browser marks cross-site (`Sec-Fetch-Site`, or an `Origin` that isn't the host)
  are refused.
- `POST /auth/logout` with the CSRF header ends an OIDC session (`204`).

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

## Shapes (abridged)

```text
Snapshot  { at, verdict: Verdict, hops: [Hop], open_incident: string?, targets: [Target] }
Target    { id: "target:<slug>", name, check, kind: tcp|http|https|ping|dns, critical,
            status, detail, rtt_ms?, loss_pct? }                                  (1.1)
ConfigView { path?, config: <wirewolf.toml as JSON>, sources: { "probe.interval": file|env|flag },
             editable, save_path?, restart_keys: [string] }                           (1.2)
FieldError { key: "probe.interval" | "targets.0.check" | "" (whole config), message }  (1.2)
Verdict   { severity: healthy|degraded|down|unknown, headline, detail, blame: hop_id?,
            confidence: low|medium|high, fix_hint?, method }
Hop       { id, kind: host|gateway|isp|internet|dns|web, label, target?, status, summary,
            metrics: { rtt_ms?, jitter_ms?, loss_pct? },
            evidence: [{ key, value }], checks: [{ name, status, detail, rtt_ms? }] }
status    = pass | warn | fail | skip | not_applicable | pending
Sample    { at, hop, status, rtt_ms?, jitter_ms?, loss_pct? }
SpeedRun  { id, started, state: running|complete|failed, backend, server?, down_mbps?,
            up_mbps?, idle_latency_ms?, loaded_latency_ms?, bloat?, bytes_used?, progress?,
            error? }
Incident  { id, started, ended?, severity, headline, blame?, evidence: Snapshot }
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

Hops always come in path order: `host`, `gateway`, `isp`, `internet`, `dns`, `web`. A
`skip` hop is collateral damage from an earlier failure. Show it, but de-emphasize it.

## Demo scenarios

`healthy`, `wifi_loss`, `isp_outage`, `dns_down`, `captive_portal`, `bufferbloat`, and
`flapping` (healthy and ISP outage alternate every 40s, which exercises the live incident
open/close).
