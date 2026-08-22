use crate::domain::Theme;
use std::path::{Path, PathBuf};

pub struct ThemeStore {
    path: PathBuf,
}

impl ThemeStore {
    pub fn new(config_dir: &Path) -> Self {
        Self {
            path: config_dir.join("theme.json"),
        }
    }

    pub fn load_or_default(&self) -> Theme {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(Theme::default_dark)
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
