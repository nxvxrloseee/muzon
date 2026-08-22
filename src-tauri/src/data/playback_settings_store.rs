use crate::domain::playback_settings::PlaybackSettings;
use std::path::{Path, PathBuf};

pub struct PlaybackSettingsStore {
    path: PathBuf,
}

impl PlaybackSettingsStore {
    pub fn new(config_dir: &Path) -> Self {
        Self {
            path: config_dir.join("playback_settings.json"),
        }
    }

    pub fn load_or_default(&self) -> PlaybackSettings {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(PlaybackSettings::default_settings)
    }

    pub fn save(&self, settings: &PlaybackSettings) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(settings)?;
        std::fs::write(&self.path, json)?;
        Ok(())
    }
}
