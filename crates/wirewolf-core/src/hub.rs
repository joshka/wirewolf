//! The state hub: the one place drivers (demo or live probes) publish into and the API reads
//! from.
//!
//! Drivers call [`Hub::publish_snapshot`], [`Hub::push_sample`], and [`Hub::upsert_speed`]. The
//! hub keeps bounded in-memory history, opens and closes incidents as the verdict changes, and
//! broadcasts every change as an [`Event`]. Clients send requests (run a speed test, switch demo
//! scenario) as [`Command`]s, which the driver receives from [`Hub::take_commands`].

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, RwLock};

use jiff::Timestamp;
use tokio::sync::{broadcast, mpsc};

use crate::config::Incidents;
use crate::editor::Editor;
use crate::model::{ConfigView, Event, Incident, Meta, Sample, Severity, Snapshot, SpeedRun, SpeedState};

/// How many samples to keep in memory (about 48h of 20 hops and targets at one sample per 30s).
const SAMPLE_CAPACITY: usize = 20 * 2 * 60 * 48;
const SPEED_CAPACITY: usize = 500;
const INCIDENT_CAPACITY: usize = 500;

/// A request from a client to the driver.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    RunSpeedTest,
    /// Demo mode only.
    SetScenario(String),
    /// A saved config to use from the next probe cycle on. Live mode only.
    ApplyConfig(Arc<crate::config::Config>),
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
    policy: Incidents,
    /// Unhealthy snapshots in a row while no incident is open: the first one's time, and the
    /// worst one.
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
            let bad = matches!(severity, Severity::Degraded | Severity::Down);
            let open = state.incidents.back_mut().filter(|i| i.ended.is_none());
            match (open, bad) {
                (Some(incident), true) => {
                    state.recovering = None;
                    if severity > incident.severity {
                        incident.severity = severity;
                        incident.headline = snapshot.verdict.headline.clone();
                        incident.blame = snapshot.verdict.blame.clone();
                        incident.evidence = Snapshot { open_incident: Some(incident.id.clone()), ..snapshot.clone() };
                        events.push(Event::Incident(incident.clone()));
                    }
                    snapshot.open_incident = Some(incident.id.clone());
                }
                (Some(incident), false) => {
                    let (n, since) = state.recovering.map_or((1, snapshot.at), |(n, t)| (n + 1, t));
                    if n < state.policy.close_after.max(1) {
                        state.recovering = Some((n, since));
                        snapshot.open_incident = Some(incident.id.clone());
                    } else {
                        state.recovering = None;
                        incident.ended = Some(since);
                        events.push(Event::Incident(incident.clone()));
                        snapshot.open_incident = None;
                    }
                }
                (None, true) => {
                    let (n, start, worst) = match state.pending.take() {
                        Some((n, start, worst)) if worst.verdict.severity >= severity => (n + 1, start, worst),
                        Some((n, start, _)) => (n + 1, start, snapshot.clone()),
                        None => (1, snapshot.at, snapshot.clone()),
                    };
                    if n < state.policy.open_after.max(1) {
                        state.pending = Some((n, start, worst));
                        snapshot.open_incident = None;
                    } else {
                        let id = format!("inc-{}", state.next_incident);
                        state.next_incident += 1;
                        snapshot.open_incident = Some(id.clone());
                        let incident = Incident {
                            id: id.clone(),
                            started: start,
                            ended: None,
                            severity: worst.verdict.severity,
                            headline: worst.verdict.headline.clone(),
                            blame: worst.verdict.blame.clone(),
                            evidence: Snapshot { open_incident: Some(id), ..worst },
                        };
                        push_bounded(&mut state.incidents, incident.clone(), INCIDENT_CAPACITY);
                        events.push(Event::Incident(incident));
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
        {
            let mut state = self.write();
            match state.speed.iter_mut().find(|r| r.id == run.id) {
                Some(existing) => *existing = run.clone(),
                None => push_bounded(&mut state.speed, run.clone(), SPEED_CAPACITY),
            }
        }
        let _ = self.inner.events.send(Event::Speed(run));
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

    #[test]
    fn open_after_one_opens_immediately() {
        let hub = hub(1, 1);
        hub.publish_snapshot(snap(0, Severity::Down));
        assert_eq!(hub.incidents().len(), 1);
        hub.publish_snapshot(snap(30, Severity::Healthy));
        assert_eq!(hub.incidents()[0].ended.unwrap().as_second(), 30);
    }
}
