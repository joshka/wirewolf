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
}
