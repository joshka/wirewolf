//! Renders every TUI view against a running demo server into text and HTML captures.
//!
//! ```sh
//! cargo run -- serve --demo --listen 127.0.0.1:7475 &
//! cargo run -p wirewolf-tui --example capture -- http://127.0.0.1:7475 docs/compare/tui
//! ```

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use jiff::{SignedDuration, Timestamp};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::style::{Color, Modifier};
use wirewolf_tui::{App, Client, Loaded, Msg};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let url = args.next().unwrap_or("http://127.0.0.1:7475".into());
    let out = PathBuf::from(args.next().unwrap_or("docs/compare/tui".into()));
    std::fs::create_dir_all(&out)?;
    let client = Client::new(&url)?;
    let scenarios: Vec<String> = client
        .meta()
        .await?
        .scenarios
        .into_iter()
        .map(|s| s.id)
        .collect();

    for scenario in &scenarios {
        client.set_scenario(scenario).await?;
        tokio::time::sleep(Duration::from_millis(2500)).await;
        let app = load(&client).await?;
        shot(&out, &format!("now-{scenario}"), (140, 42), app, &[])?;
        if scenario == "healthy" || scenario == "dns_down" || scenario == "wifi_loss" {
            shot(
                &out,
                &format!("now-{scenario}-narrow"),
                (80, 44),
                load(&client).await?,
                &[],
            )?;
        }
    }

    client.set_scenario("dns_down").await?;
    tokio::time::sleep(Duration::from_millis(2500)).await;
    let keys = |codes: &[KeyCode]| codes.to_vec();
    use KeyCode::*;
    shot(
        &out,
        "hop-detail-dns",
        (140, 42),
        load(&client).await?,
        &keys(&[Right, Right, Right, Right, Enter]),
    )?;
    shot(
        &out,
        "history-1h",
        (140, 42),
        load(&client).await?,
        &keys(&[Char('2')]),
    )?;
    shot(
        &out,
        "history-24h",
        (140, 42),
        load(&client).await?,
        &keys(&[Char('2'), Char('t'), Char('t')]),
    )?;
    shot(
        &out,
        "incidents",
        (140, 42),
        load(&client).await?,
        &keys(&[Char('3')]),
    )?;
    shot(
        &out,
        "incident-detail",
        (140, 42),
        load(&client).await?,
        &keys(&[Char('3'), Down, Enter]),
    )?;
    shot(
        &out,
        "scenarios",
        (140, 42),
        load(&client).await?,
        &keys(&[Char('s')]),
    )?;
    shot(
        &out,
        "help",
        (140, 42),
        load(&client).await?,
        &keys(&[Char('?')]),
    )?;
    client.set_scenario("healthy").await?;
    shot(
        &out,
        "speed-idle",
        (140, 42),
        load(&client).await?,
        &keys(&[Char('4')]),
    )?;
    client.run_speed().await?;
    tokio::time::sleep(Duration::from_millis(2500)).await;
    shot(
        &out,
        "speed-running",
        (140, 42),
        load(&client).await?,
        &keys(&[Char('4')]),
    )?;
    shot(
        &out,
        "now-speed-running",
        (140, 42),
        load(&client).await?,
        &[],
    )?;
    // Every view and popup at many sizes, including tiny ones, must render without panicking.
    let views: [&[KeyCode]; 7] = [
        &[],
        &[Enter],
        &[Char('2')],
        &[Char('3')],
        &[Char('3'), Enter],
        &[Char('4')],
        &[Char('s')],
    ];
    for keys in views {
        let mut app = load(&client).await?;
        for key in keys {
            app.update(Msg::Key(KeyEvent::from(*key)));
        }
        for width in (10..=160).step_by(9) {
            for height in (3..=50).step_by(4) {
                let mut terminal = Terminal::new(TestBackend::new(width, height))?;
                terminal.draw(|frame| wirewolf_tui::render(&mut app, frame))?;
            }
        }
    }
    println!("wrote captures to {}", out.display());
    Ok(())
}

async fn load(client: &Client) -> anyhow::Result<App> {
    let since = Timestamp::now() - SignedDuration::from_hours(24);
    let loaded = Loaded {
        meta: client.meta().await?,
        now: client.now().await?,
        history: client.history(since).await?,
        speed: client.speed().await?,
        incidents: client.incidents().await?,
    };
    let mut app = App::new(client.base());
    app.update(Msg::Loaded(Box::new(loaded)));
    Ok(app)
}

