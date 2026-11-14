//! A standalone Iced frontend for Muzon.
//!
//! Two modes, chosen on the command line (see [`cli`]):
//!
//! - **demo** ([`demo`]) — 5000 synthetic tracks, a simulated playhead, no
//!   audio and no backend. Useful without GStreamer or a library.
//! - **real** ([`real`]) — the actual player: `muzon-core`'s audio engine,
//!   SQLite library, queue controller and session, confined to an explicit
//!   profile directory.
//!
//! Neither mode talks to Tauri, and the real mode owns its queue through the
//! single `muzon-core` control worker: the UI sends commands and renders
//! snapshots, it never decides what plays next.

pub mod cli;
pub mod demo;
pub mod real;
pub mod ui;
