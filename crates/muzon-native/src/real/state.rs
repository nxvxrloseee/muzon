//! The native UI keeps presentation state; the core is the sole playback owner.
mod editor;
mod input;
mod library;
mod lifecycle;
mod queue;
mod services;
mod settings;

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use super::{
    audio_settings::{AudioUi, PRESETS},
    desktop::{Desktop, WindowRequest},
    message::Message,
};
use iced::widget::{image, scrollable};
use iced::{Subscription, Task};
use muzon_core::controller::{Controller, CoreConfig, Media, PlaylistEdit, Snapshot};
use muzon_core::data::{
    db::Db, playback_settings_store::PlaybackSettingsStore, session_store::SessionStore,
};

pub const LIBRARY_LIST: &str = "muzon-native/library";
pub const QUEUE_LIST: &str = "muzon-native/queue";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Library,
    Albums,
    Artists,
    Genres,
    Favorites,
    Smart,
    Playlists,
    Audio,
    Sleep,
    Shortcuts,
    Appearance,
    Cloud,
    Scrobble,
    NowPlaying,
    Queue,
}

#[derive(Debug, Clone, Copy)]
pub enum CollectionOverlay {
    AddTrack(i32),
    DeletePlaylist(i32),
}

pub struct QueueDrag {
    pub from: usize,
    pub to: usize,
    pub baseline: Arc<Vec<usize>>,
}
pub struct RealApp {
    controller: Option<Controller>,
    desktop: Option<Desktop>,
    tray: Option<super::tray::Tray>,
    pub(super) hidden: bool,
    // An explicit profile has one writer, even if two native windows are launched.
    _profile_lock: File,
    pub(super) window: crate::real::window_state::WindowMemory,
    pub snapshot: Arc<Snapshot>,
    pub media: Arc<Media>,
    pub cover: Option<image::Handle>,
    pub screen: Screen,
    pub audio: AudioUi,
    pub hotkeys: super::hotkeys::Hotkeys,
    pub appearance: crate::real::appearance::Appearance,
    pub services: crate::real::services::Services,
    services_hub: Option<Arc<muzon_services::hub::Hub>>,
    pub spectrum: Arc<Vec<f32>>,
    spectrum_enabled: bool,
    pub editor: Option<super::editor::Editor>,
    pub palette: Option<super::palette::Palette>,
    pub queue_drag: Option<QueueDrag>,
    editor_generation: u64,
    held_keys: HashSet<iced::keyboard::Key>,
    pub query: String,
    pub smart: super::smart::Smart,
    smart_refresh: std::time::Instant,
    pub album_grid: crate::real::album_grid::Grid,
    pub groups: Vec<super::groups::Group>,
    pub group_rows: Vec<usize>,
    pub selected_group: Option<super::groups::GroupKey>,
    pub group_scroll: f32,
    pub filtered: Vec<usize>,
    search_index: Vec<String>,
    library_by_id: HashMap<i32, usize>,
    pub selected_playlist: Option<i32>,
    pub new_playlist_name: String,
    pub rename_playlist_name: String,
    pub collection_overlay: Option<CollectionOverlay>,
    pub playlist_busy: bool,
    pub pending_favorites: HashSet<i32>,
    pub playlist_scroll: f32,
    pub picker_scroll: f32,
    pub folder_input: String,
    pub folder_picker_open: bool,
    pub track_covers: crate::real::track_covers::Covers,
    pub library_scroll: f32,
    pub queue_scroll: f32,
    pub seek_preview: Option<f64>,
    pub closing: bool,
    pub audio_close_failed: bool,
    pub ui_error: Option<String>,
}

pub use library::{collection_indices, filtered_track_id};
pub use queue::displayed_queue_index;

