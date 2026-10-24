//! The state hub: the one place drivers (demo or live probes) publish into and the API reads
//! from.
//!
//! Drivers call [`Hub::publish_snapshot`], [`Hub::push_sample`], and [`Hub::upsert_speed`]. The
//! hub keeps bounded in-memory history, opens and closes incidents as the verdict changes, and
//! broadcasts every change as an [`Event`]. Clients send requests (run a speed test, switch demo
//! scenario) as [`Command`]s, which the driver receives from [`Hub::take_commands`].
//!
//! The hub also keeps the activity timeline: verdict changes, incidents, and speed tests are
//! recorded as they happen, and drivers and the API add their own with [`Hub::record`].

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, RwLock};

use jiff::Timestamp;
use tokio::sync::{broadcast, mpsc};

use crate::config::Incidents;
use crate::editor::Editor;
use crate::logs::Logs;
use crate::model::{Activity, ActivityKind, ConfigView, Event, Incident, Meta, Sample, Severity, Snapshot, SpeedRun, SpeedState};

/// How many samples to keep in memory (about 48h of 20 hops and targets at one sample per 30s).
const SAMPLE_CAPACITY: usize = 20 * 2 * 60 * 48;
const SPEED_CAPACITY: usize = 500;
const INCIDENT_CAPACITY: usize = 500;
const ACTIVITY_CAPACITY: usize = 1000;

/// A request from a client to the driver.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    RunSpeedTest,
    /// Demo mode only.
    SetScenario(String),
    /// A saved config to use from the next probe cycle on. Live mode only.
    ApplyConfig(Arc<crate::config::Config>),
    /// Send a test notification with the current `[notify]` settings, recording who asked.
    /// Live mode only.
    TestNotification(Option<String>),
}

#[derive(Debug, Clone)]
pub struct Hub {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    state: RwLock<State>,
    events: broadcast::Sender<Event>,
    commands: mpsc::UnboundedSender<Command>,
    command_rx: Mutex<Option<mpsc::UnboundedReceiver<Command>>>,
}

#[derive(Debug)]
struct State {
    meta: Meta,
    snapshot: Option<Snapshot>,
    samples: VecDeque<Sample>,
    speed: VecDeque<SpeedRun>,
    incidents: VecDeque<Incident>,
    next_incident: u64,
    config: Option<ConfigView>,
    editor: Option<Arc<Editor>>,
    logs: Option<Arc<Logs>>,
    activity: VecDeque<Activity>,
    next_activity: u64,
    policy: Incidents,
    /// Unhealthy snapshots in a row toward a new incident (none open, or the open one blames a
    /// different hop): the first one's time, and the worst one.
    pending: Option<(u32, Timestamp, Snapshot)>,
    /// Healthy snapshots in a row while an incident is open, and the first one's time.
    recovering: Option<(u32, Timestamp)>,
}

impl Hub {
    pub fn new(meta: Meta) -> Self {
        let (events, _) = broadcast::channel(1024);
        let (commands, command_rx) = mpsc::unbounded_channel();
        Self {
            inner: Arc::new(Inner {
                state: RwLock::new(State {
                    meta,
                    snapshot: None,
                    samples: VecDeque::new(),
                    speed: VecDeque::new(),
                    incidents: VecDeque::new(),
                    next_incident: 1,
                    config: None,
                    editor: None,
                    logs: None,
                    activity: VecDeque::new(),
                    next_activity: 1,
                    policy: Incidents::default(),
                    pending: None,
                    recovering: None,
                }),
                events,
                commands,
                command_rx: Mutex::new(Some(command_rx)),
            }),
        }
    }

    // ---- reads (API side) ----

    pub fn meta(&self) -> Meta {
        self.read().meta.clone()
    }

    pub fn snapshot(&self) -> Option<Snapshot> {
        self.read().snapshot.clone()
    }

