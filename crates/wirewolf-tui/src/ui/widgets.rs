//! Widgets shared between the Now view and the incident detail.

use jiff::Timestamp;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Padding, Paragraph, Widget, Wrap};
use wirewolf_core::model::{Hop, Severity, Snapshot, Status};

use super::theme;
use crate::format;

/// The verdict: severity, headline, detail, fix hint, confidence, and since when.
pub struct VerdictBanner<'a> {
    pub snapshot: &'a Snapshot,
    /// The start of the incident this verdict belongs to.
    pub since: Option<Timestamp>,
    pub now: Timestamp,
}

impl VerdictBanner<'_> {
    /// The height needed to show everything at this width.
    pub fn height(&self, width: u16) -> u16 {
        // Borders plus one column of padding on each side.
        let (lines, _) = self.content();
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .line_count(width.saturating_sub(4)) as u16
            + 2
    }

    fn paragraph(&self) -> Paragraph<'_> {
        let (lines, block) = self.content();
        Paragraph::new(lines).wrap(Wrap { trim: true }).block(block)
    }

    fn content(&self) -> (Vec<Line<'_>>, Block<'_>) {
        let verdict = &self.snapshot.verdict;
        let color = theme::severity(verdict.severity);
        let healthy = verdict.severity == Severity::Healthy;
        let badge = format!(
            " {} {} ",
            format::severity_icon(verdict.severity),
            format::severity_word(verdict.severity).to_uppercase()
        );
        let badge = if healthy {
            Span::styled(badge, Style::new().fg(color).bold())
        } else {
            Span::styled(badge, Style::new().fg(theme::SURFACE).bg(color).bold())
        };
        let mut block = Block::bordered()
            .border_type(if healthy {
                BorderType::Rounded
            } else {
                BorderType::Thick
            })
            .border_style(if healthy {
                theme::border()
            } else {
                Style::new().fg(color)
            })
            .title(Line::from(vec![Span::raw(" "), badge, Span::raw(" ")]))
            .padding(Padding::horizontal(1));
        if let Some(since) = self.since {
            block = block.title(
                Line::from(vec![
                    Span::styled(" since ", theme::muted()),
                    Span::styled(format::clock(since, self.now), theme::text()),
                    Span::styled(
                        format!(" · {} ", format::duration(self.now.duration_since(since))),
                        theme::muted(),
                    ),
                ])
                .right_aligned(),
            );
        }
        let mut lines = vec![
            Line::from(Span::styled(
                verdict.headline.as_str(),
                Style::new()
                    .fg(if healthy { theme::TEXT } else { color })
                    .bold(),
            )),
            Line::from(Span::styled(verdict.detail.as_str(), theme::text())),
        ];
        if let Some(hint) = &verdict.fix_hint {
            lines.push(Line::from(vec![
                Span::styled("→ ", Style::new().fg(theme::ACCENT)),
                Span::styled(hint.as_str(), theme::text()),
            ]));
        }
        lines.push(Line::from(vec![
            Span::styled(
                format!("{:?} confidence", verdict.confidence).to_lowercase(),
                theme::muted(),
            ),
            Span::styled(" · ", Style::new().fg(theme::FAINT)),
            Span::styled(verdict.method.as_str(), theme::muted()),
            Span::styled(" · ", Style::new().fg(theme::FAINT)),
            Span::styled(
                format!("measured {}", format::ago(self.snapshot.at, self.now)),
                theme::muted(),
            ),
        ]));
        (lines, block)
    }
}

impl Widget for VerdictBanner<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.paragraph().render(area, buf);
    }
}

/// The hops as a chain: horizontal cards when there's room, a vertical list otherwise.
pub struct PathChain<'a> {
    pub snapshot: &'a Snapshot,
    pub selected: Option<usize>,
}

impl PathChain<'_> {
    const CARD_MIN: u16 = 15;
    const ARROW: u16 = 3;

    fn horizontal(&self, width: u16) -> bool {
        let n = self.snapshot.hops.len() as u16;
        n * Self::CARD_MIN + n.saturating_sub(1) * Self::ARROW <= width
    }

    pub fn height(&self, width: u16) -> u16 {
        if self.horizontal(width) {
            5
        } else {
            self.snapshot.hops.len() as u16 + 2
        }
    }

    fn is_blamed(&self, hop: &Hop) -> bool {
        self.snapshot.verdict.blame.as_deref() == Some(hop.id.as_str())
    }
}

impl Widget for PathChain<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.horizontal(area.width) {
            render_cards(&self, area, buf);
        } else {
            render_list(&self, area, buf);
        }
    }
}

