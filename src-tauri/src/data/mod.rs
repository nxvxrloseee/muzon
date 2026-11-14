pub mod appearance_store;
pub mod control;
pub mod cover_protocol;
pub mod hotkeys_store;
pub mod keyring;
pub mod library_watch;
pub mod lrclib;
pub mod mpris;
pub mod queue_adapter;
pub mod s3;
pub mod s3_config_store;
pub mod scrobble;
pub mod system_theme;
pub mod theme_store;
pub mod tray;

// The shared half — the audio engine, the library database, tags, covers and
// local lyrics, the session and playback-settings stores — lives in
// `muzon-core` and is re-exported here. Same paths, one implementation.
pub use muzon_core::data::{
    covers, db, lrc_source, playback_settings_store, scanner, session_store, tag_writer,
};

pub mod audio {
    pub use muzon_core::data::audio::*;
}
