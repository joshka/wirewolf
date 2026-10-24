//! Core types, diagnosis engine, and state hub for wirewolf.
//!
//! - [`model`]: the API types (`Snapshot`, `Hop`, `Verdict`, `Incident`, `SpeedRun`, …). They
//!   are the source of truth for the JSON API in `docs/api.md`.
//! - [`diagnose`]: turns a path of hops into one [`model::Verdict`], blaming the first break and
//!   treating later hops as collateral.
//! - [`hub`]: the [`Hub`], which holds the latest state, incidents, and history, and broadcasts
//!   events to the web server and other clients. Probe drivers publish into it.
//! - [`config`]: `wirewolf.toml` loading, defaults, and overrides from env vars and flags.
//! - [`editor`]: saves config changes back to the file, keeping its comments.
//! - [`logs`]: recent server log lines for the dashboard, and the live log filter.
//! - [`brief`]: the compact summary of the network for an AI assistant.
//! - `demo` (feature `demo`): a scripted driver for `wirewolf serve --demo`.
//!
//! This crate is part of [wirewolf](https://github.com/joshka/wirewolf), a self-hosted network
//! diagnostics service. Most people want the `wirewolf` crate, which installs the binary
//! (`cargo install wirewolf`). This crate's API is internal and may change in any release.

#[cfg(feature = "demo")]
pub mod demo;
pub mod brief;
pub mod config;
pub mod diagnose;
pub mod editor;
pub mod hub;
pub mod logs;
pub mod model;

pub use hub::{Command, Hub};
