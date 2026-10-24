//! Server logs for the dashboard: a bounded, redacted buffer of recent log lines, fed by a
//! `tracing` layer, and the switch that changes the log filter while running.
//!
//! The binary installs [`Logs::layer`] next to its usual formatter and hands the filter's reload
//! handle to [`Logs::set_reloader`]. The API reads [`Logs::lines`], follows [`Logs::subscribe`],
//! and changes the filter with [`Logs::set_filter`].
//!
//! Levels, sized for a probe cycle every 30 seconds and a [`CAPACITY`]-line buffer:
//!
//! - `error`: wirewolf can't do its job, such as an unwritable database. Once, not per attempt.
//! - `warn`: something degraded or misconfigured that a person should look at. Log the change
//!   (it started failing, it recovered), never every cycle.
//! - `info`: rare lifecycle events: startup, settings applied, verdict changes, speed results.
//!   A healthy server writes a few lines an hour.
//! - `debug`: one summary line per cycle, plus status changes; about two hours fit the buffer.
//! - `trace`: per-check and per-request detail.
//!
//! User-facing events belong in the activity timeline, not here. Never log per request or per
//! live event above `debug`.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex, RwLock};

use jiff::Timestamp;
use tokio::sync::broadcast;
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::Context;

use crate::model::LogLine;

/// How many lines to keep.
pub const CAPACITY: usize = 2000;

/// The filter used when neither `log.filter` nor `RUST_LOG` says otherwise.
pub const DEFAULT_FILTER: &str = "info";

type Reloader = Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

pub struct Logs {
    ring: Mutex<Ring>,
    tx: broadcast::Sender<LogLine>,
    filter: RwLock<String>,
    reloader: Mutex<Option<Reloader>>,
    secrets: RwLock<Vec<String>>,
}

struct Ring {
    lines: VecDeque<LogLine>,
    capacity: usize,
    next: u64,
}

impl std::fmt::Debug for Logs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Logs").field("filter", &self.filter()).finish_non_exhaustive()
    }
}

impl Logs {
    pub fn new() -> Arc<Self> {
        Self::with_capacity(CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Arc<Self> {
        let (tx, _) = broadcast::channel(256);
        Arc::new(Self {
            ring: Mutex::new(Ring { lines: VecDeque::with_capacity(capacity.min(CAPACITY)), capacity: capacity.max(1), next: 1 }),
            tx,
            filter: RwLock::new(DEFAULT_FILTER.into()),
            reloader: Mutex::new(None),
            secrets: RwLock::new(Vec::new()),
        })
    }

    /// The `tracing` layer that records into this buffer.
    pub fn layer(self: &Arc<Self>) -> Layer {
        Layer { logs: self.clone() }
    }

    /// Lets [`Logs::set_filter`] change the live filter, which is currently `filter`.
    pub fn set_reloader(&self, filter: &str, reload: impl Fn(&str) -> Result<(), String> + Send + Sync + 'static) {
        *self.filter.write().unwrap() = filter.into();
        *self.reloader.lock().unwrap() = Some(Box::new(reload));
    }

    /// Values that must never appear in a line, such as the OIDC client secret.
    pub fn add_secret(&self, secret: &str) {
        if secret.len() >= 4 {
            self.secrets.write().unwrap().push(secret.into());
        }
    }

    /// The filter in effect, in `RUST_LOG` syntax.
    pub fn filter(&self) -> String {
        self.filter.read().unwrap().clone()
    }

    /// Checks and applies a new filter, for example `info,wirewolf_probe=debug`.
    pub fn set_filter(&self, filter: &str) -> Result<(), String> {
        let filter = filter.trim();
        check_filter(filter)?;
        if let Some(reload) = &*self.reloader.lock().unwrap() {
            reload(filter)?;
        }
        *self.filter.write().unwrap() = filter.into();
        Ok(())
    }

    /// Buffered lines after `after` (a [`LogLine::seq`]), oldest first.
    pub fn lines(&self, after: u64) -> Vec<LogLine> {
        self.ring.lock().unwrap().lines.iter().filter(|l| l.seq > after).cloned().collect()
    }

    pub fn capacity(&self) -> usize {
        self.ring.lock().unwrap().capacity
    }

    pub fn subscribe(&self) -> broadcast::Receiver<LogLine> {
        self.tx.subscribe()
    }

    /// Adds a line, redacted. Normally the [`Layer`] calls this.
    pub fn push(&self, level: &str, target: &str, message: &str) {
        let message = self.redact(message);
        let line = {
            let mut ring = self.ring.lock().unwrap();
            let line = LogLine { seq: ring.next, at: Timestamp::now(), level: level.to_ascii_lowercase(), target: target.into(), message };
            ring.next += 1;
            if ring.lines.len() >= ring.capacity {
                ring.lines.pop_front();
            }
            ring.lines.push_back(line.clone());
            line
        };
        let _ = self.tx.send(line);
    }

    fn redact(&self, message: &str) -> String {
        let mut out = redact(message);
        for secret in self.secrets.read().unwrap().iter() {
            out = out.replace(secret.as_str(), "***");
        }
        out
    }
}

/// Checks a filter in `RUST_LOG` syntax.
pub fn check_filter(filter: &str) -> Result<(), String> {
    if filter.trim().is_empty() {
        return Err("needs a level, for example \"info\"".into());
    }
    tracing_subscriber::EnvFilter::builder().parse(filter.trim()).map(|_| ()).map_err(|e| e.to_string())
}

/// Words whose `name=value` or `name: value` value is always hidden.
const SECRET_KEYS: &[&str] = &["secret", "password", "passwd", "token", "cookie", "csrf", "authorization", "session", "code", "key"];

/// Hides credentials in a log line: URL user info, and the values of secret-looking keys
/// (`client_secret=…`, `Cookie: …`, `X-Wirewolf-CSRF: …`, `?token=…`).
pub fn redact(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    for (i, word) in message.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&redact_word(word));
    }
    // `Name: value` with a space after the colon: hide the next word.
    let mut words: Vec<String> = out.split(' ').map(str::to_string).collect();
    for i in 1..words.len() {
        let prev = words[i - 1].to_ascii_lowercase();
        if let Some(name) = prev.strip_suffix(':')
            && secret_key(name)
            && !words[i].is_empty()
        {
            words[i] = "***".into();
        }
    }
    words.join(" ")
}