    /// Samples in time order, optionally filtered by hop and time range (inclusive).
    pub fn samples(&self, hop: Option<&str>, since: Option<Timestamp>, until: Option<Timestamp>) -> Vec<Sample> {
        self.read()
            .samples
            .iter()
            .filter(|s| hop.is_none_or(|h| s.hop == h))
            .filter(|s| since.is_none_or(|t| s.at >= t))
            .filter(|s| until.is_none_or(|t| s.at <= t))
            .cloned()
            .collect()
    }

    /// Speed runs, newest first.
    pub fn speed_runs(&self) -> Vec<SpeedRun> {
        self.read().speed.iter().rev().cloned().collect()
    }

    /// Incidents, newest first.
    pub fn incidents(&self) -> Vec<Incident> {
        self.read().incidents.iter().rev().cloned().collect()
    }

    /// The effective configuration, if the driver published one.
    pub fn config(&self) -> Option<ConfigView> {
        self.read().config.clone()
    }

    /// The config editor, if this server can save settings.
    pub fn editor(&self) -> Option<Arc<Editor>> {
        self.read().editor.clone()
    }

    /// The server log buffer, if the binary installed one.
    pub fn logs(&self) -> Option<Arc<Logs>> {
        self.read().logs.clone()
    }

    /// The activity timeline, newest first.
    pub fn activity(&self) -> Vec<Activity> {
        self.read().activity.iter().rev().cloned().collect()
    }

    pub fn incident(&self, id: &str) -> Option<Incident> {
        self.read().incidents.iter().find(|i| i.id == id).cloned()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.inner.events.subscribe()
    }

    pub fn send(&self, command: Command) {
        let _ = self.inner.commands.send(command);
    }

    // ---- writes (driver side) ----

    /// Takes the command receiver. Only the driver should call this, once.
    pub fn take_commands(&self) -> Option<mpsc::UnboundedReceiver<Command>> {
        self.inner.command_rx.lock().unwrap().take()
    }

    pub fn update_meta(&self, f: impl FnOnce(&mut Meta)) {
        f(&mut self.write().meta);
    }

    /// Publishes the effective configuration for `GET /config` and applies its incident rules.
    pub fn set_config(&self, view: ConfigView) {
        let mut state = self.write();
        state.policy = view.config.incidents;
        state.config = Some(view);
    }

    /// Lets `PUT /config` save settings through `editor`.
    pub fn set_editor(&self, editor: Arc<Editor>) {
        self.write().editor = Some(editor);
    }

    /// Lets the API serve recent log lines and change the log filter.
    pub fn set_logs(&self, logs: Arc<Logs>) {
        self.write().logs = Some(logs);
    }

    /// Adds an entry to the activity timeline and broadcasts it.
    pub fn record(&self, kind: ActivityKind, message: impl Into<String>, subject: Option<String>, actor: Option<String>) {
        let entry = {
            let mut state = self.write();
            let entry = state.activity_entry(kind, message.into(), subject, actor);
            push_bounded(&mut state.activity, entry.clone(), ACTIVITY_CAPACITY);
            entry
        };
        let _ = self.inner.events.send(Event::Activity(entry));
    }

    /// Bulk-loads saved activity without broadcasting. Entries must be in time order.
    pub fn load_activity(&self, entries: Vec<Activity>) {
        let mut state = self.write();
        if let Some(max) = entries.iter().map(|a| a.id).max() {
            state.next_activity = state.next_activity.max(max + 1);
        }
        for e in entries {
            push_bounded(&mut state.activity, e, ACTIVITY_CAPACITY);
        }
    }

    /// Sets how many cycles in a row open and close an incident.
    pub fn set_incident_policy(&self, policy: Incidents) {
        self.write().policy = policy;
    }

