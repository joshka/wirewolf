//! The Incidents view: a list, and a detail page that renders the evidence like the Now view.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Padding, Paragraph, Row, Table};

use super::theme;
use super::widgets::{PathChain, VerdictBanner};
use crate::app::App;
use crate::{format, report};

pub fn render(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.detail_incident().is_some() {
        render_detail(app, frame, area);
    } else {
        render_list(app, frame, area);
    }
}

fn render_list(app: &mut App, frame: &mut Frame, area: Rect) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::border())
        .title(Line::from(format!(" Incidents · {} ", app.incidents.len())).style(theme::title()))
        .padding(Padding::horizontal(1));
    if app.incidents.is_empty() {
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new(Span::styled(
                "No incidents. Everything has been healthy.",
                theme::muted(),
            ))
            .centered(),
            inner.centered_vertically(Constraint::Length(1)),
        );
        return;
    }
    let now = app.now;
    let hop_label = |id: &str| {
        app.snapshot
            .as_ref()
            .and_then(|s| s.hops.iter().find(|h| h.id == id))
            .map(|h| h.label.clone())
            .unwrap_or_else(|| id.to_string())
    };
    let rows: Vec<Row> = app
        .incidents
        .iter()
        .map(|i| {
            let color = theme::severity(i.severity);
            let duration = match i.ended {
                Some(ended) => Span::styled(
                    format::duration(ended.duration_since(i.started)),
                    theme::text(),
                ),
                None => Span::styled(
                    format!(
                        "ongoing · {}",
                        format::duration(now.duration_since(i.started))
                    ),
                    Style::new().fg(color).bold(),
                ),
            };
            Row::new(vec![
                Line::from(vec![
                    Span::styled(format::severity_icon(i.severity), Style::new().fg(color)),
                    Span::raw(" "),
                    Span::styled(format::severity_word(i.severity), Style::new().fg(color)),
                ]),
                Line::from(Span::styled(i.headline.as_str(), theme::text())),
                Line::from(Span::styled(
                    i.blame.as_deref().map(hop_label).unwrap_or("—".into()),
                    theme::text(),
                )),
                Line::from(vec![
                    Span::styled(format::clock(i.started, now), theme::text()),
                    Span::styled(format!("  {}", format::ago(i.started, now)), theme::muted()),
                ]),
                Line::from(duration).right_aligned(),
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Fill(1),
            Constraint::Length(9),
            Constraint::Length(26),
            Constraint::Length(18),
        ],
    )
    .header(
        Row::new(["Severity", "What happened", "Blamed", "Started", "Duration"])
            .style(theme::muted().bold())
            .bottom_margin(1),
    )
    .row_highlight_style(Style::new().bg(theme::SURFACE))
    .highlight_symbol(Span::styled("▌", Style::new().fg(theme::ACCENT)))
    .column_spacing(2)
    .block(block);
    frame.render_stateful_widget(table, area, &mut app.incident_table);
}

fn render_detail(app: &App, frame: &mut Frame, area: Rect) {
    let Some(incident) = app.detail_incident() else {
        return;
    };
    let chain = PathChain {
        snapshot: &incident.evidence,
        selected: None,
    };
    let banner_widget = VerdictBanner {
        snapshot: &incident.evidence,
        since: Some(incident.started),
        now: app.now,
    };
    let [title, banner, path, report_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(banner_widget.height(area.width)),
        Constraint::Length(chain.height(area.width)),
        Constraint::Fill(1),
    ])
    .areas(area);
    let mut heading = vec![
        Span::styled("Incidents › ", theme::muted()),
        Span::styled(incident.headline.as_str(), theme::title()),
        Span::styled(
            format!("  {}", format::clock(incident.started, app.now)),
            theme::muted(),
        ),
    ];
    heading.push(match incident.ended {
        Some(ended) => Span::styled(
            format!(
                " – {} ({})",
                format::clock(ended, app.now),
                format::duration(ended.duration_since(incident.started))
            ),
            theme::muted(),
        ),
        None => Span::styled(
            "  ongoing",
            Style::new().fg(theme::severity(incident.severity)).bold(),
        ),
    });
    frame.render_widget(Line::from(heading), title);
    frame.render_widget(banner_widget, banner);
    frame.render_widget(chain, path);

    let text = report::incident(incident, app.meta.as_ref().map(|m| m.host.as_str()));
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::border())
        .title(Line::from(" Report ").style(theme::title()))
        .title(
            Line::from(vec![
                Span::styled(" c ", theme::key()),
                Span::styled("copy to clipboard ", theme::muted()),
            ])
            .right_aligned(),
        )
        .padding(Padding::horizontal(1));
    frame.render_widget(
        Paragraph::new(text).style(theme::muted()).block(block),
        report_area,
    );
}