fn secret_key(name: &str) -> bool {
    let name = name.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-').to_ascii_lowercase();
    // `code` and `key` alone are too common in prose; only exact names or suffixes count.
    SECRET_KEYS.iter().any(|k| match *k {
        "code" | "key" => name == *k || name.ends_with(&format!("_{k}")) || name.ends_with(&format!("-{k}")),
        _ => name.contains(k),
    })
}

fn redact_word(word: &str) -> String {
    let mut w = word.to_string();
    // scheme://user:pass@host
    if let Some(start) = w.find("://") {
        let rest = &w[start + 3..];
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        if let Some(at) = rest[..end].rfind('@') {
            w = format!("{}://***@{}", &w[..start], &rest[at + 1..]);
        }
    }
    // name=value pairs, including query strings (`?a=1&token=2`).
    let mut out = String::with_capacity(w.len());
    let mut rest = w.as_str();
    while let Some(eq) = rest.find('=') {
        let (head, tail) = rest.split_at(eq);
        let name_start = head.rfind(['?', '&', ',', ';', '(', '{', '"', '\'']).map_or(0, |i| i + 1);
        out.push_str(head);
        out.push('=');
        let tail = &tail[1..];
        let end = tail.find(['&', ',', ';', ')', '}', '"', '\'']).unwrap_or(tail.len());
        if secret_key(&head[name_start..]) && end > 0 {
            out.push_str("***");
        } else {
            out.push_str(&tail[..end]);
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// Records events into a [`Logs`].
pub struct Layer {
    logs: Arc<Logs>,
}

/// A span's fields, formatted once when it opens.
struct SpanFields(String);

impl<S> tracing_subscriber::Layer<S> for Layer
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &tracing::span::Attributes<'_>, id: &tracing::span::Id, ctx: Context<'_, S>) {
        let mut visitor = Message::default();
        attrs.record(&mut visitor);
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(SpanFields(visitor.text));
        }
    }

    fn on_record(&self, id: &tracing::span::Id, values: &tracing::span::Record<'_>, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else { return };
        let mut ext = span.extensions_mut();
        if let Some(fields) = ext.get_mut::<SpanFields>() {
            let mut visitor = Message { text: std::mem::take(&mut fields.0) };
            values.record(&mut visitor);
            fields.0 = visitor.text;
        }
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        // Like the console: `outer{a=1}:inner: message k=v`.
        let mut text = String::new();
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                text.push_str(span.name());
                if let Some(SpanFields(f)) = span.extensions().get::<SpanFields>()
                    && !f.is_empty()
                {
                    let _ = write!(text, "{{{f}}}");
                }
                text.push_str(": ");
            }
        }
        let mut visitor = Message::default();
        event.record(&mut visitor);
        text.push_str(&visitor.text);
        let meta = event.metadata();
        self.logs.push(meta.level().as_str(), meta.target(), &text);
    }
}

