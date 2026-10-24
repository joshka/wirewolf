//! The Now view: verdict, path, sparklines, latest speed test, and the hop detail popup.

use jiff::SignedDuration;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Axis, Block, BorderType, Chart, Dataset, GraphType, LineGauge, Padding, Paragraph, Row,
    Sparkline, SparklineBar, Table,
};
use wirewolf_core::model::{Sample, SpeedRun, SpeedState, Status};

use super::theme;
use super::widgets::{PathChain, VerdictBanner, primary_metric};
use crate::app::App;
use crate::format;

const SPARK_WINDOW: SignedDuration = SignedDuration::from_mins(15);

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let Some(snapshot) = &app.snapshot else {
        return;
    };
    let chain = PathChain {
        snapshot,
        selected: Some(app.selected_hop),
    };
    let banner_widget = VerdictBanner {
        snapshot,
        since: app.open_incident().map(|i| i.started),
        now: app.now,
    };
    let [banner, path, rest] = Layout::vertical([
        Constraint::Length(banner_widget.height(area.width)),
        Constraint::Length(chain.height(area.width)),
        Constraint::Fill(1),
    ])
    .areas(area);
    frame.render_widget(banner_widget, banner);
    frame.render_widget(chain, path);

    let [sparks, speed] = if rest.width >= 100 {
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(40)])
            .spacing(1)
            .areas(rest)
    } else {
        Layout::vertical([Constraint::Fill(1), Constraint::Length(8)]).areas(rest)
    };
    render_sparklines(app, frame, sparks);
    render_speed_card(app, frame, speed);
}

fn card(title: &str) -> Block<'_> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::border())
        .title(Line::from(format!(" {title} ")).style(theme::title()))
        .padding(Padding::horizontal(1))
}

fn render_sparklines(app: &App, frame: &mut Frame, area: Rect) {
    let Some(snapshot) = &app.snapshot else {
        return;
    };
    let block = card("Latency · last 15 min");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let label_width = snapshot
        .hops
        .iter()
        .map(|h| h.label.chars().count())
        .max()
        .unwrap_or(0) as u16;
    // Taller bars when there's room; the newest sample sits at the right edge.
    let row_height = (inner.height / snapshot.hops.len().max(1) as u16).clamp(1, 3);
    let rows =
        Layout::vertical(snapshot.hops.iter().map(|_| Constraint::Length(row_height))).split(inner);
    let cutoff = app.now - SPARK_WINDOW;
    for (i, (hop, row)) in snapshot.hops.iter().zip(rows.iter()).enumerate() {
        if row.height == 0 {
            continue;
        }
        let [label, spark, value] = Layout::horizontal([
            Constraint::Length(label_width + 3),
            Constraint::Fill(1),
            Constraint::Length(20),
        ])
        .spacing(1)
        .areas(*row);
        let selected = i == app.selected_hop;
        let label = Rect {
            y: label.bottom() - 1,
            height: 1,
            ..label
        };
        let label_style = if hop.status == Status::Skip {
            theme::muted()
        } else {
            theme::text()
        };
        frame.render_widget(
            Line::from(vec![
                Span::styled(
                    format::status_icon(hop.status),
                    Style::new().fg(theme::status(hop.status)),
                ),
                Span::raw(" "),
                Span::styled(
                    hop.label.as_str(),
                    if selected {
                        label_style.bold()
                    } else {
                        label_style
                    },
                ),
            ]),
            label,
        );
        let samples: Vec<&Sample> = app
            .samples
            .get(&hop.id)
            .map(|s| s.iter().filter(|s| s.at >= cutoff).collect())
            .unwrap_or_default();
        let samples = &samples[samples.len().saturating_sub(spark.width as usize)..];
        if !samples.iter().any(|s| s.rtt_ms.is_some()) {
            // Nothing to plot (the host hop has no latency): show status over time instead.
            let strip_area = Rect {
                y: spark.bottom() - 1,
                height: 1,
                ..spark
            };
            frame.render_widget(status_strip(samples).right_aligned(), strip_area);
        } else {
            let spark = Rect {
                x: spark.right() - samples.len() as u16,
                width: samples.len() as u16,
                ..spark
            };
            frame.render_widget(sparkline(samples), spark);
        }
        let value = Rect {
            y: value.bottom() - 1,
            height: 1,
            ..value
        };
        frame.render_widget(
            Line::from(Span::styled(
                primary_metric(hop),
                Style::new().fg(theme::status(hop.status)),
            ))
            .right_aligned(),
            value,
        );
    }
}

