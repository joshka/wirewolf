# Workstreams (phase 1)

Five threads work in parallel, each in its own jj workspace under `work/` (git-ignored), and
integrate back to `main`. The coordinator thread owns `main`'s contract (`model.rs`,
`docs/api.md`, this file) and runs the comparison in phase 2.

| Workstream | Workspace | Owns |
|---|---|---|
| `ui-shadcn` | `work/ui-shadcn` | `crates/wirewolf-web/ui-shadcn/` (Node/Vite source), `src/ui_shadcn.rs`, feature `ui-shadcn` |
| `ui-htmx` | `work/ui-htmx` | `crates/wirewolf-web/src/ui_htmx/`, templates and assets, feature `ui-htmx` |
| `ui-topcoat` | `work/ui-topcoat` | `crates/wirewolf-web/src/ui_topcoat/`, feature `ui-topcoat` (or a separate crate if Topcoat needs to own the server, so say so) |
| `probes` | `work/probes` | `crates/wirewolf-probe` (live driver, Cloudflare plus Ookla speed backends, SQLite history), and the live branch of `serve` in `wirewolf-cli` |
| `tui` | `work/tui` | `crates/wirewolf-tui`, plus the `tui` and `check` subcommands in `wirewolf-cli` |

## Rules

- **Stay in your lane.** Edit files you own. Shared files (`wirewolf-web/src/lib.rs`
  routing, `wirewolf-cli/src/lib.rs`, workspace `Cargo.toml`, `Cargo.lock`) take small,
  additive edits only: one `nest`/`route` line, one subcommand, one feature.
- **Contract changes** (`wirewolf-core/src/model.rs`, `docs/api.md`) must be additive.
  Land them on `main` early and on their own, then message the other threads. A breaking
  change needs the coordinator's agreement first.
- **Demo first.** UIs and the TUI develop against `cargo run -- serve --demo`. The probes
  thread publishes into the same `Hub` with the same types, so everything works live once
  it lands.
- **Ports.** Use 7465 + your offset so parallel servers don't collide: ui-shadcn 7471,
  ui-htmx 7472, ui-topcoat 7473, probes 7474, tui 7475.

## Integrating to main

Integrate small and often:

```sh
jj git fetch 2>/dev/null; jj rebase -d main          # bring your work up to date
cargo build --all-features && cargo test             # must pass
jj bookmark set main -r @-                           # or @ if @ is the finished change
```

`main` only moves forward. If `jj bookmark set` refuses because `main` moved, rebase again.
Don't rewrite commits already on `main`. Write commit messages in a conventional style
(`feat(web): …`, `fix(core): …`).

## Talking to each other

Use T3 thread messages (`t3_thread_send`) to reach another workstream's thread directly
when you need something from it, or when you landed something it should know about
(contract changes, shared-file edits). Keep messages short and actionable. The coordinator
thread has the roster.
