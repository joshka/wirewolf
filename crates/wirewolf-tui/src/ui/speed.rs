//! The Speed view: the latest result as big numbers, live progress, history chart and table.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Axis, Block, BorderType, Chart, Dataset, GraphType, LegendPosition, LineGauge, Padding, Row,
    Table,
};
use tui_big_text::{BigText, PixelSize};
use wirewolf_core::model::{SpeedRun, SpeedState};

use super::theme;
use crate::app::App;
use crate::format;

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let running = app.running_speed();
    let latest = running.or_else(|| app.speed.iter().find(|r| r.state == SpeedState::Complete));
    let [cards, progress, rest] = Layout::vertical([
        Constraint::Length(8),
        Constraint::Length(if running.is_some() { 2 } else { 1 }),
        Constraint::Fill(1),
    ])
    .areas(area);
    render_cards(latest, frame, cards);
    match running {
        Some(run) => render_progress(run, frame, progress),
        None => {
            let line = match latest {
                Some(run) => Line::from(vec![
                    Span::styled(" Last run ", theme::muted()),
                    Span::styled(format::ago(run.started, app.now), theme::text()),
                    Span::styled(
                        format!(
                            " · {} · {} · ",
                            run.server.as_deref().unwrap_or("unknown server"),
                            run.backend
                        ),
                        theme::muted(),
                    ),
                    Span::styled("r", theme::key()),
                    Span::styled(" run now", theme::muted()),
                ]),
                None => Line::from(vec![
                    Span::styled(" No speed tests yet. Press ", theme::muted()),
                    Span::styled("r", theme::key()),
                    Span::styled(" to run one.", theme::muted()),
                ]),
            };
            frame.render_widget(line, progress);
        }
    }
    let [chart, table] = if rest.width >= 140 {
        Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)])
            .spacing(1)
            .areas(rest)
    } else {
        Layout::vertical([Constraint::Fill(1), Constraint::Fill(1)]).areas(rest)
    };
    render_chart(app, frame, chart);
    render_table(app, frame, table);
}

fn render_cards(run: Option<&SpeedRun>, frame: &mut Frame, area: Rect) {
    let areas = Layout::horizontal([Constraint::Fill(1); 4])
        .spacing(1)
        .split(area);
    let number = |v: Option<f64>| {
        v.map(|v| {
            if v < 10.0 {
                format!("{v:.1}")
            } else {
                format!("{v:.0}")
            }
        })
        .unwrap_or("-".into())
    };
    let run = run.cloned();
    let get = |f: fn(&SpeedRun) -> Option<f64>| run.as_ref().and_then(f);
    big_card(
        frame,
        areas[0],
        "↓ Download",
        &number(get(|r| r.down_mbps)),
        "Mbps",
        theme::TEXT,
    );
    big_card(
        frame,
        areas[1],
        "↑ Upload",
        &number(get(|r| r.up_mbps)),
        "Mbps",
        theme::TEXT,
    );
    let idle = get(|r| r.idle_latency_ms)
        .map(format::ms)
        .unwrap_or("—".into());
    big_card(
        frame,
        areas[2],
        "Loaded latency",
        &number(get(|r| r.loaded_latency_ms)),
        &format!("ms · {idle} idle"),
        theme::TEXT,
    );
    let grade = run.as_ref().and_then(|r| r.bloat);
    big_card(
        frame,
        areas[3],
        "Bufferbloat",
        grade.map(format::grade).unwrap_or("-"),
        "grade",
        grade.map(theme::grade).unwrap_or(theme::MUTED),
    );
}

fn big_card(frame: &mut Frame, area: Rect, label: &str, value: &str, unit: &str, color: Color) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::border())
        .title(Line::from(format!(" {label} ")).style(theme::muted()))
        .padding(Padding::new(2, 1, 1, 0));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [big, unit_area] =
        Layout::vertical([Constraint::Length(4), Constraint::Length(1)]).areas(inner);
    let text = BigText::builder()
        .pixel_size(PixelSize::Quadrant)
        .style(Style::new().fg(color).bold())
        .lines(vec![Line::from(value.to_string())])
        .build();
    frame.render_widget(text, big);
    frame.render_widget(Span::styled(unit.to_string(), theme::muted()), unit_area);
}

fn render_progress(run: &SpeedRun, frame: &mut Frame, area: Rect) {
    let progress = run.progress.unwrap_or(0.0).clamp(0.0, 1.0);
    let phase = match progress {
        p if p < 0.1 => "measuring idle latency",
        p if p < 0.55 => "downloading",
        p if p < 0.95 => "uploading",
        _ => "finishing",
    };
    let [label, gauge] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);
    frame.render_widget(
        Line::from(vec![
            Span::styled(" ● ", Style::new().fg(theme::ACCENT)),
            Span::styled("Running speed test", theme::title()),
            Span::styled(
                format!(" · {phase} · {}", run.server.as_deref().unwrap_or("")),
                theme::muted(),
            ),
        ]),
        label,
    );
    frame.render_widget(
        LineGauge::default()
            .ratio(progress)
            .label(Span::styled(
                format!(" {:>3.0}% ", progress * 100.0),
                theme::text(),
            ))
            .filled_symbol("━")
            .unfilled_symbol("─")
            .filled_style(Style::new().fg(theme::ACCENT))
            .unfilled_style(Style::new().fg(theme::BORDER)),
        gauge,
    );
}