impl RealApp {
    /// Route presentation messages; playback mutations remain controller commands.
    fn dispatch(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ScreenSelected(Screen::Audio) => {
                let navigate = self.update_library(Message::ScreenSelected(Screen::Audio));
                let refresh = self.update_settings(Message::RefreshAudioDevices);
                Task::batch([navigate, refresh])
            }
            Message::Poll => {
                let poll = self.update_lifecycle(Message::Poll);
                if self.screen == Screen::Audio
                    && !self.audio.devices_refreshing
                    && self
                        .audio
                        .devices_refreshed
                        .is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(5))
                {
                    Task::batch([poll, self.update_settings(Message::RefreshAudioDevices)])
                } else {
                    poll
                }
            }
            Message::WindowFocusLost
            | Message::WindowOpened(..)
            | Message::WindowResized(..)
            | Message::WindowObserved(..)
            | Message::WindowSaved(..)
            | Message::Quit
            | Message::DiscardAudioAndQuit => self.update_lifecycle(message),
            Message::AddFolder
            | Message::Rescan
            | Message::SmartSelected(..)
            | Message::AlbumCovers(..)
            | Message::AlbumsBack
            | Message::GroupsScrolled(..)
            | Message::AlbumSelected(..)
            | Message::PlayAlbum(..)
            | Message::GroupSelected(..)
            | Message::SearchChanged(..)
            | Message::ScreenSelected(..)
            | Message::ToggleFavorite(..)
            | Message::FavoriteFinished { .. }
            | Message::NewPlaylistName(..)
            | Message::RenamePlaylistName(..)
            | Message::PlaylistSelected(..)
            | Message::OpenPlaylistPicker(..)
            | Message::AskDeletePlaylist(..)
            | Message::CloseCollectionOverlay
            | Message::CreatePlaylist
            | Message::RenamePlaylist
            | Message::EditPlaylist(..)
            | Message::PlaylistFinished(..)
            | Message::PlaylistListScrolled(..)
            | Message::PlaylistPickerScrolled(..)
            | Message::PlayCollection
            | Message::EnqueueCollection
            | Message::LibraryScrolled(..)
            | Message::TrackCovers(..)
            | Message::PickFolder
            | Message::FolderPicked(..)
            | Message::FolderInputChanged(..) => self.update_library(message),
            Message::AppearanceChanged(..)
            | Message::AppearanceAccentInput(..)
            | Message::AppearanceAccentApply
            | Message::AppearanceFileInput(..)
            | Message::AppearanceRyoku
            | Message::AppearanceImport
            | Message::AppearanceSave
            | Message::AppearanceSaved(..)
            | Message::AppearanceRefresh
            | Message::AppearanceThemeLoaded(..)
            | Message::AppearanceSystem(..)
            | Message::SetSleep(..)
            | Message::AudioDeviceInput(..)
            | Message::AudioDeviceApply
            | Message::RefreshAudioDevices
            | Message::AudioDevicesLoaded(..)
            | Message::AudioChanged(..)
            | Message::AudioPreset(..)
            | Message::ResetAudioEffects
            | Message::TrackTempoChanged { .. }
            | Message::ResetTrackTempo(..)
            | Message::AudioSave
            | Message::AudioSaved(..) => self.update_settings(message),
            Message::OpenEditor(..)
            | Message::EditorLoaded(..)
            | Message::EditorField(..)
            | Message::EditorText(..)
            | Message::EditorTab(..)
            | Message::EditorStorage(..)
            | Message::EditorClose
            | Message::EditorKeep
            | Message::EditorDiscard
            | Message::EditorFetch
            | Message::EditorFetched(..)
            | Message::EditorFetchCancel
            | Message::EditorUseSuggestion
            | Message::EditorDismissSuggestion
            | Message::EditorTapStart
            | Message::EditorTapStamp
            | Message::EditorTapUndo
            | Message::EditorTapFinish
            | Message::EditorTapCancel
            | Message::EditorTimestamp
            | Message::EditorSave
            | Message::EditorSaved(..) => self.update_editor(message),
            Message::ServiceAction(..)
            | Message::ServiceFinished(..)
            | Message::ServiceField(..)
            | Message::ServiceSecret(..)
            | Message::ServicePathStyle(..)
            | Message::ServiceCancel
            | Message::ServiceCopyAuth
            | Message::ServiceOpenAuth
            | Message::ServiceBrowser(..) => self.update_services(message),
            Message::PaletteQuery(..)
            | Message::PaletteClose
            | Message::PaletteExecute
            | Message::PaletteRun(..)
            | Message::HotkeyCapture(..)
            | Message::HotkeysReset
            | Message::HotkeysSave
            | Message::HotkeysSaved(..)
            | Message::KeyReleased(..)
            | Message::KeyPressed(..)
            | Message::Shortcut(..) => self.update_input(message),
            _ => self.update_queue(message),
        }
    }
}

#[cfg(test)]
mod tests;
