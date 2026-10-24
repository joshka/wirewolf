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
| GET | `/api/v1/now` | `Snapshot`, or `204` before the first measurement |
| GET | `/api/v1/history?hop=&since=&until=` | `[Sample]` in time order. All params are optional. `hop` is a hop id, `since`/`until` are RFC 3339 |
| GET | `/api/v1/speed` | `[SpeedRun]`, newest first |
| POST | `/api/v1/speed/run` | `202` when started, `409` if one is already running. Progress arrives as `speed` events |
| GET | `/api/v1/incidents` | `[Incident]`, newest first |
| GET | `/api/v1/incidents/{id}` | `Incident` or `404` |
| GET | `/api/v1/events` | SSE stream (see below) |
| POST | `/api/v1/demo/scenario` | Body `{"id":"dns_down"}`. Returns `202`, `422` for an unknown id, or `404` in live mode |

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
Snapshot  { at, verdict: Verdict, hops: [Hop], open_incident: string? }
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
```

Hops always come in path order: `host`, `gateway`, `isp`, `internet`, `dns`, `web`. A
`skip` hop is collateral damage from an earlier failure. Show it, but de-emphasize it.

## Demo scenarios

`healthy`, `wifi_loss`, `isp_outage`, `dns_down`, `captive_portal`, `bufferbloat`, and
`flapping` (healthy and ISP outage alternate every 40s, which exercises the live incident
open/close).
