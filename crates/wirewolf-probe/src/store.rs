//! SQLite history: samples, speed runs, incidents, and the activity timeline.
//!
//! The store follows the hub's event stream rather than hooking into it, so the hub stays
//! storage-agnostic: samples, finished speed runs, and every incident change are written as they
//! are broadcast. At startup [`Store::load`] returns recent history for [`Hub::load_history`].

use std::path::Path;
use std::time::Duration;

use jiff::{SignedDuration, Timestamp};
use rusqlite::{Connection, params};
use tokio::sync::broadcast::error::RecvError;
use wirewolf_core::Hub;
use wirewolf_core::model::{Activity, Event, Incident, Sample, SpeedRun, SpeedState, Status};

pub struct Store {
    conn: Connection,
}

/// History loaded at startup.
pub struct History {
    /// The oldest sample kept, which may be older than `samples`.
    pub first_sample: Option<Timestamp>,
    pub samples: Vec<Sample>,
    pub speed: Vec<SpeedRun>,
    pub incidents: Vec<Incident>,
    pub activity: Vec<Activity>,
    /// The incident a short restart interrupted, still open, and when the last sample before
    /// the restart was taken.
    pub resumed: Option<(String, Timestamp)>,
    /// Incidents a previous run left open that were closed on load, because wirewolf was off
    /// too long to say they continued.
    pub interrupted: Vec<Incident>,
}

/// How long wirewolf can be off with an incident open and still pick it up again on restart.
/// An upgrade or a crash restarts within this; anything longer is a gap nobody watched.
pub const RESUME_WITHIN: SignedDuration = SignedDuration::from_mins(10);

