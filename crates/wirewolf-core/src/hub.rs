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

use crate::model::{Event, Incident, Meta, Sample, Severity, Snapshot, SpeedRun, SpeedState};

/// How many samples to keep in memory (about 48h at one sample per hop per 30s).
const SAMPLE_CAPACITY: usize = 6 * 2 * 60 * 48;
const SPEED_CAPACITY: usize = 500;
const INCIDENT_CAPACITY: usize = 500;

/// A request from a client to the driver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    RunSpeedTest,
    /// Demo mode only.
    SetScenario(String),
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

    /// Publishes a new snapshot, opening, escalating, or closing incidents as the verdict changes.
    pub fn publish_snapshot(&self, mut snapshot: Snapshot) {
        let mut events = Vec::new();
        {
            let mut state = self.write();
            let severity = snapshot.verdict.severity;
            let bad = matches!(severity, Severity::Degraded | Severity::Down);
            let open = state.incidents.back_mut().filter(|i| i.ended.is_none());
            match (open, bad) {
                (Some(incident), true) => {
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
                    incident.ended = Some(snapshot.at);
                    events.push(Event::Incident(incident.clone()));
                    snapshot.open_incident = None;
                }
                (None, true) => {
                    let id = format!("inc-{}", state.next_incident);
                    state.next_incident += 1;
                    snapshot.open_incident = Some(id.clone());
                    let incident = Incident {
                        id,
                        started: snapshot.at,
                        ended: None,
                        severity,
                        headline: snapshot.verdict.headline.clone(),
                        blame: snapshot.verdict.blame.clone(),
                        evidence: snapshot.clone(),
                    };
                    push_bounded(&mut state.incidents, incident.clone(), INCIDENT_CAPACITY);
                    events.push(Event::Incident(incident));
                }
                (None, false) => snapshot.open_incident = None,
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
        state.next_incident += incidents.len() as u64;
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
