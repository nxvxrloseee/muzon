use crate::data::audio::pipeline::{AudioPlayer, PlaybackTick};
use crate::data::db::Db;
use crate::data::hotkeys_store::HotkeysStore;
use crate::data::playback_settings_store::PlaybackSettingsStore;
use crate::data::theme_store::ThemeStore;
use crate::domain::{Hotkeys, Theme};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::ipc::Channel;

pub struct AppState {
    pub db: Db,
    pub player: AudioPlayer,
    pub tick_channel: Mutex<Option<Channel<PlaybackTick>>>,
    pub theme_store: ThemeStore,
    pub theme: Mutex<Theme>,
    pub cover_cache: Mutex<HashMap<String, Option<String>>>,
    pub hotkeys_store: HotkeysStore,
    pub hotkeys: Mutex<Hotkeys>,
    pub playback_settings_store: PlaybackSettingsStore,
}
