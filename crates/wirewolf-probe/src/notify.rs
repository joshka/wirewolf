//! Notifications: a message to ntfy and/or a webhook when an incident opens, gets worse, or
//! closes (`[notify]` in the config). The settings are re-read for every message, so edits in
//! Settings apply without a restart.
//!
//! The URLs and token are secrets the log filter may not know about (they can change after
//! startup), so nothing here logs them: messages name the channel, and errors drop the URL.

use std::collections::HashMap;
use std::time::Duration;

use serde_json::json;
use wirewolf_core::Hub;
use wirewolf_core::config::Notify;
use wirewolf_core::model::{ActivityKind, Event, Incident, Severity};

const TIMEOUT: Duration = Duration::from_secs(10);

/// What happened to an incident.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Change {
    Opened,
    Worse,
    Resolved,
}

/// A message, ready for any channel.
struct Message {
    event: &'static str,
    title: String,
    body: String,
    /// ntfy priority: 5 for down, 3 for degraded, 2 for resolved.
    priority: u8,
    /// ntfy tags, shown as emoji.
    tag: &'static str,
}

fn severity_word(s: Severity) -> &'static str {
    match s {
        Severity::Down => "Down",
        Severity::Degraded => "Degraded",
        Severity::Healthy => "Healthy",
        Severity::Unknown => "Unknown",
    }
}

fn message(change: Change, i: &Incident, host: &str) -> Message {
    // The blamed hop's or target's name ("ISP", "NAS"), not its id.
    let blamed = i.blame.as_deref().map(|b| {
        let hop = i.evidence.hops.iter().find(|h| h.id == b).map(|h| h.label.as_str());
        hop.or_else(|| i.evidence.targets.iter().find(|t| t.id == b).map(|t| t.name.as_str())).unwrap_or(b)
    });
    let at = blamed.map(|b| format!(" at {b}")).unwrap_or_default();
    match change {
        Change::Opened | Change::Worse => Message {
            event: if change == Change::Opened { "opened" } else { "worse" },
            title: format!("{}: {}", severity_word(i.severity), i.headline),
            body: format!("{}{at} on {host}, since {}.", if change == Change::Opened { "Started" } else { "Got worse" }, i.started.strftime("%H:%M UTC")),
            priority: if i.severity == Severity::Down { 5 } else { 3 },
            tag: if i.severity == Severity::Down { "rotating_light" } else { "warning" },
        },
        Change::Resolved => {
            let secs = i.ended.map(|e| e.duration_since(i.started).as_secs()).unwrap_or(0);
            let took = if secs < 120 { format!("{secs} s") } else if secs < 7200 { format!("{} min", secs / 60) } else { format!("{} h {} min", secs / 3600, secs % 3600 / 60) };
            Message { event: "resolved", title: format!("Resolved: {}", i.headline), body: format!("Back to normal on {host} after {took}."), priority: 2, tag: "white_check_mark" }
        }
    }
}

/// Whether `notify` wants a message about this change.
fn wanted(notify: &Notify, change: Change, i: &Incident) -> bool {
    let min = if notify.min_severity == "down" { Severity::Down } else { Severity::Degraded };
    match change {
        // Resolution follows the opening: only for incidents that were bad enough to announce.
        Change::Resolved => notify.resolved && i.severity >= min,
        _ => i.severity >= min,
    }
}

/// Sends `m` to every configured channel, returning one error per channel that failed.
async fn send(client: &reqwest::Client, notify: &Notify, m: &Message, incident: Option<&Incident>, host: &str) -> Vec<String> {
    let mut errors = Vec::new();
    let check = |name: &str, r: Result<reqwest::Response, reqwest::Error>| match r {
        Ok(res) if res.status().is_success() => None,
        Ok(res) => Some(format!("{name}: HTTP {}", res.status().as_u16())),
        Err(e) => Some(format!("{name}: {}", crate::measure::error_chain(&e.without_url()))),
    };
    if !notify.ntfy.is_empty() {
        let mut req = client
            .post(&notify.ntfy)
            .header("Title", m.title.clone())
            .header("Priority", m.priority.to_string())
            .header("Tags", m.tag)
            .body(m.body.clone());
        if !notify.ntfy_token.is_empty() {
            req = req.bearer_auth(&notify.ntfy_token);
        }
        errors.extend(check("ntfy", req.send().await));
    }
    if !notify.webhook.is_empty() {
        let text = format!("{}\n{}", m.title, m.body);
        let body = json!({ "event": m.event, "text": text, "content": text, "host": host, "incident": incident });
        errors.extend(check("webhook", client.post(&notify.webhook).json(&body).send().await));
    }
    errors
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("wirewolf/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("a basic HTTP client builds")
}

fn channels(n: &Notify) -> Vec<&'static str> {
    [(!n.ntfy.is_empty()).then_some("ntfy"), (!n.webhook.is_empty()).then_some("webhook")].into_iter().flatten().collect()
}

