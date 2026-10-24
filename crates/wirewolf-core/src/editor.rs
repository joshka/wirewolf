//! Editing the config from the API: check a submitted [`Config`], write it back to
//! `wirewolf.toml` with `toml_edit` (comments and ordering survive), and reload.
//!
//! Who may write is decided by the API's auth layer. This module only refuses edits that can't
//! take effect: values pinned by env vars or flags, settings that need a restart, and `[auth]`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table};

use crate::config::{self, Config, Loaded, Override, Source};
use crate::model::ConfigView;

/// Settings read once at startup. Changing them means editing the file and restarting.
pub const RESTART_KEYS: &[&str] = &["server.listen", "server.allowed_hosts", "storage.db", "storage.retention"];

/// Tables the API never writes, because they decide who can change anything.
const PROTECTED: &[&str] = &["auth"];

/// One rejected setting. `key` is a dotted path such as `probe.interval` or `targets.0.check`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldError {
    pub key: String,
    pub message: String,
}

impl FieldError {
    pub fn new(key: impl Into<String>, message: impl Into<String>) -> Self {
        Self { key: key.into(), message: message.into() }
    }
}

/// Checks that need more than the schema (target URLs, the speed backend). Supplied by the
/// probe crate.
pub type Validator = fn(&Config) -> Vec<FieldError>;

pub struct Editor {
    path: PathBuf,
    overrides: Vec<Override>,
    validate: Validator,
    read_only: bool,
    lock: Mutex<()>,
}

impl std::fmt::Debug for Editor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Editor").field("path", &self.path).field("read_only", &self.read_only).finish()
    }
}

impl Editor {
    /// `path` is where edits go; it's created on the first save if it doesn't exist.
    pub fn new(path: PathBuf, overrides: Vec<Override>, validate: Validator, read_only: bool) -> Self {
        Self { path, overrides, validate, read_only, lock: Mutex::new(()) }
    }

    pub fn read_only(&self) -> bool {
        self.read_only
    }

    /// The view for `GET /config`, with what's editable filled in.
    pub fn view(&self, loaded: &Loaded) -> ConfigView {
        let mut view = loaded.view();
        view.editable = !self.read_only;
        view.save_path = Some(self.path.display().to_string());
        view.restart_keys = RESTART_KEYS.iter().map(|k| k.to_string()).collect();
        view
    }

    /// Checks `submitted`, writes it to the file, and returns the reloaded config.
    pub fn apply(&self, submitted: Config) -> Result<Loaded, Vec<FieldError>> {
        if self.read_only {
            return Err(vec![FieldError::new("", "settings are read-only on this server (--read-only-settings)")]);
        }
        let _guard = self.lock.lock().unwrap();
        let one = |e: String| vec![FieldError::new("", e)];
        let current = config::load(Some(&self.path), false, &self.overrides).map_err(one)?;
        let mut new = submitted;
        unredact(&mut new, &current.config);
        let cur = serde_json::to_value(&current.config).expect("config serializes");
        let mut next = serde_json::to_value(&new).expect("config serializes");
        for key in PROTECTED {
            if let (Some(c), Some(n)) = (cur.get(*key).cloned(), next.as_object_mut()) {
                n.insert(key.to_string(), c);
            }
        }
        let mut errors = Vec::new();
        for (key, source) in &current.sources {
            if matches!(source, Source::Env | Source::Flag) && lookup(&cur, key) != lookup(&next, key) {
                let how = if *source == Source::Env { "an environment variable" } else { "a command line flag" };
                errors.push(FieldError::new(key, format!("set by {how}; change it there")));
            }
        }
        for key in RESTART_KEYS {
            if !current.sources.get(*key).is_some_and(|s| matches!(s, Source::Env | Source::Flag)) && lookup(&cur, key) != lookup(&next, key) {
                errors.push(FieldError::new(*key, "needs a restart: edit the file and restart wirewolf"));
            }
        }
        let new: Config = serde_json::from_value(next.clone()).map_err(|e| one(e.to_string()))?;
        errors.extend(check(&new));
        errors.extend((self.validate)(&new));
        if !errors.is_empty() {
            return Err(errors);
        }
        let text = match std::fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(one(format!("can't read {}: {e}", self.path.display()))),
        };
        let mut skip: Vec<String> = current.sources.iter().filter(|(_, s)| matches!(s, Source::Env | Source::Flag)).map(|(k, _)| k.clone()).collect();
        skip.extend(PROTECTED.iter().map(|k| k.to_string()));
        let out = write_toml(&text, &next, &skip).map_err(one)?;
        write_atomic(&self.path, &out).map_err(|e| one(format!("can't write {}: {e}", self.path.display())))?;
        config::load(Some(&self.path), true, &self.overrides).map_err(one)
    }
}

