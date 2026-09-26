use crate::domain::S3Config;
use std::path::{Path, PathBuf};

/// The bucket settings sit next to the theme; the keys never do - they are in
/// the system keyring (data/s3/creds.rs).
pub struct S3ConfigStore {
    path: PathBuf,
}

impl S3ConfigStore {
    pub fn new(config_dir: &Path) -> Self {
        Self {
            path: config_dir.join("s3.json"),
        }
    }

    pub fn load_or_default(&self) -> S3Config {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, cfg: &S3Config) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, serde_json::to_string_pretty(cfg)?)?;
        Ok(())
    }
}