/// RTT bars, scaled to the window's max. Failed samples are full-height red bars so outages
/// stand out; unmeasured samples are blank.
pub fn sparkline(samples: &[&Sample]) -> Sparkline<'static> {
    let max = samples
        .iter()
        .filter_map(|s| s.rtt_ms)
        .fold(1.0_f64, f64::max);
    // A floor keeps the smallest values visible as a baseline instead of blank cells.
    let height = |rtt: f64| (12.0 + rtt / max * 88.0).round() as u64;
    let bars = samples.iter().map(|s| {
        let (value, color) = match (s.status, s.rtt_ms) {
            (Status::Fail, _) => (Some(100), theme::FAIL),
            (Status::Warn, rtt) => (rtt.map(height), theme::WARN),
            (_, Some(rtt)) if s.loss_pct.is_some_and(|l| l > 0.0) => {
                (Some(height(rtt)), theme::WARN)
            }
            (_, Some(rtt)) => (Some(height(rtt)), theme::ACCENT),
            (_, None) => (None, theme::MUTED),
        };
        SparklineBar::from(value).style(Style::new().fg(color))
    });
    Sparkline::default()
        .data(bars.collect::<Vec<_>>())
        .max(100)
        .absent_value_symbol(" ")
        .absent_value_style(Style::new().fg(theme::FAINT))
}

fn render_speed_card(app: &App, frame: &mut Frame, area: Rect) {
    let running = app.running_speed();
    let block = card("Speed test").title(
        Line::from(vec![
            Span::styled(" r ", theme::key()),
            Span::styled("run now ", theme::muted()),
        ])
        .right_aligned(),
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let latest = app.speed.iter().find(|r| r.state == SpeedState::Complete);
    let shown = running.or(latest);
    let Some(run) = shown else {
        frame.render_widget(
            Paragraph::new(Span::styled("No speed tests yet.", theme::muted())),
            inner,
        );
        return;
    };
    let [numbers, latency, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Fill(1),
    ])
    .areas(inner);
    let big = |label: &str, value: Option<f64>| {
        vec![
            Line::from(Span::styled(label.to_string(), theme::muted())),
            Line::from(vec![
                Span::styled(
                    value.map(|v| format!("{v:.0}")).unwrap_or("—".into()),
                    theme::title(),
                ),
                Span::styled(" Mbps", theme::muted()),
            ]),
        ]
    };
    let [down, up] = Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).areas(numbers);
    frame.render_widget(Paragraph::new(big("↓ Download", run.down_mbps)), down);
    frame.render_widget(Paragraph::new(big("↑ Upload", run.up_mbps)), up);
    frame.render_widget(Paragraph::new(latency_lines(run)), latency);
    if let Some(progress) = running.and_then(|r| r.progress) {
        let gauge = LineGauge::default()
            .ratio(progress.clamp(0.0, 1.0))
            .label(Span::styled(
                format!("{:>3.0}%", progress * 100.0),
                theme::text(),
            ))
            .filled_style(Style::new().fg(theme::ACCENT))
            .unfilled_style(Style::new().fg(theme::BORDER));
        frame.render_widget(gauge, footer);
    } else {
        let mut line = vec![Span::styled(
            format::ago(run.started, app.now),
            theme::muted(),
        )];
        if let Some(server) = &run.server {
            line.extend([
                Span::styled(" · ", theme::muted()),
                Span::styled(server.as_str(), theme::muted()),
            ]);
        }
        let [when, recent] =
            Layout::vertical([Constraint::Length(2), Constraint::Fill(1)]).areas(footer);
        frame.render_widget(Paragraph::new(Line::from(line)), when);
        render_recent_runs(app, frame, recent);
    }
}