/// Watches incidents and notifies on each change. Runs until the hub is dropped.
pub async fn run(hub: Hub) {
    let client = client();
    let mut events = hub.subscribe();
    // The worst severity announced per open incident, to tell "opened" from "got worse".
    let mut seen: HashMap<String, Severity> = HashMap::new();
    loop {
        let incident = match events.recv().await {
            Ok(Event::Incident(i)) => i,
            Ok(_) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
        };
        let change = match (incident.ended.is_some(), seen.get(&incident.id)) {
            (true, _) => {
                seen.remove(&incident.id);
                Change::Resolved
            }
            (false, None) => Change::Opened,
            (false, Some(was)) if incident.severity > *was => Change::Worse,
            (false, Some(_)) => continue,
        };
        if change != Change::Resolved {
            seen.insert(incident.id.clone(), incident.severity);
        }
        let Some(notify) = hub.config().map(|v| v.config.notify) else { continue };
        if channels(&notify).is_empty() || !wanted(&notify, change, &incident) {
            continue;
        }
        let host = hub.meta().host;
        let m = message(change, &incident, &host);
        let errors = send(&client, &notify, &m, Some(&incident), &host).await;
        for e in &errors {
            tracing::warn!("couldn't send the {} notification for {}: {e}", m.event, incident.id);
        }
        if errors.is_empty() {
            tracing::info!("notified {} ({}) about {}", channels(&notify).join(" and "), m.event, incident.id);
        }
    }
}

/// Sends a test message with the current settings and records the outcome in the activity log.
pub async fn test(hub: Hub, actor: Option<String>) {
    let Some(notify) = hub.config().map(|v| v.config.notify) else { return };
    let names = channels(&notify);
    let message = if names.is_empty() {
        "Test notification not sent: no ntfy or webhook URL is set".to_string()
    } else {
        let host = hub.meta().host;
        let m = Message {
            event: "test",
            title: "wirewolf test notification".into(),
            body: format!("Notifications from {host} work. You'll get one like this when an incident opens or closes."),
            priority: 3,
            tag: "wolf",
        };
        let errors = send(&client(), &notify, &m, None, &host).await;
        if errors.is_empty() { format!("Test notification sent to {}", names.join(" and ")) } else { format!("Test notification failed: {}", errors.join("; ")) }
    };
    tracing::info!("{message}");
    hub.record(ActivityKind::Config, message, None, actor);
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::Timestamp;
    use wirewolf_core::model::{Confidence, Snapshot, Verdict};

    fn incident(severity: Severity, ended: Option<i64>) -> Incident {
        let at = Timestamp::from_second(0).unwrap();
        let verdict = Verdict { severity, headline: "h".into(), detail: String::new(), blame: Some("isp".into()), confidence: Confidence::High, fix_hint: None, method: String::new() };
        Incident {
            id: "inc-1".into(),
            started: at,
            ended: ended.map(|s| Timestamp::from_second(s).unwrap()),
            severity,
            headline: "Your internet provider's connection is down".into(),
            blame: Some("isp".into()),
            evidence: Snapshot { at, verdict, hops: vec![], open_incident: None, targets: vec![] },
        }
    }

    #[test]
    fn min_severity_filters_degraded() {
        let mut n = Notify { min_severity: "down".into(), ..Notify::default() };
        assert!(!wanted(&n, Change::Opened, &incident(Severity::Degraded, None)));
        assert!(wanted(&n, Change::Opened, &incident(Severity::Down, None)));
        n.resolved = false;
        assert!(!wanted(&n, Change::Resolved, &incident(Severity::Down, Some(60))));
    }

    #[test]
    fn messages_say_what_and_how_long() {
        let opened = message(Change::Opened, &incident(Severity::Down, None), "nas");
        assert_eq!(opened.title, "Down: Your internet provider's connection is down");
        assert_eq!(opened.priority, 5);
        let resolved = message(Change::Resolved, &incident(Severity::Down, Some(360)), "nas");
        assert_eq!(resolved.body, "Back to normal on nas after 6 min.");
    }
}
