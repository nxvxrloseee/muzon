pub mod appearance;
pub mod hotkeys;
pub mod palette;
pub mod playlist;
pub mod s3;
pub mod scrobble;
pub mod theme;
pub mod window_chrome;

// Moved to `muzon-core`, which the native Iced player shares. The module names
// are re-exported so every `crate::domain::…` path in this app keeps working
// and there is exactly one definition of each type.
pub use muzon_core::domain::{library, lyrics, playback_settings, session, track};

pub use appearance::Appearance;
pub use hotkeys::Hotkeys;
pub use muzon_core::domain::Track;
pub use palette::TrackPalette;
pub use s3::S3Config;
pub use theme::Theme;
