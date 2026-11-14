use crate::domain::S3Config;
use std::path::{Path, PathBuf};

/// The bucket settings sit next to the theme; the keys never do - they are in
/// the system keyring (data/s3/creds.rs).
pub struct S3ConfigStore {
    path: PathBuf,
    serial: std::sync::Mutex<()>,
}

impl S3ConfigStore {
    pub fn new(config_dir: &Path) -> Self {
        Self {
            path: config_dir.join("s3.json"),
            serial: std::sync::Mutex::new(()),
        }
    }

    pub fn load_or_default(&self) -> S3Config {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, cfg: &S3Config) -> anyhow::Result<()> {
        let _guard = self.serial.lock().unwrap();
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        use std::io::Write;
        let temporary = self.path.with_extension("json.tmp");
        let result = (|| -> anyhow::Result<()> {
            let mut file = std::fs::File::create(&temporary)?;
            file.write_all(&serde_json::to_vec_pretty(cfg)?)?;
            file.sync_all()?;
            std::fs::rename(&temporary, &self.path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    }
}
