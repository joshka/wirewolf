//! The verdict outside the window: the status item in the menu bar, the Dock badge, and
//! notifications when incidents open and close.
//!
//! The status item shows the wolf; while the network is degraded or down it turns amber or red
//! and says so beside the icon. Its menu has the headline, and the commands worth reaching
//! without the window. The Dock badge is the number of open incidents.

use std::collections::HashMap;

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::broadcast::error::RecvError;
use wirewolf_core::Hub;
use wirewolf_core::model::{Event, Incident, Severity, Snapshot};

const WOLF: &[u8] = include_bytes!("../icons/tray.png");
const WOLF_WARN: &[u8] = include_bytes!("../icons/tray-warn.png");
const WOLF_FAIL: &[u8] = include_bytes!("../icons/tray-fail.png");

struct Tray {
    icon: TrayIcon,
    headline: MenuItem<tauri::DynRuntime>,
}

/// Builds the status item and keeps it, the badge, and notifications in step with the hub.
pub fn watch(app: AppHandle, hub: Hub) -> tauri::Result<()> {
    let tray = build(&app)?;
    // Subscribe before reading, so nothing between the two is missed.
    let mut events = hub.subscribe();
    let mut incidents: HashMap<String, Incident> = hub.incidents().into_iter().map(|i| (i.id.clone(), i)).collect();
    apply(&app, &tray, hub.snapshot().as_ref(), open(&incidents));
    crate::server::runtime().spawn(async move {
        loop {
            match events.recv().await {
                Ok(Event::Snapshot(snapshot)) => apply(&app, &tray, Some(&snapshot), open(&incidents)),
                Ok(Event::Incident(incident)) => {
                    notify(&app, incidents.get(&incident.id), &incident);
                    incidents.insert(incident.id.clone(), incident);
                    apply(&app, &tray, hub.snapshot().as_ref(), open(&incidents));
                }
                Ok(_) | Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => break,
            }
        }
    });
    Ok(())
}

fn open(incidents: &HashMap<String, Incident>) -> usize {
    incidents.values().filter(|i| i.ended.is_none()).count()
}

fn build(app: &AppHandle) -> tauri::Result<Tray> {
    let headline = MenuItem::with_id(app, "headline", "Measuring…", false, None::<&str>)?;
    let item = |id: &str, text: &str| MenuItem::with_id(app, id, text, true, None::<&str>);
    let menu = Menu::with_items(
        app,
        &[
            &headline,
            &PredefinedMenuItem::separator(app)?,
            &item("open", "Open Wirewolf")?,
            &item("run-speed", "Run Speed Test")?,
            &item("copy-brief", "Copy Brief for AI")?,
            &PredefinedMenuItem::separator(app)?,
            &item("quit", "Quit Wirewolf")?,
        ],
    )?;
    let icon = TrayIconBuilder::with_id("status")
        .icon(Image::from_bytes(WOLF)?)
        .icon_as_template(true)
        .tooltip("Wirewolf")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| crate::menu::handle(app, event.id().as_ref()))
        .build(app)?;
    Ok(Tray { icon, headline })
}

/// Shows the verdict in the status item and the open incidents on the Dock icon.
fn apply(app: &AppHandle, tray: &Tray, snapshot: Option<&Snapshot>, open: usize) {
    let (bytes, template, title) = match snapshot.map(|s| s.verdict.severity) {
        Some(Severity::Degraded) => (WOLF_WARN, false, Some("Degraded")),
        Some(Severity::Down) => (WOLF_FAIL, false, Some("Down")),
        _ => (WOLF, true, None),
    };
    let headline = snapshot.map_or("Measuring…", |s| s.verdict.headline.as_str());
    if let Ok(image) = Image::from_bytes(bytes) {
        let _ = tray.icon.set_icon(Some(image));
    }
    let _ = tray.icon.set_icon_as_template(template);
    let _ = tray.icon.set_title(title);
    let _ = tray.icon.set_tooltip(Some(headline));
    let _ = tray.headline.set_text(headline);
    if let Some(window) = app.get_webview_window(crate::WINDOW) {
        let _ = window.set_badge_count((open > 0).then_some(open as i64));
    }
}

/// A notification when an incident opens, gets worse, or closes. Nothing for the incidents
/// loaded from history at startup, or for updates that change neither.
fn notify(app: &AppHandle, before: Option<&Incident>, now: &Incident) {
    let (title, body) = match (before, &now.ended) {
        (None, None) => (format!("Network {}", label(now.severity)), now.headline.clone()),
        (Some(b), None) if b.ended.is_none() && rank(now.severity) > rank(b.severity) => (format!("Now {}", label(now.severity)), now.headline.clone()),
        (Some(b), Some(ended)) if b.ended.is_none() => {
            let mins = ended.duration_since(now.started).as_mins();
            let lasted = if mins < 1 { "under a minute".to_string() } else if mins < 120 { format!("{mins} min") } else { format!("{:.1} h", mins as f64 / 60.0) };
            ("Back to normal".to_string(), format!("{} ({lasted})", now.headline))
        }
        _ => return,
    };
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        tracing::debug!("notification: {e}");
    }
}

fn label(severity: Severity) -> &'static str {
    match severity {
        Severity::Healthy => "healthy",
        Severity::Degraded => "degraded",
        Severity::Down => "down",
        Severity::Unknown => "unknown",
    }
}

fn rank(severity: Severity) -> u8 {
    match severity {
        Severity::Healthy => 0,
        Severity::Unknown => 1,
        Severity::Degraded => 2,
        Severity::Down => 3,
    }
}
