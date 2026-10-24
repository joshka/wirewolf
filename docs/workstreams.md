# Working in parallel

Several agent threads often work on wirewolf at once. Each one works in its own jj workspace
and integrates into `main`. One coordinator thread hands out work, owns this file, and runs
Josh's local server.

## Workspaces

- Each thread gets its own jj workspace, usually `work/<name>` (git-ignored). Use `jj`, not
  `git`, inside it. jj workspaces are not git worktrees.
- Never edit in the repo root; that is the coordinator's working copy.
- T3 Code deletes a thread's `work/<name>` directory when the thread goes away. Land work on
  `main` before finishing, and don't keep anything you need only in a workspace.
- The release build of Josh's server runs from `/Users/joshka/local/wirewolf-deploy`, a jj
  workspace that is deliberately outside `work/`. Don't use it.

## Rules

- **Contract changes** (`crates/wirewolf-core/src/model.rs`, `docs/api.md`) must be
  additive. Land them early, on their own, and tell the threads they affect. A breaking
  change needs the coordinator's agreement first.
- **Shared files** (`wirewolf-web/src/lib.rs` routing, `wirewolf-cli/src/lib.rs`, workspace
  `Cargo.toml`, `Cargo.lock`, `App.tsx`) take small, focused edits. If another active
  thread is likely to touch the same file, message it first.
- **Commit messages and images** follow `AGENTS.md`: Tim Pope / Chris Beams style, and no
  screenshots in the repo. Put them in `artifacts/<topic>/`.
- **Messaging.** Reach other threads with `t3_thread_send` in `auto` mode (steer). Don't
  queue a message unless it's a separate follow-up task.
- **Servers.** Start whatever test servers you need, on your own port, and stop them before
  you finish. Leave 127.0.0.1:7480 alone; it's Josh's.

## Ports

| Port | Use |
|---|---|
| 7465 | Default `serve` port |
| 7480 | Josh's local server. `artifacts/redeploy.sh` rebuilds and restarts it whenever `main` moves |
| 7476–7479, 7481–7489 | Per-thread test servers. The coordinator assigns one in each brief |

## Integrating to main

Integrate small and often:

```sh
jj rebase -d main                                           # bring your work up to date
cargo build --all-features && cargo test && cargo clippy --all-features
(cd crates/wirewolf-web/ui-shadcn && pnpm test)             # if you touched the dashboard
jj bookmark set main -r @-                                  # or @ if @ is the finished change
```

`main` only moves forward. If `jj bookmark set` refuses because `main` moved, rebase again.
Don't rewrite commits that are already on `main`.