/// Puts back credentials (userinfo and query values) that `GET /config` replaced with `***`,
/// matching by position.
fn unredact(new: &mut Config, current: &Config) {
    let restore = |shown: &mut String, real: &str| {
        if shown.contains("***") && *shown == config::redact_url(real) {
            *shown = real.to_string();
        }
    };
    for (i, t) in new.targets.iter_mut().enumerate() {
        if let Some(c) = current.targets.get(i) {
            restore(&mut t.check, &c.check);
        }
    }
    for (i, s) in new.web.sites.iter_mut().enumerate() {
        if let Some(c) = current.web.sites.get(i) {
            restore(s, c);
        }
    }
    restore(&mut new.web.captive_portal, &current.web.captive_portal);
}

fn lookup<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    key.split('.').try_fold(v, |v, k| v.get(k))
}

/// Range and consistency checks the schema can't express.
pub fn check(c: &Config) -> Vec<FieldError> {
    let mut e = Vec::new();
    let mut err = |k: String, m: &str| e.push(FieldError::new(k, m));
    if c.probe.interval.0.as_secs() < 5 {
        err("probe.interval".into(), "must be at least 5s");
    }
    if c.internet.port == 0 {
        err("internet.port".into(), "must be a port from 1 to 65535");
    }
    for (i, t) in c.internet.targets.iter().enumerate() {
        if t.name.trim().is_empty() {
            err(format!("internet.targets.{i}.name"), "needs a name");
        }
        if t.v4.is_none() && t.v6.is_none() {
            err(format!("internet.targets.{i}.v4"), "needs an IPv4 or IPv6 address");
        }
    }
    if c.internet.targets.is_empty() {
        err("internet.targets".into(), "needs at least one target");
    }
    for (i, r) in c.dns.resolvers.iter().enumerate() {
        if r.name.trim().is_empty() {
            err(format!("dns.resolvers.{i}.name"), "needs a name");
        }
    }
    if c.dns.queries.is_empty() {
        err("dns.queries".into(), "needs at least one name");
    }
    for (i, q) in c.dns.queries.iter().enumerate() {
        if q.name.trim().is_empty() || q.name.contains(char::is_whitespace) {
            err(format!("dns.queries.{i}.name"), "must be a domain name");
        }
    }
    if c.dns.record_types.is_empty() {
        err("dns.record_types".into(), "pick A, AAAA, or both");
    }
    if !c.web.captive_portal.starts_with("http://") {
        err("web.captive_portal".into(), "must be a plain http:// URL");
    }
    if c.web.tls_host.trim().is_empty() || c.web.tls_host.contains(['/', ' ']) {
        err("web.tls_host".into(), "must be a host name");
    }
    for (i, s) in c.web.sites.iter().enumerate() {
        if !s.starts_with("https://") || s.len() <= "https://".len() {
            err(format!("web.sites.{i}"), "must be an https:// URL");
        }
    }
    let t = &c.thresholds;
    for (name, l) in [("gateway", t.gateway), ("isp", t.isp), ("internet", t.internet), ("dns", t.dns), ("targets", t.targets)] {
        for (k, v, max) in [("rtt_warn_ms", l.rtt_warn_ms, 60_000.0), ("jitter_warn_ms", l.jitter_warn_ms, 60_000.0), ("loss_warn_pct", l.loss_warn_pct, 100.0)] {
            if !(v.is_finite() && v > 0.0 && v <= max) {
                err(format!("thresholds.{name}.{k}"), if max == 100.0 { "must be above 0 and at most 100" } else { "must be above 0" });
            }
        }
    }
    if c.incidents.open_after == 0 || c.incidents.open_after > 100 {
        err("incidents.open_after".into(), "must be from 1 to 100");
    }
    if c.incidents.close_after == 0 || c.incidents.close_after > 100 {
        err("incidents.close_after".into(), "must be from 1 to 100");
    }
    if !c.speed.interval.0.is_zero() && c.speed.interval.0.as_secs() < 5 * 60 {
        err("speed.interval".into(), "must be 0 (off) or at least 5m");
    }
    if !(1..=64).contains(&c.speed.streams) {
        err("speed.streams".into(), "must be from 1 to 64");
    }
    if !(2..=60).contains(&c.speed.duration.0.as_secs()) {
        err("speed.duration".into(), "must be from 2s to 60s");
    }
    let mut names = BTreeMap::new();
    for (i, t) in c.targets.iter().enumerate() {
        if t.name.trim().is_empty() {
            err(format!("targets.{i}.name"), "needs a name");
        } else if names.insert(t.name.trim().to_lowercase(), i).is_some() {
            err(format!("targets.{i}.name"), "another target has this name");
        }
    }
    e
}

