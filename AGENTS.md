# wirewolf

Network diagnostics you can point a browser at. Rust workspace in `crates/`.

- Design: `docs/design-v1.md`. API contract: `docs/api.md`. UI brief: `docs/ui-brief.md`.
- Parallel work and integration rules: `docs/workstreams.md`.
- Run the demo: `cargo run -- serve --demo`. Then open http://127.0.0.1:7465.
- Before integrating: `cargo build --all-features && cargo test && cargo clippy --all-features`.

## Images

- **No screenshots or other non-site images in the repo, ever.** Put them in `artifacts/` (git-ignored),
  e.g. `artifacts/compare/<variant>/…`. Reference them from docs by path; don't commit them.
- Site raster images (ones the UI actually ships) go through Git LFS (`.gitattributes`). jj does not
  run LFS filters, so add or change those files with `git add` / `git commit` (the repo is colocated),
  not with jj. SVGs stay as plain text.
