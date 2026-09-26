use crate::domain::Theme;
use std::path::{Path, PathBuf};

/// Where the window's colours come from: the palette the user picked by hand,
/// or the desktop shell's own scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ThemeSource {
    Manual,
    System,
}

pub struct ThemeStore {
    path: PathBuf,
    source_path: PathBuf,
}

impl ThemeStore {
    pub fn new(config_dir: &Path) -> Self {
        Self {
            path: config_dir.join("theme.json"),
            source_path: config_dir.join("theme-source.json"),
        }
    }

    pub fn load_or_default(&self) -> Theme {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(Theme::default_dark)
    }

    /// The manual theme is kept even while the system one is in use, so
    /// switching back restores the user's own colours.
    pub fn load_source(&self) -> ThemeSource {
        std::fs::read_to_string(&self.source_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(ThemeSource::Manual)
    }

    pub fn save_source(&self, source: ThemeSource) -> anyhow::Result<()> {
        if let Some(parent) = self.source_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.source_path, serde_json::to_string(&source)?)?;
        Ok(())
    }

    pub fn save(&self, theme: &Theme) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(theme)?;
        std::fs::write(&self.path, json)?;
        Ok(())
    }
}
