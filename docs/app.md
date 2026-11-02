# The desktop app

wirewolf is also a Mac app. It is the same server and dashboard as `wirewolf serve`, running
inside a native window, with the things a Mac app is expected to have: a menu bar, keyboard
shortcuts, a status item showing the verdict, a Dock badge for open incidents, notifications,
and a window that remembers where it was. Nothing listens on the network: the dashboard is
served on a random loopback port that only the app can reach, so there is no sign-in to set up.

Use it when wirewolf should watch the network from the machine you sit at. For a box that
watches around the clock (a NAS, a Raspberry Pi, a home server) and a dashboard you open from
anywhere, see [Deploying](deploy.md).

## Building

The app is built from the repo with [Tauri](https://tauri.app) 3, which is in alpha, so the
CLI version is pinned; it isn't on crates.io.

```sh
cargo install tauri-cli --version 3.0.0-alpha.5
cd crates/wirewolf-app
cargo tauri build
```

That needs Rust, Node, and pnpm (the dashboard is built along the way, as for `wirewolf-web`).
The result is `target/release/bundle/macos/Wirewolf.app` and a `.dmg` beside it, in
`target/release/bundle/dmg/`. The bundle is unsigned, so the first launch goes through
right-click → Open.

For a quick run without a bundle, `cargo run -p wirewolf-app` (or `cargo tauri dev`) starts the
app from the build directory. Notifications need a bundle, so they are the one thing that
doesn't work this way. `cargo run -p wirewolf-app -- --demo` shows scripted data instead of
probing the network, with `--scenario <id>` to pick the scene (`healthy`, `isp-outage`, …, the
same ids as `wirewolf serve --demo`).

## The window

The dashboard fills the window, with the sidebar under the traffic lights. The sidebar's empty
space and the strip above the page drag the window; double-clicking the strip zooms it. The
window keeps its size and position between launches.

Closing the window (⌘W or the red button) hides it; the probes keep running and the status item
stays in the menu bar. Click the Dock icon or the status item, or open the app again, to bring
the window back. **Quit** in the Wirewolf menu (⌘Q) stops everything.

Light, dark, or system theme is the dashboard's own choice (the sun/moon button in the sidebar).
The window follows it, so the sidebar's translucency and the system dialogs match.

## Menus and shortcuts

| Menu | Item | Shortcut |
|---|---|---|
| Wirewolf | Settings… | ⌘, |
| Wirewolf | Launch at Login | |
| Wirewolf | Quit Wirewolf | ⌘Q |
| File | Run Speed Test | ⌘T |
| File | Copy Brief for AI | ⇧⌘C |
| File | Connection Report | ⇧⌘R |
| File | Export → History, Incidents, Speed Tests as CSV | |
| File | Close Window | ⌘W |
| View | Now, History, Incidents, Speed, Logs | ⌘1 … ⌘5 |
| View | Toggle Sidebar | ⌘B |
| View | Reload | ⌘R |
| View | Enter Full Screen | ⌃⌘F |
| Help | Wirewolf Help, Report an Issue | |

**Copy Brief for AI** copies the same prompt and brief as the dashboard's "Ask an AI" button,
with public addresses redacted, ready to paste into an assistant. **Launch at Login** registers
the app with macOS so it starts, hidden to the status item, when you sign in.

CSV exports, and the brief when you download it from the dashboard, are saved to Downloads
(numbered if the name is taken) and shown in the Finder. Links to other sites open in your
browser; the window shows only wirewolf.

## The status item

The wolf in the menu bar is the verdict at a glance. It is monochrome while the network is
healthy, amber with the word *Degraded* while something is wrong, and red with *Down* when the
connection is out. Its menu shows the verdict's headline, opens the window, runs a speed test,
copies the brief, or quits.

The Dock icon shows the number of open incidents as a badge, and clears when they close.

## Notifications

The app posts a notification when an incident opens, when one gets worse, and when the network
is back to normal (with how long the incident lasted). macOS asks for permission the first time.
These are separate from the ntfy and webhook notifications under Settings → Notifications, which
the app can also send; use those for a phone or a chat channel.

## Where things are

Settings and history live in `~/Library/Application Support/net.joshka.wirewolf/`:
`wirewolf.toml`, which Settings writes (there is no file until you change something), and
`wirewolf.db`, the SQLite history. The paths, and the address the server listens on, are set by
the app, so Settings shows them but can't change them. Everything else in the
[configuration](guide.md#configuration) applies, and the file can be edited by hand while the
app is closed.

To remove the app, delete `Wirewolf.app`, that directory, and, if Launch at Login was on,
`~/Library/LaunchAgents/net.joshka.wirewolf.plist`.

## Differences from `wirewolf serve`

- The dashboard is reachable only from the app. To see it from other devices, run
  `wirewolf serve` instead, or as well.
- There is no sign-in, and the `[auth]` and `server.listen` settings are ignored.
- `wirewolf tui` and `wirewolf check --url` can't reach the app's server, since its port changes
  each launch.
- The probes are the same, with the same needs: see [Troubleshooting](guide.md#troubleshooting)
  if the host hop reports that ICMP isn't available.
