use crate::domain::Appearance;
use std::path::{Path, PathBuf};

pub struct AppearanceStore {
    path: PathBuf,
}

impl AppearanceStore {
    pub fn new(config_dir: &Path) -> Self {
        Self {
            path: config_dir.join("appearance.json"),
        }
    }

    pub fn load_or_default(&self) -> Appearance {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|s| Appearance::from_json(&s))
            .unwrap_or_default()
    }

    pub fn save(&self, appearance: &Appearance) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(appearance)?;
        std::fs::write(&self.path, json)?;
        Ok(())
    }
}
