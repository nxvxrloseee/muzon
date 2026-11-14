//! Muzon's platform-free core.
//!
//! Everything here is shared between the Tauri desktop app and the native Iced
//! player: the audio engine, the SQLite library, tag reading, cover decoding,
//! local lyrics, the session and playback-settings stores, and the playback
//! controller the native UI drives. Deliberately absent: Tauri, Iced, WebKit,
//! the tray, MPRIS, scrobbling and S3 — those stay in the desktop app.
//!
//! The module tree mirrors the one `src-tauri` grew, so moved code kept its
//! `crate::domain::…` / `crate::data::…` paths and needed no rewriting:
//!
//! - [`domain`] — plain data types (tracks, sessions, lyrics, settings);
//! - [`data`] — storage and the audio engine ([`data::audio`]);
//! - [`controller`] — the one worker that owns the queue and the playback policy.
//!
//! There is exactly one implementation of the audio pipeline: `src-tauri`
//! re-exports [`data::audio`] rather than keeping a copy.
//!
//! ```
//! use muzon_core::domain::Track;
//! let track = Track {
//!     id: 1,
//!     path: "/music/a.flac".into(),
//!     title: "A".into(),
//!     artist: None,
//!     album: None,
//!     duration_secs: Some(180.0),
//!     track_no: None,
//!     genre: None,
//!     year: None,
//!     is_favorite: false,
//!     tempo: 1.0,
//!     play_count: 0,
//!     last_played_at: None,
//!     added_at: 0.0,
//! };
//! assert_eq!(track.id, 1);
//! ```

#![forbid(unsafe_code)]

pub mod controller;
pub mod data;
pub mod domain;
pub mod editing;
pub mod listens;
pub mod sleep_timer;
// Test fixtures (tone files, scratch directories, a fake HTTP server) are
// compiled into the library so the desktop app's tests can keep using the one
// copy; nothing in the app calls them.
#[cfg(any(test, feature = "test-support"))]
pub mod testing;