/// Download speed of recent runs, oldest to newest, when the card has room.
fn render_recent_runs(app: &App, frame: &mut Frame, area: Rect) {
    if area.height < 3 {
        return;
    }
    let [label, bars] = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(area);
    let mut runs: Vec<u64> = app
        .speed
        .iter()
        .filter(|r| r.state == SpeedState::Complete)
        .filter_map(|r| r.down_mbps)
        .take(bars.width as usize)
        .map(|v| v.round() as u64)
        .collect();
    runs.reverse();
    frame.render_widget(
        Span::styled(
            format!("Download · last {} runs", runs.len()),
            theme::muted(),
        ),
        label,
    );
    frame.render_widget(
        Sparkline::default()
            .data(&runs)
            .style(Style::new().fg(theme::ACCENT)),
        bars,
    );
}

pub fn latency_lines(run: &SpeedRun) -> Vec<Line<'static>> {
    let ms = |v: Option<f64>| v.map(format::ms).unwrap_or("—".into());
    let mut grade = vec![Span::styled("Bufferbloat ", theme::muted())];
    match run.bloat {
        Some(g) => grade.push(Span::styled(
            format::grade(g),
            Style::new().fg(theme::grade(g)).bold(),
        )),
        None => grade.push(Span::styled("—", theme::muted())),
    }
    vec![
        Line::from(vec![
            Span::styled("Latency ", theme::muted()),
            Span::styled(ms(run.idle_latency_ms), theme::text()),
            Span::styled(" idle · ", theme::muted()),
            Span::styled(ms(run.loaded_latency_ms), theme::text()),
            Span::styled(" loaded", theme::muted()),
        ]),
        Line::from(grade),
    ]
}