    /// Publishes a new snapshot, opening, escalating, or closing incidents as the verdict changes.
    ///
    /// An incident opens after `open_after` unhealthy snapshots in a row, backdated to the
    /// first, with the worst of them as evidence. It closes after `close_after` healthy ones,
    /// dated at the first. Shorter blips never become incidents.
    pub fn publish_snapshot(&self, mut snapshot: Snapshot) {
        let mut events = Vec::new();
        {
            let mut guard = self.write();
            let state = &mut *guard;
            let severity = snapshot.verdict.severity;
            let previous = state.snapshot.as_ref().map(|s| s.verdict.severity);
            if previous != Some(severity) {
                let message = match previous {
                    Some(p) => format!("Verdict {} → {}: {}", severity_word(p), severity_word(severity), snapshot.verdict.headline),
                    None => format!("First verdict {}: {}", severity_word(severity), snapshot.verdict.headline),
                };
                state.note(&mut events, ActivityKind::Verdict, message, None);
            }
            let bad = matches!(severity, Severity::Degraded | Severity::Down);
            let open = state.incidents.back().filter(|i| i.ended.is_none()).map(|i| (i.id.clone(), i.blame.clone()));
            match (open, bad) {
                // The blame moved: a different problem, not a continuation. Once the new blame
                // has held for `open_after` cycles, close this incident where the new one began
                // and open the new one, so an incident never tells another problem's story.
                (Some((id, blame)), true) if blame != snapshot.verdict.blame => {
                    state.recovering = None;
                    let (n, start, worst) = state.advance_pending(&snapshot);
                    if n < state.policy.open_after.max(1) {
                        state.pending = Some((n, start, worst));
                        snapshot.open_incident = Some(id);
                    } else {
                        let incident = state.incidents.back_mut().expect("open incident");
                        incident.ended = Some(start);
                        events.push(Event::Incident(incident.clone()));
                        let to = worst.verdict.blame.as_deref().unwrap_or("nothing");
                        let message = format!("Incident {id} closed after {}: the problem moved to {to}", short_duration(start.duration_since(incident.started)));
                        state.note(&mut events, ActivityKind::Incident, message, Some(id));
                        snapshot.open_incident = Some(state.open_incident(&mut events, start, worst));
                    }
                }
                (Some((id, _)), true) => {
                    state.recovering = None;
                    state.pending = None;
                    let incident = state.incidents.back_mut().expect("open incident");
                    if severity > incident.severity {
                        incident.severity = severity;
                        incident.headline = snapshot.verdict.headline.clone();
                        incident.evidence = Snapshot { open_incident: Some(id.clone()), ..snapshot.clone() };
                        events.push(Event::Incident(incident.clone()));
                        let message = format!("Incident {id} got worse ({}): {}", severity_word(severity), snapshot.verdict.headline);
                        state.note(&mut events, ActivityKind::Incident, message, Some(id.clone()));
                    }
                    snapshot.open_incident = Some(id);
                }
                (Some((id, _)), false) => {
                    state.pending = None;
                    let (n, since) = state.recovering.map_or((1, snapshot.at), |(n, t)| (n + 1, t));
                    if n < state.policy.close_after.max(1) {
                        state.recovering = Some((n, since));
                        snapshot.open_incident = Some(id);
                    } else {
                        state.recovering = None;
                        let incident = state.incidents.back_mut().expect("open incident");
                        incident.ended = Some(since);
                        events.push(Event::Incident(incident.clone()));
                        let message = format!("Incident {id} closed after {}", short_duration(since.duration_since(incident.started)));
                        snapshot.open_incident = None;
                        state.note(&mut events, ActivityKind::Incident, message, Some(id));
                    }
                }
                (None, true) => {
                    let (n, start, worst) = state.advance_pending(&snapshot);
                    if n < state.policy.open_after.max(1) {
                        state.pending = Some((n, start, worst));
                        snapshot.open_incident = None;
                    } else {
                        snapshot.open_incident = Some(state.open_incident(&mut events, start, worst));
                    }
                }
                (None, false) => {
                    state.pending = None;
                    snapshot.open_incident = None;
                }
            }
            state.snapshot = Some(snapshot.clone());
        }
        events.insert(0, Event::Snapshot(snapshot));
        for event in events {
            let _ = self.inner.events.send(event);
        }
    }

    pub fn push_sample(&self, sample: Sample) {
        push_bounded(&mut self.write().samples, sample.clone(), SAMPLE_CAPACITY);
        let _ = self.inner.events.send(Event::Sample(sample));
    }