/// Rewrites the config file `text` so it loads as `new`, touching as little as possible.
///
/// A setting is written when it differs from what the file already says, or when it differs
/// from the default and the file doesn't mention it. Keys in `skip` (and their children) are
/// left alone. Comments, ordering, and untouched values keep their formatting.
pub fn write_toml(text: &str, new: &Value, skip: &[String]) -> Result<String, String> {
    let mut doc: DocumentMut = text.parse().map_err(|e| format!("can't edit the config file: {e}"))?;
    let file = if text.trim().is_empty() { Value::Object(Default::default()) } else { config::parse_toml(text)? };
    let defaults = serde_json::to_value(Config::default()).expect("config serializes");
    let mut changes = Vec::new();
    diff(&defaults, new, &file, &mut Vec::new(), skip, &mut changes);
    for (path, value) in changes {
        let (leaf, parents) = path.split_last().expect("non-empty path");
        let mut table = doc.as_table_mut();
        for p in parents {
            if !table.contains_key(p) {
                let mut t = Table::new();
                t.set_implicit(true);
                table.insert(p, Item::Table(t));
            }
            table = table.get_mut(p).and_then(Item::as_table_mut).ok_or_else(|| format!("{} isn't a table in the config file", parents.join(".")))?;
        }
        let item = to_item(&value);
        match (table.get_mut(leaf), item) {
            (Some(Item::Value(old)), Item::Value(mut v)) => {
                *v.decor_mut() = old.decor().clone();
                *old = v;
            }
            (Some(Item::ArrayOfTables(old)), Item::ArrayOfTables(mut aot)) => {
                for (i, t) in aot.iter_mut().enumerate() {
                    if let Some(o) = old.get(i) {
                        *t.decor_mut() = o.decor().clone();
                        if let Some(pos) = o.position() {
                            t.set_position(Some(pos));
                        }
                    }
                }
                *old = aot;
            }
            (_, Item::None) => {
                table.remove(leaf);
            }
            (_, item) => {
                table.insert(leaf, item);
            }
        }
        if let Some(t) = table.get_mut(leaf).and_then(Item::as_table_mut) {
            t.set_implicit(false);
        }
    }
    Ok(doc.to_string())
}

fn diff(default: &Value, new: &Value, file: &Value, path: &mut Vec<String>, skip: &[String], out: &mut Vec<(Vec<String>, Value)>) {
    let dotted = path.join(".");
    if !dotted.is_empty() && skip.contains(&dotted) {
        return;
    }
    if let (Value::Object(d), Value::Object(n)) = (default, new) {
        for (k, nv) in n {
            path.push(k.clone());
            diff(d.get(k).unwrap_or(&Value::Null), nv, file.get(k).unwrap_or(&Value::Null), path, skip, out);
            path.pop();
        }
        return;
    }
    let in_file = !file.is_null();
    let changed = if in_file { !same(file, new) } else { !same(default, new) };
    if changed {
        out.push((path.clone(), new.clone()));
    }
}

/// Equal, treating `150` and `150.0` as the same number.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b)),
        (Value::Object(x), Value::Object(y)) => {
            let keys = |m: &serde_json::Map<String, Value>| m.iter().filter(|(_, v)| !v.is_null()).map(|(k, _)| k.clone()).collect::<Vec<_>>();
            keys(x) == keys(y) && x.iter().filter(|(_, v)| !v.is_null()).all(|(k, v)| same(v, &y[k]))
        }
        _ => a == b,
    }
}