#[derive(Default)]
struct Message {
    text: String,
}

impl Visit for Message {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let fields = std::mem::take(&mut self.text);
            let _ = write!(self.text, "{value:?}");
            if !fields.is_empty() {
                self.text.push(' ');
                self.text.push_str(&fields);
            }
        } else {
            if !self.text.is_empty() {
                self.text.push(' ');
            }
            let _ = write!(self.text, "{}={value:?}", field.name());
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.record_debug(field, &format_args!("{value}"));
        } else {
            self.record_debug(field, &value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_credentials() {
        assert_eq!(redact("fetching https://admin:hunter2@router.example.com/x"), "fetching https://***@router.example.com/x");
        assert_eq!(redact("client_secret=abc123 client_id=wolf"), "client_secret=*** client_id=wolf");
        assert_eq!(redact("GET /auth/callback?code=xyz&state=s1"), "GET /auth/callback?code=***&state=s1");
        assert_eq!(redact("Cookie: wirewolf_session=abc"), "Cookie: ***");
        assert_eq!(redact("header X-Wirewolf-CSRF: deadbeef sent"), "header X-Wirewolf-CSRF: *** sent");
        assert_eq!(redact("sent {\"token\":\"x\"} ok"), "sent {\"token\":\"x\"} ok");
        assert_eq!(redact("icmp to gateway failed, falling back to tcp: timed out"), "icmp to gateway failed, falling back to tcp: timed out");
        assert_eq!(redact("pruned 4 rows; key=v"), "pruned 4 rows; key=***");
    }

    #[test]
    fn known_secrets_are_hidden_anywhere() {
        let logs = Logs::new();
        logs.add_secret("s3cr3t-value");
        logs.push("WARN", "wirewolf_web::auth", "token exchange failed for s3cr3t-value");
        assert_eq!(logs.lines(0)[0].message, "token exchange failed for ***");
        assert_eq!(logs.lines(0)[0].level, "warn");
    }

    #[test]
    fn ring_is_bounded_and_numbered() {
        let logs = Logs::with_capacity(3);
        for i in 0..5 {
            logs.push("INFO", "t", &format!("line {i}"));
        }
        let lines = logs.lines(0);
        assert_eq!(lines.iter().map(|l| l.seq).collect::<Vec<_>>(), [3, 4, 5]);
        assert_eq!(lines[0].message, "line 2");
        assert_eq!(logs.lines(4).len(), 1);
    }

    #[test]
    fn filters_are_checked_before_reloading() {
        let logs = Logs::new();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        logs.set_reloader("info", move |f| {
            s.lock().unwrap().push(f.to_string());
            Ok(())
        });
        assert!(logs.set_filter("info,wirewolf_probe=debug").is_ok());
        assert!(logs.set_filter("info,wirewolf_probe=loud").is_err());
        assert!(logs.set_filter(" ").is_err());
        assert_eq!(logs.filter(), "info,wirewolf_probe=debug");
        assert_eq!(*seen.lock().unwrap(), ["info,wirewolf_probe=debug"]);
    }

    #[test]
    fn layer_records_message_and_fields() {
        use tracing_subscriber::layer::SubscriberExt;
        let logs = Logs::new();
        let subscriber = tracing_subscriber::registry().with(logs.layer());
        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(peer = "192.0.2.1", "refused password=hunter2");
            let _cycle = tracing::info_span!("cycle", n = 3).entered();
            let _hop = tracing::debug_span!("hop", id = "dns").entered();
            tracing::info!(rtt_ms = 2.5, "answered");
        });
        let line = &logs.lines(0)[0];
        assert_eq!(line.level, "warn");
        assert_eq!(line.message, "refused password=*** peer=\"192.0.2.1\"");
        assert!(line.target.starts_with("wirewolf_core"));
        assert_eq!(logs.lines(0)[1].message, "cycle{n=3}: hop{id=\"dns\"}: answered rtt_ms=2.5");
    }
}
