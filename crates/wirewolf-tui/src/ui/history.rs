//! The History view: RTT and loss per hop over 1h/6h/24h, with incident bands and speed runs.

use jiff::Timestamp;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::symbols;
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Block, BorderType, Chart, Dataset, GraphType, Padding};
use wirewolf_core::model::Severity;

use super::theme;
use crate::app::{App, Range};
use crate::format;

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let [controls, rtt_area, loss_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(3),
        Constraint::Fill(2),
    ])
    .areas(area);
    render_controls(app, frame, controls);

    let span = app.range.duration();
    let start = app.now - span;
    // x is minutes relative to now, so the axis reads naturally in every range.
    let x = |at: Timestamp| at.duration_since(app.now).as_secs_f64() / 60.0;
    let x_min = -span.as_secs_f64() / 60.0;

    let hops: Vec<(usize, &str, &str)> = app
        .snapshot
        .iter()
        .flat_map(|s| s.hops.iter().enumerate())
        .filter(|(_, h)| !app.hidden_hops.contains(&h.id))
        .map(|(i, h)| (i, h.id.as_str(), h.label.as_str()))
        .collect();
    let series =
        |metric: fn(&wirewolf_core::model::Sample) -> Option<f64>| -> Vec<Vec<(f64, f64)>> {
            hops.iter()
                .map(|(_, id, _)| {
                    app.samples
                        .get(*id)
                        .map(|s| {
                            s.iter()
                                .filter(|s| s.at >= start)
                                .filter_map(|s| Some((x(s.at), metric(s)?)))
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .collect()
        };
    let rtt = series(|s| s.rtt_ms);
    let loss = series(|s| s.loss_pct);
    let rtt_max = nice_max(
        rtt.iter()
            .flatten()
            .map(|p| p.1)
            .fold(0.0, f64::max)
            .max(10.0),
    );
    let loss_max = nice_max(
        loss.iter()
            .flatten()
            .map(|p| p.1)
            .fold(0.0, f64::max)
            .max(5.0),
    );

    // Incident periods, clipped to the range, as filled columns behind the lines.
    let bands: Vec<(Severity, f64, f64)> = app
        .incidents
        .iter()
        .filter(|i| i.ended.is_none_or(|e| e >= start))
        .map(|i| {
            (
                i.severity,
                x(i.started.max(start)),
                x(i.ended.unwrap_or(app.now)),
            )
        })
        .collect();
    let speed_marks: Vec<f64> = app
        .speed
        .iter()
        .filter(|r| r.started >= start)
        .map(|r| x(r.started))
        .collect();

    let x_labels = match app.range {
        Range::Hour => ["-60m", "-30m", "now"],
        Range::SixHours => ["-6h", "-3h", "now"],
        Range::Day => ["-24h", "-12h", "now"],
    }
    .map(String::from);
    let axis = Axes {
        x_min,
        x_labels,
        hops: &hops,
    };
    frame.render_widget(
        chart(&axis, "Round-trip time", &rtt, rtt_max, format::ms),
        rtt_area,
    );
    overlay(frame.buffer_mut(), rtt_area, x_min, &bands, &speed_marks);
    frame.render_widget(
        chart(&axis, "Packet loss", &loss, loss_max, format::pct),
        loss_area,
    );
    overlay(frame.buffer_mut(), loss_area, x_min, &bands, &[]);
}

/// What both charts share: the x range and the visible hops.
struct Axes<'a> {
    x_min: f64,
    x_labels: [String; 3],
    hops: &'a [(usize, &'a str, &'a str)],
}

fn chart<'a>(
    axes: &Axes<'a>,
    title: &'static str,
    data: &'a [Vec<(f64, f64)>],
    y_max: f64,
    unit: fn(f64) -> String,
) -> Chart<'a> {
    let datasets: Vec<Dataset<'a>> = axes
        .hops
        .iter()
        .zip(data)
        .map(|((i, _, label), points)| {
            Dataset::default()
                .name(*label)
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::new().fg(theme::SERIES[i % theme::SERIES.len()]))
                .data(points)
        })
        .collect();
    Chart::new(datasets)
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(theme::border())
                .title(Line::from(format!(" {title} ")).style(theme::title()))
                .padding(Padding::horizontal(1)),
        )
        .legend_position(None)
        .x_axis(
            Axis::default()
                .bounds([axes.x_min, 0.0])
                .labels(axes.x_labels.clone())
                .style(Style::new().fg(theme::FAINT)),
        )
        .y_axis(
            Axis::default()
                .bounds([0.0, y_max])
                .labels([unit(0.0), unit(y_max / 2.0), unit(y_max)])
                .style(Style::new().fg(theme::FAINT)),
        )
}

