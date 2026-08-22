use crate::domain::Hotkeys;
use std::path::{Path, PathBuf};

pub struct HotkeysStore {
    path: PathBuf,
}

impl HotkeysStore {
    pub fn new(config_dir: &Path) -> Self {
        Self {
            path: config_dir.join("hotkeys.json"),
        }
    }

    pub fn load_or_default(&self) -> Hotkeys {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(Hotkeys::default_bindings)
    }

    pub fn save(&self, hotkeys: &Hotkeys) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(hotkeys)?;
        std::fs::write(&self.path, json)?;
        Ok(())
    }
}