fn render_cards(chain: &PathChain, area: Rect, buf: &mut Buffer) {
    let hops = &chain.snapshot.hops;
    let mut constraints = Vec::with_capacity(hops.len() * 2);
    for i in 0..hops.len() {
        if i > 0 {
            constraints.push(Constraint::Length(PathChain::ARROW));
        }
        constraints.push(Constraint::Fill(1));
    }
    let areas = Layout::horizontal(constraints).split(area);
    for (i, hop) in hops.iter().enumerate() {
        if i > 0 {
            let arrow_area = areas[i * 2 - 1];
            let broken =
                matches!(hops[i - 1].status, Status::Fail) || matches!(hop.status, Status::Fail);
            let style = if broken {
                Style::new().fg(theme::FAIL)
            } else {
                Style::new().fg(theme::FAINT)
            };
            let arrow = if broken { "─╳─" } else { "──▶" };
            if arrow_area.height > 2 {
                buf.set_string(arrow_area.x, arrow_area.y + 2, arrow, style);
            }
        }
        render_card(chain, hop, chain.selected == Some(i), areas[i * 2], buf);
    }
}

fn render_card(chain: &PathChain, hop: &Hop, selected: bool, area: Rect, buf: &mut Buffer) {
    let color = theme::status(hop.status);
    let blamed = chain.is_blamed(hop);
    let collateral = hop.status == Status::Skip;
    let (border_type, border_style) = match (selected, blamed) {
        (true, _) => (
            if blamed {
                BorderType::Thick
            } else {
                BorderType::Rounded
            },
            Style::new().fg(theme::ACCENT),
        ),
        (false, true) => (BorderType::Thick, Style::new().fg(color)),
        (false, false) => (BorderType::Rounded, theme::border()),
    };
    let label_style = if collateral {
        theme::muted()
    } else {
        theme::title()
    };
    let mut block = Block::bordered()
        .border_type(border_type)
        .border_style(border_style)
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(format::status_icon(hop.status), Style::new().fg(color)),
            Span::raw(" "),
            Span::styled(hop.label.as_str(), label_style),
            Span::raw(" "),
        ]))
        .padding(Padding::horizontal(1));
    if blamed {
        block = block.title_bottom(
            Line::from(Span::styled(
                " problem here ",
                Style::new().fg(color).bold(),
            ))
            .centered(),
        );
    } else if selected {
        block = block.title_bottom(Line::from(Span::styled(" enter ", theme::key())).centered());
    }
    let summary_style = match hop.status {
        Status::Fail | Status::Warn => Style::new().fg(color),
        Status::Skip | Status::NotApplicable | Status::Pending => Style::new().fg(theme::FAINT),
        Status::Pass => theme::text(),
    };
    let lines = vec![
        Line::from(Span::styled(
            hop.target.as_deref().unwrap_or("—"),
            if collateral {
                Style::new().fg(theme::FAINT)
            } else {
                theme::muted()
            },
        )),
        Line::from(Span::styled(hop.summary.as_str(), summary_style)),
    ];
    Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .block(block)
        .render(area, buf);
}

/// The one number that matters most for a hop: latency, or its status when it has none.
pub fn primary_metric(hop: &Hop) -> String {
    match (hop.metrics.rtt_ms, hop.metrics.loss_pct) {
        (Some(rtt), Some(loss)) if loss > 0.0 => {
            format!("{} · {}", format::ms(rtt), format::loss(loss))
        }
        (Some(rtt), _) => format::ms(rtt),
        (None, Some(loss)) if loss > 0.0 => format::loss(loss),
        _ => format::status_word(hop.status).to_string(),
    }
}

fn render_list(chain: &PathChain, area: Rect, buf: &mut Buffer) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::border())
        .title(" Path ");
    let inner = block.inner(area);
    block.render(area, buf);
    let label_width = chain
        .snapshot
        .hops
        .iter()
        .map(|h| h.label.chars().count())
        .max()
        .unwrap_or(0);
    let lines: Vec<Line> = chain
        .snapshot
        .hops
        .iter()
        .enumerate()
        .map(|(i, hop)| {
            let color = theme::status(hop.status);
            let selected = chain.selected == Some(i);
            let dim = hop.status == Status::Skip;
            let cursor = if selected {
                Span::styled("▌", Style::new().fg(theme::ACCENT))
            } else {
                Span::raw(" ")
            };
            let mut line = Line::from(vec![
                cursor,
                Span::styled(format::status_icon(hop.status), Style::new().fg(color)),
                Span::raw(" "),
                Span::styled(
                    format!("{:<label_width$}  ", hop.label),
                    if dim { theme::muted() } else { theme::title() },
                ),
                Span::styled(
                    hop.summary.as_str(),
                    if dim {
                        Style::new().fg(theme::FAINT)
                    } else {
                        theme::text()
                    },
                ),
            ]);
            if chain.is_blamed(hop) {
                line.push_span(Span::styled(
                    "  ◀ problem here",
                    Style::new().fg(color).bold(),
                ));
            }
            if selected {
                line = line.patch_style(Style::new().bg(theme::SURFACE));
            }
            line
        })
        .collect();
    Paragraph::new(lines).render(inner, buf);
}

#[cfg(test)]
mod tests {
    use ratatui::text::Line;
    use ratatui::widgets::{Paragraph, Wrap};

    #[test]
    fn wrapped_count() {
        let lines = vec![
            Line::from("a"),
            Line::from(
                "Hosts answer by IP address, but names don't resolve. Websites will look down.",
            ),
        ];
        assert_eq!(
            Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .line_count(74),
            3
        );
    }
}