fn to_item(v: &Value) -> Item {
    match v {
        Value::Array(a) if !a.is_empty() && a.iter().all(Value::is_object) => {
            let mut aot = ArrayOfTables::new();
            for o in a {
                let mut t = Table::new();
                for (k, v) in o.as_object().unwrap() {
                    if let Some(val) = to_value(v) {
                        t.insert(k, Item::Value(val));
                    }
                }
                aot.push(t);
            }
            Item::ArrayOfTables(aot)
        }
        _ => to_value(v).map_or(Item::None, Item::Value),
    }
}

fn to_value(v: &Value) -> Option<toml_edit::Value> {
    Some(match v {
        Value::Null => return None,
        Value::Bool(b) => (*b).into(),
        Value::Number(n) => match n.as_i64() {
            Some(i) => i.into(),
            None => n.as_f64()?.into(),
        },
        Value::String(s) => s.as_str().into(),
        Value::Array(a) => {
            let mut arr = toml_edit::Array::new();
            for x in a {
                arr.push_formatted(to_value(x)?);
            }
            arr.fmt();
            toml_edit::Value::Array(arr)
        }
        Value::Object(o) => {
            let mut t = toml_edit::InlineTable::new();
            for (k, v) in o {
                if let Some(v) = to_value(v) {
                    t.insert(k, v);
                }
            }
            toml_edit::Value::InlineTable(t)
        }
    })
}

