# wirewolf

Network diagnostics you can point a browser at. Rust workspace in `crates/`.

- User docs: `README.md`, then `docs/` (guide, deploy, design, API, dashboard, CLI reference).
- API contract: `docs/api.md`; the Rust types in `crates/wirewolf-core/src/model.rs` are the
  source of truth. Keep changes additive.
- Regenerate the CLI reference after changing flags: `WIREWOLF_BLESS=1 cargo test -p wirewolf-cli --test cli_reference`.
- Run the demo: `cargo run -- serve --demo`. Then open http://127.0.0.1:7465.
- Before integrating: `cargo build --all-features && cargo test && cargo clippy --all-features`,
  plus `pnpm test` in `crates/wirewolf-web/ui-shadcn` if you touched the dashboard.
- Local, machine-specific instructions may live in an ignored `local/AGENTS.md` at the root of the
  main checkout (for a workspace under `work/<name>`, that's `../../local/AGENTS.md`). Read it if
  it exists.

## Docs

Docs are public. Refer to "the maintainer", not a person, and use only documentation addresses
(RFC 5737, RFC 3849) and `example.com` names, never real network values.

## Images

- **Screenshots the docs use** live in `docs/images/` and go through Git LFS. Take them from
  `serve --demo` data, never from a live network, and check they show no real public addresses.
- **Throwaway and comparison screenshots** go in `artifacts/` (git-ignored). Don't commit them.
- Raster images in the repo (site images the UI ships, and `docs/images/`) go through Git LFS
  (`.gitattributes`). Check `git lfs ls-files` lists them. SVGs stay as plain text.

## Commit messages

Tim Pope / Chris Beams style, not conventional commits:

- A capitalized, imperative subject of 50 characters or fewer, with no trailing period
  ("Add live probes", not "feat(probe): added live probes.").
- A blank line, then a body wrapped at 72 columns that explains what changed and why.
  Small, self-explanatory changes may omit the body.

## Changelog

Add a line under `[Unreleased]` in `CHANGELOG.md` in the same commit as any change a user would
notice. See [CONTRIBUTING.md](CONTRIBUTING.md#changelog) for what a good entry says.
