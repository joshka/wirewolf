# wirewolf

Network diagnostics you can point a browser at.

wirewolf is a small self-hosted service for your home network. Run it on a home server, NAS, or
laptop, open it in a browser, and it answers three questions: **is the network broken, where, and
since when?**

It probes the path from your machine outward (host → gateway → ISP → internet → DNS → web) every
30 seconds, blames the first hop that breaks, and keeps a history of incidents and speed tests,
so "the internet was flaky last night" turns into a time, a hop, and a report you can send your
ISP.

![The Now page during an ISP outage: the verdict banner blames the ISP hop, and the hops after it are skipped](https://github.com/joshka/wirewolf/releases/download/v0.1.1/now-isp-outage.png)

## Features

- **One verdict.** Healthy, degraded, or down, with the hop to blame, how confident it is, and a
  suggested fix. Failures after the first break are shown as collateral, not as more problems.
- **The whole path.** Gateway RTT and loss, the ISP's edge router (by ICMP TTL), TCP to anycast
  internet hosts over IPv4 and IPv6, your resolvers compared with public ones, captive portal and
  TLS checks, and path MTU.
- **Your own targets.** Watch a NAS, router admin page, printer, or Pi-hole with `tcp://`,
  `https://`, `ping://`, or `dns://` checks. A `critical` target can open incidents.
- **Incidents.** Opened and closed automatically, with the evidence kept as it was and a
  copyable report for your ISP.
- **Speed tests.** Scheduled and on demand, against Cloudflare or Ookla's speedtest.net servers,
  with a bufferbloat grade. Built in; no external CLI.
- **Ask an AI.** A compact, redacted summary of what wirewolf measured, ready to paste into a
  chat assistant.
- **Works offline.** The dashboard is embedded in the binary and loads nothing from a CDN, so it
  still works when the internet doesn't.
- **A terminal UI and a one-shot check.** `wirewolf tui` for a terminal dashboard, `wirewolf
  check` for scripts, with exit codes.
- **Sign-in when you want it.** Reads are open on your LAN by default; changing settings needs
  OIDC or a trusted reverse-proxy header.

## Quick start

### Docker

```sh
docker run -d --name wirewolf --restart unless-stopped --network host \
  --cap-drop ALL --cap-add NET_RAW --read-only --security-opt no-new-privileges \
  -v wirewolf:/data ghcr.io/joshka/wirewolf
```

Then open `http://<host>:7465`. Or, from a checkout, `docker compose up -d`. Host networking
matters: on a bridge network the "gateway" wirewolf sees is Docker's bridge, not your router.
[docs/deploy.md](docs/deploy.md) explains these options and how to expose wirewolf safely.

### From source

```sh
cargo install wirewolf
wirewolf serve
```

The build compiles the dashboard, so it needs [Node](https://nodejs.org/) and
[pnpm](https://pnpm.io/) as well as Rust 1.88 or newer.

### Just check, once

```sh
wirewolf check
```

This runs one probe cycle on this machine, prints the verdict and each hop, and exits `0`
healthy, `1` degraded, `2` down, `3` if a server given with `--url` can't be reached, or `4`
unknown. No server, no history.

### Try it without probing anything

```sh
wirewolf serve --demo
```

Serves scripted data with a day of history and a scenario switcher (Wi-Fi loss, ISP outage, DNS
down, captive portal, bufferbloat, flapping).

## Configuration

Everything works with no config file. To change targets, thresholds, or the speed test schedule,
start from the fully commented defaults:

```sh
wirewolf config default > wirewolf.toml
```

The dashboard's Settings page shows the effective config and where each value came from, and can
edit it when you're allowed to.

## Documentation

- [User guide](docs/guide.md): concepts, reading verdicts, configuration, the CLI and TUI, speed
  tests, the AI brief, and troubleshooting
- [Deploying](docs/deploy.md): Docker, sign-in, reverse proxies, and who can see what
- [CLI reference](docs/cli.md): every command and flag
- [API](docs/api.md): the JSON and SSE API the dashboard and TUI use
- [Dashboard](docs/ui.md): the pages and how to develop the UI
- [Design](docs/design.md): why wirewolf works the way it does
- [Changelog](CHANGELOG.md)

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without
any additional terms or conditions.
