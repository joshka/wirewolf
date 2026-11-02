# Changelog

All notable changes to wirewolf are recorded here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and wirewolf follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) from its first release.

## [Unreleased]

### Fixed

- Server logs no longer carry colour codes when stderr isn't a terminal (a file, a pipe, or
  the systemd journal), or when `NO_COLOR` is set.
- On a phone, each line of the verdict's status line started with a stray dot.

### Added

- The connection report is harder to dismiss: speed is split into peak hours (weekdays 7–11
  pm local time, as the FCC's Measuring Broadband America reports do) and the rest, it says
  what speed 80% of tests reached, and it ends with how it was measured (the machine and its
  port, the probe cadence and targets, the incident rule, and the speed test setup). The
  copied text has the same sections.
- Verdicts weigh their evidence: an ISP outage that could only be inferred (no raw socket for
  TTL probes) is low confidence and says so, and a degraded verdict that has held for four
  checks in a row becomes high confidence instead of staying "medium" forever.
- One address family failing gets its own headline ("IPv6 is broken; IPv4 still works")
  instead of a generic "slow" one, with a fix hint for the router.
- `docs/guide.md` has a Home Assistant REST sensor recipe, and `docs/design.md` no longer
  lists metrics and notifications as unbuilt.
- Releases attach prebuilt binaries for Linux (x86_64, aarch64) and macOS (Apple silicon,
  Intel), with a `SHA256SUMS` file and build provenance, and `cargo binstall wirewolf` fetches
  them. The README's install section starts from those, and `docs/deploy.md` gains a "Linux
  without Docker" section with a hardened systemd unit.
- The TUI catches up with the dashboard's verdict explanation: the banner leads with whose
  side the problem is on, `e` opens the checks behind the verdict (on Now, or on an incident's
  evidence) with the cause, knock-on failures, and excused checks marked, and your targets get
  their own card on Now.
- The verdict on Now explains itself. Under the headline, it says whose side the problem is
  on, then lists every check behind the verdict in path order: what wirewolf did, what came
  back against which limit, and what that means, marking the cause, knock-on failures, and
  hops that look bad but don't count. When everything passes, the checks collapse to one
  line. **Copy explanation** copies it as plain text for a support chat.
- API 1.8: `Verdict` gains `steps`, the checks behind the verdict in path order, each with
  what was checked, what came back, the limit, and what it means, plus `bottom_line`,
  `confidence_reason`, and `held_cycles`. All are additive; older servers omit them.

### Changed

- Charts have a real clock axis (ticks on :00, :15, midnight, with the date where a day starts),
  a log scale for round trips with ms ticks, a dashed 100 ms limit line, a gap wherever a hop
  stopped answering instead of a line drawn through the outage, each hop's name at the end of
  its line, and a tooltip that lists every hop at that moment, worst first, with its loss. A
  note under each chart says how it was summarised ("avg · 2 min · log scale") and the timezone.
- The Now page shows the last hour of every hop on one chart, with the blamed hop bold and
  incidents shaded, in place of a sparkline per path row. It links to History on the same
  window. The dashboard keeps an hour of live samples instead of 15 minutes.
- History is the investigation tool: range presets from 15 minutes to 2 days, drag on a chart
  to zoom (with ← → to pan and Esc or "Back to live" to return), avg/p95/max per bucket, log
  or linear, one crosshair shared by the round-trip, loss, speed and events charts, a stats
  table under the round-trip chart (last, avg, p95, worst, loss per hop) that is also the
  legend (tick a hop to hide it, "only" to see it alone), and an events lane with incidents,
  speed tests and settings changes on the same axis. The whole view is in the URL, so it can
  be linked to and shared, and the CSV download covers the window shown.
- A hop's page charts 15 minutes to 2 days and links to History to compare it with the other
  hops; an incident's page charts every hop from 15 minutes before it opened to 15 minutes
  after it closed, with a link to the same window in History.
- Dashboard polish: the primary button is neutral (the accent now only marks navigation,
  focus, and changed settings); time, period, tab, and layout choosers are segmented controls
  with only the selected item raised; incident headlines line up in the Incidents list and the
  two tables of the connection report share one set of column widths; and the Thresholds
  grid in Settings no longer slides out of its columns on a read-only server.

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