/// Shades incident periods behind the plotted lines and marks speed runs along the top.
///
/// Chart has no notion of background bands, so this finds the plot area from the axis corner
/// the chart drew and tints those cells' background.
fn overlay(
    buf: &mut Buffer,
    area: Rect,
    x_min: f64,
    bands: &[(Severity, f64, f64)],
    marks: &[f64],
) {
    let Some((corner_x, corner_y)) = area
        .positions()
        .find(|p| buf[*p].symbol() == symbols::line::BOTTOM_LEFT)
        .map(|p| (p.x, p.y))
    else {
        return;
    };
    let (left, right, top) = (corner_x + 1, area.right().saturating_sub(3), area.y + 1);
    if right <= left {
        return;
    }
    let width = f64::from(right - left);
    let column = |x: f64| left + (((x - x_min) / -x_min) * width).round().clamp(0.0, width) as u16;
    for (severity, from, to) in bands {
        for col in column(*from)..=column(*to) {
            for row in top..corner_y {
                if let Some(cell) = buf.cell_mut((col, row)) {
                    cell.set_bg(theme::severity_band(*severity));
                }
            }
        }
    }
    for x in marks {
        if let Some(cell) = buf.cell_mut((column(*x), top)) {
            cell.set_symbol("▾").set_fg(theme::ACCENT);
        }
    }
}

fn render_controls(app: &App, frame: &mut Frame, area: Rect) {
    let mut spans = vec![Span::styled(" Range ", theme::muted())];
    for range in Range::ALL {
        let style = if range == app.range {
            Style::new().fg(theme::SURFACE).bg(theme::ACCENT).bold()
        } else {
            theme::muted()
        };
        spans.extend([
            Span::styled(format!(" {} ", range.label()), style),
            Span::raw(" "),
        ]);
    }
    spans.extend([Span::styled("  Hops ", theme::muted())]);
    let legend = [
        Span::styled("  █", Style::new().fg(theme::severity_band(Severity::Down))),
        Span::styled(" incident  ", theme::muted()),
        Span::styled("▾", Style::new().fg(theme::ACCENT)),
        Span::styled(" speed test", theme::muted()),
    ];
    for (i, hop) in app.snapshot.iter().flat_map(|s| s.hops.iter()).enumerate() {
        let shown = !app.hidden_hops.contains(&hop.id);
        let color = theme::SERIES[i % theme::SERIES.len()];
        let (mark, label_style) = if shown {
            ("■ ", Style::new().fg(theme::TEXT))
        } else {
            ("□ ", Style::new().fg(theme::FAINT))
        };
        let mut label_style = label_style;
        if i == app.history_cursor {
            label_style = label_style.add_modifier(Modifier::UNDERLINED | Modifier::BOLD);
        }
        spans.extend([
            Span::styled(
                mark,
                Style::new().fg(if shown { color } else { theme::FAINT }),
            ),
            Span::styled(hop.label.as_str(), label_style),
            Span::raw("  "),
        ]);
    }
    spans.extend(legend);
    frame.render_widget(Line::from(spans), area);
}

/// Rounds an axis maximum up to 1, 2, or 5 times a power of ten.
fn nice_max(value: f64) -> f64 {
    let magnitude = 10f64.powf(value.log10().floor());
    [1.0, 2.0, 5.0, 10.0]
        .iter()
        .map(|m| m * magnitude)
        .find(|v| *v >= value)
        .unwrap_or(value)
}
