//! Application state and input handling. Rendering lives in [`crate::ui`].

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use jiff::{SignedDuration, Timestamp};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::widgets::{ListState, TableState};
use wirewolf_core::model::{Event, Incident, Meta, Sample, Snapshot, SpeedRun, SpeedState};

/// How much history the client keeps in memory.
const RETAIN: SignedDuration = SignedDuration::from_hours(24);
const TOAST_FOR: Duration = Duration::from_secs(4);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Now,
    History,
    Incidents,
    Speed,
}

impl View {
    pub const ALL: [View; 4] = [View::Now, View::History, View::Incidents, View::Speed];

    pub fn title(self) -> &'static str {
        match self {
            View::Now => "Now",
            View::History => "History",
            View::Incidents => "Incidents",
            View::Speed => "Speed",
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|v| *v == self).unwrap_or(0)
    }
}

/// The time window shown by the History view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Range {
    Hour,
    SixHours,
    Day,
}

impl Range {
    pub const ALL: [Range; 3] = [Range::Hour, Range::SixHours, Range::Day];

    pub fn label(self) -> &'static str {
        match self {
            Range::Hour => "1h",
            Range::SixHours => "6h",
            Range::Day => "24h",
        }
    }

    pub fn duration(self) -> SignedDuration {
        match self {
            Range::Hour => SignedDuration::from_hours(1),
            Range::SixHours => SignedDuration::from_hours(6),
            Range::Day => SignedDuration::from_hours(24),
        }
    }

    fn next(self) -> Self {
        match self {
            Range::Hour => Range::SixHours,
            Range::SixHours => Range::Day,
            Range::Day => Range::Hour,
        }
    }
}

/// The state of the live event stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Connection {
    Connecting,
    Live,
    Lost(String),
}

#[derive(Debug)]
pub enum Popup {
    Hop,
    Scenarios(ListState),
    Help,
}

/// Everything fetched over REST when (re)connecting.
#[derive(Debug)]
pub struct Loaded {
    pub meta: Meta,
    pub now: Option<Snapshot>,
    pub history: Vec<Sample>,
    pub speed: Vec<SpeedRun>,
    pub incidents: Vec<Incident>,
}

/// Inputs to [`App::update`].
#[derive(Debug)]
pub enum Msg {
    Key(KeyEvent),
    Tick,
    Loaded(Box<Loaded>),
    Event(Box<Event>),
    Disconnected(String),
    Toast(String),
}

/// Side effects the event loop performs for the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Quit,
    RunSpeed,
    SetScenario(String),
    Copy(String),
}

#[derive(Debug)]
pub struct App {
    pub url: String,
    pub connection: Connection,
    pub meta: Option<Meta>,
    pub snapshot: Option<Snapshot>,
    /// Samples per hop id, oldest first.
    pub samples: HashMap<String, VecDeque<Sample>>,
    /// Newest first.
    pub speed: Vec<SpeedRun>,
    /// Newest first.
    pub incidents: Vec<Incident>,
    pub view: View,
    pub popup: Option<Popup>,
    pub selected_hop: usize,
    pub range: Range,
    pub hidden_hops: HashSet<String>,
    pub history_cursor: usize,
    pub incident_table: TableState,
    /// The incident open in the detail view.
    pub incident_detail: Option<String>,
    pub toast: Option<(String, Instant)>,
    /// The clock used for relative times, refreshed on every tick.
    pub now: Timestamp,
}