/// Writes next to the target and renames, so a crash never leaves half a file.
fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("toml.tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    // The file can hold credentials (target URLs, `auth.client_secret`), so a new one is private
    // and an existing one keeps its permissions.
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mode = std::fs::metadata(path).map_or(0o600, |m| m.permissions().mode() & 0o777);
        options.mode(mode);
    }
    std::io::Write::write_all(&mut options.open(&tmp)?, text.as_bytes())?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Dur, UserTarget};

    /// A fresh directory for one test, removed when the guard drops.
    struct TmpDir(PathBuf);

    impl TmpDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("wirewolf-editor-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn file(&self) -> PathBuf {
            self.0.join("wirewolf.toml")
        }
    }

    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn no_extra(_: &Config) -> Vec<FieldError> {
        vec![]
    }

    #[test]
    fn round_trip_keeps_comments_and_order() {
        let dir = TmpDir::new("round");
        let path = dir.file();
        let original = "# my notes\n[probe]\n# how often\ninterval = \"10s\" # fast\n\n[incidents]\nopen_after = 3\n";
        std::fs::write(&path, original).unwrap();
        let ed = Editor::new(path.clone(), vec![], no_extra, false);
        let mut c = config::load(Some(&path), true, &[]).unwrap().config;
        c.probe.interval = Dur::secs(20);
        c.thresholds.gateway.rtt_warn_ms = 45.0;
        c.targets.push(UserTarget { name: "NAS".into(), check: "tcp://nas.local:445".into(), critical: true, verify_tls: true });
        let loaded = ed.apply(c).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# my notes\n[probe]\n# how often\ninterval = \"20s\" # fast\n"), "{text}");
        assert!(text.find("[probe]").unwrap() < text.find("[incidents]").unwrap());
        assert!(text.contains("open_after = 3"));
        assert!(text.contains("[thresholds.gateway]\nrtt_warn_ms = 45"), "{text}");
        assert!(!text.contains("[thresholds]\n"), "{text}");
        assert!(!text.contains("[dns]"), "untouched defaults stay out: {text}");
        assert_eq!(loaded.config.targets[0].name, "NAS");
        assert_eq!(loaded.config.probe.interval, Dur::secs(20));
        assert_eq!(loaded.sources.get("probe.interval"), Some(&Source::File));
    }

    #[test]
    fn default_file_round_trips_unchanged() {
        let dir = TmpDir::new("default");
        let path = dir.file();
        std::fs::write(&path, config::DEFAULT_TOML).unwrap();
        let ed = Editor::new(path.clone(), vec![], no_extra, false);
        ed.apply(Config::default()).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), config::DEFAULT_TOML);
    }

    #[test]
    fn creates_a_missing_file() {
        let dir = TmpDir::new("create");
        let path = dir.0.join("sub").join("wirewolf.toml");
        let ed = Editor::new(path.clone(), vec![], no_extra, false);
        let mut c = Config::default();
        c.incidents.close_after = 4;
        ed.apply(c).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[incidents]\nclose_after = 4\n");
    }

    #[cfg(unix)]
    #[test]
    fn saves_keep_the_file_private() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        let dir = TmpDir::new("mode");
        let path = dir.file();
        let ed = Editor::new(path.clone(), vec![], no_extra, false);
        let mut c = Config::default();
        c.incidents.close_after = 4;
        ed.apply(c.clone()).unwrap();
        assert_eq!(mode(&path), 0o600);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        c.incidents.close_after = 5;
        ed.apply(c).unwrap();
        assert_eq!(mode(&path), 0o640);
    }

    #[test]
    fn env_set_lists_are_locked() {
        let dir = TmpDir::new("env-locked");
        let path = dir.file();
        let o = config::env_overrides([("WIREWOLF_SERVER_ALLOWED_HOSTS".to_string(), "wirewolf.example.com".to_string())]);
        let ed = Editor::new(path.clone(), o.clone(), no_extra, false);
        let loaded = config::load(Some(&path), false, &o).unwrap();
        assert_eq!(loaded.sources.get("server.allowed_hosts"), Some(&Source::Env));
        let mut c = loaded.config;
        c.server.allowed_hosts.push("other.example.com".into());
        let errors = ed.apply(c).unwrap_err();
        assert!(errors.iter().any(|e| e.key == "server.allowed_hosts" && e.message.contains("environment")), "{errors:?}");
        assert!(!path.exists(), "nothing written");
    }

    #[test]
    fn rejects_invalid_and_locked_values() {
        let dir = TmpDir::new("invalid");
        let path = dir.file();
        let o = vec![Override { key: "speed.backend".into(), value: "ookla".into(), source: Source::Env, name: "WIREWOLF_SPEED_BACKEND".into() }];
        let ed = Editor::new(path.clone(), o, |_| vec![FieldError::new("targets.0.check", "bad")], false);
        let mut c = config::load(Some(&path), false, &[]).unwrap().config;
        c.probe.interval = Dur::secs(1);
        c.incidents.open_after = 0;
        c.server.listen = "0.0.0.0:1".into();
        c.speed.backend = "cloudflare".into();
        let keys: Vec<String> = ed.apply(c).unwrap_err().into_iter().map(|e| e.key).collect();
        for k in ["probe.interval", "incidents.open_after", "server.listen", "speed.backend", "targets.0.check"] {
            assert!(keys.iter().any(|x| x == k), "{k} missing from {keys:?}");
        }
        assert!(!path.exists(), "nothing written on error");
    }

    #[test]
    fn env_values_are_not_written() {
        let dir = TmpDir::new("env");
        let path = dir.file();
        let o = vec![Override { key: "probe.interval".into(), value: "15s".into(), source: Source::Env, name: "WIREWOLF_PROBE_INTERVAL".into() }];
        let ed = Editor::new(path.clone(), o.clone(), no_extra, false);
        let mut c = config::load(Some(&path), false, &o).unwrap().config;
        c.incidents.open_after = 5;
        let loaded = ed.apply(c).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[incidents]\nopen_after = 5\n");
        assert_eq!(loaded.config.probe.interval, Dur::secs(15));
    }

    #[test]
    fn keeps_redacted_credentials() {
        let dir = TmpDir::new("redact");
        let path = dir.file();
        std::fs::write(&path, "[[targets]]\nname = \"Router\"\ncheck = \"https://admin:pw@router.local\"\n[[targets]]\nname = \"NAS\"\ncheck = \"https://nas.lan/health?token=s3cret\"\n").unwrap();
        let ed = Editor::new(path.clone(), vec![], no_extra, false);
        let mut c = config::load(Some(&path), true, &[]).unwrap().config.redacted();
        c.targets[0].critical = true;
        ed.apply(c).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("admin:pw@router.local") && text.contains("critical = true"), "{text}");
        assert!(text.contains("health?token=s3cret"), "{text}");
    }

    #[test]
    fn read_only_refuses() {
        let dir = TmpDir::new("ro");
        let ed = Editor::new(dir.file(), vec![], no_extra, true);
        assert!(ed.apply(Config::default()).is_err());
    }
}
