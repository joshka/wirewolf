//! Number and time formatting shared by the TUI, the report, and `wirewolf check`.

use jiff::{SignedDuration, Timestamp, tz::TimeZone};
use wirewolf_core::model::{BloatGrade, Severity, Status};

/// `1.4 ms`, `42 ms`.
pub fn ms(value: f64) -> String {
    if value == 0.0 {
        "0 ms".into()
    } else if value < 10.0 {
        format!("{value:.1} ms")
    } else {
        format!("{value:.0} ms")
    }
}

/// `912 Mbps`, `8.4 Mbps`.
pub fn mbps(value: f64) -> String {
    if value < 10.0 {
        format!("{value:.1} Mbps")
    } else {
        format!("{value:.0} Mbps")
    }
}

/// `0% loss`, `2.5% loss`.
pub fn loss(value: f64) -> String {
    format!("{} loss", pct(value))
}

/// `0%`, `2.5%`.
pub fn pct(value: f64) -> String {
    if value.fract().abs() < 0.05 {
        format!("{value:.0}%")
    } else {
        format!("{value:.1}%")
    }
}

/// `900 MB`, `1.2 GB`.
pub fn bytes(value: u64) -> String {
    let mb = value as f64 / 1e6;
    if mb >= 1000.0 {
        format!("{:.1} GB", mb / 1000.0)
    } else {
        format!("{mb:.0} MB")
    }
}

/// `45 s`, `3 min`, `1 h 20 min`, `2 d 3 h`.
pub fn duration(duration: SignedDuration) -> String {
    let secs = duration.as_secs().max(0);
    match secs {
        s if s < 60 => format!("{s} s"),
        s if s < 3600 => format!("{} min", s / 60),
        s if s < 86_400 => match (s / 3600, (s % 3600) / 60) {
            (h, 0) => format!("{h} h"),
            (h, m) => format!("{h} h {m} min"),
        },
        s => match (s / 86_400, (s % 86_400) / 3600) {
            (d, 0) => format!("{d} d"),
            (d, h) => format!("{d} d {h} h"),
        },
    }
}

/// `just now`, `3 min ago`.
pub fn ago(at: Timestamp, now: Timestamp) -> String {
    let elapsed = now.duration_since(at);
    if elapsed.as_secs() < 10 {
        "just now".into()
    } else {
        format!("{} ago", duration(elapsed))
    }
}

/// Local wall-clock time, `14:05`, or `Oct 23 14:05` when not today.
pub fn clock(at: Timestamp, now: Timestamp) -> String {
    let tz = TimeZone::system();
    let at = at.to_zoned(tz.clone());
    if at.date() == now.to_zoned(tz).date() {
        at.strftime("%H:%M").to_string()
    } else {
        at.strftime("%b %d %H:%M").to_string()
    }
}

/// Local date and time with zone, for reports: `2026-10-24 15:40 AEDT`.
pub fn datetime(at: Timestamp) -> String {
    at.to_zoned(TimeZone::system())
        .strftime("%Y-%m-%d %H:%M:%S %Z")
        .to_string()
}

/// A hop target, short enough for a column.
///
/// Lists collapse to the first entry plus a count, and entries that share a label collapse
/// under it: `100.100.100.100 (Tailscale MagicDNS), fd7a::53 (Tailscale MagicDNS)` becomes
/// `Tailscale MagicDNS (100.100.100.100 +1)`. The result is cut to `max` characters with `…`.
pub fn target(target: &str, max: usize) -> String {
    let entries: Vec<(&str, Option<&str>)> = target
        .split(", ")
        .map(|entry| match entry.strip_suffix(')').and_then(|e| e.split_once(" (")) {
            Some((address, label)) => (address, Some(label)),
            None => (entry, None),
        })
        .collect();
    let more = |n: usize| if n > 1 { format!(" +{}", n - 1) } else { String::new() };
    let short = match entries.as_slice() {
        [] | [_] => target.to_string(),
        [(address, Some(label)), rest @ ..] if rest.iter().all(|(_, l)| *l == Some(*label)) => {
            format!("{label} ({address}{})", more(entries.len()))
        }
        [(address, label), ..] => {
            let label = label.map(|l| format!(" ({l})")).unwrap_or_default();
            format!("{address}{label}{}", more(entries.len()))
        }
    };
    truncate(&short, max)
}

/// Cuts `text` to at most `max` characters, ending with `…` when cut.
pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

pub fn status_icon(status: Status) -> &'static str {
    match status {
        Status::Pass => "✔",
        Status::Warn => "▲",
        Status::Fail => "✖",
        Status::Skip => "○",
        Status::NotApplicable => "–",
        Status::Pending => "…",
    }
}

pub fn status_word(status: Status) -> &'static str {
    match status {
        Status::Pass => "pass",
        Status::Warn => "warn",
        Status::Fail => "fail",
        Status::Skip => "skip",
        Status::NotApplicable => "n/a",
        Status::Pending => "pending",
    }
}

pub fn severity_icon(severity: Severity) -> &'static str {
    match severity {
        Severity::Healthy => "✔",
        Severity::Degraded => "▲",
        Severity::Down => "✖",
        Severity::Unknown => "?",
    }
}

pub fn severity_word(severity: Severity) -> &'static str {
    match severity {
        Severity::Healthy => "healthy",
        Severity::Degraded => "degraded",
        Severity::Down => "down",
        Severity::Unknown => "unknown",
    }
}

pub fn grade(grade: BloatGrade) -> &'static str {
    match grade {
        BloatGrade::APlus => "A+",
        BloatGrade::A => "A",
        BloatGrade::B => "B",
        BloatGrade::C => "C",
        BloatGrade::D => "D",
        BloatGrade::F => "F",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(ms(1.42), "1.4 ms");
        assert_eq!(ms(42.3), "42 ms");
        assert_eq!(mbps(912.4), "912 Mbps");
        assert_eq!(loss(0.0), "0% loss");
        assert_eq!(loss(2.5), "2.5% loss");
        assert_eq!(duration(SignedDuration::from_secs(4800)), "1 h 20 min");
    }

    #[test]
    fn targets() {
        let dns = "100.100.100.100 (Tailscale MagicDNS), fd7a:115c:a1e0::53 (Tailscale MagicDNS)";
        assert_eq!(target(dns, 40), "Tailscale MagicDNS (100.100.100.100 +1)");
        assert_eq!(target("192.168.1.1, 1.1.1.1 (Cloudflare)", 40), "192.168.1.1 +1");
        assert_eq!(target("Cloudflare · Google · Quad9", 40), "Cloudflare · Google · Quad9");
        assert_eq!(target("100.64.0.1 (AS64500 Example Fiber)", 40), "100.64.0.1 (AS64500 Example Fiber)");
        assert_eq!(target("abcdef", 4), "abc…");
    }
}
