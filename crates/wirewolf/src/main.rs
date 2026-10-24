//! wirewolf: network diagnostics you can point a browser at.
//!
//! This is the `wirewolf` binary. Install it with `cargo install wirewolf`, then run
//! `wirewolf serve` and open `http://<host>:7465`, or `wirewolf check` for a one-shot diagnosis.
//! See the [README](https://github.com/joshka/wirewolf) and the user guide in `docs/guide.md`.

fn main() -> anyhow::Result<()> {
    wirewolf_cli::run()
}
