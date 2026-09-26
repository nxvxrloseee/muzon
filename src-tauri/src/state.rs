use crate::data::audio::pipeline::{AudioPlayer, PlaybackTick};
use crate::data::control::ControlHub;
use crate::data::db::Db;
use crate::data::hotkeys_store::HotkeysStore;
use crate::data::mpris::MprisBridge;
use crate::data::playback_settings_store::PlaybackSettingsStore;
use crate::data::s3_config_store::S3ConfigStore;
use crate::data::session_store::SessionState;
use crate::data::theme_store::{ThemeSource, ThemeStore};
use crate::domain::{Hotkeys, S3Config, Theme};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::ipc::Channel;

pub struct AppState {
    pub db: Db,
    pub player: AudioPlayer,
    pub tick_channel: Mutex<Option<Channel<PlaybackTick>>>,
    pub theme_store: ThemeStore,
    pub theme: Mutex<Theme>,
    pub theme_source: Mutex<ThemeSource>,
    pub cover_cache: Mutex<HashMap<String, Option<String>>>,
    pub hotkeys_store: HotkeysStore,
    pub hotkeys: Mutex<Hotkeys>,
    pub playback_settings_store: PlaybackSettingsStore,
    pub s3_config_store: S3ConfigStore,
    pub s3_config: Mutex<S3Config>,
    pub session: SessionState,
    pub mpris: MprisBridge,
    pub control: ControlHub,
}
