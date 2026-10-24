//! Rendering. Every function here reads [`App`] and draws; none of them mutate state other
//! than widget scroll state.

mod history;
mod incidents;
mod now;
mod speed;
pub(crate) mod theme;
mod widgets;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, List, ListItem, Paragraph, Tabs, Wrap};

use crate::app::{App, Connection, Popup, View};

pub fn render(app: &mut App, frame: &mut Frame) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    render_header(app, frame, header);
    let body = body.inner(ratatui::layout::Margin::new(1, 0));
    if app.snapshot.is_none() {
        render_waiting(app, frame, body);
    } else {
        match app.view {
            View::Now => now::render(app, frame, body),
            View::History => history::render(app, frame, body),
            View::Incidents => incidents::render(app, frame, body),
            View::Speed => speed::render(app, frame, body),
        }
    }
    render_footer(app, frame, footer);
    match &mut app.popup {
        Some(Popup::Hop) => now::render_hop_detail(app, frame, body),
        Some(Popup::Scenarios(_)) => render_scenarios(app, frame, body),
        Some(Popup::Help) => render_help(frame, body),
        None => {}
    }
}

fn render_header(app: &App, frame: &mut Frame, area: Rect) {
    let live = match &app.connection {
        Connection::Live => Span::styled("● live", Style::new().fg(theme::PASS)),
        Connection::Connecting => Span::styled("◌ connecting", Style::new().fg(theme::WARN)),
        Connection::Lost(_) => Span::styled("◌ reconnecting", Style::new().fg(theme::FAIL)),
    };
    // Drop detail from the right as the terminal narrows; the tabs matter more.
    let mut right = vec![];
    if let Some(meta) = &app.meta {
        if let Some(scenario) = meta
            .scenario
            .as_deref()
            .filter(|_| app.is_demo() && area.width >= 90)
        {
            right.extend([
                Span::styled("demo ", theme::muted()),
                Span::styled(scenario, theme::text()),
                Span::raw("  "),
            ]);
        }
        if area.width >= 110 {
            right.extend([
                Span::styled(meta.host.as_str(), theme::muted()),
                Span::raw("  "),
            ]);
        }
    }
    right.extend([live, Span::raw(" ")]);
    let right = Line::from(right).right_aligned();

    let logo = Line::from(vec![
        Span::raw(" "),
        Span::styled("◆ wirewolf", Style::new().fg(theme::ACCENT).bold()),
    ]);
    let [logo_area, tabs_area, right_area] = Layout::horizontal([
        Constraint::Length(logo.width() as u16 + 3),
        Constraint::Fill(1),
        Constraint::Length(right.width() as u16),
    ])
    .areas(area);
    frame.render_widget(logo, logo_area);
    let titles = View::ALL.iter().enumerate().map(|(i, view)| {
        let mut title = Line::from(vec![
            Span::styled(format!("{}", i + 1), theme::muted()),
            Span::raw(" "),
            Span::raw(view.title()),
        ]);
        if *view == View::Incidents && app.open_incident().is_some() {
            title.push_span(Span::styled(" ●", Style::new().fg(theme::FAIL)));
        }
        title
    });
    let selected = View::ALL.iter().position(|v| *v == app.view);
    let tabs = Tabs::new(titles)
        .select(selected)
        .style(theme::muted())
        .highlight_style(
            Style::new()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )
        .divider(" ")
        .padding(" ", " ");
    frame.render_widget(tabs, tabs_area);
    frame.render_widget(right, right_area);
}

fn render_footer(app: &App, frame: &mut Frame, area: Rect) {
    let mut keys: Vec<(&str, &str)> = match (app.view, app.popup.as_ref()) {
        (_, Some(Popup::Hop)) => vec![("←→", "hop"), ("esc", "close")],
        (_, Some(Popup::Scenarios(_))) => {
            vec![("↑↓", "choose"), ("enter", "switch"), ("esc", "close")]
        }
        (_, Some(Popup::Help)) => vec![("any key", "close")],
        (View::Now, None) => vec![("←→", "hop"), ("enter", "details"), ("r", "run speed test")],
        (View::History, None) => vec![("t", "range"), ("←→", "hop"), ("space", "show/hide")],
        (View::Incidents, None) if app.incident_detail.is_some() => {
            vec![("c", "copy report"), ("esc", "back")]
        }
        (View::Incidents, None) => {
            vec![("↑↓", "select"), ("enter", "details"), ("c", "copy report")]
        }
        (View::Speed, None) => vec![("r", "run now")],
    };
    if app.popup.is_none() {
        if app.is_demo() {
            keys.push(("s", "scenario"));
        }
        keys.extend([("?", "help"), ("q", "quit")]);
    }
    let mut spans = vec![Span::raw(" ")];
    for (key, label) in keys {
        spans.extend([
            Span::styled(key, theme::key()),
            Span::styled(format!(" {label}   "), theme::muted()),
        ]);
    }
    frame.render_widget(Line::from(spans), area);

    let note = match (&app.toast, &app.connection) {
        (Some((toast, _)), _) => Some(Line::from(Span::styled(format!("{toast} "), theme::text()))),
        (None, Connection::Lost(error)) => Some(Line::from(Span::styled(
            format!("connection lost: {error} "),
            Style::new().fg(theme::FAIL),
        ))),
        _ => None,
    };
    if let Some(note) = note {
        let width = (note.width() as u16).min(area.width);
        let note_area = Rect {
            x: area.right() - width,
            width,
            ..area
        };
        frame.render_widget(Clear, note_area);
        frame.render_widget(note.right_aligned(), note_area);
    }
}

