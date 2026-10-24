//! Builds the `ui-shadcn` front end (Vite) when that feature is enabled.
//!
//! Without the feature this does nothing, so plain `cargo build` never needs Node. Set
//! `WIREWOLF_UI_SHADCN_PREBUILT=1` to skip the Node build and embed an existing
//! `ui-shadcn/dist` (for example one built in an earlier container stage).

use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo::rerun-if-env-changed=WIREWOLF_UI_SHADCN_PREBUILT");
    if std::env::var_os("CARGO_FEATURE_UI_SHADCN").is_none() {
        return;
    }
    let ui = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui-shadcn");
    for input in [
        "src",
        "public",
        "index.html",
        "package.json",
        "pnpm-lock.yaml",
        "vite.config.ts",
        "tsconfig.json",
        "tsconfig.app.json",
    ] {
        println!("cargo::rerun-if-changed=ui-shadcn/{input}");
    }
    // A published crate ships the built dashboard (ui-shadcn/dist) but not its sources, so
    // `cargo install wirewolf` needs no Node.
    let packaged = !ui.join("package.json").exists();
    if packaged || std::env::var_os("WIREWOLF_UI_SHADCN_PREBUILT").is_some() {
        assert!(
            ui.join("dist/index.html").exists(),
            "ui-shadcn/dist/index.html is missing (WIREWOLF_UI_SHADCN_PREBUILT is set, or the sources aren't here)"
        );
        return;
    }
    // pnpm writes node_modules/.modules.yaml on install; reinstall when the lockfile is newer.
    let installed = std::fs::metadata(ui.join("node_modules/.modules.yaml")).and_then(|m| m.modified()).ok();
    let locked = std::fs::metadata(ui.join("pnpm-lock.yaml")).and_then(|m| m.modified()).ok();
    if installed.is_none() || locked > installed {
        pnpm(&ui, &["install", "--frozen-lockfile"]);
    }
    pnpm(&ui, &["run", "build"]);
}

fn pnpm(dir: &Path, args: &[&str]) {
    let status = Command::new("pnpm").args(args).current_dir(dir).status().unwrap_or_else(|e| {
        panic!(
            "feature `ui-shadcn` needs Node and pnpm at build time (failed to run pnpm: {e}). \
             Install them, or build without the feature."
        )
    });
    assert!(status.success(), "`pnpm {}` failed in {}", args.join(" "), dir.display());
}
