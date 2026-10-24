# wirewolf user guide

This guide covers how wirewolf thinks about your network, how to read what it tells you, and how
to configure and run it. For installing and exposing it safely, see [deploy.md](deploy.md). For
every flag, see [cli.md](cli.md).

- [Concepts](#concepts)
- [Reading verdicts](#reading-verdicts)
- [Configuration](#configuration)
- [The command line and the TUI](#the-command-line-and-the-tui)
- [Speed tests](#speed-tests)
- [The AI brief and "Ask an AI"](#the-ai-brief-and-ask-an-ai)
- [Troubleshooting](#troubleshooting)

## Concepts

### The path

wirewolf models your connection as six hops, always in this order:

| Hop | What it is | How it's measured |
|---|---|---|
| **host** | The machine wirewolf runs on | The default interface: up or down, IPv4 and global IPv6 addresses, MTU, path MTU |
| **gateway** | Your router | 20 ICMP echoes, or TCP connects to ports 53, 80, and 443 when ICMP isn't available |
| **isp** | The first router on your provider's side | ICMP with TTL 2, so the router two hops out answers. A full TTL sweep runs when something upstream is wrong |
| **internet** | Anycast hosts (Cloudflare, Google, Quad9 by default) | TCP handshakes on port 443 over IPv4 and IPv6, plus informational ICMP pings |
| **dns** | Your resolvers | Your system resolvers (they decide the hop's status), compared with public ones |
| **web** | Real web traffic | A captive-portal check that must answer `204`, a TLS handshake, and a few HTTPS sites |

Each hop gets a status: `pass`, `warn` (degraded), `fail`, `skip`, `not_applicable`, or
`pending` (not measured yet). A probe cycle runs every 30 seconds by default.

You can add your own **targets** (a NAS, a printer, a Pi-hole) alongside the path. They get the
same statuses and their own history; see [Targets](#targets).

### Blame and collateral skips

wirewolf blames the **first** hop that fails. Everything after a hard failure can't be tested
meaningfully, so it's marked `skip`: collateral damage, not more problems. If your ISP is down,
you'll see the ISP hop fail and the internet, DNS, and web hops skipped, not four failures.

With no hard failure, wirewolf blames the first degraded hop. Degraded means reachable but slow
or lossy, and it never counts as a failure.

A failing target marked `critical = true` makes an otherwise healthy verdict degraded and is
blamed by its id (for example `target:nas`). A broken path still takes the headline, since it
explains the target too.

### Incidents

An incident opens after `incidents.open_after` unhealthy cycles in a row and closes after
`incidents.close_after` healthy ones. Both default to 2, so at the default 30-second interval a
blip under a minute isn't an incident. The incident's times are still exact: it starts at the
first bad cycle and ends at the first good one.

An incident keeps a copy of the evidence from when it happened. Open it in the dashboard
(Incidents → the incident) to see the path as it was and copy a plain-text report for your ISP.

## Reading verdicts

Every verdict has a severity, a headline, a detail line, the hop it blames, a confidence, and
usually a suggested fix:

| Severity | Meaning | `wirewolf check` exits |
|---|---|---|
| healthy | Everything passes | `0` |
| degraded | Something is slow or lossy, or a critical target is failing | `1` |
| down | Something on the path has failed | `2` |
| unknown | No measurements yet | `4` |

Confidence is high for a healthy verdict or when the blamed hop has two or more checks, medium
for a hop with one check or any degraded verdict, and low while waiting for the first
measurements.

The examples below are `wirewolf check` output from the demo scenarios
(`wirewolf serve --demo`, then pick a scenario). Hop lines are trimmed.

### Healthy

```text
HEALTHY: Everything looks good
The gateway, internet, DNS, and web checks all pass.
Confidence: High. Method: 16 checks across 6 hops.
```

### Packet loss on your local network

The gateway hop is lossy. Everything after it inherits the loss, but only the first lossy hop is
blamed.

```text
DEGRADED: Packet loss on your local network (Wi-Fi or LAN)
Gateway is dropping 9% of packets; everything after it inherits that.
Suggested fix: Check Wi-Fi signal or interference, or try a cable.

  [WARN] Gateway    192.168.1.1                         4.8 ms, 9% loss   <-- problem here
  [PASS] ISP        100.64.0.1 (AS64500 Example Fiber)  7.8 ms, 9% loss
```

The same shape with no loss reads "Slow on your local network". At the ISP it reads "Packet loss
at your internet provider" or "Slow at your internet provider"; for the internet hop, "… reaching
the internet"; and so on for DNS and the web.

### Bufferbloat: slow at your provider

Latency rises while the link is busy. This often means something on your network is saturating
the connection; a speed test shows the bufferbloat grade.

```text
DEGRADED: Slow at your internet provider
ISP is responding slowly (33.8 ms, no loss).
Suggested fix: Check nothing on your network is using the whole connection, then run a speed test.
```

### Your provider is down

The router answers, but nothing beyond it does. The later hops are skipped as collateral.

```text
DOWN: Your internet provider's connection is down
The router answers, but traffic dies just beyond it on the provider's side.
Suggested fix: Check the modem/ONT lights and your ISP's status page.

  [PASS] Gateway    192.168.1.1                         1.4 ms, no loss
  [FAIL] ISP        100.64.0.1 (AS64500 Example Fiber)  100% loss   <-- problem here
  [SKIP] Internet   Cloudflare · Google · Quad9         skipped — ISP is down
```

When the ISP hop answers but no internet host accepts connections, the headline is "The internet
is unreachable" instead.

### DNS is failing

Hosts answer by IP address but names don't resolve, so websites look down even though the
connection works. Comparing your resolver with public ones tells "my resolver is broken" apart
from "DNS is blocked".

```text
DOWN: DNS is failing — the internet is reachable by IP
Hosts answer by IP address, but names don't resolve. Websites will look down.
Suggested fix: Switch resolver (for example 1.1.1.1) or restart the router's DNS forwarder.

  [FAIL] DNS        192.168.1.1                         system resolver timing out; public resolvers fine   <-- problem here

DNS checks:
  [FAIL] System resolver: 192.168.1.1 — timed out after 4s (3 tries)
  [PASS] Cloudflare 1.1.1.1: example.com → 192.0.2.10 (11 ms)
```

### Web traffic is intercepted

DNS and connections work, but the captive-portal check didn't get its `204`. Usually a hotel or
guest-network login page, or a filtering proxy.

```text
DOWN: Web traffic is being intercepted
DNS and connections work, but HTTP responses are not what was expected. This is usually a
captive portal or a filtering proxy.
Suggested fix: Open a browser to complete any login page, or check proxy settings.

Web checks:
  [FAIL] Captive portal: expected 204, got 302 → login.hotspot.example (31 ms)
```

### Other failures

- **"This machine has no working network connection"**: the host's interface is down or has no
  usable address. Check the cable or Wi-Fi association, then DHCP.
- **"Can't reach the router"**: the gateway isn't answering, so nothing beyond it can be reached.
- **"Waiting for the first measurements"**: wirewolf has just started.

## Configuration

wirewolf runs with no config file. To change anything, start from the defaults, which are fully
commented and are the reference for every key:

```sh
wirewolf config default > wirewolf.toml
```

Keep only the keys you change; anything you leave out keeps its default. Settings are applied in
this order, later ones winning:

1. built-in defaults
2. the config file: `--config <path>` or `WIREWOLF_CONFIG`, otherwise `wirewolf.toml` in the
   current directory if it exists (`/data/wirewolf.toml` in the container)
3. environment variables and flags, such as `WIREWOLF_INTERVAL` or `--interval` (see
   [cli.md](cli.md))

`wirewolf config show` loads everything, reports errors, and prints the effective settings. The
dashboard's **Settings** page shows the same thing with where each value came from, and can save
changes to the file when you're allowed to write (see [deploy.md](deploy.md)). Changes apply from
the next probe cycle, except `server.listen` and `storage.*`, which need a restart. A value set by
an environment variable or flag can't be changed from the dashboard, since the override would
still win.

### Targets

Add your own things to watch with `[[targets]]`:

```toml
[[targets]]
name = "NAS"
check = "tcp://nas.local:445"
critical = true

[[targets]]
name = "Router admin"
check = "https://router.local"
verify_tls = false

[[targets]]
name = "Printer"
check = "ping://192.0.2.5"
```

The checks are `tcp://host:port` (a TCP handshake), `http://…` or `https://…` (a GET; any status
below 500 counts), `ping://host` (ICMP echo, or TCP to common ports when ICMP is unavailable),
and `dns://server[:port]` (the first `[[dns.queries]]` name, asked of that server). Targets may
point anywhere on your LAN, but not at cloud metadata addresses.

You can also change the path itself: `[[internet.targets]]`, `[[dns.resolvers]]`,
`[[dns.queries]]` (with `expect` to catch filtering or hijacking resolvers), and `[web]`.

### Thresholds

A hop above any of its limits in `[thresholds.<hop>]` is degraded. There are sections for
`gateway`, `isp`, `internet`, `dns`, and `targets`, each with `rtt_warn_ms`, `jitter_warn_ms`,
and `loss_warn_pct`. Not every limit applies to every hop; the comments in `wirewolf config
default` say which. For example, the ISP hop only uses loss, and only while the internet hop is
lossy too, because routers on the way rate-limit ICMP.

## The command line and the TUI

### `wirewolf serve`

Runs the probes, keeps history in SQLite (`storage.db`, kept for `storage.retention`, 30 days by
default), and serves the dashboard and API on `0.0.0.0:7465`. Useful flags:

- `--demo [--scenario <id>]` serves scripted data instead of probing: a day of history with past
  incidents, and a scenario switcher in the sidebar. Scenarios are `healthy`, `wifi_loss`,
  `isp_outage`, `dns_down`, `captive_portal`, `bufferbloat`, `slow_link`, and `flapping`.
- `--read-only-settings` shows settings without letting the dashboard change them.
- `--listen`, `--interval`, `--speed-interval`, `--speed-backend`, `--db`, `--retention`
  override the config file.

Set `RUST_LOG` (for example `RUST_LOG=debug`) for more logging.

### `wirewolf check`

Prints the verdict and each hop, then exits with the severity's code: `0` healthy, `1` degraded,
`2` down, `3` unreachable (the server given with `--url` didn't answer), `4` unknown.

Without `--url` it runs one probe cycle on this machine: no server, no history. That's the "is it
me or the internet?" check for a laptop. It skips the slow TTL sweep, which the verdict doesn't
depend on. With `--url http://host:7465` (or `WIREWOLF_URL`) it asks a running server instead.

- `--json` prints the snapshot as JSON (the API's `Snapshot`; `null` if the server hasn't
  measured anything yet).
- `--brief` prints the AI brief (see [below](#the-ai-brief-and-ask-an-ai)). From a server, it
  includes history, incidents, and speed runs; locally, it has only the current snapshot.
- `--brief --no-redact` keeps public IP addresses. A server allows that only for clients that may
  change settings.

Output is colored on a terminal unless `NO_COLOR` is set.

```sh
# Alert from cron when the network isn't healthy
wirewolf check --url http://wirewolf.lan:7465 > /dev/null || echo "network: exit $?"
```

### `wirewolf tui`

A terminal dashboard for a running server (`--url`, default `http://127.0.0.1:7465`). It loads
state over the API, follows live events, and reconnects when the connection drops.

| Key | Does |
|---|---|
| `1`–`4`, `Tab`, `Shift-Tab` | Now, History, Incidents, Speed |
| `←` `→` or `h` `l` | Pick a hop |
| `Enter` | Hop details (Now), toggle a hop (History), open an incident (Incidents) |
| `i` | Jump to the open incident (Now) |
| `t` | Change the time range (History) |
| `c` or `y` | Copy the incident report to the clipboard (Incidents) |
| `r` | Run a speed test |
| `s` | Switch demo scenario (demo mode only) |
| `?` | Help |
| `q` | Quit (or close a popup) |

### `wirewolf config`

`wirewolf config default` prints the commented default config. `wirewolf config show [--config
<path>]` prints the effective settings, with credentials in URLs replaced by `***`.

## Speed tests

wirewolf runs a speed test every hour by default (`speed.interval`; `"0"` turns scheduled tests
off), and on demand from the Speed page, the TUI (`r`), or `POST /api/v1/speed/run`. Only one
runs at a time.

There are two backends, both built in:

- **`cloudflare`** (the default) downloads from and uploads to `speed.cloudflare.com`.
- **`ookla`** uses Ookla's speedtest.net server network. It picks the fastest of the nearest few
  servers, or always uses `speed.server = "host:port"` if you set it.

Each test measures idle latency, then downloads and uploads with `speed.streams` parallel
connections (8) for `speed.duration` (10 seconds) in each direction, measuring latency while the
line is loaded.

**Data use.** A test moves roughly what your connection can carry in those 20 seconds: about
1.25 MB for each Mbps of download plus upload. On a 100/20 Mbps line that's about 150 MB a test;
on gigabit, about 2 GB. Hourly on gigabit adds up to tens of gigabytes a day. If your plan has a
cap, lengthen `speed.interval` or set it to `"0"`. Each result records the bytes it used.

**Your plan.** Set `speed.plan_down_mbps` and `speed.plan_up_mbps` to the speeds you pay for
(Settings → Speed tests, or in `wirewolf.toml`). Results then show as a share of the plan ("91% of
1000 Mbps plan"), the speed chart draws the plan as a dashed line, and runs under 80% of it are
flagged in the table. Without a plan, runs under half your usual speed are flagged instead.

**Bufferbloat grade.** The grade comes from how much latency rises under load (loaded minus idle),
using the common Waveform thresholds:

| Grade | Increase under load |
|---|---|
| A+ | under 5 ms |
| A | under 30 ms |
| B | under 60 ms |
| C | under 200 ms |
| D | under 400 ms |
| F | 400 ms or more |

A poor grade means your router's queues fill up when the line is busy, so calls and games lag
while anything big is downloading or uploading. Smart queue management (SQM, such as `cake` or
`fq_codel`) on the router usually fixes it.

## Export and Prometheus

The History, Incidents, and Speed pages each have a **Download CSV** button (History exports the
range on screen). The same files are at `/api/v1/export/history.csv`, `incidents.csv`, and
`speed.csv`.

For Grafana or alerting, point Prometheus at `/api/v1/metrics`:

```yaml
scrape_configs:
  - job_name: wirewolf
    metrics_path: /api/v1/metrics
    static_configs:
      - targets: ["wirewolf.lan:7465"]
```

`wirewolf_verdict{severity="down"} == 1` is "the network is down", and
`wirewolf_incident_open == 1` is "an incident is open". `docs/api.md` lists every metric. With
`auth.require_for_reads` on, Prometheus can't scrape it, because it can't sign in; leave reads
open on your LAN, or scrape through a proxy that signs in.

## The AI brief and "Ask an AI"

The brief is a compact text summary of what wirewolf measured, written for a chat assistant: the
verdict, each hop in path order with only its non-passing checks, your targets, the latest 5
incidents, per-hop history statistics over the last 24 hours, and the latest 3 speed runs. It's
a few dozen lines, about 25 times smaller than the raw API data for a day.

```text
# wirewolf brief v1 · demo · 2026-10-24 12:23Z
verdict: DEGRADED (medium confidence) — Slow at your internet provider
blame: isp
...
## path (in order; the first hop that isn't pass is the break, later skips are collateral)
- isp [warn] 100.64.0.1 (AS64500 Example Fiber) — 29.1 ms, no loss · rtt=29.1ms jitter=22.1ms loss=0.0%
...
## history (2026-10-23 12:23Z → 2026-10-24 12:23Z, 14580 samples)
- isp: rtt p50=4.6 p95=33.9 max=69.2ms; loss max=100%; not pass 134/1458
```

**"Ask an AI"** in the dashboard shows the brief with a short request to explain the problem and
suggest what to try, cheapest first. Copy it into any assistant, or open ChatGPT, Claude,
Gemini, Perplexity, or Grok from the dialog. Most of those open with the prompt filled in; the
prompt is copied to the clipboard too, so you can paste it if it isn't. wirewolf doesn't send the
brief anywhere itself. If your internet is down, copy it and paste it from your phone. On an
incident's page, the brief describes that incident instead of now.

**Redaction.** By default, public IP addresses become placeholders such as `<public-ip-1>` (the
same address gets the same placeholder), and MAC addresses and the DNS query list are left out.
Private, CGNAT, link-local, documentation, and configured probe addresses stay, because they
explain the network. Turning redaction off needs a user who may change settings, because reads
are open by default.

From the command line, `wirewolf check --brief` prints the brief, and the API serves it at
`GET /api/v1/brief` and `GET /api/v1/brief/prompt` (see [api.md](api.md#the-brief-15)).

## Logs

The **Logs** page has two tabs.

- **Activity** is a timeline of what wirewolf did or noticed: the verdict changing, incidents
  opening and closing, speed test results, settings saved, sign-ins, and whether ICMP works
  here. It's kept with the rest of the history.
- **Server logs** shows the server's recent log lines, the same ones `docker logs` shows,
  and follows new ones live. Narrow them by level or search text. Logs name hosts and
  addresses on your network, so only people who can change settings see them. Passwords,
  tokens, and cookies are removed before a line is kept.

The **Filter** on that tab sets what the server logs, in `RUST_LOG` syntax: a level
(`error`, `warn`, `info`, `debug`, `trace`), optionally per module. It applies at once,
without a restart, and is saved as `log.filter` (also on the Settings page, and
`WIREWOLF_LOG_FILTER`). If `RUST_LOG` is set when wirewolf starts, it wins until you change
the filter. Lines read like the console output: time, level, module, then the message and
its fields.

Plain `debug` and `trace` are mostly the internals of the DNS, TLS, and HTTP libraries,
thousands of lines a minute, so start from a preset:

| Preset | Filter | Adds |
|---|---|---|
| Normal | `info` | Startup, speed tests, problems (the default) |
| Quiet | `warn` | Only warnings and errors |
| wirewolf detail | `info,wirewolf=debug` | One line per probe cycle: each hop's status and time |
| DNS detail | `info,wirewolf=debug,hickory_resolver=debug` | Each DNS query and which server answered |
| HTTP and TLS detail | `info,wirewolf=debug,reqwest=debug,rustls=debug` | Web checks, connections, and handshakes |
| Everything | `trace` | Every library's internals |

## Troubleshooting

### The ISP hop says "inferred"

Measuring the ISP's edge router needs to see ICMP "time exceeded" replies. On Linux that takes
either a raw socket or unprivileged ICMP (`net.ipv4.ping_group_range` covering wirewolf's group;
wirewolf checks at startup that time-exceeded replies arrive). Without either, wirewolf infers the ISP hop from whether the internet answers, and
the hop's evidence says `inferred from internet reachability (TTL probes need a raw socket)`. The
rest of the path still works.

- In Docker, keep `--cap-add NET_RAW` and run as the image's default user (see
  [deploy.md](deploy.md)).
- On Linux without a container, run as a user with `CAP_NET_RAW`, for example
  `sudo setcap cap_net_raw+ep "$(which wirewolf)"`, or allow unprivileged ICMP for its group,
  for example `sysctl net.ipv4.ping_group_range="1000 1000"`.
- A container that runs as a non-root user without `NET_RAW` can use unprivileged ICMP too:
  set the sysctl on the host with host networking, or pass
  `--sysctl net.ipv4.ping_group_range="0 2147483647"` in bridge mode.
- macOS delivers these replies to unprivileged sockets, so no extra setup is needed.

Separately, some ISP routers never send time-exceeded replies. Then the hop reads "edge router
silent — traffic flows" and passes as long as the internet answers.

### The gateway is a 172.x address and the router looks perfect

The container is on a Docker bridge network, so the gateway wirewolf sees is the bridge, not your
router. The host hop's evidence says so. Run with `--network host` (`network_mode: host` in
compose), which is what the provided `compose.yaml` does.

### IPv6

`probe.ipv6 = "auto"` (the default) tests IPv6 targets only when the host has a global IPv6
address. A host with no IPv6 isn't a failure: the IPv6 check reads `not_applicable`. When one
family works and the other doesn't, the internet hop warns with "IPv4 broken, IPv6 works" or
"IPv6 broken, IPv4 works". In Docker, use host networking so wirewolf sees the host's IPv6
addresses. Set `ipv6 = "on"` to test it anyway, or `"off"` to skip it.

### Sign-in problems

- **wirewolf refuses to start with an OIDC issuer set**: list who may sign in with
  `auth.allowed_emails` or `auth.allowed_groups`, or set `auth.allow_any_user = true` if your
  issuer only has your own users. `issuer`, `client_id`, `client_secret`, and `redirect_url` are
  all required.
- **"Sign-in failed: … is not in auth.allowed_emails or auth.allowed_groups"**: add that email or
  one of its groups.
- **"no sign-in in progress (expired, or cookies are blocked)"** or **"state mismatch"**: start
  again from the sign-in button in the same browser, and check that cookies aren't blocked. Open
  wirewolf at the same host name as `auth.redirect_url`, so the cookie set before sign-in comes
  back with the callback.
- **Sign-in finishes but you're still signed out**: with an `https://` redirect URL the session
  cookie is marked secure, and browsers only send it over HTTPS. Open wirewolf through your
  proxy's HTTPS address, not its plain-HTTP port.
- **`421 Misdirected Request`**: you reached wirewolf by a public host name it doesn't know. Add
  it to `server.allowed_hosts`.
- **"writes are off"**: no sign-in method is configured. Configure `[auth]`, or set
  `auth.allow_unauthenticated_writes = true` on a network you trust.
- **A trusted-header setup doesn't sign anyone in**: requests must come from an address in
  `auth.trusted_header.proxies`. With a Docker bridge between proxy and wirewolf, that's the
  proxy container's address, not `127.0.0.1`.

[deploy.md](deploy.md) covers sign-in and reverse proxies in more depth.
