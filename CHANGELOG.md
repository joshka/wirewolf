# Changelog

All notable changes to wirewolf are recorded here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and wirewolf follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) from its first release.

## [Unreleased]

## [0.2.0] - 2026-10-24

### Added

- Notifications when incidents open, get worse, and close, sent to ntfy and/or a webhook
  (Slack, Discord, and Home Assistant accept its JSON). Set them up under Settings →
  Notifications or `[notify]` in the config (`ntfy`, `ntfy_token`, `webhook`, `min_severity`,
  `resolved`); "Send test" checks the settings. The token and webhook URL are kept out of the
  logs and hidden from people who can't change settings.
- A printable connection report at `/report`, linked from Incidents: outages on the provider's
  side kept apart from problems on your own network, total downtime, and speed against your
  plan over 7 or 30 days. It prints on white and can be copied as text.
- Prometheus metrics at `/api/v1/metrics`: the verdict, each hop's status, latency, jitter, and
  loss, user targets, whether an incident is open, and the latest speed test, in base units.
- CSV downloads of history, incidents, and speed tests (Download CSV on those pages, or
  `/api/v1/export/{history,incidents,speed}.csv`).
- Speed results compared with the plan you pay for. Set `speed.plan_down_mbps` and
  `speed.plan_up_mbps` (in Settings or the config; 0 means not set) to see each result as a
  share of the plan, a dashed plan line on the speed chart, and runs under 80% flagged.
- API 1.7: `Meta.monitoring_since` (the oldest measurement kept) and the metrics, export, and
  `POST /api/v1/notify/test` endpoints. All additions; see `docs/api.md`.
- The current state says how long it has lasted, and speed tests say which phase they are in.
- DNS and web rows say what failed.
- Every page has a title and description.

### Fixed

- A probe that hangs no longer stops probing: each probe is capped at 20 seconds and skipped
  for that cycle, and ICMP reads can no longer block forever.
- A new incident opens when the blamed hop moves, instead of extending the old one.

### Changed

- The Incidents page starts with a 30-day uptime strip, one cell per day. Days before
  wirewolf started measuring show as no data rather than as good days.
- Speed tests are drawn as measured points, the speed tests table is trimmed, and each
  target's timing is shown once.
- Overlapping history lines are easier to follow, and keyboard-focused charts show a focus
  ring.
- Charts and other pages load on demand, and the sidebar collapses on tablets.
- The dashboard builds with Node 26, TypeScript 7.0, and recharts 3.10. Dependencies and
  GitHub Actions are updated, and the unused `rand` dependency is removed from
  `wirewolf-web`.

## [0.1.1] - 2026-10-24

### Fixed

- A speed test that never finishes no longer stops all probing. Tests are capped at about
  twice their duration plus 90 seconds; one that runs over or crashes is marked failed with
  the reason, and probing resumes.
- Reloading the dashboard shows the last known page straight away instead of a blank frame.
- The release workflow pins pnpm, and the Docker build no longer races on the cargo cache
  when building several platforms.

### Changed

- Logging records state changes (a check starting to fail, and recovering) rather than every
  probe cycle, so the default `info` level stays quiet when the network is healthy. A
  finished speed test logs its result.
- The Logs page gains filter presets, colored console-style lines, and shows when it's
  streaming.

## [0.1.0] - 2026-10-24

The first release.

### Added

- Live probing of the path from the host outward: host, gateway, ISP edge (ICMP TTL, or
  inferred without a raw socket), internet (TCP to anycast hosts over IPv4 and IPv6), DNS
  (system resolvers compared with public ones), and web (captive portal, TLS, HTTPS sites),
  plus path MTU.
- A diagnosis engine that blames the first broken hop, marks later hops as collateral skips,
  explains common patterns (DNS failing while the internet answers by IP, captive portals),
  and suggests a fix.
- User targets (`tcp://`, `http(s)://`, `ping://`, `dns://`), with `critical` targets able to
  degrade the verdict and open incidents.
- Incidents that open and close after a configurable number of cycles, keep their evidence,
  and produce a copyable report for your ISP.
- Speed tests against Cloudflare or Ookla's speedtest.net servers, scheduled and on demand,
  with bufferbloat grades and data use per run.
- SQLite history with retention.
- A web dashboard embedded in the binary (Now, Hop, Incidents, History, Speed, Settings) that
  loads nothing from a CDN.
- A JSON and server-sent events API (`/api/v1`), documented in `docs/api.md`.
- A Settings page and `PUT /api/v1/config` that edit `wirewolf.toml` while keeping its comments.
- The AI brief (`/api/v1/brief`, `wirewolf check --brief`) and the dashboard's "Ask an AI"
  dialog, with public IP addresses redacted by default.
- Sign-in with OIDC or trusted reverse-proxy headers, CSRF protection, and a host-name check
  against DNS rebinding.
- The `wirewolf` command line: `serve` (with `--demo` scenarios), `check` with exit codes and
  `--json`, `tui`, and `config default` / `config show`.
- A terminal dashboard (`wirewolf tui`).
- A Logs page: an activity timeline kept with the history, and recent server log lines
  (redacted, for users who can write) with a log filter (`log.filter`) that changes without a
  restart.
- A hardened container image and compose file, and a signed release workflow.
- A user guide, a deployment guide, and a CLI reference generated from `--help`.


[Unreleased]: https://github.com/joshka/wirewolf/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/joshka/wirewolf/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/joshka/wirewolf/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/joshka/wirewolf/releases/tag/v0.1.0
