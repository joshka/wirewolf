//! Terminal UI client for wirewolf.
//!
//! The TUI talks to a wirewolf server over its HTTP API (`docs/api.md`): it loads state over
//! REST, then follows the SSE event stream, reconnecting with backoff when it drops.

mod app;
pub mod client;
pub mod format;
pub mod report;
mod ui;

use std::io::Write;
use std::time::Duration;

use jiff::{SignedDuration, Timestamp};
use ratatui::crossterm::event::{self, Event as TermEvent};
use tokio::sync::mpsc::{self, UnboundedSender};

pub use app::{Action, App, Loaded, Msg, Range, View};
pub use client::Client;

/// Renders the app into a frame. Exposed for capture tooling and tests.
pub fn render(app: &mut App, frame: &mut ratatui::Frame) {
    ui::render(app, frame);
}

/// Runs the TUI against the server at `url` until the user quits.
pub async fn run(url: &str) -> anyhow::Result<()> {
    let client = Client::new(url)?;
    let (tx, mut rx) = mpsc::unbounded_channel();

    spawn_input(tx.clone());
    tokio::spawn(follow(client.clone(), tx.clone()));
    let ticker = tx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        while ticker.send(Msg::Tick).is_ok() {
            interval.tick().await;
        }
    });

    let mut terminal = ratatui::init();
    let mut app = App::new(client.base());
    let result = async {
        loop {
            terminal.draw(|frame| ui::render(&mut app, frame))?;
            let Some(msg) = rx.recv().await else {
                return Ok(());
            };
            let mut actions = Vec::new();
            actions.extend(app.update(msg));
            // Apply everything already queued (a cycle sends one sample per hop) before drawing.
            while let Ok(msg) = rx.try_recv() {
                actions.extend(app.update(msg));
            }
            for action in actions {
                match action {
                    Action::Quit => return Ok(()),
                    Action::RunSpeed => spawn_request(&tx, client.clone(), |c| async move {
                        match c.run_speed().await? {
                            client::SpeedRequest::Started => Ok(None),
                            client::SpeedRequest::AlreadyRunning => {
                                Ok(Some("A speed test is already running".into()))
                            }
                        }
                    }),
                    Action::SetScenario(id) => {
                        spawn_request(&tx, client.clone(), move |c| async move {
                            c.set_scenario(&id).await?;
                            Ok(Some(format!("Switched to scenario {id}")))
                        })
                    }
                    Action::Copy(text) => copy_to_clipboard(&text)?,
                }
            }
        }
    }
    .await;
    ratatui::restore();
    result
}

/// Reads terminal input on a dedicated thread; crossterm's `read` blocks.
fn spawn_input(tx: UnboundedSender<Msg>) {
    std::thread::spawn(move || {
        while let Ok(event) = event::read() {
            let msg = match event {
                TermEvent::Key(key) => Msg::Key(key),
                TermEvent::Resize(..) => Msg::Tick,
                _ => continue,
            };
            if tx.send(msg).is_err() {
                break;
            }
        }
    });
}

/// Loads state, then follows the event stream. Reconnects with backoff, forever.
async fn follow(client: Client, tx: UnboundedSender<Msg>) {
    let mut backoff = Duration::from_millis(500);
    loop {
        let result = async {
            let loaded = load(&client).await?;
            tx.send(Msg::Loaded(Box::new(loaded)))?;
            let events = tx.clone();
            client
                .events(move |event| drop(events.send(Msg::Event(Box::new(event)))))
                .await?;
            anyhow::Ok(())
        }
        .await;
        let error = match result {
            Ok(()) => "the server closed the stream".to_string(),
            Err(error) => error.root_cause().to_string(),
        };
        if tx.send(Msg::Disconnected(error)).is_err() {
            return;
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(10));
    }
}

async fn load(client: &Client) -> anyhow::Result<Loaded> {
    let since = Timestamp::now() - SignedDuration::from_hours(24);
    let (meta, now, history, speed, incidents) = tokio::try_join!(
        client.meta(),
        client.now(),
        client.history(since),
        client.speed(),
        client.incidents()
    )?;
    Ok(Loaded {
        meta,
        now,
        history,
        speed,
        incidents,
    })
}

fn spawn_request<F, Fut>(tx: &UnboundedSender<Msg>, client: Client, request: F)
where
    F: FnOnce(Client) -> Fut + Send + 'static,
    Fut: Future<Output = anyhow::Result<Option<String>>> + Send,
{
    let tx = tx.clone();
    tokio::spawn(async move {
        let toast = match request(client).await {
            Ok(toast) => toast,
            Err(error) => Some(format!("Request failed: {error}")),
        };
        if let Some(toast) = toast {
            let _ = tx.send(Msg::Toast(toast));
        }
    });
}

/// Copies via OSC 52, which works over SSH and in most modern terminals.
fn copy_to_clipboard(text: &str) -> std::io::Result<()> {
    let mut stdout = std::io::stdout();
    write!(stdout, "\x1b]52;c;{}\x07", base64(text.as_bytes()))?;
    stdout.flush()
}

fn base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from_be_bytes([0, b[0], b[1], b[2]]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64() {
        assert_eq!(super::base64(b"wirewolf"), "d2lyZXdvbGY=");
        assert_eq!(super::base64(b"ab"), "YWI=");
    }
}
