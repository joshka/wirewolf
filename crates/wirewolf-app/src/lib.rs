//! The wirewolf desktop app.
//!
//! The server that `wirewolf serve` runs, in-process and listening on a random loopback port,
//! with the dashboard in a native window. Around it, what a Mac app is expected to have: a menu
//! bar with the app's commands and their shortcuts, a status item in the menu bar that shows the
//! verdict, a Dock badge with the number of open incidents, notifications when incidents open
//! and close, downloads that land in Downloads, links that open in the browser, and a window that
//! remembers its size and place. Closing the window keeps the probes running; the status item,
//! the Dock icon, or launching the app again brings it back.
//!
//! Settings and history live in the app's data directory
//! (`~/Library/Application Support/net.joshka.wirewolf` on macOS). `--demo [--scenario <id>]`
//! shows scripted data instead of probing the network.
//!
//! This crate is part of [wirewolf](https://github.com/joshka/wirewolf) and is built with
//! `cargo tauri build` (see `docs/app.md`). It isn't published.

mod menu;
mod server;
mod status;

use std::path::PathBuf;

use tauri::webview::{DownloadEvent, NewWindowResponse};
use tauri::{AppHandle, Manager, RunEvent, TitleBarStyle, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_notification::NotificationExt;

/// The label of the dashboard window.
pub const WINDOW: &str = "main";

/// Where the docs live, for the Help menu.
pub const DOCS_URL: &str = "https://github.com/joshka/wirewolf/blob/main/docs/guide.md";
pub const ISSUES_URL: &str = "https://github.com/joshka/wirewolf/issues/new";

/// Command line options: `--demo` and `--scenario <id>`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    /// Serve scripted demo data instead of probing the network.
    pub demo: bool,
    /// The demo scenario to start with (`healthy` by default).
    pub scenario: Option<String>,
}

impl Options {
    pub fn parse(args: impl IntoIterator<Item = String>) -> anyhow::Result<Self> {
        let mut options = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--demo" => options.demo = true,
                "--scenario" => options.scenario = Some(args.next().ok_or_else(|| anyhow::anyhow!("--scenario needs a value"))?),
                s if s.starts_with("--scenario=") => options.scenario = Some(s["--scenario=".len()..].to_string()),
                // macOS passes `-psn_...` to apps opened from the Finder on older systems.
                s if s.starts_with("-psn_") => {}
                other => anyhow::bail!("unknown argument {other:?} (the app takes --demo and --scenario <id>)"),
            }
        }
        if options.scenario.is_some() && !options.demo {
            anyhow::bail!("--scenario needs --demo");
        }
        Ok(options)
    }
}

pub fn run() -> anyhow::Result<()> {
    let options = Options::parse(std::env::args().skip(1))?;
    tauri::Builder::default()
        .runtime(tauri_runtime_wry::Wry::default())
        // A second launch (double-clicking the app again) brings the running window forward.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show(app)))
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .invoke_handler(tauri::generate_handler![set_theme])
        .setup(move |app| {
            let data = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data)?;
            let started = server::start(&data, &options)?;
            tracing::info!("dashboard at {}", started.url);
            menu::install(app.handle())?;
            let origin = started.url.origin();
            let downloads = app.path().download_dir().ok();
            let window = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::External(started.url.clone()))
                .title("Wirewolf")
                .inner_size(1180.0, 760.0)
                .min_inner_size(800.0, 560.0)
                // The traffic lights sit over the sidebar, which leaves room for them (App.tsx).
                .title_bar_style(TitleBarStyle::Overlay)
                .hidden_title(true)
                // The sidebar is translucent (vibrancy, below) so the desktop shows through it.
                .transparent(true)
                // Tells the dashboard it's inside the app before any of its scripts run.
                .initialization_script("window.__WIREWOLF_DESKTOP__ = true")
                // The window shows the local dashboard only; anything else opens in the browser.
                .on_navigation(move |url| {
                    tracing::debug!("navigation to {url}");
                    if url.origin() == origin || url.scheme() == "about" {
                        return true;
                    }
                    open_in_browser(url.as_str());
                    false
                })
                .on_new_window(|url, _features| {
                    open_in_browser(url.as_str());
                    NewWindowResponse::Deny
                })
                // CSV and brief downloads go to Downloads, then Finder shows them.
                .on_download(move |webview, event| {
                    match event {
                        DownloadEvent::Requested { destination, .. } => {
                            if let Some(dir) = &downloads {
                                *destination = unique_path(dir, destination.file_name().map(PathBuf::from).unwrap_or_else(|| "wirewolf-download".into()));
                            }
                        }
                        DownloadEvent::Finished { path: Some(path), success: true, .. } => {
                            let _ = tauri_plugin_opener::reveal_item_in_dir(&path);
                            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                            let _ = webview.app_handle().notification().builder().title("Saved to Downloads").body(name).show();
                        }
                        _ => {}
                    }
                    true
                })
                .build()?;
            #[cfg(target_os = "macos")]
            if let Err(e) = window_vibrancy::apply_vibrancy(&window, window_vibrancy::NSVisualEffectMaterial::Sidebar, None, None) {
                tracing::warn!("no sidebar vibrancy: {e}");
            }
            // The close button hides the window; monitoring continues. Quit is in the menus.
            let handle = app.handle().clone();
            window.on_window_event(move |event| {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    if let Some(w) = handle.get_webview_window(WINDOW) {
                        let _ = w.hide();
                    }
                }
            });
            status::watch(app.handle().clone(), started.hub)?;
            Ok(())
        })
        .build(tauri::generate_context!())?
        .run(|app, event| match event {
            // The Dock icon, with the window hidden.
            RunEvent::Reopen { .. } => show(app),
            // No window closes for good (above), but keep running if one ever does.
            RunEvent::ExitRequested { code: None, api, .. } => api.prevent_exit(),
            _ => {}
        });
    Ok(())
}