fn shot(
    out: &Path,
    name: &str,
    (width, height): (u16, u16),
    mut app: App,
    keys: &[KeyCode],
) -> anyhow::Result<()> {
    for key in keys {
        app.update(Msg::Key(KeyEvent::from(*key)));
    }
    let mut terminal = Terminal::new(TestBackend::new(width, height))?;
    terminal.draw(|frame| wirewolf_tui::render(&mut app, frame))?;
    let buffer = terminal.backend().buffer();
    std::fs::write(out.join(format!("{name}.txt")), text(buffer))?;
    std::fs::write(out.join(format!("{name}.html")), html(name, buffer))?;
    Ok(())
}

fn text(buffer: &Buffer) -> String {
    let mut out = String::new();
    for y in 0..buffer.area.height {
        let mut line = String::new();
        for x in 0..buffer.area.width {
            line.push_str(buffer[(x, y)].symbol());
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

fn css(color: Color) -> Option<String> {
    let named = |i: u8| {
        const ANSI: [&str; 16] = [
            "#09090b", "#f87171", "#34d399", "#fbbf24", "#60a5fa", "#c084fc", "#22d3ee", "#d4d4d8",
            "#52525b", "#fca5a5", "#6ee7b7", "#fde68a", "#93c5fd", "#d8b4fe", "#67e8f9", "#fafafa",
        ];
        ANSI[i as usize].to_string()
    };
    Some(match color {
        Color::Reset => return None,
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Black => named(0),
        Color::Red => named(1),
        Color::Green => named(2),
        Color::Yellow => named(3),
        Color::Blue => named(4),
        Color::Magenta => named(5),
        Color::Cyan => named(6),
        Color::Gray => named(7),
        Color::DarkGray => named(8),
        Color::LightRed => named(9),
        Color::LightGreen => named(10),
        Color::LightYellow => named(11),
        Color::LightBlue => named(12),
        Color::LightMagenta => named(13),
        Color::LightCyan => named(14),
        Color::White => named(15),
        Color::Indexed(i) if i < 16 => named(i),
        Color::Indexed(_) => return None,
    })
}

/// A self-contained HTML page that looks like the terminal did.
fn html(name: &str, buffer: &Buffer) -> String {
    let mut body = String::new();
    for y in 0..buffer.area.height {
        let mut x = 0;
        while x < buffer.area.width {
            let cell = &buffer[(x, y)];
            let mut style = String::new();
            let (mut fg, mut bg) = (cell.fg, cell.bg);
            if cell.modifier.contains(Modifier::REVERSED) {
                std::mem::swap(&mut fg, &mut bg);
            }
            if let Some(c) = css(fg) {
                let _ = write!(style, "color:{c};");
            }
            if let Some(c) = css(bg) {
                let _ = write!(style, "background:{c};");
            }
            if cell.modifier.contains(Modifier::BOLD) {
                style.push_str("font-weight:700;");
            }
            if cell.modifier.contains(Modifier::DIM) {
                style.push_str("opacity:.6;");
            }
            if cell.modifier.contains(Modifier::UNDERLINED) {
                style.push_str("text-decoration:underline;");
            }
            let mut run = String::new();
            while x < buffer.area.width {
                let next = &buffer[(x, y)];
                if next.fg != cell.fg || next.bg != cell.bg || next.modifier != cell.modifier {
                    break;
                }
                for ch in next.symbol().chars() {
                    match ch {
                        '<' => run.push_str("&lt;"),
                        '>' => run.push_str("&gt;"),
                        '&' => run.push_str("&amp;"),
                        c => run.push(c),
                    }
                }
                x += 1;
            }
            if style.is_empty() {
                body.push_str(&run);
            } else {
                let _ = write!(body, "<span style=\"{style}\">{run}</span>");
            }
        }
        body.push('\n');
    }
    format!(
        "<!doctype html><meta charset=utf-8><title>wirewolf tui · {name}</title>\
         <style>body{{margin:0;background:#09090b}}pre{{margin:0;padding:16px;color:#e4e4e7;\
         font:13px/1.25 'JetBrains Mono','SF Mono',Menlo,monospace;white-space:pre}}</style><pre>{body}</pre>\n"
    )
}
