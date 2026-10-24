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
    pub samples: Vec<Sample>,
    pub speed: Vec<SpeedRun>,
    pub incidents: Vec<Incident>,
    pub activity: Vec<Activity>,
}

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
             CREATE INDEX IF NOT EXISTS activity_at ON activity (at_ms);",
        )?;
        Ok(Self { conn })
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
    /// incidents. All in time order. An incident left open by a previous run is closed at the
    /// last sample time, since nothing was watching after that.
    pub fn load(&self, since: Timestamp, limit: usize) -> anyhow::Result<History> {
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
        for incident in incidents.iter_mut().filter(|i| i.ended.is_none()) {
            let ended = last_seen.and_then(|ms| Timestamp::from_millisecond(ms).ok()).filter(|t| *t >= incident.started).unwrap_or(incident.started);
            incident.ended = Some(ended);
            self.upsert_incident(incident)?;
        }
        let mut stmt = self.conn.prepare("SELECT json FROM (SELECT id, json FROM activity ORDER BY id DESC LIMIT ?1) ORDER BY id")?;
        let activity = stmt.query_map([limit as i64], |row| row.get::<_, String>(0))?.filter_map(|j| serde_json::from_str(&j.ok()?).ok()).collect();
        Ok(History { samples, speed, incidents, activity })
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
        while let Some(event) = rx.blocking_recv() {
            let result = match &event {
                Event::Sample(s) => store.insert_sample(s).map_err(Into::into),
                Event::Speed(r) if r.state != SpeedState::Running => store.upsert_speed(r),
                Event::Incident(i) => store.upsert_incident(i),
                Event::Activity(a) => store.insert_activity(a),
                _ => Ok(()),
            };
            if let Err(e) = result {
                tracing::warn!("writing history: {e}");
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
}
