fn main() {
    // The dashboard is a remote page to Tauri (it comes from the app's own server), and remote
    // pages may only call commands a capability names. Listing the commands here generates the
    // `allow-set-theme` permission that capabilities/default.json grants.
    let manifest = tauri_build::AppManifest::new().commands(&["set_theme"]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest)).expect("tauri build")
}
