//! The menu bar, and what its items (and the status item's) do.
//!
//! Items that act on the dashboard send it an event (`navigate`, `run-speed`, `copy-brief`,
//! `toggle-sidebar`, `reload`, `download`), which `lib/desktop.ts` in the dashboard handles.
//! Shortcuts are the menu's accelerators, so they work wherever focus is.

use tauri::menu::{AboutMetadataBuilder, CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;

/// Menu items the handlers update later.
pub struct Items {
    autostart: CheckMenuItem<tauri::DynRuntime>,
}

pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let about = AboutMetadataBuilder::new()
        .name(Some("Wirewolf"))
        .version(Some(env!("CARGO_PKG_VERSION")))
        .comments(Some("See what's wrong with your network."))
        .website(Some("https://github.com/joshka/wirewolf"))
        .website_label(Some("github.com/joshka/wirewolf"))
        .license(Some("MIT or Apache-2.0"))
        .build();
    let autostart = CheckMenuItem::with_id(app, "autostart", "Launch at Login", true, app.autostart().is_enabled().unwrap_or(false), None::<&str>)?;
    let item = |id: &str, text: &str, key: Option<&str>| MenuItem::with_id(app, id, text, true, key);
    let sep = || PredefinedMenuItem::separator(app);

    let app_menu = Submenu::with_items(
        app,
        "Wirewolf",
        true,
        &[
            &PredefinedMenuItem::about(app, Some("About Wirewolf"), Some(about))?,
            &sep()?,
            &item("go:/settings", "Settings…", Some("Cmd+,"))?,
            &autostart,
            &sep()?,
            &PredefinedMenuItem::services(app, None)?,
            &sep()?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &sep()?,
            // Not the predefined Quit: closing the window keeps the app running, so Quit must
            // ask for an exit explicitly (`handle`).
            &item("quit", "Quit Wirewolf", Some("Cmd+Q"))?,
        ],
    )?;
    let export = Submenu::with_items(
        app,
        "Export",
        true,
        &[&item("export:history", "History as CSV", None)?, &item("export:incidents", "Incidents as CSV", None)?, &item("export:speed", "Speed Tests as CSV", None)?],
    )?;
    let file = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &item("run-speed", "Run Speed Test", Some("Cmd+T"))?,
            &item("copy-brief", "Copy Brief for AI", Some("Cmd+Shift+C"))?,
            &item("go:/report", "Connection Report", Some("Cmd+Shift+R"))?,
            &sep()?,
            &export,
            &sep()?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &sep()?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;
    let view = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &item("go:/", "Now", Some("Cmd+1"))?,
            &item("go:/history", "History", Some("Cmd+2"))?,
            &item("go:/incidents", "Incidents", Some("Cmd+3"))?,
            &item("go:/speed", "Speed", Some("Cmd+4"))?,
            &item("go:/logs", "Logs", Some("Cmd+5"))?,
            &sep()?,
            &item("toggle-sidebar", "Toggle Sidebar", Some("Cmd+B"))?,
            &item("reload", "Reload", Some("Cmd+R"))?,
            &sep()?,
            &PredefinedMenuItem::fullscreen(app, None)?,
        ],
    )?;
    let window = Submenu::with_items(app, "Window", true, &[&PredefinedMenuItem::minimize(app, None)?, &PredefinedMenuItem::maximize(app, Some("Zoom"))?])?;
    let help = Submenu::with_items(app, "Help", true, &[&item("help", "Wirewolf Help", None)?, &item("issue", "Report an Issue", None)?])?;
    let menu = Menu::with_items(app, &[&app_menu, &file, &edit, &view, &window, &help])?;
    app.set_menu(menu)?;
    app.manage(Items { autostart });
    app.on_menu_event(|app, event| handle(app, event.id().as_ref()));
    Ok(())
}

/// Runs a menu or status item by id.
pub fn handle(app: &AppHandle, id: &str) {
    match id {
        "open" => crate::show(app),
        "quit" => app.exit(0),
        "autostart" => {
            let launcher = app.autostart();
            let result = if launcher.is_enabled().unwrap_or(false) { launcher.disable() } else { launcher.enable() };
            if let Err(e) = result {
                tracing::warn!("launch at login: {e}");
            }
            if let Some(items) = app.try_state::<Items>() {
                let _ = items.autostart.set_checked(launcher.is_enabled().unwrap_or(false));
            }
        }
        "help" => crate::open_in_browser(crate::DOCS_URL),
        "issue" => crate::open_in_browser(crate::ISSUES_URL),
        "run-speed" | "copy-brief" | "toggle-sidebar" | "reload" => {
            crate::show(app);
            let _ = app.emit(id, ());
        }
        _ if id.starts_with("go:") => {
            crate::show(app);
            let _ = app.emit("navigate", &id["go:".len()..]);
        }
        _ if id.starts_with("export:") => {
            let _ = app.emit("download", format!("/api/v1/export/{}.csv", &id["export:".len()..]));
        }
        _ => {}
    }
}
