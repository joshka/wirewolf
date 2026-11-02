//! The wirewolf server inside the app: the same hub, probes, store, and web app as `wirewolf
//! serve`, on a random loopback port, with the config and the history in the app's data
//! directory.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::{Arc, OnceLock};

use tokio::runtime::Runtime;
use url::Url;
use wirewolf_core::config::{self, Override, Source};
use wirewolf_core::editor::Editor;
use wirewolf_core::logs::{self, Logs};
use wirewolf_core::{Hub, demo};

use crate::Options;

/// A running server.
pub struct Started {
    pub hub: Hub,
    /// Where the dashboard is: `http://127.0.0.1:<port>/`.
    pub url: Url,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// The tokio runtime the server and probes run on. Tauri owns the main thread, so the runtime
/// lives beside it for the life of the process.
pub fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| Runtime::new().expect("tokio runtime"))
}

/// Binds a loopback port, starts the probes (or the demo), and serves the dashboard on it.
pub fn start(data: &Path, options: &Options) -> anyhow::Result<Started> {
    let (logs, rust_log) = init_logging();
    let rt = runtime();
    let listener = rt.block_on(tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)))?;
    let addr = listener.local_addr()?;
    let url = Url::parse(&format!("http://{addr}/"))?;
    let hub = if options.demo {
        let scenario = options.scenario.clone().unwrap_or_else(|| "healthy".into());
        let hub = Hub::new(demo::meta(&scenario));
        hub.set_logs(logs);
        rt.spawn(demo::run(hub.clone(), scenario));
        hub
    } else {
        live(data, addr, logs, rust_log)?
    };
    tracing::info!("listening on {url}");
    // No sign-in: only this app can reach the port. The peer address is what the auth layer
    // uses to tell local requests apart, so pass it along as `wirewolf serve` does.
    let app = wirewolf_web::router(hub.clone()).into_make_service_with_connect_info::<SocketAddr>();
    rt.spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!("the server stopped: {e}");
        }
    });
    Ok(Started { hub, url })
}

/// Live probing, as `wirewolf serve` sets it up, with the file paths fixed to the data directory.
fn live(data: &Path, addr: SocketAddr, logs: Arc<Logs>, rust_log: bool) -> anyhow::Result<Hub> {
    let path = data.join("wirewolf.toml");
    // Shown in Settings as set by the app, so they can't be changed there.
    let fixed = |key: &str, value: String| Override { key: key.into(), value, source: Source::Flag, name: "the app".into() };
    let overrides = vec![fixed("server.listen", addr.to_string()), fixed("storage.db", data.join("wirewolf.db").to_string_lossy().into_owned())];
    let loaded = config::load(Some(&path), false, &overrides).map_err(anyhow::Error::msg)?;
    wirewolf_probe::validate(&loaded.config)?;
    match &loaded.path {
        Some(p) => tracing::info!("config from {} ({} settings)", p.display(), loaded.sources.len()),
        None => tracing::info!("no config file yet; using defaults (Settings writes {})", path.display()),
    }
    if !rust_log && let Err(e) = logs.set_filter(&loaded.config.log.filter) {
        tracing::warn!("log.filter {:?}: {e}", loaded.config.log.filter);
    }
    let auth = &loaded.config.auth;
    logs.add_secret(&auth.client_secret);
    logs.add_secret(&loaded.config.notify.ntfy_token);
    logs.add_secret(&loaded.config.notify.webhook);
    if !auth.client_secret_file.is_empty()
        && let Ok(secret) = std::fs::read_to_string(&auth.client_secret_file)
    {
        logs.add_secret(secret.trim());
    }
    let hub = Hub::new(wirewolf_probe::meta());
    hub.set_logs(logs);
    let editor = Arc::new(Editor::new(path, overrides, wirewolf_probe::field_errors, false));
    hub.set_config(editor.view(&loaded));
    hub.set_editor(editor);
    let config = Arc::new(loaded.config);
    wirewolf_probe::attach_store(&hub, &config)?;
    runtime().spawn(wirewolf_probe::run(hub.clone(), config));
    Ok(hub)
}

/// Logs to stderr and to the buffer the Logs page reads, through a filter that can change while
/// running. Starts from `RUST_LOG` if set (the bool says so), else `info` until the config loads.
fn init_logging() -> (Arc<Logs>, bool) {
    use tracing_subscriber::{EnvFilter, prelude::*, reload};
    let logs = Logs::new();
    let rust_log = std::env::var("RUST_LOG").ok().filter(|f| logs::check_filter(f).is_ok());
    let start = rust_log.clone().unwrap_or_else(|| logs::DEFAULT_FILTER.into());
    let (filter, handle) = reload::Layer::new(EnvFilter::new(&start));
    tracing_subscriber::registry().with(filter).with(tracing_subscriber::fmt::layer()).with(logs.layer()).init();
    logs.set_reloader(&start, move |f| handle.reload(EnvFilter::try_new(f).map_err(|e| e.to_string())?).map_err(|e| e.to_string()));
    (logs, rust_log.is_some())
}
