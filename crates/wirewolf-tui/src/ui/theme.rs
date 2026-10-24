//! Colors and styles. Neutral surfaces, one accent, status colors used sparingly and always
//! paired with an icon or a word.

use ratatui::style::palette::tailwind::{AMBER, EMERALD, RED, SKY, ZINC};
use ratatui::style::{Color, Modifier, Style};
use wirewolf_core::model::{BloatGrade, Severity, Status};

pub const ACCENT: Color = SKY.c400;
pub const TEXT: Color = ZINC.c200;
pub const MUTED: Color = ZINC.c500;
pub const FAINT: Color = ZINC.c600;
pub const BORDER: Color = ZINC.c700;
pub const PASS: Color = EMERALD.c400;
pub const WARN: Color = AMBER.c400;
pub const FAIL: Color = RED.c400;
pub const SURFACE: Color = ZINC.c900;

/// Distinct series colors for per-hop charts, in path order.
pub const SERIES: [Color; 6] = [
    ZINC.c400,
    Color::Rgb(167, 139, 250), // violet-400
    Color::Rgb(56, 189, 248),  // sky-400
    Color::Rgb(52, 211, 153),  // emerald-400
    Color::Rgb(251, 191, 36),  // amber-400
    Color::Rgb(244, 114, 182), // pink-400
];

pub fn status(status: Status) -> Color {
    match status {
        Status::Pass => PASS,
        Status::Warn => WARN,
        Status::Fail => FAIL,
        Status::Skip | Status::NotApplicable | Status::Pending => MUTED,
    }
}

pub fn severity(severity: Severity) -> Color {
    match severity {
        Severity::Healthy => PASS,
        Severity::Degraded => WARN,
        Severity::Down => FAIL,
        Severity::Unknown => MUTED,
    }
}

/// A dark wash of a severity color, for incident bands.
pub fn severity_band(severity: Severity) -> Color {
    match severity {
        Severity::Down => RED.c950,
        Severity::Degraded => AMBER.c950,
        _ => ZINC.c800,
    }
}

pub fn grade(grade: BloatGrade) -> Color {
    match grade {
        BloatGrade::APlus | BloatGrade::A => PASS,
        BloatGrade::B | BloatGrade::C => WARN,
        BloatGrade::D | BloatGrade::F => FAIL,
    }
}

pub fn text() -> Style {
    Style::new().fg(TEXT)
}

pub fn muted() -> Style {
    Style::new().fg(MUTED)
}

pub fn border() -> Style {
    Style::new().fg(BORDER)
}

pub fn title() -> Style {
    Style::new().fg(TEXT).add_modifier(Modifier::BOLD)
}

pub fn key() -> Style {
    Style::new().fg(ACCENT).bold()
}