fn render_chart(app: &App, frame: &mut Frame, area: Rect) {
    let mut runs: Vec<&SpeedRun> = app
        .speed
        .iter()
        .filter(|r| r.state == SpeedState::Complete)
        .collect();
    runs.reverse();
    let start = runs.first().map(|r| r.started).unwrap_or(app.now);
    let x = |r: &SpeedRun| r.started.duration_since(app.now).as_secs_f64() / 3600.0;
    let down: Vec<(f64, f64)> = runs
        .iter()
        .filter_map(|r| Some((x(r), r.down_mbps?)))
        .collect();
    let up: Vec<(f64, f64)> = runs
        .iter()
        .filter_map(|r| Some((x(r), r.up_mbps?)))
        .collect();
    let max = down.iter().chain(&up).map(|p| p.1).fold(10.0, f64::max) * 1.15;
    let x_min = (start.duration_since(app.now).as_secs_f64() / 3600.0).min(-1.0);
    let chart = Chart::new(vec![
        Dataset::default()
            .name("download")
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::new().fg(theme::ACCENT))
            .data(&down),
        Dataset::default()
            .name("upload")
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::new().fg(theme::SERIES[1]))
            .data(&up),
    ])
    .block(
        Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(theme::border())
            .title(Line::from(" Throughput ").style(theme::title()))
            .padding(Padding::horizontal(1)),
    )
    .legend_position(Some(LegendPosition::BottomRight))
    .x_axis(
        Axis::default()
            .bounds([x_min, 0.0])
            .labels([format::clock(start, app.now), "now".into()])
            .style(Style::new().fg(theme::FAINT)),
    )
    .y_axis(
        Axis::default()
            .bounds([0.0, max])
            .labels(["0".to_string(), format::mbps(max / 2.0), format::mbps(max)])
            .style(Style::new().fg(theme::FAINT)),
    );
    frame.render_widget(chart, area);
}

fn render_table(app: &App, frame: &mut Frame, area: Rect) {
    let ms = |v: Option<f64>| v.map(format::ms).unwrap_or("—".into());
    let mbps = |v: Option<f64>| v.map(format::mbps).unwrap_or("—".into());
    let rows = app.speed.iter().map(|r| {
        let grade = match r.bloat {
            Some(g) => Span::styled(format::grade(g), Style::new().fg(theme::grade(g)).bold()),
            None => Span::styled("—", theme::muted()),
        };
        let when = Line::from(vec![
            Span::styled(format::clock(r.started, app.now), theme::text()),
            Span::styled(
                format!(" {}", format::ago(r.started, app.now)),
                theme::muted(),
            ),
        ]);
        let state = match r.state {
            SpeedState::Running => Span::styled(
                format!("running {:.0}%", r.progress.unwrap_or(0.0) * 100.0),
                Style::new().fg(theme::ACCENT),
            ),
            SpeedState::Failed => Span::styled(
                r.error.clone().unwrap_or("failed".into()),
                Style::new().fg(theme::FAIL),
            ),
            SpeedState::Complete => {
                Span::styled(r.server.clone().unwrap_or_default(), theme::muted())
            }
        };
        Row::new(vec![
            when,
            Line::from(mbps(r.down_mbps)).right_aligned(),
            Line::from(mbps(r.up_mbps)).right_aligned(),
            Line::from(ms(r.idle_latency_ms)).right_aligned(),
            Line::from(ms(r.loaded_latency_ms)).right_aligned(),
            Line::from(grade).centered(),
            Line::from(state),
            Line::from(Span::styled(
                r.bytes_used.map(format::bytes).unwrap_or_default(),
                theme::muted(),
            ))
            .right_aligned(),
        ])
        .style(theme::text())
    });
    let header = Row::new(vec![
        Line::from("When"),
        Line::from("Down").right_aligned(),
        Line::from("Up").right_aligned(),
        Line::from("Idle").right_aligned(),
        Line::from("Loaded").right_aligned(),
        Line::from("Bloat").centered(),
        Line::from("Server"),
        Line::from("Data").right_aligned(),
    ])
    .style(theme::muted().bold());
    let table = Table::new(
        rows,
        [
            Constraint::Length(20),
            Constraint::Length(9),
            Constraint::Length(9),
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Length(5),
            Constraint::Fill(1),
            Constraint::Length(7),
        ],
    )
    .header(header)
    .column_spacing(2)
    .block(
        Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(theme::border())
            .title(Line::from(format!(" Runs · {} ", app.speed.len())).style(theme::title()))
            .padding(Padding::horizontal(1)),
    );
    frame.render_widget(table, area);
}
