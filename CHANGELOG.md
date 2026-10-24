# Changelog

All notable changes to wirewolf are recorded here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and wirewolf follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) from its first release.

## [Unreleased]

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


[Unreleased]: https://github.com/joshka/wirewolf/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/joshka/wirewolf/releases/tag/v0.1.0