    /// Inserts or replaces (by id) a speed run.
    pub fn upsert_speed(&self, run: SpeedRun) {
        let mut events = vec![Event::Speed(run.clone())];
        {
            let mut state = self.write();
            let before = match state.speed.iter_mut().find(|r| r.id == run.id) {
                Some(existing) => Some(std::mem::replace(existing, run.clone()).state),
                None => {
                    push_bounded(&mut state.speed, run.clone(), SPEED_CAPACITY);
                    None
                }
            };
            if before != Some(run.state) {
                let message = match run.state {
                    SpeedState::Running => format!("Speed test started ({})", run.backend),
                    SpeedState::Complete => {
                        let mbps = |v: Option<f64>| v.map_or("?".into(), |v| format!("{v:.0}"));
                        let bloat = run.bloat.and_then(|b| serde_json::to_value(b).ok()).and_then(|v| v.as_str().map(|s| format!(", bufferbloat {s}"))).unwrap_or_default();
                        format!("Speed test finished: {} down, {} up Mbps{bloat}", mbps(run.down_mbps), mbps(run.up_mbps))
                    }
                    SpeedState::Failed => format!("Speed test failed: {}", run.error.as_deref().unwrap_or("unknown error")),
                };
                state.note(&mut events, ActivityKind::Speed, message, Some(run.id.clone()));
            }
        }
        for event in events {
            let _ = self.inner.events.send(event);
        }
    }

    /// True if a speed test is currently running.
    pub fn speed_running(&self) -> bool {
        self.read().speed.iter().any(|r| r.state == SpeedState::Running)
    }

    /// Bulk-loads history without broadcasting (for backfill at startup). Items must be in time
    /// order.
    pub fn load_history(&self, samples: Vec<Sample>, speed: Vec<SpeedRun>, incidents: Vec<Incident>) {
        let mut state = self.write();
        for s in samples {
            push_bounded(&mut state.samples, s, SAMPLE_CAPACITY);
        }
        for r in speed {
            push_bounded(&mut state.speed, r, SPEED_CAPACITY);
        }
        // Continue numbering after the highest loaded `inc-N`, so ids never repeat even when
        // older incidents were pruned.
        let loaded_max = incidents.iter().filter_map(|i| i.id.strip_prefix("inc-")?.parse::<u64>().ok()).max();
        state.next_incident = match loaded_max {
            Some(max) => state.next_incident.max(max + 1),
            None => state.next_incident + incidents.len() as u64,
        };
        for i in incidents {
            push_bounded(&mut state.incidents, i, INCIDENT_CAPACITY);
        }
    }

