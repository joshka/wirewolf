# wirewolf

Network diagnostics you can point a browser at. Rust workspace in `crates/`.

- User docs: `README.md`, then `docs/` (guide, deploy, API, dashboard, CLI reference).
- Dev history and design notes: `docs/dev/` (`design-v1.md` is the original design).
- API contract: `docs/api.md`. Dashboard: `docs/ui.md`.
- Parallel work and integration rules: `docs/dev/workstreams.md`.
- Regenerate the CLI reference after changing flags: `WIREWOLF_BLESS=1 cargo test -p wirewolf-cli --test cli_reference`.
- Run the demo: `cargo run -- serve --demo`. Then open http://127.0.0.1:7465.
- Before integrating: `cargo build --all-features && cargo test && cargo clippy --all-features`.

## Images

- **Screenshots the docs use** live in `docs/images/` and go through Git LFS. Take them from
  `serve --demo` data (RFC 5737 addresses, the `wirewolf-demo` host), never from a live network.
- **Throwaway and comparison screenshots** go in `artifacts/` (git-ignored), e.g.
  `artifacts/compare/<variant>/…`. Don't commit them.
- Raster images in the repo (site images the UI ships, and `docs/images/`) go through Git LFS
  (`.gitattributes`). jj does not run LFS filters, so add or change those files with `git add` /
  `git commit` (the repo is colocated), not with jj. Check `git lfs ls-files` lists them. SVGs
  stay as plain text.

## Commit messages

Tim Pope / Chris Beams style, not conventional commits:

- A capitalized, imperative subject of 50 characters or fewer, with no trailing period
  ("Add live probes", not "feat(probe): added live probes.").
- A blank line, then a body wrapped at 72 columns that explains what changed and why.
  Small, self-explanatory changes may omit the body.
