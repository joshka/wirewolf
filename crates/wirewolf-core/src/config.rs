//! What wirewolf tests, and how hard: the `wirewolf.toml` schema and its layering.
//!
//! Settings come from three layers, lowest first: built-in defaults, the config file, then
//! environment variables and command line flags. [`load`] merges them and records where every
//! non-default value came from, which `GET /api/v1/config` and the Settings page show.
//!
//! The defaults must be good on their own: with no file at all, wirewolf tests three anycast
//! targets, four resolvers, captive portal, TLS, three HTTPS sites, and path MTU.
//! `wirewolf config default` prints [`DEFAULT_TOML`], which documents every key.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The default config, fully commented. Parsing it gives [`Config::default`].
pub const DEFAULT_TOML: &str = include_str!("default.toml");

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub server: Server,
    pub probe: Probe,
    pub internet: Internet,
    pub dns: Dns,
    pub web: Web,
    pub thresholds: Thresholds,
    pub incidents: Incidents,
    pub speed: Speed,
    pub storage: Storage,
    /// Server log verbosity.
    pub log: Log,
    /// Who may sign in, and what anonymous visitors may do.
    pub auth: Auth,
    /// Your own things to watch: a NAS, the router admin page, a Pi-hole.
    pub targets: Vec<UserTarget>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Server {
    pub listen: String,
    /// Extra host names the server answers to, beyond IP addresses, `localhost`, single-label
    /// names, local suffixes (`.local`, `.lan`, `.home.arpa`, `.internal`, `.ts.net`), and the
    /// host in `auth.redirect_url`. `*.example.com` matches subdomains; `*` turns the check off.
    /// Other names get 421, which stops DNS rebinding attacks from web pages.
    pub allowed_hosts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Probe {
    /// Time between probe cycles. At least 5s.
    pub interval: Dur,
    /// `auto` tests IPv6 when the host has a global IPv6 address.
    pub ipv6: Ipv6Mode,
    /// Measure the path MTU toward the internet with don't-fragment pings (needs ICMP).
    pub path_mtu: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ipv6Mode {
    Auto,
    On,
    Off,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Internet {
    /// TCP port for the handshake test.
    pub port: u16,
    /// Also ping each target with ICMP where the host allows it. Informational only.
    pub icmp: bool,
    pub targets: Vec<InternetTarget>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InternetTarget {
    pub name: String,
    #[serde(default)]
    pub v4: Option<Ipv4Addr>,
    #[serde(default)]
    pub v6: Option<Ipv6Addr>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Dns {
    /// Test the resolvers this host is configured with.
    pub system: bool,
    /// Public resolvers to compare against. They tell "my resolver is broken" from "DNS is
    /// blocked".
    pub resolvers: Vec<Resolver>,
    /// Names asked of every resolver. The first is the headline check.
    pub queries: Vec<DnsQuery>,
    /// `A`, `AAAA`, or both.
    pub record_types: Vec<RecordType>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resolver {
    pub name: String,
    pub address: IpAddr,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DnsQuery {
    pub name: String,
    /// If set, an A answer must contain one of these, or the resolver is flagged as filtering or
    /// hijacking.
    #[serde(default)]
    pub expect: Vec<IpAddr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordType {
    A,
    #[serde(rename = "AAAA")]
    Aaaa,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Web {
    /// Must answer `204 No Content` over plain HTTP; anything else means a captive portal or an
    /// intercepting proxy.
    pub captive_portal: String,
    /// Host for the TLS handshake and certificate check.
    pub tls_host: String,
    /// HTTPS sites that must answer.
    pub sites: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Thresholds {
    pub gateway: Limits,
    pub isp: Limits,
    pub internet: Limits,
    pub dns: Limits,
    pub targets: Limits,
}

/// Above any of these, a hop is degraded (warn). Not every hop uses every limit.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    pub rtt_warn_ms: f64,
    pub jitter_warn_ms: f64,
    pub loss_warn_pct: f64,
}

impl Default for Limits {
    fn default() -> Self {
        Self { rtt_warn_ms: 150.0, jitter_warn_ms: 40.0, loss_warn_pct: 10.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Incidents {
    /// Consecutive unhealthy cycles before an incident opens. The incident starts at the first.
    pub open_after: u32,
    /// Consecutive healthy cycles before it closes. It ends at the first.
    pub close_after: u32,
}

impl Default for Incidents {
    fn default() -> Self {
        Self { open_after: 2, close_after: 2 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Speed {
    /// `cloudflare` or `ookla`.
    pub backend: String,
    /// Time between scheduled tests. `0` disables them.
    pub interval: Dur,
    /// Ookla only: a `host:port` from speedtest.net to always use. Empty picks the fastest of the
    /// nearest servers.
    pub server: String,
    /// Parallel connections per direction.
    pub streams: usize,
    /// How long each direction runs.
    pub duration: Dur,
    /// The download speed you pay for, in Mbps, so results can be shown as a share of it.
    /// `0` means not set.
    pub plan_down_mbps: f64,
    /// The upload speed you pay for, in Mbps. `0` means not set.
    pub plan_up_mbps: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Storage {
    /// SQLite history file.
    pub db: PathBuf,
    pub retention: Dur,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Log {
    /// Which log lines to keep, in `RUST_LOG` syntax, for example `info,wirewolf_probe=debug`.
    /// Changes apply without a restart. `RUST_LOG`, when set, wins at startup.
    pub filter: String,
}

impl Default for Log {
    fn default() -> Self {
        Self { filter: crate::logs::DEFAULT_FILTER.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Auth {
    /// OIDC issuer URL, for example `https://auth.example.com`. Empty turns OIDC login off.
    pub issuer: String,
    pub client_id: String,
    /// Also `WIREWOLF_AUTH_CLIENT_SECRET`, or `client_secret_file`. Never shown by the API.
    pub client_secret: String,
    /// A file holding the client secret, for Docker or systemd credentials.
    pub client_secret_file: String,
    /// Where the issuer sends the browser back: `https://<this server>/auth/callback`.
    pub redirect_url: String,
    /// Scopes beyond `openid`. Add `groups` if your issuer needs it for `allowed_groups`.
    pub scopes: Vec<String>,
    /// If set, only these emails may sign in. Matched case-insensitively.
    pub allowed_emails: Vec<String>,
    /// If set, only members of one of these groups (the `groups` claim) may sign in.
    pub allowed_groups: Vec<String>,
    /// Let anyone the issuer signs in use wirewolf when both lists above are empty. Without
    /// this, OIDC with no allowlist is refused at startup: with a public issuer such as Google
    /// it would let any account change settings.
    pub allow_any_user: bool,
    /// Sign-in lifetime.
    pub session_ttl: Dur,
    /// Key that signs and encrypts session cookies. Created on first start; keep it so restarts
    /// don't sign everyone out.
    pub session_key_file: PathBuf,
    /// Require sign-in to see the dashboard and API, not just to change things.
    pub require_for_reads: bool,
    /// With no sign-in method configured, let anyone on the network run speed tests and change
    /// settings. Off by default.
    pub allow_unauthenticated_writes: bool,
    /// Trust a reverse proxy that has already signed the user in (oauth2-proxy, Authelia,
    /// Authentik, Tailscale serve).
    pub trusted_header: TrustedHeader,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TrustedHeader {
    /// Header carrying the user name, for example `Remote-User`. Empty turns this off.
    pub user: String,
    pub email: String,
    pub name: String,
    /// Comma-separated groups, checked against `allowed_groups`.
    pub groups: String,
    /// Proxy addresses or CIDRs to trust the headers from. Required: requests from anywhere
    /// else are anonymous.
    pub proxies: Vec<String>,
}

impl Auth {
    pub fn oidc_enabled(&self) -> bool {
        !self.issuer.is_empty()
    }

    pub fn header_enabled(&self) -> bool {
        !self.trusted_header.user.is_empty()
    }
}

impl Default for Auth {
    fn default() -> Self {
        Self {
            issuer: String::new(),
            client_id: String::new(),
            client_secret: String::new(),
            client_secret_file: String::new(),
            redirect_url: String::new(),
            scopes: vec!["email".into(), "profile".into()],
            allowed_emails: vec![],
            allowed_groups: vec![],
            allow_any_user: false,
            session_ttl: Dur::secs(7 * 24 * 60 * 60),
            session_key_file: "wirewolf-session.key".into(),
            require_for_reads: false,
            allow_unauthenticated_writes: false,
            trusted_header: TrustedHeader::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserTarget {
    pub name: String,
    /// `tcp://host:port`, `http(s)://…`, `ping://host`, or `dns://server[:port]`.
    pub check: String,
    /// A failing critical target degrades the verdict and can open an incident.
    #[serde(default)]
    pub critical: bool,
    /// HTTPS only: check the certificate. Turn off for self-signed admin pages.
    #[serde(default = "yes")]
    pub verify_tls: bool,
}

fn yes() -> bool {
    true
}

impl Default for Server {
    fn default() -> Self {
        Self { listen: "0.0.0.0:7465".into(), allowed_hosts: vec![] }
    }
}

impl Default for Probe {
    fn default() -> Self {
        Self { interval: Dur::secs(30), ipv6: Ipv6Mode::Auto, path_mtu: true }
    }
}

impl Default for Internet {
    fn default() -> Self {
        let t = |name: &str, v4: [u8; 4], v6: Ipv6Addr| InternetTarget { name: name.into(), v4: Some(v4.into()), v6: Some(v6) };
        Self {
            port: 443,
            icmp: true,
            targets: vec![
                t("Cloudflare", [1, 1, 1, 1], Ipv6Addr::new(0x2606, 0x4700, 0x4700, 0, 0, 0, 0, 0x1111)),
                t("Google", [8, 8, 8, 8], Ipv6Addr::new(0x2001, 0x4860, 0x4860, 0, 0, 0, 0, 0x8888)),
                t("Quad9", [9, 9, 9, 9], Ipv6Addr::new(0x2620, 0xfe, 0, 0, 0, 0, 0, 0xfe)),
            ],
        }
    }
}

impl Default for Dns {
    fn default() -> Self {
        let r = |name: &str, a: [u8; 4]| Resolver { name: name.into(), address: IpAddr::from(a) };
        Self {
            system: true,
            resolvers: vec![r("Cloudflare", [1, 1, 1, 1]), r("Google", [8, 8, 8, 8]), r("Quad9", [9, 9, 9, 9])],
            queries: vec![
                DnsQuery { name: "one.one.one.one".into(), expect: vec![IpAddr::from([1, 1, 1, 1]), IpAddr::from([1, 0, 0, 1])] },
                DnsQuery { name: "www.google.com".into(), expect: vec![] },
                DnsQuery { name: "www.wikipedia.org".into(), expect: vec![] },
            ],
            record_types: vec![RecordType::A, RecordType::Aaaa],
        }
    }
}

impl Default for Web {
    fn default() -> Self {
        Self {
            captive_portal: "http://cp.cloudflare.com/generate_204".into(),
            tls_host: "www.cloudflare.com".into(),
            sites: vec!["https://www.google.com".into(), "https://www.wikipedia.org".into(), "https://github.com".into()],
        }
    }
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            // Wi-Fi to the router is routinely 5–20 ms, and routers answer ICMP at low priority,
            // so only a clearly slow median counts.
            gateway: Limits { rtt_warn_ms: 30.0, jitter_warn_ms: 30.0, loss_warn_pct: 10.0 },
            // Routers on the way rate-limit ICMP, so ISP loss only counts alongside internet loss.
            isp: Limits { loss_warn_pct: 25.0, ..Limits::default() },
            internet: Limits::default(),
            dns: Limits { rtt_warn_ms: 250.0, ..Limits::default() },
            targets: Limits { rtt_warn_ms: 250.0, ..Limits::default() },
        }
    }
}

impl Default for Speed {
    fn default() -> Self {
        Self { backend: "cloudflare".into(), interval: Dur::secs(60 * 60), server: String::new(), streams: 8, duration: Dur::secs(10), plan_down_mbps: 0.0, plan_up_mbps: 0.0 }
    }
}

impl Default for Storage {
    fn default() -> Self {
        Self { db: "wirewolf.db".into(), retention: Dur::secs(30 * 24 * 60 * 60) }
    }
}

impl Config {
    /// A copy safe to show: credentials in URLs are replaced with `***`.
    pub fn redacted(&self) -> Self {
        let mut c = self.clone();
        for t in &mut c.targets {
            t.check = redact_url(&t.check);
        }
        c.web.sites = c.web.sites.iter().map(|s| redact_url(s)).collect();
        c.web.captive_portal = redact_url(&c.web.captive_portal);
        if !c.auth.client_secret.is_empty() {
            c.auth.client_secret = "***".into();
        }
        c
    }
}

/// Hides credentials in a URL: `user:pass@` becomes `***@`, and query values (`?token=…`)
/// become `***`, since tokens often travel there.
pub fn redact_url(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else { return url.to_string() };
    let authority_end = rest.find('/').or_else(|| rest.find('?')).unwrap_or(rest.len());
    let rest = match rest[..authority_end].rfind('@') {
        Some(at) => format!("***@{}", &rest[at + 1..]),
        None => rest.to_string(),
    };
    let rest = match rest.split_once('?') {
        Some((path, query)) => {
            let (query, fragment) = query.split_once('#').map_or((query, None), |(q, f)| (q, Some(f)));
            let query: Vec<String> = query.split('&').map(|kv| kv.split_once('=').map_or(kv.to_string(), |(k, _)| format!("{k}=***"))).collect();
            format!("{path}?{}{}", query.join("&"), fragment.map(|f| format!("#{f}")).unwrap_or_default())
        }
        None => rest,
    };
    format!("{scheme}://{rest}")
}

/// A duration written like `30s`, `5m`, `1h`, or `30d`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Dur(pub Duration);

impl Dur {
    pub const fn secs(s: u64) -> Self {
        Self(Duration::from_secs(s))
    }
}

impl std::str::FromStr for Dur {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        if s.trim() == "0" {
            return Ok(Self(Duration::ZERO));
        }
        let span: jiff::Span = s.parse().map_err(|e| format!("invalid duration {s:?}: {e}"))?;
        let d = span.to_duration(jiff::SpanRelativeTo::days_are_24_hours()).map_err(|e| format!("{e}"))?;
        Ok(Self(d.try_into().map_err(|e| format!("invalid duration {s:?}: {e}"))?))
    }
}

impl std::fmt::Display for Dur {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self.0.as_secs();
        if self.0.subsec_nanos() != 0 {
            return write!(f, "{}ms", self.0.as_millis());
        }
        match s {
            0 => write!(f, "0"),
            s if s % 86400 == 0 => write!(f, "{}d", s / 86400),
            s if s % 3600 == 0 => write!(f, "{}h", s / 3600),
            s if s % 60 == 0 => write!(f, "{}m", s / 60),
            s => write!(f, "{s}s"),
        }
    }
}

impl Serialize for Dur {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Dur {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Where a setting came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Default,
    File,
    Env,
    Flag,
}

/// The merged config and where each value came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    pub config: Config,
    pub path: Option<PathBuf>,
    /// Dotted paths of non-default settings. Lists count as one setting.
    pub sources: BTreeMap<String, Source>,
}

impl Loaded {
    pub fn view(&self) -> crate::model::ConfigView {
        crate::model::ConfigView {
            path: self.path.as_ref().map(|p| p.display().to_string()),
            config: self.config.redacted(),
            sources: self.sources.clone(),
            editable: false,
            save_path: None,
            restart_keys: Vec::new(),
            defaults: Config::default().redacted(),
        }
    }
}

/// One override from the environment or a flag, for example `("probe.interval", "10s")`.
#[derive(Debug, Clone)]
pub struct Override {
    /// Dotted path, for example `probe.interval`.
    pub key: String,
    pub value: String,
    pub source: Source,
    /// The variable or flag it came from, for errors: `WIREWOLF_PROBE_INTERVAL`, `--interval`.
    pub name: String,
}

/// The environment variable for a setting: `auth.trusted_header.user` is
/// `WIREWOLF_AUTH_TRUSTED_HEADER_USER`.
pub fn env_name(key: &str) -> String {
    format!("WIREWOLF_{}", key.replace('.', "_").to_ascii_uppercase())
}

/// Every setting that can be set on its own, as dotted paths. Lists (including lists of tables
/// such as `targets`) are one setting each.
pub fn keys() -> Vec<String> {
    fn walk(value: &Value, path: &mut String, out: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                for (k, v) in map {
                    let len = path.len();
                    if !path.is_empty() {
                        path.push('.');
                    }
                    path.push_str(k);
                    walk(v, path, out);
                    path.truncate(len);
                }
            }
            _ => out.push(path.clone()),
        }
    }
    let mut out = vec![];
    walk(&serde_json::to_value(Config::default()).expect("config serializes"), &mut String::new(), &mut out);
    out
}

/// Overrides from `WIREWOLF_<SECTION>_<KEY>` variables, for example `WIREWOLF_AUTH_ISSUER` or
/// `WIREWOLF_SERVER_ALLOWED_HOSTS`. Variables that aren't settings (`WIREWOLF_CONFIG`, …) are
/// ignored.
pub fn env_overrides(vars: impl IntoIterator<Item = (String, String)>) -> Vec<Override> {
    let by_name: BTreeMap<String, String> = keys().into_iter().map(|k| (env_name(&k), k)).collect();
    let mut out: Vec<Override> = vars
        .into_iter()
        .filter_map(|(name, value)| by_name.get(&name).map(|key| Override { key: key.clone(), value, source: Source::Env, name }))
        .collect();
    out.sort_by(|a, b| a.key.cmp(&b.key));
    out
}

/// Lists of tables, which only take the TOML array form.
const TABLE_LISTS: &[&str] = &["targets", "internet.targets", "dns.resolvers", "dns.queries"];

/// Turns an override's text into a value shaped like `slot`: lists are comma-separated or a TOML
/// array, booleans are true/false (or 1/0, yes/no, on/off).
fn override_value(slot: &Value, o: &Override) -> Result<Value, String> {
    let text = o.value.trim();
    let bad = |what: &str| format!("{}={:?}: {what}", o.name, o.value);
    let toml_literal = |text: &str| -> Result<Value, String> {
        let table: toml::Table = toml::from_str(&format!("v = {text}")).map_err(|e| bad(&format!("not a TOML value: {}", e.message())))?;
        serde_json::to_value(&table["v"]).map_err(|e| bad(&e.to_string()))
    };
    Ok(match slot {
        Value::Array(_) if text.starts_with('[') => toml_literal(text)?,
        Value::Array(_) if TABLE_LISTS.contains(&o.key.as_str()) => return Err(bad("write this list as a TOML array, for example [{ name = \"NAS\", check = \"tcp://nas.lan:445\" }]")),
        Value::Array(_) => Value::Array(text.split(',').map(str::trim).filter(|x| !x.is_empty()).map(|x| Value::String(x.into())).collect()),
        Value::Number(_) => toml_literal(text).ok().filter(Value::is_number).ok_or_else(|| bad("not a number"))?,
        Value::Bool(_) => match text.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Value::Bool(true),
            "0" | "false" | "no" | "off" => Value::Bool(false),
            _ => return Err(bad("expected true or false")),
        },
        _ => Value::String(o.value.clone()),
    })
}

/// Merges defaults, the file at `path` (if it exists), and `overrides`, in that order.
///
/// A missing file is an error only when `required` (it was named on the command line).
pub fn load(path: Option<&Path>, required: bool, overrides: &[Override]) -> Result<Loaded, String> {
    let mut merged = serde_json::to_value(Config::default()).expect("config serializes");
    let mut sources = BTreeMap::new();
    let mut used = None;
    if let Some(path) = path {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let file = parse_toml(&text).map_err(|e| format!("{}: {e}", path.display()))?;
                merge(&mut merged, file, &mut String::new(), &mut sources);
                used = Some(path.to_path_buf());
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && !required => {}
            Err(e) => return Err(format!("can't read {}: {e}", path.display())),
        }
    }
    for o in overrides {
        let mut slot = &mut merged;
        for part in o.key.split('.') {
            slot = slot.get_mut(part).unwrap_or_else(|| panic!("override for unknown key {}", o.key));
        }
        *slot = override_value(slot, o)?;
        // Check each one as it lands, so a bad value is blamed on the variable that set it.
        if let Err(e) = serde_json::from_value::<Config>(merged.clone()) {
            return Err(format!("{}={:?}: {e}", o.name, o.value));
        }
        sources.insert(o.key.clone(), o.source);
    }
    let config: Config = serde_json::from_value(merged).map_err(|e| format!("invalid config: {e}"))?;
    // Values set to their default still count as set: the user wrote them down.
    Ok(Loaded { config, path: used, sources })
}

/// Parses a config file, checking it against the schema so typos are reported with their key.
pub fn parse_toml(text: &str) -> Result<Value, String> {
    let _: Config = toml::from_str(text).map_err(|e| e.to_string())?;
    let value: toml::Value = toml::from_str(text).map_err(|e| e.to_string())?;
    serde_json::to_value(value).map_err(|e| e.to_string())
}

/// Deep-merges tables; arrays and scalars replace. Records each replaced leaf as from the file.
fn merge(base: &mut Value, over: Value, path: &mut String, sources: &mut BTreeMap<String, Source>) {
    match (base, over) {
        (Value::Object(b), Value::Object(o)) => {
            for (k, v) in o {
                let len = path.len();
                if !path.is_empty() {
                    path.push('.');
                }
                path.push_str(&k);
                match b.get_mut(&k) {
                    Some(slot) => merge(slot, v, path, sources),
                    None => {
                        sources.insert(path.clone(), Source::File);
                        b.insert(k, v);
                    }
                }
                path.truncate(len);
            }
        }
        (slot, v) => {
            sources.insert(path.clone(), Source::File);
            *slot = v;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_toml_is_the_default() {
        let parsed: Config = toml::from_str(DEFAULT_TOML).unwrap();
        assert_eq!(parsed, Config::default());
    }

    #[test]
    fn layers_file_then_overrides() {
        let dir = std::env::temp_dir().join(format!("wirewolf-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wirewolf.toml");
        std::fs::write(
            &path,
            r#"
[probe]
interval = "10s"
[thresholds.gateway]
rtt_warn_ms = 50
[[targets]]
name = "NAS"
check = "tcp://nas.local:445"
critical = true
"#,
        )
        .unwrap();
        let o = [Override { key: "probe.interval".into(), value: "15s".into(), source: Source::Env, name: "WIREWOLF_INTERVAL".into() }];
        let loaded = load(Some(&path), true, &o).unwrap();
        assert_eq!(loaded.config.probe.interval, Dur::secs(15));
        assert_eq!(loaded.config.thresholds.gateway.rtt_warn_ms, 50.0);
        assert_eq!(loaded.config.thresholds.gateway.loss_warn_pct, 10.0);
        assert_eq!(loaded.config.targets[0].name, "NAS");
        assert!(loaded.config.targets[0].verify_tls);
        assert_eq!(loaded.sources.get("probe.interval"), Some(&Source::Env));
        assert_eq!(loaded.sources.get("thresholds.gateway.rtt_warn_ms"), Some(&Source::File));
        assert_eq!(loaded.sources.get("targets"), Some(&Source::File));
        assert_eq!(loaded.sources.get("dns.queries"), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_file_is_fine_unless_required() {
        let path = Path::new("/nonexistent/wirewolf.toml");
        assert_eq!(load(Some(path), false, &[]).unwrap().config, Config::default());
        assert!(load(Some(path), true, &[]).is_err());
    }

    #[test]
    fn rejects_unknown_keys() {
        let err = parse_toml("[probe]\nintervall = \"5s\"\n").unwrap_err();
        assert!(err.contains("intervall"), "{err}");
    }

    #[test]
    fn durations_round_trip() {
        for s in ["30s", "5m", "1h", "30d", "0"] {
            assert_eq!(s.parse::<Dur>().unwrap().to_string(), s);
        }
        assert_eq!("90s".parse::<Dur>().unwrap().to_string(), "90s");
    }

    #[test]
    fn redacts_credentials() {
        assert_eq!(redact_url("https://admin:hunter2@router.local/x@y"), "https://***@router.local/x@y");
        assert_eq!(redact_url("tcp://nas.local:445"), "tcp://nas.local:445");
        assert_eq!(redact_url("https://nas.lan/health?token=s3cret&v=2#top"), "https://nas.lan/health?token=***&v=***#top");
        assert_eq!(redact_url("https://u:p@nas.lan?key=x"), "https://***@nas.lan?key=***");
        let mut c = Config::default();
        c.auth.client_secret = "s3cret".into();
        assert_eq!(c.redacted().auth.client_secret, "***");
    }

    fn env(vars: &[(&str, &str)]) -> Vec<Override> {
        env_overrides(vars.iter().map(|(k, v)| (k.to_string(), v.to_string())))
    }

    #[test]
    fn every_key_has_its_own_variable() {
        let keys = keys();
        let names: std::collections::BTreeSet<String> = keys.iter().map(|k| env_name(k)).collect();
        assert_eq!(names.len(), keys.len(), "two settings share a variable name");
        for k in ["auth.issuer", "auth.trusted_header.user", "server.allowed_hosts", "thresholds.gateway.rtt_warn_ms", "targets"] {
            assert!(keys.iter().any(|x| x == k), "{k}");
        }
        for k in TABLE_LISTS {
            assert!(keys.iter().any(|x| x == k), "{k}");
        }
        assert_eq!(env_name("auth.trusted_header.proxies"), "WIREWOLF_AUTH_TRUSTED_HEADER_PROXIES");
    }

    #[test]
    fn env_beats_file_beats_default() {
        let dir = std::env::temp_dir().join(format!("wirewolf-config-env-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wirewolf.toml");
        std::fs::write(&path, "[auth]\nissuer = \"https://file.example\"\nclient_id = \"from-file\"\n").unwrap();
        let o = env(&[("WIREWOLF_AUTH_ISSUER", "https://id.example.com"), ("WIREWOLF_CONFIG", "/ignored"), ("PATH", "/bin")]);
        assert_eq!(o.len(), 1);
        let loaded = load(Some(&path), true, &o).unwrap();
        assert_eq!(loaded.config.auth.issuer, "https://id.example.com");
        assert_eq!(loaded.config.auth.client_id, "from-file");
        assert_eq!(loaded.config.auth.scopes, Config::default().auth.scopes);
        assert_eq!(loaded.sources.get("auth.issuer"), Some(&Source::Env));
        assert_eq!(loaded.sources.get("auth.client_id"), Some(&Source::File));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn env_values_parse_like_the_file() {
        let o = env(&[
            ("WIREWOLF_SERVER_ALLOWED_HOSTS", "wirewolf.home.example.com, *.lan"),
            ("WIREWOLF_AUTH_ALLOWED_GROUPS", "[\"wirewolf-admins\", \"ops\"]"),
            ("WIREWOLF_AUTH_SCOPES", ""),
            ("WIREWOLF_AUTH_REQUIRE_FOR_READS", "yes"),
            ("WIREWOLF_PROBE_INTERVAL", "45s"),
            ("WIREWOLF_INCIDENTS_OPEN_AFTER", "3"),
            ("WIREWOLF_THRESHOLDS_GATEWAY_RTT_WARN_MS", "12.5"),
            ("WIREWOLF_TARGETS", "[{ name = \"NAS\", check = \"tcp://nas.lan:445\", critical = true }]"),
        ]);
        let c = load(None, false, &o).unwrap().config;
        assert_eq!(c.server.allowed_hosts, ["wirewolf.home.example.com", "*.lan"]);
        assert_eq!(c.auth.allowed_groups, ["wirewolf-admins", "ops"]);
        assert!(c.auth.scopes.is_empty());
        assert!(c.auth.require_for_reads);
        assert_eq!(c.probe.interval, Dur::secs(45));
        assert_eq!(c.incidents.open_after, 3);
        assert_eq!(c.thresholds.gateway.rtt_warn_ms, 12.5);
        assert_eq!(c.targets[0].name, "NAS");
        assert!(c.targets[0].critical && c.targets[0].verify_tls);
    }

    #[test]
    fn bad_env_values_name_the_variable() {
        for (name, value, needle) in [
            ("WIREWOLF_PROBE_INTERVAL", "soon", "invalid duration"),
            ("WIREWOLF_AUTH_REQUIRE_FOR_READS", "maybe", "expected true or false"),
            ("WIREWOLF_INCIDENTS_OPEN_AFTER", "two", "not a number"),
            ("WIREWOLF_PROBE_IPV6", "sometimes", "unknown variant"),
            ("WIREWOLF_TARGETS", "nas", "TOML array"),
            ("WIREWOLF_DNS_RESOLVERS", "[{ name = \"x\" }]", "address"),
        ] {
            let err = load(None, false, &env(&[(name, value)])).unwrap_err();
            assert!(err.starts_with(name) && err.contains(needle), "{name}: {err}");
        }
    }
}
