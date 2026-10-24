# wirewolf

Network diagnostics you can point a browser at. Rust workspace in `crates/`.

- Design: `docs/design-v1.md`. API contract: `docs/api.md`. UI brief: `docs/ui-brief.md`.
- Parallel work and integration rules: `docs/workstreams.md`.
- Run the demo: `cargo run -- serve --demo`. Then open http://127.0.0.1:7465.
- Before integrating: `cargo build --all-features && cargo test && cargo clippy --all-features`.