    /// Clears the live state (snapshot and any open incident) while keeping history.
    pub fn reset_live(&self) {
        let mut state = self.write();
        state.snapshot = None;
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, State> {
        self.inner.state.read().unwrap()
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, State> {
        self.inner.state.write().unwrap()
    }
}

impl State {
    /// Counts one more unhealthy cycle toward opening an incident, keeping the first one's time
    /// and the worst snapshot.
    fn advance_pending(&mut self, snapshot: &Snapshot) -> (u32, Timestamp, Snapshot) {
        match self.pending.take() {
            Some((n, start, worst)) if worst.verdict.severity >= snapshot.verdict.severity => (n + 1, start, worst),
            Some((n, start, _)) => (n + 1, start, snapshot.clone()),
            None => (1, snapshot.at, snapshot.clone()),
        }
    }

    /// Opens an incident that started at `start`, with `worst` as its evidence, and returns its id.
    fn open_incident(&mut self, events: &mut Vec<Event>, start: Timestamp, worst: Snapshot) -> String {
        let id = format!("inc-{}", self.next_incident);
        self.next_incident += 1;
        let incident = Incident {
            id: id.clone(),
            started: start,
            ended: None,
            severity: worst.verdict.severity,
            headline: worst.verdict.headline.clone(),
            blame: worst.verdict.blame.clone(),
            evidence: Snapshot { open_incident: Some(id.clone()), ..worst },
        };
        let message = format!("Incident {id} opened ({}): {}", severity_word(incident.severity), incident.headline);
        push_bounded(&mut self.incidents, incident.clone(), INCIDENT_CAPACITY);
        events.push(Event::Incident(incident));
        self.note(events, ActivityKind::Incident, message, Some(id.clone()));
        id
    }

    fn activity_entry(&mut self, kind: ActivityKind, message: String, subject: Option<String>, actor: Option<String>) -> Activity {
        let id = self.next_activity;
        self.next_activity += 1;
        Activity { id, at: Timestamp::now(), kind, message, subject, actor }
    }

    /// Records an entry whose event goes out with `events`, after what it describes.
    fn note(&mut self, events: &mut Vec<Event>, kind: ActivityKind, message: String, subject: Option<String>) {
        let entry = self.activity_entry(kind, message, subject, None);
        push_bounded(&mut self.activity, entry.clone(), ACTIVITY_CAPACITY);
        events.push(Event::Activity(entry));
    }
}

/// `45s`, `12m`, `3h 5m`.
fn short_duration(d: jiff::SignedDuration) -> String {
    let s = d.as_secs().max(0);
    match s {
        0..60 => format!("{s}s"),
        60..3600 => format!("{}m", s / 60),
        _ if s % 3600 / 60 == 0 => format!("{}h", s / 3600),
        _ => format!("{}h {}m", s / 3600, s % 3600 / 60),
    }
}

fn severity_word(s: Severity) -> &'static str {
    match s {
        Severity::Healthy => "healthy",
        Severity::Degraded => "degraded",
        Severity::Down => "down",
        Severity::Unknown => "unknown",
    }
}

fn push_bounded<T>(queue: &mut VecDeque<T>, item: T, capacity: usize) {
    if queue.len() == capacity {
        queue.pop_front();
    }
    queue.push_back(item);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Confidence, Verdict};

    fn snap(at: i64, severity: Severity) -> Snapshot {
        Snapshot {
            at: Timestamp::from_second(at).unwrap(),
            verdict: Verdict {
                severity,
                headline: format!("{severity:?}"),
                detail: String::new(),
                blame: None,
                confidence: Confidence::High,
                fix_hint: None,
                method: String::new(),
            },
            hops: vec![],
            open_incident: None,
            targets: vec![],
        }
    }

    fn hub(open_after: u32, close_after: u32) -> Hub {
        let hub = Hub::new(crate::model::Meta {
            version: String::new(),
            api_version: String::new(),
            mode: "live".into(),
            host: String::new(),
            scenarios: vec![],
            scenario: None,
            monitoring_since: None,
        });
        hub.set_incident_policy(Incidents { open_after, close_after });
        hub
    }

    #[test]
    fn one_bad_cycle_is_not_an_incident() {
        let hub = hub(2, 2);
        hub.publish_snapshot(snap(0, Severity::Down));
        hub.publish_snapshot(snap(30, Severity::Healthy));
        assert!(hub.incidents().is_empty());
    }