/// Shows the dashboard window and brings it to the front.
pub fn show(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub fn open_in_browser(url: &str) {
    if let Err(e) = tauri_plugin_opener::open_url(url, None::<&str>) {
        tracing::warn!("couldn't open {url}: {e}");
    }
}

/// `dir/name`, or `dir/name 2.ext`, `dir/name 3.ext`, … if that exists, as the Finder does.
fn unique_path(dir: &std::path::Path, name: PathBuf) -> PathBuf {
    let first = dir.join(&name);
    if !first.exists() {
        return first;
    }
    let stem = name.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = name.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (2..).map(|n| dir.join(format!("{stem} {n}{ext}"))).find(|p| !p.exists()).expect("unbounded")
}

/// The dashboard's theme choice, so the window's own chrome (vibrancy, dialogs) matches it.
/// `system` or anything unknown follows the system.
#[tauri::command]
fn set_theme(window: tauri::Window, theme: Option<String>) -> Result<(), String> {
    let theme = match theme.as_deref() {
        Some("light") => Some(tauri::Theme::Light),
        Some("dark") => Some(tauri::Theme::Dark),
        _ => None,
    };
    tracing::debug!("the dashboard chose the {} theme", theme.map_or("system".to_string(), |t| format!("{t:?}").to_lowercase()));
    window.set_theme(theme).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> anyhow::Result<Options> {
        Options::parse(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn options() {
        assert_eq!(parse(&[]).unwrap(), Options::default());
        assert_eq!(parse(&["--demo"]).unwrap(), Options { demo: true, scenario: None });
        assert_eq!(parse(&["--demo", "--scenario", "outage"]).unwrap(), Options { demo: true, scenario: Some("outage".into()) });
        assert_eq!(parse(&["--demo", "--scenario=outage"]).unwrap().scenario.as_deref(), Some("outage"));
        assert_eq!(parse(&["-psn_0_1"]).unwrap(), Options::default());
        assert!(parse(&["--scenario", "outage"]).is_err());
        assert!(parse(&["--demo", "--scenario"]).is_err());
        assert!(parse(&["--port"]).is_err());
    }

    #[test]
    fn unique_paths() {
        let dir = std::env::temp_dir().join(format!("wirewolf-app-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(unique_path(&dir, "a.csv".into()), dir.join("a.csv"));
        std::fs::write(dir.join("a.csv"), "").unwrap();
        assert_eq!(unique_path(&dir, "a.csv".into()), dir.join("a 2.csv"));
        std::fs::write(dir.join("a 2.csv"), "").unwrap();
        assert_eq!(unique_path(&dir, "a.csv".into()), dir.join("a 3.csv"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