pub fn render_hop_detail(app: &App, frame: &mut Frame, area: Rect) {
    let Some(hop) = app
        .snapshot
        .as_ref()
        .and_then(|s| s.hops.get(app.selected_hop))
    else {
        return;
    };
    let color = theme::status(hop.status);
    let mut title = vec![
        Span::raw(" "),
        Span::styled(format::status_icon(hop.status), Style::new().fg(color)),
        Span::raw(" "),
        Span::styled(hop.label.as_str(), theme::title()),
    ];
    if let Some(target) = &hop.target {
        title.extend([
            Span::styled(" · ", theme::muted()),
            Span::styled(target.as_str(), theme::muted()),
        ]);
    }
    title.push(Span::raw(" "));
    let inner = super::popup(
        frame,
        area,
        Constraint::Percentage(92),
        Constraint::Percentage(92),
        Line::from(title),
    );

    let table_rows = hop.checks.len().max(hop.evidence.len()) as u16 + 1;
    let [summary, chart_area, loss_area, tables] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(6),
        Constraint::Length(1),
        Constraint::Length(table_rows.min(12) + 1),
    ])
    .areas(inner);

    let metric = |label: &str, value: Option<String>| {
        [
            Span::styled(format!("{label} "), theme::muted()),
            Span::styled(value.unwrap_or("—".into()), theme::text()),
            Span::raw("   "),
        ]
    };
    let mut metrics = vec![];
    metrics.extend(metric("RTT", hop.metrics.rtt_ms.map(format::ms)));
    metrics.extend(metric("Jitter", hop.metrics.jitter_ms.map(format::ms)));
    metrics.extend(metric("Loss", hop.metrics.loss_pct.map(format::pct)));
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    format::status_word(hop.status),
                    Style::new().fg(color).bold(),
                ),
                Span::styled(" · ", theme::muted()),
                Span::styled(hop.summary.as_str(), theme::text()),
            ]),
            Line::from(metrics),
        ]),
        summary,
    );

    // The last hour, as an RTT chart over a loss strip.
    let cutoff = app.now - SignedDuration::from_hours(1);
    let samples: Vec<&Sample> = app
        .samples
        .get(&hop.id)
        .map(|s| s.iter().filter(|s| s.at >= cutoff).collect())
        .unwrap_or_default();
    let points: Vec<(f64, f64)> = samples
        .iter()
        .filter_map(|s| Some((s.at.duration_since(app.now).as_secs_f64() / 60.0, s.rtt_ms?)))
        .collect();
    let max = points.iter().map(|p| p.1).fold(1.0_f64, f64::max) * 1.15;
    let chart = Chart::new(vec![
        Dataset::default()
            .name("RTT")
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::new().fg(theme::ACCENT))
            .data(&points),
    ])
    .block(Block::new().title(Line::from(" RTT · last hour").style(theme::muted())))
    .legend_position(None)
    .x_axis(
        Axis::default()
            .bounds([-60.0, 0.0])
            .labels(["-60 min", "-30 min", "now"])
            .style(Style::new().fg(theme::FAINT))
            .labels_alignment(ratatui::layout::HorizontalAlignment::Right),
    )
    .y_axis(
        Axis::default()
            .bounds([0.0, max])
            .labels([format::ms(0.0), format::ms(max / 2.0), format::ms(max)])
            .style(Style::new().fg(theme::FAINT)),
    );
    if points.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "No latency measurements for this hop.",
                theme::muted(),
            ))
            .centered(),
            chart_area.centered_vertically(Constraint::Length(1)),
        );
    } else {
        frame.render_widget(chart, chart_area);
    }
    let [loss_label, loss_spark] =
        Layout::horizontal([Constraint::Length(10), Constraint::Fill(1)]).areas(loss_area);
    frame.render_widget(Span::styled("status", theme::muted()), loss_label);
    let strip = &samples[samples.len().saturating_sub(loss_spark.width as usize)..];
    frame.render_widget(status_strip(strip), loss_spark);

    let [checks, evidence] = Layout::horizontal([Constraint::Fill(3), Constraint::Fill(2)])
        .spacing(2)
        .areas(tables);
    let check_rows = hop.checks.iter().map(|c| {
        Row::new(vec![
            Line::from(Span::styled(
                format::status_icon(c.status),
                Style::new().fg(theme::status(c.status)),
            )),
            Line::from(Span::styled(c.name.as_str(), theme::text())),
            Line::from(Span::styled(c.detail.as_str(), theme::muted())),
            Line::from(Span::styled(
                c.rtt_ms.map(format::ms).unwrap_or_default(),
                theme::text(),
            ))
            .right_aligned(),
        ])
    });
    frame.render_widget(
        Table::new(
            check_rows,
            [
                Constraint::Length(1),
                Constraint::Fill(2),
                Constraint::Fill(3),
                Constraint::Length(8),
            ],
        )
        .header(Row::new(["", "Check", "Detail", "RTT"]).style(theme::muted().bold())),
        checks,
    );
    let evidence_rows = hop.evidence.iter().map(|e| {
        Row::new(vec![
            Span::styled(e.key.as_str(), theme::muted()),
            Span::styled(e.value.as_str(), theme::text()),
        ])
    });
    frame.render_widget(
        Table::new(evidence_rows, [Constraint::Fill(1), Constraint::Fill(2)])
            .header(Row::new(["Evidence", ""]).style(theme::muted().bold())),
        evidence,
    );
}

/// One cell per sample, colored by status: a compact uptime strip.
fn status_strip(samples: &[&Sample]) -> Line<'static> {
    Line::from(
        samples
            .iter()
            .map(|s| {
                let lossy = s.loss_pct.is_some_and(|l| l > 0.0);
                let color = if lossy && s.status == Status::Pass {
                    theme::WARN
                } else {
                    theme::status(s.status)
                };
                Span::styled("▮", Style::new().fg(color))
            })
            .collect::<Vec<_>>(),
    )
}
