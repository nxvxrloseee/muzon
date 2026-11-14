use crate::domain::playback_settings::PlaybackSettings;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone)]
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
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let json = serde_json::to_vec_pretty(&settings.normalized())?;
        let temp = self.path.with_extension(format!(
            "{}.{}.tmp",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let mut created = false;
        let result = (|| -> anyhow::Result<()> {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)?;
            created = true;
            file.write_all(&json)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&temp, &self.path)?;
            Ok(())
        })();
        if created && result.is_err() {
            let _ = std::fs::remove_file(temp);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atomic_replacement_does_not_leave_temporary_files_after_failure() {
        let dir = crate::testing::scratch_dir("audio-settings-atomic-replace");
        let store = PlaybackSettingsStore::new(&dir);
        let mut settings = PlaybackSettings::default_settings();
        settings.crossfade_secs = 2.0;
        store.save(&settings).unwrap();
        assert_eq!(store.load_or_default(), settings);
        std::fs::remove_file(&store.path).unwrap();
        std::fs::create_dir(&store.path).unwrap();
        std::fs::write(store.path.join("keep"), b"old data").unwrap();
        assert!(store.save(&settings).is_err());
        assert_eq!(std::fs::read(store.path.join("keep")).unwrap(), b"old data");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    }
}