impl Store {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        Self::init(Connection::open(path)?)
    }

    pub fn in_memory() -> anyhow::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> anyhow::Result<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS samples (
                 at_ms INTEGER NOT NULL,
                 hop TEXT NOT NULL,
                 status TEXT NOT NULL,
                 rtt_ms REAL,
                 jitter_ms REAL,
                 loss_pct REAL
             );
             CREATE INDEX IF NOT EXISTS samples_at ON samples (at_ms);
             CREATE TABLE IF NOT EXISTS speed_runs (
                 id TEXT PRIMARY KEY,
                 started_ms INTEGER NOT NULL,
                 json TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS incidents (
                 id TEXT PRIMARY KEY,
                 started_ms INTEGER NOT NULL,
                 json TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS activity (
                 id INTEGER PRIMARY KEY,
                 at_ms INTEGER NOT NULL,
                 json TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS activity_at ON activity (at_ms);
             CREATE TABLE IF NOT EXISTS settings (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );",
        )?;
        Ok(Self { conn })
    }

    /// This install's id, made up and saved the first time it's asked for.
    pub fn instance(&self) -> rusqlite::Result<String> {
        let id = format!("ww-{:08x}", fastrand::u32(..));
        self.conn.execute("INSERT OR IGNORE INTO settings (key, value) VALUES ('instance', ?1)", [id])?;
        self.conn.query_row("SELECT value FROM settings WHERE key = 'instance'", [], |r| r.get(0))
    }

    pub fn insert_sample(&self, s: &Sample) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO samples (at_ms, hop, status, rtt_ms, jitter_ms, loss_pct) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![s.at.as_millisecond(), s.hop, status_str(s.status), s.rtt_ms, s.jitter_ms, s.loss_pct],
        )?;
        Ok(())
    }

    pub fn upsert_speed(&self, r: &SpeedRun) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO speed_runs (id, started_ms, json) VALUES (?1, ?2, ?3)",
            params![r.id, r.started.as_millisecond(), serde_json::to_string(r)?],
        )?;
        Ok(())
    }

    pub fn upsert_incident(&self, i: &Incident) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO incidents (id, started_ms, json) VALUES (?1, ?2, ?3)",
            params![i.id, i.started.as_millisecond(), serde_json::to_string(i)?],
        )?;
        Ok(())
    }

    pub fn insert_activity(&self, a: &Activity) -> anyhow::Result<()> {
        self.conn.execute("INSERT OR REPLACE INTO activity (id, at_ms, json) VALUES (?1, ?2, ?3)", params![a.id as i64, a.at.as_millisecond(), serde_json::to_string(a)?])?;
        Ok(())
    }

    /// Deletes everything older than `keep`.
    pub fn prune(&self, now: Timestamp, keep: SignedDuration) -> rusqlite::Result<usize> {
        let cutoff = (now - keep).as_millisecond();
        let mut n = self.conn.execute("DELETE FROM samples WHERE at_ms < ?1", [cutoff])?;
        n += self.conn.execute("DELETE FROM speed_runs WHERE started_ms < ?1", [cutoff])?;
        n += self.conn.execute("DELETE FROM activity WHERE at_ms < ?1", [cutoff])?;
        n += self.conn.execute("DELETE FROM incidents WHERE started_ms < ?1 AND json_extract(json, '$.ended') IS NOT NULL", [cutoff])?;
        Ok(n)
    }

    /// Loads history for the hub: samples newer than `since`, and the latest speed runs and
    /// incidents. All in time order.
    pub fn load(&self, since: Timestamp, limit: usize) -> anyhow::Result<History> {
        self.load_at(since, limit, Timestamp::now())
    }

    /// [`Store::load`] as of `now`. The newest incident, if a previous run left it open less than
    /// [`RESUME_WITHIN`] before `now`, stays open so this run continues it. Any other incident
    /// left open is closed at the last sample time, since nothing was watching after that.
    pub fn load_at(&self, since: Timestamp, limit: usize, now: Timestamp) -> anyhow::Result<History> {
        let mut stmt = self.conn.prepare("SELECT at_ms, hop, status, rtt_ms, jitter_ms, loss_pct FROM samples WHERE at_ms >= ?1 ORDER BY at_ms, rowid")?;
        let samples = stmt
            .query_map([since.as_millisecond()], |row| {
                Ok(Sample {
                    at: Timestamp::from_millisecond(row.get(0)?).unwrap_or(Timestamp::UNIX_EPOCH),
                    hop: row.get(1)?,
                    status: parse_status(&row.get::<_, String>(2)?),
                    rtt_ms: row.get(3)?,
                    jitter_ms: row.get(4)?,
                    loss_pct: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let json_rows = |table: &str| -> anyhow::Result<Vec<String>> {
            let sql = format!("SELECT json FROM (SELECT json, started_ms FROM {table} ORDER BY started_ms DESC LIMIT ?1) ORDER BY started_ms");
            let mut stmt = self.conn.prepare(&sql)?;
            Ok(stmt.query_map([limit as i64], |row| row.get(0))?.collect::<Result<Vec<_>, _>>()?)
        };
        let speed = json_rows("speed_runs")?.iter().filter_map(|j| serde_json::from_str::<SpeedRun>(j).ok()).filter(|r| r.state != SpeedState::Running).collect();
        let last_seen: Option<i64> = self.conn.query_row("SELECT max(at_ms) FROM samples", [], |r| r.get(0))?;
        let mut incidents: Vec<Incident> = json_rows("incidents")?.iter().filter_map(|j| serde_json::from_str(j).ok()).collect();
        let last_seen = last_seen.and_then(|ms| Timestamp::from_millisecond(ms).ok());
        let newest = incidents.len().checked_sub(1);
        let mut resumed = None;
        let mut interrupted = Vec::new();
        for (n, incident) in incidents.iter_mut().enumerate().filter(|(_, i)| i.ended.is_none()) {
            let last = last_seen.filter(|t| *t >= incident.started).unwrap_or(incident.started);
            if Some(n) == newest && now.duration_since(last) <= RESUME_WITHIN {
                resumed = Some((incident.id.clone(), last));
                continue;
            }
            incident.ended = Some(last);
            self.upsert_incident(incident)?;
            interrupted.push(incident.clone());
        }
        let mut stmt = self.conn.prepare("SELECT json FROM (SELECT id, json FROM activity ORDER BY id DESC LIMIT ?1) ORDER BY id")?;
        let activity = stmt.query_map([limit as i64], |row| row.get::<_, String>(0))?.filter_map(|j| serde_json::from_str(&j.ok()?).ok()).collect();
        let first: Option<i64> = self.conn.query_row("SELECT min(at_ms) FROM samples", [], |r| r.get(0))?;
        let first_sample = first.and_then(|ms| Timestamp::from_millisecond(ms).ok());
        Ok(History { first_sample, samples, speed, incidents, activity, resumed, interrupted })
    }
}

fn status_str(s: Status) -> String {
    serde_json::to_value(s).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

fn parse_status(s: &str) -> Status {
    serde_json::from_value(serde_json::Value::String(s.into())).unwrap_or(Status::Pending)
}

/// Writes hub events to the store until the hub goes away, pruning to `retention` hourly.
pub async fn follow(hub: Hub, store: Store, retention: SignedDuration) {
    let mut events = hub.subscribe();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    // SQLite is blocking; keep it on its own thread.
    let writer = tokio::task::spawn_blocking(move || {
        let mut last_prune: Option<std::time::Instant> = None;
        // A broken database fails every write; say so once, and again when it recovers.
        let mut failing = false;
        while let Some(event) = rx.blocking_recv() {
            let result = match &event {
                Event::Sample(s) => store.insert_sample(s).map_err(Into::into),
                Event::Speed(r) if r.state != SpeedState::Running => store.upsert_speed(r),
                Event::Incident(i) => store.upsert_incident(i),
                Event::Activity(a) => store.insert_activity(a),
                _ => Ok(()),
            };
            match result {
                Err(e) if !failing => {
                    failing = true;
                    tracing::error!("can't write history: {e}");
                }
                Err(e) => tracing::trace!("writing history: {e}"),
                Ok(()) if failing => {
                    failing = false;
                    tracing::info!("writing history again");
                }
                Ok(()) => {}
            }
            if last_prune.is_none_or(|t| t.elapsed() > Duration::from_secs(3600)) {
                last_prune = Some(std::time::Instant::now());
                match store.prune(Timestamp::now(), retention) {
                    Ok(n) if n > 0 => tracing::info!("pruned {n} history rows older than {retention:#}"),
                    Ok(_) => {}
                    Err(e) => tracing::warn!("pruning history: {e}"),
                }
            }
        }
    });
    loop {
        match events.recv().await {
            Ok(event) => {
                if tx.send(event).is_err() {
                    break;
                }
            }
            Err(RecvError::Lagged(n)) => tracing::warn!("history writer fell behind; {n} events not saved"),
            Err(RecvError::Closed) => break,
        }
    }
    drop(tx);
    let _ = writer.await;
}

#[cfg(test)]
mod tests {
    use jiff::ToSpan;
    use wirewolf_core::model::{Confidence, Severity, Snapshot, Verdict};

    use super::*;

    fn incident(id: &str, started: Timestamp, ended: Option<Timestamp>) -> Incident {
        Incident {
            id: id.into(),
            started,
            ended,
            severity: Severity::Down,
            headline: "DNS is failing".into(),
            blame: Some("dns".into()),
            evidence: Snapshot {
                at: started,
                verdict: Verdict {
                    severity: Severity::Down,
                    headline: "DNS is failing".into(),
                    detail: String::new(),
                    blame: Some("dns".into()),
                    confidence: Confidence::High,
                    fix_hint: None,
                    method: String::new(),
                    bottom_line: None,
                    confidence_reason: None,
                    steps: vec![],
                    held_cycles: None,
                },
                hops: vec![],
                open_incident: Some(id.into()),
                targets: vec![],
            },
        }
    }

    #[test]
    fn round_trips_and_closes_dangling_incidents() {
        let store = Store::in_memory().unwrap();
        let t0: Timestamp = "2026-10-23T10:00:00Z".parse().unwrap();
        for i in 0..3 {
            store
                .insert_sample(&Sample { at: t0 + (i * 30).seconds(), hop: "gateway".into(), status: Status::Warn, rtt_ms: Some(2.5), jitter_ms: None, loss_pct: Some(10.0) })
                .unwrap();
        }
        store.upsert_incident(&incident("inc-1", t0, Some(t0 + 30.seconds()))).unwrap();
        store.upsert_incident(&incident("inc-2", t0 + 45.seconds(), None)).unwrap();
        let h = store.load(t0 - 1.hour(), 10).unwrap();
        assert_eq!(h.samples.len(), 3);
        assert_eq!(h.samples[0].status, Status::Warn);
        assert_eq!(h.samples[2].at, t0 + 60.seconds());
        assert_eq!(h.incidents.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["inc-1", "inc-2"]);
        assert_eq!(h.incidents[1].ended, Some(t0 + 60.seconds()));
        let entry = |id, at| Activity { id, at, kind: wirewolf_core::model::ActivityKind::Probe, message: "ICMP unavailable".into(), subject: None, actor: None };
        store.insert_activity(&entry(7, t0)).unwrap();
        store.insert_activity(&entry(8, t0 + 60.seconds())).unwrap();
        assert_eq!(store.load(t0 - 1.hour(), 10).unwrap().activity.iter().map(|a| a.id).collect::<Vec<_>>(), [7, 8]);
        // Pruning keeps recent rows.
        assert_eq!(store.prune(t0 + 90.seconds(), SignedDuration::from_secs(45)), Ok(4));
        let h = store.load(t0 - 1.hour(), 10).unwrap();
        assert_eq!((h.samples.len(), h.activity.len()), (1, 1));
    }

    #[test]
    fn a_short_restart_keeps_the_incident_open_and_a_long_one_closes_it() {
        let store = Store::in_memory().unwrap();
        let t0: Timestamp = "2026-10-23T10:00:00Z".parse().unwrap();
        store.insert_sample(&Sample { at: t0 + 60.seconds(), hop: "gateway".into(), status: Status::Fail, rtt_ms: None, jitter_ms: None, loss_pct: None }).unwrap();
        store.upsert_incident(&incident("inc-1", t0, None)).unwrap();
        let h = store.load_at(t0 - 1.hour(), 10, t0 + 5.minutes()).unwrap();
        assert_eq!(h.resumed, Some(("inc-1".into(), t0 + 60.seconds())));
        assert_eq!(h.incidents[0].ended, None);
        assert!(h.interrupted.is_empty());
        let h = store.load_at(t0 - 1.hour(), 10, t0 + 2.hours()).unwrap();
        assert_eq!(h.resumed, None);
        assert_eq!(h.incidents[0].ended, Some(t0 + 60.seconds()));
        assert_eq!(h.interrupted.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["inc-1"]);
    }

    #[test]
    fn the_instance_id_is_made_once() {
        let store = Store::in_memory().unwrap();
        let id = store.instance().unwrap();
        assert!(id.starts_with("ww-") && id.len() == 11, "{id}");
        assert_eq!(store.instance().unwrap(), id);
    }

    fn run(id: &str, started: Timestamp, state: SpeedState) -> SpeedRun {
        SpeedRun {
            id: id.into(),
            started,
            state,
            backend: "cloudflare".into(),
            server: None,
            down_mbps: Some(900.0),
            up_mbps: Some(40.0),
            idle_latency_ms: Some(8.0),
            loaded_latency_ms: Some(20.0),
            bloat: None,
            bytes_used: None,
            progress: None,
            error: None,
        }
    }

    #[test]
    fn load_window_and_limits_bound_what_comes_back() {
        let store = Store::in_memory().unwrap();
        let t0: Timestamp = "2026-10-01T00:00:00Z".parse().unwrap();
        // Ten days of one sample an hour, so the window matters.
        for h in 0..240 {
            store.insert_sample(&Sample { at: t0 + (h as i64).hours(), hop: "gateway".into(), status: Status::Pass, rtt_ms: Some(1.0), jitter_ms: None, loss_pct: Some(0.0) }).unwrap();
        }
        for i in 0..6 {
            store.upsert_speed(&run(&format!("sp-{i}"), t0 + (i * 24).hours(), SpeedState::Complete)).unwrap();
            store.upsert_incident(&incident(&format!("inc-{i}"), t0 + (i * 24).hours(), Some(t0 + (i * 24 + 1).hours()))).unwrap();
        }
        let h = store.load(t0 + (9 * 24).hours(), 4).unwrap();
        assert_eq!(h.samples.len(), 24, "only the last day of samples");
        // The oldest kept sample is older than the window, so "monitoring since" isn't the window.
        assert_eq!(h.first_sample, Some(t0));
        // Latest N, in time order.
        assert_eq!(h.speed.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["sp-2", "sp-3", "sp-4", "sp-5"]);
        assert_eq!(h.incidents.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["inc-2", "inc-3", "inc-4", "inc-5"]);
    }

    #[test]
    fn running_speed_tests_are_not_restored_and_results_replace_them() {
        let store = Store::in_memory().unwrap();
        let t0: Timestamp = "2026-10-01T00:00:00Z".parse().unwrap();
        store.upsert_speed(&run("sp-1", t0, SpeedState::Running)).unwrap();
        assert!(store.load(t0 - 1.hour(), 10).unwrap().speed.is_empty(), "a run that never finished isn't history");
        store.upsert_speed(&run("sp-1", t0, SpeedState::Complete)).unwrap();
        store.upsert_speed(&run("sp-1", t0, SpeedState::Failed)).unwrap();
        let speed = store.load(t0 - 1.hour(), 10).unwrap().speed;
        assert_eq!(speed.len(), 1, "same id, one row");
        assert_eq!(speed[0].state, SpeedState::Failed);
    }

    #[test]
    fn pruning_spares_an_open_incident_however_old() {
        let store = Store::in_memory().unwrap();
        let t0: Timestamp = "2026-10-01T00:00:00Z".parse().unwrap();
        store.upsert_incident(&incident("old-closed", t0, Some(t0 + 1.hour()))).unwrap();
        store.upsert_incident(&incident("old-open", t0 + 1.hour(), None)).unwrap();
        store.upsert_speed(&run("old", t0, SpeedState::Complete)).unwrap();
        store.insert_sample(&Sample { at: t0 + (40 * 24).hours(), hop: "gateway".into(), status: Status::Pass, rtt_ms: Some(1.0), jitter_ms: None, loss_pct: None }).unwrap();
        let pruned = store.prune(t0 + (40 * 24).hours(), SignedDuration::from_hours(24 * 30)).unwrap();
        assert_eq!(pruned, 2, "the closed incident and the speed run");
        let h = store.load_at(t0 - 1.hour(), 10, t0 + (41 * 24).hours()).unwrap();
        assert_eq!(h.incidents.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["old-open"]);
        // Loading a day later closes it at the last sample, since nothing was watching in between.
        assert_eq!(h.incidents[0].ended, Some(t0 + (40 * 24).hours()));
        assert_eq!(h.samples.len(), 1);
    }

    #[test]
    fn reopening_a_file_keeps_its_history() {
        let dir = std::env::temp_dir().join(format!("wirewolf-store-test-{}", std::process::id()));
        let path = dir.join("nested").join("wirewolf.db");
        let t0: Timestamp = "2026-10-01T00:00:00Z".parse().unwrap();
        {
            let store = Store::open(&path).unwrap();
            store.insert_sample(&Sample { at: t0, hop: "dns".into(), status: Status::Fail, rtt_ms: None, jitter_ms: None, loss_pct: Some(100.0) }).unwrap();
        }
        let store = Store::open(&path).unwrap();
        let h = store.load(t0 - 1.hour(), 10).unwrap();
        assert_eq!((h.samples.len(), h.samples[0].status), (1, Status::Fail));
        drop(store);
        let _ = std::fs::remove_dir_all(dir);
    }
}