fn render_waiting(app: &App, frame: &mut Frame, area: Rect) {
    let message = match &app.connection {
        Connection::Lost(error) => vec![
            Line::from(Span::styled(
                format!("Can't reach wirewolf at {}", app.url),
                Style::new().fg(theme::FAIL).bold(),
            )),
            Line::from(Span::styled(error.as_str(), theme::muted())),
            Line::from(""),
            Line::from(Span::styled(
                "Retrying… start a server with `wirewolf serve`.",
                theme::muted(),
            )),
        ],
        _ if app.meta.is_some() => vec![Line::from(Span::styled(
            "Waiting for the first measurement…",
            theme::muted(),
        ))],
        _ => vec![Line::from(Span::styled(
            format!("Connecting to {}…", app.url),
            theme::muted(),
        ))],
    };
    let height = message.len() as u16;
    let area = area.centered_vertically(Constraint::Length(height));
    frame.render_widget(
        Paragraph::new(message).centered().wrap(Wrap { trim: true }),
        area,
    );
}

/// A centered popup frame. Returns the inner area.
fn popup(
    frame: &mut Frame,
    area: Rect,
    width: Constraint,
    height: Constraint,
    title: Line,
) -> Rect {
    let area = area.centered(width, height);
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme::ACCENT))
        .title(title)
        .padding(ratatui::widgets::Padding::horizontal(1));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

fn render_scenarios(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(meta) = &app.meta else { return };
    let Some(Popup::Scenarios(state)) = &mut app.popup else {
        return;
    };
    let items: Vec<ListItem> = meta
        .scenarios
        .iter()
        .map(|s| {
            let active = meta.scenario.as_deref() == Some(s.id.as_str());
            let marker = if active {
                Span::styled("● ", Style::new().fg(theme::ACCENT))
            } else {
                Span::raw("  ")
            };
            ListItem::new(vec![
                Line::from(vec![marker, Span::styled(s.name.as_str(), theme::title())]),
                Line::from(Span::styled(format!("  {}", s.description), theme::muted())),
            ])
        })
        .collect();
    let height = (items.len() as u16 * 2 + 2).min(area.height);
    let title = Line::from(" Demo scenario ").style(theme::title());
    let inner = popup(
        frame,
        area,
        Constraint::Max(76),
        Constraint::Length(height),
        title,
    );
    let list = List::new(items)
        .highlight_style(Style::new().bg(theme::SURFACE))
        .highlight_symbol("▌");
    frame.render_stateful_widget(list, inner, state);
}

fn render_help(frame: &mut Frame, area: Rect) {
    let rows = [
        ("1-4, tab", "switch view"),
        ("r", "run a speed test"),
        ("s", "switch demo scenario (demo mode)"),
        ("←→ / h l", "select a hop (Now, History)"),
        ("enter", "open hop or incident details"),
        ("i", "jump to the open incident (Now)"),
        ("t", "change the time range (History)"),
        ("space", "show or hide a hop (History)"),
        ("c", "copy the incident report (Incidents)"),
        ("esc", "close or go back"),
        ("q, ctrl-c", "quit"),
    ];
    let lines: Vec<Line> = rows
        .iter()
        .map(|(key, what)| {
            Line::from(vec![
                Span::styled(format!("{key:>10}  "), theme::key()),
                Span::styled(*what, theme::text()),
            ])
        })
        .collect();
    let title = Line::from(" Keys ").style(theme::title());
    let inner = popup(
        frame,
        area,
        Constraint::Length(52),
        Constraint::Length(lines.len() as u16 + 2),
        title,
    );
    frame.render_widget(Paragraph::new(lines), inner);
}