impl App {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            connection: Connection::Connecting,
            meta: None,
            snapshot: None,
            samples: HashMap::new(),
            speed: Vec::new(),
            incidents: Vec::new(),
            view: View::Now,
            popup: None,
            selected_hop: 0,
            range: Range::Hour,
            hidden_hops: HashSet::from(["host".to_string()]),
            history_cursor: 0,
            incident_table: TableState::new().with_selected(0),
            incident_detail: None,
            toast: None,
            now: Timestamp::now(),
        }
    }

    pub fn is_demo(&self) -> bool {
        self.meta.as_ref().is_some_and(|m| m.mode == "demo")
    }

    /// Hop ids in path order, from the latest snapshot.
    pub fn hop_ids(&self) -> Vec<&str> {
        self.snapshot
            .iter()
            .flat_map(|s| s.hops.iter().map(|h| h.id.as_str()))
            .collect()
    }

    pub fn running_speed(&self) -> Option<&SpeedRun> {
        self.speed.iter().find(|r| r.state == SpeedState::Running)
    }

    pub fn open_incident(&self) -> Option<&Incident> {
        let id = self.snapshot.as_ref()?.open_incident.as_deref()?;
        self.incidents.iter().find(|i| i.id == id)
    }

    pub fn detail_incident(&self) -> Option<&Incident> {
        let id = self.incident_detail.as_deref()?;
        self.incidents.iter().find(|i| i.id == id)
    }

    fn selected_incident(&self) -> Option<&Incident> {
        self.incident_table
            .selected()
            .and_then(|i| self.incidents.get(i))
    }

    fn toast(&mut self, message: impl Into<String>) {
        self.toast = Some((message.into(), Instant::now()));
    }

    pub fn update(&mut self, msg: Msg) -> Option<Action> {
        match msg {
            Msg::Key(key) if key.kind != KeyEventKind::Release => return self.on_key(key),
            Msg::Key(_) => {}
            Msg::Tick => {
                self.now = Timestamp::now();
                if self
                    .toast
                    .as_ref()
                    .is_some_and(|(_, at)| at.elapsed() > TOAST_FOR)
                {
                    self.toast = None;
                }
            }
            Msg::Loaded(loaded) => self.on_loaded(*loaded),
            Msg::Event(event) => self.on_event(*event),
            Msg::Disconnected(error) => self.connection = Connection::Lost(error),
            Msg::Toast(message) => self.toast(message),
        }
        None
    }

    fn on_loaded(&mut self, loaded: Loaded) {
        self.connection = Connection::Live;
        self.meta = Some(loaded.meta);
        if loaded.now.is_some() {
            self.snapshot = loaded.now;
        }
        self.samples.clear();
        for sample in loaded.history {
            self.push_sample(sample);
        }
        self.speed = loaded.speed;
        self.incidents = loaded.incidents;
        self.clamp_selection();
    }

    fn on_event(&mut self, event: Event) {
        self.connection = Connection::Live;
        match event {
            Event::Snapshot(snapshot) => {
                self.snapshot = Some(snapshot);
                self.clamp_selection();
            }
            Event::Sample(sample) => {
                self.push_sample(sample);
                let cutoff = self.now - RETAIN;
                for samples in self.samples.values_mut() {
                    while samples.front().is_some_and(|s| s.at < cutoff) {
                        samples.pop_front();
                    }
                }
            }
            Event::Speed(run) => {
                if run.state == SpeedState::Complete
                    && self.running_speed().is_some_and(|r| r.id == run.id)
                    && let Some(down) = run.down_mbps {
                        self.toast(format!(
                            "Speed test done: {} down",
                            crate::format::mbps(down)
                        ));
                    }
                match self.speed.iter_mut().find(|r| r.id == run.id) {
                    Some(existing) => *existing = run,
                    None => self.speed.insert(0, run),
                }
            }
            Event::Activity(_) => {}
            Event::Incident(incident) => {
                match self.incidents.iter_mut().find(|i| i.id == incident.id) {
                    Some(existing) => *existing = incident,
                    None => {
                        self.incidents.insert(0, incident);
                        // Keep the same incident selected as the list grows at the top.
                        if let Some(selected) = self.incident_table.selected()
                            && self.incidents.len() > 1 {
                                self.incident_table.select(Some(selected + 1));
                            }
                    }
                }
                self.clamp_selection();
            }
        }
    }

    fn push_sample(&mut self, sample: Sample) {
        self.samples
            .entry(sample.hop.clone())
            .or_default()
            .push_back(sample);
    }

    fn clamp_selection(&mut self) {
        let hops = self.hop_ids().len();
        self.selected_hop = self.selected_hop.min(hops.saturating_sub(1));
        self.history_cursor = self.history_cursor.min(hops.saturating_sub(1));
        match self.incident_table.selected() {
            _ if self.incidents.is_empty() => self.incident_table.select(None),
            Some(i) if i >= self.incidents.len() => {
                self.incident_table.select(Some(self.incidents.len() - 1))
            }
            None => self.incident_table.select(Some(0)),
            _ => {}
        }
    }

    fn on_key(&mut self, key: KeyEvent) -> Option<Action> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Some(Action::Quit);
        }
        if let Some(popup) = &mut self.popup {
            match popup {
                Popup::Scenarios(state) => match key.code {
                    KeyCode::Esc | KeyCode::Char('s') | KeyCode::Char('q') => self.popup = None,
                    KeyCode::Up | KeyCode::Char('k') => state.select_previous(),
                    KeyCode::Down | KeyCode::Char('j') => state.select_next(),
                    KeyCode::Enter => {
                        let scenarios = self
                            .meta
                            .as_ref()
                            .map(|m| m.scenarios.as_slice())
                            .unwrap_or_default();
                        let id = state
                            .selected()
                            .and_then(|i| scenarios.get(i))
                            .map(|s| s.id.clone());
                        self.popup = None;
                        return id.map(Action::SetScenario);
                    }
                    _ => {}
                },
                Popup::Hop => match key.code {
                    KeyCode::Left | KeyCode::Char('h') => {
                        self.selected_hop = self.selected_hop.saturating_sub(1)
                    }
                    KeyCode::Right | KeyCode::Char('l') => self.select_hop_next(),
                    _ => self.popup = None,
                },
                Popup::Help => self.popup = None,
            }
            return None;
        }

        match key.code {
            KeyCode::Char('q') => return Some(Action::Quit),
            KeyCode::Esc if self.incident_detail.is_some() => self.incident_detail = None,
            KeyCode::Char(c @ '1'..='4') => self.view = View::ALL[c as usize - '1' as usize],
            KeyCode::Tab => self.view = View::ALL[(self.view.index() + 1) % View::ALL.len()],
            KeyCode::BackTab => {
                self.view = View::ALL[(self.view.index() + View::ALL.len() - 1) % View::ALL.len()]
            }
            KeyCode::Char('?') => self.popup = Some(Popup::Help),
            KeyCode::Char('s') if self.is_demo() => {
                let active = self.meta.as_ref().and_then(|m| {
                    let id = m.scenario.as_deref()?;
                    m.scenarios.iter().position(|s| s.id == id)
                });
                self.popup = Some(Popup::Scenarios(
                    ListState::default().with_selected(Some(active.unwrap_or(0))),
                ));
            }
            KeyCode::Char('r') => {
                if self.running_speed().is_some() {
                    self.toast("A speed test is already running");
                    return None;
                }
                self.toast("Starting speed test…");
                return Some(Action::RunSpeed);
            }
            _ => return self.on_view_key(key),
        }
        None
    }

    fn select_hop_next(&mut self) {
        self.selected_hop = (self.selected_hop + 1).min(self.hop_ids().len().saturating_sub(1));
    }

    fn on_view_key(&mut self, key: KeyEvent) -> Option<Action> {
        match self.view {
            View::Now => match key.code {
                KeyCode::Left | KeyCode::Char('h') => {
                    self.selected_hop = self.selected_hop.saturating_sub(1)
                }
                KeyCode::Right | KeyCode::Char('l') => self.select_hop_next(),
                KeyCode::Enter | KeyCode::Char(' ') if self.snapshot.is_some() => {
                    self.popup = Some(Popup::Hop)
                }
                KeyCode::Char('i') => {
                    if let Some(id) = self.open_incident().map(|i| i.id.clone()) {
                        self.view = View::Incidents;
                        self.incident_detail = Some(id);
                    }
                }
                _ => {}
            },
            View::History => match key.code {
                KeyCode::Char('t') => self.range = self.range.next(),
                KeyCode::Left | KeyCode::Char('h') => {
                    self.history_cursor = self.history_cursor.saturating_sub(1)
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    self.history_cursor =
                        (self.history_cursor + 1).min(self.hop_ids().len().saturating_sub(1));
                }
                KeyCode::Char(' ') | KeyCode::Enter => {
                    if let Some(id) = self
                        .hop_ids()
                        .get(self.history_cursor)
                        .map(|s| s.to_string())
                        && !self.hidden_hops.remove(&id) {
                            self.hidden_hops.insert(id);
                        }
                }
                _ => {}
            },
            View::Incidents => match key.code {
                KeyCode::Up | KeyCode::Char('k') if self.incident_detail.is_none() => {
                    self.incident_table.select_previous()
                }
                KeyCode::Down | KeyCode::Char('j') if self.incident_detail.is_none() => {
                    self.incident_table.select_next();
                    self.clamp_selection();
                }
                KeyCode::Enter if self.incident_detail.is_none() => {
                    self.incident_detail = self.selected_incident().map(|i| i.id.clone());
                }
                KeyCode::Char('c') | KeyCode::Char('y') => {
                    let incident = self
                        .detail_incident()
                        .or_else(|| self.selected_incident())?;
                    let host = self.meta.as_ref().map(|m| m.host.as_str());
                    let report = crate::report::incident(incident, host);
                    self.toast("Report copied to the clipboard");
                    return Some(Action::Copy(report));
                }
                _ => {}
            },
            View::Speed => {}
        }
        None
    }
}