    #[test]
    fn opens_backdated_and_closes_at_first_good_cycle() {
        let hub = hub(2, 2);
        hub.publish_snapshot(snap(0, Severity::Degraded));
        hub.publish_snapshot(snap(30, Severity::Down));
        let open = hub.incidents();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].started.as_second(), 0);
        assert_eq!(open[0].severity, Severity::Down);
        hub.publish_snapshot(snap(60, Severity::Healthy));
        assert_eq!(hub.incidents()[0].ended, None);
        assert!(hub.snapshot().unwrap().open_incident.is_some());
        // A relapse while recovering keeps the same incident.
        hub.publish_snapshot(snap(90, Severity::Degraded));
        hub.publish_snapshot(snap(120, Severity::Healthy));
        hub.publish_snapshot(snap(150, Severity::Healthy));
        let closed = hub.incidents();
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].ended.unwrap().as_second(), 120);
        assert_eq!(hub.snapshot().unwrap().open_incident, None);
    }

    fn blamed(at: i64, severity: Severity, hop: &str) -> Snapshot {
        let mut s = snap(at, severity);
        s.verdict.blame = Some(hop.into());
        s.verdict.headline = format!("{hop} broke");
        s
    }

    #[test]
    fn moving_blame_closes_the_incident_and_opens_another() {
        let hub = hub(2, 2);
        hub.publish_snapshot(blamed(0, Severity::Down, "isp"));
        hub.publish_snapshot(blamed(30, Severity::Down, "isp"));
        // One cycle blaming DNS isn't enough to move.
        hub.publish_snapshot(blamed(60, Severity::Down, "dns"));
        assert_eq!(hub.incidents().len(), 1);
        hub.publish_snapshot(blamed(90, Severity::Down, "dns"));
        let all = hub.incidents(); // newest first
        assert_eq!(all.len(), 2);
        let (new, old) = (&all[0], &all[1]);
        assert_eq!(old.blame.as_deref(), Some("isp"));
        assert_eq!(old.ended.unwrap().as_second(), 60);
        assert_eq!(new.blame.as_deref(), Some("dns"));
        assert_eq!(new.headline, "dns broke");
        assert_eq!(new.started.as_second(), 60);
        assert_eq!(new.ended, None);
        assert_eq!(hub.snapshot().unwrap().open_incident.as_deref(), Some(new.id.as_str()));
    }

    #[test]
    fn a_blip_of_other_blame_keeps_the_incident() {
        let hub = hub(2, 2);
        hub.publish_snapshot(blamed(0, Severity::Down, "isp"));
        hub.publish_snapshot(blamed(30, Severity::Down, "isp"));
        hub.publish_snapshot(blamed(60, Severity::Down, "dns"));
        hub.publish_snapshot(blamed(90, Severity::Down, "isp"));
        hub.publish_snapshot(blamed(120, Severity::Down, "dns"));
        assert_eq!(hub.incidents().len(), 1);
    }

    #[test]
    fn open_after_one_opens_immediately() {
        let hub = hub(1, 1);
        hub.publish_snapshot(snap(0, Severity::Down));
        assert_eq!(hub.incidents().len(), 1);
        hub.publish_snapshot(snap(30, Severity::Healthy));
        assert_eq!(hub.incidents()[0].ended.unwrap().as_second(), 30);
    }

    #[test]
    fn records_verdict_incident_and_speed_activity() {
        let hub = hub(1, 1);
        hub.publish_snapshot(snap(0, Severity::Healthy));
        hub.publish_snapshot(snap(30, Severity::Healthy));
        hub.publish_snapshot(snap(60, Severity::Down));
        hub.publish_snapshot(snap(90, Severity::Healthy));
        let run = SpeedRun {
            id: "s1".into(),
            started: Timestamp::from_second(100).unwrap(),
            state: SpeedState::Running,
            backend: "demo".into(),
            server: None,
            down_mbps: None,
            up_mbps: None,
            idle_latency_ms: None,
            loaded_latency_ms: None,
            bloat: None,
            bytes_used: None,
            progress: Some(0.1),
            error: None,
        };
        hub.upsert_speed(run.clone());
        hub.upsert_speed(SpeedRun { progress: Some(0.5), ..run.clone() });
        hub.upsert_speed(SpeedRun { state: SpeedState::Complete, down_mbps: Some(500.0), up_mbps: Some(40.0), ..run });
        let mut log: Vec<_> = hub.activity().into_iter().map(|a| (a.kind, a.message)).collect();
        log.reverse();
        let kinds: Vec<_> = log.iter().map(|(k, _)| *k).collect();
        use ActivityKind::*;
        assert_eq!(kinds, [Verdict, Verdict, Incident, Verdict, Incident, Speed, Speed], "{log:?}");
        assert_eq!(log[2].1, "Incident inc-1 opened (down): Down");
        assert_eq!(log[4].1, "Incident inc-1 closed after 30s");
        assert_eq!(log[6].1, "Speed test finished: 500 down, 40 up Mbps");
        let ids: Vec<_> = hub.activity().iter().map(|a| a.id).collect();
        assert!(ids.windows(2).all(|w| w[0] > w[1]));
    }
}
